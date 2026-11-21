//! The public's verdict on the owner's works: the scores and awards services report, fetched in
//! batches by the worker, kept by TMDB id, and read by Details and the Rating sort. Nothing here
//! knows which site a score came from.

use rusqlite::{Connection, params};
use serde::Serialize;

use crate::db;
use crate::error::{Error, Result};
use crate::lang;
use crate::model::ItemKind;
use crate::service::{Account, Award, Lookup, Provides, Quota, Score, Service, TmdbFilm};
use crate::sync::code;
use crate::tmdb::Http;

const DAY: i64 = 86_400;
/// Scores of works from the last year move quickly; older ones hardly at all.
const RECENT_EVERY: i64 = 7 * DAY;
const OLD_EVERY: i64 = 60 * DAY;
const AWARDS_EVERY: i64 = 60 * DAY;
/// The languages award names are kept in: the base and every other the window offers.
pub const NAME_LANGS: [&str; 2] = ["en", "es"];
/// The code of a spent daily allowance.
pub const LIMIT_REACHED: &str = "service.limit_reached";

/// A public score and the service that fetched it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SourcedScore {
    pub service: String,
    pub origin: String,
    pub value: f32,
    pub scale: f32,
    pub votes: Option<i64>,
    pub url: Option<String>,
    pub certified: bool,
}

/// An award as Details shows it: named in the library's language.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AwardEntry {
    pub service: String,
    pub body: String,
    pub award: String,
    pub name: String,
    pub year: Option<i32>,
    pub won: bool,
    pub rank: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ReceptionReport {
    /// Works asked about.
    pub asked: u32,
    /// Works the service knew.
    pub found: u32,
    /// Set when the allowance ran out: no asking before this (unix seconds).
    pub paused_until: Option<i64>,
}

/// Asks `svc` about the works that are due: the owner's films and shows (and watchlist films)
/// it has not been asked about, or not lately. Stops for the day when the allowance runs out,
/// which is recorded and is not a failure. `on` gets the count asked so far after each batch.
/// A switched-off service, or one that provides neither scores nor awards, does nothing.
pub fn refresh(
    c: &Connection,
    svc: &dyn Service,
    account: &Account,
    http: &dyn Http,
    now: i64,
    on: &mut dyn FnMut(u32),
) -> Result<ReceptionReport> {
    let info = svc.info();
    let scores = info.provides.contains(&Provides::Reception);
    let awards = info.provides.contains(&Provides::Awards);
    let mut report = ReceptionReport::default();
    if !(scores || awards) || info.batch == 0 {
        return Ok(report);
    }
    let id = info.id;
    // A service that needs nothing is on until the owner switches it off.
    c.prepare_cached("INSERT OR IGNORE INTO service_state(service) VALUES (?1)")?
        .execute([id])?;
    let (enabled, paused_until): (bool, Option<i64>) = c
        .prepare_cached("SELECT enabled, paused_until FROM service_state WHERE service = ?1")?
        .query_row([id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    if !enabled {
        return Ok(report);
    }
    if let Some(until) = paused_until.filter(|t| *t > now) {
        report.paused_until = Some(until);
        return Ok(report);
    }
    let films = due(c, id, scores, now)?;
    for chunk in films.chunks(info.batch) {
        let got = if scores {
            svc.reception(account, chunk, http).map(Got::Scores)
        } else {
            svc.awards(chunk, &NAME_LANGS, http).map(Got::Awards)
        };
        let got = match got {
            Ok(got) => got,
            Err(e) if code(&e) == LIMIT_REACHED => {
                let until = limit_until(&e).unwrap_or(now + DAY);
                pause(c, id, until)?;
                report.paused_until = Some(until);
                return Ok(report);
            }
            Err(e) => {
                c.prepare_cached("UPDATE service_state SET error = ?2 WHERE service = ?1")?
                    .execute(params![id, code(&e)])?;
                return Err(e);
            }
        };
        let quota = store(c, id, chunk, &got, now)?;
        report.asked += chunk.len() as u32;
        report.found += got.found();
        on(report.asked);
        if let Some(q) = quota {
            c.prepare_cached("UPDATE service_state SET remaining = ?2 WHERE service = ?1")?
                .execute(params![id, q.remaining])?;
            if q.remaining == 0 {
                pause(c, id, q.resets_at)?;
                report.paused_until = Some(q.resets_at);
                return Ok(report);
            }
        }
    }
    c.prepare_cached(
        "UPDATE service_state SET synced_at = ?2, error = NULL, paused_until = NULL \
         WHERE service = ?1",
    )?
    .execute(params![id, now])?;
    Ok(report)
}

enum Got {
    Scores(Lookup<Score>),
    Awards(Lookup<Award>),
}

impl Got {
    fn found(&self) -> u32 {
        match self {
            Got::Scores(l) => l.films.len() as u32,
            Got::Awards(l) => l.films.len() as u32,
        }
    }

    fn quota(&self) -> Option<Quota> {
        match self {
            Got::Scores(l) => l.quota,
            Got::Awards(l) => l.quota,
        }
    }
}

fn limit_until(e: &Error) -> Option<i64> {
    e.detail()?
        .params
        .iter()
        .find(|(k, _)| *k == "until")
        .and_then(|(_, v)| v.parse().ok())
}

/// The allowance is spent until `until`.
fn pause(c: &Connection, id: &str, until: i64) -> Result<()> {
    c.prepare_cached(
        "UPDATE service_state SET paused_until = ?2, remaining = 0, error = NULL \
         WHERE service = ?1",
    )?
    .execute(params![id, until])?;
    Ok(())
}

/// The works to ask about, never asked first, then the newest, then the most recently played.
/// Scores cover films and shows; awards films only.
fn due(c: &Connection, service: &str, scores: bool, now: i64) -> Result<Vec<TmdbFilm>> {
    let this_year = 1970 + now / (365 * DAY + DAY / 4);
    let kinds = if scores {
        "('movie', 'show')"
    } else {
        "('movie')"
    };
    let (recent, old) = if scores {
        (RECENT_EVERY, OLD_EVERY)
    } else {
        (AWARDS_EVERY, AWARDS_EVERY)
    };
    let sql = format!(
        "SELECT i.kind, i.tmdb_id FROM items i \
         LEFT JOIN service_lookups l ON l.service = ?1 AND l.kind = i.kind AND l.tmdb_id = i.tmdb_id \
         LEFT JOIN play_state p ON p.item_id = i.id \
         WHERE i.tmdb_id IS NOT NULL AND i.kind IN {kinds} \
           AND (i.kind = 'show' OR i.wanted = 1 OR EXISTS (SELECT 1 FROM files f WHERE f.item_id = i.id)) \
           AND (l.fetched_at IS NULL OR l.fetched_at <= ?2 - \
                CASE WHEN coalesce(i.year, 0) >= ?3 - 1 THEN ?4 ELSE ?5 END) \
         GROUP BY i.kind, i.tmdb_id \
         ORDER BY max(l.fetched_at IS NOT NULL), max(i.year) IS NULL, max(i.year) DESC, \
                  max(coalesce(p.last_played_at, 0)) DESC, i.tmdb_id"
    );
    let mut s = c.prepare_cached(&sql)?;
    let rows = s.query_map(params![service, now, this_year, recent, old], |r| {
        let kind: String = r.get(0)?;
        Ok(TmdbFilm {
            kind: if kind == "show" {
                ItemKind::Show
            } else {
                ItemKind::Movie
            },
            tmdb_id: r.get(1)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn kind_str(k: ItemKind) -> &'static str {
    match k {
        ItemKind::Show => "show",
        _ => "movie",
    }
}

/// Replaces what `service` said about each film of `chunk` with `got`, and notes that each was
/// asked about now, found or not.
fn store(
    c: &Connection,
    service: &str,
    chunk: &[TmdbFilm],
    got: &Got,
    now: i64,
) -> Result<Option<Quota>> {
    let tx = db::write_tx(c)?;
    let table = match got {
        Got::Scores(_) => "service_scores",
        Got::Awards(_) => "service_awards",
    };
    for f in chunk {
        tx.prepare_cached(&format!(
            "DELETE FROM {table} WHERE service = ?1 AND kind = ?2 AND tmdb_id = ?3"
        ))?
        .execute(params![service, kind_str(f.kind), f.tmdb_id])?;
        tx.prepare_cached(
            "INSERT INTO service_lookups(service, kind, tmdb_id, fetched_at) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(service, kind, tmdb_id) DO UPDATE SET fetched_at = excluded.fetched_at",
        )?
        .execute(params![service, kind_str(f.kind), f.tmdb_id, now])?;
    }
    match got {
        Got::Scores(l) => {
            for (f, scores) in &l.films {
                for s in scores {
                    tx.prepare_cached(
                        "INSERT INTO service_scores(service, kind, tmdb_id, origin, value, scale, \
                         votes, url, certified) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
                         ON CONFLICT(kind, tmdb_id, origin) DO UPDATE SET service = excluded.service, \
                         value = excluded.value, scale = excluded.scale, votes = excluded.votes, \
                         url = excluded.url, certified = excluded.certified",
                    )?
                    .execute(params![
                        service,
                        kind_str(f.kind),
                        f.tmdb_id,
                        s.origin,
                        s.value,
                        s.scale,
                        s.votes,
                        s.url,
                        s.certified
                    ])?;
                }
            }
        }
        Got::Awards(l) => {
            for (f, awards) in &l.films {
                for a in awards {
                    let names: serde_json::Map<String, serde_json::Value> = a
                        .names
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone().into()))
                        .collect();
                    // A win and a nomination for the same award: the win stands.
                    tx.prepare_cached(
                        "INSERT INTO service_awards(service, kind, tmdb_id, award, year, won, body, \
                         names, rank) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
                         ON CONFLICT(service, kind, tmdb_id, award, year) DO UPDATE SET \
                         won = max(won, excluded.won)",
                    )?
                    .execute(params![
                        service,
                        kind_str(f.kind),
                        f.tmdb_id,
                        a.award,
                        a.year.unwrap_or(0),
                        a.won,
                        a.body,
                        serde_json::Value::Object(names).to_string(),
                        a.rank
                    ])?;
                }
            }
        }
    }
    tx.commit()?;
    Ok(got.quota())
}

/// The public scores of a work, from switched-on services only.
pub(crate) fn scores_of(c: &Connection, kind: &str, tmdb_id: i64) -> Result<Vec<SourcedScore>> {
    let mut s = c.prepare_cached(
        "SELECT sc.service, sc.origin, sc.value, sc.scale, sc.votes, sc.url, sc.certified \
         FROM service_scores sc JOIN service_state ss ON ss.service = sc.service AND ss.enabled = 1 \
         WHERE sc.kind = ?1 AND sc.tmdb_id = ?2 ORDER BY sc.origin",
    )?;
    let rows = s.query_map(params![kind, tmdb_id], |r| {
        Ok(SourcedScore {
            service: r.get(0)?,
            origin: r.get(1)?,
            value: r.get(2)?,
            scale: r.get(3)?,
            votes: r.get(4)?,
            url: r.get(5)?,
            certified: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// A work's awards from switched-on services, named in the library's language (English where
/// the service had no name in it): wins first, the most prominent first, then by year.
pub(crate) fn awards_of(c: &Connection, kind: &str, tmdb_id: i64) -> Result<Vec<AwardEntry>> {
    let tag = lang::language(c)?;
    let base = tag.split('-').next().unwrap_or("en");
    let mut s = c.prepare_cached(
        "SELECT a.service, a.body, a.award, \
           coalesce(a.names ->> ('$.' || ?3), a.names ->> '$.en', a.award), \
           nullif(a.year, 0), a.won, a.rank \
         FROM service_awards a JOIN service_state ss ON ss.service = a.service AND ss.enabled = 1 \
         WHERE a.kind = ?1 AND a.tmdb_id = ?2 \
         ORDER BY a.won DESC, a.rank, a.year, a.award",
    )?;
    let rows = s.query_map(params![kind, tmdb_id, base], |r| {
        Ok(AwardEntry {
            service: r.get(0)?,
            body: r.get(1)?,
            award: r.get(2)?,
            name: r.get(3)?,
            year: r.get(4)?,
            won: r.get(5)?,
            rank: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The public's rating of the work `item` from 0 to 100: the mean of every score switched-on
/// services hold, each over its own scale. NULL when there is none.
pub(crate) fn public_rating_sql(item: &str) -> String {
    format!(
        "(SELECT avg(100.0 * sc.value / sc.scale) FROM service_scores sc \
         JOIN service_state ss ON ss.service = sc.service AND ss.enabled = 1 \
         WHERE sc.kind = {item}.kind AND sc.tmdb_id = {item}.tmdb_id AND sc.scale > 0)"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Detail;
    use crate::service::{BodyInfo, Need, ServiceInfo};
    use std::sync::Mutex;
    use tempfile::TempDir;

    const NOW: i64 = 1_795_000_000; // 2026-11-17
    const YEAR: i32 = 2026;

    struct NoHttp;
    impl Http for NoHttp {
        fn get(&self, _: &str, _: Option<&str>) -> Result<(u16, Vec<u8>)> {
            panic!("the fake service asks nothing of the network")
        }
    }

    struct Fake {
        info: ServiceInfo,
        asked: Mutex<Vec<Vec<i64>>>,
        /// The allowance left; `None` says nothing about it.
        left: Mutex<Option<u32>>,
        fail: Mutex<Option<&'static str>>,
    }

    static SCORES: ServiceInfo = ServiceInfo {
        id: "scores",
        name: "Scores",
        mark_svg: "<svg/>",
        provides: &[Provides::Reception],
        needs: &[Need::SignIn],
        export: None,
        origins: &[],
        bodies: &[],
        batch: 2,
    };
    static AWARDS: ServiceInfo = ServiceInfo {
        id: "awards",
        name: "Awards",
        mark_svg: "<svg/>",
        provides: &[Provides::Awards],
        needs: &[],
        export: None,
        origins: &[],
        bodies: &[BodyInfo {
            id: "fest",
            name: "Fest",
        }],
        batch: 10,
    };

    fn fake(info: &ServiceInfo) -> Fake {
        Fake {
            info: info.clone(),
            asked: Mutex::default(),
            left: Mutex::default(),
            fail: Mutex::default(),
        }
    }

    impl Fake {
        fn note(&self, films: &[TmdbFilm]) -> Result<Option<Quota>> {
            self.asked
                .lock()
                .unwrap()
                .push(films.iter().map(|f| f.tmdb_id).collect());
            if let Some(code) = *self.fail.lock().unwrap() {
                let d = Detail::coded(code, "no").with("until", NOW + 2500);
                return Err(Error::Provider(d));
            }
            let mut left = self.left.lock().unwrap();
            Ok(left.map(|n| {
                let n = n.saturating_sub(1);
                *left = Some(n);
                Quota {
                    remaining: n,
                    resets_at: NOW + 1000,
                }
            }))
        }

        fn asked(&self) -> Vec<Vec<i64>> {
            std::mem::take(&mut *self.asked.lock().unwrap())
        }
    }

    impl Service for Fake {
        fn info(&self) -> &ServiceInfo {
            &self.info
        }

        fn check(&self, _: &Account, _: &dyn Http) -> Result<u32> {
            Ok(0)
        }

        fn reception(
            &self,
            _: &Account,
            films: &[TmdbFilm],
            _: &dyn Http,
        ) -> Result<Lookup<Score>> {
            let quota = self.note(films)?;
            // It knows every film but 99.
            let films = films
                .iter()
                .filter(|f| f.tmdb_id != 99)
                .map(|f| {
                    let s = Score {
                        origin: "o".into(),
                        value: f.tmdb_id as f32,
                        scale: 100.0,
                        votes: Some(3),
                        url: None,
                        certified: false,
                    };
                    (*f, vec![s])
                })
                .collect();
            Ok(Lookup { films, quota })
        }

        fn awards(
            &self,
            films: &[TmdbFilm],
            langs: &[&str],
            _: &dyn Http,
        ) -> Result<Lookup<Award>> {
            assert_eq!(langs, NAME_LANGS);
            let quota = self.note(films)?;
            let award = |won| Award {
                body: "fest".into(),
                award: "Q1".into(),
                names: vec![
                    ("en".into(), "Top Prize".into()),
                    ("es".into(), "Gran premio".into()),
                ],
                year: Some(1976),
                won,
                rank: 0,
            };
            let films = films
                .iter()
                .map(|f| (*f, vec![award(false), award(true)]))
                .collect();
            Ok(Lookup { films, quota })
        }
    }

    struct Env {
        _dir: TempDir,
        c: Connection,
    }

    fn env() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let c = db::open(dir.path()).unwrap();
        c.execute_batch("INSERT INTO sources(id, path, volume_id) VALUES (1, '/m', 'v')")
            .unwrap();
        Env { _dir: dir, c }
    }

    impl Env {
        /// A film the owner has a file of, or wants, or (neither) does not.
        fn film(&self, tmdb_id: i64, year: i32, file: bool, wanted: bool) {
            self.c
                .execute(
                    "INSERT INTO items(kind, title, year, tmdb_id, wanted) \
                     VALUES ('movie', 'F', ?1, ?2, ?3)",
                    params![year, tmdb_id, wanted],
                )
                .unwrap();
            if file {
                self.c
                    .execute(
                        "INSERT INTO files(source_id, rel_path, item_id) \
                         VALUES (1, ?1, last_insert_rowid())",
                        [format!("{tmdb_id}.mkv")],
                    )
                    .unwrap();
            }
        }

        fn run(&self, svc: &Fake, now: i64) -> Result<ReceptionReport> {
            let account = Account {
                username: None,
                key: Some("t"),
            };
            refresh(&self.c, svc, &account, &NoHttp, now, &mut |_| {})
        }

        fn count(&self, sql: &str) -> i64 {
            self.c.query_row(sql, [], |r| r.get(0)).unwrap()
        }
    }

    #[test]
    fn asks_about_due_works_in_batches_newest_first_and_stores_what_it_hears() {
        let e = env();
        e.film(1, 1976, true, false);
        e.film(2, YEAR, true, false);
        e.film(3, 1995, false, true);
        e.film(99, 2000, true, false);
        // Not the owner's: no file and not wanted.
        e.film(4, 2001, false, false);
        let svc = fake(&SCORES);
        let r = e.run(&svc, NOW).unwrap();
        assert_eq!(svc.asked(), [vec![2, 99], vec![3, 1]]);
        assert_eq!((r.asked, r.found), (4, 3));
        assert_eq!(e.count("SELECT count(*) FROM service_scores"), 3);
        // Asked again at once, nothing is due, not even the film it did not know.
        e.run(&svc, NOW + 60).unwrap();
        assert!(svc.asked().is_empty());
        // A week on, only this year's film is due; two months on, all of them.
        e.run(&svc, NOW + 7 * DAY).unwrap();
        assert_eq!(svc.asked(), [vec![2]]);
        e.run(&svc, NOW + 61 * DAY).unwrap();
        assert_eq!(svc.asked().concat().len(), 4);
        assert_eq!(e.count("SELECT count(*) FROM service_scores"), 3);
    }

    #[test]
    fn a_spent_allowance_pauses_until_it_resets_and_is_no_failure() {
        let e = env();
        for id in 1..=5 {
            e.film(id, 2000, true, false);
        }
        let svc = fake(&SCORES);
        *svc.left.lock().unwrap() = Some(2);
        let r = e.run(&svc, NOW).unwrap();
        assert_eq!(r.paused_until, Some(NOW + 1000));
        assert_eq!(svc.asked().len(), 2);
        // Paused: nothing is asked until the allowance resets.
        e.run(&svc, NOW + 10).unwrap();
        assert!(svc.asked().is_empty());
        let st = &crate::sync::service_states(&e.c).unwrap()[0];
        assert_eq!((st.remaining, st.paused_until), (Some(0), Some(NOW + 1000)));
        assert_eq!(st.error, None);
        // The service saying so itself pauses as well.
        *svc.fail.lock().unwrap() = Some(LIMIT_REACHED);
        let r = e.run(&svc, NOW + 2000).unwrap();
        assert_eq!(r.paused_until, Some(NOW + 2500));
        // After it resets, the rest is asked.
        *svc.fail.lock().unwrap() = None;
        *svc.left.lock().unwrap() = None;
        e.run(&svc, NOW + 3000).unwrap();
        assert_eq!(e.count("SELECT count(*) FROM service_scores"), 5);
        let st = &crate::sync::service_states(&e.c).unwrap()[0];
        assert_eq!((st.paused_until, st.synced_at), (None, Some(NOW + 3000)));
    }

    #[test]
    fn a_failure_is_recorded_and_the_works_are_asked_again() {
        let e = env();
        e.film(1, 2000, true, false);
        let svc = fake(&SCORES);
        *svc.fail.lock().unwrap() = Some("service.unreachable");
        assert!(e.run(&svc, NOW).is_err());
        let st = &crate::sync::service_states(&e.c).unwrap()[0];
        assert_eq!(st.error.as_deref(), Some("service.unreachable"));
        assert_eq!(e.count("SELECT count(*) FROM service_lookups"), 0);
        *svc.fail.lock().unwrap() = None;
        e.run(&svc, NOW + 1).unwrap();
        let st = &crate::sync::service_states(&e.c).unwrap()[0];
        assert_eq!(st.error, None);
        assert_eq!(e.count("SELECT count(*) FROM service_scores"), 1);
    }

    #[test]
    fn awards_are_for_films_named_in_the_library_language_and_a_win_beats_a_nomination() {
        let e = env();
        e.film(1, 1976, true, false);
        e.c.execute_batch("INSERT INTO items(kind, title, tmdb_id) VALUES ('show', 'S', 7)")
            .unwrap();
        let svc = fake(&AWARDS);
        e.run(&svc, NOW).unwrap();
        assert_eq!(svc.asked(), [vec![1]]);
        let got = awards_of(&e.c, "movie", 1).unwrap();
        assert_eq!(got.len(), 1);
        assert!(got[0].won);
        assert_eq!(
            (got[0].name.as_str(), got[0].year),
            ("Top Prize", Some(1976))
        );
        crate::lang::set_language(&e.c, "es-MX").unwrap();
        assert_eq!(awards_of(&e.c, "movie", 1).unwrap()[0].name, "Gran premio");
        // Switched off, it says nothing and asks nothing.
        crate::sync::set_service_enabled(&e.c, "awards", false).unwrap();
        assert!(awards_of(&e.c, "movie", 1).unwrap().is_empty());
        e.run(&svc, NOW + 100 * DAY).unwrap();
        assert!(svc.asked().is_empty());
    }

    #[test]
    fn forgetting_a_service_removes_its_scores_awards_and_lookups() {
        let e = env();
        e.film(1, 2000, true, false);
        e.run(&fake(&SCORES), NOW).unwrap();
        e.run(&fake(&AWARDS), NOW).unwrap();
        crate::sync::set_service_username(&e.c, "scores", None).unwrap();
        assert_eq!(e.count("SELECT count(*) FROM service_scores"), 0);
        assert_eq!(
            e.count("SELECT count(*) FROM service_lookups WHERE service = 'scores'"),
            0
        );
        assert_eq!(e.count("SELECT count(*) FROM service_awards"), 1);
    }
}
