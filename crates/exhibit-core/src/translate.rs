//! The library's text in the owner's language. Identification stores TMDB's English, the
//! base; this fills `item_texts` with the title, overview, genres and poster of every
//! matched movie, show and episode in another language, once. Nothing is matched again.
//!
//! A work TMDB has not translated still gets its row, with empty fields, so it is not asked
//! for again; reads fall back to the base field by field.

use crate::db;
use crate::error::{Error, Result};
use crate::identify::{
    POSTER_SIZE, POSTER_SMALL_SIZE, download, download_again, poster_name, small_poster,
};
use crate::lang;
use crate::model::{Id, ItemKind};
use crate::par;
use crate::rows::enum_str;
use crate::tmdb::{MetadataProvider, Texts};
use rusqlite::{Connection, params};
use serde::Serialize;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TranslateReport {
    /// Items that now have their text in the language.
    pub translated: u32,
    /// What went wrong along the way, in English. After TMDB stops answering the rest waits
    /// for the next run.
    pub errors: Vec<String>,
}

/// Fills in `lang` for every matched item that lacks it. English needs nothing. `on` gets
/// each item done. A rejected key ends the run with its error; TMDB out of reach ends it
/// with the error in the report.
pub fn translate(
    c: &Connection,
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    lang: &str,
    on: &mut dyn FnMut(Id),
) -> Result<TranslateReport> {
    let mut report = TranslateReport::default();
    if lang == lang::BASE {
        return Ok(report);
    }
    if works(c, p, cache_dir, lang, &mut report, on)? {
        episodes(c, p, lang, &mut report, on)?;
    }
    Ok(report)
}

/// Whether the run should go on: false once TMDB is out of reach.
fn keep_going(e: Error, what: String, report: &mut TranslateReport) -> Result<bool> {
    match e {
        Error::Invalid(_) | Error::Db(_) | Error::Io(_) => Err(e),
        e => {
            let unreachable = matches!(e, Error::Provider(_));
            report.errors.push(format!("{what}: {e}"));
            Ok(!unreachable)
        }
    }
}

/// Movies and shows. False when TMDB went out of reach.
fn works(
    c: &Connection,
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    lang: &str,
    report: &mut TranslateReport,
    on: &mut dyn FnMut(Id),
) -> Result<bool> {
    let todo: Vec<(Id, String, i64)> = c
        .prepare_cached(
            "SELECT i.id, i.kind, i.tmdb_id FROM items i \
             WHERE i.kind IN ('movie', 'show') AND i.tmdb_id IS NOT NULL \
               AND NOT EXISTS (SELECT 1 FROM item_texts t WHERE t.item_id = i.id AND t.lang = ?1) \
             ORDER BY i.id",
        )?
        .query_map([lang], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let kind_of = |kind: &str| {
        if kind == enum_str(ItemKind::Show) {
            ItemKind::Show
        } else {
            ItemKind::Movie
        }
    };
    // Fetched a few at a time; stored here, in the order they come back. `Err(None)` stops
    // the run once TMDB is out of reach; `Err(Some(_))` is a failure that ends it.
    let fetch = |(_, kind, tmdb_id): &(Id, String, i64)| {
        work_texts(p, cache_dir, kind_of(kind), *tmdb_id, lang, false)
    };
    let done = par::each(&todo, WORK_THREADS, fetch, |(id, kind, tmdb_id), got| {
        let (texts, poster) = match got {
            Ok(t) => t,
            Err(e) => {
                let what = format!("{} {tmdb_id} in {lang}", enum_str(kind_of(kind)));
                return match keep_going(e, what, report) {
                    Ok(true) => Ok(()),
                    Ok(false) => Err(None),
                    Err(e) => Err(Some(e)),
                };
            }
        };
        store_work_texts(c, *id, lang, &texts, poster.as_deref()).map_err(Some)?;
        report.translated += 1;
        on(*id);
        Ok(())
    });
    match done {
        Ok(()) => Ok(true),
        Err(None) => Ok(false),
        Err(Some(e)) => Err(e),
    }
}

/// Works whose text is fetched at once.
const WORK_THREADS: usize = 6;

/// A movie's or show's text in `lang`, and its poster in that language saved (afresh with
/// `again`, else only when not cached yet). A work TMDB no longer knows has empty text.
pub(crate) fn work_texts(
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    kind: ItemKind,
    tmdb_id: i64,
    lang: &str,
    again: bool,
) -> Result<(Texts, Option<String>)> {
    let texts = match p.texts(kind, tmdb_id, lang) {
        Ok(t) => t,
        Err(Error::NotFound(_)) => Texts::default(),
        Err(e) => return Err(e),
    };
    let tag = if kind == ItemKind::Show {
        "tv"
    } else {
        "movie"
    };
    let poster = texts
        .poster
        .as_deref()
        .and_then(|path| poster(p, cache_dir, path, &poster_name(tag, tmdb_id, lang), again));
    Ok((texts, poster))
}

pub(crate) fn store_work_texts(
    c: &Connection,
    id: Id,
    lang: &str,
    texts: &Texts,
    poster: Option<&str>,
) -> Result<()> {
    c.prepare_cached(
        "INSERT INTO item_texts(item_id, lang, title, overview, genres, poster) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(item_id, lang) DO UPDATE SET title=excluded.title, \
         overview=excluded.overview, genres=excluded.genres, poster=excluded.poster",
    )?
    .execute(params![
        id,
        lang,
        texts.title,
        texts.overview,
        serde_json::to_string(&texts.genres).unwrap_or_else(|_| "[]".into()),
        poster,
    ])?;
    Ok(())
}

pub(crate) fn store_episode_text(
    c: &Connection,
    id: Id,
    lang: &str,
    title: &str,
    overview: &str,
) -> Result<()> {
    c.prepare_cached(
        "INSERT INTO item_texts(item_id, lang, title, overview) VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(item_id, lang) DO UPDATE SET title=excluded.title, \
         overview=excluded.overview",
    )?
    .execute(params![id, lang, title, overview])?;
    Ok(())
}

/// Downloads a poster large and small; its file name, or `None` when the download failed, in
/// which case the base poster shows.
fn poster(
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    path: &str,
    name: &str,
    again: bool,
) -> Option<String> {
    let get = if again { download_again } else { download };
    get(p, cache_dir, path, POSTER_SIZE, name).ok()?;
    get(p, cache_dir, path, POSTER_SMALL_SIZE, &small_poster(name)).ok()?;
    Some(name.to_string())
}

/// Episodes, one season of a show per request.
fn episodes(
    c: &Connection,
    p: &dyn MetadataProvider,
    lang: &str,
    report: &mut TranslateReport,
    on: &mut dyn FnMut(Id),
) -> Result<()> {
    // Seasons of matched shows with an episode not yet in `lang`.
    let seasons: Vec<(i64, i32)> = c
        .prepare_cached(
            "SELECT DISTINCT sh.tmdb_id, e.season FROM items e \
             JOIN items s ON s.id = e.parent_id AND s.kind = 'season' \
             JOIN items sh ON sh.id = s.parent_id AND sh.kind = 'show' \
             WHERE e.kind = 'episode' AND e.season IS NOT NULL AND e.episode IS NOT NULL \
               AND sh.tmdb_id IS NOT NULL \
               AND NOT EXISTS (SELECT 1 FROM item_texts t WHERE t.item_id = e.id AND t.lang = ?1) \
             ORDER BY sh.tmdb_id, e.season",
        )?
        .query_map([lang], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (show, season) in seasons {
        let texts = match p.season_texts(show, season, lang) {
            Ok(t) => t,
            Err(Error::NotFound(_)) => Default::default(),
            Err(e) => {
                if keep_going(e, format!("tv {show} season {season} in {lang}"), report)? {
                    continue;
                }
                return Ok(());
            }
        };
        let tx = db::write_tx(c)?;
        let missing: Vec<(Id, i32)> = tx
            .prepare_cached(
                "SELECT e.id, e.episode FROM items e \
                 JOIN items s ON s.id = e.parent_id AND s.kind = 'season' \
                 JOIN items sh ON sh.id = s.parent_id AND sh.kind = 'show' \
                 WHERE e.kind = 'episode' AND sh.tmdb_id = ?1 AND e.season = ?2 \
                   AND e.episode IS NOT NULL \
                   AND NOT EXISTS (SELECT 1 FROM item_texts t WHERE t.item_id = e.id AND t.lang = ?3)",
            )?
            .query_map(params![show, season, lang], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut done = Vec::new();
        for (id, episode) in missing {
            let (title, overview) = texts
                .episodes
                .iter()
                .find(|(n, _, _)| *n == episode)
                .map(|(_, t, o)| (t.as_str(), o.as_str()))
                .unwrap_or_default();
            store_episode_text(&tx, id, lang, title, overview)?;
            done.push(id);
        }
        tx.commit()?;
        for id in done {
            report.translated += 1;
            on(id);
        }
    }
    Ok(())
}

/// Whether every matched movie, show and episode has its text in `lang`.
pub fn complete(c: &Connection, lang: &str) -> Result<bool> {
    if lang == lang::BASE {
        return Ok(true);
    }
    let missing: bool = c
        .prepare_cached(
            "SELECT EXISTS (SELECT 1 FROM items i \
             WHERE i.tmdb_id IS NOT NULL AND i.kind IN ('movie', 'show') \
               AND NOT EXISTS (SELECT 1 FROM item_texts t WHERE t.item_id = i.id AND t.lang = ?1)) \
             OR EXISTS (SELECT 1 FROM items e \
             JOIN items s ON s.id = e.parent_id AND s.kind = 'season' \
             JOIN items sh ON sh.id = s.parent_id AND sh.kind = 'show' AND sh.tmdb_id IS NOT NULL \
             WHERE e.kind = 'episode' AND e.season IS NOT NULL AND e.episode IS NOT NULL \
               AND NOT EXISTS (SELECT 1 FROM item_texts t WHERE t.item_id = e.id AND t.lang = ?1))",
        )?
        .query_row([lang], |r| r.get(0))?;
    Ok(!missing)
}
