//! Folders for the in-app folder browser: drive roots, then sub-folders one level at a time.

use std::io;
use std::path::Path;

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Folder {
    pub name: String,
    pub path: String,
}

/// Drive roots for `None`; otherwise the visible sub-folders of `path`, sorted by name
/// ignoring case. Files, hidden and system folders are left out.
pub fn list_folders(path: Option<&Path>) -> io::Result<Vec<Folder>> {
    let Some(path) = path else {
        return Ok(roots());
    };
    let mut found = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        // An entry that vanished or cannot be read is not offered.
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() && !name.starts_with('.') && !hidden(&meta) {
            found.push(Folder {
                name,
                path: entry.path().to_string_lossy().to_string(),
            });
        }
    }
    found.sort_by_cached_key(|f| (f.name.to_lowercase(), f.name.clone()));
    Ok(found)
}

#[cfg(windows)]
fn hidden(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    meta.file_attributes() & (HIDDEN | SYSTEM) != 0
}

#[cfg(not(windows))]
fn hidden(_meta: &std::fs::Metadata) -> bool {
    false
}

/// Every drive letter that answers, as `C:` at `C:\`.
#[cfg(windows)]
fn roots() -> Vec<Folder> {
    (b'A'..=b'Z')
        .map(|l| format!("{}:", l as char))
        .filter(|name| Path::new(&format!("{name}\\")).is_dir())
        .map(|name| Folder {
            path: format!("{name}\\"),
            name,
        })
        .collect()
}

#[cfg(not(windows))]
fn roots() -> Vec<Folder> {
    vec![Folder {
        name: "/".into(),
        path: "/".into(),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn list_folders_lists_only_visible_subfolders() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["beta", "Alpha", "gamma", ".dot", "secret"] {
            fs::create_dir(dir.path().join(name)).unwrap();
        }
        fs::write(dir.path().join("file.mkv"), b"x").unwrap();
        #[cfg(windows)]
        {
            let hidden = dir.path().join("secret");
            let ok = std::process::Command::new("attrib")
                .arg("+h")
                .arg(&hidden)
                .status()
                .unwrap();
            assert!(ok.success());
        }
        #[cfg(not(windows))]
        fs::rename(dir.path().join("secret"), dir.path().join(".secret")).unwrap();

        let found = list_folders(Some(dir.path())).unwrap();
        let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Alpha", "beta", "gamma"]);
        assert_eq!(
            found[0].path,
            dir.path().join("Alpha").to_string_lossy().to_string()
        );
    }

    #[test]
    fn list_folders_of_a_missing_folder_is_an_io_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_folders(Some(&dir.path().join("gone"))).is_err());
    }

    #[test]
    fn list_folders_none_lists_roots() {
        let roots = list_folders(None).unwrap();
        assert!(!roots.is_empty());
        let system = std::env::temp_dir();
        let root = system
            .ancestors()
            .last()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(
            roots
                .iter()
                .any(|r| r.path.eq_ignore_ascii_case(&root) && Path::new(&r.path).is_dir()),
            "{roots:?} lacks {root}"
        );
    }
}
