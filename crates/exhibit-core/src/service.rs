//! The interface a connected site implements. The core knows facts about films, never sites.

use crate::error::{Detail, Error, Result};
use crate::tmdb::Http;
use serde::Serialize;

/// The reserved service id for facts Exhibit holds itself.
pub const EXHIBIT: &str = "exhibit";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provides {
    Activity,
    Reception,
    Awards,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Need {
    Username,
    Key,
    ExportFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportHelp {
    pub page: &'static str,
    pub file_prefix: &'static str,
    pub file_ext: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ServiceInfo {
    pub id: &'static str,
    pub name: &'static str,
    /// One-colour SVG, drawn in the text colour.
    pub mark_svg: &'static str,
    pub provides: &'static [Provides],
    pub needs: &'static [Need],
    pub export: Option<ExportHelp>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FilmRef {
    pub key: String,
    pub tmdb_id: Option<i64>,
    pub title: String,
    pub year: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Fact {
    Rating { value: f32, scale: f32 },
    Review { text: String, spoiler: bool },
    Watch { rewatch: bool },
    Like,
    Watchlisted,
}

/// A fact about a film; `on` is the unix seconds of the day, UTC.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Dated {
    pub film: FilmRef,
    pub fact: Fact,
    pub on: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ActivityBatch {
    pub facts: Vec<Dated>,
}

pub enum SyncInput<'a> {
    Snapshot(&'a std::path::Path),
    Since(Option<i64>),
}

pub struct Account<'a> {
    pub username: Option<&'a str>,
    pub key: Option<&'a str>,
}

pub trait Service: Send + Sync {
    fn info(&self) -> &ServiceInfo;

    fn activity(
        &self,
        _account: &Account,
        _input: SyncInput,
        _http: &dyn Http,
    ) -> Result<ActivityBatch> {
        Err(Error::Invalid(Detail::coded(
            "service.unsupported",
            "this service does not provide activity",
        )))
    }

    /// How many recent entries the account shows; a coded error says what is wrong.
    fn check(&self, account: &Account, http: &dyn Http) -> Result<u32>;
}

/// A stable key for matching a film across services: lowercased title, spaces collapsed.
pub fn film_key(title: &str, year: Option<i32>) -> String {
    let title = title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    match year {
        Some(y) => format!("{title}|{y}"),
        None => format!("{title}|"),
    }
}

pub fn normalize_username(s: &str) -> String {
    s.trim().trim_start_matches('@').to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn film_key_lowercases_trims_and_collapses() {
        assert_eq!(film_key("  Taxi Driver ", Some(1976)), "taxi driver|1976");
        assert_eq!(
            film_key("Heat   (Director's  Cut)", None),
            "heat (director's cut)|"
        );
    }

    #[test]
    fn username_loses_at_and_case() {
        assert_eq!(normalize_username(" @Rian "), "rian");
    }

    #[test]
    fn fact_serializes_tagged() {
        let v = serde_json::to_value(Fact::Rating {
            value: 4.5,
            scale: 5.0,
        })
        .unwrap();
        assert_eq!(v["type"], "rating");
        assert_eq!(serde_json::to_value(Fact::Like).unwrap()["type"], "like");
    }

    #[test]
    fn core_names_no_site() {
        let needles = [
            ["letter", "boxd"].concat(),
            ["mdb", "list"].concat(),
            ["wiki", "data"].concat(),
            ["toma", "to"].concat(),
            ["meta", "critic"].concat(),
        ];
        let mut files = vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
        let mut seen = 0;
        while let Some(p) = files.pop() {
            if p.is_dir() {
                files.extend(std::fs::read_dir(&p).unwrap().map(|e| e.unwrap().path()));
            } else if p.extension().is_some_and(|e| e == "rs") {
                seen += 1;
                let text = std::fs::read_to_string(&p).unwrap().to_lowercase();
                for n in &needles {
                    assert!(!text.contains(n.as_str()), "{} names {n}", p.display());
                }
            }
        }
        assert!(seen > 10);
    }
}
