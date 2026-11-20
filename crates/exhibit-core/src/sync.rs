//! Taking a service's activity into the library: each film it names is resolved to a TMDB
//! film, its facts are stored, and the films on its watchlist the owner does not have become
//! `wanted` rows. Nothing here knows which site the activity came from.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::db;
use crate::error::{Detail, Error, Result};
use crate::identify::{self, How};
use crate::matcher::{self, Evidence};
use crate::model::{Id, ItemKind, MatchSource};
use crate::service::{Account, Dated, Fact, Service, SyncInput, normalize_username};
use crate::tmdb::{Http, MetadataProvider};

/// Services name films only, so far.
const KIND: &str = "movie";
const DAY: i64 = 86_400;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ServiceState {
    pub service: String,
    pub enabled: bool,
    pub username: Option<String>,
    pub export_name: Option<String>,
    pub export_at: Option<i64>,
    pub synced_at: Option<i64>,
    pub error: Option<String>,
    /// Films the matcher could not place surely; they are left out.
    pub unmatched: u32,
}

/// What the service holds after a sync, counted in films.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SyncReport {
    pub logged: u32,
    pub rated: u32,
    pub reviewed: u32,
    pub liked: u32,
    pub watchlisted: u32,
    /// Watchlisted films with no file.
    pub not_in_library: u32,
    pub unmatched: u32,
}

pub fn service_states(c: &Connection) -> Result<Vec<ServiceState>> {
    let mut s = c.prepare_cached(
        "SELECT s.service, s.enabled, s.username, s.export_name, s.export_at, s.synced_at, \
         s.error, (SELECT count(*) FROM service_films f \
                   WHERE f.service = s.service AND f.tmdb_id IS NULL AND f.tried = 1) \
         FROM service_state s ORDER BY s.service",
    )?;
    let rows = s.query_map([], |r| {
        Ok(ServiceState {
            service: r.get(0)?,
            enabled: r.get(1)?,
            username: r.get(2)?,
            export_name: r.get(3)?,
            export_at: r.get(4)?,
            synced_at: r.get(5)?,
            error: r.get(6)?,
            unmatched: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn service_username(c: &Connection, service: &str) -> Result<Option<String>> {
    let name: Option<Option<String>> = c
        .prepare_cached("SELECT username FROM service_state WHERE service = ?1")?
        .query_row([service], |r| r.get(0))
        .optional()?;
    Ok(name.flatten())
}

pub fn set_service_enabled(c: &Connection, service: &str, on: bool) -> Result<()> {
    c.prepare_cached(
        "INSERT INTO service_state(service, enabled) VALUES (?1, ?2) \
         ON CONFLICT(service) DO UPDATE SET enabled = excluded.enabled",
    )?
    .execute(params![service, on])?;
    Ok(())
}

/// `None` forgets the service: its facts, films, state and `wanted` rows go. Another username
/// than the one stored starts afresh, so one account's activity never mixes with another's.
pub fn set_service_username(c: &Connection, service: &str, username: Option<&str>) -> Result<()> {
    let name = username.map(normalize_username);
    if name.as_deref() == Some("") {
        return Err(Error::Invalid(Detail::coded(
            "service.no_username",
            "the username is empty",
        )));
    }
    let tx = db::write_tx(c)?;
    let stored = service_username(&tx, service)?;
    match name {
        None => {
            tx.prepare_cached("DELETE FROM service_state WHERE service = ?1")?
                .execute([service])?;
            forget(&tx, service)?;
        }
        Some(name) => {
            if stored.is_some_and(|old| old != name) {
                tx.prepare_cached(
                    "UPDATE service_state SET export_name = NULL, export_at = NULL, \
                     synced_at = NULL, error = NULL WHERE service = ?1",
                )?
                .execute([service])?;
                forget(&tx, service)?;
            }
            tx.prepare_cached(
                "INSERT INTO service_state(service, username) VALUES (?1, ?2) \
                 ON CONFLICT(service) DO UPDATE SET username = excluded.username",
            )?
            .execute([service, name.as_str()])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// The films of `service` left out because no match was sure, by title.
pub fn service_unmatched(c: &Connection, service: &str) -> Result<Vec<(String, Option<i32>)>> {
    let mut s = c.prepare_cached(
        "SELECT coalesce(title, film_key), year FROM service_films \
         WHERE service = ?1 AND tmdb_id IS NULL AND tried = 1 ORDER BY title, year",
    )?;
    let rows = s.query_map([service], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Takes `svc`'s activity into the library. A snapshot replaces what the service said before
/// and is followed at once by its recent activity (`Since(None)`), which a snapshot may not
/// hold yet; `Since` adds and updates. Films are resolved and wanted rows made only with
/// `tmdb`, the provider and the artwork folder. `on` gets each item whose facts changed. A
/// failure is recorded as the service's error and returned.
pub fn sync(
    c: &Connection,
    svc: &dyn Service,
    account: &Account,
    input: SyncInput,
    http: &dyn Http,
    tmdb: Option<(&dyn MetadataProvider, &Path)>,
    on: &mut dyn FnMut(Id),
) -> Result<SyncReport> {
    let done = run(c, svc, account, input, http, tmdb, on);
    if let Err(e) = &done {
        c.prepare_cached(
            "INSERT INTO service_state(service, error) VALUES (?1, ?2) \
             ON CONFLICT(service) DO UPDATE SET error = excluded.error",
        )?
        .execute([svc.info().id, e.to_string().as_str()])?;
    }
    done
}

fn run(
    c: &Connection,
    svc: &dyn Service,
    account: &Account,
    input: SyncInput,
    http: &dyn Http,
    tmdb: Option<(&dyn MetadataProvider, &Path)>,
    on: &mut dyn FnMut(Id),
) -> Result<SyncReport> {
    let service = svc.info().id;
    let (facts, export) = match input {
        SyncInput::Snapshot(path) => {
            let mut facts = svc
                .activity(account, SyncInput::Snapshot(path), http)?
                .facts;
            facts.extend(svc.activity(account, SyncInput::Since(None), http)?.facts);
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            (facts, Some(name.unwrap_or_default()))
        }
        since => (svc.activity(account, since, http)?.facts, None),
    };
    let films = resolve(c, service, &facts, tmdb.map(|(p, _)| p))?;

    let tx = db::write_tx(c)?;
    let mut touched = BTreeSet::new();
    tx.prepare_cached("INSERT OR IGNORE INTO service_state(service) VALUES (?1)")?
        .execute([service])?;
    if let Some(name) = &export {
        let mut s = tx.prepare_cached(
            "SELECT DISTINCT tmdb_id FROM service_facts WHERE service = ?1 AND kind = ?2",
        )?;
        for id in s.query_map([service, KIND], |r| r.get::<_, i64>(0))? {
            touched.insert(id?);
        }
        tx.prepare_cached("DELETE FROM service_facts WHERE service = ?1")?
            .execute([service])?;
        tx.prepare_cached(
            "UPDATE service_state SET export_name = ?2, export_at = ?3 WHERE service = ?1",
        )?
        .execute(params![service, name, db::now()])?;
    }
    for (key, f) in &films {
        tx.prepare_cached(
            "INSERT INTO service_films(service, film_key, kind, title, year, tmdb_id, tried) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
             ON CONFLICT(service, film_key) DO UPDATE SET title = excluded.title, \
             year = excluded.year, tmdb_id = excluded.tmdb_id, tried = excluded.tried",
        )?
        .execute(params![
            service, key, KIND, f.title, f.year, f.tmdb_id, f.tried
        ])?;
    }
    for d in &facts {
        if let Some(tmdb_id) = films.get(&d.film.key).and_then(|f| f.tmdb_id)
            && store(&tx, service, tmdb_id, d)?
        {
            touched.insert(tmdb_id);
        }
    }
    let mut items: BTreeSet<Id> = prune_wanted(&tx)?.into_iter().collect();
    tx.prepare_cached("UPDATE service_state SET synced_at = ?2, error = NULL WHERE service = ?1")?
        .execute(params![service, db::now()])?;
    tx.commit()?;

    if let Some((p, cache_dir)) = tmdb {
        items.extend(add_wanted(c, service, p, cache_dir)?);
    }
    for tmdb_id in touched {
        items.extend(identify::find_item(c, ItemKind::Movie, tmdb_id)?);
    }
    for id in items {
        on(id);
    }
    report(c, service)
}

/// A film as a service names it, and the TMDB film it is.
struct Film {
    title: String,
    year: Option<i32>,
    tmdb_id: Option<i64>,
    /// The matcher has looked for it.
    tried: bool,
}

/// The film each key in `facts` names. A TMDB id from the service is used as is and fixes
/// a film left out before; a film without one is searched for once, and kept only when the
/// match is sure. Without a provider nothing is searched.
fn resolve(
    c: &Connection,
    service: &str,
    facts: &[Dated],
    p: Option<&dyn MetadataProvider>,
) -> Result<HashMap<String, Film>> {
    let mut known = c.prepare_cached(
        "SELECT tmdb_id, tried FROM service_films WHERE service = ?1 AND film_key = ?2",
    )?;
    let mut films: HashMap<String, Film> = HashMap::new();
    for Dated { film, .. } in facts {
        if !films.contains_key(&film.key) {
            let (tmdb_id, tried) = known
                .query_row([service, film.key.as_str()], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?
                .unwrap_or((None, false));
            let f = Film {
                title: film.title.clone(),
                year: film.year,
                tmdb_id,
                tried,
            };
            films.insert(film.key.clone(), f);
        }
        if let (Some(id), Some(f)) = (film.tmdb_id, films.get_mut(&film.key)) {
            f.tmdb_id = Some(id);
        }
    }
    if let Some(p) = p {
        for f in films
            .values_mut()
            .filter(|f| f.tmdb_id.is_none() && !f.tried)
        {
            f.tmdb_id = search(p, &f.title, f.year)?;
            f.tried = true;
        }
    }
    Ok(films)
}

/// The TMDB movie the identification matcher accepts for a title and year, if any.
fn search(p: &dyn MetadataProvider, title: &str, year: Option<i32>) -> Result<Option<i64>> {
    let ev = Evidence {
        title,
        year,
        duration_min: None,
    };
    let hits = identify::search_all(p, ItemKind::Movie, title, year)?;
    let scored: Vec<_> = hits
        .iter()
        .take(identify::MAX_HITS)
        .map(|h| identify::candidate(h, matcher::score(&ev, h, None)))
        .collect();
    Ok(matcher::decide(&scored).map(|c| c.tmdb_id))
}

/// Stores one fact at the start of its day, or day 0 when undated. A rating, like or
/// watchlisting is kept once per film, the newest; false when a newer one is there already.
fn store(c: &Connection, service: &str, tmdb_id: i64, d: &Dated) -> Result<bool> {
    let fact = name(&d.fact);
    let day = d.on.map_or(0, |t| t - t.rem_euclid(DAY));
    if matches!(d.fact, Fact::Rating { .. } | Fact::Like | Fact::Watchlisted) {
        let newer: bool = c
            .prepare_cached(
                "SELECT EXISTS (SELECT 1 FROM service_facts WHERE service = ?1 AND kind = ?2 \
                 AND tmdb_id = ?3 AND fact = ?4 AND on_day > ?5)",
            )?
            .query_row(params![service, KIND, tmdb_id, fact, day], |r| r.get(0))?;
        if newer {
            return Ok(false);
        }
        c.prepare_cached(
            "DELETE FROM service_facts WHERE service = ?1 AND kind = ?2 AND tmdb_id = ?3 \
             AND fact = ?4",
        )?
        .execute(params![service, KIND, tmdb_id, fact])?;
    }
    c.prepare_cached(
        "INSERT OR REPLACE INTO service_facts(service, kind, tmdb_id, fact, on_day, value) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?
    .execute(params![service, KIND, tmdb_id, fact, day, value(&d.fact)])?;
    Ok(true)
}

fn name(f: &Fact) -> &'static str {
    match f {
        Fact::Rating { .. } => "rating",
        Fact::Review { .. } => "review",
        Fact::Watch { .. } => "watch",
        Fact::Like => "like",
        Fact::Watchlisted => "watchlisted",
    }
}

/// What a fact says beside its name, as stored in `service_facts.value`.
#[derive(Serialize)]
#[serde(untagged)]
enum Value<'a> {
    Rating { value: f32, scale: f32 },
    Review { text: &'a str, spoiler: bool },
    Watch { rewatch: bool },
    Nothing {},
}

fn value(f: &Fact) -> String {
    let v = match f {
        Fact::Rating { value, scale } => Value::Rating {
            value: *value,
            scale: *scale,
        },
        Fact::Review { text, spoiler } => Value::Review {
            text,
            spoiler: *spoiler,
        },
        Fact::Watch { rewatch } => Value::Watch { rewatch: *rewatch },
        Fact::Like | Fact::Watchlisted => Value::Nothing {},
    };
    serde_json::to_string(&v).unwrap_or_else(|_| "{}".into())
}

/// Deletes what `service` said, and the wanted rows nothing wants any more.
fn forget(c: &Connection, service: &str) -> Result<()> {
    for sql in [
        "DELETE FROM service_facts WHERE service = ?1",
        "DELETE FROM service_films WHERE service = ?1",
    ] {
        c.prepare_cached(sql)?.execute([service])?;
    }
    prune_wanted(c)?;
    Ok(())
}

/// Deletes each wanted row with no files whose film no service has on its watchlist, and
/// returns them.
fn prune_wanted(c: &Connection) -> Result<Vec<Id>> {
    let ids = c
        .prepare_cached(
            "SELECT id FROM items i WHERE i.wanted = 1 \
             AND NOT EXISTS (SELECT 1 FROM files f WHERE f.item_id = i.id) \
             AND NOT EXISTS (SELECT 1 FROM service_facts sf WHERE sf.kind = i.kind \
                             AND sf.tmdb_id = i.tmdb_id AND sf.fact = 'watchlisted')",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<Id>>>()?;
    for id in &ids {
        c.prepare_cached("DELETE FROM items WHERE id = ?1")?
            .execute([id])?;
    }
    Ok(ids)
}

/// Gives each film on `service`'s watchlist that has no row a wanted row, filled from TMDB as
/// identification fills one. A film TMDB no longer knows is skipped.
fn add_wanted(
    c: &Connection,
    service: &str,
    p: &dyn MetadataProvider,
    cache_dir: &Path,
) -> Result<Vec<Id>> {
    let missing = c
        .prepare_cached(
            "SELECT DISTINCT sf.tmdb_id FROM service_facts sf \
             WHERE sf.service = ?1 AND sf.kind = ?2 AND sf.fact = 'watchlisted' \
             AND NOT EXISTS (SELECT 1 FROM items i WHERE i.kind = sf.kind \
                             AND i.tmdb_id = sf.tmdb_id)",
        )?
        .query_map([service, KIND], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<i64>>>()?;
    let how = How::certain(MatchSource::Lookup);
    let mut made = Vec::new();
    for tmdb_id in missing {
        let m = match p.movie(tmdb_id) {
            Ok(m) => m,
            Err(Error::NotFound(_)) => continue,
            Err(e) => return Err(e),
        };
        made.push(identify::save_movie(c, p, cache_dir, None, &m, how)?);
    }
    Ok(made)
}

fn report(c: &Connection, service: &str) -> Result<SyncReport> {
    let films = |fact: &str| -> Result<u32> {
        Ok(c.prepare_cached(
            "SELECT count(DISTINCT tmdb_id) FROM service_facts WHERE service = ?1 AND fact = ?2",
        )?
        .query_row([service, fact], |r| r.get(0))?)
    };
    let one = |sql: &str| -> Result<u32> {
        Ok(c.prepare_cached(sql)?.query_row([service], |r| r.get(0))?)
    };
    Ok(SyncReport {
        logged: films("watch")?,
        rated: films("rating")?,
        reviewed: films("review")?,
        liked: films("like")?,
        watchlisted: films("watchlisted")?,
        not_in_library: one("SELECT count(DISTINCT sf.tmdb_id) FROM service_facts sf \
             WHERE sf.service = ?1 AND sf.fact = 'watchlisted' AND NOT EXISTS \
             (SELECT 1 FROM items i JOIN files f ON f.item_id = i.id \
              WHERE i.kind = sf.kind AND i.tmdb_id = sf.tmdb_id)")?,
        unmatched: one("SELECT count(*) FROM service_films \
             WHERE service = ?1 AND tmdb_id IS NULL AND tried = 1")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identify::{self, IdentifyScope};
    use crate::model::{ItemKind, Person};
    use crate::nfo::ExternalId;
    use crate::parse::normalize_title;
    use crate::scan::{add_source, scan_source};
    use crate::service::{
        ActivityBatch, Dated, Fact, FilmRef, Need, Provides, Service, ServiceInfo, SyncInput,
        film_key,
    };
    use crate::tmdb::{MovieMeta, SearchHit, SeasonMeta, SeasonTexts, ShowMeta, Texts};
    use crate::volume::FakeVolumes;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use tempfile::TempDir;

    const SVC: &str = "diary";
    const INFO: ServiceInfo = ServiceInfo {
        id: SVC,
        name: "Diary",
        mark_svg: "<svg/>",
        provides: &[Provides::Activity],
        needs: &[Need::Username],
        export: None,
    };
    /// 2026-01-01 and 2026-03-03, midnight UTC.
    const JAN_1: i64 = 1_767_225_600;
    const MAR_3: i64 = 1_772_496_000;

    /// A service whose export and feed the test sets.
    #[derive(Default)]
    struct FakeService {
        export: Mutex<Vec<Dated>>,
        feed: Mutex<Vec<Dated>>,
        fail: Mutex<bool>,
    }

    impl FakeService {
        fn export(&self, facts: Vec<Dated>) {
            *self.export.lock().unwrap() = facts;
        }

        fn feed(&self, facts: Vec<Dated>) {
            *self.feed.lock().unwrap() = facts;
        }
    }

    impl Service for FakeService {
        fn info(&self) -> &ServiceInfo {
            &INFO
        }

        fn activity(&self, _: &Account, input: SyncInput, _: &dyn Http) -> Result<ActivityBatch> {
            if *self.fail.lock().unwrap() {
                return Err(Error::Invalid(Detail::coded(
                    "service.no_user",
                    "no such user",
                )));
            }
            let facts = match input {
                SyncInput::Snapshot(_) => self.export.lock().unwrap().clone(),
                SyncInput::Since(_) => self.feed.lock().unwrap().clone(),
            };
            Ok(ActivityBatch { facts })
        }

        fn check(&self, _: &Account, _: &dyn Http) -> Result<u32> {
            Ok(3)
        }
    }

    struct NoHttp;

    impl Http for NoHttp {
        fn get(&self, url: &str, _: Option<&str>) -> Result<(u16, Vec<u8>)> {
            panic!("no request expected: {url}")
        }
    }

    /// In-memory TMDB knowing a few movies; every search is logged.
    #[derive(Default)]
    struct Fake {
        movies: Vec<MovieMeta>,
        hits: Vec<(&'static str, Vec<SearchHit>)>,
        calls: Mutex<Vec<String>>,
    }

    impl Fake {
        fn searches(&self, title: &str) -> usize {
            let calls = self.calls.lock().unwrap();
            calls
                .iter()
                .filter(|c| **c == format!("search {title}"))
                .count()
        }
    }

    fn not_found(what: String) -> Error {
        Error::NotFound(format!("TMDB has nothing at {what}").into())
    }

    impl MetadataProvider for Fake {
        fn search_movie(&self, title: &str, _: Option<i32>, _: &str) -> Result<Vec<SearchHit>> {
            self.calls.lock().unwrap().push(format!("search {title}"));
            Ok(self
                .hits
                .iter()
                .find(|(k, _)| normalize_title(k) == normalize_title(title))
                .map(|(_, h)| h.clone())
                .unwrap_or_default())
        }

        fn search_show(&self, _: &str, _: Option<i32>, _: &str) -> Result<Vec<SearchHit>> {
            Ok(Vec::new())
        }

        fn find_external(&self, _: &ExternalId) -> Result<Option<SearchHit>> {
            Ok(None)
        }

        fn movie(&self, id: i64) -> Result<MovieMeta> {
            self.calls.lock().unwrap().push(format!("movie {id}"));
            let m = self.movies.iter().find(|m| m.tmdb_id == id);
            m.cloned().ok_or_else(|| not_found(format!("movie {id}")))
        }

        fn show(&self, id: i64) -> Result<ShowMeta> {
            Err(not_found(format!("show {id}")))
        }

        fn season(&self, id: i64, n: i32) -> Result<SeasonMeta> {
            Err(not_found(format!("season {id}/{n}")))
        }

        fn image(&self, path: &str, size: &str) -> Result<Vec<u8>> {
            Ok(format!("{size}{path}").into_bytes())
        }

        fn texts(&self, _: ItemKind, id: i64, lang: &str) -> Result<Texts> {
            Err(not_found(format!("texts {id} {lang}")))
        }

        fn season_texts(&self, id: i64, n: i32, lang: &str) -> Result<SeasonTexts> {
            Err(not_found(format!("season_texts {id}/{n} {lang}")))
        }
    }

    fn hit(id: i64, title: &str, year: i32) -> SearchHit {
        SearchHit {
            tmdb_id: id,
            kind: ItemKind::Movie,
            title: title.into(),
            original_title: title.into(),
            year: Some(year),
            popularity: 1.0,
            poster: None,
            titles: Vec::new(),
        }
    }

    fn movie(id: i64, title: &str, year: i32) -> MovieMeta {
        MovieMeta {
            tmdb_id: id,
            title: title.into(),
            year: Some(year),
            overview: format!("About {title}"),
            runtime_min: Some(160),
            genres: vec!["Drama".into()],
            people: vec![Person {
                name: "A Director".into(),
                role: "director".into(),
                ..Person::default()
            }],
            poster: Some(format!("/p{id}.jpg")),
            backdrop: Some(format!("/b{id}.jpg")),
        }
    }

    /// TMDB knows two films called Solaris.
    fn solaris() -> Fake {
        Fake {
            movies: vec![movie(593, "Solaris", 1972), movie(2103, "Solaris", 2002)],
            hits: vec![(
                "Solaris",
                vec![hit(593, "Solaris", 1972), hit(2103, "Solaris", 2002)],
            )],
            ..Default::default()
        }
    }

    fn by_title(title: &str, year: Option<i32>) -> FilmRef {
        FilmRef {
            key: film_key(title, year),
            tmdb_id: None,
            title: title.into(),
            year,
        }
    }

    fn by_id(title: &str, year: Option<i32>, tmdb_id: i64) -> FilmRef {
        FilmRef {
            tmdb_id: Some(tmdb_id),
            ..by_title(title, year)
        }
    }

    fn dated(film: FilmRef, fact: Fact, on: Option<i64>) -> Dated {
        Dated { film, fact, on }
    }

    struct Env {
        data: TempDir,
        c: Connection,
    }

    impl Env {
        fn cache(&self) -> PathBuf {
            self.data.path().join("cache").join("images")
        }

        fn sync(&self, s: &FakeService, p: Option<&Fake>, input: SyncInput) -> SyncReport {
            self.try_sync(s, p, input, &mut |_| {}).unwrap()
        }

        fn try_sync(
            &self,
            s: &FakeService,
            p: Option<&Fake>,
            input: SyncInput,
            on: &mut dyn FnMut(Id),
        ) -> Result<SyncReport> {
            let cache = self.cache();
            let tmdb = p.map(|p| (p as &dyn MetadataProvider, cache.as_path()));
            let account = Account {
                username: Some("someone"),
                key: None,
            };
            sync(&self.c, s, &account, input, &NoHttp, tmdb, on)
        }

        fn facts(&self) -> Vec<(i64, String, i64, String)> {
            let mut s = self
                .c
                .prepare(
                    "SELECT tmdb_id, fact, on_day, value FROM service_facts \
                     ORDER BY tmdb_id, fact, on_day",
                )
                .unwrap();
            s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        }

        /// (id, title, wanted) of every movie row with `tmdb_id`.
        fn movies(&self, tmdb_id: i64) -> Vec<(Id, String, bool)> {
            let mut s = self
                .c
                .prepare("SELECT id, title, wanted FROM items WHERE tmdb_id=?1 ORDER BY id")
                .unwrap();
            s.query_map([tmdb_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        }

        fn count(&self, sql: &str) -> i64 {
            self.c.query_row(sql, [], |r| r.get(0)).unwrap()
        }
    }

    fn env() -> Env {
        let data = tempfile::tempdir().unwrap();
        let c = db::open(data.path()).unwrap();
        Env { data, c }
    }

    fn export() -> SyncInput<'static> {
        SyncInput::Snapshot(Path::new("diary-export.zip"))
    }

    #[test]
    fn snapshot_resolves_by_title_and_year_and_leaves_unsure_out() {
        let e = env();
        let p = solaris();
        let s = FakeService::default();
        s.export(vec![
            dated(
                by_title("Solaris", Some(1972)),
                Fact::Watch { rewatch: false },
                Some(JAN_1 + 3600),
            ),
            dated(
                by_title("Untitled Thing", None),
                Fact::Rating {
                    value: 3.5,
                    scale: 5.0,
                },
                None,
            ),
        ]);
        let r = e.sync(&s, Some(&p), export());
        assert_eq!((r.logged, r.rated, r.unmatched), (1, 0, 1), "{r:?}");
        assert_eq!(
            e.facts(),
            [(593, "watch".into(), JAN_1, r#"{"rewatch":false}"#.into())]
        );
        assert_eq!(
            service_unmatched(&e.c, SVC).unwrap(),
            [("Untitled Thing".to_string(), None)]
        );
        let state = &service_states(&e.c).unwrap()[0];
        assert_eq!(state.export_name.as_deref(), Some("diary-export.zip"));
        assert!(state.export_at.is_some() && state.synced_at.is_some());
        assert_eq!((state.unmatched, state.enabled), (1, true));

        let (solaris_searches, untitled) = (p.searches("Solaris"), p.searches("Untitled Thing"));
        assert!(untitled > 0);
        let r = e.sync(&s, Some(&p), export());
        assert_eq!(r.unmatched, 1);
        assert_eq!(p.searches("Untitled Thing"), untitled);
        assert_eq!(p.searches("Solaris"), solaris_searches);
    }

    #[test]
    fn a_later_feed_entry_with_a_tmdb_id_fixes_an_unmatched_row() {
        let e = env();
        let p = solaris();
        let s = FakeService::default();
        let rating = Fact::Rating {
            value: 4.0,
            scale: 5.0,
        };
        s.export(vec![dated(
            by_title("Untitled Thing", None),
            rating.clone(),
            None,
        )]);
        assert_eq!(e.sync(&s, Some(&p), export()).unmatched, 1);

        s.feed(vec![dated(
            by_id("Untitled Thing", None, 777),
            Fact::Like,
            Some(MAR_3),
        )]);
        let r = e.sync(&s, Some(&p), SyncInput::Since(None));
        assert_eq!((r.unmatched, r.liked), (0, 1), "{r:?}");
        assert!(service_unmatched(&e.c, SVC).unwrap().is_empty());

        // The next export's rating of it is kept, without searching again.
        let searched = p.searches("Untitled Thing");
        let r = e.sync(&s, Some(&p), export());
        assert_eq!((r.rated, r.liked, r.unmatched), (1, 1, 0), "{r:?}");
        assert_eq!(p.searches("Untitled Thing"), searched);
        assert_eq!(
            e.facts(),
            [
                (777, "like".into(), MAR_3, "{}".into()),
                (
                    777,
                    "rating".into(),
                    0,
                    r#"{"value":4.0,"scale":5.0}"#.into()
                ),
            ]
        );
    }

    #[test]
    fn snapshot_then_since_keeps_newer_feed_entries() {
        let e = env();
        let p = solaris();
        let s = FakeService::default();
        let rated = |value| Fact::Rating { value, scale: 5.0 };
        s.export(vec![
            dated(
                by_title("Solaris", Some(1972)),
                Fact::Watch { rewatch: false },
                Some(JAN_1),
            ),
            dated(by_title("Solaris", Some(1972)), rated(3.0), Some(JAN_1)),
        ]);
        s.feed(vec![
            dated(
                by_id("Solaris", Some(1972), 593),
                Fact::Watch { rewatch: true },
                Some(MAR_3 + 20 * 3600),
            ),
            dated(by_id("Solaris", Some(1972), 593), rated(4.5), Some(MAR_3)),
        ]);
        let expected: Vec<(i64, String, i64, String)> = vec![
            (
                593,
                "rating".into(),
                MAR_3,
                r#"{"value":4.5,"scale":5.0}"#.into(),
            ),
            (593, "watch".into(), JAN_1, r#"{"rewatch":false}"#.into()),
            (593, "watch".into(), MAR_3, r#"{"rewatch":true}"#.into()),
        ];
        let r = e.sync(&s, Some(&p), export());
        assert_eq!((r.logged, r.rated), (1, 1), "{r:?}");
        assert_eq!(e.facts(), expected);

        // A feed entry older than what is stored does not replace it.
        s.feed(vec![dated(
            by_id("Solaris", Some(1972), 593),
            rated(1.0),
            Some(JAN_1),
        )]);
        e.sync(&s, Some(&p), SyncInput::Since(Some(JAN_1)));
        assert_eq!(e.facts(), expected);
    }

    #[test]
    fn watchlist_makes_wanted_rows_and_drops_them_when_gone() {
        let e = env();
        let p = solaris();
        let s = FakeService::default();
        s.export(vec![dated(
            by_title("Solaris", Some(1972)),
            Fact::Watchlisted,
            Some(JAN_1),
        )]);
        let mut seen = Vec::new();
        let r = e
            .try_sync(&s, Some(&p), export(), &mut |id| seen.push(id))
            .unwrap();
        assert_eq!((r.watchlisted, r.not_in_library), (1, 1), "{r:?}");
        let rows = e.movies(593);
        assert_eq!(rows.len(), 1);
        let (id, title, wanted) = &rows[0];
        assert_eq!((title.as_str(), *wanted), ("Solaris", true));
        assert_eq!(seen, [*id]);
        let poster: Option<String> =
            e.c.query_row("SELECT poster FROM items WHERE id=?1", [id], |r| r.get(0))
                .unwrap();
        assert_eq!(poster.as_deref(), Some("movie-593-poster.jpg"));

        // Synced again, nothing new is made.
        e.sync(&s, Some(&p), export());
        assert_eq!(e.movies(593).len(), 1);

        s.export(Vec::new());
        let r = e.sync(&s, Some(&p), export());
        assert_eq!((r.watchlisted, r.not_in_library), (0, 0), "{r:?}");
        assert!(e.movies(593).is_empty());
    }

    #[test]
    fn without_a_key_only_films_with_ids_are_kept() {
        let e = env();
        let s = FakeService::default();
        s.export(vec![
            dated(by_title("Solaris", Some(1972)), Fact::Like, None),
            dated(by_id("Stalker", Some(1979), 1398), Fact::Watchlisted, None),
        ]);
        let r = e.sync(&s, None, export());
        assert_eq!(
            (r.liked, r.watchlisted, r.not_in_library, r.unmatched),
            (0, 1, 1, 0),
            "{r:?}"
        );
        assert_eq!(e.facts(), [(1398, "watchlisted".into(), 0, "{}".into())]);
        assert_eq!(e.count("SELECT count(*) FROM items"), 0);
        // With a key the next sync searches the film it could not before.
        let p = solaris();
        let r = e.sync(&s, Some(&p), export());
        assert_eq!(r.liked, 1, "{r:?}");
    }

    #[test]
    fn a_file_identified_as_a_wanted_film_joins_its_row() {
        let e = env();
        let p = solaris();
        let s = FakeService::default();
        s.export(vec![dated(
            by_title("Solaris", Some(1972)),
            Fact::Watchlisted,
            None,
        )]);
        e.sync(&s, Some(&p), export());
        let (wanted_id, _, _) = e.movies(593)[0];
        e.c.execute(
            "UPDATE items SET watched_choice = 1, added_at = 5 WHERE id = ?1",
            [wanted_id],
        )
        .unwrap();

        let drive = tempfile::tempdir().unwrap();
        let media = drive.path().join("media");
        std::fs::create_dir_all(&media).unwrap();
        std::fs::write(media.join("Solaris (1972).mkv"), b"").unwrap();
        let v = FakeVolumes(vec![("VOL1".into(), drive.path().to_path_buf())]);
        let src = add_source(&e.c, &v, &media).unwrap().id;
        scan_source(&e.c, &v, src, &mut |_| {}).unwrap();
        let r = identify::identify(
            &e.c,
            &v,
            &p,
            &e.cache(),
            IdentifyScope::Unmatched,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(r.matched, 1, "{r:?}");

        let rows = e.movies(593);
        assert_eq!(rows, [(wanted_id, "Solaris".to_string(), false)]);
        let (watched, added_at): (Option<bool>, i64) =
            e.c.query_row(
                "SELECT watched_choice, added_at FROM items WHERE id = ?1",
                [wanted_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(watched, Some(true));
        // It arrived now, not when it was put on the watchlist.
        assert!(added_at > 5);
        let file_item: Id =
            e.c.query_row("SELECT item_id FROM files", [], |r| r.get(0))
                .unwrap();
        assert_eq!(file_item, wanted_id);
        assert_eq!(e.sync(&s, Some(&p), export()).not_in_library, 0);
    }

    #[test]
    fn forgetting_a_service_removes_its_facts_and_wanted_rows() {
        let e = env();
        let p = solaris();
        let s = FakeService::default();
        set_service_username(&e.c, SVC, Some(" @Someone ")).unwrap();
        assert_eq!(
            service_states(&e.c).unwrap()[0].username.as_deref(),
            Some("someone")
        );
        s.export(vec![
            dated(by_title("Solaris", Some(1972)), Fact::Watchlisted, None),
            dated(by_title("Untitled Thing", None), Fact::Like, None),
        ]);
        e.sync(&s, Some(&p), export());
        assert_eq!(e.movies(593).len(), 1);

        set_service_username(&e.c, SVC, None).unwrap();
        assert!(service_states(&e.c).unwrap().is_empty());
        for table in ["service_facts", "service_films", "service_state", "items"] {
            assert_eq!(
                e.count(&format!("SELECT count(*) FROM {table}")),
                0,
                "{table}"
            );
        }
    }

    #[test]
    fn another_username_starts_afresh() {
        let e = env();
        let p = solaris();
        let s = FakeService::default();
        set_service_username(&e.c, SVC, Some("someone")).unwrap();
        s.export(vec![dated(
            by_title("Solaris", Some(1972)),
            Fact::Like,
            None,
        )]);
        e.sync(&s, Some(&p), export());
        set_service_username(&e.c, SVC, Some("Someone")).unwrap();
        assert_eq!(e.facts().len(), 1);
        set_service_username(&e.c, SVC, Some("someone-else")).unwrap();
        assert!(e.facts().is_empty());
        let state = &service_states(&e.c).unwrap()[0];
        assert_eq!(state.username.as_deref(), Some("someone-else"));
        assert_eq!(
            (state.export_name.as_deref(), state.synced_at),
            (None, None)
        );
    }

    #[test]
    fn switching_a_service_off_keeps_its_state() {
        let e = env();
        set_service_enabled(&e.c, SVC, false).unwrap();
        set_service_username(&e.c, SVC, Some("someone")).unwrap();
        let state = &service_states(&e.c).unwrap()[0];
        assert_eq!(
            (state.enabled, state.username.as_deref()),
            (false, Some("someone"))
        );
        set_service_enabled(&e.c, SVC, true).unwrap();
        assert!(service_states(&e.c).unwrap()[0].enabled);
    }

    #[test]
    fn a_failed_sync_records_its_error_and_the_next_clears_it() {
        let e = env();
        let s = FakeService::default();
        *s.fail.lock().unwrap() = true;
        assert!(e.try_sync(&s, None, export(), &mut |_| {}).is_err());
        let state = &service_states(&e.c).unwrap()[0];
        assert!(state.error.as_deref().unwrap().contains("no such user"));
        assert_eq!(state.synced_at, None);

        *s.fail.lock().unwrap() = false;
        e.sync(&s, None, SyncInput::Since(None));
        assert_eq!(service_states(&e.c).unwrap()[0].error, None);
    }

    #[test]
    fn the_library_syncs_and_checks_through_its_own_wiring() {
        use crate::library::Library;
        use crate::secrets::MemorySecrets;
        use crate::tools::Env as Machine;
        let data = tempfile::tempdir().unwrap();
        let lib = Library::open(
            data.path().to_path_buf(),
            Box::new(FakeVolumes(Vec::new())),
            Box::new(MemorySecrets::default()),
            Machine::default(),
        )
        .unwrap();
        let s = FakeService::default();
        assert_eq!(lib.check_service(&s, "@someone").unwrap(), 3);
        lib.set_service_username(SVC, Some("someone")).unwrap();
        s.export(vec![dated(
            by_id("Stalker", Some(1979), 1398),
            Fact::Like,
            None,
        )]);
        let r = lib.sync_activity(&s, export(), &mut |_| {}).unwrap();
        assert_eq!(r.liked, 1);
        lib.set_service_enabled(SVC, false).unwrap();
        let states = lib.service_states().unwrap();
        assert_eq!((states.len(), states[0].enabled), (1, false));
        assert!(lib.service_unmatched(SVC).unwrap().is_empty());
    }
}
