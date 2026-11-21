//! Signing in to a service with a short code the owner approves on another device, and
//! keeping the token it gives current. The token lives in the credential store, under the
//! service's id, as JSON.

use rusqlite::{Connection, params};
use serde::Serialize;

use crate::error::{Detail, Error, Result};
use crate::secrets::Secrets;
use crate::service::{Service, SignInPoll, Token};
use crate::sync::{self, code};
use crate::tmdb::Http;

/// A token this close to expiry is renewed before it is used.
const RENEW_BEFORE: i64 = 2 * 86_400;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignInState {
    Pending,
    /// The service asked Exhibit to ask less often.
    SlowDown,
    Done,
}

/// Asks whether the owner has approved `device_code`. On approval the token is stored and the
/// account named; another account than the one before starts afresh.
pub fn poll(
    c: &Connection,
    secrets: &dyn Secrets,
    svc: &dyn Service,
    device_code: &str,
    http: &dyn Http,
) -> Result<SignInState> {
    match svc.sign_in_poll(device_code, http)? {
        SignInPoll::Pending => Ok(SignInState::Pending),
        SignInPoll::SlowDown => Ok(SignInState::SlowDown),
        SignInPoll::Done(token) => {
            let id = svc.info().id;
            let name = token.name.clone().unwrap_or_else(|| "?".into());
            sync::set_service_username(c, id, Some(&name))?;
            store(secrets, id, &token)?;
            c.prepare_cached(
                "UPDATE service_state SET error = NULL, paused_until = NULL WHERE service = ?1",
            )?
            .execute([id])?;
            Ok(SignInState::Done)
        }
    }
}

/// The access token for `svc`, renewed first when it is about to expire. `None` when the owner
/// has not signed in. A renewal the service refuses is recorded as the service's error.
pub fn access(
    c: &Connection,
    secrets: &dyn Secrets,
    svc: &dyn Service,
    http: &dyn Http,
    now: i64,
) -> Result<Option<String>> {
    let id = svc.info().id;
    let Some(raw) = secrets.get(id)? else {
        return Ok(None);
    };
    let token: Token = serde_json::from_str(&raw).map_err(|_| signed_out())?;
    if token.expires_at - now > RENEW_BEFORE {
        return Ok(Some(token.access));
    }
    match svc.refresh_token(&token, http) {
        Ok(mut fresh) => {
            fresh.name = fresh.name.or(token.name);
            store(secrets, id, &fresh)?;
            Ok(Some(fresh.access))
        }
        // Still good for a while: use it, and renew next time.
        Err(_) if token.expires_at > now => Ok(Some(token.access)),
        Err(e) => {
            c.prepare_cached("UPDATE service_state SET error = ?2 WHERE service = ?1")?
                .execute(params![id, code(&e)])?;
            Err(e)
        }
    }
}

fn store(secrets: &dyn Secrets, id: &str, token: &Token) -> Result<()> {
    let json = serde_json::to_string(token).map_err(|e| Error::Invalid(e.to_string().into()))?;
    secrets.set(id, Some(&json))
}

fn signed_out() -> Error {
    Error::Invalid(Detail::coded(
        "service.signed_out",
        "the stored sign-in cannot be read; sign in again",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemorySecrets;
    use crate::service::{Account, Need, Provides, ServiceInfo};
    use std::sync::Mutex;

    static INFO: ServiceInfo = ServiceInfo {
        id: "scores",
        name: "Scores",
        mark_svg: "<svg/>",
        provides: &[Provides::Reception],
        needs: &[Need::SignIn],
        export: None,
        origins: &[],
        bodies: &[],
        batch: 10,
    };

    struct NoHttp;
    impl Http for NoHttp {
        fn get(&self, _: &str, _: Option<&str>) -> Result<(u16, Vec<u8>)> {
            unreachable!()
        }
    }

    #[derive(Default)]
    struct Fake {
        polls: Mutex<u32>,
        refused: bool,
    }

    fn token(access: &str, expires_at: i64) -> Token {
        Token {
            access: access.into(),
            refresh: Some("r".into()),
            expires_at,
            name: Some("Someone".into()),
        }
    }

    impl Service for Fake {
        fn info(&self) -> &ServiceInfo {
            &INFO
        }
        fn check(&self, _: &Account, _: &dyn Http) -> Result<u32> {
            Ok(0)
        }
        fn sign_in_poll(&self, code: &str, _: &dyn Http) -> Result<SignInPoll> {
            assert_eq!(code, "dev");
            let mut n = self.polls.lock().unwrap();
            *n += 1;
            Ok(if *n < 2 {
                SignInPoll::Pending
            } else {
                SignInPoll::Done(token("a1", 1000))
            })
        }
        fn refresh_token(&self, t: &Token, _: &dyn Http) -> Result<Token> {
            assert_eq!(t.refresh.as_deref(), Some("r"));
            if self.refused {
                return Err(Error::Provider(Detail::coded("service.signed_out", "no")));
            }
            Ok(Token {
                name: None,
                ..token("a2", 9_000_000)
            })
        }
    }

    fn db() -> (tempfile::TempDir, Connection) {
        let d = tempfile::tempdir().unwrap();
        let c = crate::db::open(d.path()).unwrap();
        (d, c)
    }

    #[test]
    fn approval_stores_the_token_and_names_the_account() {
        let (_d, c) = db();
        let s = MemorySecrets::default();
        let svc = Fake::default();
        assert_eq!(
            poll(&c, &s, &svc, "dev", &NoHttp).unwrap(),
            SignInState::Pending
        );
        assert!(s.get("scores").unwrap().is_none());
        assert_eq!(
            poll(&c, &s, &svc, "dev", &NoHttp).unwrap(),
            SignInState::Done
        );
        let stored: Token = serde_json::from_str(&s.get("scores").unwrap().unwrap()).unwrap();
        assert_eq!(stored.access, "a1");
        assert_eq!(
            sync::service_username(&c, "scores").unwrap().as_deref(),
            Some("someone")
        );
    }

    #[test]
    fn a_token_near_expiry_is_renewed_and_keeps_its_name() {
        let (_d, c) = db();
        let s = MemorySecrets::default();
        let svc = Fake::default();
        assert_eq!(access(&c, &s, &svc, &NoHttp, 0).unwrap(), None);
        store(&s, "scores", &token("a1", 10 * 86_400)).unwrap();
        // Far from expiry: used as it is.
        assert_eq!(
            access(&c, &s, &svc, &NoHttp, 0).unwrap().as_deref(),
            Some("a1")
        );
        // A day before: renewed.
        let got = access(&c, &s, &svc, &NoHttp, 9 * 86_400).unwrap();
        assert_eq!(got.as_deref(), Some("a2"));
        let stored: Token = serde_json::from_str(&s.get("scores").unwrap().unwrap()).unwrap();
        assert_eq!(stored.name.as_deref(), Some("Someone"));
        // Refused after it expired: the service's error says to sign in again.
        let svc = Fake {
            refused: true,
            ..Fake::default()
        };
        sync::set_service_username(&c, "scores", Some("someone")).unwrap();
        assert!(access(&c, &s, &svc, &NoHttp, 10_000_000).is_err());
        let st = &sync::service_states(&c).unwrap()[0];
        assert_eq!(st.error.as_deref(), Some("service.signed_out"));
    }
}
