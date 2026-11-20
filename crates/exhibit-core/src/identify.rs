//! Identification: turns the scanner's provisional items (one `Other`/`Unmatched` item per
//! file) into movies and show/season/episode trees, from `.nfo` sidecars, the parsed file
//! name and TMDB lookups. Anything uncertain goes to needs review instead of a guess.
//!
//! Network calls happen before a transaction opens; everything one item (or one show group)
//! changes is written in a single transaction, so a provider error or a crash leaves that
//! item exactly as it was. The show path lives in `identify_show`.

use crate::db;
use crate::error::{Error, Result};
use crate::identify_show::{self, Member};
use crate::matcher::{self, Evidence};
use crate::model::{Candidate, Id, ItemKind, MatchSource, MatchState, Person, TechInfo};
use crate::nfo::{self, ExternalId, NfoInfo};
use crate::parse::{self, KindHint};
use crate::rows::{ITEM_COLUMNS, enum_str, item_from_row};
use crate::scan::{join_rel, resolve, source_rows};
use crate::tmdb::{MetadataProvider, MovieMeta, SearchHit};
use crate::volume::Volumes;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Search hits scored per lookup.
pub(crate) const MAX_HITS: usize = 5;
/// Of those, how many get a `movie()` call for their runtime.
const RUNTIME_LOOKUPS: usize = 3;
const POSTER_SIZE: &str = "w780";
/// The small poster, for walls of many works; stored beside the large one with `-s`.
const POSTER_SMALL_SIZE: &str = "w342";
const BACKDROP_SIZE: &str = "original";
/// Candidate posters downloaded at once by `lookup`, which the owner waits on.
const LOOKUP_POSTER_THREADS: usize = 5;
/// An episode's still, kept as the episode's backdrop: it is the episode's frame on the show
/// screen and its backdrop on its details screen, so it is taken at full size like a backdrop.
pub(crate) const STILL_SIZE: &str = "original";

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentifyScope {
    Unmatched,
    /// Only items waiting for review, for example once probing has given them a duration.
    Review,
    UnmatchedAndReview,
    Item(Id),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct IdentifyReport {
    pub matched: u32,
    pub needs_review: u32,
    pub failed: u32,
    pub errors: Vec<String>,
}

/// A provisional item waiting to be identified, seen through one of its files.
pub(crate) struct Pending {
    pub(crate) item_id: Id,
    pub(crate) rel_path: String,
    /// The file on disk, when its source is online.
    abs: Option<PathBuf>,
    duration_min: Option<f64>,
    pub(crate) added_at: i64,
}

/// Where a pending item goes after its sidecar and name are read. The nfo routes keep the
/// title and year, to match by name when TMDB does not know the nfo's id.
enum Route {
    Movie {
        title: String,
        year: Option<i32>,
    },
    NfoMovie {
        id: i64,
        title: String,
        year: Option<i32>,
    },
    Episode {
        title: String,
        year: Option<i32>,
        show_id: Option<i64>,
        season: i32,
        episode: Option<i32>,
    },
    /// An `.nfo` names a show but the file name has no episode number.
    ShowWithoutEpisode {
        id: i64,
        title: String,
        year: Option<i32>,
    },
}

/// How an item was matched, written alongside its metadata.
#[derive(Clone, Copy)]
pub(crate) struct How {
    pub(crate) state: MatchState,
    pub(crate) source: MatchSource,
    pub(crate) confidence: Option<f32>,
}

impl How {
    /// From an `.nfo`, the owner or the agent: no scoring. Only the owner's choice is `Manual`.
    pub(crate) fn certain(source: MatchSource) -> How {
        let state = match source {
            MatchSource::Manual => MatchState::Manual,
            _ => MatchState::Matched,
        };
        How {
            state,
            source,
            confidence: Some(1.0),
        }
    }

    pub(crate) fn lookup(score: f32) -> How {
        How {
            state: MatchState::Matched,
            source: MatchSource::Lookup,
            confidence: Some(score),
        }
    }

    /// Matched by lookup without a score of its own: an episode file that joined the group of
    /// an nfo show id.
    pub(crate) fn joined() -> How {
        How {
            state: MatchState::Matched,
            source: MatchSource::Lookup,
            confidence: None,
        }
    }
}

/// (matched, needs review) item counts of one step.
pub(crate) type Counts = (u32, u32);

pub fn identify(
    c: &Connection,
    v: &dyn Volumes,
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    scope: IdentifyScope,
    on: &mut dyn FnMut(Id),
) -> Result<IdentifyReport> {
    let mut report = IdentifyReport::default();
    small_posters(c, p, cache_dir, &mut report)?;
    let mut posters = CandidatePosters::new(p, cache_dir);
    let mut episodes: Vec<Member> = Vec::new();
    for item in pending(c, v, &scope)? {
        let outcome = match route(p, &item) {
            Ok(Route::Episode {
                title,
                year,
                show_id,
                season,
                episode,
            }) => {
                episodes.push(identify_show::member(
                    item, title, year, show_id, season, episode,
                ));
                continue;
            }
            Ok(route) => identify_single(c, p, cache_dir, &mut posters, &item, route),
            Err(e) => Err(e),
        };
        record(&mut report, &item.rel_path, outcome, 1)?;
        on(item.item_id);
    }
    for g in &identify_show::groups(episodes) {
        let outcome = identify_show::identify_group(c, p, cache_dir, &mut posters, g);
        record(&mut report, &g.title, outcome, g.members.len() as u32)?;
        for m in &g.members {
            on(m.pending.item_id);
        }
    }
    report.errors.append(&mut posters.errors);
    identify_show::episode_stills(c, p, cache_dir, &mut report)?;
    Ok(report)
}

/// Adds an outcome to the report. A provider error (or something TMDB does not have) is
/// counted against the items and the run continues; any other error (the database, a
/// rejected key) ends the run.
fn record(
    report: &mut IdentifyReport,
    label: &str,
    outcome: Result<Counts>,
    items: u32,
) -> Result<()> {
    match outcome {
        Ok((matched, review)) => {
            report.matched += matched;
            report.needs_review += review;
        }
        Err(e @ (Error::Provider(_) | Error::NotFound(_))) => {
            report.failed += items;
            report.errors.push(format!("{label}: {e}"));
        }
        Err(e) => return Err(e),
    }
    Ok(())
}

/// Provisional items in scope, each seen through its first file on an online source (or its
/// first file, if all are offline). Matched and `Manual` items are never selected;
/// naming one with `IdentifyScope::Item` is `Error::Invalid`.
fn pending(c: &Connection, v: &dyn Volumes, scope: &IdentifyScope) -> Result<Vec<Pending>> {
    let (unmatched, review, only) = match scope {
        IdentifyScope::Unmatched => (true, false, None),
        IdentifyScope::Review => (false, true, None),
        IdentifyScope::UnmatchedAndReview => (true, true, None),
        IdentifyScope::Item(id) => (true, true, Some(*id)),
    };
    if let Some(id) = only {
        let state: String = c
            .prepare_cached("SELECT match_state FROM items WHERE id=?1")?
            .query_row([id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("item {id}")))?;
        if state == "matched" || state == "manual" {
            return Err(Error::Invalid(format!(
                "item {id} is already identified; change it with \
                 `exhibit review match {id} <tmdb-id> --kind movie|show` \
                 or `exhibit review other {id}`"
            )));
        }
    }
    let roots: HashMap<Id, PathBuf> = source_rows(c)?
        .iter()
        .filter_map(|s| Some((s.id, resolve(v, s).filter(|p| p.is_dir())?)))
        .collect();
    let mut s = c.prepare_cached(
        "SELECT i.id, i.added_at, f.source_id, f.rel_path, f.tech \
         FROM items i JOIN files f ON f.item_id = i.id \
         WHERE i.kind = 'other' AND (?3 IS NULL OR i.id = ?3) \
           AND ((?1 AND i.match_state = 'unmatched') OR (?2 AND i.match_state = 'needsreview')) \
         ORDER BY i.id, f.id",
    )?;
    let rows = s.query_map(params![unmatched, review, only], |r| {
        Ok((
            r.get::<_, Id>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, Id>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?,
        ))
    })?;
    let mut out: Vec<Pending> = Vec::new();
    for row in rows {
        let (item_id, added_at, source_id, rel_path, tech) = row?;
        let abs = roots.get(&source_id).map(|root| join_rel(root, &rel_path));
        let duration_min = tech
            .and_then(|t| serde_json::from_str::<TechInfo>(&t).ok())
            .map(|t| t.duration_s / 60.0)
            .filter(|d| *d > 0.0);
        let next = Pending {
            item_id,
            rel_path,
            abs,
            duration_min,
            added_at,
        };
        match out.last_mut() {
            Some(last) if last.item_id == item_id => {
                if last.abs.is_none() && next.abs.is_some() {
                    *last = next;
                }
            }
            _ => out.push(next),
        }
    }
    Ok(out)
}

fn route(p: &dyn MetadataProvider, item: &Pending) -> Result<Route> {
    let parsed = parse::parse_path(&item.rel_path);
    let episodic = parsed.hint == KindHint::Episode;
    let (mut title, mut year, mut target) = (parsed.title, parsed.year, None);
    if let Some(side) = item.abs.as_deref().and_then(sidecar) {
        target = nfo_target(p, &side.info, side.show_level || episodic)?;
        // A show's nfo names the show, so it speaks only for episode files; a movie or
        // scene nfo only for files that are not episodes.
        if side.show_level == episodic
            && let Some(t) = side.info.title
        {
            title = t;
            year = side.info.year.or(year);
        }
    }
    // In an episode's context the nfo id is always a show's.
    Ok(match (target, episodic) {
        (Some((id, ItemKind::Movie)), _) => Route::NfoMovie { id, title, year },
        (Some((id, _)), false) => Route::ShowWithoutEpisode { id, title, year },
        (None, false) => Route::Movie { title, year },
        (target, true) => Route::Episode {
            title,
            year,
            show_id: target.map(|(id, _)| id),
            // Absolute numbering (no season) maps to season 1.
            season: parsed.season.unwrap_or(1),
            episode: parsed.episode,
        },
    })
}

struct Sidecar {
    info: NfoInfo,
    /// Read from `tvshow.nfo`: its ids and title are the show's.
    show_level: bool,
}

/// The nfo that speaks for `file`. An episode's own nfo (`<episodedetails>`) carries the
/// episode's ids and title, not the show's, so it is passed over for `tvshow.nfo`.
fn sidecar(file: &Path) -> Option<Sidecar> {
    let found = nfo::find_nfo(file)?;
    let show_level = found
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("tvshow.nfo"));
    let info = nfo::read_nfo_file(&found)?;
    if show_level || !info.is_show {
        return Some(Sidecar { info, show_level });
    }
    let tvshow = file
        .parent()?
        .ancestors()
        .take(3)
        .map(|d| d.join("tvshow.nfo"))
        .find(|p| p.is_file())?;
    Some(Sidecar {
        info: nfo::read_nfo_file(&tvshow)?,
        show_level: true,
    })
}

/// The first id in the nfo that resolves in this context, as (TMDB id, Movie or Show). A
/// TMDB id is never tried as the other kind: in a show context (a show nfo, or an episode
/// file) it is the show's id; in a movie context it is the movie's id, checked by `movie()`
/// later, and a TV id is ignored. IMDb and TVDB results of the wrong kind, or that TMDB does
/// not know, are ignored too.
fn nfo_target(
    p: &dyn MetadataProvider,
    info: &NfoInfo,
    as_show: bool,
) -> Result<Option<(i64, ItemKind)>> {
    let wanted = if as_show {
        ItemKind::Show
    } else {
        ItemKind::Movie
    };
    for id in &info.ids {
        let target = match (id, as_show) {
            (ExternalId::Tmdb(n) | ExternalId::TmdbTv(n), true) => Some((*n, wanted)),
            (ExternalId::Tmdb(n), false) => Some((*n, wanted)),
            (ExternalId::TmdbTv(_), false) => None,
            _ => match p.find_external(id) {
                Ok(hit) => hit
                    .filter(|hit| hit.kind == wanted)
                    .map(|hit| (hit.tmdb_id, hit.kind)),
                Err(Error::NotFound(_)) => None,
                Err(e) => return Err(e),
            },
        };
        if target.is_some() {
            return Ok(target);
        }
    }
    Ok(None)
}

fn identify_single(
    c: &Connection,
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    posters: &mut CandidatePosters,
    item: &Pending,
    route: Route,
) -> Result<Counts> {
    match route {
        Route::Movie { title, year } => match_movie(c, p, cache_dir, posters, item, &title, year),
        // A stale or wrong-kind nfo id is ignored: the item is matched by name instead.
        Route::NfoMovie { id, title, year } => match p.movie(id) {
            Ok(meta) => {
                let how = How::certain(MatchSource::Nfo);
                save_movie(c, p, cache_dir, item.item_id, &meta, how)?;
                Ok((1, 0))
            }
            Err(Error::NotFound(_)) => match_movie(c, p, cache_dir, posters, item, &title, year),
            Err(e) => Err(e),
        },
        Route::ShowWithoutEpisode { id, title, year } => match p.show(id) {
            Ok(s) => {
                let only = Candidate {
                    poster: posters.poster(ItemKind::Show, s.tmdb_id, s.poster.as_deref())?,
                    tmdb_id: s.tmdb_id,
                    kind: ItemKind::Show,
                    title: s.title,
                    year: s.year,
                    score: 1.0,
                };
                set_review(c, item.item_id, &[only])?;
                Ok((0, 1))
            }
            Err(Error::NotFound(_)) => match_movie(c, p, cache_dir, posters, item, &title, year),
            Err(e) => Err(e),
        },
        Route::Episode { .. } => unreachable!("episodes are identified in groups"),
    }
}

fn match_movie(
    c: &Connection,
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    posters: &mut CandidatePosters,
    item: &Pending,
    title: &str,
    year: Option<i32>,
) -> Result<Counts> {
    let ev = Evidence {
        title,
        year,
        duration_min: item.duration_min,
    };
    let hits = p.search_movie(title, year)?;
    let mut metas: Vec<MovieMeta> = Vec::new();
    let mut scored = Vec::new();
    for (i, hit) in hits.iter().take(MAX_HITS).enumerate() {
        // A runtime only counts against a probed duration; without one, skip the call.
        let runtime = if i < RUNTIME_LOOKUPS && ev.duration_min.is_some() {
            let m = p.movie(hit.tmdb_id)?;
            let r = m.runtime_min;
            metas.push(m);
            r
        } else {
            None
        };
        scored.push(candidate(hit, matcher::score(&ev, hit, runtime)));
    }
    sort_candidates(&mut scored);
    let Some(best) = matcher::decide(&scored) else {
        posters.add(&mut scored, &hits)?;
        set_review(c, item.item_id, &scored)?;
        return Ok((0, 1));
    };
    let meta = match metas.into_iter().find(|m| m.tmdb_id == best.tmdb_id) {
        Some(m) => m,
        None => p.movie(best.tmdb_id)?,
    };
    save_movie(
        c,
        p,
        cache_dir,
        item.item_id,
        &meta,
        How::lookup(best.score),
    )?;
    Ok((1, 0))
}

pub(crate) fn candidate(hit: &SearchHit, score: f32) -> Candidate {
    Candidate {
        tmdb_id: hit.tmdb_id,
        kind: hit.kind,
        title: hit.title.clone(),
        year: hit.year,
        score,
        poster: None,
    }
}

/// Best first.
pub(crate) fn sort_candidates(scored: &mut [Candidate]) {
    scored.sort_by(|a, b| b.score.total_cmp(&a.score));
}

/// Makes `item_id` the movie `m`; when another movie item already has that TMDB id, moves
/// the files there instead (a second version) and deletes `item_id`.
fn save_movie(
    c: &Connection,
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    item_id: Id,
    m: &MovieMeta,
    how: How,
) -> Result<()> {
    let existing = find_item(c, ItemKind::Movie, m.tmdb_id)?.filter(|id| *id != item_id);
    let (poster, backdrop) = match existing {
        Some(_) => (None, None),
        None => artwork(
            p,
            cache_dir,
            "movie",
            m.tmdb_id,
            m.poster.as_deref(),
            m.backdrop.as_deref(),
        ),
    };
    let tx = db::write_tx(c)?;
    let target = match existing {
        Some(id) => {
            confirm(&tx, id, how)?;
            id
        }
        None => {
            let fill = Fill {
                year: m.year,
                overview: &m.overview,
                runtime_min: m.runtime_min,
                genres: &m.genres,
                people: &m.people,
                poster,
                backdrop,
                tmdb_id: Some(m.tmdb_id),
                ..Fill::new(ItemKind::Movie, &m.title)
            };
            let (kind, added_at): (String, i64) = tx.query_row(
                "SELECT kind, added_at FROM items WHERE id=?1",
                [item_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            // Provisional and movie items become the movie; an episode hands its files over.
            if kind == "other" || kind == "movie" {
                fill_item(&tx, item_id, &fill, how)?;
                item_id
            } else {
                insert_item(&tx, &fill, how, added_at)?
            }
        }
    };
    absorb(&tx, item_id, target)?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn find_item(c: &Connection, kind: ItemKind, tmdb_id: i64) -> Result<Option<Id>> {
    Ok(
        c.prepare_cached("SELECT id FROM items WHERE kind=?1 AND tmdb_id=?2 ORDER BY id LIMIT 1")?
            .query_row(params![enum_str(kind), tmdb_id], |r| r.get(0))
            .optional()?,
    )
}

/// Moves every file of `from` to `into` and deletes `from`, keeping what the owner did with
/// it: play history moves over, the newer resume state wins and `loved` carries over.
pub(crate) fn absorb(c: &Connection, from: Id, into: Id) -> Result<()> {
    if from != into {
        for sql in [
            "UPDATE files SET item_id=?2 WHERE item_id=?1",
            "UPDATE play_log SET item_id=?2 WHERE item_id=?1",
            "INSERT INTO play_state(item_id, position_s, duration_s, watched, last_played_at) \
             SELECT ?2, position_s, duration_s, watched, last_played_at FROM play_state \
             WHERE item_id=?1 \
             ON CONFLICT(item_id) DO UPDATE SET position_s=excluded.position_s, \
             duration_s=excluded.duration_s, watched=excluded.watched, \
             last_played_at=excluded.last_played_at \
             WHERE excluded.last_played_at > play_state.last_played_at",
            "UPDATE items SET loved = 1 WHERE id=?2 AND (SELECT loved FROM items WHERE id=?1)",
        ] {
            c.prepare_cached(sql)?.execute([from, into])?;
        }
        c.prepare_cached("DELETE FROM items WHERE id=?1")?
            .execute([from])?;
    }
    Ok(())
}

pub(crate) fn set_review(c: &Connection, item_id: Id, scored: &[Candidate]) -> Result<()> {
    let score = scored.first().map(|c| f64::from(c.score));
    c.prepare_cached(
        "UPDATE items SET match_state='needsreview', candidates=?2, confidence=?3 WHERE id=?1",
    )?
    .execute(params![item_id, json(scored), score])?;
    Ok(())
}

/// When the owner's choice lands on an item that already exists, that item becomes
/// `Manual` too, so the choice is permanent.
pub(crate) fn confirm(c: &Connection, id: Id, how: How) -> Result<()> {
    if how.state == MatchState::Manual {
        c.prepare_cached("UPDATE items SET match_state=?2, match_source=?3 WHERE id=?1")?
            .execute(params![id, enum_str(how.state), enum_str(how.source)])?;
    }
    Ok(())
}

/// Metadata written to an item.
pub(crate) struct Fill<'a> {
    pub(crate) kind: ItemKind,
    pub(crate) parent_id: Option<Id>,
    pub(crate) title: &'a str,
    pub(crate) year: Option<i32>,
    pub(crate) season: Option<i32>,
    pub(crate) episode: Option<i32>,
    pub(crate) overview: &'a str,
    pub(crate) runtime_min: Option<i32>,
    pub(crate) genres: &'a [String],
    pub(crate) people: &'a [Person],
    pub(crate) poster: Option<String>,
    pub(crate) backdrop: Option<String>,
    pub(crate) tmdb_id: Option<i64>,
}

impl<'a> Fill<'a> {
    pub(crate) fn new(kind: ItemKind, title: &'a str) -> Fill<'a> {
        Fill {
            kind,
            parent_id: None,
            title,
            year: None,
            season: None,
            episode: None,
            overview: "",
            runtime_min: None,
            genres: &[],
            people: &[],
            poster: None,
            backdrop: None,
            tmdb_id: None,
        }
    }
}

pub(crate) fn insert_item(c: &Connection, f: &Fill, how: How, added_at: i64) -> Result<Id> {
    c.prepare_cached("INSERT INTO items(kind, title, added_at) VALUES (?1, '', ?2)")?
        .execute(params![enum_str(f.kind), added_at])?;
    let id = c.last_insert_rowid();
    fill_item(c, id, f, how)?;
    Ok(id)
}

/// Overwrites every metadata and match column of `id` and clears its candidates.
fn fill_item(c: &Connection, id: Id, f: &Fill, how: How) -> Result<()> {
    c.prepare_cached(
        "UPDATE items SET kind=?2, parent_id=?3, title=?4, year=?5, season=?6, episode=?7, \
         overview=?8, runtime_min=?9, genres=?10, people=?11, poster=?12, backdrop=?13, \
         tmdb_id=?14, match_state=?15, match_source=?16, confidence=?17, candidates='[]' \
         WHERE id=?1",
    )?
    .execute(params![
        id,
        enum_str(f.kind),
        f.parent_id,
        f.title,
        f.year,
        f.season,
        f.episode,
        (!f.overview.is_empty()).then_some(f.overview),
        f.runtime_min,
        json(f.genres),
        json(f.people),
        f.poster,
        f.backdrop,
        f.tmdb_id,
        enum_str(how.state),
        enum_str(how.source),
        how.confidence.map(f64::from),
    ])?;
    Ok(())
}

/// JSON for the array columns; the FTS triggers need valid JSON there.
pub(crate) fn json<T: Serialize + ?Sized>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "[]".into())
}

/// Downloads poster and backdrop into `cache_dir` as `<kind>-<id>-poster.jpg` and
/// `<kind>-<id>-backdrop.jpg` and returns the file names; the poster is also saved small,
/// as `<kind>-<id>-poster-s.jpg`. A file already cached is reused; a failed download gives
/// `None` and never fails the match.
pub(crate) fn artwork(
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    kind: &str,
    id: i64,
    poster: Option<&str>,
    backdrop: Option<&str>,
) -> (Option<String>, Option<String>) {
    let get = |path: Option<&str>, size: &str, name: String| -> Option<String> {
        download(p, cache_dir, path?, size, &name).ok()?;
        Some(name)
    };
    let large = get(poster, POSTER_SIZE, poster_name(kind, id));
    if let Some(name) = &large {
        get(poster, POSTER_SMALL_SIZE, small_poster(name));
    }
    (
        large,
        get(backdrop, BACKDROP_SIZE, format!("{kind}-{id}-backdrop.jpg")),
    )
}

/// The cached poster of TMDB movie or show `id`: `movie-603-poster.jpg`, `tv-95396-poster.jpg`.
fn poster_name(tag: &str, id: i64) -> String {
    format!("{tag}-{id}-poster.jpg")
}

/// Small posters for match candidates, so the owner can judge them by sight. Each is saved
/// under the name the item's own poster takes if that candidate is applied; the large one
/// comes only then. After the first provider error (TMDB out of reach) the rest are taken
/// only from the cache, so an outage costs one timeout rather than one per candidate.
pub(crate) struct CandidatePosters<'a> {
    p: &'a dyn MetadataProvider,
    cache_dir: &'a Path,
    unreachable: bool,
    /// Every failed download, for the report.
    pub(crate) errors: Vec<String>,
}

impl<'a> CandidatePosters<'a> {
    pub(crate) fn new(p: &'a dyn MetadataProvider, cache_dir: &'a Path) -> Self {
        CandidatePosters {
            p,
            cache_dir,
            unreachable: false,
            errors: Vec::new(),
        }
    }

    /// Gives each candidate the poster of its search hit. Only a rejected key is an error.
    pub(crate) fn add(&mut self, scored: &mut [Candidate], hits: &[SearchHit]) -> Result<()> {
        for c in scored {
            let path = hits
                .iter()
                .find(|h| h.tmdb_id == c.tmdb_id && h.kind == c.kind)
                .and_then(|h| h.poster.as_deref());
            c.poster = self.poster(c.kind, c.tmdb_id, path)?;
        }
        Ok(())
    }

    /// The poster's file name once its small file is on disk; `None` when TMDB has no poster
    /// (`path`) or the download failed. Only a rejected key is an error.
    pub(crate) fn poster(
        &mut self,
        kind: ItemKind,
        id: i64,
        path: Option<&str>,
    ) -> Result<Option<String>> {
        let Some(path) = path else {
            return Ok(None);
        };
        let tag = if kind == ItemKind::Show {
            "tv"
        } else {
            "movie"
        };
        let name = poster_name(tag, id);
        let small = small_poster(&name);
        if self.unreachable && !self.cache_dir.join(&small).is_file() {
            return Ok(None);
        }
        match download(self.p, self.cache_dir, path, POSTER_SMALL_SIZE, &small) {
            Ok(()) => Ok(Some(name)),
            Err(e @ Error::Invalid(_)) => Err(e),
            Err(e) => {
                self.unreachable |= matches!(e, Error::Provider(_));
                self.errors.push(format!("poster for {small}: {e}"));
                Ok(None)
            }
        }
    }
}

/// The small poster's file name for a large poster's: `-s` before the extension.
fn small_poster(large: &str) -> String {
    match large.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}-s.{ext}"),
        None => format!("{large}-s"),
    }
}

/// Saves TMDB image `path` at `size` as `cache_dir/name`, unless that file exists.
pub(crate) fn download(
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    path: &str,
    size: &str,
    name: &str,
) -> Result<()> {
    let dest = cache_dir.join(name);
    if !dest.is_file() {
        let bytes = p.image(path, size)?;
        std::fs::create_dir_all(cache_dir)?;
        // Written aside and renamed, so a cut-off download is never taken as cached.
        let part = cache_dir.join(format!("{name}.part"));
        std::fs::write(&part, bytes)?;
        std::fs::rename(&part, &dest)?;
    }
    Ok(())
}

/// Downloads the small poster of every movie and show that has a large one but no small
/// file yet (items identified before small posters existed). A failure is added to the
/// report's errors and the run goes on; a rejected key ends it. After the first provider
/// error (TMDB out of reach) the rest wait for the next run, so an outage costs one timeout.
fn small_posters(
    c: &Connection,
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    report: &mut IdentifyReport,
) -> Result<()> {
    let missing: Vec<_> = c
        .prepare_cached(
            "SELECT kind, tmdb_id, poster FROM items \
             WHERE kind IN ('movie', 'show') AND tmdb_id IS NOT NULL AND poster IS NOT NULL \
             ORDER BY id",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<Vec<(String, i64, String)>>>()?
        .into_iter()
        .filter(|(_, _, poster)| !cache_dir.join(small_poster(poster)).is_file())
        .collect();
    for (kind, tmdb_id, poster) in missing {
        let path = if kind == "movie" {
            p.movie(tmdb_id).map(|m| m.poster)
        } else {
            p.show(tmdb_id).map(|s| s.poster)
        };
        let saved = path.and_then(|path| match path {
            Some(path) => download(
                p,
                cache_dir,
                &path,
                POSTER_SMALL_SIZE,
                &small_poster(&poster),
            ),
            None => Ok(()),
        });
        match saved {
            Ok(()) => {}
            Err(e @ Error::Invalid(_)) => return Err(e),
            Err(e) => {
                let unreachable = matches!(e, Error::Provider(_));
                report
                    .errors
                    .push(format!("small poster for {poster}: {e}"));
                if unreachable {
                    break;
                }
            }
        }
    }
    Ok(())
}

/// TMDB search for a match picked by hand: every hit scored as the matcher scores it
/// (title, and year when given), best first, each with its small poster when it could be
/// cached into `cache_dir`. `kind` must be `Movie` or `Show`.
pub fn lookup(
    p: &(dyn MetadataProvider + Sync),
    cache_dir: &Path,
    text: &str,
    kind: ItemKind,
    year: Option<i32>,
) -> Result<Vec<Candidate>> {
    let hits = match kind {
        ItemKind::Movie => p.search_movie(text, year)?,
        ItemKind::Show => p.search_show(text, year)?,
        other => {
            return Err(Error::Invalid(format!(
                "cannot search for a {}; use movie or show",
                enum_str(other)
            )));
        }
    };
    let ev = Evidence {
        title: text,
        year,
        duration_min: None,
    };
    let mut scored: Vec<Candidate> = hits
        .iter()
        .map(|hit| candidate(hit, matcher::score(&ev, hit, None)))
        .collect();
    sort_candidates(&mut scored);
    // The owner is waiting: posters come a few at a time, each thread over its share.
    let share = scored.len().div_ceil(LOOKUP_POSTER_THREADS).max(1);
    let hits = &hits;
    std::thread::scope(|s| {
        let jobs: Vec<_> = scored
            .chunks_mut(share)
            .map(|chunk| s.spawn(move || CandidatePosters::new(p, cache_dir).add(chunk, hits)))
            .collect();
        jobs.into_iter()
            .try_for_each(|job| job.join().expect("a poster download panicked"))
    })?;
    Ok(scored)
}

/// Matches `item_id` to TMDB `tmdb_id` as a movie or a show; for a show, the item's file
/// name gives the season and episode. `MatchSource::Manual` makes the match permanent.
pub fn apply_match(
    c: &Connection,
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    item_id: Id,
    tmdb_id: i64,
    kind: ItemKind,
    source: MatchSource,
) -> Result<()> {
    let item = c
        .prepare_cached(&format!("SELECT {ITEM_COLUMNS} FROM items WHERE id=?1"))?
        .query_row([item_id], item_from_row)
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("item {item_id}")))?;
    if matches!(item.kind, ItemKind::Show | ItemKind::Season) {
        return Err(Error::Invalid(format!(
            "item {item_id} is a {}; a match applies to a movie, an episode or an unidentified item",
            enum_str(item.kind)
        )));
    }
    let how = How::certain(source);
    match kind {
        ItemKind::Movie => save_movie(c, p, cache_dir, item_id, &p.movie(tmdb_id)?, how),
        ItemKind::Show => identify_show::apply(c, p, cache_dir, item_id, tmdb_id, how),
        other => Err(Error::Invalid(format!(
            "cannot match to a {}; use movie or show",
            enum_str(other)
        ))),
    }
}

/// Makes the item a plain `Other` item under its cleaned file name, permanently (`Manual`).
pub fn move_to_other(c: &Connection, item_id: Id) -> Result<()> {
    let kind: String = c
        .prepare_cached("SELECT kind FROM items WHERE id=?1")?
        .query_row([item_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("item {item_id}")))?;
    if kind == "show" || kind == "season" {
        return Err(Error::Invalid(format!(
            "item {item_id} is a {kind}; only items with files can move to Other"
        )));
    }
    let title: Option<String> = c
        .prepare_cached("SELECT rel_path FROM files WHERE item_id=?1 ORDER BY id LIMIT 1")?
        .query_row([item_id], |r| r.get::<_, String>(0))
        .optional()?
        .map(|rel| parse::parse_path(&rel).title);
    c.prepare_cached(
        "UPDATE items SET kind='other', parent_id=NULL, title=coalesce(?2, title), year=NULL, \
         season=NULL, episode=NULL, overview=NULL, runtime_min=NULL, genres='[]', \
         people='[]', poster=NULL, backdrop=NULL, tmdb_id=NULL, match_state='manual', \
         match_source='manual', confidence=NULL, candidates='[]' WHERE id=?1",
    )?
    .execute(params![item_id, title])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::matcher::ACCEPT_SCORE;
    use crate::model::{Item, MatchState, Person};
    use crate::nfo::ExternalId;
    use crate::parse::normalize_title;
    use crate::rows::{ITEM_COLUMNS, item_from_row};
    use crate::scan::{add_source, scan_source};
    use crate::tmdb::{EpisodeMeta, MovieMeta, SearchHit, SeasonMeta, ShowMeta};
    use crate::volume::FakeVolumes;
    use ItemKind::{Episode, Movie, Other, Show};
    use std::cell::RefCell;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use tempfile::TempDir;

    /// In-memory TMDB. Every call is logged; a call whose log line contains a string in
    /// `fail` returns a provider error.
    #[derive(Default)]
    struct Fake {
        movies: Vec<MovieMeta>,
        shows: Vec<ShowMeta>,
        seasons: Vec<(i64, SeasonMeta)>,
        movie_hits: Vec<(&'static str, Vec<SearchHit>)>,
        show_hits: Vec<(&'static str, Vec<SearchHit>)>,
        images_fail: bool,
        fail: Mutex<Vec<String>>,
        calls: Mutex<Vec<String>>,
    }

    impl Fake {
        fn log(&self, call: String) -> Result<()> {
            let refused = self
                .fail
                .lock()
                .unwrap()
                .iter()
                .any(|f| call.contains(f.as_str()));
            self.calls.lock().unwrap().push(call.clone());
            if refused {
                return Err(Error::Provider(format!("refused: {call}")));
            }
            Ok(())
        }

        fn count(&self, prefix: &str) -> usize {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|c| c.starts_with(prefix))
                .count()
        }
    }

    fn hits_for(table: &[(&str, Vec<SearchHit>)], title: &str) -> Vec<SearchHit> {
        table
            .iter()
            .find(|(k, _)| normalize_title(k) == normalize_title(title))
            .map(|(_, h)| h.clone())
            .unwrap_or_default()
    }

    /// What the TMDB client returns for HTTP 404.
    fn not_found(what: String) -> Error {
        Error::NotFound(format!("TMDB has nothing at {what}"))
    }

    impl MetadataProvider for Fake {
        fn search_movie(&self, title: &str, _year: Option<i32>) -> Result<Vec<SearchHit>> {
            self.log(format!("search_movie {title}"))?;
            Ok(hits_for(&self.movie_hits, title))
        }

        fn search_show(&self, title: &str, year: Option<i32>) -> Result<Vec<SearchHit>> {
            self.log(format!("search_show {title} {year:?}"))?;
            Ok(hits_for(&self.show_hits, title))
        }

        fn find_external(&self, id: &ExternalId) -> Result<Option<SearchHit>> {
            self.log(format!("find {id:?}"))?;
            let ExternalId::Tmdb(n) = id else {
                return Ok(None);
            };
            let movie = self.movies.iter().find(|m| m.tmdb_id == *n);
            let show = self.shows.iter().find(|s| s.tmdb_id == *n);
            Ok(movie
                .map(|m| hit(m.tmdb_id, Movie, &m.title, m.year))
                .or_else(|| show.map(|s| hit(s.tmdb_id, Show, &s.title, s.year))))
        }

        fn movie(&self, id: i64) -> Result<MovieMeta> {
            self.log(format!("movie {id}"))?;
            let m = self.movies.iter().find(|m| m.tmdb_id == id);
            m.cloned().ok_or_else(|| not_found(format!("movie {id}")))
        }

        fn show(&self, id: i64) -> Result<ShowMeta> {
            self.log(format!("show {id}"))?;
            let s = self.shows.iter().find(|s| s.tmdb_id == id);
            s.cloned().ok_or_else(|| not_found(format!("show {id}")))
        }

        fn season(&self, show_id: i64, season: i32) -> Result<SeasonMeta> {
            self.log(format!("season {show_id} {season}"))?;
            let s = self
                .seasons
                .iter()
                .find(|(id, s)| *id == show_id && s.season == season);
            s.map(|(_, s)| s.clone())
                .ok_or_else(|| not_found(format!("season {show_id}/{season}")))
        }

        fn image(&self, path: &str, size: &str) -> Result<Vec<u8>> {
            self.log(format!("image {size}{path}"))?;
            if self.images_fail {
                return Err(Error::Provider("HTTP 500".into()));
            }
            Ok(format!("{size}{path}").into_bytes())
        }
    }

    fn hit(id: i64, kind: ItemKind, title: &str, year: Option<i32>) -> SearchHit {
        SearchHit {
            tmdb_id: id,
            kind,
            title: title.into(),
            original_title: title.into(),
            year,
            popularity: 1.0,
            poster: None,
        }
    }

    /// Two Solaris hits, each with a TMDB poster path: never accepted, always reviewed.
    fn solaris_with_posters() -> Fake {
        let with_poster = |id, year| SearchHit {
            poster: Some(format!("/p{id}.jpg")),
            ..hit(id, Movie, "Solaris", Some(year))
        };
        Fake {
            movie_hits: vec![(
                "Solaris",
                vec![with_poster(593, 1972), with_poster(2103, 2002)],
            )],
            ..Default::default()
        }
    }

    fn movie(id: i64, title: &str, year: i32, runtime: i32) -> MovieMeta {
        MovieMeta {
            tmdb_id: id,
            title: title.into(),
            year: Some(year),
            overview: format!("About {title}"),
            runtime_min: Some(runtime),
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

    fn show(id: i64, title: &str, year: i32, seasons: &[(i32, u32)]) -> ShowMeta {
        ShowMeta {
            tmdb_id: id,
            title: title.into(),
            year: Some(year),
            overview: format!("About {title}"),
            genres: vec!["Drama".into()],
            people: vec![],
            poster: Some(format!("/p{id}.jpg")),
            backdrop: Some(format!("/b{id}.jpg")),
            seasons: seasons.to_vec(),
        }
    }

    fn season(n: i32, episodes: i32) -> SeasonMeta {
        SeasonMeta {
            season: n,
            poster: None,
            episodes: (1..=episodes)
                .map(|e| EpisodeMeta {
                    episode: e,
                    title: format!("Episode {e} title"),
                    overview: String::new(),
                    runtime_min: Some(50),
                    still: Some(format!("/s{n}e{e}.jpg")),
                })
                .collect(),
        }
    }

    fn matrix() -> Fake {
        Fake {
            movies: vec![movie(603, "The Matrix", 1999, 136)],
            movie_hits: vec![(
                "The Matrix",
                vec![hit(603, Movie, "The Matrix", Some(1999))],
            )],
            ..Default::default()
        }
    }

    fn severance() -> Fake {
        Fake {
            shows: vec![show(95396, "Severance", 2022, &[(1, 9), (2, 10)])],
            seasons: vec![(95396, season(2, 10))],
            show_hits: vec![("Severance", vec![hit(95396, Show, "Severance", Some(2022))])],
            ..Default::default()
        }
    }

    struct Env {
        data: TempDir,
        drive: TempDir,
        c: Connection,
        v: FakeVolumes,
        src: Id,
    }

    impl Env {
        fn media(&self) -> PathBuf {
            self.drive.path().join("media")
        }

        fn cache(&self) -> PathBuf {
            self.data.path().join("cache").join("images")
        }
    }

    fn write(root: &Path, rel: &str, text: &str) {
        let p = rel.split('/').fold(root.to_path_buf(), |p, s| p.join(s));
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    /// A scanned source holding `files` (empty files under `<drive>/media`).
    fn env(files: &[&str]) -> Env {
        let data = tempfile::tempdir().unwrap();
        let drive = tempfile::tempdir().unwrap();
        let c = db::open(data.path()).unwrap();
        let media = drive.path().join("media");
        fs::create_dir_all(&media).unwrap();
        for f in files {
            write(&media, f, "");
        }
        let v = FakeVolumes(vec![("VOL1".into(), drive.path().to_path_buf())]);
        let src = add_source(&c, &v, &media).unwrap().id;
        scan_source(&c, &v, src, &mut |_| {}).unwrap();
        Env {
            data,
            drive,
            c,
            v,
            src,
        }
    }

    fn run(e: &Env, p: &Fake, scope: IdentifyScope) -> IdentifyReport {
        identify(&e.c, &e.v, p, &e.cache(), scope, &mut |_| {}).unwrap()
    }

    fn report(matched: u32, needs_review: u32, failed: u32) -> IdentifyReport {
        IdentifyReport {
            matched,
            needs_review,
            failed,
            errors: vec![],
        }
    }

    fn item(c: &Connection, id: Id) -> Item {
        c.query_row(
            &format!("SELECT {ITEM_COLUMNS} FROM items WHERE id=?1"),
            [id],
            item_from_row,
        )
        .unwrap()
    }

    fn file_item(e: &Env, rel: &str) -> Item {
        let id =
            e.c.query_row("SELECT item_id FROM files WHERE rel_path=?1", [rel], |r| {
                r.get(0)
            })
            .unwrap();
        item(&e.c, id)
    }

    fn items_of(c: &Connection, kind: &str) -> Vec<Item> {
        let mut s = c
            .prepare(&format!(
                "SELECT {ITEM_COLUMNS} FROM items WHERE kind=?1 ORDER BY season, episode, id"
            ))
            .unwrap();
        s.query_map([kind], item_from_row)
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    #[test]
    fn movie_matches_and_fills_metadata() {
        let rel = "Movies/The Matrix (1999)/The.Matrix.1999.1080p.mkv";
        let e = env(&[rel]);
        let p = matrix();
        let mut seen = vec![];
        let r = identify(
            &e.c,
            &e.v,
            &p,
            &e.cache(),
            IdentifyScope::Unmatched,
            &mut |id| seen.push(id),
        )
        .unwrap();
        assert_eq!(r, report(1, 0, 0));
        let it = file_item(&e, rel);
        assert_eq!(seen, vec![it.id]);
        assert_eq!(
            (it.kind, it.title.as_str(), it.year, it.tmdb_id),
            (Movie, "The Matrix", Some(1999), Some(603))
        );
        assert_eq!(
            (it.match_state, it.match_source),
            (MatchState::Matched, Some(MatchSource::Lookup))
        );
        assert!(it.confidence.unwrap() >= ACCEPT_SCORE);
        assert_eq!(it.runtime_min, Some(136));
        assert_eq!(it.overview.as_deref(), Some("About The Matrix"));
        assert_eq!(it.genres, ["Drama"]);
        assert_eq!(it.people.len(), 1);
        assert!(it.candidates.is_empty());
        assert_eq!(it.poster.as_deref(), Some("movie-603-poster.jpg"));
        assert_eq!(it.backdrop.as_deref(), Some("movie-603-backdrop.jpg"));
        let read = |n: &str| fs::read(e.cache().join(n)).unwrap();
        assert_eq!(read("movie-603-poster.jpg"), b"w780/p603.jpg");
        assert_eq!(read("movie-603-backdrop.jpg"), b"original/b603.jpg");
        // Nothing left to do on a second run.
        assert_eq!(
            run(&e, &p, IdentifyScope::UnmatchedAndReview),
            report(0, 0, 0)
        );
    }

    #[test]
    fn nfo_id_bypasses_scoring() {
        let e = env(&["Stuff/xq7_final.mkv"]);
        write(
            &e.media(),
            "Stuff/xq7_final.nfo",
            r#"<movie><uniqueid type="tmdb">603</uniqueid></movie>"#,
        );
        let p = Fake {
            movies: vec![movie(603, "The Matrix", 1999, 136)],
            ..Default::default()
        };
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(1, 0, 0));
        let it = file_item(&e, "Stuff/xq7_final.mkv");
        assert_eq!((it.kind, it.tmdb_id), (Movie, Some(603)));
        assert_eq!(
            (it.match_state, it.match_source, it.confidence),
            (MatchState::Matched, Some(MatchSource::Nfo), Some(1.0))
        );
        assert_eq!(p.count("search"), 0);
    }

    #[test]
    fn nfo_without_id_overrides_title() {
        let e = env(&["Stuff/xq7_final.mkv"]);
        write(
            &e.media(),
            "Stuff/xq7_final.nfo",
            "<movie><title>The Matrix</title><year>1999</year></movie>",
        );
        let p = matrix();
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(1, 0, 0));
        let it = file_item(&e, "Stuff/xq7_final.mkv");
        assert_eq!(
            (it.tmdb_id, it.match_source),
            (Some(603), Some(MatchSource::Lookup))
        );
        assert_eq!(
            *p.calls.lock().unwrap(),
            [
                "search_movie The Matrix",
                "movie 603",
                "image w780/p603.jpg",
                "image w342/p603.jpg",
                "image original/b603.jpg"
            ]
        );
    }

    #[test]
    fn offline_source_skips_nfo_and_matches_by_name() {
        let e = env(&["Stuff/The Matrix 1999.mkv"]);
        write(
            &e.media(),
            "Stuff/The Matrix 1999.nfo",
            r#"<movie><uniqueid type="tmdb">1</uniqueid></movie>"#,
        );
        let mut p = matrix();
        p.movies.push(movie(1, "Wrong", 2000, 90));
        let offline = FakeVolumes(vec![]);
        let r = identify(
            &e.c,
            &offline,
            &p,
            &e.cache(),
            IdentifyScope::Unmatched,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(r, report(1, 0, 0));
        let it = file_item(&e, "Stuff/The Matrix 1999.mkv");
        assert_eq!(
            (it.tmdb_id, it.match_source),
            (Some(603), Some(MatchSource::Lookup))
        );
        assert_eq!(p.count("find"), 0);
    }

    #[test]
    fn episodes_build_show_season_episode_tree() {
        let files = [
            "Severance/Season 02/Severance.S02E01.mkv",
            "Severance/Season 02/Severance.S02E02.mkv",
            "Severance/Season 02/Severance.S02E03.mkv",
        ];
        let e = env(&files);
        let p = severance();
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(3, 0, 0));

        let shows = items_of(&e.c, "show");
        assert_eq!(shows.len(), 1);
        let s = &shows[0];
        assert_eq!(
            (s.title.as_str(), s.year, s.tmdb_id),
            ("Severance", Some(2022), Some(95396))
        );
        assert_eq!(
            (s.match_state, s.match_source),
            (MatchState::Matched, Some(MatchSource::Lookup))
        );
        assert_eq!(s.poster.as_deref(), Some("tv-95396-poster.jpg"));

        let seasons = items_of(&e.c, "season");
        assert_eq!(seasons.len(), 1);
        assert_eq!(
            (seasons[0].parent_id, seasons[0].season),
            (Some(s.id), Some(2))
        );
        assert_eq!(seasons[0].title, "Season 2");

        let eps = items_of(&e.c, "episode");
        assert_eq!(eps.len(), 3);
        for (i, ep) in eps.iter().enumerate() {
            let n = i as i32 + 1;
            assert_eq!(ep.parent_id, Some(seasons[0].id));
            assert_eq!((ep.season, ep.episode), (Some(2), Some(n)));
            assert_eq!(ep.title, format!("Episode {n} title"));
            assert_eq!(ep.runtime_min, Some(50));
            assert_eq!(file_item(&e, files[i]).id, ep.id);
        }
        assert!(items_of(&e.c, "other").is_empty());
        assert_eq!(p.count("search_show"), 1);
        assert_eq!(p.count("season"), 1);
        assert_eq!(p.count("search_movie"), 0);
    }

    #[test]
    fn episode_stills_become_episode_backdrops() {
        let files = [
            "Severance/Season 02/Severance.S02E01.mkv",
            "Severance/Season 02/Severance.S02E02.mkv",
        ];
        let e = env(&files);
        let mut p = severance();
        // TMDB has no still for episode 2 yet.
        p.seasons[0].1.episodes[1].still = None;
        run(&e, &p, IdentifyScope::Unmatched);
        let backdrop = |rel| file_item(&e, rel).backdrop;
        assert_eq!(
            backdrop(files[0]).as_deref(),
            Some("tv-95396-s2e1-backdrop.jpg")
        );
        assert_eq!(backdrop(files[1]), None);
        let still = e.cache().join("tv-95396-s2e1-backdrop.jpg");
        assert_eq!(fs::read(&still).unwrap(), b"original/s2e1.jpg");

        // An episode identified before stills were kept gets its still on the next identify
        // of any scope, its season fetched once for all its episodes.
        fs::remove_file(&still).unwrap();
        e.c.execute(
            "UPDATE items SET backdrop = NULL WHERE kind = 'episode'",
            [],
        )
        .unwrap();
        let seasons = p.count("season");
        assert_eq!(run(&e, &p, IdentifyScope::Review), report(0, 0, 0));
        assert_eq!(
            backdrop(files[0]).as_deref(),
            Some("tv-95396-s2e1-backdrop.jpg")
        );
        assert_eq!(fs::read(&still).unwrap(), b"original/s2e1.jpg");
        assert_eq!(backdrop(files[1]), None);
        assert_eq!(p.count("season"), seasons + 1);
    }

    #[test]
    fn artwork_backfills_stop_at_the_first_outage() {
        let files = [
            "The Matrix (1999).mkv",
            "Heat (1995).mkv",
            "Severance/Season 02/Severance.S02E01.mkv",
            "Severance/Season 02/Severance.S02E02.mkv",
        ];
        let e = env(&files);
        let mut p = Fake {
            movies: vec![
                movie(603, "The Matrix", 1999, 136),
                movie(949, "Heat", 1995, 170),
            ],
            movie_hits: vec![
                (
                    "The Matrix",
                    vec![hit(603, Movie, "The Matrix", Some(1999))],
                ),
                ("Heat", vec![hit(949, Movie, "Heat", Some(1995))]),
            ],
            ..severance()
        };
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(4, 0, 0));
        // Small posters and stills to backfill, with TMDB out of reach.
        fs::remove_dir_all(e.cache()).unwrap();
        e.c.execute(
            "UPDATE items SET backdrop = NULL WHERE kind = 'episode'",
            [],
        )
        .unwrap();
        p.images_fail = true;
        let images = p.count("image");
        let r = run(&e, &p, IdentifyScope::Review);
        // One try for the small posters (three missing), one for the stills (two missing).
        assert_eq!(p.count("image"), images + 2);
        assert_eq!(r.errors.len(), 2);
    }

    #[test]
    fn later_episodes_reuse_show_and_season() {
        let e = env(&["Severance/Season 02/Severance.S02E01.mkv"]);
        let p = severance();
        run(&e, &p, IdentifyScope::Unmatched);
        write(&e.media(), "Severance/Season 02/Severance.S02E02.mkv", "");
        scan_source(&e.c, &e.v, e.src, &mut |_| {}).unwrap();
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(1, 0, 0));
        assert_eq!(items_of(&e.c, "show").len(), 1);
        assert_eq!(items_of(&e.c, "season").len(), 1);
        assert_eq!(items_of(&e.c, "episode").len(), 2);
    }

    #[test]
    fn multi_episode_file_attaches_to_first_episode() {
        let e = env(&["Severance/Severance.S02E01E02.mkv"]);
        assert_eq!(
            run(&e, &severance(), IdentifyScope::Unmatched),
            report(1, 0, 0)
        );
        let eps = items_of(&e.c, "episode");
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].episode, Some(1));
    }

    #[test]
    fn absolute_numbering_maps_to_season_one() {
        let five = "Cowboy Bebop/[SubsGroup] Cowboy Bebop - 05 [1080p][ABCD1234].mkv";
        let thirty = "Cowboy Bebop/[SubsGroup] Cowboy Bebop - 30 [1080p][ABCD1234].mkv";
        let e = env(&[five, thirty]);
        let p = Fake {
            shows: vec![show(30991, "Cowboy Bebop", 1998, &[(1, 26)])],
            seasons: vec![(30991, season(1, 26))],
            show_hits: vec![(
                "Cowboy Bebop",
                vec![hit(30991, Show, "Cowboy Bebop", Some(1998))],
            )],
            ..Default::default()
        };
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(1, 1, 0));
        let ep = file_item(&e, five);
        assert_eq!(
            (ep.kind, ep.season, ep.episode),
            (Episode, Some(1), Some(5))
        );
        let missing = file_item(&e, thirty);
        assert_eq!(
            (missing.kind, missing.match_state),
            (Other, MatchState::NeedsReview)
        );
        assert_eq!(missing.candidates.len(), 1);
        assert_eq!(
            (missing.candidates[0].tmdb_id, missing.candidates[0].kind),
            (30991, Show)
        );
    }

    #[test]
    fn episode_nfo_ids_are_not_show_ids() {
        let rel = "Severance/Season 02/Severance.S02E01.mkv";
        let e = env(&[rel]);
        write(
            &e.media(),
            "Severance/Season 02/Severance.S02E01.nfo",
            r#"<episodedetails><title>Hello, Ms. Cobel</title><uniqueid type="tmdb">3794411</uniqueid></episodedetails>"#,
        );
        write(
            &e.media(),
            "Severance/tvshow.nfo",
            r#"<tvshow><title>Severance</title><uniqueid type="tmdb">95396</uniqueid></tvshow>"#,
        );
        let mut p = severance();
        p.show_hits.clear();
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(1, 0, 0));
        let ep = file_item(&e, rel);
        assert_eq!(
            (ep.kind, ep.episode, ep.match_source),
            (Episode, Some(1), Some(MatchSource::Nfo))
        );
        assert_eq!(items_of(&e.c, "show")[0].tmdb_id, Some(95396));
        assert_eq!(p.count("search"), 0);
        assert_eq!(p.count("find"), 0);
    }

    #[test]
    fn low_confidence_goes_to_review_with_candidates() {
        let e = env(&["Solaris.mkv", "Nothing Here (2001).mkv"]);
        let p = Fake {
            movies: vec![
                movie(593, "Solaris", 1972, 167),
                movie(2103, "Solaris", 2002, 99),
            ],
            movie_hits: vec![(
                "Solaris",
                vec![
                    hit(593, Movie, "Solaris", Some(1972)),
                    hit(2103, Movie, "Solaris", Some(2002)),
                ],
            )],
            ..Default::default()
        };
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(0, 2, 0));
        let sol = file_item(&e, "Solaris.mkv");
        assert_eq!(
            (sol.kind, sol.match_state, sol.tmdb_id),
            (Other, MatchState::NeedsReview, None)
        );
        let mut ids: Vec<i64> = sol.candidates.iter().map(|c| c.tmdb_id).collect();
        ids.sort();
        assert_eq!(ids, [593, 2103]);
        assert!(sol.candidates[0].score >= sol.candidates[1].score);
        let none = file_item(&e, "Nothing Here (2001).mkv");
        assert_eq!(none.match_state, MatchState::NeedsReview);
        assert!(none.candidates.is_empty());
        // Review items are retried only when asked.
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(0, 0, 0));
        assert_eq!(
            run(&e, &p, IdentifyScope::UnmatchedAndReview),
            report(0, 2, 0)
        );
    }

    #[test]
    fn review_candidates_keep_small_posters() {
        let e = env(&["Solaris.mkv"]);
        assert_eq!(
            run(&e, &solaris_with_posters(), IdentifyScope::Unmatched),
            report(0, 1, 0)
        );
        let mut posters: Vec<_> = file_item(&e, "Solaris.mkv")
            .candidates
            .into_iter()
            .map(|c| c.poster)
            .collect();
        posters.sort();
        assert_eq!(
            posters,
            [
                Some("movie-2103-poster.jpg".to_string()),
                Some("movie-593-poster.jpg".to_string())
            ]
        );
        let read = |n: &str| fs::read(e.cache().join(n)).unwrap();
        assert_eq!(read("movie-593-poster-s.jpg"), b"w342/p593.jpg");
        assert!(!e.cache().join("movie-593-poster.jpg").exists());
    }

    #[test]
    fn failed_candidate_poster_still_goes_to_review() {
        let e = env(&["Solaris.mkv"]);
        let p = Fake {
            images_fail: true,
            ..solaris_with_posters()
        };
        let r = run(&e, &p, IdentifyScope::Unmatched);
        assert_eq!((r.needs_review, r.failed, r.errors.len()), (1, 0, 1));
        let it = file_item(&e, "Solaris.mkv");
        assert_eq!(it.match_state, MatchState::NeedsReview);
        assert!(it.candidates.iter().all(|c| c.poster.is_none()));
        // An outage costs one try, not one per candidate.
        assert_eq!(p.count("image"), 1);
    }

    #[test]
    fn review_scope_retries_only_review_items() {
        let e = env(&["Solaris.mkv"]);
        let p = matrix();
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(0, 1, 0));
        write(&e.media(), "The Matrix (1999).mkv", "");
        scan_source(&e.c, &e.v, e.src, &mut |_| {}).unwrap();
        assert_eq!(run(&e, &p, IdentifyScope::Review), report(0, 1, 0));
        let m = file_item(&e, "The Matrix (1999).mkv");
        assert_eq!((m.kind, m.match_state), (Other, MatchState::Unmatched));
        assert_eq!(p.count("search_movie The Matrix"), 0);
    }

    #[test]
    fn probed_duration_picks_the_right_remake() {
        let e = env(&["Solaris.mkv"]);
        let p = Fake {
            movies: vec![
                movie(593, "Solaris", 1972, 167),
                movie(2103, "Solaris", 2002, 99),
            ],
            movie_hits: vec![(
                "Solaris",
                vec![
                    hit(2103, Movie, "Solaris", Some(2002)),
                    hit(593, Movie, "Solaris", Some(1972)),
                ],
            )],
            ..Default::default()
        };
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(0, 1, 0));
        let id = file_item(&e, "Solaris.mkv").id;
        let tech = r#"{"duration_s":10020.0,"width":1920,"height":1080,"video_codec":"h264","hdr":null,"audio":[],"subtitles":[]}"#;
        e.c.execute("UPDATE files SET tech=?1, probed=1", [tech])
            .unwrap();
        assert!(matches!(
            identify(
                &e.c,
                &e.v,
                &p,
                &e.cache(),
                IdentifyScope::Item(99999),
                &mut |_| {}
            ),
            Err(Error::NotFound(_))
        ));
        assert_eq!(run(&e, &p, IdentifyScope::Item(id)), report(1, 0, 0));
        assert_eq!(item(&e.c, id).tmdb_id, Some(593));
        assert_eq!(p.count("movie "), 2);
    }

    #[test]
    fn two_files_same_movie_share_one_item() {
        let a = "The Matrix (1999)/The.Matrix.1999.1080p.mkv";
        let b = "The Matrix (1999)/The.Matrix.1999.2160p.mkv";
        let e = env(&[a, b]);
        let p = matrix();
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(2, 0, 0));
        let (ia, ib) = (file_item(&e, a), file_item(&e, b));
        assert_eq!(ia.id, ib.id);
        assert_eq!(ia.tmdb_id, Some(603));
        let n: i64 =
            e.c.query_row("SELECT count(*) FROM items", [], |r| r.get(0))
                .unwrap();
        assert_eq!(n, 1);
        assert_eq!(p.count("image"), 3);
    }

    /// `--item` on a matched or manual item says so and how to change it.
    fn assert_already_identified(e: &Env, p: &Fake, id: Id) {
        let r = identify(
            &e.c,
            &e.v,
            p,
            &e.cache(),
            IdentifyScope::Item(id),
            &mut |_| {},
        );
        assert!(
            matches!(&r, Err(Error::Invalid(m)) if m.contains("already identified")
                && m.contains("exhibit review match")),
            "{r:?}"
        );
    }

    #[test]
    fn item_scope_refuses_a_matched_item() {
        let rel = "The Matrix (1999).mkv";
        let e = env(&[rel]);
        let p = matrix();
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(1, 0, 0));
        let it = file_item(&e, rel);
        p.calls.lock().unwrap().clear();
        assert_already_identified(&e, &p, it.id);
        assert_eq!(file_item(&e, rel), it);
        assert!(p.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn manual_items_are_never_rematched() {
        let rel = "The Matrix (1999).mkv";
        let e = env(&[rel]);
        let id = file_item(&e, rel).id;
        e.c.execute(
            "UPDATE items SET match_state='manual', title='Mine' WHERE id=?1",
            [id],
        )
        .unwrap();
        let p = matrix();
        let before = item(&e.c, id);
        assert_eq!(
            run(&e, &p, IdentifyScope::UnmatchedAndReview),
            report(0, 0, 0)
        );
        assert_already_identified(&e, &p, id);
        assert_eq!(item(&e.c, id), before);
        assert!(p.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn provider_failure_is_isolated() {
        let files = [
            "Paprika (2006).mkv",
            "Perfect Blue (1997).mkv",
            "The Matrix (1999).mkv",
        ];
        let e = env(&files);
        let mut p = matrix();
        p.movies.push(movie(4977, "Paprika", 2006, 90));
        p.movies.push(movie(10494, "Perfect Blue", 1997, 81));
        p.movie_hits
            .push(("Paprika", vec![hit(4977, Movie, "Paprika", Some(2006))]));
        p.movie_hits.push((
            "Perfect Blue",
            vec![hit(10494, Movie, "Perfect Blue", Some(1997))],
        ));
        p.fail.lock().unwrap().push("Perfect Blue".into());

        let r = run(&e, &p, IdentifyScope::Unmatched);
        assert_eq!((r.matched, r.needs_review, r.failed), (2, 0, 1));
        assert_eq!(r.errors.len(), 1);
        assert!(r.errors[0].contains("Perfect Blue"), "{:?}", r.errors);
        let pb = file_item(&e, files[1]);
        assert_eq!((pb.kind, pb.match_state), (Other, MatchState::Unmatched));

        p.fail.lock().unwrap().clear();
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(1, 0, 0));
        assert_eq!(file_item(&e, files[1]).tmdb_id, Some(10494));
    }

    /// Fails every request with a message that echoes the URL, as a transport error may.
    struct EchoHttp;

    impl crate::tmdb::Http for EchoHttp {
        fn get(&self, url: &str, _bearer: Option<&str>) -> Result<(u16, Vec<u8>)> {
            Err(Error::Provider(format!("request failed: {url}")))
        }
    }

    /// Answers every request with the status it holds and counts the requests.
    struct StatusHttp(u16, RefCell<u32>);

    impl crate::tmdb::Http for &StatusHttp {
        fn get(&self, _url: &str, _bearer: Option<&str>) -> Result<(u16, Vec<u8>)> {
            *self.1.borrow_mut() += 1;
            Ok((self.0, b"{}".to_vec()))
        }
    }

    #[test]
    fn rejected_key_stops_the_run_at_once() {
        let files = [
            "Heat (1995).mkv",
            "Paprika (2006).mkv",
            "The Matrix (1999).mkv",
        ];
        let e = env(&files);
        for status in [401, 403] {
            let http = StatusHttp(status, RefCell::new(0));
            let p = crate::tmdb::Tmdb::new("k".into(), &http);
            let r = identify(
                &e.c,
                &e.v,
                &p,
                &e.cache(),
                IdentifyScope::Unmatched,
                &mut |_| {},
            );
            assert!(
                matches!(&r, Err(Error::Invalid(m)) if m == crate::tmdb::KEY_REJECTED),
                "{r:?}"
            );
            assert_eq!(*http.1.borrow(), 1);
        }
        for f in files {
            let it = file_item(&e, f);
            assert_eq!((it.kind, it.match_state), (Other, MatchState::Unmatched));
        }
    }

    #[test]
    fn stale_nfo_movie_id_falls_back_to_the_name() {
        let rel = "Stuff/The Matrix (1999).mkv";
        let e = env(&[rel]);
        write(
            &e.media(),
            "Stuff/The Matrix (1999).nfo",
            r#"<movie><uniqueid type="tmdb">999</uniqueid><uniqueid type="imdb">tt9999999</uniqueid></movie>"#,
        );
        let p = matrix();
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(1, 0, 0));
        let it = file_item(&e, rel);
        assert_eq!(
            (it.tmdb_id, it.match_source),
            (Some(603), Some(MatchSource::Lookup))
        );
        assert_eq!(p.count("movie 999"), 1);
    }

    #[test]
    fn stale_nfo_show_id_falls_back_to_the_name() {
        let ep = "Severance/Season 02/Severance.S02E01.mkv";
        let extra = "Odds/The Matrix (1999).mkv";
        let e = env(&[ep, extra]);
        write(&e.media(), "Severance/tvshow.nfo", &office_nfo(1));
        // A show nfo beside a file without an episode number.
        write(&e.media(), "Odds/tvshow.nfo", &office_nfo(2));
        let mut p = severance();
        p.movies = matrix().movies;
        p.movie_hits = matrix().movie_hits;
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(2, 0, 0));
        let it = file_item(&e, ep);
        assert_eq!(
            (it.kind, it.match_source),
            (Episode, Some(MatchSource::Lookup))
        );
        assert_eq!(show_of(&e, ep).tmdb_id, Some(95396));
        assert_eq!(file_item(&e, extra).tmdb_id, Some(603));
        assert_eq!((p.count("show 1"), p.count("show 2")), (1, 1));
    }

    #[test]
    fn tmdb_key_never_reaches_report_errors() {
        let e = env(&["The Matrix (1999).mkv"]);
        let p = crate::tmdb::Tmdb::new("sekrit-key-42".into(), EchoHttp);
        let r = identify(
            &e.c,
            &e.v,
            &p,
            &e.cache(),
            IdentifyScope::Unmatched,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!((r.failed, r.errors.len()), (1, 1));
        assert!(!r.errors[0].contains("sekrit"), "{:?}", r.errors);
        assert!(r.errors[0].contains("api_key=***"), "{:?}", r.errors);
    }

    #[test]
    fn failed_show_group_changes_nothing() {
        let files = [
            "Severance/Season 02/Severance.S02E01.mkv",
            "Severance/Season 02/Severance.S02E02.mkv",
        ];
        let e = env(&files);
        let p = severance();
        p.fail.lock().unwrap().push("season".into());
        let r = run(&e, &p, IdentifyScope::Unmatched);
        assert_eq!((r.matched, r.failed, r.errors.len()), (0, 2, 1));
        assert!(items_of(&e.c, "show").is_empty());
        assert_eq!(items_of(&e.c, "other").len(), 2);
    }

    #[test]
    fn artwork_failure_does_not_fail_match() {
        let rel = "The Matrix (1999).mkv";
        let e = env(&[rel]);
        let p = Fake {
            images_fail: true,
            ..matrix()
        };
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(1, 0, 0));
        let it = file_item(&e, rel);
        assert_eq!(
            (it.match_state, it.tmdb_id),
            (MatchState::Matched, Some(603))
        );
        assert_eq!((it.poster, it.backdrop), (None, None));
    }

    #[test]
    fn small_poster_saved_beside_large() {
        let rel = "The Matrix (1999).mkv";
        let e = env(&[rel]);
        let p = matrix();
        run(&e, &p, IdentifyScope::Unmatched);
        assert_eq!(
            file_item(&e, rel).poster.as_deref(),
            Some("movie-603-poster.jpg")
        );
        let small = e.cache().join("movie-603-poster-s.jpg");
        assert_eq!(fs::read(&small).unwrap(), b"w342/p603.jpg");

        // An item identified before small posters existed gets one on the next identify of
        // any scope, even with nothing pending.
        fs::remove_file(&small).unwrap();
        let movie_calls = p.count("movie 603");
        assert_eq!(run(&e, &p, IdentifyScope::Review), report(0, 0, 0));
        assert_eq!(fs::read(&small).unwrap(), b"w342/p603.jpg");
        assert_eq!(p.count("movie 603"), movie_calls + 1);
        // Nothing missing: no call at all.
        run(&e, &p, IdentifyScope::Review);
        assert_eq!(p.count("movie 603"), movie_calls + 1);

        // A failed download is reported and does not fail the run.
        fs::remove_file(&small).unwrap();
        p.fail.lock().unwrap().push("w342".into());
        let r = run(&e, &p, IdentifyScope::Review);
        assert_eq!((r.matched, r.failed, r.errors.len()), (0, 0, 1));
        assert!(!small.exists());
    }

    #[test]
    fn lookup_returns_scored_candidates() {
        let mut p = matrix();
        p.movie_hits = vec![(
            "Matrix",
            vec![
                hit(1, Movie, "Matrix Reloaded", Some(2003)),
                hit(603, Movie, "The Matrix", Some(1999)),
            ],
        )];
        let found = lookup(&p, Path::new("."), "Matrix", Movie, Some(1999)).unwrap();
        assert_eq!(
            found.iter().map(|c| c.tmdb_id).collect::<Vec<_>>(),
            [603, 1]
        );
        let ev = Evidence {
            title: "Matrix",
            year: Some(1999),
            duration_min: None,
        };
        let expected = matcher::score(&ev, &hit(603, Movie, "The Matrix", Some(1999)), None);
        assert_eq!(found[0].score, expected);
        assert_eq!(found[0].kind, Movie);
        // Shows are searched as shows.
        let s = severance();
        let found = lookup(&s, Path::new("."), "Severance", Show, None).unwrap();
        assert_eq!((found.len(), found[0].kind), (1, Show));
        assert_eq!(s.count("search_show"), 1);
    }

    #[test]
    fn lookup_rejects_other_kind() {
        let p = matrix();
        for kind in [Other, Episode, ItemKind::Season] {
            assert!(matches!(
                lookup(&p, Path::new("."), "The Matrix", kind, None),
                Err(Error::Invalid(_))
            ));
        }
        assert!(p.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn apply_match_sets_manual_and_survives_rescan_and_identify() {
        let rel = "Misc/zz capture.mkv";
        let e = env(&[rel]);
        let p = matrix();
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(0, 1, 0));
        let id = file_item(&e, rel).id;

        apply_match(&e.c, &p, &e.cache(), id, 603, Movie, MatchSource::Manual).unwrap();
        let it = item(&e.c, id);
        assert_eq!(
            (it.kind, it.title.as_str(), it.tmdb_id),
            (Movie, "The Matrix", Some(603))
        );
        assert_eq!(
            (it.match_state, it.match_source),
            (MatchState::Manual, Some(MatchSource::Manual))
        );
        assert_eq!(it.poster.as_deref(), Some("movie-603-poster.jpg"));

        let scanned = scan_source(&e.c, &e.v, e.src, &mut |_| {}).unwrap();
        assert_eq!(scanned.unchanged, 1);
        p.calls.lock().unwrap().clear();
        assert_eq!(
            run(&e, &p, IdentifyScope::UnmatchedAndReview),
            report(0, 0, 0)
        );
        assert_already_identified(&e, &p, id);
        assert_eq!(item(&e.c, id), it);
        assert!(p.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn apply_match_show_places_episode_or_refuses() {
        let ep = "Misc/Severance.S02E03.mkv";
        let extra = "Misc/severance bts.mkv";
        let e = env(&[ep, extra]);
        let mut p = severance();
        p.show_hits.clear();
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(0, 2, 0));
        let (ep_id, extra_id) = (file_item(&e, ep).id, file_item(&e, extra).id);

        apply_match(
            &e.c,
            &p,
            &e.cache(),
            ep_id,
            95396,
            Show,
            MatchSource::Manual,
        )
        .unwrap();
        let it = file_item(&e, ep);
        assert_eq!(
            (it.kind, it.season, it.episode),
            (Episode, Some(2), Some(3))
        );
        assert_eq!(it.match_state, MatchState::Manual);
        assert_eq!(items_of(&e.c, "show")[0].tmdb_id, Some(95396));
        let gone: i64 =
            e.c.query_row("SELECT count(*) FROM items WHERE id=?1", [ep_id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(gone, 0);

        let before = item(&e.c, extra_id);
        for kind in [Show, ItemKind::Season] {
            let r = apply_match(
                &e.c,
                &p,
                &e.cache(),
                extra_id,
                95396,
                kind,
                MatchSource::Manual,
            );
            assert!(matches!(r, Err(Error::Invalid(_))), "{kind:?}: {r:?}");
        }
        assert_eq!(item(&e.c, extra_id), before);
        assert!(matches!(
            apply_match(&e.c, &p, &e.cache(), 99999, 1, Movie, MatchSource::Manual),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn move_to_other_sticks() {
        let rel = "The Matrix (1999)/The.Matrix.1999.mkv";
        let e = env(&[rel]);
        let p = matrix();
        run(&e, &p, IdentifyScope::Unmatched);
        let id = file_item(&e, rel).id;
        move_to_other(&e.c, id).unwrap();
        let it = item(&e.c, id);
        assert_eq!(
            (it.kind, it.match_state, it.match_source),
            (Other, MatchState::Manual, Some(MatchSource::Manual))
        );
        assert_eq!(it.title, crate::parse::parse_path(rel).title);
        assert_eq!(
            (it.tmdb_id, it.poster.as_deref(), it.overview.as_deref()),
            (None, None, None)
        );
        assert!(it.genres.is_empty() && it.people.is_empty());

        p.calls.lock().unwrap().clear();
        assert_eq!(
            run(&e, &p, IdentifyScope::UnmatchedAndReview),
            report(0, 0, 0)
        );
        assert_eq!(item(&e.c, id), it);
        assert!(p.calls.lock().unwrap().is_empty());
        assert!(matches!(
            move_to_other(&e.c, 99999),
            Err(Error::NotFound(_))
        ));
    }

    fn office() -> Fake {
        Fake {
            shows: vec![
                show(2316, "The Office", 2005, &[(1, 6)]),
                show(2996, "The Office", 2001, &[(1, 6)]),
            ],
            seasons: vec![(2316, season(1, 6)), (2996, season(1, 6))],
            ..Default::default()
        }
    }

    fn office_nfo(id: i64) -> String {
        format!(r#"<tvshow><uniqueid type="tmdb">{id}</uniqueid></tvshow>"#)
    }

    fn show_of(e: &Env, rel: &str) -> Item {
        let ep = file_item(e, rel);
        let season = item(&e.c, ep.parent_id.unwrap());
        item(&e.c, season.parent_id.unwrap())
    }

    #[test]
    fn different_nfo_show_ids_under_one_title_stay_apart() {
        let us = "The Office (US)/Season 01/The.Office.S01E01.mkv";
        let uk = "The Office (UK)/Season 01/The.Office.S01E01.mkv";
        let e = env(&[us, uk]);
        write(&e.media(), "The Office (US)/tvshow.nfo", &office_nfo(2316));
        write(&e.media(), "The Office (UK)/tvshow.nfo", &office_nfo(2996));
        let p = office();
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(2, 0, 0));
        assert_eq!(show_of(&e, us).tmdb_id, Some(2316));
        assert_eq!(show_of(&e, uk).tmdb_id, Some(2996));
        assert_eq!(items_of(&e.c, "show").len(), 2);
        assert_eq!(p.count("search"), 0);
    }

    #[test]
    fn file_without_nfo_joins_only_one_nfo_group_as_lookup() {
        let us = "The Office (US)/Season 01/The.Office.S01E01.mkv";
        let loose = "Loose/The.Office.S01E02.mkv";
        let e = env(&[us, loose]);
        write(&e.media(), "The Office (US)/tvshow.nfo", &office_nfo(2316));
        let p = office();
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(2, 0, 0));
        assert_eq!(show_of(&e, loose).tmdb_id, Some(2316));
        let joined = file_item(&e, loose);
        assert_eq!(
            (joined.match_source, joined.confidence),
            (Some(MatchSource::Lookup), None)
        );
        let own = file_item(&e, us);
        assert_eq!(
            (own.match_source, own.confidence),
            (Some(MatchSource::Nfo), Some(1.0))
        );
        assert_eq!(p.count("search"), 0);
    }

    #[test]
    fn file_without_nfo_beside_two_nfo_groups_is_searched() {
        let us = "The Office (US)/Season 01/The.Office.S01E01.mkv";
        let uk = "The Office (UK)/Season 01/The.Office.S01E01.mkv";
        let loose = "Loose/The.Office.S01E03.mkv";
        let e = env(&[us, uk, loose]);
        write(&e.media(), "The Office (US)/tvshow.nfo", &office_nfo(2316));
        write(&e.media(), "The Office (UK)/tvshow.nfo", &office_nfo(2996));
        let mut p = office();
        p.show_hits = vec![(
            "The Office",
            vec![
                hit(2316, Show, "The Office", Some(2005)),
                hit(2996, Show, "The Office", Some(2001)),
            ],
        )];
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(2, 1, 0));
        assert_eq!(p.count("search"), 1);
        assert_eq!(p.count("search_show The Office None"), 1);
        let it = file_item(&e, loose);
        assert_eq!((it.kind, it.match_state), (Other, MatchState::NeedsReview));
        assert_eq!(it.candidates.len(), 2);
    }

    #[test]
    fn same_title_shows_with_different_years_are_searched_apart() {
        let old = "Doctor Who (1963)/Season 01/Doctor.Who.S01E01.mkv";
        let new = "Doctor Who (2005)/Season 01/Doctor.Who.S01E01.mkv";
        let e = env(&[old, new]);
        let p = Fake {
            shows: vec![
                show(121, "Doctor Who", 1963, &[(1, 42)]),
                show(57243, "Doctor Who", 2005, &[(1, 13)]),
            ],
            seasons: vec![(121, season(1, 42)), (57243, season(1, 13))],
            show_hits: vec![(
                "Doctor Who",
                vec![
                    hit(121, Show, "Doctor Who", Some(1963)),
                    hit(57243, Show, "Doctor Who", Some(2005)),
                ],
            )],
            ..Default::default()
        };
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(2, 0, 0));
        assert_eq!(p.count("search_show Doctor Who Some(1963)"), 1);
        assert_eq!(p.count("search_show Doctor Who Some(2005)"), 1);
        assert_eq!(p.count("search"), 2);
        assert_eq!(show_of(&e, old).tmdb_id, Some(121));
        assert_eq!(show_of(&e, new).tmdb_id, Some(57243));
    }

    #[test]
    fn tv_url_beside_a_movie_file_never_matches_a_movie() {
        let rel = "Stuff/xq7_final.mkv";
        let e = env(&[rel]);
        write(
            &e.media(),
            "Stuff/xq7_final.nfo",
            "Info: https://www.themoviedb.org/tv/603\n",
        );
        let p = matrix();
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(0, 1, 0));
        let it = file_item(&e, rel);
        assert_eq!((it.kind, it.tmdb_id), (Other, None));
        assert_eq!(p.count("movie 603"), 0);
        assert_eq!(p.count("find"), 0);
    }

    #[test]
    fn show_nfo_tmdb_id_is_never_tried_as_a_movie() {
        let rel = "Severance/Season 02/Severance.S02E01.mkv";
        let e = env(&[rel]);
        write(&e.media(), "Severance/tvshow.nfo", &office_nfo(95396));
        let mut p = severance();
        p.movies
            .push(movie(95396, "A Movie With That Id", 2000, 90));
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(1, 0, 0));
        assert_eq!(show_of(&e, rel).tmdb_id, Some(95396));
        assert_eq!(p.count("movie "), 0);
        assert_eq!(p.count("find"), 0);
        assert_eq!(p.count("show 95396"), 1);
    }

    #[test]
    fn absorbed_item_keeps_play_data() {
        let a = "The Matrix (1999)/The.Matrix.1999.1080p.mkv";
        let b = "The Matrix (1999)/The.Matrix.1999.2160p.mkv";
        let e = env(&[a, b]);
        let (ia, ib) = (file_item(&e, a).id, file_item(&e, b).id);
        e.c.execute_batch(&format!(
            "INSERT INTO play_state(item_id, position_s, duration_s, watched, last_played_at) \
             VALUES ({ia}, 5, 8160, 0, 10), ({ib}, 100, 8160, 0, 50); \
             INSERT INTO play_log(item_id, started_at, ended_at, position_s) \
             VALUES ({ib}, 40, 50, 100); \
             UPDATE items SET loved=1 WHERE id={ib};"
        ))
        .unwrap();
        assert_eq!(
            run(&e, &matrix(), IdentifyScope::Unmatched),
            report(2, 0, 0)
        );
        let target = file_item(&e, b);
        assert_eq!(target.id, ia);
        assert!(target.loved);
        let (pos, last): (f64, i64) =
            e.c.query_row(
                "SELECT position_s, last_played_at FROM play_state WHERE item_id=?1",
                [ia],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((pos, last), (100.0, 50));
        let logs: i64 =
            e.c.query_row(
                "SELECT count(*) FROM play_log WHERE item_id=?1",
                [ia],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(logs, 1);
    }

    #[test]
    fn manual_choice_on_existing_movie_makes_it_manual() {
        let matched = "The Matrix (1999)/The.Matrix.1999.mkv";
        let other = "Misc/zz capture.mkv";
        let e = env(&[matched, other]);
        let p = matrix();
        assert_eq!(run(&e, &p, IdentifyScope::Unmatched), report(1, 1, 0));
        let target = file_item(&e, matched).id;
        assert_eq!(item(&e.c, target).match_state, MatchState::Matched);
        let id = file_item(&e, other).id;
        apply_match(&e.c, &p, &e.cache(), id, 603, Movie, MatchSource::Manual).unwrap();
        let it = file_item(&e, other);
        assert_eq!(it.id, target);
        assert_eq!(
            (it.match_state, it.match_source),
            (MatchState::Manual, Some(MatchSource::Manual))
        );
    }
}
