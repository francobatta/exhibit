//! What the window sees when a command fails: the core's kind (the same names the CLI
//! prints), the code the window words in the owner's language, its parameters, and the
//! English message the CLI would print.

use exhibit_core::Error;
use exhibit_core::error::{Detail, Fault};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ApiError {
    /// `Error::kind`: `not_found`, `invalid`, `source_offline`, `tool`, `provider`, `io`, `db`.
    pub kind: &'static str,
    /// What happened, as a stable code (`source.offline`); `None` when only the kind is known,
    /// and the window words the kind.
    pub code: Option<&'static str>,
    /// Values the wording of `code` names (`path`, `tool`).
    pub params: BTreeMap<&'static str, String>,
    /// The English the CLI would print, for logs. The window never shows it.
    pub message: String,
}

impl ApiError {
    /// An `invalid` error the window words from `code`.
    pub fn invalid(code: &'static str, message: &str) -> ApiError {
        Error::Invalid(Detail::coded(code, message)).into()
    }
}

impl From<Fault> for ApiError {
    fn from(f: Fault) -> ApiError {
        ApiError {
            kind: f.kind,
            code: f.code,
            params: f.params,
            message: f.message,
        }
    }
}

impl From<Error> for ApiError {
    fn from(e: Error) -> ApiError {
        Fault::from(e).into()
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> ApiError {
        Error::Io(e).into()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

/// The text of a caught panic, for an error the window can show.
pub fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(s) => *s,
        Err(p) => p
            .downcast_ref::<&str>()
            .map_or_else(|| "unknown panic".to_string(), |s| s.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_carry_the_core_kind_and_message() {
        for e in [
            Error::Db(rusqlite::Error::InvalidQuery),
            Error::SourceOffline("E:\\".into()),
            Error::NotFound("item 3".into()),
        ] {
            let (kind, text) = (e.kind(), e.to_string());
            let api = ApiError::from(e);
            assert_eq!((api.kind, api.message), (kind, text));
        }
        assert_eq!(
            ApiError::from(Error::SourceOffline("E:\\".into())).kind,
            "source_offline"
        );
        let json = serde_json::to_value(ApiError::invalid(
            "play.busy",
            "Something is already playing",
        ))
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "kind": "invalid",
                "code": "play.busy",
                "params": {},
                "message": "invalid: Something is already playing"
            })
        );
    }

    #[test]
    fn a_coded_error_carries_its_code_and_parameters() {
        let e = Error::SourceOffline(Detail::coded("source.offline", "E:\\").with("path", "E:\\"));
        let api = ApiError::from(e);
        assert_eq!(api.code, Some("source.offline"));
        assert_eq!(api.params.get("path").map(String::as_str), Some("E:\\"));
        assert_eq!(
            ApiError::from(Error::Db(rusqlite::Error::InvalidQuery)).code,
            None
        );
    }

    /// The codes written in the core and the app, as `Detail::coded("…"`, `Fault::coded(…, "…"`
    /// and `ApiError::invalid("…"`, outside tests.
    fn codes_in(dir: &std::path::Path) -> Vec<String> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let code = text.split("#[cfg(test)]").next().unwrap_or_default();
            for marker in ["Detail::coded(", "ApiError::invalid(", "Fault::coded("] {
                for piece in code.split(marker).skip(1) {
                    let args = piece.split(')').next().unwrap_or_default();
                    // The code is the quoted argument with a dot in it.
                    if let Some(c) = args
                        .split('"')
                        .skip(1)
                        .step_by(2)
                        .find(|s| s.contains('.') && !s.contains(' '))
                    {
                        out.push(c.to_string());
                    }
                }
            }
        }
        out
    }

    /// Every code the window can be sent has English words, so no error ever shows bare.
    #[test]
    fn every_error_code_has_words_in_the_window() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let catalogue: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("ui/src/i18n/locales/en-US.json")).unwrap(),
        )
        .unwrap();
        let mut codes = codes_in(&root.join("crates/exhibit-core/src"));
        codes.extend(codes_in(&root.join("crates/exhibit-app/src")));
        assert!(codes.len() > 30, "{codes:?}");
        let missing: Vec<&String> = codes
            .iter()
            .filter(|c| {
                let pointer = format!("/errors/{}", c.replace('.', "/"));
                !catalogue.pointer(&pointer).is_some_and(|v| v.is_string())
            })
            .collect();
        assert!(missing.is_empty(), "no words for {missing:?}");
    }
}
