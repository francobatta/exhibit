use std::fmt;

#[derive(Debug)]
pub enum Error {
    Db(rusqlite::Error),
    Io(std::io::Error),
    NotFound(String),
    SourceOffline(String),
    Tool(String),
    Provider(String),
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;

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
