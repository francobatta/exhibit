//! A library in a temp directory over one fake drive, no tools, secrets in memory.

use std::fs;
use std::path::{Path, PathBuf};

use exhibit_core::Library;
use exhibit_core::secrets::MemorySecrets;
use exhibit_core::tools::Env;
use exhibit_core::volume::Volumes;
use tempfile::TempDir;

/// One drive, always mounted at its temp directory.
struct Drive(PathBuf);

impl Volumes for Drive {
    fn id_of(&self, path: &Path) -> Option<String> {
        path.starts_with(&self.0).then(|| "VOL1".to_string())
    }

    fn find(&self, volume_id: &str) -> Option<PathBuf> {
        (volume_id == "VOL1").then(|| self.0.clone())
    }
}

pub struct Fixture {
    pub _data: TempDir,
    pub _drive: TempDir,
    pub lib: Library,
}

impl Fixture {
    /// A library with one source, `media`, holding empty files named `files`.
    pub fn new(files: &[&str]) -> Fixture {
        let data = tempfile::tempdir().unwrap();
        let drive = tempfile::tempdir().unwrap();
        let media = drive.path().join("media");
        fs::create_dir_all(&media).unwrap();
        for f in files {
            fs::write(media.join(f), b"x").unwrap();
        }
        let lib = Library::open(
            data.path().to_path_buf(),
            Box::new(Drive(drive.path().to_path_buf())),
            Box::new(MemorySecrets::default()),
            Env::default(),
        )
        .unwrap();
        lib.add_source(&media).unwrap();
        Fixture {
            _data: data,
            _drive: drive,
            lib,
        }
    }
}
