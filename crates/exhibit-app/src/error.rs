//! What the window sees when a command fails: the core's kind (the same names the CLI
//! prints) and its message.

use exhibit_core::Error;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ApiError {
    /// `Error::kind`: `not_found`, `invalid`, `source_offline`, `tool`, `provider`, `io`, `db`.
    pub kind: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn invalid(message: &str) -> ApiError {
        Error::Invalid(message.to_string()).into()
    }
}

impl From<Error> for ApiError {
    fn from(e: Error) -> ApiError {
        ApiError {
            kind: e.kind(),
            message: e.to_string(),
        }
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
        let json = serde_json::to_value(ApiError::invalid("Something is already playing")).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"kind": "invalid", "message": "invalid: Something is already playing"})
        );
    }
}
