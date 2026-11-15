use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub type Id = i64;

/// Position over duration at which an item counts as watched.
pub const WATCHED_FRACTION: f64 = 0.90;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Movie,
    Show,
    Season,
    Episode,
    Other,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MatchState {
    Unmatched,
    Matched,
    NeedsReview,
    Manual,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MatchSource {
    Nfo,
    Lookup,
    Agent,
    Manual,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Source {
    pub id: Id,
    pub path: PathBuf,
    pub volume_id: String,
    pub online: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AudioTrack {
    pub codec: String,
    pub channels: u32,
    pub lang: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SubTrack {
    pub codec: String,
    pub lang: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TechInfo {
    pub duration_s: f64,
    pub width: u32,
    pub height: u32,
    pub video_codec: String,
    /// "HDR10" | "HLG" | "Dolby Vision"
    pub hdr: Option<String>,
    pub audio: Vec<AudioTrack>,
    pub subtitles: Vec<SubTrack>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MediaFile {
    pub id: Id,
    pub source_id: Id,
    pub rel_path: String,
    pub size: u64,
    pub mtime: i64,
    pub item_id: Option<Id>,
    pub tech: Option<TechInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Person {
    pub name: String,
    /// "director" | "cast"
    pub role: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Candidate {
    pub tmdb_id: i64,
    pub kind: ItemKind,
    pub title: String,
    pub year: Option<i32>,
    pub score: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Item {
    pub id: Id,
    pub kind: ItemKind,
    pub parent_id: Option<Id>,
    pub title: String,
    pub year: Option<i32>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub overview: Option<String>,
    pub runtime_min: Option<i32>,
    pub genres: Vec<String>,
    pub people: Vec<Person>,
    pub poster: Option<String>,
    pub backdrop: Option<String>,
    pub tmdb_id: Option<i64>,
    pub match_state: MatchState,
    pub match_source: Option<MatchSource>,
    pub confidence: Option<f32>,
    pub candidates: Vec<Candidate>,
    pub loved: bool,
    pub added_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayState {
    pub item_id: Id,
    pub position_s: f64,
    pub duration_s: f64,
    pub watched: bool,
    pub last_played_at: i64,
}
