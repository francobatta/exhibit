//! Read side of the library: search, shelves, details, history and the taste profile, plus
//! the few small writes the UI makes (watched, loved). Every listing hides items that have
//! no file anywhere beneath them; `item` still returns them.

use crate::db;
use crate::error::{Error, Result};
use crate::model::{Id, Item, ItemKind, MatchState, MediaFile, PlayState, WATCHED_FRACTION};
use crate::rows::{
    FILE_COLUMNS, ITEM_COLUMNS, PLAY_STATE_COLUMNS, SUMMARY_COLUMNS, enum_str, item_from_row,
    media_file_from_row, play_state_from_row, summary_from_row,
};
use crate::scan;
use crate::volume::Volumes;
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use std::collections::{HashMap, HashSet};

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 500;
const TASTE_KEY: &str = "taste_profile";

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ItemSummary {
    pub id: Id,
    pub kind: ItemKind,
    pub title: String,
    pub year: Option<i32>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub show_title: Option<String>,
    pub poster: Option<String>,
    pub backdrop: Option<String>,
    pub runtime_min: Option<i32>,
    pub height: Option<u32>,
    pub hdr: Option<String>,
    pub watched: bool,
    pub progress: Option<f32>,
    pub loved: bool,
    pub available: bool,
    pub match_state: MatchState,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ItemDetail {
    pub item: Item,
    pub files: Vec<MediaFile>,
    pub play: Option<PlayState>,
    pub children: Vec<ItemSummary>,
    pub available: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Title,
    Added,
    Year,
    Runtime,
}

#[derive(Default)]
pub struct SearchQuery {
    pub text: Option<String>,
    pub kind: Option<ItemKind>,
    pub genre: Option<String>,
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
    pub runtime_max_min: Option<i32>,
    pub watched: Option<bool>,
    pub loved: Option<bool>,
    pub min_height: Option<u32>,
    pub available: Option<bool>,
    pub sort: Option<Sort>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shelf {
    ContinueWatching,
    RecentlyAdded,
    Unwatched,
    Loved,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct HistoryEntry {
    pub item: ItemSummary,
    pub started_at: i64,
    pub ended_at: i64,
    pub position_s: f64,
}

/// `tree(root_id, id)`: every item paired with itself and each of its descendants.
const TREE_CTE: &str = "tree(root_id, id) AS (SELECT id, id FROM items \
     UNION ALL SELECT tree.root_id, i.id FROM tree JOIN items i ON i.parent_id = tree.id)";

/// `lib(item_id, avail)`: items with at least one file in their subtree; `avail` is 1 when
/// one of those files is on a source whose id is in the JSON array bound as `?1`.
const LIB_CTE: &str = "lib(item_id, avail) AS (SELECT t.root_id, \
     max(f.source_id IN (SELECT value FROM json_each(?1))) \
     FROM tree t JOIN files f ON f.item_id = t.id GROUP BY t.root_id)";

/// Bound parameters of one statement; `bind` returns the `?N` placeholder for a value.
/// `?1` is always the JSON array of online source ids.
struct Params(Vec<Value>);

impl Params {
    fn new(online_sources: &[Id]) -> Params {
        Params(vec![Value::Text(json_ids(online_sources))])
    }

    fn bind(&mut self, v: impl Into<Value>) -> String {
        self.0.push(v.into());
        format!("?{}", self.0.len())
    }
}

fn json_ids(ids: &[Id]) -> String {
    serde_json::to_string(ids).unwrap_or_else(|_| "[]".into())
}

/// Ids of the sources that are online right now, computed once per public call.
fn online_sources(c: &Connection, v: &dyn Volumes) -> Result<Vec<Id>> {
    Ok(scan::list_sources(c, v)?
        .into_iter()
        .filter(|s| s.online)
        .map(|s| s.id)
        .collect())
}

/// Runs the summary listing: one row per visible item with roll-ups for shows and seasons,
/// narrowed by `wheres` (conditions on the alias `b`) and ordered by `order`.
fn list(
    c: &Connection,
    mut p: Params,
    wheres: &[String],
    order: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<ItemSummary>> {
    let show = p.bind(enum_str(ItemKind::Show));
    let season = p.bind(enum_str(ItemKind::Season));
    let episode = p.bind(enum_str(ItemKind::Episode));
    let (limit_p, offset_p) = (p.bind(limit as i64), p.bind(offset as i64));
    let container = format!("i.kind IN ({show}, {season})");
    // Episodes at or below the item that have a file, all of them and the watched ones.
    let has_file = "EXISTS (SELECT 1 FROM files ff WHERE ff.item_id = e.id)";
    let total = format!(
        "(SELECT count(*) FROM tree t JOIN items e ON e.id = t.id \
         WHERE t.root_id = i.id AND e.kind = {episode} AND {has_file})"
    );
    let seen = format!(
        "(SELECT count(*) FROM tree t JOIN items e ON e.id = t.id \
         JOIN play_state q ON q.item_id = e.id \
         WHERE t.root_id = i.id AND e.kind = {episode} AND q.watched = 1 AND {has_file})"
    );
    // Technical field of the item's first file that has any (any episode's, for a show).
    let tech = |field: &str| {
        format!(
            "(SELECT json_extract(f.tech, '$.{field}') FROM tree t \
             JOIN files f ON f.item_id = t.id \
             WHERE t.root_id = i.id AND f.tech IS NOT NULL ORDER BY f.id LIMIT 1)"
        )
    };
    let where_sql = if wheres.is_empty() {
        "1".to_string()
    } else {
        wheres.join(" AND ")
    };
    let sql = format!(
        "WITH RECURSIVE {TREE_CTE}, {LIB_CTE} SELECT {SUMMARY_COLUMNS} FROM (\
           SELECT i.id, i.kind, i.parent_id, i.title, i.year, i.season, i.episode, \
             (SELECT s.title FROM tree t JOIN items s ON s.id = t.root_id \
                WHERE t.id = i.id AND t.root_id <> i.id AND s.kind = {show}) AS show_title, \
             i.poster, i.backdrop, i.runtime_min, {height} AS height, {hdr} AS hdr, \
             CASE WHEN {container} THEN {total} > 0 AND {seen} = {total} \
                  ELSE coalesce(p.watched, 0) END AS watched, \
             CASE WHEN {container} THEN 1.0 * {seen} / nullif({total}, 0) \
                  ELSE CASE WHEN p.duration_s > 0 \
                       THEN min(1.0, p.position_s / p.duration_s) END END AS progress, \
             i.loved, lib.avail AS available, i.match_state, i.genres, i.added_at \
           FROM items i JOIN lib ON lib.item_id = i.id \
           LEFT JOIN play_state p ON p.item_id = i.id) b \
         WHERE {where_sql} ORDER BY {order} LIMIT {limit_p} OFFSET {offset_p}",
        height = tech("height"),
        hdr = tech("hdr"),
    );
    let mut s = c.prepare_cached(&sql)?;
    let rows = s
        .query_map(params_from_iter(p.0.iter()), summary_from_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Summaries of the visible items among `ids`, in the order of `ids`.
fn summaries_by_ids(c: &Connection, online: &[Id], ids: &[Id]) -> Result<Vec<ItemSummary>> {
    let mut p = Params::new(online);
    let cond = format!(
        "b.id IN (SELECT value FROM json_each({}))",
        p.bind(json_ids(ids))
    );
    let mut by_id: HashMap<Id, ItemSummary> = list(c, p, &[cond], "b.id", u32::MAX, 0)?
        .into_iter()
        .map(|s| (s.id, s))
        .collect();
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

/// Turns free text into an FTS5 query: every word becomes a quoted prefix term, so quotes,
/// `*` and operators in user input cannot change the query's meaning. `None` when the text
/// holds no word.
fn fts_query(text: &str) -> Option<String> {
    let cleaned: String = text
        .chars()
        .map(|ch| if ch.is_alphanumeric() { ch } else { ' ' })
        .collect();
    let terms: Vec<String> = cleaned
        .split_whitespace()
        .map(|w| format!("\"{w}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

fn default_kinds(p: &mut Params) -> String {
    let kinds = [ItemKind::Movie, ItemKind::Show, ItemKind::Other]
        .map(|k| p.bind(enum_str(k)))
        .join(", ");
    format!("b.kind IN ({kinds})")
}

pub fn search(c: &Connection, v: &dyn Volumes, q: &SearchQuery) -> Result<Vec<ItemSummary>> {
    let mut p = Params::new(&online_sources(c, v)?);
    let mut w = vec![match q.kind {
        Some(k) => format!("b.kind = {}", p.bind(enum_str(k))),
        None => default_kinds(&mut p),
    }];
    if let Some(m) = q.text.as_deref().and_then(fts_query) {
        // A hit on an episode or season counts for every item above it.
        w.push(format!(
            "b.id IN (SELECT t.root_id FROM tree t WHERE t.id IN \
             (SELECT rowid FROM items_fts WHERE items_fts MATCH {}))",
            p.bind(m)
        ));
    }
    if let Some(g) = &q.genre {
        w.push(format!(
            "EXISTS (SELECT 1 FROM json_each(b.genres) WHERE lower(value) = lower({}))",
            p.bind(g.clone())
        ));
    }
    if let Some(y) = q.year_min {
        w.push(format!("b.year >= {}", p.bind(y)));
    }
    if let Some(y) = q.year_max {
        w.push(format!("b.year <= {}", p.bind(y)));
    }
    if let Some(r) = q.runtime_max_min {
        w.push(format!("b.runtime_min <= {}", p.bind(r)));
    }
    if let Some(x) = q.watched {
        w.push(format!("b.watched = {}", p.bind(x)));
    }
    if let Some(x) = q.loved {
        w.push(format!("b.loved = {}", p.bind(x)));
    }
    if let Some(h) = q.min_height {
        w.push(format!("b.height >= {}", p.bind(h)));
    }
    if let Some(x) = q.available {
        w.push(format!("b.available = {}", p.bind(x)));
    }
    let order = match q.sort.unwrap_or(Sort::Title) {
        Sort::Title => "b.title COLLATE NOCASE, b.id",
        Sort::Added => "b.added_at DESC, b.id DESC",
        Sort::Year => "b.year IS NULL, b.year DESC, b.title COLLATE NOCASE, b.id",
        Sort::Runtime => "b.runtime_min IS NULL, b.runtime_min, b.title COLLATE NOCASE, b.id",
    };
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    list(c, p, &w, order, limit, q.offset.unwrap_or(0))
}

/// Full details of one item, hidden or not. Children are the seasons of a show or the
/// episodes of a season (only those with files).
pub fn item(c: &Connection, v: &dyn Volumes, id: Id) -> Result<ItemDetail> {
    item_with(c, &online_sources(c, v)?, id, true)
}

/// `item` with the online sources already known; `children` false leaves the list empty.
fn item_with(c: &Connection, online: &[Id], id: Id, children: bool) -> Result<ItemDetail> {
    let item = c
        .prepare_cached(&format!("SELECT {ITEM_COLUMNS} FROM items WHERE id = ?1"))?
        .query_row([id], item_from_row)
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("item {id}")))?;
    let files = c
        .prepare_cached(&format!(
            "SELECT {FILE_COLUMNS} FROM files WHERE item_id = ?1 ORDER BY id"
        ))?
        .query_map([id], media_file_from_row)?
        .collect::<rusqlite::Result<_>>()?;
    let play = c
        .prepare_cached(&format!(
            "SELECT {PLAY_STATE_COLUMNS} FROM play_state WHERE item_id = ?1"
        ))?
        .query_row([id], play_state_from_row)
        .optional()?;
    // Rooted at this item, so the cost does not grow with the library.
    let available = c
        .prepare_cached(
            "WITH RECURSIVE d(id) AS (SELECT ?2 UNION ALL \
                 SELECT i.id FROM items i JOIN d ON i.parent_id = d.id) \
             SELECT coalesce(max(f.source_id IN (SELECT value FROM json_each(?1))), 0) \
             FROM d JOIN files f ON f.item_id = d.id",
        )?
        .query_row(params![json_ids(online), id], |r| r.get(0))?;
    let children = if children {
        let mut p = Params::new(online);
        let cond = format!("b.parent_id = {}", p.bind(id));
        list(
            c,
            p,
            &[cond],
            "b.season IS NULL, b.season, b.episode IS NULL, b.episode, b.title COLLATE NOCASE, b.id",
            u32::MAX,
            0,
        )?
    } else {
        Vec::new()
    };
    Ok(ItemDetail {
        item,
        files,
        play,
        children,
        available,
    })
}

pub fn shelf(c: &Connection, v: &dyn Volumes, s: Shelf, limit: u32) -> Result<Vec<ItemSummary>> {
    let online = online_sources(c, v)?;
    let limit = limit.min(MAX_LIMIT);
    let mut p = Params::new(&online);
    let (wheres, order) = match s {
        Shelf::ContinueWatching => return continue_watching(c, &online, limit),
        Shelf::RecentlyAdded => (
            vec![default_kinds(&mut p)],
            // A show rises when an episode arrives.
            "(SELECT max(d.added_at) FROM tree t JOIN items d ON d.id = t.id \
              WHERE t.root_id = b.id) DESC, b.id DESC",
        ),
        Shelf::Unwatched => (
            vec![
                format!("b.kind = {}", p.bind(enum_str(ItemKind::Movie))),
                "NOT EXISTS (SELECT 1 FROM play_state q WHERE q.item_id = b.id \
                 AND (q.watched = 1 OR q.position_s > 0))"
                    .to_string(),
            ],
            "b.added_at DESC, b.id DESC",
        ),
        Shelf::Loved => (
            vec![default_kinds(&mut p), "b.loved = 1".to_string()],
            "b.added_at DESC, b.id DESC",
        ),
    };
    list(c, p, &wheres, order, limit, 0)
}

/// Items part-way through, plus the next unwatched episode of each show whose latest played
/// episode is finished; newest first.
fn continue_watching(c: &Connection, online: &[Id], limit: u32) -> Result<Vec<ItemSummary>> {
    let mut picks: Vec<(i64, Id)> = c
        .prepare_cached(
            "SELECT last_played_at, item_id FROM play_state \
             WHERE watched = 0 AND position_s > 0 AND duration_s > 0 \
               AND position_s / duration_s < ?1 \
             ORDER BY last_played_at DESC, item_id LIMIT ?2",
        )?
        .query_map(params![WATCHED_FRACTION, limit], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    // Played episodes with their show, latest first; read lazily and stop once `limit`
    // shows have offered a next episode.
    let mut q = c.prepare_cached(&format!(
        "WITH RECURSIVE {TREE_CTE} SELECT t.root_id, p.last_played_at, p.watched \
         FROM tree t JOIN items s ON s.id = t.root_id AND s.kind = ?1 \
         JOIN items e ON e.id = t.id AND e.kind = ?2 \
         JOIN play_state p ON p.item_id = e.id \
         ORDER BY p.last_played_at DESC, e.season DESC, e.episode DESC"
    ))?;
    let played = q.query_map(
        params![enum_str(ItemKind::Show), enum_str(ItemKind::Episode)],
        |r| {
            Ok((
                r.get::<_, Id>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, bool>(2)?,
            ))
        },
    )?;
    let mut done = HashSet::new();
    let mut offered = 0;
    for row in played {
        let (show, played_at, watched) = row?;
        if offered >= limit {
            break;
        }
        if !done.insert(show) {
            continue;
        }
        if watched && let Some(next) = next_episode(c, show)? {
            picks.push((played_at, next));
            offered += 1;
        }
    }
    picks.sort_by_key(|&(at, id)| (std::cmp::Reverse(at), id));
    let ids: Vec<Id> = picks.into_iter().map(|(_, id)| id).collect();
    let mut found = summaries_by_ids(c, online, &ids)?;
    found.truncate(limit as usize);
    Ok(found)
}

/// Items waiting for the owner to confirm a match.
/// Their `children` are left empty: review candidates are file-backed provisional items.
pub fn needs_review(c: &Connection, v: &dyn Volumes) -> Result<Vec<ItemDetail>> {
    let online = online_sources(c, v)?;
    let mut p = Params::new(&online);
    let cond = format!(
        "b.match_state = {}",
        p.bind(enum_str(MatchState::NeedsReview))
    );
    list(c, p, &[cond], "b.added_at DESC, b.id DESC", u32::MAX, 0)?
        .into_iter()
        .map(|s| item_with(c, &online, s.id, false))
        .collect()
}

/// Headline numbers for the status view. Movies, shows and other count items with a present
/// file somewhere beneath them; `needs_review` and `unprobed` are what still wants attention.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct LibraryCounts {
    pub movies: u32,
    pub shows: u32,
    pub other: u32,
    pub needs_review: u32,
    pub unprobed: u32,
}

pub fn library_counts(c: &Connection, v: &dyn Volumes) -> Result<LibraryCounts> {
    let of_kind = |kind: ItemKind| -> Result<u32> {
        Ok(c.query_row(
            &format!(
                "WITH RECURSIVE {TREE_CTE} SELECT count(DISTINCT r.id) FROM items r \
                 JOIN tree t ON t.root_id = r.id \
                 JOIN files f ON f.item_id = t.id WHERE r.kind = ?1"
            ),
            [enum_str(kind)],
            |r| r.get(0),
        )?)
    };
    Ok(LibraryCounts {
        movies: of_kind(ItemKind::Movie)?,
        shows: of_kind(ItemKind::Show)?,
        other: of_kind(ItemKind::Other)?,
        needs_review: needs_review(c, v)?.len() as u32,
        unprobed: c.query_row("SELECT count(*) FROM files WHERE probed = 0", [], |r| {
            r.get(0)
        })?,
    })
}

/// First unwatched episode of a show that has a file, by season then episode, specials
/// (season 0) after the numbered seasons and episodes with no season last.
pub fn next_episode(c: &Connection, show_id: Id) -> Result<Option<Id>> {
    first_episode(c, show_id, true)
}

/// First episode with a file under `root` (a show or season) in viewing order, specials
/// (season 0) after the numbered seasons and episodes with no season last; with
/// `unwatched_only`, the first one not yet watched.
pub(crate) fn first_episode(c: &Connection, root: Id, unwatched_only: bool) -> Result<Option<Id>> {
    Ok(c.prepare_cached(
        "WITH RECURSIVE d(id) AS (SELECT ?1 UNION ALL \
             SELECT i.id FROM items i JOIN d ON i.parent_id = d.id) \
         SELECT e.id FROM d JOIN items e ON e.id = d.id AND e.kind = ?2 \
         LEFT JOIN play_state p ON p.item_id = e.id \
         WHERE (?3 = 0 OR coalesce(p.watched, 0) = 0) \
           AND EXISTS (SELECT 1 FROM files f WHERE f.item_id = e.id) \
         ORDER BY e.season IS NULL, e.season = 0, e.season, e.episode, e.id LIMIT 1",
    )?
    .query_row(
        params![root, enum_str(ItemKind::Episode), unwatched_only],
        |r| r.get(0),
    )
    .optional()?)
}

/// Marks an item watched or not; on a show or season it applies to every episode below.
/// Marking watched creates a play state when there is none; unmarking resets the position.
pub fn set_watched(c: &Connection, id: Id, watched: bool) -> Result<()> {
    let kind: String = c
        .prepare_cached("SELECT kind FROM items WHERE id = ?1")?
        .query_row([id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("item {id}")))?;
    let container = [ItemKind::Show, ItemKind::Season]
        .iter()
        .any(|k| enum_str(*k) == kind);
    let targets: Vec<Id> = if container {
        c.prepare_cached(
            "WITH RECURSIVE d(id) AS (SELECT ?1 UNION ALL \
                 SELECT i.id FROM items i JOIN d ON i.parent_id = d.id) \
             SELECT i.id FROM d JOIN items i ON i.id = d.id AND i.kind = ?2",
        )?
        .query_map(params![id, enum_str(ItemKind::Episode)], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?
    } else {
        vec![id]
    };
    let tx = db::write_tx(c)?;
    for t in targets {
        if watched {
            tx.prepare_cached(
                "INSERT INTO play_state(item_id, position_s, duration_s, watched, last_played_at) \
                 VALUES (?1, 0, 0, 1, ?2) ON CONFLICT(item_id) DO UPDATE SET watched = 1",
            )?
            .execute(params![t, db::now()])?;
        } else {
            tx.prepare_cached(
                "UPDATE play_state SET watched = 0, position_s = 0 WHERE item_id = ?1",
            )?
            .execute([t])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn set_loved(c: &Connection, id: Id, loved: bool) -> Result<()> {
    let n = c
        .prepare_cached("UPDATE items SET loved = ?2 WHERE id = ?1")?
        .execute(params![id, loved])?;
    if n == 0 {
        return Err(Error::NotFound(format!("item {id}")));
    }
    Ok(())
}

/// Play sessions, newest first, for items that still have files.
pub fn history(c: &Connection, v: &dyn Volumes, limit: u32) -> Result<Vec<HistoryEntry>> {
    let online = online_sources(c, v)?;
    let limit = limit.min(MAX_LIMIT);
    let log: Vec<(Id, i64, i64, f64)> = c
        .prepare_cached(&format!(
            "WITH RECURSIVE {TREE_CTE}, {LIB_CTE} \
             SELECT l.item_id, l.started_at, coalesce(l.ended_at, l.started_at), l.position_s \
             FROM play_log l JOIN lib ON lib.item_id = l.item_id \
             ORDER BY l.started_at DESC, l.id DESC LIMIT ?2"
        ))?
        .query_map(params![json_ids(&online), limit], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let ids: Vec<Id> = log.iter().map(|e| e.0).collect();
    let by_id: HashMap<Id, ItemSummary> = summaries_by_ids(c, &online, &ids)?
        .into_iter()
        .map(|s| (s.id, s))
        .collect();
    Ok(log
        .into_iter()
        .filter_map(|(id, started_at, ended_at, position_s)| {
            by_id.get(&id).map(|item| HistoryEntry {
                item: item.clone(),
                started_at,
                ended_at,
                position_s,
            })
        })
        .collect())
}

pub fn taste_profile(c: &Connection) -> Result<String> {
    Ok(db::get_setting(c, TASTE_KEY)?.unwrap_or_default())
}

pub fn set_taste_profile(c: &Connection, text: &str) -> Result<()> {
    db::set_setting(c, TASTE_KEY, Some(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TechInfo;
    use crate::volume::FakeVolumes;
    use tempfile::TempDir;

    struct Env {
        _data: TempDir,
        _drive: TempDir,
        c: Connection,
        v: FakeVolumes,
        /// Online source.
        src: Id,
    }

    fn env() -> Env {
        let data = tempfile::tempdir().unwrap();
        let drive = tempfile::tempdir().unwrap();
        let c = db::open(data.path()).unwrap();
        let v = FakeVolumes(vec![("VOL1".to_string(), drive.path().to_path_buf())]);
        let src = scan::add_source(&c, &v, drive.path()).unwrap().id;
        Env {
            _data: data,
            _drive: drive,
            c,
            v,
            src,
        }
    }

    impl Env {
        fn add(&self, kind: ItemKind, title: &str, parent: Option<Id>) -> Id {
            self.c
                .execute(
                    "INSERT INTO items(kind, title, parent_id, match_state, added_at) \
                     VALUES (?1, ?2, ?3, ?4, 0)",
                    params![enum_str(kind), title, parent, enum_str(MatchState::Matched)],
                )
                .unwrap();
            self.c.last_insert_rowid()
        }

        /// An item with one file on the online source.
        fn with_file(&self, kind: ItemKind, title: &str, parent: Option<Id>) -> Id {
            let id = self.add(kind, title, parent);
            self.file(self.src, id, None);
            id
        }

        fn file(&self, source: Id, item: Id, height: Option<u32>) {
            let tech = height.map(|h| {
                serde_json::to_string(&TechInfo {
                    duration_s: 100.0,
                    width: h * 16 / 9,
                    height: h,
                    video_codec: "hevc".into(),
                    hdr: Some("HDR10".into()),
                    audio: vec![],
                    subtitles: vec![],
                })
                .unwrap()
            });
            self.c
                .execute(
                    "INSERT INTO files(source_id, rel_path, item_id, tech) VALUES (?1, ?2, ?3, ?4)",
                    params![source, format!("f{}.mkv", self.n("files")), item, tech],
                )
                .unwrap();
        }

        fn n(&self, table: &str) -> i64 {
            self.c
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap()
        }

        fn exec(&self, sql: &str) {
            self.c.execute_batch(sql).unwrap();
        }

        fn kind_of(&self, id: Id) -> ItemKind {
            item(&self.c, &self.v, id).unwrap().item.kind
        }

        fn episode(&self, season_id: Id, season: i32, episode: i32, title: &str) -> Id {
            let id = self.with_file(ItemKind::Episode, title, Some(season_id));
            self.exec(&format!(
                "UPDATE items SET season={season}, episode={episode} WHERE id={id}"
            ));
            id
        }

        /// Show with one season of `n` episodes: (show, season, episodes in order).
        fn show(&self, title: &str, n: i32) -> (Id, Id, Vec<Id>) {
            let show = self.add(ItemKind::Show, title, None);
            let season = self.add(ItemKind::Season, "Season 1", Some(show));
            self.exec(&format!("UPDATE items SET season=1 WHERE id={season}"));
            let eps = (1..=n)
                .map(|e| self.episode(season, 1, e, &format!("{title} ep {e}")))
                .collect();
            (show, season, eps)
        }

        fn play(&self, id: Id, pos: f64, dur: f64, watched: bool, at: i64) {
            self.c
                .execute(
                    "INSERT OR REPLACE INTO play_state VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![id, pos, dur, watched, at],
                )
                .unwrap();
        }

        fn search(&self, q: &SearchQuery) -> Vec<String> {
            search(&self.c, &self.v, q)
                .unwrap()
                .into_iter()
                .map(|s| s.title)
                .collect()
        }

        fn text(&self, t: &str) -> Vec<String> {
            self.search(&SearchQuery {
                text: Some(t.into()),
                ..Default::default()
            })
        }

        fn shelf(&self, s: Shelf) -> Vec<String> {
            shelf(&self.c, &self.v, s, 50)
                .unwrap()
                .into_iter()
                .map(|s| s.title)
                .collect()
        }

        /// The summary of `id`, whatever its kind.
        fn summary(&self, id: Id) -> ItemSummary {
            search(
                &self.c,
                &self.v,
                &SearchQuery {
                    kind: Some(self.kind_of(id)),
                    ..Default::default()
                },
            )
            .unwrap()
            .into_iter()
            .find(|s| s.id == id)
            .unwrap()
        }
    }

    #[test]
    fn text_search_by_title_person_prefix_and_hostile_input() {
        let e = env();
        let m = e.with_file(ItemKind::Movie, "Seven Samurai", None);
        e.exec(&format!(
            "UPDATE items SET people='[{{\"name\":\"Takashi Shimura\",\"role\":\"cast\"}}]' WHERE id={m}"
        ));
        e.with_file(ItemKind::Movie, "Paprika", None);
        assert_eq!(e.text("samurai"), ["Seven Samurai"]);
        assert_eq!(e.text("shimura"), ["Seven Samurai"]);
        assert_eq!(e.text("sev"), ["Seven Samurai"]);
        assert_eq!(e.text("sev samu"), ["Seven Samurai"]);
        assert_eq!(e.text("sev\"*"), ["Seven Samurai"]);
        assert_eq!(e.text("\"sev"), ["Seven Samurai"]);
        // Operators are plain words: every word must match.
        assert!(e.text("seven OR paprika").is_empty());
        // Nothing searchable left: no text restriction at all.
        assert_eq!(e.text("*\"").len(), 2);
        assert!(e.text("zzz").is_empty());
    }

    #[test]
    fn text_hit_on_an_episode_returns_its_show_once() {
        let e = env();
        let (show, _, eps) = e.show("Dark Waters", 2);
        e.exec(&format!(
            "UPDATE items SET title='The Lighthouse' WHERE id={}; \
             UPDATE items SET title='Lighthouse Return' WHERE id={}",
            eps[0], eps[1]
        ));
        let q = |kind| SearchQuery {
            text: Some("lighthouse".into()),
            kind,
            ..Default::default()
        };
        let r = search(&e.c, &e.v, &q(None)).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].id, r[0].kind), (show, ItemKind::Show));
        // Asking for episodes gives the episodes themselves.
        let r = search(&e.c, &e.v, &q(Some(ItemKind::Episode))).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].show_title.as_deref(), Some("Dark Waters"));
    }

    #[test]
    fn default_kinds_exclude_seasons_and_episodes() {
        let e = env();
        e.show("S", 1);
        e.with_file(ItemKind::Movie, "M", None);
        e.with_file(ItemKind::Other, "O", None);
        assert_eq!(e.search(&SearchQuery::default()), ["M", "O", "S"]);
        let k = |kind| {
            e.search(&SearchQuery {
                kind: Some(kind),
                ..Default::default()
            })
        };
        assert_eq!(k(ItemKind::Season), ["Season 1"]);
        assert_eq!(k(ItemKind::Episode), ["S ep 1"]);
    }

    #[test]
    fn filters_narrow() {
        let e = env();
        let a = e.with_file(ItemKind::Movie, "A", None);
        let b = e.with_file(ItemKind::Movie, "B", None);
        let c = e.with_file(ItemKind::Movie, "C", None);
        e.exec(&format!(
            "UPDATE items SET year=1990, runtime_min=80, genres='[\"Drama\"]' WHERE id={a};\
             UPDATE items SET year=2000, runtime_min=120, genres='[\"Comedy\",\"Drama\"]', loved=1 WHERE id={b};\
             UPDATE items SET year=2010, runtime_min=200 WHERE id={c};"
        ));
        // Probed files sort after the unprobed one only by id; the first with tech counts.
        e.file(e.src, c, Some(2160));
        e.file(e.src, a, Some(720));
        e.play(a, 0.0, 0.0, true, 1);
        let q = |f: fn(&mut SearchQuery)| {
            let mut q = SearchQuery::default();
            f(&mut q);
            e.search(&q)
        };
        assert_eq!(q(|q| q.genre = Some("drama".into())), ["A", "B"]);
        assert_eq!(q(|q| q.year_min = Some(2000)), ["B", "C"]);
        assert_eq!(q(|q| q.year_max = Some(2000)), ["A", "B"]);
        assert_eq!(q(|q| q.runtime_max_min = Some(120)), ["A", "B"]);
        assert_eq!(q(|q| q.watched = Some(true)), ["A"]);
        assert_eq!(q(|q| q.watched = Some(false)), ["B", "C"]);
        assert_eq!(q(|q| q.loved = Some(true)), ["B"]);
        assert_eq!(q(|q| q.min_height = Some(1080)), ["C"]);
        assert_eq!(q(|q| q.min_height = Some(720)), ["A", "C"]);
        assert_eq!(q(|q| q.sort = Some(Sort::Year)), ["C", "B", "A"]);
        assert_eq!(q(|q| q.sort = Some(Sort::Runtime)), ["A", "B", "C"]);
        assert_eq!(
            q(|q| {
                q.limit = Some(1);
                q.offset = Some(1);
            }),
            ["B"]
        );
    }

    #[test]
    fn summary_carries_tech_from_first_file_with_tech() {
        let e = env();
        let m = e.with_file(ItemKind::Movie, "M", None);
        e.file(e.src, m, Some(1080));
        let (show, _, eps) = e.show("S", 2);
        e.file(e.src, eps[1], Some(2160));
        let m = e.summary(m);
        assert_eq!((m.height, m.hdr.as_deref()), (Some(1080), Some("HDR10")));
        assert_eq!(e.summary(show).height, Some(2160));
    }

    #[test]
    fn items_without_files_are_hidden_but_item_returns_them() {
        let e = env();
        let ghost = e.add(ItemKind::Movie, "Ghost", None);
        e.with_file(ItemKind::Movie, "Real", None);
        let empty_show = e.add(ItemKind::Show, "Empty", None);
        assert_eq!(e.search(&SearchQuery::default()), ["Real"]);
        assert!(e.text("ghost").is_empty());
        assert_eq!(item(&e.c, &e.v, ghost).unwrap().item.title, "Ghost");
        assert!(!item(&e.c, &e.v, empty_show).unwrap().available);
        // A removed source leaves its items hidden too.
        scan::remove_source(&e.c, e.src).unwrap();
        assert!(e.search(&SearchQuery::default()).is_empty());
    }

    #[test]
    fn offline_source_marks_items_unavailable() {
        let e = env();
        e.exec("INSERT INTO sources(id, path, volume_id, last_path) VALUES (99, '', 'GONE', 'Z:')");
        let on = e.with_file(ItemKind::Movie, "On", None);
        let off = e.add(ItemKind::Movie, "Off", None);
        e.file(99, off, None);
        let both = e.with_file(ItemKind::Movie, "Both", None);
        e.file(99, both, None);
        let avail = |a| {
            e.search(&SearchQuery {
                available: Some(a),
                ..Default::default()
            })
        };
        assert_eq!(avail(true), ["Both", "On"]);
        assert_eq!(avail(false), ["Off"]);
        assert_eq!(e.search(&SearchQuery::default()).len(), 3);
        assert!(!e.summary(off).available);
        assert!(item(&e.c, &e.v, on).unwrap().available);
        let d = item(&e.c, &e.v, off).unwrap();
        assert!(!d.available);
        assert_eq!(d.files.len(), 1);
    }

    #[test]
    fn show_rollups_for_watched_and_progress() {
        let e = env();
        let (show, season, eps) = e.show("S", 4);
        assert!(!e.summary(show).watched);
        assert_eq!(e.summary(show).progress, Some(0.0));
        e.play(eps[0], 100.0, 100.0, true, 1);
        assert_eq!(e.summary(show).progress, Some(0.25));
        assert_eq!(e.summary(season).progress, Some(0.25));
        for &id in &eps[1..] {
            e.play(id, 100.0, 100.0, true, 1);
        }
        assert!(e.summary(show).watched);
        assert_eq!(e.summary(show).progress, Some(1.0));
        let watched = SearchQuery {
            watched: Some(true),
            ..Default::default()
        };
        assert_eq!(e.search(&watched), ["S"]);
        // A movie's progress is position over duration.
        let m = e.with_file(ItemKind::Movie, "M", None);
        e.play(m, 30.0, 120.0, false, 1);
        assert_eq!(e.summary(m).progress, Some(0.25));
    }

    #[test]
    fn set_watched_on_show_marks_every_episode_and_unmark_resets() {
        let e = env();
        let (show, _, eps) = e.show("S", 3);
        e.play(eps[0], 40.0, 100.0, false, 5);
        set_watched(&e.c, show, true).unwrap();
        for &id in &eps {
            assert!(item(&e.c, &e.v, id).unwrap().play.unwrap().watched);
        }
        let fresh = item(&e.c, &e.v, eps[2]).unwrap().play.unwrap();
        assert_eq!((fresh.position_s, fresh.duration_s), (0.0, 0.0));
        assert!(fresh.last_played_at > 0);
        assert_eq!(e.n("play_log"), 0);
        // Only episodes get a play state.
        assert_eq!(e.n("play_state"), 3);
        set_watched(&e.c, show, false).unwrap();
        let p = item(&e.c, &e.v, eps[0]).unwrap().play.unwrap();
        assert_eq!((p.watched, p.position_s), (false, 0.0));
        assert_eq!(e.n("play_state"), 3);
        // Unmarking an item with no row creates none.
        let m = e.with_file(ItemKind::Movie, "M", None);
        set_watched(&e.c, m, false).unwrap();
        assert_eq!(e.n("play_state"), 3);
        assert!(matches!(
            set_watched(&e.c, 9999, true),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn next_episode_orders_specials_last() {
        let e = env();
        let show = e.add(ItemKind::Show, "S", None);
        let s0 = e.add(ItemKind::Season, "Specials", Some(show));
        let s1 = e.add(ItemKind::Season, "Season 1", Some(show));
        let s2 = e.add(ItemKind::Season, "Season 2", Some(show));
        let sp = e.episode(s0, 0, 1, "Special");
        let a = e.episode(s1, 1, 1, "a");
        let b = e.episode(s1, 1, 2, "b");
        let c = e.episode(s2, 2, 1, "c");
        // An episode without a file is never offered.
        let no_file = e.add(ItemKind::Episode, "no file", Some(s1));
        e.exec(&format!(
            "UPDATE items SET season=1, episode=0 WHERE id={no_file}"
        ));
        let next = || next_episode(&e.c, show).unwrap();
        assert_eq!(next(), Some(a));
        e.play(a, 0.0, 0.0, true, 1);
        assert_eq!(next(), Some(b));
        e.play(b, 0.0, 0.0, true, 1);
        assert_eq!(next(), Some(c));
        e.play(c, 0.0, 0.0, true, 1);
        assert_eq!(next(), Some(sp));
        e.play(sp, 0.0, 0.0, true, 1);
        // An episode with no season comes after the specials.
        let loose = e.with_file(ItemKind::Episode, "loose", Some(show));
        assert_eq!(next(), Some(loose));
        e.play(loose, 0.0, 0.0, true, 1);
        assert_eq!(next(), None);
    }

    #[test]
    fn rollups_ignore_episodes_without_files() {
        let e = env();
        let (show, season, eps) = e.show("S", 2);
        let missing = e.add(ItemKind::Episode, "Missing", Some(season));
        assert!(!e.summary(show).watched);
        e.play(eps[0], 100.0, 100.0, true, 1);
        assert_eq!(e.summary(show).progress, Some(0.5));
        e.play(eps[1], 100.0, 100.0, true, 1);
        assert!(e.summary(show).watched);
        assert_eq!(e.summary(show).progress, Some(1.0));
        assert!(e.summary(season).watched);
        assert!(item(&e.c, &e.v, missing).is_ok());
    }

    #[test]
    fn continue_watching_shelf() {
        let e = env();
        let m1 = e.with_file(ItemKind::Movie, "Older", None);
        let m2 = e.with_file(ItemKind::Movie, "Newer", None);
        let done = e.with_file(ItemKind::Movie, "Done", None);
        let barely = e.with_file(ItemKind::Movie, "Barely started", None);
        e.with_file(ItemKind::Movie, "Untouched", None);
        e.play(m1, 50.0, 100.0, false, 10);
        e.play(m2, 50.0, 100.0, false, 20);
        e.play(done, 95.0, 100.0, false, 30);
        e.play(barely, 0.0, 100.0, false, 40);
        // A show whose latest played episode is finished offers the next episode.
        let (_, _, eps) = e.show("Series", 3);
        e.play(eps[0], 100.0, 100.0, true, 15);
        // A show with an episode in progress lists that episode only.
        let (_, _, eps2) = e.show("Other", 3);
        e.play(eps2[0], 100.0, 100.0, true, 5);
        e.play(eps2[1], 10.0, 100.0, false, 25);
        // A fully watched show lists nothing.
        let (_, _, eps3) = e.show("Finished", 1);
        e.play(eps3[0], 100.0, 100.0, true, 50);
        assert_eq!(
            e.shelf(Shelf::ContinueWatching),
            ["Other ep 2", "Newer", "Series ep 2", "Older"]
        );
    }

    #[test]
    fn recently_added_unwatched_and_loved_shelves() {
        let e = env();
        let old = e.with_file(ItemKind::Movie, "Old", None);
        let new = e.with_file(ItemKind::Movie, "New", None);
        let seen = e.with_file(ItemKind::Movie, "Seen", None);
        let (show, _, eps) = e.show("Series", 2);
        e.exec(&format!(
            "UPDATE items SET added_at=100 WHERE id={old};\
             UPDATE items SET added_at=300 WHERE id={new};\
             UPDATE items SET added_at=200, loved=1 WHERE id={seen};\
             UPDATE items SET added_at=150 WHERE id={show};\
             UPDATE items SET added_at=250 WHERE id={};",
            eps[1]
        ));
        e.play(seen, 0.0, 0.0, true, 1);
        // The show rises to the time of its newest episode and appears once.
        assert_eq!(
            e.shelf(Shelf::RecentlyAdded),
            ["New", "Series", "Seen", "Old"]
        );
        assert_eq!(e.shelf(Shelf::Unwatched), ["New", "Old"]);
        e.play(old, 5.0, 100.0, false, 1);
        assert_eq!(e.shelf(Shelf::Unwatched), ["New"]);
        assert_eq!(e.shelf(Shelf::Loved), ["Seen"]);
        let two = shelf(&e.c, &e.v, Shelf::RecentlyAdded, 2).unwrap();
        assert_eq!(two.len(), 2);
    }

    #[test]
    fn item_details_children_and_not_found() {
        let e = env();
        let (show, season, eps) = e.show("S", 2);
        let d = item(&e.c, &e.v, show).unwrap();
        assert_eq!(d.children.len(), 1);
        assert_eq!(d.children[0].id, season);
        assert!(d.available);
        assert!(d.files.is_empty());
        let d = item(&e.c, &e.v, season).unwrap();
        assert_eq!(d.children.iter().map(|c| c.id).collect::<Vec<_>>(), eps);
        let d = item(&e.c, &e.v, eps[0]).unwrap();
        assert_eq!(d.files.len(), 1);
        assert!(d.children.is_empty());
        assert!(matches!(item(&e.c, &e.v, 424242), Err(Error::NotFound(_))));
    }

    #[test]
    fn library_counts_count_items_with_files() {
        let e = env();
        let unsure = e.with_file(ItemKind::Movie, "Unsure", None);
        e.with_file(ItemKind::Movie, "Fine", None);
        e.with_file(ItemKind::Other, "Clip", None);
        e.add(ItemKind::Movie, "No file", None);
        e.show("Show", 2);
        e.add(ItemKind::Show, "Empty show", None);
        e.exec(&format!(
            "UPDATE items SET match_state='{}' WHERE id={unsure}",
            enum_str(MatchState::NeedsReview)
        ));
        e.exec("UPDATE files SET probed = 1 WHERE id = 1");
        let n = library_counts(&e.c, &e.v).unwrap();
        assert_eq!(
            n,
            LibraryCounts {
                movies: 2,
                shows: 1,
                other: 1,
                needs_review: 1,
                unprobed: 4,
            }
        );
    }

    #[test]
    fn needs_review_lists_flagged_items_with_files() {
        let e = env();
        let a = e.with_file(ItemKind::Movie, "Unsure", None);
        e.with_file(ItemKind::Movie, "Fine", None);
        let hidden = e.add(ItemKind::Movie, "No file", None);
        e.exec(&format!(
            "UPDATE items SET match_state='{}' WHERE id IN ({a}, {hidden})",
            enum_str(MatchState::NeedsReview)
        ));
        let r = needs_review(&e.c, &e.v).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].item.id, a);
    }

    #[test]
    fn history_newest_first_skips_hidden_items() {
        let e = env();
        let a = e.with_file(ItemKind::Movie, "A", None);
        let b = e.with_file(ItemKind::Movie, "B", None);
        let ghost = e.add(ItemKind::Movie, "Ghost", None);
        e.exec(&format!(
            "INSERT INTO play_log(item_id, started_at, ended_at, position_s) VALUES \
               ({a}, 100, 200, 60), ({b}, 300, NULL, 5), ({a}, 400, 450, 30), ({ghost}, 500, 510, 1)"
        ));
        let h = history(&e.c, &e.v, 10).unwrap();
        let got: Vec<_> = h
            .iter()
            .map(|x| {
                (
                    x.item.title.as_str(),
                    x.started_at,
                    x.ended_at,
                    x.position_s,
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                ("A", 400, 450, 30.0),
                ("B", 300, 300, 5.0),
                ("A", 100, 200, 60.0)
            ]
        );
        assert_eq!(history(&e.c, &e.v, 1).unwrap().len(), 1);
    }

    #[test]
    fn loved_and_taste_profile() {
        let e = env();
        let m = e.with_file(ItemKind::Movie, "M", None);
        set_loved(&e.c, m, true).unwrap();
        assert!(item(&e.c, &e.v, m).unwrap().item.loved);
        set_loved(&e.c, m, false).unwrap();
        assert!(!item(&e.c, &e.v, m).unwrap().item.loved);
        assert!(matches!(set_loved(&e.c, 77, true), Err(Error::NotFound(_))));
        assert_eq!(taste_profile(&e.c).unwrap(), "");
        set_taste_profile(&e.c, "likes slow cinema").unwrap();
        assert_eq!(taste_profile(&e.c).unwrap(), "likes slow cinema");
    }

    /// Run with `cargo test --release -p exhibit-core scale_5000_items -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn scale_5000_items() {
        let e = env();
        let tx = e.c.unchecked_transaction().unwrap();
        let tech = serde_json::to_string(&TechInfo {
            duration_s: 100.0,
            width: 1920,
            height: 1080,
            video_codec: "hevc".into(),
            hdr: None,
            audio: vec![],
            subtitles: vec![],
        })
        .unwrap();
        let mut n = 0;
        let mut add = |kind: ItemKind, title: String, parent: Option<Id>, state: MatchState| {
            n += 1;
            tx.execute(
                "INSERT INTO items(kind, title, parent_id, match_state, added_at, year) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 1990 + ?5 % 30)",
                params![enum_str(kind), title, parent, enum_str(state), n],
            )
            .unwrap();
            let id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO files(source_id, rel_path, item_id, tech) VALUES (?1, ?2, ?3, ?4)",
                params![e.src, format!("f{id}.mkv"), id, tech],
            )
            .unwrap();
            id
        };
        let movies: Vec<Id> = (0..4000)
            .map(|i| {
                add(
                    ItemKind::Movie,
                    format!("Film {i} Harbor"),
                    None,
                    MatchState::Matched,
                )
            })
            .collect();
        let mut first_eps = Vec::new();
        let mut a_show = 0;
        for s in 0..50 {
            let show = add(
                ItemKind::Show,
                format!("Series {s}"),
                None,
                MatchState::Matched,
            );
            a_show = show;
            for sn in 1..=2 {
                let season = add(
                    ItemKind::Season,
                    format!("Season {sn}"),
                    Some(show),
                    MatchState::Matched,
                );
                for ep in 1..=10 {
                    let id = add(
                        ItemKind::Episode,
                        format!("Series {s} S{sn}E{ep}"),
                        Some(season),
                        MatchState::Matched,
                    );
                    tx.execute(
                        "UPDATE items SET season=?2, episode=?3 WHERE id=?1",
                        params![id, sn, ep],
                    )
                    .unwrap();
                    if sn == 1 && ep == 1 {
                        first_eps.push(id);
                    }
                }
            }
        }
        for i in 0..300 {
            add(
                ItemKind::Movie,
                format!("Unsure {i}"),
                None,
                MatchState::NeedsReview,
            );
        }
        let play = |id: Id, pos: f64, watched: bool, at: i64| {
            tx.execute(
                "INSERT INTO play_state VALUES (?1, ?2, 100, ?3, ?4)",
                params![id, pos, watched, at],
            )
            .unwrap();
        };
        for (i, &m) in movies.iter().take(100).enumerate() {
            play(m, 50.0, false, i as i64);
        }
        for (i, &m) in movies.iter().skip(100).take(50).enumerate() {
            play(m, 100.0, true, i as i64);
        }
        for (i, &id) in first_eps.iter().enumerate() {
            play(id, 100.0, true, 1000 + i as i64);
        }
        // Shows and seasons hold no file of their own, only their episodes do.
        tx.execute(
            "DELETE FROM files WHERE item_id IN (SELECT id FROM items WHERE kind IN (?1, ?2))",
            params![enum_str(ItemKind::Show), enum_str(ItemKind::Season)],
        )
        .unwrap();
        tx.commit().unwrap();
        assert_eq!(e.n("items"), 4000 + 50 * 23 + 300);

        let time = |name: &str, f: &dyn Fn()| {
            let t = std::time::Instant::now();
            f();
            let d = t.elapsed();
            eprintln!("{name}: {d:?}");
            assert!(d.as_millis() < 250, "{name} took {d:?}");
        };
        time("search (no text)", &|| {
            assert_eq!(
                search(&e.c, &e.v, &SearchQuery::default()).unwrap().len(),
                50
            );
        });
        time("search (two words)", &|| {
            let q = SearchQuery {
                text: Some("film harb".into()),
                ..Default::default()
            };
            assert_eq!(search(&e.c, &e.v, &q).unwrap().len(), 50);
        });
        for s in [
            Shelf::ContinueWatching,
            Shelf::RecentlyAdded,
            Shelf::Unwatched,
            Shelf::Loved,
        ] {
            time(&format!("shelf {s:?}"), &|| {
                shelf(&e.c, &e.v, s, 50).unwrap();
            });
        }
        time("item (show)", &|| {
            assert_eq!(item(&e.c, &e.v, a_show).unwrap().children.len(), 2);
        });
        time("needs_review", &|| {
            assert_eq!(needs_review(&e.c, &e.v).unwrap().len(), 300);
        });
    }
}
