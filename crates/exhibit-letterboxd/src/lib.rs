//! The Letterboxd adapter: a member's account export and their public RSS feed, as the neutral
//! facts the core defines. Only this crate names the site.

mod export;
mod rss;

use exhibit_core::error::{Detail, Error, Result};
use exhibit_core::service::{
    Account, ActivityBatch, Dated, ExportHelp, Fact, FilmRef, Need, Provides, Service, ServiceInfo,
    SyncInput, film_key, normalize_username,
};
use exhibit_core::tmdb::Http;

pub struct Letterboxd;

static INFO: ServiceInfo = ServiceInfo {
    id: "letterboxd",
    name: "Letterboxd",
    mark_svg: include_str!("mark.svg"),
    provides: &[Provides::Activity],
    needs: &[Need::Username, Need::ExportFile],
    export: Some(ExportHelp {
        // Behind a login, so this address is not confirmed live.
        page: "https://letterboxd.com/settings/data/",
        file_prefix: "letterboxd-",
        file_ext: "zip",
    }),
};

impl Service for Letterboxd {
    fn info(&self) -> &ServiceInfo {
        &INFO
    }

    fn activity(
        &self,
        account: &Account,
        input: SyncInput,
        http: &dyn Http,
    ) -> Result<ActivityBatch> {
        match input {
            SyncInput::Snapshot(path) => export::read(path),
            SyncInput::Since(since) => {
                let mut facts = fetch_feed(account, http)?.facts;
                if let Some(t) = since {
                    facts.retain(|d| d.on.is_some_and(|on| on >= t));
                }
                Ok(ActivityBatch { facts })
            }
        }
    }

    fn check(&self, account: &Account, http: &dyn Http) -> Result<u32> {
        Ok(fetch_feed(account, http)?.films)
    }
}

fn fetch_feed(account: &Account, http: &dyn Http) -> Result<rss::Feed> {
    let name = account.username.map(normalize_username).unwrap_or_default();
    if name.is_empty() {
        return Err(Error::Invalid(Detail::coded(
            "service.no_username",
            "a username is needed to read the feed",
        )));
    }
    let url = format!("https://letterboxd.com/{name}/rss/");
    let unreachable = |why: String| Error::Provider(Detail::coded("service.unreachable", why));
    let (status, body) = http
        .get(&url, None)
        .map_err(|e| unreachable(e.to_string()))?;
    match status {
        200 => {}
        404 => {
            return Err(Error::NotFound(
                Detail::coded("service.user_not_found", format!("no member named {name}"))
                    .with("username", &name),
            ));
        }
        s => return Err(unreachable(format!("the feed answered {s}"))),
    }
    let feed = rss::parse(&String::from_utf8_lossy(&body)).map_err(unreachable)?;
    if feed.films == 0 {
        return Err(Error::Provider(
            Detail::coded(
                "service.private",
                format!("the feed of {name} shows no films"),
            )
            .with("username", &name),
        ));
    }
    Ok(feed)
}

/// Days since 1970-01-01 for a civil date (proleptic Gregorian).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// `YYYY-MM-DD` to unix seconds at 00:00 UTC.
fn parse_date(s: &str) -> Option<i64> {
    let mut it = s.trim().split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    if it.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86400)
}

fn film_ref(title: &str, year: Option<i32>, tmdb_id: Option<i64>) -> FilmRef {
    FilmRef {
        key: film_key(title, year),
        tmdb_id,
        title: title.to_string(),
        year,
    }
}

fn dated(film: &FilmRef, fact: Fact, on: Option<i64>) -> Dated {
    Dated {
        film: film.clone(),
        fact,
        on,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    struct Fake(u16, Vec<u8>);
    impl Http for Fake {
        fn get(&self, _url: &str, _bearer: Option<&str>) -> Result<(u16, Vec<u8>)> {
            Ok((self.0, self.1.clone()))
        }
    }

    struct Down;
    impl Http for Down {
        fn get(&self, _url: &str, _bearer: Option<&str>) -> Result<(u16, Vec<u8>)> {
            Err(Error::Provider(Detail::default()))
        }
    }

    const FEED: &str = include_str!("../tests/fixtures/feed.xml");

    fn account() -> Account<'static> {
        Account {
            username: Some("Sample"),
            key: None,
        }
    }

    fn feed_facts() -> Vec<Dated> {
        let http = Fake(200, FEED.as_bytes().to_vec());
        Letterboxd
            .activity(&account(), SyncInput::Since(None), &http)
            .unwrap()
            .facts
    }

    fn code(e: Error) -> Option<&'static str> {
        match e {
            Error::Invalid(d) | Error::NotFound(d) | Error::Provider(d) => d.code,
            _ => None,
        }
    }

    fn day(s: &str) -> Option<i64> {
        parse_date(s)
    }

    fn rating(v: f32) -> Fact {
        Fact::Rating {
            value: v,
            scale: 5.0,
        }
    }

    #[test]
    fn dates_are_unix_midnight() {
        assert_eq!(parse_date("1970-01-01"), Some(0));
        assert_eq!(parse_date("2000-01-01"), Some(946_684_800));
        assert_eq!(parse_date("2024-02-29"), Some(1_709_164_800));
        assert_eq!(parse_date("nope"), None);
    }

    #[test]
    fn feed_entries_become_facts() {
        let facts = feed_facts();
        let orchard = film_ref("The Glass Orchard", Some(2024), Some(9001));
        let ferry = film_ref("Night Ferry", Some(1991), Some(9002));
        let moons = film_ref("Paper Moons", Some(2010), Some(9003));
        let salt = film_ref("Salt & Smoke", Some(2003), Some(9005));
        let want = vec![
            dated(&orchard, Fact::Watch { rewatch: false }, day("2026-11-19")),
            dated(&orchard, rating(4.0), day("2026-11-19")),
            dated(
                &orchard,
                Fact::Review {
                    text: "A quiet film about a family's last harvest.\n\nThe ending is \"earned\", and the light is lovely.".into(),
                    spoiler: true,
                },
                day("2026-11-19"),
            ),
            dated(&ferry, Fact::Watch { rewatch: true }, day("2026-11-18")),
            dated(&ferry, Fact::Like, day("2026-11-18")),
            dated(&moons, Fact::Watch { rewatch: false }, day("2026-11-17")),
            dated(&salt, Fact::Watch { rewatch: false }, day("2026-11-15")),
            dated(&salt, rating(3.5), day("2026-11-15")),
        ];
        assert_eq!(facts, want);
        assert_eq!(facts[0].film.key, "the glass orchard|2024");
    }

    #[test]
    fn since_keeps_only_later_entries_and_check_counts_films() {
        let http = Fake(200, FEED.as_bytes().to_vec());
        let t = day("2026-11-18").unwrap();
        let facts = Letterboxd
            .activity(&account(), SyncInput::Since(Some(t)), &http)
            .unwrap()
            .facts;
        assert!(facts.iter().all(|d| d.on.unwrap() >= t));
        assert_eq!(facts.len(), 5);
        assert_eq!(Letterboxd.check(&account(), &http).unwrap(), 4);
    }

    #[test]
    fn review_text_drops_image_and_spoiler_line_and_decodes_entities() {
        assert_eq!(
            rss::review_text(
                "<p><img src=\"x\"/></p> <p><em>This review may contain spoilers.</em></p> <p>Fish &amp; chips&nbsp;&#8212; &#x41;&lt;3 &#39;ok&#39;</p><p>Second<br/>line</p>"
            ),
            Some((
                "Fish & chips\u{a0}\u{2014} A<3 'ok'\n\nSecond
line"
                    .to_string(),
                true
            ))
        );
        assert_eq!(
            rss::review_text("<p><img src=\"x\"/></p> <p>Watched on Friday.</p>"),
            None
        );
    }

    #[test]
    fn missing_user_is_user_not_found() {
        let e = Letterboxd
            .check(&account(), &Fake(404, vec![]))
            .unwrap_err();
        assert_eq!(code(e), Some("service.user_not_found"));
    }

    #[test]
    fn feed_without_films_is_private_and_network_failure_unreachable() {
        let empty = "<rss xmlns:letterboxd=\"https://letterboxd.com\"><channel><title>x</title></channel></rss>";
        let e = Letterboxd
            .check(&account(), &Fake(200, empty.into()))
            .unwrap_err();
        assert_eq!(code(e), Some("service.private"));
        let e = Letterboxd.check(&account(), &Down).unwrap_err();
        assert_eq!(code(e), Some("service.unreachable"));
    }

    fn zip_of(files: &[(&str, &str)]) -> tempfile_path::Zip {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        let bytes = w.finish().unwrap().into_inner();
        tempfile_path::Zip::new(bytes)
    }

    /// A ZIP written under the target dir, removed on drop.
    mod tempfile_path {
        pub struct Zip(pub std::path::PathBuf);
        impl Zip {
            pub fn new(bytes: Vec<u8>) -> Zip {
                use std::sync::atomic::{AtomicU32, Ordering};
                static N: AtomicU32 = AtomicU32::new(0);
                let p = std::env::temp_dir().join(format!(
                    "exhibit-lb-{}-{}.zip",
                    std::process::id(),
                    N.fetch_add(1, Ordering::Relaxed)
                ));
                std::fs::write(&p, bytes).unwrap();
                Zip(p)
            }
        }
        impl Drop for Zip {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
    }

    fn snapshot(files: &[(&str, &str)]) -> Result<ActivityBatch> {
        let z = zip_of(files);
        Letterboxd.activity(
            &Account {
                username: None,
                key: None,
            },
            SyncInput::Snapshot(&z.0),
            &Fake(500, vec![]),
        )
    }

    #[test]
    fn export_rows_become_facts() {
        let batch = snapshot(&[
            (
                "watched.csv",
                "Date,Name,Year,Letterboxd URI\n2024-01-05,\"Crouching Tiger, Hidden Dragon\",2000,https://boxd.it/a\n2024-01-06,Amélie,2001,https://boxd.it/b\n2024-01-07,Solaris,1972,https://boxd.it/c\n2024-01-08,Solaris,2002,https://boxd.it/d\n",
            ),
            (
                "ratings.csv",
                "Date,Name,Year,Letterboxd URI,Rating\n2024-01-05,\"Crouching Tiger, Hidden Dragon\",2000,https://boxd.it/a,4.5\n2024-01-07,Solaris,1972,https://boxd.it/c,5\n",
            ),
            (
                "diary.csv",
                "Date,Name,Year,Letterboxd URI,Rating,Rewatch,Tags,Watched Date\n2024-01-09,Solaris,2002,https://boxd.it/d,3,Yes,,2024-01-08\n2024-01-10,Amélie,2001,https://boxd.it/b,,,,2024-01-06\n",
            ),
            (
                "reviews.csv",
                "Date,Name,Year,Letterboxd URI,Rating,Rewatch,Review,Tags,Watched Date\n2024-01-11,Amélie,2001,https://boxd.it/b,,,\"Warm.\n\nFunny.\",,2024-01-06\n",
            ),
            (
                "likes/films.csv",
                "Date,Name,Year,Letterboxd URI\n2024-01-12,Solaris,1972,https://boxd.it/c\n",
            ),
            (
                "watchlist.csv",
                "Date,Name,Year,Letterboxd URI\n2024-02-01,Heat,1995,https://boxd.it/e\n",
            ),
        ])
        .unwrap();
        let ct = film_ref("Crouching Tiger, Hidden Dragon", Some(2000), None);
        let am = film_ref("Amélie", Some(2001), None);
        let s72 = film_ref("Solaris", Some(1972), None);
        let s02 = film_ref("Solaris", Some(2002), None);
        let heat = film_ref("Heat", Some(1995), None);
        assert_eq!(ct.key, "crouching tiger, hidden dragon|2000");
        let watch = Fact::Watch { rewatch: false };
        let want = vec![
            dated(&ct, watch.clone(), None),
            dated(&am, watch.clone(), None),
            dated(&s72, watch.clone(), None),
            dated(&s02, watch, None),
            dated(&ct, rating(4.5), None),
            dated(&s72, rating(5.0), None),
            dated(&s02, Fact::Watch { rewatch: true }, day("2024-01-08")),
            dated(&s02, rating(3.0), day("2024-01-08")),
            dated(&am, Fact::Watch { rewatch: false }, day("2024-01-06")),
            dated(
                &am,
                Fact::Review {
                    text: "Warm.\n\nFunny.".into(),
                    spoiler: false,
                },
                day("2024-01-06"),
            ),
            dated(&s72, Fact::Like, None),
            dated(&heat, Fact::Watchlisted, day("2024-02-01")),
        ];
        assert_eq!(batch.facts, want);
    }

    #[test]
    fn export_without_watched_csv_or_not_a_zip_is_bad_export() {
        let e = snapshot(&[("ratings.csv", "Date,Name,Year,Letterboxd URI,Rating\n")]).unwrap_err();
        assert_eq!(code(e), Some("service.bad_export"));
        let z = tempfile_path::Zip::new(b"not a zip".to_vec());
        let e = Letterboxd
            .activity(
                &Account {
                    username: None,
                    key: None,
                },
                SyncInput::Snapshot(&z.0),
                &Fake(500, vec![]),
            )
            .unwrap_err();
        assert_eq!(code(e), Some("service.bad_export"));
    }

    #[test]
    fn info_names_the_site_and_its_export() {
        let i = Letterboxd.info();
        assert_eq!((i.id, i.name), ("letterboxd", "Letterboxd"));
        assert_eq!(i.export.as_ref().unwrap().file_ext, "zip");
        assert!(i.mark_svg.contains("currentColor"));
    }
}
