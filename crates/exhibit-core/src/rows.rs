//! Row mapping for `items`, shared by every module that reads them. Model enums are stored
//! under their serde names (`"movie"`, `"needsreview"`), so the database and the JSON the
//! CLI prints spell them the same way.

use crate::model::Item;
use rusqlite::Row;
use rusqlite::types::Type;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub(crate) const ITEM_COLUMNS: &str = "id, kind, parent_id, title, year, season, episode, \
     overview, runtime_min, genres, people, poster, backdrop, tmdb_id, match_state, \
     match_source, confidence, candidates, loved, added_at";

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
