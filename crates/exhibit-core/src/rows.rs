//! Row mapping for `items`, shared by every module that reads them. Model enums are stored
//! under their serde names (`"movie"`, `"needsreview"`), so the database and the JSON the
//! CLI prints spell them the same way.

use crate::activity::loved_sql;
use crate::model::{Item, MediaFile, PlayState};
use crate::query::ItemSummary;
use rusqlite::Row;
use rusqlite::types::Type;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// The columns `item_from_row` reads, from `items` without an alias.
pub(crate) fn item_columns() -> String {
    format!(
        "id, kind, parent_id, title, year, season, episode, \
         overview, runtime_min, genres, people, poster, backdrop, tmdb_id, match_state, \
         match_source, confidence, candidates, {}, added_at",
        loved_sql("items")
    )
}

/// An item's title in the language bound as `?2` (its `item_texts` row, alias `tx`),
/// falling back to the English base where TMDB has no translation.
pub(crate) const LOCAL_TITLE: &str = "coalesce(nullif(tx.title, ''), i.title)";
pub(crate) const LOCAL_POSTER: &str = "coalesce(tx.poster, i.poster)";
pub(crate) const LOCAL_GENRES: &str =
    "CASE WHEN tx.genres IS NOT NULL AND tx.genres <> '[]' THEN tx.genres ELSE i.genres END";

/// `item_columns` of `items i` in the language of `item_texts tx`, in the same order.
pub(crate) fn local_item_columns() -> String {
    format!(
        "i.id, i.kind, i.parent_id, \
         coalesce(nullif(tx.title, ''), i.title), i.year, i.season, i.episode, \
         coalesce(nullif(tx.overview, ''), i.overview), i.runtime_min, \
         CASE WHEN tx.genres IS NOT NULL AND tx.genres <> '[]' THEN tx.genres ELSE i.genres END, \
         i.people, coalesce(tx.poster, i.poster), i.backdrop, i.tmdb_id, i.match_state, \
         i.match_source, i.confidence, i.candidates, {}, i.added_at",
        loved_sql("i")
    )
}

pub(crate) fn item_from_row(r: &Row) -> rusqlite::Result<Item> {
    Ok(Item {
        id: r.get(0)?,
        kind: enum_col(r, 1)?,
        parent_id: r.get(2)?,
        title: r.get(3)?,
        year: r.get(4)?,
        season: r.get(5)?,
        episode: r.get(6)?,
        overview: r.get(7)?,
        runtime_min: r.get(8)?,
        genres: json_col(r, 9)?,
        people: json_col(r, 10)?,
        poster: r.get(11)?,
        backdrop: r.get(12)?,
        tmdb_id: r.get(13)?,
        match_state: enum_col(r, 14)?,
        match_source: match r.get::<_, Option<String>>(15)? {
            Some(s) => Some(parse(15, Value::String(s))?),
            None => None,
        },
        confidence: r.get::<_, Option<f64>>(16)?.map(|c| c as f32),
        candidates: json_col(r, 17)?,
        loved: r.get(18)?,
        added_at: r.get(19)?,
    })
}

pub(crate) const FILE_COLUMNS: &str = "id, source_id, rel_path, size, mtime, item_id, tech";

pub(crate) fn media_file_from_row(r: &Row) -> rusqlite::Result<MediaFile> {
    Ok(MediaFile {
        id: r.get(0)?,
        source_id: r.get(1)?,
        rel_path: r.get(2)?,
        size: r.get::<_, i64>(3)? as u64,
        mtime: r.get(4)?,
        item_id: r.get(5)?,
        tech: match r.get::<_, Option<String>>(6)? {
            Some(t) => Some(serde_json::from_str(&t).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(6, Type::Text, Box::new(e))
            })?),
            None => None,
        },
    })
}

pub(crate) const PLAY_STATE_COLUMNS: &str =
    "item_id, position_s, duration_s, watched, last_played_at";

pub(crate) fn play_state_from_row(r: &Row) -> rusqlite::Result<PlayState> {
    Ok(PlayState {
        item_id: r.get(0)?,
        position_s: r.get(1)?,
        duration_s: r.get(2)?,
        watched: r.get(3)?,
        last_played_at: r.get(4)?,
    })
}

/// Columns of the listing query's outer select, in the order `summary_from_row` reads them.
pub(crate) const SUMMARY_COLUMNS: &str = "b.id, b.kind, b.title, b.year, b.season, b.episode, \
     b.show_title, b.poster, b.backdrop, b.runtime_min, b.width, b.height, b.hdr, \
     b.watched, b.progress, b.loved, b.available, b.match_state, b.genres, b.path, b.in_library";

pub(crate) fn summary_from_row(r: &Row) -> rusqlite::Result<ItemSummary> {
    Ok(ItemSummary {
        id: r.get(0)?,
        kind: enum_col(r, 1)?,
        title: r.get(2)?,
        year: r.get(3)?,
        season: r.get(4)?,
        episode: r.get(5)?,
        show_title: r.get(6)?,
        poster: r.get(7)?,
        backdrop: r.get(8)?,
        runtime_min: r.get(9)?,
        width: r.get(10)?,
        height: r.get(11)?,
        hdr: r.get(12)?,
        watched: r.get(13)?,
        progress: r.get::<_, Option<f64>>(14)?.map(|p| p as f32),
        loved: r.get(15)?,
        available: r.get(16)?,
        match_state: enum_col(r, 17)?,
        genres: json_col(r, 18)?,
        path: r.get(19)?,
        in_library: r.get(20)?,
    })
}

/// The database spelling of a model enum.
pub(crate) fn enum_str<T: Serialize>(v: T) -> String {
    match serde_json::to_value(v) {
        Ok(Value::String(s)) => s,
        _ => unreachable!("model enums serialise to strings"),
    }
}

fn enum_col<T: DeserializeOwned>(r: &Row, i: usize) -> rusqlite::Result<T> {
    parse(i, Value::String(r.get(i)?))
}

fn json_col<T: DeserializeOwned>(r: &Row, i: usize) -> rusqlite::Result<T> {
    let text: String = r.get(i)?;
    serde_json::from_str(&text)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(i, Type::Text, Box::new(e)))
}

fn parse<T: DeserializeOwned>(i: usize, v: Value) -> rusqlite::Result<T> {
    serde_json::from_value(v)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(i, Type::Text, Box::new(e)))
}
