//! Read side of the library: search, shelves, details, history and the taste profile, plus
//! the few small writes the UI makes (watched, loved). Every listing hides items that have
//! no file anywhere beneath them, unless it asks for the watchlist films the owner does not
//! have yet; `item` still returns them.

use crate::activity::{
    self, Activity, loved_sql, my_rating_sql, on_watchlist_sql, watched_sql, watchlisted_on_sql,
};
use crate::db;
use crate::error::{Detail, Error, Result};
use crate::filter::{self, Filters};
use crate::lang;
use crate::model::{Id, Item, ItemKind, MatchState, MediaFile, PlayState, WATCHED_FRACTION};
use crate::reception::{self, AwardEntry, SourcedScore, public_rating_sql};
use crate::rows::{
    FILE_COLUMNS, LOCAL_GENRES, LOCAL_POSTER, LOCAL_TITLE, PLAY_STATE_COLUMNS, SUMMARY_COLUMNS,
    enum_str, item_from_row, local_item_columns, media_file_from_row, play_state_from_row,
    summary_from_row,
};
use crate::scan;
use crate::volume::Volumes;
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use std::collections::{HashMap, HashSet};

const DEFAULT_LIMIT: u32 = 50;
pub(crate) const MAX_LIMIT: u32 = 500;
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
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub hdr: Option<String>,
    pub watched: bool,
    pub progress: Option<f32>,
    pub loved: bool,
    pub available: bool,
    pub match_state: MatchState,
    pub genres: Vec<String>,
    /// The item's first file (lowest id), relative to its source, as stored; `None` for an item
    /// with no file of its own (a show or season).
    pub path: Option<String>,
    /// The work has a file; false for a film on the owner's watchlist they do not have yet.
    /// `available` says whether a file is online now.
    pub in_library: bool,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ItemDetail {
    pub item: Item,
    pub files: Vec<MediaFile>,
    pub play: Option<PlayState>,
    pub children: Vec<ItemSummary>,
    pub available: bool,
    /// For a show, the episode to play next; `None` for other kinds or when all are watched.
    pub next_episode: Option<ItemSummary>,
    pub activity: Activity,
    /// The public's scores, from switched-on services.
    pub reception: Vec<SourcedScore>,
    /// Wins first, the most prominent first.
    pub awards: Vec<AwardEntry>,
    /// TMDB's keywords, most specific first.
    pub keywords: Vec<String>,
}

/// A listing's order. Works without the value sorted by (no year) come last either way; ties go
/// by title.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    Title,
    Year,
    Added,
    Runtime,
    /// When it was last played; for a show, any of its episodes.
    Played,
    /// The owner's newest rating from a switched-on service, over its scale.
    MyRating,
    /// The public's: the mean of the scores switched-on services hold, from 0 to 100.
    Rating,
}

impl Sort {
    /// The direction a listing takes unless asked: titles A to Z, runtimes shortest first, the
    /// rest newest or highest first.
    pub fn descending(self) -> bool {
        matches!(
            self,
            Sort::Year | Sort::Added | Sort::Played | Sort::MyRating | Sort::Rating
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SearchQuery {
    pub text: Option<String>,
    pub kind: Option<ItemKind>,
    #[serde(flatten)]
    pub filters: Filters,
    pub sort: Option<Sort>,
    /// `None` takes the sort's own direction (`Sort::descending`).
    pub descending: Option<bool>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    /// Also list the films on the owner's watchlist they do not have yet, after the ones they
    /// have.
    pub with_wanted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shelf {
    ContinueWatching,
    RecentlyAdded,
    Unwatched,
    Loved,
    /// On the owner's watchlist and not watched: the films they have, then the others, each
    /// newest on the watchlist first.
    Watchlist,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct HistoryEntry {
    pub item: ItemSummary,
    pub started_at: i64,
    pub ended_at: i64,
    pub position_s: f64,
}

/// `tree(root_id, id)`: every item paired with itself and each of its descendants, read from
/// `item_tree`, which triggers keep.
pub(crate) const TREE_CTE: &str = "tree(root_id, id) AS (SELECT root_id, id FROM item_tree)";

/// `lib(item_id, avail)`: items with at least one file in their subtree; `avail` is 1 when
/// one of those files is on a source whose id is in the JSON array bound as `?1`.
pub(crate) const LIB_CTE: &str = "lib(item_id, avail) AS (SELECT t.root_id, \
     max(f.source_id IN (SELECT value FROM json_each(?1))) \
     FROM tree t JOIN files f ON f.item_id = t.id GROUP BY t.root_id)";

/// Bound parameters of one statement; `bind` returns the `?N` placeholder for a value.
/// `?1` is always the JSON array of online source ids, `?2` the language in effect.
pub(crate) struct Params(pub(crate) Vec<Value>);

impl Params {
    pub(crate) fn new(c: &Connection, online_sources: &[Id]) -> Result<Params> {
        Ok(Params(vec![
            Value::Text(json_ids(online_sources)),
            Value::Text(lang::language(c)?.to_string()),
        ]))
    }

    pub(crate) fn bind(&mut self, v: impl Into<Value>) -> String {
        self.0.push(v.into());
        format!("?{}", self.0.len())
    }
}

pub(crate) fn json_ids(ids: &[Id]) -> String {
    serde_json::to_string(ids).unwrap_or_else(|_| "[]".into())
}

/// Ids of the sources that are online right now, computed once per public call.
pub(crate) fn online_sources(c: &Connection, v: &dyn Volumes) -> Result<Vec<Id>> {
    Ok(scan::list_sources(c, v)?
        .into_iter()
        .filter(|s| s.online)
        .map(|s| s.id)
        .collect())
}

/// Runs the summary listing: one row per visible item with roll-ups for shows and seasons,
/// narrowed by `wheres` (conditions on the alias `b`) and ordered by `order`. `wanted` also
/// lists the watchlist films with no file.
fn list(
    c: &Connection,
    p: Params,
    wanted: bool,
    wheres: &[String],
    order: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<ItemSummary>> {
    // First the page's ids, which computes only the columns its conditions and order read;
    // then every column, for those rows alone.
    let ids: Vec<Id> = {
        let mut p = Params(p.0.clone());
        let (limit_p, offset_p) = (p.bind(limit as i64), p.bind(offset as i64));
        let sql = format!(
            "{} ORDER BY {order} LIMIT {limit_p} OFFSET {offset_p}",
            listing("b.id", wanted, wheres)
        );
        c.prepare_cached(&sql)?
            .query_map(params_from_iter(p.0.iter()), |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    rows_by_ids(c, p, wanted, &ids)
}

/// The summaries of `ids` that the listing (`wanted` as for `list`) shows, in their order.
fn rows_by_ids(
    c: &Connection,
    mut p: Params,
    wanted: bool,
    ids: &[Id],
) -> Result<Vec<ItemSummary>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let cond = format!(
        "b.id IN (SELECT value FROM json_each({}))",
        p.bind(json_ids(ids))
    );
    let sql = listing(SUMMARY_COLUMNS, wanted, &[cond]);
    let mut by_id: HashMap<Id, ItemSummary> = c
        .prepare_cached(&sql)?
        .query_map(params_from_iter(p.0.iter()), summary_from_row)?
        .map(|r| r.map(|s| (s.id, s)))
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

/// The summary listing's SQL: `select` over the rows `b` that meet `wheres`.
fn listing(select: &str, wanted: bool, wheres: &[String]) -> String {
    let container = "i.kind IN ('show', 'season')";
    // Episodes at or below the item that have a file, all of them and the watched ones.
    let has_file = "EXISTS (SELECT 1 FROM files ff WHERE ff.item_id = e.id)";
    let total = format!(
        "(SELECT count(*) FROM tree t JOIN items e ON e.id = t.id \
         WHERE t.root_id = i.id AND e.kind = 'episode' AND {has_file})"
    );
    let seen = format!(
        "(SELECT count(*) FROM tree t JOIN items e ON e.id = t.id \
         LEFT JOIN play_state q ON q.item_id = e.id \
         WHERE t.root_id = i.id AND e.kind = 'episode' AND {} AND {has_file})",
        watched_sql("e", "q")
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
    format!(
        "WITH RECURSIVE {TREE_CTE} SELECT {select} FROM (\
           SELECT i.id, i.kind, i.parent_id, {LOCAL_TITLE} AS title, i.year, i.season, i.episode, \
             (SELECT coalesce(nullif(st.title, ''), s.title) FROM tree t \
                JOIN items s ON s.id = t.root_id \
                LEFT JOIN item_texts st ON st.item_id = s.id AND st.lang = ?2 \
                WHERE t.id = i.id AND t.root_id <> i.id AND s.kind = 'show') AS show_title, \
             {LOCAL_POSTER} AS poster, i.backdrop, i.runtime_min, {width} AS width, {height} AS height, {hdr} AS hdr, \
             CASE WHEN {container} THEN {total} > 0 AND {seen} = {total} \
                  ELSE {watched} END AS watched, \
             CASE WHEN {container} THEN 1.0 * {seen} / nullif({total}, 0) \
                  ELSE CASE WHEN p.duration_s > 0 \
                       THEN min(1.0, p.position_s / p.duration_s) END END AS progress, \
             {loved} AS loved, {available} AS available, i.match_state, \
             {LOCAL_GENRES} AS genres, i.added_at, \
             nullif((SELECT max(q.last_played_at) FROM tree t JOIN play_state q ON q.item_id = t.id \
                WHERE t.root_id = i.id), 0) AS played_at, \
             (SELECT f.rel_path FROM files f WHERE f.item_id = i.id ORDER BY f.id LIMIT 1) AS path, \
             {in_library} AS in_library, i.tmdb_id, {my_rating} AS my_rating \
           FROM items i \
           LEFT JOIN item_texts tx ON tx.item_id = i.id AND tx.lang = ?2 \
           LEFT JOIN play_state p ON p.item_id = i.id {admit}) b \
         WHERE {where_sql}",
        width = tech("width"),
        height = tech("height"),
        hdr = tech("hdr"),
        in_library = IN_LIBRARY,
        available = AVAILABLE,
        watched = watched_sql("i", "p"),
        loved = loved_sql("i"),
        my_rating = my_rating_sql("i"),
        admit = if wanted {
            format!(
                "WHERE {IN_LIBRARY} OR (i.wanted = 1 AND {})",
                on_watchlist_sql("i", &watched_sql("i", "p"))
            )
        } else {
            format!("WHERE {IN_LIBRARY}")
        },
    )
}

/// The work `i` has a file at or beneath it: it is in the library. Asked of each row, so a
/// page computes it for its own rows only.
const IN_LIBRARY: &str = "EXISTS (SELECT 1 FROM item_tree t JOIN files f ON f.item_id = t.id \
     WHERE t.root_id = i.id)";

/// `IN_LIBRARY` for the work whose id is the SQL `id`: a lookup for a few rows, where `LIB_CTE`
/// would add up the whole library first.
pub(crate) fn in_library(id: &str) -> String {
    IN_LIBRARY.replace("i.id", id)
}

/// 1 when one of the files at or beneath `i` is on a source whose id is in the JSON array
/// bound as `?1`, else 0.
const AVAILABLE: &str = "coalesce((SELECT max(f.source_id IN (SELECT value FROM json_each(?1))) \
     FROM item_tree t JOIN files f ON f.item_id = t.id WHERE t.root_id = i.id), 0)";

/// Summaries of the visible items among `ids`, in the order of `ids`.
pub(crate) fn summaries_by_ids(
    c: &Connection,
    online: &[Id],
    ids: &[Id],
) -> Result<Vec<ItemSummary>> {
    rows_by_ids(c, Params::new(c, online)?, false, ids)
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

/// Listings show movies, shows and other items unless a kind is asked for.
pub(crate) const DEFAULT_KINDS: &str = "b.kind IN ('movie', 'show', 'other')";

/// Shelves are Home's, and Home is for films and shows: Other has its own wall.
const SHELF_KINDS: &str = "b.kind IN ('movie', 'show')";

pub fn search(c: &Connection, v: &dyn Volumes, q: &SearchQuery) -> Result<Vec<ItemSummary>> {
    let mut p = Params::new(c, &online_sources(c, v)?)?;
    let mut w = vec![match q.kind {
        Some(k) => format!("b.kind = {}", p.bind(enum_str(k))),
        None => DEFAULT_KINDS.to_string(),
    }];
    if let Some(m) = q.text.as_deref().and_then(fts_query) {
        // A hit on an episode or season counts for every item above it.
        w.push(format!(
            "b.id IN (SELECT t.root_id FROM tree t WHERE t.id IN \
             (SELECT rowid FROM items_fts WHERE items_fts MATCH {}))",
            p.bind(m)
        ));
    }
    w.extend(filter::conditions(&q.filters, &mut p));
    let sort = q.sort.unwrap_or(Sort::Title);
    let dir = if q.descending.unwrap_or(sort.descending()) {
        "DESC"
    } else {
        "ASC"
    };
    let by_title = "b.title COLLATE NOCASE, b.id";
    let by = match sort {
        Sort::Title => format!("b.title COLLATE NOCASE {dir}, b.id {dir}"),
        Sort::Year => format!("b.year IS NULL, b.year {dir}, {by_title}"),
        Sort::Added => format!("b.added_at {dir}, {by_title}"),
        Sort::Runtime => format!("b.runtime_min IS NULL, b.runtime_min {dir}, {by_title}"),
        Sort::Played => format!("b.played_at IS NULL, b.played_at {dir}, {by_title}"),
        Sort::MyRating => format!("b.my_rating IS NULL, b.my_rating {dir}, {by_title}"),
        // Unrated last either way; the rating is read only when sorting by it.
        Sort::Rating => format!(
            "coalesce({}, {}) {dir}, {by_title}",
            public_rating_sql("b"),
            if dir == "DESC" { "-1" } else { "1000" }
        ),
    };
    // Films the owner does not have come after the ones they do.
    let wanted = q.with_wanted || q.filters.watchlist == Some(true);
    let order = if wanted {
        format!("NOT b.in_library, {by}")
    } else {
        by
    };
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    list(c, p, wanted, &w, &order, limit, q.offset.unwrap_or(0))
}

/// Full details of one item, hidden or not. Children are the seasons of a show or the
/// episodes of a season (only those with files).
pub fn item(c: &Connection, v: &dyn Volumes, id: Id) -> Result<ItemDetail> {
    item_with(c, &online_sources(c, v)?, id, true)
}

/// `item` with the online sources already known; `children` false leaves the list empty.
fn item_with(c: &Connection, online: &[Id], id: Id, children: bool) -> Result<ItemDetail> {
    let item = c
        .prepare_cached(&format!(
            "SELECT {} FROM items i \
             LEFT JOIN item_texts tx ON tx.item_id = i.id AND tx.lang = ?2 WHERE i.id = ?1",
            local_item_columns()
        ))?
        .query_row(params![id, lang::language(c)?], item_from_row)
        .optional()?
        .ok_or_else(|| {
            Error::NotFound(Detail::coded("item.not_found", format!("item {id}")).with("id", id))
        })?;
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
        let mut p = Params::new(c, online)?;
        let cond = format!("b.parent_id = {}", p.bind(id));
        list(
            c,
            p,
            false,
            &[cond],
            "b.season IS NULL, b.season, b.episode IS NULL, b.episode, b.title COLLATE NOCASE, b.id",
            u32::MAX,
            0,
        )?
    } else {
        Vec::new()
    };
    let next_episode = match item.kind {
        ItemKind::Show => match next_episode(c, id)? {
            Some(next) => summaries_by_ids(c, online, &[next])?.pop(),
            None => None,
        },
        _ => None,
    };
    let activity = activity::of_item(c, id)?.unwrap_or_default();
    let (reception, awards) = match (item.tmdb_id, item.kind) {
        (Some(tmdb_id), ItemKind::Movie | ItemKind::Show) => {
            let kind = enum_str(item.kind);
            (
                reception::scores_of(c, &kind, tmdb_id)?,
                reception::awards_of(c, &kind, tmdb_id)?,
            )
        }
        _ => (Vec::new(), Vec::new()),
    };
    Ok(ItemDetail {
        item,
        files,
        play,
        children,
        available,
        next_episode,
        activity,
        reception,
        awards,
        keywords: crate::similar::keywords(c, id)?,
    })
}

pub fn shelf(c: &Connection, v: &dyn Volumes, s: Shelf, limit: u32) -> Result<Vec<ItemSummary>> {
    let online = online_sources(c, v)?;
    let limit = limit.min(MAX_LIMIT);
    let (wheres, order, wanted) = match s {
        Shelf::ContinueWatching => return continue_watching(c, &online, limit),
        Shelf::RecentlyAdded => (
            vec![SHELF_KINDS.to_string()],
            // A show rises when an episode arrives.
            "(SELECT max(d.added_at) FROM tree t JOIN items d ON d.id = t.id \
              WHERE t.root_id = b.id) DESC, b.id DESC"
                .to_string(),
            false,
        ),
        Shelf::Unwatched => (
            vec![
                "b.kind = 'movie'".to_string(),
                "NOT b.watched".to_string(),
                "NOT EXISTS (SELECT 1 FROM play_state q WHERE q.item_id = b.id \
                 AND q.position_s > 0)"
                    .to_string(),
            ],
            "b.added_at DESC, b.id DESC".to_string(),
            false,
        ),
        Shelf::Loved => (
            vec![SHELF_KINDS.to_string(), "b.loved = 1".to_string()],
            "b.added_at DESC, b.id DESC".to_string(),
            false,
        ),
        Shelf::Watchlist => (
            vec![SHELF_KINDS.to_string(), on_watchlist_sql("b", "b.watched")],
            format!(
                "NOT b.in_library, {} DESC, b.id DESC",
                watchlisted_on_sql("b")
            ),
            true,
        ),
    };
    list(
        c,
        Params::new(c, &online)?,
        wanted,
        &wheres,
        &order,
        limit,
        0,
    )
}

/// Films and episodes part-way through, plus the next unwatched episode of each show whose
/// latest played episode is finished; newest first.
fn continue_watching(c: &Connection, online: &[Id], limit: u32) -> Result<Vec<ItemSummary>> {
    let mut picks: Vec<(i64, Id)> = c
        .prepare_cached(
            "SELECT p.last_played_at, p.item_id FROM play_state p \
             JOIN items i ON i.id = p.item_id AND i.kind IN ('movie', 'episode') \
             WHERE p.watched = 0 AND p.position_s > 0 AND p.duration_s > 0 \
               AND p.position_s / p.duration_s < ?1 \
             ORDER BY p.last_played_at DESC, p.item_id LIMIT ?2",
        )?
        .query_map(params![WATCHED_FRACTION, limit], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    // Played episodes with their show, latest first; read lazily and stop once `limit`
    // shows have offered a next episode.
    let mut q = c.prepare_cached(&format!(
        "WITH RECURSIVE {TREE_CTE} SELECT t.root_id, p.last_played_at, p.watched \
         FROM tree t JOIN items s ON s.id = t.root_id AND s.kind = 'show' \
         JOIN items e ON e.id = t.id AND e.kind = 'episode' \
         JOIN play_state p ON p.item_id = e.id \
         ORDER BY p.last_played_at DESC, e.season DESC, e.episode DESC"
    ))?;
    let played = q.query_map([], |r| {
        Ok((
            r.get::<_, Id>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, bool>(2)?,
        ))
    })?;
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
    // Only the ids are needed here: each item's record is read below.
    let ids: Vec<Id> = c
        .prepare_cached(
            "SELECT i.id FROM items i WHERE i.match_state = 'needsreview' \
             AND EXISTS (SELECT 1 FROM item_tree t JOIN files f ON f.item_id = t.id \
                         WHERE t.root_id = i.id) \
             ORDER BY i.added_at DESC, i.id DESC",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    ids.into_iter()
        .map(|id| item_with(c, &online, id, false))
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

pub fn library_counts(c: &Connection) -> Result<LibraryCounts> {
    // Items of each kind (or state) with a file at or below them, in one pass.
    let count = |cond: &str| format!("count(DISTINCT CASE WHEN {cond} THEN r.id END)");
    let sql = format!(
        "WITH RECURSIVE {TREE_CTE} SELECT {}, {}, {}, {}, \
         (SELECT count(*) FROM files WHERE probed = 0) FROM items r \
         JOIN tree t ON t.root_id = r.id JOIN files f ON f.item_id = t.id \
         WHERE r.kind IN ('movie', 'show', 'other') OR r.match_state = 'needsreview'",
        count("r.kind = 'movie'"),
        count("r.kind = 'show'"),
        count("r.kind = 'other'"),
        count("r.match_state = 'needsreview'"),
    );
    Ok(c.prepare_cached(&sql)?.query_row([], |r| {
        Ok(LibraryCounts {
            movies: r.get(0)?,
            shows: r.get(1)?,
            other: r.get(2)?,
            needs_review: r.get(3)?,
            unprobed: r.get(4)?,
        })
    })?)
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
    Ok(c.prepare_cached(&format!(
        "WITH RECURSIVE d(id) AS (SELECT ?1 UNION ALL \
             SELECT i.id FROM items i JOIN d ON i.parent_id = d.id) \
         SELECT e.id FROM d JOIN items e ON e.id = d.id AND e.kind = 'episode' \
         LEFT JOIN play_state p ON p.item_id = e.id \
         WHERE (?2 = 0 OR NOT {}) \
           AND EXISTS (SELECT 1 FROM files f WHERE f.item_id = e.id) \
         ORDER BY e.season IS NULL, e.season = 0, e.season, e.episode, e.id LIMIT 1",
        watched_sql("e", "p")
    ))?
    .query_row(params![root, unwatched_only], |r| r.get(0))
    .optional()?)
}

/// Marks an item watched or not; on a show or season it applies to every episode below.
/// It is the owner's choice, which wins over what any service says. Either way the stored
/// position goes back to 0; marking watched creates a play state when there is none.
pub fn set_watched(c: &Connection, id: Id, watched: bool) -> Result<()> {
    let kind: String = c
        .prepare_cached("SELECT kind FROM items WHERE id = ?1")?
        .query_row([id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| {
            Error::NotFound(Detail::coded("item.not_found", format!("item {id}")).with("id", id))
        })?;
    let targets: Vec<Id> = if kind == "show" || kind == "season" {
        c.prepare_cached(
            "WITH RECURSIVE d(id) AS (SELECT ?1 UNION ALL \
                 SELECT i.id FROM items i JOIN d ON i.parent_id = d.id) \
             SELECT i.id FROM d JOIN items i ON i.id = d.id AND i.kind = 'episode'",
        )?
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?
    } else {
        vec![id]
    };
    let tx = db::write_tx(c)?;
    for t in targets {
        tx.prepare_cached("UPDATE items SET watched_choice = ?2 WHERE id = ?1")?
            .execute(params![t, watched])?;
        if watched {
            tx.prepare_cached(
                "INSERT INTO play_state(item_id, position_s, duration_s, watched, last_played_at) \
                 VALUES (?1, 0, 0, 1, ?2) \
                 ON CONFLICT(item_id) DO UPDATE SET watched = 1, position_s = 0",
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

/// The owner's choice, which wins over what any service says.
pub fn set_loved(c: &Connection, id: Id, loved: bool) -> Result<()> {
    let n = c
        .prepare_cached("UPDATE items SET loved_choice = ?2 WHERE id = ?1")?
        .execute(params![id, loved])?;
    if n == 0 {
        return Err(Error::NotFound(
            Detail::coded("item.not_found", format!("item {id}")).with("id", id),
        ));
    }
    Ok(())
}

/// Play sessions, newest first, for items that still have files.
pub fn history(c: &Connection, v: &dyn Volumes, limit: u32) -> Result<Vec<HistoryEntry>> {
    let online = online_sources(c, v)?;
    let limit = limit.min(MAX_LIMIT);
    let log: Vec<(Id, i64, i64, f64)> = c
        .prepare_cached(&format!(
            "SELECT l.item_id, l.started_at, coalesce(l.ended_at, l.started_at), l.position_s \
             FROM play_log l WHERE {} \
             ORDER BY l.started_at DESC, l.id DESC LIMIT ?1",
            in_library("l.item_id")
        ))?
        .query_map(params![limit], |r| {
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
    use crate::filter::{Picture, Resolution, Seen, Sound};
    use crate::model::{AudioTrack, SubTrack, TechInfo};
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

        /// A movie with a file, identified as TMDB film `tmdb_id`.
        fn movie_with_tmdb(&self, title: &str, tmdb_id: i64) -> Id {
            let id = self.with_file(ItemKind::Movie, title, None);
            self.exec(&format!(
                "UPDATE items SET tmdb_id = {tmdb_id} WHERE id = {id}"
            ));
            id
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

        fn filter(&self, filters: Filters) -> Vec<String> {
            self.search(&SearchQuery {
                filters,
                ..Default::default()
            })
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
    fn summaries_carry_genres_and_their_first_files_path() {
        let e = env();
        let m = e.add(ItemKind::Movie, "M", None);
        e.exec(&format!(
            "UPDATE items SET genres='[\"Drama\",\"War\"]' WHERE id={m};\
             INSERT INTO files(source_id, rel_path, item_id) VALUES ({src}, 'Films/b.mkv', {m});\
             INSERT INTO files(source_id, rel_path, item_id) VALUES ({src}, 'Films/a.mkv', {m});",
            src = e.src
        ));
        let (show, _, _) = e.show("S", 1);
        let s = e.summary(m);
        assert_eq!(s.genres, ["Drama", "War"]);
        assert_eq!(s.path.as_deref(), Some("Films/b.mkv"));
        assert_eq!(e.summary(show).path, None);
    }

    #[test]
    fn filters_narrow() {
        let e = env();
        let a = e.with_file(ItemKind::Movie, "A", None);
        let b = e.with_file(ItemKind::Movie, "B", None);
        let c = e.with_file(ItemKind::Movie, "C", None);
        e.exec(&format!(
            "UPDATE items SET year=1990, runtime_min=80, genres='[\"Drama\"]' WHERE id={a};\
             UPDATE items SET year=2000, runtime_min=120, genres='[\"Comedy\",\"Drama\"]', loved_choice=1 WHERE id={b};\
             UPDATE items SET year=2010, runtime_min=200 WHERE id={c};"
        ));
        // Probed files sort after the unprobed one only by id; the first with tech counts.
        e.file(e.src, c, Some(2160));
        e.file(e.src, a, Some(720));
        e.play(a, 0.0, 0.0, true, 1);
        let d = e.with_file(ItemKind::Movie, "D", None);
        e.exec(&format!(
            "UPDATE items SET genres='[\"Comedy\"]' WHERE id={d}"
        ));
        e.play(d, 30.0, 100.0, false, 1);
        let q = |f: fn(&mut SearchQuery)| {
            let mut q = SearchQuery::default();
            f(&mut q);
            e.search(&q)
        };
        fn genres(g: &[&str]) -> Vec<String> {
            g.iter().map(|s| s.to_string()).collect()
        }
        assert_eq!(q(|q| q.filters.genres = vec!["drama".into()]), ["A", "B"]);
        assert_eq!(
            q(|q| q.filters.genres = genres(&["Drama", "Comedy"])),
            ["B"]
        );
        assert_eq!(
            q(|q| {
                q.filters.genres = genres(&["Drama", "Comedy"]);
                q.filters.any_genre = true;
            }),
            ["A", "B", "D"]
        );
        assert_eq!(
            q(|q| q.filters.without_genres = genres(&["Comedy"])),
            ["A", "C"]
        );
        assert_eq!(q(|q| q.filters.year_min = Some(2000)), ["B", "C"]);
        assert_eq!(
            q(|q| {
                q.filters.year_min = Some(1995);
                q.filters.year_max = Some(2005);
            }),
            ["B"]
        );
        assert_eq!(q(|q| q.filters.runtime_max = Some(120)), ["A", "B"]);
        assert_eq!(q(|q| q.filters.runtime_min = Some(120)), ["B", "C"]);
        assert_eq!(q(|q| q.filters.seen = vec![Seen::Watched]), ["A"]);
        assert_eq!(q(|q| q.filters.seen = vec![Seen::Started]), ["D"]);
        assert_eq!(q(|q| q.filters.seen = vec![Seen::Unwatched]), ["B", "C"]);
        assert_eq!(
            q(|q| q.filters.seen = vec![Seen::Unwatched, Seen::Started]),
            ["B", "C", "D"]
        );
        assert_eq!(q(|q| q.filters.loved = Some(true)), ["B"]);
        assert_eq!(q(|q| q.filters.resolution = vec![Resolution::Uhd]), ["C"]);
        assert_eq!(
            q(|q| q.filters.resolution = vec![Resolution::Uhd, Resolution::Hd]),
            ["A", "C"]
        );
        // Works without the value sorted by come last, in either direction.
        assert_eq!(q(|q| q.sort = Some(Sort::Year)), ["C", "B", "A", "D"]);
        assert_eq!(
            q(|q| {
                q.sort = Some(Sort::Year);
                q.descending = Some(false);
            }),
            ["A", "B", "C", "D"]
        );
        assert_eq!(q(|q| q.sort = Some(Sort::Runtime)), ["A", "B", "C", "D"]);
        assert_eq!(
            q(|q| {
                q.sort = Some(Sort::Runtime);
                q.descending = Some(true);
            }),
            ["C", "B", "A", "D"]
        );
        assert_eq!(q(|q| q.descending = Some(true)), ["D", "C", "B", "A"]);
        assert_eq!(q(|q| q.sort = Some(Sort::Played)), ["A", "D", "B", "C"]);
        assert_eq!(
            q(|q| {
                q.limit = Some(1);
                q.offset = Some(1);
            }),
            ["B"]
        );
    }

    fn track(lang: &str, codec: &str, profile: Option<&str>, channels: u32) -> AudioTrack {
        AudioTrack {
            codec: codec.into(),
            channels,
            lang: Some(lang.into()),
            profile: profile.map(Into::into),
            ..Default::default()
        }
    }

    fn subs(lang: &str) -> SubTrack {
        SubTrack {
            codec: "subrip".into(),
            lang: Some(lang.into()),
            ..Default::default()
        }
    }

    /// Three films and a show, each with what its files hold.
    fn formats() -> Env {
        let e = env();
        let file = |item: Id,
                    height: u32,
                    hdr: Option<&str>,
                    audio: Vec<AudioTrack>,
                    subtitles: Vec<SubTrack>| {
            let tech = serde_json::to_string(&TechInfo {
                duration_s: 100.0,
                width: height * 16 / 9,
                height,
                video_codec: "hevc".into(),
                hdr: hdr.map(Into::into),
                audio,
                subtitles,
            })
            .unwrap();
            e.c.execute(
                "INSERT INTO files(source_id, rel_path, item_id, tech) VALUES (?1, ?2, ?3, ?4)",
                params![e.src, format!("t{}.mkv", e.n("files")), item, tech],
            )
            .unwrap();
        };
        let dune = e.add(ItemKind::Movie, "Dune", None);
        file(
            dune,
            2160,
            Some("Dolby Vision"),
            vec![track(
                "eng",
                "truehd",
                Some("Dolby TrueHD + Dolby Atmos"),
                8,
            )],
            vec![subs("spa")],
        );
        file(dune, 1080, None, vec![track("ger", "ac3", None, 6)], vec![]);
        let heat = e.add(ItemKind::Movie, "Heat", None);
        file(
            heat,
            1080,
            None,
            vec![
                track("en", "dts", Some("DTS-HD MA + DTS:X"), 8),
                track("spa", "ac3", None, 2),
            ],
            vec![subs("eng")],
        );
        let mut commentary = track("fre", "aac", None, 2);
        commentary.commentary = true;
        let old = e.add(ItemKind::Movie, "Old", None);
        file(
            old,
            480,
            Some("HLG"),
            vec![track("eng", "aac", None, 2), commentary],
            vec![],
        );
        let show = e.add(ItemKind::Show, "Show", None);
        let season = e.add(ItemKind::Season, "", Some(show));
        let ep = e.add(ItemKind::Episode, "", Some(season));
        file(
            ep,
            2160,
            Some("HDR10"),
            vec![track("es-419", "flac", None, 2)],
            vec![],
        );
        e
    }

    #[test]
    fn technical_filters_match_any_file_beneath_a_work() {
        let e = formats();
        let q = |f: fn(&mut Filters)| {
            let mut filters = Filters::default();
            f(&mut filters);
            e.filter(filters)
        };
        assert_eq!(
            q(|f| f.resolution = vec![Resolution::Uhd]),
            ["Dune", "Show"]
        );
        assert_eq!(
            q(|f| f.resolution = vec![Resolution::FullHd]),
            ["Dune", "Heat"]
        );
        assert_eq!(q(|f| f.resolution = vec![Resolution::Sd]), ["Old"]);
        assert_eq!(q(|f| f.picture = vec![Picture::DolbyVision]), ["Dune"]);
        assert_eq!(
            q(|f| f.picture = vec![Picture::Hdr10, Picture::Hlg]),
            ["Old", "Show"]
        );
        assert_eq!(q(|f| f.picture = vec![Picture::Sdr]), ["Dune", "Heat"]);
        assert_eq!(q(|f| f.sound = vec![Sound::Atmos]), ["Dune"]);
        assert_eq!(q(|f| f.sound = vec![Sound::DtsX]), ["Heat"]);
        assert_eq!(
            q(|f| f.sound = vec![Sound::Lossless]),
            ["Dune", "Heat", "Show"]
        );
        assert_eq!(q(|f| f.sound = vec![Sound::Surround]), ["Dune", "Heat"]);
        // "eng", "en" and "ENG" are one language; a commentary is not one the film is heard in.
        assert_eq!(q(|f| f.audio = vec!["en".into()]), ["Dune", "Heat", "Old"]);
        assert_eq!(
            q(|f| f.audio = vec!["de".into(), "es-419".into()]),
            ["Dune", "Show"]
        );
        assert!(q(|f| f.audio = vec!["fr".into()]).is_empty());
        assert_eq!(q(|f| f.subtitles = vec!["es".into()]), ["Dune"]);
        // Across facets every choice applies.
        assert_eq!(
            q(|f| {
                f.resolution = vec![Resolution::FullHd];
                f.audio = vec!["es".into()];
            }),
            ["Heat"]
        );
    }

    #[test]
    fn facets_list_what_a_listing_holds() {
        let e = formats();
        e.exec(
            "UPDATE items SET year = 2021, runtime_min = 155, genres = '[\"Drama\",\"Science Fiction\"]' WHERE title = 'Dune';\
             UPDATE items SET year = 1995, runtime_min = 170, genres = '[\"Drama\",\"Crime\"]' WHERE title = 'Heat';",
        );
        let f = filter::facets(&e.c, &e.v, Some(ItemKind::Movie)).unwrap();
        let said = |list: &[filter::Count<String>]| {
            list.iter()
                .map(|c| format!("{} {}", c.value, c.count))
                .collect::<Vec<_>>()
        };
        assert_eq!(said(&f.genres), ["Drama 2", "Crime 1", "Science Fiction 1"]);
        assert_eq!(
            f.years
                .iter()
                .map(|c| (c.value, c.count))
                .collect::<Vec<_>>(),
            [(1995, 1), (2021, 1)]
        );
        assert_eq!(f.runtimes.len(), 2);
        assert_eq!(said(&f.resolution), ["full_hd 2", "sd 1", "uhd 1"]);
        assert_eq!(said(&f.picture), ["dolby_vision 1", "hlg 1", "sdr 2"]);
        assert_eq!(
            said(&f.sound),
            ["atmos 1", "dts_x 1", "lossless 2", "surround 2"]
        );
        assert_eq!(said(&f.audio), ["en 3", "de 1", "es 1"]);
        assert_eq!(said(&f.subtitles), ["en 1", "es 1"]);
        // The show's facets are its episodes'.
        let shows = filter::facets(&e.c, &e.v, Some(ItemKind::Show)).unwrap();
        assert_eq!(said(&shows.audio), ["es-419 1"]);
        assert_eq!(said(&shows.resolution), ["uhd 1"]);
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
            e.filter(Filters {
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
        let watched = Filters {
            seen: vec![Seen::Watched],
            ..Default::default()
        };
        assert_eq!(e.filter(watched), ["S"]);
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
        // A watched item keeps no partial progress.
        let started = item(&e.c, &e.v, eps[0]).unwrap().play.unwrap();
        assert_eq!(started.position_s, 0.0);
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
    fn set_watched_on_season_marks_its_episodes_only() {
        let e = env();
        let (show, season, eps) = e.show("S", 2);
        let s2 = e.add(ItemKind::Season, "Season 2", Some(show));
        let other = e.episode(s2, 2, 1, "S ep 2x1");
        set_watched(&e.c, season, true).unwrap();
        for &id in &eps {
            assert!(item(&e.c, &e.v, id).unwrap().play.unwrap().watched);
        }
        assert!(item(&e.c, &e.v, other).unwrap().play.is_none());
    }

    #[test]
    fn item_detail_carries_next_episode_for_shows() {
        let e = env();
        let (show, season, eps) = e.show("S", 2);
        let next = |id| item(&e.c, &e.v, id).unwrap().next_episode.map(|s| s.id);
        assert_eq!(next(show), Some(eps[0]));
        e.play(eps[0], 0.0, 0.0, true, 1);
        assert_eq!(next(show), Some(eps[1]));
        let d = item(&e.c, &e.v, show).unwrap();
        assert_eq!(d.next_episode.unwrap().title, "S ep 2");
        // Only shows carry one.
        assert_eq!(next(season), None);
        assert_eq!(next(eps[1]), None);
        e.play(eps[1], 0.0, 0.0, true, 1);
        assert_eq!(next(show), None);
    }

    #[test]
    fn cropped_widescreen_found_by_width() {
        let e = env();
        let m = e.with_file(ItemKind::Movie, "Wide", None);
        let tech = serde_json::to_string(&TechInfo {
            duration_s: 100.0,
            width: 3840,
            height: 1604,
            video_codec: "hevc".into(),
            hdr: None,
            audio: vec![],
            subtitles: vec![],
        })
        .unwrap();
        e.c.execute(
            "UPDATE files SET tech = ?1 WHERE item_id = ?2",
            params![tech, m],
        )
        .unwrap();
        assert_eq!(
            (e.summary(m).width, e.summary(m).height),
            (Some(3840), Some(1604))
        );
        let q = |r| {
            e.filter(Filters {
                resolution: vec![r],
                ..Default::default()
            })
        };
        assert_eq!(q(Resolution::Uhd), ["Wide"]);
        assert!(q(Resolution::FullHd).is_empty());
    }

    #[test]
    fn search_query_deserialises_from_partial_json() {
        let q: SearchQuery = serde_json::from_str(r#"{"kind":"movie","sort":"year"}"#).unwrap();
        assert_eq!(q.kind, Some(ItemKind::Movie));
        assert_eq!(q.sort, Some(Sort::Year));
        assert_eq!((q.text, q.limit, q.descending), (None, None, None));
        let q: SearchQuery = serde_json::from_str(
            r#"{"genres":["Drama"],"resolution":["full_hd"],"sound":["dts_x"]}"#,
        )
        .unwrap();
        assert_eq!(q.filters.genres, ["Drama"]);
        assert_eq!(q.filters.resolution, [Resolution::FullHd]);
        assert_eq!(q.filters.sound, [Sound::DtsX]);
        let q: SearchQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(q, SearchQuery::default());
        let shelf: Shelf = serde_json::from_str(r#""continue_watching""#).unwrap();
        assert_eq!(shelf, Shelf::ContinueWatching);
        assert_eq!(
            serde_json::to_string(&Shelf::RecentlyAdded).unwrap(),
            r#""recently_added""#
        );
        assert_eq!(
            serde_json::to_string(&Sort::Runtime).unwrap(),
            r#""runtime""#
        );
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
        // Other stays on its own wall, however recently it was played.
        let clip = e.with_file(ItemKind::Other, "Holiday clip", None);
        e.play(clip, 50.0, 100.0, false, 60);
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
             UPDATE items SET added_at=200, loved_choice=1 WHERE id={seen};\
             UPDATE items SET added_at=150 WHERE id={show};\
             UPDATE items SET added_at=250 WHERE id={};",
            eps[1]
        ));
        e.play(seen, 0.0, 0.0, true, 1);
        // Other stays on its own wall, however new or loved.
        let clip = e.with_file(ItemKind::Other, "Holiday clip", None);
        e.exec(&format!(
            "UPDATE items SET added_at=400, loved_choice=1 WHERE id={clip}"
        ));
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
        let n = library_counts(&e.c).unwrap();
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

    #[test]
    fn a_service_watch_counts_and_the_owners_choice_wins() {
        let e = env();
        let m = e.movie_with_tmdb("Taxi Driver", 103);
        e.exec("INSERT INTO service_state(service) VALUES ('svc')");
        e.exec(
            "INSERT INTO service_facts(service, kind, tmdb_id, fact, on_day) \
             VALUES ('svc', 'movie', 103, 'watch', 20000)",
        );
        assert!(e.summary(m).watched);
        set_watched(&e.c, m, false).unwrap();
        assert!(!e.summary(m).watched, "the choice wins");
        e.exec("UPDATE service_state SET enabled = 0");
        assert!(!e.summary(m).watched);
        e.exec("UPDATE service_state SET enabled = 1");
        assert!(!e.summary(m).watched, "still the choice");
        set_watched(&e.c, m, true).unwrap();
        assert!(e.summary(m).watched);
    }

    #[test]
    fn a_service_like_lights_loved_until_the_owner_says_no() {
        let e = env();
        let m = e.movie_with_tmdb("Taxi Driver", 103);
        e.exec("INSERT INTO service_state(service) VALUES ('svc')");
        e.exec(
            "INSERT INTO service_facts(service, kind, tmdb_id, fact) \
             VALUES ('svc', 'movie', 103, 'like')",
        );
        assert!(e.summary(m).loved);
        assert!(item(&e.c, &e.v, m).unwrap().item.loved);
        assert_eq!(e.shelf(Shelf::Loved), ["Taxi Driver"]);
        set_loved(&e.c, m, false).unwrap();
        assert!(!e.summary(m).loved, "the choice wins");
        assert!(!item(&e.c, &e.v, m).unwrap().item.loved);
        assert!(e.shelf(Shelf::Loved).is_empty());
    }

    #[test]
    fn a_switched_off_service_counts_for_nothing() {
        let e = env();
        let m = e.movie_with_tmdb("Taxi Driver", 103);
        // A show sharing the film's TMDB id is another work.
        let (show, _, _) = e.show("Show", 1);
        e.exec(&format!("UPDATE items SET tmdb_id = 103 WHERE id = {show}"));
        e.exec("INSERT INTO service_state(service, enabled) VALUES ('svc', 0)");
        e.exec(
            "INSERT INTO service_facts(service, kind, tmdb_id, fact) \
             VALUES ('svc', 'movie', 103, 'watch'), ('svc', 'movie', 103, 'like')",
        );
        assert!(!e.summary(m).watched);
        assert!(!e.summary(m).loved);
        assert_eq!(e.shelf(Shelf::Unwatched), ["Taxi Driver"]);
        e.exec("UPDATE service_state SET enabled = 1");
        assert!(e.summary(m).watched);
        assert!(e.shelf(Shelf::Unwatched).is_empty());
        assert!(!e.summary(show).watched && !e.summary(show).loved);
    }

    #[test]
    fn a_choice_on_an_episode_counts_in_its_show() {
        let e = env();
        let (show, season, eps) = e.show("S", 2);
        set_watched(&e.c, eps[0], true).unwrap();
        e.exec(&format!(
            "DELETE FROM play_state; UPDATE items SET watched_choice = 1 WHERE id = {}",
            eps[1]
        ));
        assert!(e.summary(show).watched);
        assert!(e.summary(season).watched);
        set_watched(&e.c, show, false).unwrap();
        let choices: Vec<Option<i64>> = eps
            .iter()
            .map(|id| {
                e.c.query_row(
                    "SELECT watched_choice FROM items WHERE id = ?1",
                    [id],
                    |r| r.get(0),
                )
                .unwrap()
            })
            .collect();
        assert_eq!(choices, [Some(0), Some(0)]);
        assert_eq!(e.summary(show).progress, Some(0.0));
    }

    impl Env {
        fn service(&self, id: &str, enabled: bool) {
            self.c
                .execute(
                    "INSERT INTO service_state(service, enabled) VALUES (?1, ?2)",
                    params![id, enabled],
                )
                .unwrap();
        }

        fn fact(&self, service: &str, tmdb_id: i64, fact: &str, on_day: i64, value: &str) {
            self.c
                .execute(
                    "INSERT INTO service_facts(service, kind, tmdb_id, fact, on_day, value) \
                     VALUES (?1, 'movie', ?2, ?3, ?4, ?5)",
                    params![service, tmdb_id, fact, on_day, value],
                )
                .unwrap();
        }

        /// A film on the owner's watchlist that has no file.
        fn wanted(&self, title: &str, tmdb_id: i64) -> Id {
            let id = self.add(ItemKind::Movie, title, None);
            self.exec(&format!(
                "UPDATE items SET tmdb_id = {tmdb_id}, wanted = 1 WHERE id = {id}"
            ));
            id
        }

        fn listed(&self, q: &SearchQuery) -> Vec<(String, bool)> {
            search(&self.c, &self.v, q)
                .unwrap()
                .into_iter()
                .map(|s| (s.title, s.in_library))
                .collect()
        }
    }

    fn owned(t: &str) -> (String, bool) {
        (t.to_string(), true)
    }

    fn not_owned(t: &str) -> (String, bool) {
        (t.to_string(), false)
    }

    #[test]
    fn a_wanted_row_is_hidden_from_every_listing_unless_asked() {
        let e = env();
        e.service("svc", true);
        let heat = e.movie_with_tmdb("Heat", 1);
        let w = e.wanted("Alphaville", 2);
        e.exec(&format!(
            "UPDATE items SET year = 1965, match_state = 'needsreview' WHERE id = {w}; \
             INSERT INTO play_log(item_id, started_at, ended_at, position_s) VALUES ({w}, 5, 6, 1)"
        ));
        e.fact("svc", 1, "watchlisted", 100, "{}");
        e.fact("svc", 2, "watchlisted", 200, "{}");
        assert_eq!(e.search(&SearchQuery::default()), ["Heat"]);
        assert!(e.text("alphaville").is_empty());
        assert_eq!(e.shelf(Shelf::Unwatched), ["Heat"]);
        assert!(e.shelf(Shelf::Loved).is_empty());
        let f = filter::facets(&e.c, &e.v, None).unwrap();
        assert!(f.years.is_empty());
        assert!(needs_review(&e.c, &e.v).unwrap().is_empty());
        assert!(history(&e.c, &e.v, 10).unwrap().is_empty());
        // The watchlist filter lists it after the owner's films; so does Ask.
        let on_watchlist = SearchQuery {
            filters: Filters {
                watchlist: Some(true),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            e.listed(&on_watchlist),
            [owned("Heat"), not_owned("Alphaville")]
        );
        let ask = SearchQuery {
            text: Some("alpha".into()),
            with_wanted: true,
            ..Default::default()
        };
        assert_eq!(e.listed(&ask), [not_owned("Alphaville")]);
        let all = search(
            &e.c,
            &e.v,
            &SearchQuery {
                with_wanted: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(all.len(), 2);
        let a = all.iter().find(|s| s.id == w).unwrap();
        assert!(!a.in_library && !a.available);
        assert!(
            all.iter()
                .any(|s| s.id == heat && s.in_library && s.available)
        );
        // Not in the library is not offline.
        let offline = SearchQuery {
            with_wanted: true,
            filters: Filters {
                available: Some(false),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(e.listed(&offline).is_empty());
        // A switched-off service's watchlist is not shown.
        e.exec("UPDATE service_state SET enabled = 0");
        assert!(e.listed(&ask).is_empty());
        let q: SearchQuery =
            serde_json::from_str(r#"{"with_wanted":true,"watchlist":true}"#).unwrap();
        assert!(q.with_wanted);
        assert_eq!(q.filters.watchlist, Some(true));
    }

    #[test]
    fn rating_sort_averages_public_scores_over_their_scales_unrated_last() {
        let e = env();
        e.service("svc", true);
        e.service("off", false);
        let a = e.movie_with_tmdb("Alien", 1);
        e.movie_with_tmdb("Brazil", 2);
        e.movie_with_tmdb("Casablanca", 3);
        e.movie_with_tmdb("Dune", 4);
        let score = |svc: &str, id: i64, origin: &str, value: f32, scale: f32| {
            e.c.execute(
                "INSERT INTO service_scores(service, kind, tmdb_id, origin, value, scale)                  VALUES (?1, 'movie', ?2, ?3, ?4, ?5)",
                params![svc, id, origin, value, scale],
            )
            .unwrap();
        };
        // Alien: (84 + 90) / 2 = 87. Brazil: 98. Casablanca: only a switched-off service.
        score("svc", 1, "a", 8.4, 10.0);
        score("svc", 1, "b", 90.0, 100.0);
        score("svc", 2, "a", 4.9, 5.0);
        score("off", 3, "a", 10.0, 10.0);
        let q = |descending: Option<bool>| {
            e.search(&SearchQuery {
                sort: Some(Sort::Rating),
                descending,
                ..SearchQuery::default()
            })
        };
        assert_eq!(q(None), ["Brazil", "Alien", "Casablanca", "Dune"]);
        assert_eq!(q(Some(false)), ["Alien", "Brazil", "Casablanca", "Dune"]);
        assert_eq!(serde_json::to_string(&Sort::Rating).unwrap(), "\"rating\"");
        // Details carries the scores of switched-on services only.
        let d = item(&e.c, &e.v, a).unwrap();
        assert_eq!(d.reception.len(), 2);
        assert_eq!(d.reception[0].origin, "a");
        e.exec("UPDATE service_state SET enabled = 0");
        assert!(item(&e.c, &e.v, a).unwrap().reception.is_empty());
    }

    #[test]
    fn activity_filters_and_my_rating_sort() {
        let e = env();
        e.service("svc", true);
        e.service("other", true);
        e.service("off", false);
        e.movie_with_tmdb("Alien", 1);
        e.movie_with_tmdb("Brazil", 2);
        let c = e.movie_with_tmdb("Casablanca", 3);
        // The newest rating counts; a switched-off service counts for nothing.
        e.fact("other", 1, "rating", 100, r#"{"value":1.0,"scale":5}"#);
        e.fact("svc", 1, "rating", 200, r#"{"value":4.5,"scale":5}"#);
        e.fact("svc", 2, "rating", 100, r#"{"value":2,"scale":5}"#);
        e.fact("off", 2, "rating", 300, r#"{"value":5,"scale":5}"#);
        e.fact("off", 3, "rating", 300, r#"{"value":5,"scale":5}"#);
        e.fact(
            "svc",
            1,
            "review",
            200,
            r#"{"text":"Yes.","spoiler":false}"#,
        );
        e.fact("off", 2, "review", 200, r#"{"text":"No.","spoiler":false}"#);
        e.fact("svc", 2, "watch", 100, r#"{"rewatch":false}"#);
        // Exhibit's own play is not a log.
        e.play(c, 0.0, 0.0, true, 50);
        let q = |f: fn(&mut SearchQuery)| {
            let mut q = SearchQuery::default();
            f(&mut q);
            e.search(&q)
        };
        assert_eq!(
            q(|q| q.sort = Some(Sort::MyRating)),
            ["Alien", "Brazil", "Casablanca"]
        );
        assert_eq!(
            q(|q| {
                q.sort = Some(Sort::MyRating);
                q.descending = Some(false);
            }),
            ["Brazil", "Alien", "Casablanca"]
        );
        assert_eq!(q(|q| q.filters.rated = Some(true)), ["Alien", "Brazil"]);
        assert_eq!(q(|q| q.filters.rated = Some(false)), ["Casablanca"]);
        assert_eq!(q(|q| q.filters.reviewed = Some(true)), ["Alien"]);
        assert_eq!(q(|q| q.filters.logged = Some(true)), ["Brazil"]);
        assert_eq!(
            q(|q| q.filters.logged = Some(false)),
            ["Alien", "Casablanca"]
        );
        assert!(q(|q| q.filters.watchlist = Some(true)).is_empty());
        assert_eq!(
            serde_json::to_string(&Sort::MyRating).unwrap(),
            r#""my_rating""#
        );
    }

    #[test]
    fn detail_lists_activity_newest_first_with_exhibits_own_watch() {
        let e = env();
        e.service("svc", true);
        e.service("other", true);
        e.service("off", false);
        let m = e.movie_with_tmdb("Heat", 1);
        e.fact("svc", 1, "watch", 100, r#"{"rewatch":false}"#);
        e.fact("svc", 1, "watch", 300, r#"{"rewatch":true}"#);
        e.fact("svc", 1, "watch", 0, r#"{"rewatch":false}"#);
        e.fact("off", 1, "watch", 500, r#"{"rewatch":false}"#);
        e.fact("svc", 1, "rating", 100, r#"{"value":4,"scale":5}"#);
        e.fact("other", 1, "rating", 300, r#"{"value":3.5,"scale":5}"#);
        e.fact(
            "svc",
            1,
            "review",
            300,
            r#"{"text":"Long.","spoiler":true}"#,
        );
        e.fact("svc", 1, "like", 0, "{}");
        e.fact("off", 1, "like", 0, "{}");
        e.fact("svc", 1, "watchlisted", 50, "{}");
        e.play(m, 0.0, 0.0, true, 200);
        let a = item(&e.c, &e.v, m).unwrap().activity;
        let watches: Vec<_> = a
            .watches
            .iter()
            .map(|w| (w.service.as_str(), w.on, w.fact.rewatch))
            .collect();
        assert_eq!(
            watches,
            [
                ("svc", Some(300), true),
                (crate::service::EXHIBIT, Some(200), false),
                ("svc", Some(100), false),
                ("svc", None, false),
            ]
        );
        let ratings: Vec<_> = a
            .ratings
            .iter()
            .map(|r| (r.service.as_str(), r.on, r.fact.value, r.fact.scale))
            .collect();
        assert_eq!(
            ratings,
            [("other", Some(300), 3.5, 5.0), ("svc", Some(100), 4.0, 5.0)]
        );
        assert_eq!(a.reviews.len(), 1);
        assert_eq!(
            (a.reviews[0].fact.text.as_str(), a.reviews[0].fact.spoiler),
            ("Long.", true)
        );
        assert_eq!(a.liked_by, ["svc"]);
        assert_eq!(a.watchlisted_by, ["svc"]);
        assert!(a.watched && a.loved);
        assert_eq!((a.watched_choice, a.loved_choice), (None, None));
        let json = serde_json::to_value(&a.ratings[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"service":"other","on":300,"value":3.5,"scale":5.0})
        );
        set_loved(&e.c, m, false).unwrap();
        let a = item(&e.c, &e.v, m).unwrap().activity;
        assert_eq!(a.loved_choice, Some(false));
        assert!(!a.loved);
        // A work with nothing said about it has an empty activity.
        let (show, _, eps) = e.show("S", 1);
        let a = item(&e.c, &e.v, show).unwrap().activity;
        assert!(a.watches.is_empty() && a.ratings.is_empty() && a.liked_by.is_empty());
        assert!(!a.watched);
        e.play(eps[0], 0.0, 0.0, true, 1);
        assert!(item(&e.c, &e.v, show).unwrap().activity.watched);
    }

    #[test]
    fn watchlist_shelf_puts_owned_first() {
        let e = env();
        e.service("svc", true);
        e.service("off", false);
        e.movie_with_tmdb("Alien", 1);
        e.movie_with_tmdb("Brazil", 2);
        e.movie_with_tmdb("Casablanca", 3);
        e.movie_with_tmdb("Dune", 4);
        e.movie_with_tmdb("Eraserhead", 5);
        e.wanted("Fargo", 6);
        e.wanted("Gattaca", 7);
        e.fact("svc", 1, "watchlisted", 100, "{}");
        e.fact("svc", 2, "watchlisted", 300, "{}");
        // Watched is off the watchlist; so is what a switched-off service says.
        e.fact("svc", 3, "watchlisted", 500, "{}");
        e.fact("svc", 3, "watch", 600, r#"{"rewatch":false}"#);
        e.fact("off", 5, "watchlisted", 500, "{}");
        e.fact("svc", 6, "watchlisted", 200, "{}");
        e.fact("svc", 7, "watchlisted", 400, "{}");
        let got: Vec<_> = shelf(&e.c, &e.v, Shelf::Watchlist, 50)
            .unwrap()
            .into_iter()
            .map(|s| (s.title, s.in_library))
            .collect();
        assert_eq!(
            got,
            [
                owned("Brazil"),
                owned("Alien"),
                not_owned("Gattaca"),
                not_owned("Fargo")
            ]
        );
        assert_eq!(
            serde_json::to_string(&Shelf::Watchlist).unwrap(),
            r#""watchlist""#
        );
    }

    #[test]
    fn facets_count_activity_only_for_enabled_services() {
        let e = env();
        e.service("svc", true);
        e.service("off", false);
        e.movie_with_tmdb("Alien", 1);
        e.movie_with_tmdb("Brazil", 2);
        e.movie_with_tmdb("Casablanca", 3);
        e.wanted("Fargo", 6);
        e.wanted("Gattaca", 7);
        e.fact("svc", 1, "rating", 100, r#"{"value":4,"scale":5}"#);
        e.fact("svc", 1, "review", 100, r#"{"text":"x","spoiler":false}"#);
        e.fact("svc", 1, "watch", 100, r#"{"rewatch":false}"#);
        e.fact("off", 2, "rating", 100, r#"{"value":4,"scale":5}"#);
        e.fact("off", 2, "review", 100, r#"{"text":"x","spoiler":false}"#);
        e.fact("off", 2, "watch", 100, r#"{"rewatch":false}"#);
        e.fact("svc", 3, "watchlisted", 100, "{}");
        e.fact("svc", 6, "watchlisted", 100, "{}");
        e.fact("off", 7, "watchlisted", 100, "{}");
        let counts = |kind| filter::facets(&e.c, &e.v, kind).unwrap().activity;
        let want = filter::ActivityCounts {
            rated: 1,
            reviewed: 1,
            logged: 1,
            watchlist: 2,
            not_in_library: 1,
        };
        assert_eq!(counts(None), want);
        assert_eq!(counts(Some(ItemKind::Movie)), want);
        assert_eq!(
            counts(Some(ItemKind::Show)),
            filter::ActivityCounts::default()
        );
        let json = serde_json::to_value(filter::facets(&e.c, &e.v, None).unwrap()).unwrap();
        assert_eq!(json["activity"]["not_in_library"], 1);
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
            audio: vec![
                track("eng", "dts", Some("DTS-HD MA"), 6),
                track("spa", "ac3", None, 6),
            ],
            subtitles: vec![subs("eng"), subs("spa")],
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
        // A service with half the films rated, a fifth logged and reviewed, and a watchlist
        // of 500 the owner has and 200 they do not.
        tx.execute("INSERT INTO service_state(service) VALUES ('svc')", [])
            .unwrap();
        let fact = |tmdb: i64, fact: &str, day: i64, value: &str| {
            tx.execute(
                "INSERT INTO service_facts(service, kind, tmdb_id, fact, on_day, value) \
                 VALUES ('svc', 'movie', ?1, ?2, ?3, ?4)",
                params![tmdb, fact, day, value],
            )
            .unwrap();
        };
        for (i, &m) in movies.iter().enumerate() {
            let tmdb = 10_000 + i as i64;
            tx.execute(
                "UPDATE items SET tmdb_id = ?2 WHERE id = ?1",
                params![m, tmdb],
            )
            .unwrap();
            let day = i as i64 * 86_400;
            if i % 2 == 0 {
                let rating = format!(r#"{{"value":{},"scale":5}}"#, (i % 10) as f32 / 2.0);
                fact(tmdb, "rating", day, &rating);
            }
            if i % 5 == 0 {
                fact(tmdb, "watch", day, r#"{"rewatch":false}"#);
                fact(tmdb, "review", day, r#"{"text":"x","spoiler":false}"#);
            }
            if i % 8 == 1 {
                fact(tmdb, "watchlisted", day, "{}");
            }
        }
        for i in 0..200 {
            let tmdb = 50_000 + i;
            tx.execute(
                "INSERT INTO items(kind, title, tmdb_id, match_state, added_at, wanted) \
                 VALUES ('movie', ?1, ?2, 'matched', 0, 1)",
                params![format!("Wanted {i}"), tmdb],
            )
            .unwrap();
            fact(tmdb, "watchlisted", i, "{}");
        }
        // Shows and seasons hold no file of their own, only their episodes do.
        tx.execute(
            "DELETE FROM files WHERE item_id IN (SELECT id FROM items WHERE kind IN (?1, ?2))",
            params![enum_str(ItemKind::Show), enum_str(ItemKind::Season)],
        )
        .unwrap();
        tx.commit().unwrap();
        assert_eq!(e.n("items"), 4000 + 50 * 23 + 300 + 200);

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
            Shelf::Watchlist,
        ] {
            time(&format!("shelf {s:?}"), &|| {
                shelf(&e.c, &e.v, s, 50).unwrap();
            });
        }
        time("search (your activity, by your rating)", &|| {
            let q = SearchQuery {
                kind: Some(ItemKind::Movie),
                filters: Filters {
                    rated: Some(true),
                    reviewed: Some(true),
                    logged: Some(true),
                    ..Default::default()
                },
                sort: Some(Sort::MyRating),
                limit: Some(500),
                ..Default::default()
            };
            assert_eq!(search(&e.c, &e.v, &q).unwrap().len(), 400);
        });
        time("search (on the watchlist, by your rating)", &|| {
            let q = SearchQuery {
                kind: Some(ItemKind::Movie),
                filters: Filters {
                    watchlist: Some(true),
                    ..Default::default()
                },
                sort: Some(Sort::MyRating),
                limit: Some(500),
                ..Default::default()
            };
            // 396 the owner has and has not watched, then the first of those they do not have.
            let page = search(&e.c, &e.v, &q).unwrap();
            assert_eq!(page.iter().filter(|s| s.in_library).count(), 396);
            assert_eq!(page.len(), 500);
        });
        time("search (every filter, by year)", &|| {
            let q = SearchQuery {
                kind: Some(ItemKind::Movie),
                filters: Filters {
                    genres: vec!["Drama".into()],
                    without_genres: vec!["Horror".into()],
                    year_min: Some(1995),
                    seen: vec![Seen::Unwatched],
                    resolution: vec![Resolution::FullHd],
                    picture: vec![Picture::Sdr],
                    sound: vec![Sound::Surround],
                    audio: vec!["en".into()],
                    subtitles: vec!["es".into()],
                    rated: Some(true),
                    reviewed: Some(false),
                    logged: Some(false),
                    watchlist: Some(false),
                    ..Default::default()
                },
                sort: Some(Sort::Year),
                limit: Some(500),
                ..Default::default()
            };
            search(&e.c, &e.v, &q).unwrap();
        });
        time("search (audio language, all pages)", &|| {
            let q = SearchQuery {
                kind: Some(ItemKind::Movie),
                filters: Filters {
                    audio: vec!["en".into()],
                    ..Default::default()
                },
                limit: Some(500),
                ..Default::default()
            };
            assert_eq!(search(&e.c, &e.v, &q).unwrap().len(), 500);
        });
        time("facets (movies)", &|| {
            filter::facets(&e.c, &e.v, Some(ItemKind::Movie)).unwrap();
        });
        time("item (show)", &|| {
            assert_eq!(item(&e.c, &e.v, a_show).unwrap().children.len(), 2);
        });
        time("needs_review", &|| {
            assert_eq!(needs_review(&e.c, &e.v).unwrap().len(), 300);
        });
    }
}
