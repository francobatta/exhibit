//! Folders for the in-app folder browser: drive roots, then sub-folders one level at a time,
//! and optionally the files of one kind among them (a tool's `.exe`).

use std::io;
use std::path::Path;

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Folder {
    pub name: String,
    pub path: String,
    /// A file with the asked-for extension, not a folder.
    pub file: bool,
}

/// Drive roots for `None`; otherwise the visible sub-folders of `path`, sorted by name
/// ignoring case, then (with `ext`, such as `"exe"`) the visible files with that extension,
/// sorted the same way. Hidden and system entries are left out.
pub fn list_folders(path: Option<&Path>, ext: Option<&str>) -> io::Result<Vec<Folder>> {
    let Some(path) = path else {
        return Ok(roots());
    };
    let mut folders = Vec::new();
    let mut files = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        // An entry that vanished or cannot be read is not offered.
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if name.starts_with('.') || hidden(&meta) {
            continue;
        }
        let path = entry.path();
        let file = meta.is_file()
            && ext.is_some_and(|ext| {
                path.extension()
                    .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(ext))
            });
        if meta.is_dir() || file {
            let found = Folder {
                name,
                path: path.to_string_lossy().to_string(),
                file,
            };
            if file { &mut files } else { &mut folders }.push(found);
        }
    }
    for list in [&mut folders, &mut files] {
        list.sort_by_cached_key(|f| (f.name.to_lowercase(), f.name.clone()));
    }
    folders.append(&mut files);
    Ok(folders)
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
            file: false,
        })
        .collect()
}

#[cfg(not(windows))]
fn roots() -> Vec<Folder> {
    vec![Folder {
        name: "/".into(),
        path: "/".into(),
        file: false,
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

        let found = list_folders(Some(dir.path()), None).unwrap();
        let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Alpha", "beta", "gamma"]);
        assert_eq!(
            found[0].path,
            dir.path().join("Alpha").to_string_lossy().to_string()
        );
        assert!(found.iter().all(|f| !f.file));
    }

    #[test]
    fn list_folders_with_an_extension_adds_those_files_after_the_folders() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("zeta")).unwrap();
        for name in ["mpv.exe", "Alpha.EXE", "notes.txt", "mpv.com"] {
            fs::write(dir.path().join(name), b"x").unwrap();
        }
        let found = list_folders(Some(dir.path()), Some("exe")).unwrap();
        let entries: Vec<(&str, bool)> = found.iter().map(|f| (f.name.as_str(), f.file)).collect();
        assert_eq!(
            entries,
            [("zeta", false), ("Alpha.EXE", true), ("mpv.exe", true)]
        );
    }

    #[test]
    fn list_folders_of_a_missing_folder_is_an_io_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_folders(Some(&dir.path().join("gone")), None).is_err());
    }

    #[test]
    fn list_folders_none_lists_roots() {
        let roots = list_folders(None, None).unwrap();
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
