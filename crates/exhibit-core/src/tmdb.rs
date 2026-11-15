//! TMDB (API v3) metadata provider over a swappable HTTP layer. The stored credential is
//! either a v3 key or a v4 read token.

use crate::error::{Error, Result};
use crate::model::{ItemKind, Person};
use crate::nfo::ExternalId;
use serde_json::Value;
use std::time::Duration;

const API: &str = "https://api.themoviedb.org/3";
const IMAGE_BASE: &str = "https://image.tmdb.org/t/p";
const MAX_CAST: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub tmdb_id: i64,
    pub kind: ItemKind,
    pub title: String,
    pub original_title: String,
    pub year: Option<i32>,
    pub popularity: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MovieMeta {
    pub tmdb_id: i64,
    pub title: String,
    pub year: Option<i32>,
    pub overview: String,
    pub runtime_min: Option<i32>,
    pub genres: Vec<String>,
    pub people: Vec<Person>,
    pub poster: Option<String>,
    pub backdrop: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShowMeta {
    pub tmdb_id: i64,
    pub title: String,
    pub year: Option<i32>,
    pub overview: String,
    pub genres: Vec<String>,
    pub people: Vec<Person>,
    pub poster: Option<String>,
    pub backdrop: Option<String>,
    /// (season number, episode count)
    pub seasons: Vec<(i32, u32)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EpisodeMeta {
    pub episode: i32,
    pub title: String,
    pub overview: String,
    pub runtime_min: Option<i32>,
    pub still: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeasonMeta {
    pub season: i32,
    pub poster: Option<String>,
    pub episodes: Vec<EpisodeMeta>,
}

pub trait MetadataProvider {
    fn search_movie(&self, title: &str, year: Option<i32>) -> Result<Vec<SearchHit>>;
    fn search_show(&self, title: &str, year: Option<i32>) -> Result<Vec<SearchHit>>;
    fn find_external(&self, id: &ExternalId) -> Result<Option<SearchHit>>;
    fn movie(&self, tmdb_id: i64) -> Result<MovieMeta>;
    fn show(&self, tmdb_id: i64) -> Result<ShowMeta>;
    fn season(&self, show_id: i64, season: i32) -> Result<SeasonMeta>;
    fn image(&self, path: &str, size: &str) -> Result<Vec<u8>>;
}

pub trait Http {
    /// GET `url`, optionally with a Bearer token; returns the status and body. Only a request
    /// that got no response is an error (`Error::Provider`).
    fn get(&self, url: &str, bearer: Option<&str>) -> Result<(u16, Vec<u8>)>;
}

/// The error for HTTP 401 or 403: retrying with the same key cannot succeed.
pub const KEY_REJECTED: &str = "TMDB rejected the key; set a valid one with `exhibit key set tmdb`";

pub struct Tmdb<H: Http> {
    key: String,
    http: H,
}

impl<H: Http> Tmdb<H> {
    pub fn new(key: String, http: H) -> Self {
        Tmdb { key, http }
    }

    /// A key starting with `eyJ` is a v4 read token (a JWT, sent as Bearer); anything else is
    /// a v3 key, sent as the `api_key` query parameter.
    fn json(&self, path: &str, params: &[(&str, &str)]) -> Result<Value> {
        let mut url = format!("{API}{path}");
        let mut sep = '?';
        for (k, v) in params {
            url.push_str(&format!("{sep}{k}={}", encode(v)));
            sep = '&';
        }
        let bearer = self.key.starts_with("eyJ");
        if !bearer {
            url.push_str(&format!("{sep}api_key={}", encode(&self.key)));
        }
        let body = self.fetch(&url, bearer.then_some(self.key.as_str()))?;
        serde_json::from_slice(&body)
            .map_err(|e| Error::Provider(format!("invalid JSON from TMDB: {e}")))
    }

    /// The body of a 200 response. 404 is `NotFound`, 401 and 403 are `Invalid`
    /// (`KEY_REJECTED`), any other status a `Provider` error.
    fn fetch(&self, url: &str, bearer: Option<&str>) -> Result<Vec<u8>> {
        let (status, body) = self.http.get(url, bearer).map_err(|e| self.redact(e))?;
        match status {
            200 => Ok(body),
            401 | 403 => Err(Error::Invalid(KEY_REJECTED.into())),
            // The path alone: the query may hold the key.
            404 => Err(Error::NotFound(format!(
                "TMDB has nothing at {}",
                url.split('?').next().unwrap_or_default()
            ))),
            s => Err(Error::Provider(format!("HTTP {s}"))),
        }
    }

    /// Replaces the key with `***` in an error's text: a transport error may echo the URL.
    fn redact(&self, e: Error) -> Error {
        if self.key.is_empty() {
            return e;
        }
        let hide = |m: String| {
            m.replace(&encode(&self.key), "***")
                .replace(&self.key, "***")
        };
        match e {
            Error::Provider(m) => Error::Provider(hide(m)),
            Error::NotFound(m) => Error::NotFound(hide(m)),
            Error::Invalid(m) => Error::Invalid(hide(m)),
            other => other,
        }
    }

    fn search(
        &self,
        path: &str,
        year_param: &str,
        kind: ItemKind,
        title: &str,
        year: Option<i32>,
    ) -> Result<Vec<SearchHit>> {
        let year = year.map(|y| y.to_string());
        let mut params = vec![("query", title)];
        if let Some(y) = &year {
            params.push((year_param, y));
        }
        let v = self.json(path, &params)?;
        Ok(hits(&v["results"], kind))
    }
}

impl<H: Http> MetadataProvider for Tmdb<H> {
    fn search_movie(&self, title: &str, year: Option<i32>) -> Result<Vec<SearchHit>> {
        self.search("/search/movie", "year", ItemKind::Movie, title, year)
    }

    fn search_show(&self, title: &str, year: Option<i32>) -> Result<Vec<SearchHit>> {
        self.search(
            "/search/tv",
            "first_air_date_year",
            ItemKind::Show,
            title,
            year,
        )
    }

    fn find_external(&self, id: &ExternalId) -> Result<Option<SearchHit>> {
        let (ext, source) = match id {
            // A bare TMDB id is a movie's; a show's comes as `TmdbTv`.
            ExternalId::Tmdb(n) => {
                let m = self.movie(*n)?;
                return Ok(Some(SearchHit {
                    tmdb_id: m.tmdb_id,
                    kind: ItemKind::Movie,
                    original_title: m.title.clone(),
                    title: m.title,
                    year: m.year,
                    popularity: 0.0,
                }));
            }
            ExternalId::TmdbTv(n) => {
                let s = self.show(*n)?;
                return Ok(Some(SearchHit {
                    tmdb_id: s.tmdb_id,
                    kind: ItemKind::Show,
                    original_title: s.title.clone(),
                    title: s.title,
                    year: s.year,
                    popularity: 0.0,
                }));
            }
            ExternalId::Imdb(s) => (s.clone(), "imdb_id"),
            ExternalId::Tvdb(n) => (n.to_string(), "tvdb_id"),
        };
        let v = self.json(
            &format!("/find/{}", encode(&ext)),
            &[("external_source", source)],
        )?;
        let movies = hits(&v["movie_results"], ItemKind::Movie);
        let shows = hits(&v["tv_results"], ItemKind::Show);
        // TVDB ids are shows; IMDb ids are mostly movies.
        let (first, second) = if matches!(id, ExternalId::Tvdb(_)) {
            (shows, movies)
        } else {
            (movies, shows)
        };
        Ok(first.into_iter().chain(second).next())
    }

    fn movie(&self, tmdb_id: i64) -> Result<MovieMeta> {
        let v = self.json(
            &format!("/movie/{tmdb_id}"),
            &[("append_to_response", "credits")],
        )?;
        Ok(MovieMeta {
            tmdb_id: v["id"].as_i64().unwrap_or(tmdb_id),
            title: text(&v["title"]),
            year: year_of(&v["release_date"]),
            overview: text(&v["overview"]),
            runtime_min: int(&v["runtime"]),
            genres: names(&v["genres"]),
            people: people(crew_directors(&v), &v),
            poster: opt_text(&v["poster_path"]),
            backdrop: opt_text(&v["backdrop_path"]),
        })
    }

    fn show(&self, tmdb_id: i64) -> Result<ShowMeta> {
        let v = self.json(
            &format!("/tv/{tmdb_id}"),
            &[("append_to_response", "credits")],
        )?;
        let seasons = arr(&v["seasons"])
            .iter()
            .filter_map(|s| {
                let n = int(&s["season_number"])?;
                Some((n, s["episode_count"].as_u64().unwrap_or(0) as u32))
            })
            .collect();
        Ok(ShowMeta {
            tmdb_id: v["id"].as_i64().unwrap_or(tmdb_id),
            title: text(&v["name"]),
            year: year_of(&v["first_air_date"]),
            overview: text(&v["overview"]),
            genres: names(&v["genres"]),
            people: people(names(&v["created_by"]), &v),
            poster: opt_text(&v["poster_path"]),
            backdrop: opt_text(&v["backdrop_path"]),
            seasons,
        })
    }

    fn season(&self, show_id: i64, season: i32) -> Result<SeasonMeta> {
        let v = self.json(&format!("/tv/{show_id}/season/{season}"), &[])?;
        let episodes = arr(&v["episodes"])
            .iter()
            .filter_map(|e| {
                Some(EpisodeMeta {
                    episode: int(&e["episode_number"])?,
                    title: text(&e["name"]),
                    overview: text(&e["overview"]),
                    runtime_min: int(&e["runtime"]),
                    still: opt_text(&e["still_path"]),
                })
            })
            .collect();
        Ok(SeasonMeta {
            season: int(&v["season_number"]).unwrap_or(season),
            poster: opt_text(&v["poster_path"]),
            episodes,
        })
    }

    fn image(&self, path: &str, size: &str) -> Result<Vec<u8>> {
        self.fetch(&format!("{IMAGE_BASE}/{size}{path}"), None)
    }
}

fn arr(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

fn opt_text(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

fn int(v: &Value) -> Option<i32> {
    v.as_i64().and_then(|n| i32::try_from(n).ok())
}

fn year_of(v: &Value) -> Option<i32> {
    v.as_str()?.get(..4)?.parse().ok()
}

/// The `name` of every entry in a JSON array of objects.
fn names(v: &Value) -> Vec<String> {
    arr(v).iter().filter_map(|p| opt_text(&p["name"])).collect()
}

fn crew_directors(v: &Value) -> Vec<String> {
    arr(&v["credits"]["crew"])
        .iter()
        .filter(|c| c["job"] == "Director")
        .filter_map(|c| opt_text(&c["name"]))
        .collect()
}

/// Directors first, then the first `MAX_CAST` cast members.
fn people(directors: Vec<String>, v: &Value) -> Vec<Person> {
    let person = |role: &'static str| {
        move |name| Person {
            name,
            role: role.to_string(),
        }
    };
    let cast = names(&v["credits"]["cast"]).into_iter().take(MAX_CAST);
    directors
        .into_iter()
        .map(person("director"))
        .chain(cast.map(person("cast")))
        .collect()
}

fn hits(results: &Value, kind: ItemKind) -> Vec<SearchHit> {
    let (title, orig, date) = match kind {
        ItemKind::Show => ("name", "original_name", "first_air_date"),
        _ => ("title", "original_title", "release_date"),
    };
    arr(results)
        .iter()
        .filter_map(|r| {
            Some(SearchHit {
                tmdb_id: r["id"].as_i64()?,
                kind,
                title: text(&r[title]),
                original_title: text(&r[orig]),
                year: year_of(&r[date]),
                popularity: r["popularity"].as_f64().unwrap_or(0.0) as f32,
            })
        })
        .collect()
}

/// Percent-encode everything except RFC 3986 unreserved characters.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Real HTTP: 10 s timeout, one retry on 429 after `Retry-After` (capped at 10 s).
pub struct UreqHttp {
    agent: ureq::Agent,
}

impl UreqHttp {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .http_status_as_error(false)
            .build()
            .into();
        UreqHttp { agent }
    }
}

impl Default for UreqHttp {
    fn default() -> Self {
        Self::new()
    }
}

impl Http for UreqHttp {
    fn get(&self, url: &str, bearer: Option<&str>) -> Result<(u16, Vec<u8>)> {
        let mut retried = false;
        loop {
            let mut req = self.agent.get(url);
            if let Some(token) = bearer {
                req = req.header("Authorization", &format!("Bearer {token}"));
            }
            let mut resp = req
                .call()
                .map_err(|e| Error::Provider(format!("request failed: {e}")))?;
            let status = resp.status().as_u16();
            if status == 429 && !retried {
                retried = true;
                let wait = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.trim().parse::<u64>().ok())
                    .unwrap_or(1);
                std::thread::sleep(Duration::from_secs(wait.min(10)));
                continue;
            }
            if status != 200 {
                return Ok((status, Vec::new()));
            }
            let body = resp
                .body_mut()
                .read_to_vec()
                .map_err(|e| Error::Provider(format!("reading response failed: {e}")))?;
            return Ok((status, body));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct FakeHttp {
        routes: Vec<(&'static str, &'static str)>,
        /// Status of every response; 200 serves the routes (404 when none matches).
        status: u16,
        log: RefCell<Vec<(String, Option<String>)>>,
    }

    fn fixture(name: &str) -> Vec<u8> {
        let p = format!("{}/tests/fixtures/tmdb/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read(p).unwrap()
    }

    impl Http for &FakeHttp {
        fn get(&self, url: &str, bearer: Option<&str>) -> Result<(u16, Vec<u8>)> {
            self.log
                .borrow_mut()
                .push((url.to_string(), bearer.map(str::to_string)));
            if self.status != 200 {
                return Ok((self.status, b"{}".to_vec()));
            }
            Ok(self
                .routes
                .iter()
                .find(|(k, _)| url.contains(k))
                .map_or((404, b"{}".to_vec()), |(_, f)| (200, fixture(f))))
        }
    }

    fn fake(routes: &[(&'static str, &'static str)]) -> FakeHttp {
        FakeHttp {
            routes: routes.to_vec(),
            status: 200,
            log: RefCell::new(vec![]),
        }
    }

    /// Fails every request with a message that echoes the URL and the token, as a transport
    /// error may.
    struct EchoHttp;

    impl Http for EchoHttp {
        fn get(&self, url: &str, bearer: Option<&str>) -> Result<(u16, Vec<u8>)> {
            Err(Error::Provider(format!("request failed: {url} {bearer:?}")))
        }
    }

    fn last_url(f: &FakeHttp) -> String {
        f.log.borrow().last().unwrap().0.clone()
    }

    #[test]
    fn search_movie_parses_hits_and_years() {
        let f = fake(&[("/search/movie", "search_movie_matrix.json")]);
        let t = Tmdb::new("abc123".into(), &f);
        let hits = t.search_movie("The Matrix", Some(1999)).unwrap();
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].tmdb_id, 603);
        assert_eq!(hits[0].kind, ItemKind::Movie);
        assert_eq!(hits[0].title, "The Matrix");
        assert_eq!(hits[0].year, Some(1999));
        assert!((hits[0].popularity - 87.4).abs() < 0.01);
        assert_eq!(
            last_url(&f),
            "https://api.themoviedb.org/3/search/movie?query=The%20Matrix&year=1999&api_key=abc123"
        );
    }

    #[test]
    fn search_hit_with_empty_date_has_no_year() {
        let f = fake(&[("/search/movie", "search_movie_matrix.json")]);
        let hits = Tmdb::new("k".into(), &f)
            .search_movie("Matrix", None)
            .unwrap();
        assert_eq!(hits[2].year, None);
        assert!(!last_url(&f).contains("year="));
    }

    #[test]
    fn search_show_uses_tv_fields() {
        let f = fake(&[("/search/tv", "search_tv_severance.json")]);
        let hits = Tmdb::new("k".into(), &f)
            .search_show("Severance", Some(2022))
            .unwrap();
        assert_eq!(hits[0].kind, ItemKind::Show);
        assert_eq!(hits[0].title, "Severance");
        assert_eq!(hits[0].year, Some(2022));
        assert!(last_url(&f).contains("first_air_date_year=2022"));
    }

    #[test]
    fn query_is_url_encoded() {
        let f = fake(&[("/search/movie", "search_movie_matrix.json")]);
        Tmdb::new("k".into(), &f)
            .search_movie("Amélie & Co", None)
            .unwrap();
        assert_eq!(
            last_url(&f),
            "https://api.themoviedb.org/3/search/movie?query=Am%C3%A9lie%20%26%20Co&api_key=k"
        );
    }

    const V4_TOKEN: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJzZWtyaXQifQ.c2lnbmF0dXJl";

    #[test]
    fn v4_token_is_bearer_and_anything_else_is_api_key() {
        let f = fake(&[("/search/movie", "search_movie_matrix.json")]);
        Tmdb::new(V4_TOKEN.into(), &f)
            .search_movie("x", None)
            .unwrap();
        let (url, bearer) = f.log.borrow()[0].clone();
        assert!(!url.contains("api_key"));
        assert_eq!(bearer.as_deref(), Some(V4_TOKEN));

        for key in ["0123456789abcdef0123456789abcdef", "a.b"] {
            Tmdb::new(key.into(), &f).search_movie("x", None).unwrap();
            let (url, bearer) = f.log.borrow().last().unwrap().clone();
            assert!(url.ends_with(&format!("&api_key={key}")), "{url}");
            assert_eq!(bearer, None);
        }
    }

    #[test]
    fn movie_details() {
        let f = fake(&[("/movie/603", "movie_603.json")]);
        let m = Tmdb::new("k".into(), &f).movie(603).unwrap();
        assert!(last_url(&f).contains("append_to_response=credits"));
        assert_eq!(m.title, "The Matrix");
        assert_eq!(m.year, Some(1999));
        assert_eq!(m.runtime_min, Some(136));
        assert_eq!(m.genres, ["Action", "Science Fiction"]);
        assert_eq!(
            m.poster.as_deref(),
            Some("/f89U3ADr1oiB1s9GkdPOEpXUk5H.jpg")
        );
        let directors: Vec<_> = m
            .people
            .iter()
            .filter(|p| p.role == "director")
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(directors, ["Lilly Wachowski", "Lana Wachowski"]);
        assert_eq!(m.people.len(), 2 + 8);
        assert_eq!(
            m.people[2],
            Person {
                name: "Keanu Reeves".into(),
                role: "cast".into()
            }
        );
    }

    #[test]
    fn show_details_and_seasons() {
        let f = fake(&[("/tv/95396", "tv_95396.json")]);
        let s = Tmdb::new("k".into(), &f).show(95396).unwrap();
        assert_eq!(s.title, "Severance");
        assert_eq!(s.year, Some(2022));
        assert_eq!(s.seasons, [(0, 3), (1, 9), (2, 10)]);
        assert_eq!(s.backdrop, None);
        assert_eq!(
            s.people[0],
            Person {
                name: "Dan Erickson".into(),
                role: "director".into()
            }
        );
        assert_eq!(s.people.len(), 3);
    }

    #[test]
    fn season_episodes() {
        let f = fake(&[("/tv/95396/season/2", "tv_95396_season_2.json")]);
        let s = Tmdb::new("k".into(), &f).season(95396, 2).unwrap();
        assert_eq!(s.season, 2);
        assert_eq!(s.poster.as_deref(), Some("/season2.jpg"));
        assert_eq!(s.episodes.len(), 2);
        assert_eq!(s.episodes[0].title, "Hello, Ms. Cobel");
        assert_eq!(s.episodes[0].runtime_min, Some(52));
        assert_eq!(s.episodes[0].still.as_deref(), Some("/still1.jpg"));
        assert_eq!(s.episodes[1].runtime_min, None);
        assert_eq!(s.episodes[1].still, None);
    }

    #[test]
    fn details_tolerate_nulls_and_missing_credits() {
        let f = fake(&[("/movie/1", "movie_nulls.json")]);
        let m = Tmdb::new("k".into(), &f).movie(1).unwrap();
        assert_eq!(m.year, None);
        assert_eq!(m.runtime_min, None);
        assert_eq!(m.poster, None);
        assert_eq!(m.backdrop, None);
        assert_eq!(m.overview, "");
        assert!(m.genres.is_empty() && m.people.is_empty());
    }

    #[test]
    fn find_imdb_yields_movie_hit() {
        let f = fake(&[("/find/tt0133093", "find_tt0133093.json")]);
        let hit = Tmdb::new("k".into(), &f)
            .find_external(&ExternalId::Imdb("tt0133093".into()))
            .unwrap()
            .unwrap();
        assert_eq!(
            (hit.tmdb_id, hit.kind, hit.year),
            (603, ItemKind::Movie, Some(1999))
        );
        assert!(last_url(&f).contains("external_source=imdb_id"));
    }

    #[test]
    fn find_with_no_results_is_none() {
        let f = fake(&[("/find/", "find_empty.json")]);
        let t = Tmdb::new("k".into(), &f);
        assert_eq!(t.find_external(&ExternalId::Tvdb(1)).unwrap(), None);
        assert!(last_url(&f).contains("external_source=tvdb_id"));
    }

    #[test]
    fn find_tmdb_id_asks_for_the_movie_only() {
        let f = fake(&[
            ("/tv/95396", "tv_95396.json"),
            ("/movie/603", "movie_603.json"),
        ]);
        let t = Tmdb::new("k".into(), &f);
        let hit = t.find_external(&ExternalId::Tmdb(603)).unwrap().unwrap();
        assert_eq!((hit.tmdb_id, hit.kind), (603, ItemKind::Movie));
        assert!(matches!(
            t.find_external(&ExternalId::Tmdb(95396)),
            Err(Error::NotFound(_))
        ));
        assert!(f.log.borrow().iter().all(|(url, _)| !url.contains("/tv/")));
    }

    #[test]
    fn status_codes_map_to_errors() {
        let mut f = fake(&[]);
        assert!(matches!(
            Tmdb::new("k".into(), &f).movie(7),
            Err(Error::NotFound(_))
        ));
        for status in [401, 403] {
            f.status = status;
            let e = Tmdb::new("k".into(), &f).movie(603).unwrap_err();
            assert!(matches!(&e, Error::Invalid(m) if m == KEY_REJECTED), "{e}");
        }
        f.status = 500;
        assert!(matches!(
            Tmdb::new("k".into(), &f).movie(603),
            Err(Error::Provider(_))
        ));
    }

    #[test]
    fn find_tmdb_tv_id_asks_for_the_show_only() {
        let f = fake(&[
            ("/tv/95396", "tv_95396.json"),
            ("/movie/603", "movie_603.json"),
        ]);
        let t = Tmdb::new("k".into(), &f);
        let hit = t
            .find_external(&ExternalId::TmdbTv(95396))
            .unwrap()
            .unwrap();
        assert_eq!((hit.tmdb_id, hit.kind), (95396, ItemKind::Show));
        assert!(matches!(
            t.find_external(&ExternalId::TmdbTv(603)),
            Err(Error::NotFound(_))
        ));
        assert!(
            f.log
                .borrow()
                .iter()
                .all(|(url, _)| !url.contains("/movie/"))
        );
    }

    #[test]
    fn errors_never_carry_the_key() {
        let v3 = "5ec4e75ec4e75ec4e75ec4e75ec4e75e";
        for key in [v3, V4_TOKEN] {
            let e = Tmdb::new(key.into(), EchoHttp)
                .movie(9)
                .unwrap_err()
                .to_string();
            assert!(!e.contains(key), "{e}");
            assert!(e.contains("***"), "{e}");
        }
        let e = Tmdb::new(v3.into(), EchoHttp).movie(9).unwrap_err();
        assert!(e.to_string().contains("api_key=***"), "{e}");
    }

    #[test]
    fn image_url_and_unmapped_url_error() {
        let f = fake(&[("image.tmdb.org", "movie_603.json")]);
        let t = Tmdb::new("k".into(), &f);
        t.image("/abc.jpg", "w500").unwrap();
        assert_eq!(last_url(&f), "https://image.tmdb.org/t/p/w500/abc.jpg");
        assert!(matches!(t.movie(9), Err(Error::NotFound(_))));
    }
}
