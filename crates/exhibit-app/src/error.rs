//! What the window sees when a command fails: a kind it can branch on and the core's message.

use exhibit_core::Error;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ApiError {
    /// One of `not_found`, `invalid`, `offline`, `tool`, `provider`, `io`, `db`.
    pub kind: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn invalid(message: &str) -> ApiError {
        ApiError {
            kind: "invalid",
            message: message.to_string(),
        }
    }
}

impl From<Error> for ApiError {
    fn from(e: Error) -> ApiError {
        let kind = match &e {
            Error::Db(_) => "db",
            Error::Io(_) => "io",
            Error::NotFound(_) => "not_found",
            Error::SourceOffline(_) => "offline",
            Error::Tool(_) => "tool",
            Error::Provider(_) => "provider",
            Error::Invalid(_) => "invalid",
        };
        ApiError {
            kind,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_kinds_map() {
        let cases = [
            (Error::Db(rusqlite::Error::InvalidQuery), "db"),
            (Error::Io(std::io::Error::other("disk")), "io"),
            (Error::NotFound("item 3".into()), "not_found"),
            (Error::SourceOffline("E:\\".into()), "offline"),
            (Error::Tool("mpv".into()), "tool"),
            (Error::Provider("tmdb".into()), "provider"),
            (Error::Invalid("bad".into()), "invalid"),
        ];
        for (e, kind) in cases {
            let text = e.to_string();
            let api = ApiError::from(e);
            assert_eq!(api.kind, kind);
            assert_eq!(api.message, text);
        }
        let json = serde_json::to_value(ApiError::invalid("Something is already playing")).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"kind": "invalid", "message": "Something is already playing"})
        );
    }
}
