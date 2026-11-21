//! TMDB (API v3) metadata provider over a swappable HTTP layer. The stored credential is
//! either a v3 key or a v4 read token.

use crate::error::{Detail, Error, Result};
use crate::lang;
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
    /// TMDB's poster path, for `image()`.
    pub poster: Option<String>,
    /// Its title in each language that found it, when it was searched in several.
    pub titles: Vec<(&'static str, String)>,
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

/// TMDB as identification sees it. Details (`movie`, `show`, `season`) come in English, the
/// library's base language; searches and `texts` in the language asked for.
pub trait MetadataProvider {
    fn search_movie(&self, title: &str, year: Option<i32>, lang: &str) -> Result<Vec<SearchHit>>;
    fn search_show(&self, title: &str, year: Option<i32>, lang: &str) -> Result<Vec<SearchHit>>;
    fn find_external(&self, id: &ExternalId) -> Result<Option<SearchHit>>;
    fn movie(&self, tmdb_id: i64) -> Result<MovieMeta>;
    fn show(&self, tmdb_id: i64) -> Result<ShowMeta>;
    fn season(&self, show_id: i64, season: i32) -> Result<SeasonMeta>;
    fn image(&self, path: &str, size: &str) -> Result<Vec<u8>>;
    /// The title, overview and genres of movie or show `tmdb_id` in `lang`, empty where TMDB
    /// has no translation, and its poster only when one is tagged with that language.
    fn texts(&self, kind: ItemKind, tmdb_id: i64, lang: &str) -> Result<Texts>;
    /// A season's episode titles and overviews in `lang`, and its poster in that language.
    fn season_texts(&self, show_id: i64, season: i32, lang: &str) -> Result<SeasonTexts>;
}

/// A work's text in one language. Empty strings are gaps TMDB has not translated.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Texts {
    pub title: String,
    pub overview: String,
    pub genres: Vec<String>,
    /// TMDB's poster path, only when the poster is in this language.
    pub poster: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SeasonTexts {
    pub poster: Option<String>,
    /// (episode number, title, overview)
    pub episodes: Vec<(i32, String, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

/// A request with what `get` cannot say: a method, a body and a client name.
#[derive(Clone, Debug, PartialEq)]
pub struct Request<'a> {
    pub method: Method,
    pub url: &'a str,
    pub bearer: Option<&'a str>,
    /// The content type and the body.
    pub body: Option<(&'static str, Vec<u8>)>,
    /// Some sites ask automated clients to name themselves.
    pub user_agent: Option<&'a str>,
}

impl<'a> Request<'a> {
    pub fn get(url: &'a str) -> Self {
        Request {
            method: Method::Get,
            url,
            bearer: None,
            body: None,
            user_agent: None,
        }
    }

    /// A form post (`application/x-www-form-urlencoded`).
    pub fn form(url: &'a str, fields: &[(&str, &str)]) -> Self {
        let body = fields
            .iter()
            .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        Request {
            method: Method::Post,
            body: Some(("application/x-www-form-urlencoded", body.into_bytes())),
            ..Request::get(url)
        }
    }

    pub fn json(url: &'a str, body: &serde_json::Value) -> Self {
        Request {
            method: Method::Post,
            body: Some(("application/json", body.to_string().into_bytes())),
            ..Request::get(url)
        }
    }

    pub fn bearer(mut self, token: Option<&'a str>) -> Self {
        self.bearer = token;
        self
    }
}

/// A response of any status, with its headers (names lowercased) and its body.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }
}

pub trait Http {
    /// GET `url`, optionally with a Bearer token; returns the status and body. Only a request
    /// that got no response is an error (`Error::Provider`).
    fn get(&self, url: &str, bearer: Option<&str>) -> Result<(u16, Vec<u8>)>;

    /// Any request, answered whatever its status, never retried. Only a request that got no
    /// response is an error. Clients that only `get` serve GETs here, without headers.
    fn send(&self, req: &Request) -> Result<Response> {
        match req.method {
            Method::Get => {
                let (status, body) = self.get(req.url, req.bearer)?;
                Ok(Response {
                    status,
                    headers: Vec::new(),
                    body,
                })
            }
            Method::Post => Err(Error::Provider(Detail::coded(
                "service.unreachable",
                "this client cannot post",
            ))),
        }
    }
}

impl<H: Http + ?Sized> Http for &H {
    fn get(&self, url: &str, bearer: Option<&str>) -> Result<(u16, Vec<u8>)> {
        (**self).get(url, bearer)
    }

    fn send(&self, req: &Request) -> Result<Response> {
        (**self).send(req)
    }
}

/// Must match `build.rs`.
const PAD: &[u8] = b"exhibit-gallery";

/// Exhibit's own TMDB key, when the build baked one in (`build.rs`).
pub fn builtin_key() -> Option<String> {
    unscramble(env!("EXHIBIT_TMDB_BUILTIN"))
}

fn unscramble(hex: &str) -> Option<String> {
    if hex.is_empty() || !hex.len().is_multiple_of(2) {
        return None;
    }
    let bytes = (0..hex.len() / 2)
        .map(|i| {
            let b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()?;
            Some(b ^ PAD[i % PAD.len()])
        })
        .collect::<Option<Vec<u8>>>()?;
    String::from_utf8(bytes).ok().filter(|k| !k.is_empty())
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
    ///
    /// Every call names its language: `lang` in `params`, or else English, the base.
    fn json(&self, path: &str, params: &[(&str, &str)]) -> Result<Value> {
        let mut url = format!("{API}{path}");
        let mut sep = '?';
        let base = [("language", lang::BASE)];
        let named = params.iter().any(|(k, _)| *k == "language");
        for (k, v) in params.iter().chain(base.iter().filter(|_| !named)) {
            url.push_str(&format!("{sep}{k}={}", url_encode(v)));
            sep = '&';
        }
        let bearer = self.key.starts_with("eyJ");
        if !bearer {
            url.push_str(&format!("{sep}api_key={}", url_encode(&self.key)));
        }
        let body = self.fetch(&url, bearer.then_some(self.key.as_str()))?;
        serde_json::from_slice(&body).map_err(|e| {
            Error::Provider(Detail::coded(
                "tmdb.unreadable",
                format!("invalid JSON from TMDB: {e}"),
            ))
        })
    }

    /// The body of a 200 response. 404 is `NotFound`, 401 and 403 are `Invalid`
    /// (`KEY_REJECTED`), any other status a `Provider` error.
    fn fetch(&self, url: &str, bearer: Option<&str>) -> Result<Vec<u8>> {
        let (status, body) = self.http.get(url, bearer).map_err(|e| self.redact(e))?;
        match status {
            200 => Ok(body),
            401 | 403 => Err(Error::Invalid(Detail::coded(
                "tmdb.key_rejected",
                KEY_REJECTED,
            ))),
            // The path alone: the query may hold the key.
            404 => Err(Error::NotFound(
                format!(
                    "TMDB has nothing at {}",
                    url.split('?').next().unwrap_or_default()
                )
                .into(),
            )),
            s => Err(Error::Provider(
                Detail::coded("tmdb.http", format!("HTTP {s}")).with("status", s),
            )),
        }
    }

    /// Replaces the key with `***` in an error's text: a transport error may echo the URL.
    fn redact(&self, e: Error) -> Error {
        if self.key.is_empty() {
            return e;
        }
        let hide = |mut m: Detail| {
            m.text = m
                .text
                .replace(&url_encode(&self.key), "***")
                .replace(&self.key, "***");
            m
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
        lang: &str,
    ) -> Result<Vec<SearchHit>> {
        let year = year.map(|y| y.to_string());
        let mut params = vec![("query", title), ("language", lang)];
        if let Some(y) = &year {
            params.push((year_param, y));
        }
        let v = self.json(path, &params)?;
        Ok(hits(&v["results"], kind))
    }
}

impl<H: Http> MetadataProvider for Tmdb<H> {
    fn search_movie(&self, title: &str, year: Option<i32>, lang: &str) -> Result<Vec<SearchHit>> {
        self.search("/search/movie", "year", ItemKind::Movie, title, year, lang)
    }

    fn search_show(&self, title: &str, year: Option<i32>, lang: &str) -> Result<Vec<SearchHit>> {
        self.search(
            "/search/tv",
            "first_air_date_year",
            ItemKind::Show,
            title,
            year,
            lang,
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
                    poster: m.poster,
                    titles: Vec::new(),
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
                    poster: s.poster,
                    titles: Vec::new(),
                }));
            }
            ExternalId::Imdb(s) => (s.clone(), "imdb_id"),
            ExternalId::Tvdb(n) => (n.to_string(), "tvdb_id"),
        };
        let v = self.json(
            &format!("/find/{}", url_encode(&ext)),
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

    fn texts(&self, kind: ItemKind, tmdb_id: i64, lang: &str) -> Result<Texts> {
        let (path, title) = match kind {
            ItemKind::Show => (format!("/tv/{tmdb_id}"), "name"),
            _ => (format!("/movie/{tmdb_id}"), "title"),
        };
        let v = self.json(&path, &localized(lang))?;
        Ok(Texts {
            title: text(&v[title]),
            overview: text(&v["overview"]),
            genres: names(&v["genres"]),
            poster: own_poster(&v),
        })
    }

    fn season_texts(&self, show_id: i64, season: i32, lang: &str) -> Result<SeasonTexts> {
        let v = self.json(&format!("/tv/{show_id}/season/{season}"), &localized(lang))?;
        let episodes = arr(&v["episodes"])
            .iter()
            .filter_map(|e| {
                Some((
                    int(&e["episode_number"])?,
                    text(&e["name"]),
                    text(&e["overview"]),
                ))
            })
            .collect();
        Ok(SeasonTexts {
            poster: own_poster(&v),
            episodes,
        })
    }
}

/// The query for details in `lang` with the posters tagged with its language alone: TMDB
/// tags images by language without region (`es`), and `poster_path` falls back to the
/// work's original language, so only a tagged poster counts as this language's.
fn localized(lang: &str) -> [(&str, &str); 3] {
    [
        ("language", lang),
        ("append_to_response", "images"),
        ("include_image_language", lang::image_language(lang)),
    ]
}

/// `poster_path` when TMDB has a poster in the language asked for. Its choice among them is
/// the one the website shows; without any, the poster stays the base language's.
fn own_poster(v: &Value) -> Option<String> {
    if arr(&v["images"]["posters"]).is_empty() {
        return None;
    }
    opt_text(&v["poster_path"])
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
    let cast = arr(&v["credits"]["cast"])
        .iter()
        .filter_map(|c| {
            Some(Person {
                name: opt_text(&c["name"])?,
                role: "cast".into(),
                tmdb_id: c["id"].as_i64(),
                character: opt_text(&c["character"]),
                profile: opt_text(&c["profile_path"]),
            })
        })
        .take(MAX_CAST);
    directors
        .into_iter()
        .map(|name| Person {
            name,
            role: "director".into(),
            ..Person::default()
        })
        .chain(cast)
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
                poster: opt_text(&r["poster_path"]),
                titles: Vec::new(),
            })
        })
        .collect()
}

/// Percent-encode everything except RFC 3986 unreserved characters.
pub fn url_encode(s: &str) -> String {
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
            let mut resp = req.call().map_err(|e| {
                Error::Provider(Detail::coded(
                    "tmdb.unreachable",
                    format!("request failed: {e}"),
                ))
            })?;
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
            let body = resp.body_mut().read_to_vec().map_err(|e| {
                Error::Provider(Detail::coded(
                    "tmdb.unreachable",
                    format!("reading response failed: {e}"),
                ))
            })?;
            return Ok((status, body));
        }
    }

    fn send(&self, req: &Request) -> Result<Response> {
        let unreachable = |why: String| Error::Provider(Detail::coded("service.unreachable", why));
        let mut headers = vec![("Accept", "application/json".to_string())];
        if let Some(token) = req.bearer {
            headers.push(("Authorization", format!("Bearer {token}")));
        }
        if let Some(ua) = req.user_agent {
            headers.push(("User-Agent", ua.to_string()));
        }
        let sent = if req.method == Method::Get {
            let mut r = self
                .agent
                .get(req.url)
                .config()
                .timeout_global(Some(SEND_TIMEOUT))
                .build();
            for (k, v) in &headers {
                r = r.header(*k, v);
            }
            r.call()
        } else {
            let mut r = self
                .agent
                .post(req.url)
                .config()
                .timeout_global(Some(SEND_TIMEOUT))
                .build();
            for (k, v) in &headers {
                r = r.header(*k, v);
            }
            match &req.body {
                Some((ty, bytes)) => r.header("Content-Type", *ty).send(&bytes[..]),
                None => r.send_empty(),
            }
        };
        let mut resp = sent.map_err(|e| unreachable(format!("request failed: {e}")))?;
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .filter_map(|(k, v)| {
                Some((
                    k.as_str().to_ascii_lowercase(),
                    v.to_str().ok()?.to_string(),
                ))
            })
            .collect();
        let body = resp
            .body_mut()
            .with_config()
            .limit(SEND_LIMIT)
            .read_to_vec()
            .map_err(|e| unreachable(format!("reading response failed: {e}")))?;
        Ok(Response {
            status,
            headers,
            body,
        })
    }
}

/// Batched lookups take longer than a TMDB call.
const SEND_TIMEOUT: Duration = Duration::from_secs(40);
/// A batch of a hundred films runs to a few hundred kilobytes.
const SEND_LIMIT: u64 = 32 * 1024 * 1024;

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
            Err(Error::Provider(
                format!("request failed: {url} {bearer:?}").into(),
            ))
        }
    }

    fn last_url(f: &FakeHttp) -> String {
        f.log.borrow().last().unwrap().0.clone()
    }

    #[test]
    fn search_movie_parses_hits_and_years() {
        let f = fake(&[("/search/movie", "search_movie_matrix.json")]);
        let t = Tmdb::new("abc123".into(), &f);
        let hits = t.search_movie("The Matrix", Some(1999), "en-US").unwrap();
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].tmdb_id, 603);
        assert_eq!(hits[0].kind, ItemKind::Movie);
        assert_eq!(hits[0].title, "The Matrix");
        assert_eq!(hits[0].year, Some(1999));
        assert!((hits[0].popularity - 87.4).abs() < 0.01);
        assert_eq!(
            hits[0].poster.as_deref(),
            Some("/f89U3ADr1oiB1s9GkdPOEpXUk5H.jpg")
        );
        assert_eq!(
            last_url(&f),
            "https://api.themoviedb.org/3/search/movie?query=The%20Matrix&language=en-US&year=1999&api_key=abc123"
        );
    }

    #[test]
    fn search_hit_with_empty_date_has_no_year() {
        let f = fake(&[("/search/movie", "search_movie_matrix.json")]);
        let hits = Tmdb::new("k".into(), &f)
            .search_movie("Matrix", None, "en-US")
            .unwrap();
        assert_eq!(hits[2].year, None);
        assert!(!last_url(&f).contains("year="));
    }

    #[test]
    fn search_show_uses_tv_fields() {
        let f = fake(&[("/search/tv", "search_tv_severance.json")]);
        let hits = Tmdb::new("k".into(), &f)
            .search_show("Severance", Some(2022), "en-US")
            .unwrap();
        assert_eq!(hits[0].kind, ItemKind::Show);
        assert_eq!(hits[0].title, "Severance");
        assert_eq!(hits[0].year, Some(2022));
        assert!(last_url(&f).contains("first_air_date_year=2022"));
    }

    #[test]
    fn a_baked_key_unscrambles_and_a_missing_one_is_none() {
        let key = "abc123";
        let hex: String = key
            .bytes()
            .enumerate()
            .map(|(i, b)| format!("{:02x}", b ^ PAD[i % PAD.len()]))
            .collect();
        assert_ne!(hex, "616263313233", "the key is not stored as plain text");
        assert_eq!(unscramble(&hex).as_deref(), Some(key));
        assert_eq!(unscramble(""), None);
        assert_eq!(unscramble("zz"), None);
        assert_eq!(unscramble("abc"), None);
    }

    #[test]
    fn query_is_url_encoded() {
        let f = fake(&[("/search/movie", "search_movie_matrix.json")]);
        Tmdb::new("k".into(), &f)
            .search_movie("Amélie & Co", None, "en-US")
            .unwrap();
        assert_eq!(
            last_url(&f),
            "https://api.themoviedb.org/3/search/movie?query=Am%C3%A9lie%20%26%20Co&language=en-US&api_key=k"
        );
    }

    const V4_TOKEN: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJzZWtyaXQifQ.c2lnbmF0dXJl";

    #[test]
    fn v4_token_is_bearer_and_anything_else_is_api_key() {
        let f = fake(&[("/search/movie", "search_movie_matrix.json")]);
        Tmdb::new(V4_TOKEN.into(), &f)
            .search_movie("x", None, "en-US")
            .unwrap();
        let (url, bearer) = f.log.borrow()[0].clone();
        assert!(!url.contains("api_key"));
        assert_eq!(bearer.as_deref(), Some(V4_TOKEN));

        for key in ["0123456789abcdef0123456789abcdef", "a.b"] {
            Tmdb::new(key.into(), &f)
                .search_movie("x", None, "en-US")
                .unwrap();
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
                role: "cast".into(),
                tmdb_id: Some(6384),
                character: Some("Thomas A. Anderson / Neo".into()),
                profile: Some("/4D0PpNI0kmP58hgrwGC3wCjxhnm.jpg".into()),
            }
        );
        // No portrait, and no character: each simply absent.
        assert_eq!(m.people[3].profile, None);
        assert_eq!(m.people[4].character, None);
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
                role: "director".into(),
                ..Person::default()
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

    #[test]
    fn texts_come_in_the_language_with_a_poster_only_in_it() {
        let f = fake(&[("/movie/603?language=es-ES", "movie_603_es-ES.json")]);
        let t = Tmdb::new("k".into(), &f)
            .texts(ItemKind::Movie, 603, "es-ES")
            .unwrap();
        assert_eq!(t.title, "Matrix");
        assert_eq!(t.genres, ["Acción", "Ciencia ficción"]);
        assert_eq!(t.poster.as_deref(), Some("/esMatrix.jpg"));
        assert_eq!(
            last_url(&f),
            "https://api.themoviedb.org/3/movie/603?language=es-ES&append_to_response=images\
             &include_image_language=es&api_key=k"
        );
    }

    #[test]
    fn a_poster_from_another_language_is_not_this_ones() {
        let f = fake(&[("/tv/95396?language=es-MX", "tv_95396_es-MX.json")]);
        let t = Tmdb::new("k".into(), &f)
            .texts(ItemKind::Show, 95396, "es-MX")
            .unwrap();
        assert_eq!((t.title.as_str(), t.overview.as_str()), ("Separación", ""));
        assert_eq!(t.poster, None);
    }

    #[test]
    fn season_texts_list_each_episode() {
        let f = fake(&[(
            "/tv/95396/season/2?language=es-MX",
            "tv_95396_season_2_es-MX.json",
        )]);
        let s = Tmdb::new("k".into(), &f)
            .season_texts(95396, 2, "es-MX")
            .unwrap();
        assert_eq!(s.poster.as_deref(), Some("/esSeason2.jpg"));
        assert_eq!(
            s.episodes,
            [
                (
                    1,
                    "Hola, señora Cobel".into(),
                    "Mark regresa a Lumon.".into()
                ),
                (2, String::new(), String::new())
            ]
        );
    }

    #[test]
    fn details_ask_for_english() {
        let f = fake(&[("/movie/603", "movie_603.json")]);
        Tmdb::new("k".into(), &f).movie(603).unwrap();
        assert!(last_url(&f).contains("language=en-US"), "{}", last_url(&f));
    }
}
