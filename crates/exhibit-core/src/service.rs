//! The interface a connected site implements. The core knows facts about films, never sites.

use crate::error::{Detail, Error, Result};
use crate::model::ItemKind;
use crate::tmdb::Http;
use serde::{Deserialize, Serialize};

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
    /// The owner approves Exhibit on the site, from any device, with a short code.
    SignIn,
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
    /// Whose scores the service reports, in the order they are shown.
    pub origins: &'static [OriginInfo],
    /// The award bodies it reports, most prominent first.
    pub bodies: &'static [BodyInfo],
    /// How many films one lookup may ask about.
    pub batch: usize,
}

/// How a score reads: 8.2, 89% or 94 in a square.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreStyle {
    Decimal,
    Percent,
    Badge,
}

/// Whose score it is, when that is not the service that fetched it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OriginInfo {
    pub id: &'static str,
    pub name: &'static str,
    /// One-colour SVG, drawn in the text colour.
    pub mark_svg: &'static str,
    pub style: ScoreStyle,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BodyInfo {
    pub id: &'static str,
    /// In English; the window words it in the owner's language.
    pub name: &'static str,
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

/// A film or show by its TMDB id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct TmdbFilm {
    pub kind: ItemKind,
    pub tmdb_id: i64,
}

/// The public's verdict on a work, from `origin`, on its own scale.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Score {
    pub origin: String,
    pub value: f32,
    pub scale: f32,
    pub votes: Option<i64>,
    pub url: Option<String>,
    /// The origin's own seal on the score (a certified one).
    pub certified: bool,
}

/// A win or a nomination. `award` is the service's id for it; `names` its short name by
/// language (`en`, `es`); a lower `rank` is more prominent within its body.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Award {
    pub body: String,
    pub award: String,
    pub names: Vec<(String, String)>,
    pub year: Option<i32>,
    pub won: bool,
    pub rank: i32,
}

/// What the service has left of its daily allowance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Quota {
    pub remaining: u32,
    /// Unix seconds.
    pub resets_at: i64,
}

/// One lookup's answer: what each film has (films the service does not know are left out).
#[derive(Debug, Clone, PartialEq)]
pub struct Lookup<T> {
    pub films: Vec<(TmdbFilm, Vec<T>)>,
    pub quota: Option<Quota>,
}

/// A sign-in under way: the owner enters `user_code` at `verify_url` (or opens
/// `verify_url_full`, which carries it). Exhibit asks every `interval` seconds.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeviceCode {
    #[serde(skip)]
    pub device_code: String,
    pub user_code: String,
    pub verify_url: String,
    pub verify_url_full: Option<String>,
    pub interval: u64,
    pub expires_in: u64,
}

/// What the owner's sign-in gave: kept in the credential store as JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Token {
    pub access: String,
    pub refresh: Option<String>,
    /// Unix seconds.
    pub expires_at: i64,
    /// The account's name on the site.
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SignInPoll {
    Pending,
    /// Asked too often: wait longer.
    SlowDown,
    Done(Token),
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
        Err(unsupported("activity"))
    }

    /// How many recent entries the account shows, or lookups it has left today; a coded
    /// error says what is wrong.
    fn check(&self, account: &Account, http: &dyn Http) -> Result<u32>;

    /// The public scores of `films` (at most `batch`). A spent allowance is the coded error
    /// `service.limit_reached` with the param `until` (unix seconds).
    fn reception(
        &self,
        _account: &Account,
        _films: &[TmdbFilm],
        _http: &dyn Http,
    ) -> Result<Lookup<Score>> {
        Err(unsupported("reception"))
    }

    /// The awards of `films` (at most `batch`), named in `langs` where the service can.
    fn awards(
        &self,
        _films: &[TmdbFilm],
        _langs: &[&str],
        _http: &dyn Http,
    ) -> Result<Lookup<Award>> {
        Err(unsupported("awards"))
    }

    /// Starts a sign-in (`Need::SignIn`).
    fn sign_in(&self, _http: &dyn Http) -> Result<DeviceCode> {
        Err(unsupported("sign-in"))
    }

    /// Whether the owner has approved yet. Denied and expired are the coded errors
    /// `service.sign_in_denied` and `service.sign_in_expired`.
    fn sign_in_poll(&self, _device_code: &str, _http: &dyn Http) -> Result<SignInPoll> {
        Err(unsupported("sign-in"))
    }

    /// A new token for one about to expire; `service.signed_out` when the site refuses.
    fn refresh_token(&self, _token: &Token, _http: &dyn Http) -> Result<Token> {
        Err(unsupported("sign-in"))
    }
}

fn unsupported(what: &str) -> Error {
    Error::Invalid(Detail::coded(
        "service.unsupported",
        format!("this service does not provide {what}"),
    ))
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
