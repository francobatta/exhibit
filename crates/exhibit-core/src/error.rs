use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;
use std::ops::Deref;

#[derive(Debug)]
pub enum Error {
    Db(rusqlite::Error),
    Io(std::io::Error),
    NotFound(Detail),
    SourceOffline(Detail),
    Tool(Detail),
    Provider(Detail),
    Invalid(Detail),
}

/// What went wrong, in English for the CLI and logs, and, when the window should word it in
/// the owner's language, a stable code (`source.not_a_folder`) with its parameters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Detail {
    pub text: String,
    pub code: Option<&'static str>,
    pub params: Vec<(&'static str, String)>,
}

impl Detail {
    /// A detail the window words from `code`; `text` is the English for the CLI and logs.
    pub fn coded(code: &'static str, text: impl Into<String>) -> Detail {
        Detail {
            text: text.into(),
            code: Some(code),
            params: Vec::new(),
        }
    }

    /// Adds a parameter the window's wording can use (`{{path}}`).
    pub fn with(mut self, name: &'static str, value: impl fmt::Display) -> Detail {
        self.params.push((name, value.to_string()));
        self
    }
}

impl From<String> for Detail {
    fn from(text: String) -> Detail {
        Detail {
            text,
            ..Detail::default()
        }
    }
}

impl From<&str> for Detail {
    fn from(text: &str) -> Detail {
        text.to_string().into()
    }
}

impl Deref for Detail {
    type Target = str;
    fn deref(&self) -> &str {
        &self.text
    }
}

impl PartialEq<str> for Detail {
    fn eq(&self, other: &str) -> bool {
        self.text == other
    }
}

impl PartialEq<&str> for Detail {
    fn eq(&self, other: &&str) -> bool {
        self.text == *other
    }
}

impl fmt::Display for Detail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// An error as it travels in a report: its kind, its code and parameters for the window,
/// and the English message the CLI prints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Fault {
    pub kind: &'static str,
    pub code: Option<&'static str>,
    pub params: BTreeMap<&'static str, String>,
    pub message: String,
}

impl Fault {
    /// A fault the window words from `code`, of kind `kind`.
    pub fn coded(kind: &'static str, code: &'static str, message: impl Into<String>) -> Fault {
        Fault {
            kind,
            code: Some(code),
            params: BTreeMap::new(),
            message: message.into(),
        }
    }

    /// Adds a parameter the window's wording can use.
    pub fn with(mut self, name: &'static str, value: impl fmt::Display) -> Fault {
        self.params.insert(name, value.to_string());
        self
    }
}

impl From<&Error> for Fault {
    fn from(e: &Error) -> Fault {
        let (code, params) = match e.detail() {
            Some(d) => (d.code, d.params.iter().cloned().collect()),
            None => (None, BTreeMap::new()),
        };
        Fault {
            kind: e.kind(),
            code,
            params,
            message: e.to_string(),
        }
    }
}

impl From<Error> for Fault {
    fn from(e: Error) -> Fault {
        Fault::from(&e)
    }
}

impl Error {
    /// The machine-readable kind every front end prints beside the message.
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Db(_) => "db",
            Error::Io(_) => "io",
            Error::NotFound(_) => "not_found",
            Error::SourceOffline(_) => "source_offline",
            Error::Tool(_) => "tool",
            Error::Provider(_) => "provider",
            Error::Invalid(_) => "invalid",
        }
    }

    /// The detail of the kinds that carry one; `None` for database and file errors.
    pub fn detail(&self) -> Option<&Detail> {
        match self {
            Error::Db(_) | Error::Io(_) => None,
            Error::NotFound(d)
            | Error::SourceOffline(d)
            | Error::Tool(d)
            | Error::Provider(d)
            | Error::Invalid(d) => Some(d),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Db(e) => write!(f, "database error: {e}"),
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::NotFound(m) => write!(f, "not found: {m}"),
            Error::SourceOffline(m) => write!(f, "source offline: {m}"),
            Error::Tool(m) => write!(f, "tool error: {m}"),
            Error::Provider(m) => write!(f, "provider error: {m}"),
            Error::Invalid(m) => write!(f, "invalid: {m}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Db(e) => Some(e),
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Db(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_are_the_printed_names() {
        let cases = [
            (Error::Db(rusqlite::Error::InvalidQuery), "db"),
            (Error::Io(std::io::Error::other("disk")), "io"),
            (Error::NotFound("x".into()), "not_found"),
            (Error::SourceOffline("x".into()), "source_offline"),
            (Error::Tool("x".into()), "tool"),
            (Error::Provider("x".into()), "provider"),
            (Error::Invalid("x".into()), "invalid"),
        ];
        for (e, kind) in cases {
            assert_eq!(e.kind(), kind);
        }
    }
}
