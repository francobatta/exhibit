//! Keeping what TMDB gave fresh. TMDB's terms allow its data to be kept six months at most, so
//! every matched movie and show is fetched again before it is five months old: its details,
//! artwork, episodes and its text in the library's language. Its match, its files and the
//! owner's choices are not touched.
//!
//! The worker calls this a little at a time: oldest first, at most `budget` requests a run,
//! `pace` apart, so a large library is renewed in a steady trickle rather than a burst.
//! Migration 007 spread the works already matched across the window for the same reason.

use std::cell::Cell;
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, params};
use serde::Serialize;

use crate::db;
use crate::error::{Error, Result};
use crate::identify::{
    BACKDROP_SIZE, POSTER_SIZE, POSTER_SMALL_SIZE, STILL_SIZE, download_again, json, poster_name,
    small_poster,
};
use crate::identify_show::still_name;
use crate::lang;
use crate::model::{Id, ItemKind};
use crate::nfo::ExternalId;
use crate::tmdb::{
    MetadataProvider, MovieMeta, SearchHit, SeasonMeta, SeasonTexts, ShowMeta, Texts,
};
use crate::translate::{store_episode_text, store_work_texts, work_texts};

const DAY: i64 = 86_400;
/// A work's details are fetched again once they are this old: five months, well inside TMDB's
/// six, with room for a backlog.
pub const MAX_AGE: i64 = 140 * DAY;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FreshenReport {
    /// Works fetched again.
    pub refreshed: u32,
    /// Requests made to TMDB, images included.
    pub requests: u32,
    /// What went wrong, in English. After TMDB stops answering the rest waits for the next run.
    pub errors: Vec<String>,
}

/// Fetches again the works whose details are older than `MAX_AGE`, oldest first, until about
/// `budget` requests are spent (a work under way is finished). `on` gets each work done. A
/// rejected key ends the run with its error; TMDB out of reach ends it with the error in the
/// report. A work TMDB no longer knows keeps what it has and waits for the next window.
pub fn freshen(
    c: &Connection,
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    now: i64,
    budget: u32,
    pace: Duration,
    on: &mut dyn FnMut(Id),
) -> Result<FreshenReport> {
    let paced = Paced {
        inner: p,
        pace,
        used: Cell::new(0),
    };
    let lang = lang::language(c)?;
    let due: Vec<(Id, String, i64)> = c
        .prepare_cached(
            "SELECT id, kind, tmdb_id FROM items \
             WHERE kind IN ('movie', 'show') AND tmdb_id IS NOT NULL \
               AND coalesce(fetched_at, 0) <= ?1 \
             ORDER BY coalesce(fetched_at, 0), id",
        )?
        .query_map([now - MAX_AGE], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut report = FreshenReport::default();
    for (id, kind, tmdb_id) in due {
        if paced.used.get() >= budget {
            break;
        }
        let done = if kind == "show" {
            show(c, &paced, cache_dir, id, tmdb_id, lang, now)
        } else {
            movie(c, &paced, cache_dir, id, tmdb_id, lang, now)
        };
        match done {
            Ok(()) => {
                report.refreshed += 1;
                on(id);
            }
            Err(Error::NotFound(_)) => {
                c.prepare_cached("UPDATE items SET fetched_at = ?2 WHERE id = ?1")?
                    .execute(params![id, now])?;
            }
            Err(e @ (Error::Invalid(_) | Error::Db(_) | Error::Io(_))) => {
                report.requests = paced.used.get();
                return Err(e);
            }
            Err(e) => {
                report.errors.push(format!("{kind} {tmdb_id}: {e}"));
                break;
            }
        }
    }
    report.requests = paced.used.get();
    Ok(report)
}

fn movie(
    c: &Connection,
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    id: Id,
    tmdb_id: i64,
    lang: &str,
    now: i64,
) -> Result<()> {
    let m: MovieMeta = p.movie(tmdb_id)?;
    let (poster, backdrop) = artwork(
        p,
        cache_dir,
        "movie",
        tmdb_id,
        m.poster.as_deref(),
        m.backdrop.as_deref(),
    );
    let texts = in_lang(lang, || {
        work_texts(p, cache_dir, ItemKind::Movie, tmdb_id, lang, true)
    })?;
    let tx = db::write_tx(c)?;
    tx.prepare_cached(
        "UPDATE items SET title = ?2, year = ?3, overview = ?4, runtime_min = ?5, genres = ?6, \
         people = ?7, poster = coalesce(?8, poster), backdrop = coalesce(?9, backdrop), \
         fetched_at = ?10 WHERE id = ?1",
    )?
    .execute(params![
        id,
        m.title,
        m.year,
        (!m.overview.is_empty()).then_some(&m.overview),
        m.runtime_min,
        json(&m.genres),
        json(&m.people),
        poster,
        backdrop,
        now
    ])?;
    keep_only(&tx, &[id], lang)?;
    if let Some((t, poster)) = texts {
        store_work_texts(&tx, id, lang, &t, poster.as_deref())?;
    }
    tx.commit()?;
    Ok(())
}

/// A season fetched again: its details, its text in the language, and the stills saved afresh
/// (episode number, file name).
type Season = (SeasonMeta, SeasonTexts, Vec<(i32, String)>);

/// A show, and the episodes of it the owner has: their titles, overviews, runtimes and stills.
fn show(
    c: &Connection,
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    id: Id,
    tmdb_id: i64,
    lang: &str,
    now: i64,
) -> Result<()> {
    let m: ShowMeta = p.show(tmdb_id)?;
    let (poster, backdrop) = artwork(
        p,
        cache_dir,
        "tv",
        tmdb_id,
        m.poster.as_deref(),
        m.backdrop.as_deref(),
    );
    let texts = in_lang(lang, || {
        work_texts(p, cache_dir, ItemKind::Show, tmdb_id, lang, true)
    })?;
    let owned: Vec<(Id, i32, i32)> = c
        .prepare_cached(
            "SELECT e.id, e.season, e.episode FROM items e \
             JOIN items s ON s.id = e.parent_id AND s.kind = 'season' \
             WHERE s.parent_id = ?1 AND e.kind = 'episode' \
               AND e.season IS NOT NULL AND e.episode IS NOT NULL \
             ORDER BY e.season, e.episode",
        )?
        .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut numbers: Vec<i32> = owned.iter().map(|(_, s, _)| *s).collect();
    numbers.dedup();
    // Each season the show still lists, its text in the language, and the stills saved afresh.
    let mut seasons: Vec<Season> = Vec::new();
    for n in numbers
        .into_iter()
        .filter(|n| m.seasons.iter().any(|(s, _)| s == n))
    {
        let meta = match p.season(tmdb_id, n) {
            Ok(s) => s,
            Err(Error::NotFound(_)) => continue,
            Err(e) => return Err(e),
        };
        let words = in_lang(lang, || match p.season_texts(tmdb_id, n, lang) {
            Err(Error::NotFound(_)) => Ok(SeasonTexts::default()),
            got => got,
        })?
        .unwrap_or_default();
        let stills = owned
            .iter()
            .filter(|(_, s, _)| *s == n)
            .filter_map(|(_, _, ep)| {
                let path = meta
                    .episodes
                    .iter()
                    .find(|e| e.episode == *ep)?
                    .still
                    .as_deref()?;
                let name = still_name(tmdb_id, n, *ep);
                download_again(p, cache_dir, path, STILL_SIZE, &name).ok()?;
                Some((*ep, name))
            })
            .collect();
        seasons.push((meta, words, stills));
    }

    let tx = db::write_tx(c)?;
    tx.prepare_cached(
        "UPDATE items SET title = ?2, year = ?3, overview = ?4, genres = ?5, people = ?6, \
         poster = coalesce(?7, poster), backdrop = coalesce(?8, backdrop), fetched_at = ?9 \
         WHERE id = ?1",
    )?
    .execute(params![
        id,
        m.title,
        m.year,
        (!m.overview.is_empty()).then_some(&m.overview),
        json(&m.genres),
        json(&m.people),
        poster,
        backdrop,
        now
    ])?;
    let mut ids = vec![id];
    for (meta, words, stills) in &seasons {
        for (ep_id, _, n) in owned.iter().filter(|(_, s, _)| *s == meta.season) {
            let Some(ep) = meta.episodes.iter().find(|e| e.episode == *n) else {
                continue;
            };
            let still = stills.iter().find(|(e, _)| e == n).map(|(_, name)| name);
            tx.prepare_cached(
                "UPDATE items SET title = ?2, overview = ?3, runtime_min = ?4, \
                 backdrop = coalesce(?5, backdrop) WHERE id = ?1",
            )?
            .execute(params![
                ep_id,
                ep.title,
                (!ep.overview.is_empty()).then_some(&ep.overview),
                ep.runtime_min,
                still
            ])?;
            ids.push(*ep_id);
            if lang != lang::BASE {
                let (title, overview) = words
                    .episodes
                    .iter()
                    .find(|(e, _, _)| e == n)
                    .map(|(_, t, o)| (t.as_str(), o.as_str()))
                    .unwrap_or_default();
                store_episode_text(&tx, *ep_id, lang, title, overview)?;
            }
        }
    }
    keep_only(&tx, &ids, lang)?;
    if let Some((t, poster)) = texts {
        store_work_texts(&tx, id, lang, &t, poster.as_deref())?;
    }
    tx.commit()?;
    Ok(())
}

/// `get` when the library speaks another language than the base; English needs nothing more.
fn in_lang<T>(lang: &str, get: impl FnOnce() -> Result<T>) -> Result<Option<T>> {
    if lang == lang::BASE {
        Ok(None)
    } else {
        get().map(Some)
    }
}

/// Text kept for other languages is as old as before: it goes, and is fetched again should the
/// owner switch back.
fn keep_only(c: &Connection, ids: &[Id], lang: &str) -> Result<()> {
    let list = serde_json::to_string(ids).unwrap_or_else(|_| "[]".into());
    c.prepare_cached(
        "DELETE FROM item_texts WHERE lang <> ?2 \
         AND item_id IN (SELECT value FROM json_each(?1))",
    )?
    .execute(params![list, lang])?;
    Ok(())
}

/// The base poster (large and small) and backdrop saved afresh; a file that could not be
/// fetched keeps the one cached.
fn artwork(
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    tag: &str,
    tmdb_id: i64,
    poster: Option<&str>,
    backdrop: Option<&str>,
) -> (Option<String>, Option<String>) {
    let get = |path: Option<&str>, size: &str, name: String| -> Option<String> {
        download_again(p, cache_dir, path?, size, &name).ok()?;
        Some(name)
    };
    let large = get(poster, POSTER_SIZE, poster_name(tag, tmdb_id, lang::BASE));
    if let Some(name) = &large {
        get(poster, POSTER_SMALL_SIZE, small_poster(name));
    }
    (
        large,
        get(
            backdrop,
            BACKDROP_SIZE,
            format!("{tag}-{tmdb_id}-backdrop.jpg"),
        ),
    )
}

/// TMDB, counted and paced: every request waits `pace` first.
struct Paced<'a> {
    inner: &'a dyn MetadataProvider,
    pace: Duration,
    used: Cell<u32>,
}

impl Paced<'_> {
    fn tick(&self) {
        if !self.pace.is_zero() {
            std::thread::sleep(self.pace);
        }
        self.used.set(self.used.get() + 1);
    }
}

impl MetadataProvider for Paced<'_> {
    fn search_movie(&self, title: &str, year: Option<i32>, lang: &str) -> Result<Vec<SearchHit>> {
        self.tick();
        self.inner.search_movie(title, year, lang)
    }
    fn search_show(&self, title: &str, year: Option<i32>, lang: &str) -> Result<Vec<SearchHit>> {
        self.tick();
        self.inner.search_show(title, year, lang)
    }
    fn find_external(&self, id: &ExternalId) -> Result<Option<SearchHit>> {
        self.tick();
        self.inner.find_external(id)
    }
    fn movie(&self, tmdb_id: i64) -> Result<MovieMeta> {
        self.tick();
        self.inner.movie(tmdb_id)
    }
    fn show(&self, tmdb_id: i64) -> Result<ShowMeta> {
        self.tick();
        self.inner.show(tmdb_id)
    }
    fn season(&self, show_id: i64, season: i32) -> Result<SeasonMeta> {
        self.tick();
        self.inner.season(show_id, season)
    }
    fn image(&self, path: &str, size: &str) -> Result<Vec<u8>> {
        self.tick();
        self.inner.image(path, size)
    }
    fn texts(&self, kind: ItemKind, tmdb_id: i64, lang: &str) -> Result<Texts> {
        self.tick();
        self.inner.texts(kind, tmdb_id, lang)
    }
    fn season_texts(&self, show_id: i64, season: i32, lang: &str) -> Result<SeasonTexts> {
        self.tick();
        self.inner.season_texts(show_id, season, lang)
    }
}
