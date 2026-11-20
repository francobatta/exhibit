use crate::error::{Error, Result};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Migrations in order; `PRAGMA user_version` holds how many have been applied.
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/001_init.sql"),
    include_str!("migrations/002_drop_ffprobe.sql"),
    include_str!("migrations/003_reprobe_tracks.sql"),
];

/// Data directory: `EXHIBIT_DATA_DIR` if set, else `%APPDATA%\exhibit`.
/// The uninstaller removes `%APPDATA%\exhibit` (`crates/exhibit-app/installer-hooks.nsh`).
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("EXHIBIT_DATA_DIR") {
        return PathBuf::from(d);
    }
    if let Some(a) = std::env::var_os("APPDATA") {
        return PathBuf::from(a).join("exhibit");
    }
    // Non-Windows fallback.
    if let Some(h) = std::env::var_os("HOME") {
        return PathBuf::from(h)
            .join(".local")
            .join("share")
            .join("exhibit");
    }
    PathBuf::from("exhibit-data")
}

/// Open (creating if needed) `exhibit.db` in `data_dir`, configure it and migrate.
pub fn open(data_dir: &Path) -> Result<Connection> {
    std::fs::create_dir_all(data_dir)?;
    let c = Connection::open(data_dir.join("exhibit.db"))?;
    c.busy_timeout(std::time::Duration::from_millis(5000))?;
    c.pragma_update(None, "foreign_keys", "ON")?;
    use_wal(&c)?;
    migrate(&c)?;
    Ok(c)
}

/// Switches to WAL, which several processes sharing the database rely on.
fn use_wal(c: &Connection) -> Result<()> {
    let mode: String = c.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(Error::Invalid(format!(
            "the database folder does not support SQLite WAL mode (got journal mode {mode}); \
             set EXHIBIT_DATA_DIR to a folder on a local drive"
        )));
    }
    Ok(())
}

fn migrate(c: &Connection) -> Result<()> {
    let version = |c: &Connection| -> rusqlite::Result<usize> {
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .map(|v| v as usize)
    };
    if version(c)? >= MIGRATIONS.len() {
        return Ok(());
    }
    // Take the write lock first so concurrent processes migrate one at a time.
    let tx = write_tx(c)?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version(&tx)?) {
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
    }
    tx.commit()?;
    Ok(())
}

/// A write transaction that takes the write lock up front (`BEGIN IMMEDIATE`), so it waits
/// out another process's write under the busy timeout instead of failing part-way through.
pub(crate) fn write_tx(c: &Connection) -> Result<Transaction<'_>> {
    Ok(Transaction::new_unchecked(
        c,
        TransactionBehavior::Immediate,
    )?)
}

/// The current time in Unix seconds.
pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

pub fn get_setting(c: &Connection, key: &str) -> Result<Option<String>> {
    let v: Option<Option<String>> = c
        .query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(v.flatten())
}

/// Set a setting; `None` removes it.
pub fn set_setting(c: &Connection, key: &str, value: Option<&str>) -> Result<()> {
    match value {
        Some(v) => c.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [key, v],
        )?,
        None => c.execute("DELETE FROM settings WHERE key=?1", [key])?,
    };
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_schema_and_wal() {
        let d = tempfile::tempdir().unwrap();
        let c = open(d.path()).unwrap();
        let mode: String = c
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
        for t in [
            "settings",
            "sources",
            "items",
            "files",
            "play_state",
            "play_log",
            "items_fts",
        ] {
            let n: i64 = c
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE name=?1",
                    [t],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "{t}");
        }
    }

    #[test]
    fn no_wal_is_an_error() {
        // An in-memory database answers `memory`, as a folder without WAL support answers
        // `delete`.
        let c = Connection::open_in_memory().unwrap();
        let e = use_wal(&c).unwrap_err();
        assert!(
            matches!(&e, crate::error::Error::Invalid(m) if m.contains("WAL")),
            "{e}"
        );
    }

    #[test]
    fn open_twice_is_idempotent() {
        let d = tempfile::tempdir().unwrap();
        let v1: i64 = {
            let c = open(d.path()).unwrap();
            c.query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap()
        };
        assert!(v1 >= 1);
        let c = open(d.path()).unwrap();
        let v2: i64 = c
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v1, v2);
    }

    #[test]
    fn open_creates_missing_directory() {
        let d = tempfile::tempdir().unwrap();
        let nested = d.path().join("a").join("b");
        open(&nested).unwrap();
        assert!(nested.join("exhibit.db").exists());
    }

    #[test]
    fn settings_round_trip_and_clear() {
        let d = tempfile::tempdir().unwrap();
        let c = open(d.path()).unwrap();
        assert_eq!(get_setting(&c, "k").unwrap(), None);
        set_setting(&c, "k", Some("v")).unwrap();
        assert_eq!(get_setting(&c, "k").unwrap(), Some("v".to_string()));
        set_setting(&c, "k", Some("w")).unwrap();
        assert_eq!(get_setting(&c, "k").unwrap(), Some("w".to_string()));
        set_setting(&c, "k", None).unwrap();
        assert_eq!(get_setting(&c, "k").unwrap(), None);
    }

    #[test]
    fn sources_are_unique_per_volume_and_cascade_to_files() {
        let d = tempfile::tempdir().unwrap();
        let c = open(d.path()).unwrap();
        c.execute_batch(
            "INSERT INTO sources(id, path, volume_id) VALUES (1, 'Movies', 'V1');
             INSERT INTO sources(id, path, volume_id) VALUES (2, 'Movies', 'V2');
             INSERT INTO files(source_id, rel_path) VALUES (1, 'a.mkv');",
        )
        .unwrap();
        assert!(
            c.execute(
                "INSERT INTO sources(path, volume_id) VALUES ('Movies', 'V1')",
                []
            )
            .is_err()
        );
        c.execute("DELETE FROM sources WHERE id=1", []).unwrap();
        let n: i64 = c
            .query_row("SELECT count(*) FROM files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn write_tx_waits_for_another_writer() {
        let d = tempfile::tempdir().unwrap();
        let a = open(d.path()).unwrap();
        let tx = write_tx(&a).unwrap();
        tx.execute("INSERT INTO settings(key, value) VALUES ('a', '1')", [])
            .unwrap();
        let path = d.path().to_path_buf();
        let other = std::thread::spawn(move || -> Result<i64> {
            let b = open(&path)?;
            let tx = write_tx(&b)?;
            // Read, then write: a deferred transaction fails here once A commits.
            let n: i64 = tx.query_row("SELECT count(*) FROM settings", [], |r| r.get(0))?;
            tx.execute("INSERT INTO settings(key, value) VALUES ('b', '2')", [])?;
            tx.commit()?;
            Ok(n)
        });
        std::thread::sleep(std::time::Duration::from_millis(300));
        tx.commit().unwrap();
        assert_eq!(other.join().unwrap().unwrap(), 1);
    }

    fn fts_ids(c: &rusqlite::Connection, q: &str) -> Vec<i64> {
        let mut s = c
            .prepare("SELECT rowid FROM items_fts WHERE items_fts MATCH ?1")
            .unwrap();
        s.query_map([q], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    #[test]
    fn fts_follows_item_title() {
        let d = tempfile::tempdir().unwrap();
        let c = open(d.path()).unwrap();
        c.execute(
            "INSERT INTO items(kind, title, match_state, added_at) VALUES ('movie','Paprika','unmatched',0)",
            [],
        )
        .unwrap();
        let id = c.last_insert_rowid();
        assert_eq!(fts_ids(&c, "paprika"), vec![id]);
        c.execute("UPDATE items SET title='Perfect Blue' WHERE id=?1", [id])
            .unwrap();
        assert!(fts_ids(&c, "paprika").is_empty());
        assert_eq!(fts_ids(&c, "perfect"), vec![id]);
        c.execute("DELETE FROM items WHERE id=?1", [id]).unwrap();
        assert!(fts_ids(&c, "perfect").is_empty());
    }

    #[test]
    fn fts_indexes_people_genres_overview() {
        let d = tempfile::tempdir().unwrap();
        let c = open(d.path()).unwrap();
        c.execute(
            "INSERT INTO items(kind, title, overview, genres, people, match_state, added_at) VALUES \
             ('movie','X','a dream detective','[\"Animation\"]','[{\"name\":\"Satoshi Kon\",\"role\":\"director\"}]','matched',0)",
            [],
        )
        .unwrap();
        let id = c.last_insert_rowid();
        assert_eq!(fts_ids(&c, "kon"), vec![id]);
        assert_eq!(fts_ids(&c, "animation"), vec![id]);
        assert_eq!(fts_ids(&c, "detective"), vec![id]);
        assert!(fts_ids(&c, "director").is_empty());
    }
}
