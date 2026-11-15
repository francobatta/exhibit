use std::collections::HashMap;
use std::sync::Mutex;

use crate::error::{Error, Result};

/// Named secrets ("tmdb", "agent"). `set` with `None` deletes the secret.
pub trait Secrets {
    fn get(&self, name: &str) -> Result<Option<String>>;
    fn set(&self, name: &str, value: Option<&str>) -> Result<()>;
}

/// Secrets in the OS credential store.
pub struct OsSecrets {
    service: String,
}

impl OsSecrets {
    /// Service name is `exhibit`, or `EXHIBIT_KEYRING_SERVICE` when set (read once here).
    pub fn new() -> Self {
        let service = std::env::var("EXHIBIT_KEYRING_SERVICE").unwrap_or_else(|_| "exhibit".into());
        OsSecrets { service }
    }

    fn entry(&self, name: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, name).map_err(keyring_err)
    }
}

impl Default for OsSecrets {
    fn default() -> Self {
        Self::new()
    }
}

fn keyring_err(e: keyring::Error) -> Error {
    Error::Invalid(format!("credential store: {e}"))
}

impl Secrets for OsSecrets {
    fn get(&self, name: &str) -> Result<Option<String>> {
        match self.entry(name)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keyring_err(e)),
        }
    }

    fn set(&self, name: &str, value: Option<&str>) -> Result<()> {
        let entry = self.entry(name)?;
        let result = match value {
            Some(v) => entry.set_password(v),
            None => entry.delete_credential(),
        };
        match result {
            Err(keyring::Error::NoEntry) | Ok(()) => Ok(()),
            Err(e) => Err(keyring_err(e)),
        }
    }
}

/// In-memory secrets for tests.
#[derive(Default)]
pub struct MemorySecrets(Mutex<HashMap<String, String>>);

impl Secrets for MemorySecrets {
    fn get(&self, name: &str) -> Result<Option<String>> {
        Ok(self.0.lock().unwrap().get(name).cloned())
    }

    fn set(&self, name: &str, value: Option<&str>) -> Result<()> {
        let mut map = self.0.lock().unwrap();
        match value {
            Some(v) => map.insert(name.to_string(), v.to_string()),
            None => map.remove(name),
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_secrets_set_get_clear() {
        let s = MemorySecrets::default();
        assert_eq!(s.get("tmdb").unwrap(), None);
        s.set("tmdb", Some("abc")).unwrap();
        assert_eq!(s.get("tmdb").unwrap().as_deref(), Some("abc"));
        s.set("tmdb", Some("def")).unwrap();
        assert_eq!(s.get("tmdb").unwrap().as_deref(), Some("def"));
        s.set("tmdb", None).unwrap();
        assert_eq!(s.get("tmdb").unwrap(), None);
        s.set("agent", None).unwrap();
    }
}
