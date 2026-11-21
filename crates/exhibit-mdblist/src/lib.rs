//! The MDBList adapter: the public's scores of a film or show (IMDb, Rotten Tomatoes'
//! Tomatometer and Popcornmeter, Metacritic, Letterboxd's average) by TMDB id, read with the
//! owner's MDBList sign-in. Only this crate names these sites.
//!
//! Confirmed against the live API on 2026-11-21: `POST /tmdb/{movie|show}` with `{"ids": [...]}`
//! answers an array of titles and costs one request for a hundred ids; the free tier allows
//! 1,000 requests a day, reported in `X-RateLimit-*` headers; a spent allowance is 429 with
//! `Retry-After`. Sign-in is OAuth's device flow (`/oauth/device-authorization/`, `/oauth/token/`).

use std::time::{SystemTime, UNIX_EPOCH};

use exhibit_core::error::{Detail, Error, Result};
use exhibit_core::model::ItemKind;
use exhibit_core::service::{
    Account, DeviceCode, Lookup, Need, OriginInfo, Provides, Quota, Score, ScoreStyle, Service,
    ServiceInfo, SignInPoll, TmdbFilm, Token,
};
use exhibit_core::tmdb::{Http, Request, Response};
use serde_json::{Value, json};

const API: &str = "https://api.mdblist.com";
/// Exhibit's device-code app, registered on mdblist.com/developer. A device app keeps no
/// secret, so its id is public.
const CLIENT_ID: &str = "porn7NObPJVcEGl1199xDbNkAK3HRNO7vyl5IriS";
/// The only scope MDBList grants. Exhibit only reads.
const SCOPE: &str = "write";

pub struct MdbList;

static ORIGINS: [OriginInfo; 5] = [
    OriginInfo {
        id: "imdb",
        name: "IMDb",
        mark_svg: include_str!("marks/imdb.svg"),
        style: ScoreStyle::Decimal,
    },
    OriginInfo {
        id: "tomatometer",
        name: "Tomatometer",
        mark_svg: include_str!("marks/tomatometer.svg"),
        style: ScoreStyle::Percent,
    },
    OriginInfo {
        id: "metacritic",
        name: "Metacritic",
        mark_svg: include_str!("marks/metacritic.svg"),
        style: ScoreStyle::Badge,
    },
    OriginInfo {
        id: "letterboxd",
        name: "Letterboxd",
        mark_svg: include_str!("marks/letterboxd.svg"),
        style: ScoreStyle::Decimal,
    },
    OriginInfo {
        id: "popcornmeter",
        name: "Popcornmeter",
        mark_svg: include_str!("marks/popcornmeter.svg"),
        style: ScoreStyle::Percent,
    },
];

static INFO: ServiceInfo = ServiceInfo {
    id: "mdblist",
    name: "MDBList",
    mark_svg: include_str!("mark.svg"),
    provides: &[Provides::Reception],
    needs: &[Need::SignIn],
    export: None,
    origins: &ORIGINS,
    bodies: &[],
    batch: 100,
};

impl Service for MdbList {
    fn info(&self) -> &ServiceInfo {
        &INFO
    }

    /// The lookups left today.
    fn check(&self, account: &Account, http: &dyn Http) -> Result<u32> {
        let user = user(account.key.ok_or_else(signed_out)?, http)?;
        Ok(user["rate_limit_remaining"].as_u64().unwrap_or(0) as u32)
    }

    fn reception(
        &self,
        account: &Account,
        films: &[TmdbFilm],
        http: &dyn Http,
    ) -> Result<Lookup<Score>> {
        let token = account.key.ok_or_else(signed_out)?;
        let mut out = Lookup {
            films: Vec::new(),
            quota: None,
        };
        for (kind, path) in [(ItemKind::Movie, "movie"), (ItemKind::Show, "show")] {
            let ids: Vec<i64> = films
                .iter()
                .filter(|f| f.kind == kind)
                .map(|f| f.tmdb_id)
                .collect();
            if ids.is_empty() {
                continue;
            }
            let url = format!("{API}/tmdb/{path}");
            let req = Request::json(&url, &json!({ "ids": ids })).bearer(Some(token));
            let resp = answered(http.send(&req)?)?;
            out.quota = quota(&resp).or(out.quota);
            let titles: Vec<Value> = serde_json::from_slice(&resp.body)
                .map_err(|e| unreachable(format!("the answer is not a list of titles: {e}")))?;
            for t in &titles {
                let Some(tmdb_id) = t["ids"]["tmdb"].as_i64() else {
                    continue;
                };
                let scores = scores(t, kind);
                if !scores.is_empty() {
                    out.films.push((TmdbFilm { kind, tmdb_id }, scores));
                }
            }
        }
        Ok(out)
    }

    fn sign_in(&self, http: &dyn Http) -> Result<DeviceCode> {
        let url = format!("{API}/oauth/device-authorization/");
        let req = Request::form(&url, &[("client_id", CLIENT_ID), ("scope", SCOPE)]);
        let resp = http.send(&req)?;
        if resp.status != 200 {
            return Err(unreachable(format!("sign-in answered {}", resp.status)));
        }
        let v = json_of(&resp)?;
        let text = |k: &str| v[k].as_str().map(str::to_string);
        Ok(DeviceCode {
            device_code: text("device_code").ok_or_else(|| unreachable("no device code".into()))?,
            user_code: text("user_code").ok_or_else(|| unreachable("no user code".into()))?,
            verify_url: text("verification_uri")
                .unwrap_or_else(|| "https://mdblist.com/oauth/device/".into()),
            verify_url_full: text("verification_uri_complete"),
            interval: v["interval"].as_u64().unwrap_or(5).max(1),
            expires_in: v["expires_in"].as_u64().unwrap_or(300),
        })
    }

    fn sign_in_poll(&self, device_code: &str, http: &dyn Http) -> Result<SignInPoll> {
        let url = format!("{API}/oauth/token/");
        let req = Request::form(
            &url,
            &[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", device_code),
                ("client_id", CLIENT_ID),
            ],
        );
        let resp = http.send(&req)?;
        if resp.status == 200 {
            let mut token = token_of(&resp)?;
            token.name = user(&token.access, http)?["username"]
                .as_str()
                .map(str::to_string);
            return Ok(SignInPoll::Done(token));
        }
        let why = json_of(&resp)
            .ok()
            .and_then(|v| v["error"].as_str().map(str::to_string))
            .unwrap_or_default();
        match why.as_str() {
            "authorization_pending" => Ok(SignInPoll::Pending),
            "slow_down" => Ok(SignInPoll::SlowDown),
            "access_denied" => Err(Error::Provider(Detail::coded(
                "service.sign_in_denied",
                "the sign-in was declined",
            ))),
            "expired_token" => Err(Error::Provider(Detail::coded(
                "service.sign_in_expired",
                "the sign-in code expired",
            ))),
            _ => Err(unreachable(format!(
                "the sign-in answered {} {why}",
                resp.status
            ))),
        }
    }

    fn refresh_token(&self, token: &Token, http: &dyn Http) -> Result<Token> {
        let refresh = token.refresh.as_deref().ok_or_else(signed_out)?;
        let url = format!("{API}/oauth/token/");
        let req = Request::form(
            &url,
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh),
                ("client_id", CLIENT_ID),
            ],
        );
        let resp = http.send(&req)?;
        match resp.status {
            200 => token_of(&resp),
            400 | 401 | 403 => Err(signed_out()),
            s => Err(unreachable(format!("renewing the sign-in answered {s}"))),
        }
    }
}

/// The scores a title carries, for the origins Exhibit shows; the rest are left out.
fn scores(t: &Value, kind: ItemKind) -> Vec<Score> {
    let Some(ratings) = t["ratings"].as_array() else {
        return Vec::new();
    };
    let section = if kind == ItemKind::Show {
        "tv"
    } else {
        "movie"
    };
    ratings
        .iter()
        .filter_map(|r| {
            let raw = r["value"].as_f64()? as f32;
            let path = r["url"].as_str().filter(|p| !p.is_empty());
            let at = |site: &str| path.map(|p| format!("{site}{}", slash(p)));
            let (origin, scale, url) = match r["source"].as_str()? {
                "imdb" => (
                    "imdb",
                    10.0,
                    t["ids"]["imdb"]
                        .as_str()
                        .map(|id| format!("https://www.imdb.com/title/{id}/")),
                ),
                "tomatoes" => ("tomatometer", 100.0, at("https://www.rottentomatoes.com")),
                "popcorn" => ("popcornmeter", 100.0, at("https://www.rottentomatoes.com")),
                "metacritic" => (
                    "metacritic",
                    100.0,
                    at(&format!("https://www.metacritic.com/{section}")),
                ),
                "letterboxd" => ("letterboxd", 5.0, at("https://letterboxd.com")),
                _ => return None,
            };
            // `score` is the value over 100 in every answer; `value` is not: the batch gives
            // Letterboxd's average out of 10, the single lookup out of 5.
            let value = r["score"]
                .as_f64()
                .map_or(raw, |score| (score as f32) * scale / 100.0);
            Some(Score {
                origin: origin.into(),
                value,
                scale,
                votes: r["votes"].as_i64().filter(|v| *v > 0),
                url,
                certified: origin == "tomatometer" && r["fresh"].as_i64() == Some(1),
            })
        })
        .collect()
}

fn slash(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

/// A successful answer, or the error its status means.
fn answered(resp: Response) -> Result<Response> {
    match resp.status {
        200 => Ok(resp),
        401 | 403 => Err(signed_out()),
        429 => {
            let wait = resp
                .header("retry-after")
                .and_then(|v| v.trim().parse::<i64>().ok());
            let daily = json_of(&resp)
                .ok()
                .and_then(|v| v["error"].as_str().map(|e| e.starts_with("Daily")))
                .unwrap_or(false);
            let until = match (daily, quota(&resp)) {
                (true, Some(q)) => q.resets_at,
                _ => now() + wait.unwrap_or(300),
            };
            Err(Error::Provider(
                Detail::coded("service.limit_reached", "the lookups for today are spent")
                    .with("until", until),
            ))
        }
        s => Err(unreachable(format!("the lookup answered {s}"))),
    }
}

fn quota(resp: &Response) -> Option<Quota> {
    let num = |h: &str| resp.header(h).and_then(|v| v.trim().parse::<i64>().ok());
    Some(Quota {
        remaining: num("x-ratelimit-remaining")?.max(0) as u32,
        resets_at: num("x-ratelimit-reset")?,
    })
}

fn user(token: &str, http: &dyn Http) -> Result<Value> {
    let url = format!("{API}/user");
    let resp = answered(http.send(&Request::get(&url).bearer(Some(token)))?)?;
    json_of(&resp)
}

fn token_of(resp: &Response) -> Result<Token> {
    let v = json_of(resp)?;
    Ok(Token {
        access: v["access_token"]
            .as_str()
            .ok_or_else(|| unreachable("no access token".into()))?
            .to_string(),
        refresh: v["refresh_token"].as_str().map(str::to_string),
        expires_at: now() + v["expires_in"].as_i64().unwrap_or(30 * 86_400),
        name: None,
    })
}

fn json_of(resp: &Response) -> Result<Value> {
    serde_json::from_slice(&resp.body).map_err(|e| unreachable(format!("unreadable answer: {e}")))
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn unreachable(why: String) -> Error {
    Error::Provider(Detail::coded("service.unreachable", why))
}

fn signed_out() -> Error {
    Error::Provider(Detail::coded(
        "service.signed_out",
        "MDBList no longer accepts the sign-in; sign in again",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use exhibit_core::tmdb::Method;
    use std::sync::Mutex;

    /// A request as sent: its method, URL, bearer token and body.
    type Sent = (Method, String, Option<String>, String);

    /// Answers each request with the first canned response whose URL part it contains, and
    /// keeps what was sent.
    struct Fake {
        routes: Vec<(&'static str, Response)>,
        sent: Mutex<Vec<Sent>>,
    }

    impl Http for Fake {
        fn get(&self, _: &str, _: Option<&str>) -> Result<(u16, Vec<u8>)> {
            unreachable!("the adapter sends")
        }

        fn send(&self, req: &Request) -> Result<Response> {
            let body = req
                .body
                .as_ref()
                .map(|(_, b)| String::from_utf8_lossy(b).to_string())
                .unwrap_or_default();
            self.sent.lock().unwrap().push((
                req.method,
                req.url.to_string(),
                req.bearer.map(str::to_string),
                body,
            ));
            Ok(self
                .routes
                .iter()
                .find(|(k, _)| req.url.contains(k))
                .map(|(_, r)| r.clone())
                .unwrap_or(Response {
                    status: 404,
                    ..Response::default()
                }))
        }
    }

    fn fake(routes: Vec<(&'static str, Response)>) -> Fake {
        Fake {
            routes,
            sent: Mutex::default(),
        }
    }

    fn ok(body: &str, headers: &[(&str, &str)]) -> Response {
        Response {
            status: 200,
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            body: body.as_bytes().to_vec(),
        }
    }

    fn status(status: u16, body: &str) -> Response {
        Response {
            status,
            body: body.as_bytes().to_vec(),
            ..Response::default()
        }
    }

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    const ACCOUNT: Account = Account {
        username: None,
        key: Some("tok"),
    };

    fn film(kind: ItemKind, tmdb_id: i64) -> TmdbFilm {
        TmdbFilm { kind, tmdb_id }
    }

    #[test]
    fn one_lookup_per_kind_maps_each_origin_with_its_scale_and_link() {
        let limits = [
            ("x-ratelimit-remaining", "996"),
            ("x-ratelimit-reset", "1795305600"),
        ];
        let f = fake(vec![
            ("/tmdb/movie", ok(&fixture("movies.json"), &limits)),
            ("/tmdb/show", ok(&fixture("shows.json"), &limits)),
        ]);
        let films = [
            film(ItemKind::Movie, 103),
            film(ItemKind::Show, 95396),
            film(ItemKind::Movie, 949),
            film(ItemKind::Movie, 1),
        ];
        let got = MdbList.reception(&ACCOUNT, &films, &f).unwrap();
        let sent = f.sent.lock().unwrap();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].0, Method::Post);
        assert_eq!(sent[0].2.as_deref(), Some("tok"));
        assert_eq!(sent[0].3, r#"{"ids":[103,949,1]}"#);
        assert_eq!(
            got.quota,
            Some(Quota {
                remaining: 996,
                resets_at: 1_795_305_600
            })
        );
        // Film 1 is unknown to it; Heat has only one score it keeps.
        let ids: Vec<i64> = got.films.iter().map(|(f, _)| f.tmdb_id).collect();
        assert_eq!(ids, [103, 949, 95396]);
        let taxi = &got.films[0].1;
        let by = |o: &str| taxi.iter().find(|s| s.origin == o).unwrap();
        assert_eq!(taxi.len(), 5, "trakt, tmdb and the rest are left out");
        assert!((by("imdb").value - 8.2).abs() < 1e-4 && by("imdb").scale == 10.0);
        assert_eq!(by("imdb").votes, Some(1_049_339));
        assert_eq!(
            by("imdb").url.as_deref(),
            Some("https://www.imdb.com/title/tt0075314/")
        );
        assert!(by("tomatometer").certified);
        assert_eq!(
            by("tomatometer").url.as_deref(),
            Some("https://www.rottentomatoes.com/m/taxi_driver")
        );
        assert!(!by("popcornmeter").certified);
        assert_eq!(
            by("metacritic").url.as_deref(),
            Some("https://www.metacritic.com/movie/taxi-driver-1976")
        );
        // The batch gives Letterboxd's average out of 10; it reads out of 5, as Letterboxd writes it.
        assert!((by("letterboxd").value - 4.1).abs() < 1e-4 && by("letterboxd").scale == 5.0);
        // A show's Metacritic page is under tv; a missing score is left out.
        let show = &got.films[2].1;
        let mc = show.iter().find(|s| s.origin == "metacritic").unwrap();
        assert_eq!(
            mc.url.as_deref(),
            Some("https://www.metacritic.com/tv/severance")
        );
        assert!(!show.iter().any(|s| s.origin == "letterboxd"));
    }

    #[test]
    fn a_spent_allowance_and_a_refused_sign_in_are_coded() {
        let mut limit = status(429, r#"{"error": "Daily API limit exceeded!"}"#);
        limit.headers = vec![
            ("retry-after".into(), "3600".into()),
            ("x-ratelimit-remaining".into(), "0".into()),
            ("x-ratelimit-reset".into(), "1795305600".into()),
        ];
        let f = fake(vec![("/tmdb/", limit)]);
        let e = MdbList
            .reception(&ACCOUNT, &[film(ItemKind::Movie, 1)], &f)
            .unwrap_err();
        let d = e.detail().unwrap();
        assert_eq!(d.code, Some("service.limit_reached"));
        assert_eq!(d.params, [("until", "1795305600".to_string())]);

        let f = fake(vec![(
            "/tmdb/",
            status(401, r#"{"error": "Invalid API key!"}"#),
        )]);
        let e = MdbList
            .reception(&ACCOUNT, &[film(ItemKind::Movie, 1)], &f)
            .unwrap_err();
        assert_eq!(e.detail().unwrap().code, Some("service.signed_out"));
        let none = Account {
            username: None,
            key: None,
        };
        assert!(MdbList.reception(&none, &[], &f).is_err());
    }

    #[test]
    fn device_sign_in_starts_polls_and_names_the_account() {
        let f = fake(vec![(
            "/oauth/device-authorization/",
            ok(
                r#"{"verification_uri": "https://mdblist.com/oauth/device/", "expires_in": 1800, "user_code": "7EFVV31O", "device_code": "dev", "interval": 5, "verification_uri_complete": "https://mdblist.com/oauth/device/?user_code=7EFVV31O"}"#,
                &[],
            ),
        )]);
        let code = MdbList.sign_in(&f).unwrap();
        assert_eq!(code.user_code, "7EFVV31O");
        assert_eq!(code.device_code, "dev");
        assert_eq!((code.interval, code.expires_in), (5, 1800));
        assert!(
            code.verify_url_full
                .unwrap()
                .ends_with("user_code=7EFVV31O")
        );
        let body = &f.sent.lock().unwrap()[0].3;
        assert!(body.contains("client_id=") && body.contains("scope=write"));

        for (answer, want) in [
            (r#"{"error": "authorization_pending"}"#, SignInPoll::Pending),
            (r#"{"error": "slow_down"}"#, SignInPoll::SlowDown),
        ] {
            let f = fake(vec![("/oauth/token/", status(400, answer))]);
            assert_eq!(MdbList.sign_in_poll("dev", &f).unwrap(), want);
        }
        for (answer, code) in [
            (r#"{"error": "access_denied"}"#, "service.sign_in_denied"),
            (r#"{"error": "expired_token"}"#, "service.sign_in_expired"),
        ] {
            let f = fake(vec![("/oauth/token/", status(400, answer))]);
            let e = MdbList.sign_in_poll("dev", &f).unwrap_err();
            assert_eq!(e.detail().unwrap().code, Some(code));
        }

        let f = fake(vec![
            (
                "/oauth/token/",
                ok(
                    r#"{"access_token": "A", "token_type": "Bearer", "expires_in": 2592000, "refresh_token": "R", "scope": "write"}"#,
                    &[],
                ),
            ),
            (
                "/user",
                ok(
                    r#"{"username": "someone", "rate_limit_remaining": 990}"#,
                    &[],
                ),
            ),
        ]);
        let SignInPoll::Done(t) = MdbList.sign_in_poll("dev", &f).unwrap() else {
            panic!("approved")
        };
        assert_eq!((t.access.as_str(), t.refresh.as_deref()), ("A", Some("R")));
        assert_eq!(t.name.as_deref(), Some("someone"));
        assert!((t.expires_at - now() - 2_592_000).abs() < 5);
        let sent = f.sent.lock().unwrap();
        assert!(
            sent[0]
                .3
                .contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code")
        );
        assert_eq!(sent[1].2.as_deref(), Some("A"));
    }

    #[test]
    fn check_reports_the_lookups_left_and_refresh_renews_or_signs_out() {
        let f = fake(vec![(
            "/user",
            ok(
                r#"{"username": "someone", "rate_limit_remaining": 990}"#,
                &[],
            ),
        )]);
        assert_eq!(MdbList.check(&ACCOUNT, &f).unwrap(), 990);

        let old = Token {
            access: "A".into(),
            refresh: Some("R".into()),
            expires_at: 0,
            name: Some("someone".into()),
        };
        let f = fake(vec![(
            "/oauth/token/",
            ok(
                r#"{"access_token": "B", "refresh_token": "S", "expires_in": 10}"#,
                &[],
            ),
        )]);
        let t = MdbList.refresh_token(&old, &f).unwrap();
        assert_eq!((t.access.as_str(), t.refresh.as_deref()), ("B", Some("S")));
        assert!(f.sent.lock().unwrap()[0].3.contains("refresh_token=R"));
        let f = fake(vec![(
            "/oauth/token/",
            status(400, r#"{"error": "invalid_grant"}"#),
        )]);
        let e = MdbList.refresh_token(&old, &f).unwrap_err();
        assert_eq!(e.detail().unwrap().code, Some("service.signed_out"));
    }

    #[test]
    fn every_origin_has_a_mark() {
        for o in INFO.origins {
            assert!(o.mark_svg.contains("viewBox"), "{}", o.id);
        }
    }
}
