//! Sources and the scanner: walks a source folder, records video files and creates one
//! provisional library item per new file. Only reads inside media folders.

use crate::db;
use crate::error::{Error, Result};
use crate::model::{Id, Source};
use crate::parse::parse_path;
use crate::volume::Volumes;
use regex::Regex;
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "m4v", "avi", "mov", "wmv", "ts", "m2ts", "webm", "mpg", "mpeg", "flv",
];

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ScanReport {
    pub source_id: Id,
    pub online: bool,
    pub added: u32,
    pub changed: u32,
    pub moved: u32,
    pub removed: u32,
    pub unchanged: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanEvent {
    Started { source_id: Id },
    FileAdded { file_id: Id, rel_path: String },
    Finished(ScanReport),
}

/// A file under `SAMPLE_MAX_BYTES` (150 MiB) whose name has the word "sample", or that lies
/// in a folder named exactly "sample", is a release sample, not the film.
static SAMPLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\bsample\b").unwrap());
const SAMPLE_MAX_BYTES: u64 = 150 * 1024 * 1024;

fn is_sample(rel: &str, size: u64) -> bool {
    let mut parts = rel.rsplit('/');
    let name = parts.next().unwrap_or(rel);
    size < SAMPLE_MAX_BYTES
        && (SAMPLE.is_match(name) || parts.any(|dir| dir.eq_ignore_ascii_case("sample")))
}

/// A `sources` row: `path` is relative to the volume root (forward slashes, no leading
/// slash, `""` for the root itself); `last_path` is the last known absolute path.
pub(crate) struct SourceRow {
    pub(crate) id: Id,
    path: String,
    volume_id: String,
    last_path: String,
}

const SOURCE_COLS: &str = "id, path, volume_id, last_path";

fn source_row(r: &Row) -> rusqlite::Result<SourceRow> {
    Ok(SourceRow {
        id: r.get(0)?,
        path: r.get(1)?,
        volume_id: r.get(2)?,
        last_path: r.get(3)?,
    })
}

/// A `files` row as the scanner sees it.
struct FileRow {
    id: Id,
    rel_path: String,
    size: i64,
    mtime: i64,
}

fn file_row(r: &Row) -> rusqlite::Result<FileRow> {
    Ok(FileRow {
        id: r.get(0)?,
        rel_path: r.get(1)?,
        size: r.get(2)?,
        mtime: r.get(3)?,
    })
}

pub(crate) fn source_rows(c: &Connection) -> Result<Vec<SourceRow>> {
    let mut s = c.prepare_cached(&format!("SELECT {SOURCE_COLS} FROM sources ORDER BY id"))?;
    let rows = s
        .query_map([], source_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

pub fn add_source(c: &Connection, v: &dyn Volumes, path: &Path) -> Result<Source> {
    let abs = std::path::absolute(path)?;
    if !abs.is_dir() {
        return Err(Error::Invalid(format!("not a folder: {}", abs.display())));
    }
    let unknown = || Error::Invalid(format!("cannot identify the drive of {}", abs.display()));
    let volume_id = v.id_of(&abs).ok_or_else(unknown)?;
    let root = v.find(&volume_id).ok_or_else(unknown)?;
    let rel = abs
        .strip_prefix(&root)
        .ok()
        .and_then(rel_string)
        .ok_or_else(unknown)?;
    for other in source_rows(c)? {
        if other.volume_id == volume_id
            && (contains(&other.path, &rel) || contains(&rel, &other.path))
        {
            return Err(Error::Invalid(format!(
                "{} overlaps the existing source {}",
                abs.display(),
                other.last_path
            )));
        }
    }
    c.prepare_cached(
        "INSERT INTO sources(path, volume_id, added_at, last_path) VALUES (?1, ?2, ?3, ?4)",
    )?
    .execute(params![rel, volume_id, db::now(), abs.to_string_lossy()])?;
    Ok(Source {
        id: c.last_insert_rowid(),
        path: abs,
        volume_id,
        online: true,
    })
}

/// Deletes the source and its files; items left without files stay (hidden by queries).
pub fn remove_source(c: &Connection, id: Id) -> Result<()> {
    let n = c
        .prepare_cached("DELETE FROM sources WHERE id=?1")?
        .execute([id])?;
    if n == 0 {
        return Err(Error::NotFound(format!("source {id}")));
    }
    Ok(())
}

/// All sources with `online` filled and `path` resolved through the volume id.
pub fn list_sources(c: &Connection, v: &dyn Volumes) -> Result<Vec<Source>> {
    Ok(source_rows(c)?
        .into_iter()
        .map(|row| {
            let (path, online) = match resolve(v, &row) {
                Some(p) => {
                    let online = p.is_dir();
                    (p, online)
                }
                None => (PathBuf::from(&row.last_path), false),
            };
            Source {
                id: row.id,
                path,
                volume_id: row.volume_id,
                online,
            }
        })
        .collect())
}

pub fn scan_source(
    c: &Connection,
    v: &dyn Volumes,
    id: Id,
    on: &mut dyn FnMut(ScanEvent),
) -> Result<ScanReport> {
    scan_with(c, v, id, on, walk)
}

/// What a walk of a source found: video files (relative path to size and mtime) and the
/// relative paths of folders or files that could not be read.
type Walked = (HashMap<String, (i64, i64)>, Vec<String>);

/// `scan_source` with the walk passed in, so a test can stand in for unreadable folders.
fn scan_with(
    c: &Connection,
    v: &dyn Volumes,
    id: Id,
    on: &mut dyn FnMut(ScanEvent),
    walk: fn(&Path) -> Walked,
) -> Result<ScanReport> {
    let row = c
        .prepare_cached(&format!("SELECT {SOURCE_COLS} FROM sources WHERE id=?1"))?
        .query_row([id], source_row)
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("source {id}")))?;
    on(ScanEvent::Started { source_id: id });
    let mut report = ScanReport {
        source_id: id,
        online: false,
        added: 0,
        changed: 0,
        moved: 0,
        removed: 0,
        unchanged: 0,
    };
    let root = resolve(v, &row).filter(|p| std::fs::read_dir(long_path(p)).is_ok());
    let Some(root) = root else {
        on(ScanEvent::Finished(report.clone()));
        return Ok(report);
    };
    report.online = true;

    let (seen, unreadable) = walk(&root);
    let mut known: HashMap<String, FileRow> = {
        let mut s = c.prepare_cached(
            "SELECT id, rel_path, size, mtime FROM files WHERE source_id=?1 ORDER BY rel_path",
        )?;
        s.query_map([id], file_row)?
            .map(|r| r.map(|f| (path_key(&f.rel_path), f)))
            .collect::<rusqlite::Result<_>>()?
    };

    let mut changed = Vec::new();
    let mut new = Vec::new();
    let mut moved = Vec::new();
    for (rel, (size, mtime)) in &seen {
        let Some(f) = known.remove(&path_key(rel)) else {
            new.push((rel.clone(), *size, *mtime));
            continue;
        };
        // On Windows a rename that changes only letter case is the same file.
        let renamed = f.rel_path != *rel;
        if renamed {
            moved.push((f.id, rel.clone()));
        }
        if f.size != *size || f.mtime != *mtime {
            changed.push((f.id, *size, *mtime));
        } else if !renamed {
            report.unchanged += 1;
        }
    }
    // Rows under a folder that could not be read are left alone, not removed.
    let mut missing: Vec<FileRow> = known
        .into_values()
        .filter(|f| !unreadable.iter().any(|u| contains(u, &f.rel_path)))
        .collect();
    missing.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    new.sort();
    // A missing row and a new path alike in size and mtime are one moved file: when the file
    // name is the same, or when that size and mtime is unique among both.
    let mut alike: HashMap<(i64, i64), (u32, u32)> = HashMap::new();
    for f in &missing {
        alike.entry((f.size, f.mtime)).or_default().0 += 1;
    }
    for (_, size, mtime) in &new {
        alike.entry((*size, *mtime)).or_default().1 += 1;
    }
    new.retain(|(rel, size, mtime)| {
        let same = |f: &FileRow| f.size == *size && f.mtime == *mtime;
        let pos = missing
            .iter()
            .position(|f| same(f) && path_key(file_name(&f.rel_path)) == path_key(file_name(rel)))
            .or_else(|| {
                (alike[&(*size, *mtime)] == (1, 1))
                    .then(|| missing.iter().position(same))
                    .flatten()
            });
        match pos {
            Some(i) => {
                moved.push((missing.remove(i).id, rel.clone()));
                false
            }
            None => true,
        }
    });

    let scanned_at = db::now();
    let mut added = Vec::new();
    let tx = db::write_tx(c)?;
    for (file_id, size, mtime) in &changed {
        tx.prepare_cached("UPDATE files SET size=?2, mtime=?3, probed=0 WHERE id=?1")?
            .execute(params![file_id, size, mtime])?;
    }
    for (file_id, rel) in &moved {
        tx.prepare_cached("UPDATE files SET rel_path=?2 WHERE id=?1")?
            .execute(params![file_id, rel])?;
    }
    for f in &missing {
        tx.prepare_cached("DELETE FROM files WHERE id=?1")?
            .execute([f.id])?;
    }
    for (rel, size, mtime) in new {
        tx.prepare_cached(
            "INSERT INTO items(kind, title, match_state, added_at) \
             VALUES ('other', ?1, 'unmatched', ?2)",
        )?
        .execute(params![parse_path(&rel).title, scanned_at])?;
        let item_id = tx.last_insert_rowid();
        tx.prepare_cached(
            "INSERT INTO files(source_id, rel_path, size, mtime, item_id) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?
        .execute(params![id, rel, size, mtime, item_id])?;
        added.push((tx.last_insert_rowid(), rel));
    }
    tx.prepare_cached("UPDATE sources SET last_path=?2 WHERE id=?1")?
        .execute(params![id, root.to_string_lossy()])?;
    tx.commit()?;

    report.changed = changed.len() as u32;
    report.moved = moved.len() as u32;
    report.removed = missing.len() as u32;
    report.added = added.len() as u32;
    for (file_id, rel_path) in added {
        on(ScanEvent::FileAdded { file_id, rel_path });
    }
    on(ScanEvent::Finished(report.clone()));
    Ok(report)
}

/// Walks `root`; unreadable entries are skipped and listed.
fn walk(root: &Path) -> Walked {
    let walk_root = long_path(root);
    let rel_of = |p: &Path| p.strip_prefix(&walk_root).ok().and_then(rel_string);
    let mut seen = HashMap::new();
    let mut unreadable = Vec::new();
    for entry in WalkDir::new(&walk_root) {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                unreadable.extend(e.path().and_then(rel_of));
                continue;
            }
        };
        if !entry.file_type().is_file() || !is_video(entry.path()) {
            continue;
        }
        // Names that are not valid Unicode cannot be stored faithfully; skip them.
        let Some(rel) = rel_of(entry.path()) else {
            continue;
        };
        let Ok(md) = entry.metadata() else {
            unreadable.push(rel);
            continue;
        };
        if is_sample(&rel, md.len()) {
            continue;
        }
        let mtime = match md.modified().map(|t| t.duration_since(UNIX_EPOCH)) {
            Ok(Ok(d)) => d.as_secs() as i64,
            Ok(Err(e)) => -(e.duration().as_secs() as i64),
            Err(_) => 0,
        };
        seen.insert(rel, (md.len() as i64, mtime));
    }
    (seen, unreadable)
}

fn is_video(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Absolute path of a source through its volume's current root, if the volume is mounted.
pub(crate) fn resolve(v: &dyn Volumes, row: &SourceRow) -> Option<PathBuf> {
    Some(join_rel(&v.find(&row.volume_id)?, &row.path))
}

/// `root` joined with a stored relative path (forward slashes; `""` is `root` itself).
pub(crate) fn join_rel(root: &Path, rel: &str) -> PathBuf {
    rel.split('/')
        .filter(|s| !s.is_empty())
        .fold(root.to_path_buf(), |p, s| p.join(s))
}

/// `p` (relative) as forward-slash segments; `None` if it is not plain Unicode names.
fn rel_string(p: &Path) -> Option<String> {
    let parts = p
        .components()
        .map(|c| match c {
            Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some(parts.join("/"))
}

/// Whether relative path `inner` is `outer` or lies under it. Windows names are compared
/// case-insensitively, as the file system does.
fn contains(outer: &str, inner: &str) -> bool {
    let (outer, inner) = (path_key(outer), path_key(inner));
    outer.is_empty() || inner == outer || inner.starts_with(&format!("{outer}/"))
}

fn file_name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

/// A relative path as the file system compares it: case-insensitively on Windows.
fn path_key(rel: &str) -> String {
    if cfg!(windows) {
        rel.to_lowercase()
    } else {
        rel.to_string()
    }
}

/// On Windows, the `\\?\` extended-length form of an absolute path, so folders deeper than
/// 260 characters can be walked. Never stored: relative paths are taken against it.
#[cfg(windows)]
fn long_path(p: &Path) -> PathBuf {
    match p.to_str() {
        Some(s) if s.starts_with(r"\\?\") => p.to_path_buf(),
        Some(s) if s.starts_with(r"\\") => PathBuf::from(format!(r"\\?\UNC\{}", &s[2..])),
        Some(s) if p.is_absolute() => PathBuf::from(format!(r"\\?\{s}")),
        _ => p.to_path_buf(),
    }
}

#[cfg(not(windows))]
fn long_path(p: &Path) -> PathBuf {
    p.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::error::Error;
    use crate::volume::FakeVolumes;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    struct Env {
        _data: TempDir,
        drive: TempDir,
        c: Connection,
    }

    fn env() -> Env {
        let data = tempfile::tempdir().unwrap();
        let c = db::open(data.path()).unwrap();
        Env {
            _data: data,
            drive: tempfile::tempdir().unwrap(),
            c,
        }
    }

    fn vols(root: &Path) -> FakeVolumes {
        FakeVolumes(vec![("VOL1".to_string(), root.to_path_buf())])
    }

    fn at(root: &Path, rel: &str) -> PathBuf {
        rel.split('/').fold(root.to_path_buf(), |p, s| p.join(s))
    }

    fn write(root: &Path, rel: &str, bytes: usize) -> PathBuf {
        let p = at(root, rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, vec![0u8; bytes]).unwrap();
        p
    }

    const TREE: &[&str] = &[
        "Movies/Paprika (2006)/Paprika.2006.1080p.mkv",
        "Movies/Perfect Blue (1997).mp4",
        "Shows/Mononoke/Season 01/Mononoke.S01E01.avi",
    ];

    /// Media tree under `<root>/media`: three videos plus a .txt and an .nfo.
    fn media_tree(root: &Path) {
        for f in TREE {
            write(root, &format!("media/{f}"), 1000);
        }
        write(root, "media/readme.txt", 10);
        write(root, "media/Movies/Paprika (2006)/movie.nfo", 10);
    }

    fn scan(c: &Connection, v: &dyn Volumes, id: Id) -> ScanReport {
        scan_source(c, v, id, &mut |_| {}).unwrap()
    }

    fn report(id: Id, online: bool, counts: [u32; 5]) -> ScanReport {
        let [added, changed, moved, removed, unchanged] = counts;
        ScanReport {
            source_id: id,
            online,
            added,
            changed,
            moved,
            removed,
            unchanged,
        }
    }

    fn file_rows(c: &Connection) -> Vec<(String, Option<Id>, i64)> {
        let mut s = c
            .prepare("SELECT rel_path, item_id, probed FROM files ORDER BY rel_path")
            .unwrap();
        s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    fn count(c: &Connection, table: &str) -> i64 {
        c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    fn item_of(c: &Connection, rel: &str) -> Id {
        c.query_row("SELECT item_id FROM files WHERE rel_path=?1", [rel], |r| {
            r.get(0)
        })
        .unwrap()
    }

    #[test]
    fn first_scan_adds_video_files_only() {
        let e = env();
        media_tree(e.drive.path());
        let v = vols(e.drive.path());
        let src = add_source(&e.c, &v, &e.drive.path().join("media")).unwrap();
        assert!(src.online);
        assert_eq!(src.path, e.drive.path().join("media"));
        assert_eq!(src.volume_id, "VOL1");

        let mut events = Vec::new();
        let r = scan_source(&e.c, &v, src.id, &mut |ev| events.push(ev)).unwrap();
        assert_eq!(r, report(src.id, true, [3, 0, 0, 0, 0]));
        assert_eq!(
            events.first(),
            Some(&ScanEvent::Started { source_id: src.id })
        );
        assert_eq!(events.last(), Some(&ScanEvent::Finished(r.clone())));
        let mut added: Vec<String> = events
            .iter()
            .filter_map(|ev| match ev {
                ScanEvent::FileAdded { rel_path, .. } => Some(rel_path.clone()),
                _ => None,
            })
            .collect();
        added.sort();
        assert_eq!(added, TREE.to_vec());

        let rows = file_rows(&e.c);
        assert_eq!(
            rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
            TREE.to_vec()
        );
        let mut s =
            e.c.prepare(
                "SELECT i.kind, i.match_state, i.title, i.added_at FROM files f \
                 JOIN items i ON i.id = f.item_id ORDER BY f.rel_path",
            )
            .unwrap();
        let items: Vec<(String, String, String, i64)> = s
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(items.len(), 3);
        for (i, (kind, state, title, added_at)) in items.iter().enumerate() {
            assert_eq!(kind, "other");
            assert_eq!(state, "unmatched");
            assert_eq!(title, &crate::parse::parse_path(TREE[i]).title);
            assert!(*added_at > 0);
        }
        assert_eq!(count(&e.c, "items"), 3);
    }

    #[test]
    fn rescan_is_unchanged() {
        let e = env();
        media_tree(e.drive.path());
        let v = vols(e.drive.path());
        let src = add_source(&e.c, &v, &e.drive.path().join("media")).unwrap();
        scan(&e.c, &v, src.id);
        let before = file_rows(&e.c);
        assert_eq!(
            scan(&e.c, &v, src.id),
            report(src.id, true, [0, 0, 0, 0, 3])
        );
        assert_eq!(file_rows(&e.c), before);
        assert_eq!(count(&e.c, "items"), 3);
    }

    #[test]
    fn changed_file_resets_probed() {
        let e = env();
        media_tree(e.drive.path());
        let v = vols(e.drive.path());
        let src = add_source(&e.c, &v, &e.drive.path().join("media")).unwrap();
        scan(&e.c, &v, src.id);
        e.c.execute("UPDATE files SET probed = 1", []).unwrap();
        write(e.drive.path(), &format!("media/{}", TREE[1]), 2000);

        assert_eq!(
            scan(&e.c, &v, src.id),
            report(src.id, true, [0, 1, 0, 0, 2])
        );
        let (size, probed): (i64, i64) =
            e.c.query_row(
                "SELECT size, probed FROM files WHERE rel_path=?1",
                [TREE[1]],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((size, probed), (2000, 0));
        let others: i64 =
            e.c.query_row("SELECT count(*) FROM files WHERE probed = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(others, 2);
    }

    #[test]
    fn moved_file_keeps_item() {
        let e = env();
        media_tree(e.drive.path());
        let v = vols(e.drive.path());
        let src = add_source(&e.c, &v, &e.drive.path().join("media")).unwrap();
        scan(&e.c, &v, src.id);
        let item = item_of(&e.c, TREE[1]);
        let media = e.drive.path().join("media");
        fs::create_dir_all(media.join("Archive")).unwrap();
        fs::rename(
            media.join("Movies").join("Perfect Blue (1997).mp4"),
            media.join("Archive").join("Perfect Blue (1997).mp4"),
        )
        .unwrap();

        assert_eq!(
            scan(&e.c, &v, src.id),
            report(src.id, true, [0, 0, 1, 0, 2])
        );
        assert_eq!(item_of(&e.c, "Archive/Perfect Blue (1997).mp4"), item);
        assert_eq!(count(&e.c, "files"), 3);
        assert_eq!(count(&e.c, "items"), 3);
    }

    #[test]
    fn rows_under_an_unreadable_folder_are_kept() {
        let e = env();
        media_tree(e.drive.path());
        let v = vols(e.drive.path());
        let src = add_source(&e.c, &v, &e.drive.path().join("media")).unwrap();
        scan(&e.c, &v, src.id);
        let before = file_rows(&e.c);
        // `Movies` cannot be read: its files are not seen, and the folder is listed.
        fn denied(root: &Path) -> Walked {
            let (mut seen, mut unreadable) = walk(root);
            seen.retain(|rel, _| !rel.starts_with("Movies/"));
            unreadable.push("Movies".into());
            (seen, unreadable)
        }
        let r = scan_with(&e.c, &v, src.id, &mut |_| {}, denied).unwrap();
        assert_eq!(r, report(src.id, true, [0, 0, 0, 0, 1]));
        assert_eq!(file_rows(&e.c), before);
    }

    #[cfg(windows)]
    #[test]
    fn case_only_rename_keeps_item() {
        let e = env();
        media_tree(e.drive.path());
        let v = vols(e.drive.path());
        let src = add_source(&e.c, &v, &e.drive.path().join("media")).unwrap();
        scan(&e.c, &v, src.id);
        let item = item_of(&e.c, TREE[0]);
        let dir = e
            .drive
            .path()
            .join("media")
            .join("Movies")
            .join("Paprika (2006)");
        fs::rename(
            dir.join("Paprika.2006.1080p.mkv"),
            dir.join("PAPRIKA.2006.1080p.mkv"),
        )
        .unwrap();
        assert_eq!(
            scan(&e.c, &v, src.id),
            report(src.id, true, [0, 0, 1, 0, 2])
        );
        assert_eq!(
            item_of(&e.c, "Movies/Paprika (2006)/PAPRIKA.2006.1080p.mkv"),
            item
        );
        assert_eq!(count(&e.c, "items"), 3);
    }

    /// Writes `bytes` zero bytes at `rel` with a fixed mtime.
    fn write_at(root: &Path, rel: &str, bytes: usize) {
        let p = write(root, rel, bytes);
        let t = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        fs::File::options()
            .write(true)
            .open(p)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }

    #[test]
    fn rename_with_unique_size_and_mtime_keeps_item() {
        let e = env();
        let d = e.drive.path();
        write_at(d, "media/a.mkv", 10);
        write_at(d, "media/twin1.mkv", 20);
        write_at(d, "media/twin2.mkv", 20);
        let v = vols(d);
        let src = add_source(&e.c, &v, &d.join("media")).unwrap();
        scan(&e.c, &v, src.id);
        let item = item_of(&e.c, "a.mkv");
        let media = d.join("media");
        fs::create_dir_all(media.join("Films")).unwrap();
        fs::rename(
            media.join("a.mkv"),
            media.join("Films").join("Alpha (2001).mkv"),
        )
        .unwrap();
        // Two files alike in size and mtime are not told apart: they are removed and added.
        fs::rename(media.join("twin1.mkv"), media.join("Twin One.mkv")).unwrap();
        fs::rename(media.join("twin2.mkv"), media.join("Twin Two.mkv")).unwrap();
        assert_eq!(
            scan(&e.c, &v, src.id),
            report(src.id, true, [2, 0, 1, 2, 0])
        );
        assert_eq!(item_of(&e.c, "Films/Alpha (2001).mkv"), item);
    }

    #[test]
    fn removed_file_keeps_item_row() {
        let e = env();
        media_tree(e.drive.path());
        let v = vols(e.drive.path());
        let src = add_source(&e.c, &v, &e.drive.path().join("media")).unwrap();
        scan(&e.c, &v, src.id);
        let item = item_of(&e.c, TREE[2]);
        fs::remove_file(at(e.drive.path(), &format!("media/{}", TREE[2]))).unwrap();

        assert_eq!(
            scan(&e.c, &v, src.id),
            report(src.id, true, [0, 0, 0, 1, 2])
        );
        assert_eq!(count(&e.c, "files"), 2);
        let n: i64 =
            e.c.query_row("SELECT count(*) FROM items WHERE id=?1", [item], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn offline_source_changes_nothing() {
        let e = env();
        media_tree(e.drive.path());
        let v = vols(e.drive.path());
        let src = add_source(&e.c, &v, &e.drive.path().join("media")).unwrap();
        scan(&e.c, &v, src.id);
        let before = file_rows(&e.c);

        let unplugged = FakeVolumes(vec![]);
        assert_eq!(
            scan(&e.c, &unplugged, src.id),
            report(src.id, false, [0, 0, 0, 0, 0])
        );
        assert_eq!(file_rows(&e.c), before);
        assert_eq!(count(&e.c, "items"), 3);
        let listed = list_sources(&e.c, &unplugged).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].online);
        assert_eq!(listed[0].path, e.drive.path().join("media"));

        // Volume present but the folder is gone: also offline, nothing removed.
        fs::remove_dir_all(e.drive.path().join("media")).unwrap();
        assert_eq!(
            scan(&e.c, &v, src.id),
            report(src.id, false, [0, 0, 0, 0, 0])
        );
        assert_eq!(file_rows(&e.c), before);
    }

    #[test]
    fn drive_letter_change_resolves() {
        let e = env();
        media_tree(e.drive.path());
        let src = add_source(&e.c, &vols(e.drive.path()), &e.drive.path().join("media")).unwrap();
        scan(&e.c, &vols(e.drive.path()), src.id);

        let other = tempfile::tempdir().unwrap();
        media_tree(other.path());
        // Same mtimes as the original tree, as on the same physical drive.
        for f in TREE {
            let rel = format!("media/{f}");
            let mtime = fs::metadata(at(e.drive.path(), &rel))
                .unwrap()
                .modified()
                .unwrap();
            let p = write(other.path(), &rel, 1000);
            fs::File::options()
                .write(true)
                .open(p)
                .unwrap()
                .set_modified(mtime)
                .unwrap();
        }
        let moved = vols(other.path());
        assert_eq!(
            scan(&e.c, &moved, src.id),
            report(src.id, true, [0, 0, 0, 0, 3])
        );
        let listed = list_sources(&e.c, &moved).unwrap();
        assert!(listed[0].online);
        assert_eq!(listed[0].path, other.path().join("media"));
    }

    #[test]
    fn unicode_and_long_paths_round_trip() {
        let e = env();
        let long_dir = (0..6)
            .map(|i| format!("{i}-{}", "x".repeat(45)))
            .collect::<Vec<_>>()
            .join("/");
        let rels = [
            "Amélie (2001)/Amélie.2001.mkv".to_string(),
            "千と千尋の神隠し (2001)/千と千尋の神隠し.mkv".to_string(),
            format!("{long_dir}/Long Movie (2010).mkv"),
            "🎬 Movie Night (2019).mkv".to_string(),
        ];
        for r in &rels {
            write(e.drive.path(), &format!("media/{r}"), 1000);
        }
        let v = vols(e.drive.path());
        let src = add_source(&e.c, &v, &e.drive.path().join("media")).unwrap();
        let full = src
            .path
            .join(rels[2].replace('/', std::path::MAIN_SEPARATOR_STR));
        assert!(full.to_string_lossy().chars().count() > 260);

        assert_eq!(
            scan(&e.c, &v, src.id),
            report(src.id, true, [4, 0, 0, 0, 0])
        );
        let mut stored: Vec<String> = file_rows(&e.c).into_iter().map(|r| r.0).collect();
        stored.sort();
        let mut expected = rels.to_vec();
        expected.sort();
        assert_eq!(stored, expected);
        assert_eq!(
            scan(&e.c, &v, src.id),
            report(src.id, true, [0, 0, 0, 0, 4])
        );
    }

    #[test]
    fn sample_files_skipped() {
        let e = env();
        write(e.drive.path(), "media/Paprika (2006)/Paprika.mkv", 1000);
        write(
            e.drive.path(),
            "media/Paprika (2006)/paprika-sample.mkv",
            1000,
        );
        write(e.drive.path(), "media/Paprika (2006)/Sample/clip.mkv", 1000);
        write(e.drive.path(), "media/Samples Of Life (2001).mkv", 1000);
        write(e.drive.path(), "media/Movie/movie-sample.mkv", 1000);
        write(e.drive.path(), "media/Movie/SAMPLE/clip.mkv", 1000);
        // "sample" in a folder name that is not exactly "sample" does not count.
        write(e.drive.path(), "media/The Sample Show/S01E01.mkv", 1000);
        // A big file named "sample" is kept: only small clips are samples.
        let big = write(e.drive.path(), "media/The Sample (2020).mkv", 0);
        fs::File::options()
            .write(true)
            .open(big)
            .unwrap()
            .set_len(150 * 1024 * 1024)
            .unwrap();
        let v = vols(e.drive.path());
        let src = add_source(&e.c, &v, &e.drive.path().join("media")).unwrap();

        assert_eq!(
            scan(&e.c, &v, src.id),
            report(src.id, true, [4, 0, 0, 0, 0])
        );
        let stored: Vec<String> = file_rows(&e.c).into_iter().map(|r| r.0).collect();
        assert_eq!(
            stored,
            vec![
                "Paprika (2006)/Paprika.mkv",
                "Samples Of Life (2001).mkv",
                "The Sample (2020).mkv",
                "The Sample Show/S01E01.mkv"
            ]
        );
    }

    #[test]
    fn nested_source_rejected() {
        let e = env();
        media_tree(e.drive.path());
        let v = vols(e.drive.path());
        let media = e.drive.path().join("media");
        add_source(&e.c, &v, &media).unwrap();
        for p in [
            media.join("Movies"),
            e.drive.path().to_path_buf(),
            media.clone(),
        ] {
            assert!(
                matches!(add_source(&e.c, &v, &p), Err(Error::Invalid(_))),
                "{p:?}"
            );
        }
        // A sibling folder on the same volume is fine.
        fs::create_dir_all(e.drive.path().join("media2")).unwrap();
        add_source(&e.c, &v, &e.drive.path().join("media2")).unwrap();
        assert_eq!(list_sources(&e.c, &v).unwrap().len(), 2);
    }

    #[test]
    fn same_relative_path_on_two_volumes() {
        let e = env();
        let other = tempfile::tempdir().unwrap();
        let v = FakeVolumes(vec![
            ("VOL1".to_string(), e.drive.path().to_path_buf()),
            ("VOL2".to_string(), other.path().to_path_buf()),
        ]);
        let a = add_source(&e.c, &v, e.drive.path()).unwrap();
        let b = add_source(&e.c, &v, other.path()).unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(b.path, other.path());
    }

    #[test]
    fn remove_source_keeps_items() {
        let e = env();
        media_tree(e.drive.path());
        let v = vols(e.drive.path());
        let src = add_source(&e.c, &v, &e.drive.path().join("media")).unwrap();
        scan(&e.c, &v, src.id);
        remove_source(&e.c, src.id).unwrap();
        assert_eq!(count(&e.c, "files"), 0);
        assert_eq!(count(&e.c, "items"), 3);
        assert!(list_sources(&e.c, &v).unwrap().is_empty());
        assert!(matches!(
            remove_source(&e.c, src.id),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn add_source_rejects_missing_folder_and_unknown_volume() {
        let e = env();
        let v = vols(e.drive.path());
        assert!(matches!(
            add_source(&e.c, &v, &e.drive.path().join("nope")),
            Err(Error::Invalid(_))
        ));
        let elsewhere = tempfile::tempdir().unwrap();
        assert!(matches!(
            add_source(&e.c, &v, elsewhere.path()),
            Err(Error::Invalid(_))
        ));
    }
}
