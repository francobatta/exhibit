//! Drive identity: a volume id survives a changed drive letter, so a source on an external
//! drive resolves wherever the drive is mounted next time.

use std::path::{Path, PathBuf};

pub trait Volumes {
    /// Id of the volume holding `path` (an absolute path), or `None` if it cannot be read.
    fn id_of(&self, path: &Path) -> Option<String>;
    /// Current root of the volume (for example `E:\`), or `None` if it is not mounted.
    fn find(&self, volume_id: &str) -> Option<PathBuf>;
    /// Whether the folder `path` can be read now.
    fn is_online(&self, path: &Path) -> bool {
        path.is_dir()
    }
}

/// The machine's real volumes. On Windows a drive is identified by its volume serial
/// number (`GetVolumeInformationW`), found again among the drive letters; a network share
/// (`\\server\share`) is identified by its UNC root. What it reads of the drives is kept for
/// the whole process, so every connection the app opens shares it.
#[derive(Default)]
pub struct SystemVolumes;

/// How long a folder's online state is reused. Every listing asks which sources are online;
/// a sleeping disk or a network share that went away can take seconds to answer.
const ONLINE_FRESH: std::time::Duration = std::time::Duration::from_secs(2);

/// Whether `path` is a readable folder, asked of the disk at most once every `ONLINE_FRESH`.
/// The disk is asked outside the lock, so one slow share never holds up another path.
fn online_now(path: &Path) -> bool {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::Instant;
    static SEEN: Mutex<Option<HashMap<PathBuf, (bool, Instant)>>> = Mutex::new(None);
    let lock = || SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&(online, at)) = lock().get_or_insert_with(HashMap::new).get(path)
        && at.elapsed() < ONLINE_FRESH
    {
        return online;
    }
    let online = path.is_dir();
    lock()
        .get_or_insert_with(HashMap::new)
        .insert(path.to_path_buf(), (online, Instant::now()));
    online
}

impl SystemVolumes {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(windows)]
impl Volumes for SystemVolumes {
    fn is_online(&self, path: &Path) -> bool {
        online_now(path)
    }

    fn id_of(&self, path: &Path) -> Option<String> {
        use std::path::{Component, Prefix};
        let Some(Component::Prefix(p)) = path.components().next() else {
            return None;
        };
        match p.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                win::serial(&format!("{}:\\", letter as char)).map(|s| format!("{s:08X}"))
            }
            Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
                let root = format!(
                    "\\\\{}\\{}\\",
                    server.to_string_lossy(),
                    share.to_string_lossy()
                );
                win::serial(&root).map(|_| format!("unc:{root}"))
            }
            _ => None,
        }
    }

    fn find(&self, volume_id: &str) -> Option<PathBuf> {
        if let Some(root) = volume_id.strip_prefix("unc:") {
            return win::serial(root).map(|_| PathBuf::from(root));
        }
        let serial = u32::from_str_radix(volume_id, 16).ok()?;
        let mut cache = win::DRIVES.lock().unwrap_or_else(|e| e.into_inner());
        if cache
            .as_ref()
            .is_none_or(|d| d.read_at.elapsed() >= win::FRESH)
        {
            *cache = Some(win::Drives::read());
        }
        let d = cache.as_mut()?;
        // Network drives can hang when their server is gone: asked only when no local one matches.
        let letter = pick_letter(&d.local, serial).or_else(|| {
            let remote = d.remote.get_or_insert_with(|| win::Drives::letters(true));
            pick_letter(remote, serial)
        })?;
        Some(PathBuf::from(format!("{letter}:\\")))
    }
}

#[cfg(windows)]
mod win {
    use super::DRIVE_REMOTE;
    use std::ptr::null_mut;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };

    /// How long a read of the drive letters is reused.
    pub const FRESH: Duration = Duration::from_secs(3);

    /// Drive letters and their serials, read at most once every `FRESH`.
    pub static DRIVES: std::sync::Mutex<Option<Drives>> = std::sync::Mutex::new(None);

    /// `(letter, serial, drive type)` of the mounted drives; network drives read lazily.
    pub struct Drives {
        pub read_at: Instant,
        pub local: Vec<(char, u32, u32)>,
        pub remote: Option<Vec<(char, u32, u32)>>,
    }

    impl Drives {
        pub fn read() -> Drives {
            Drives {
                read_at: Instant::now(),
                local: Drives::letters(false),
                remote: None,
            }
        }

        /// The network drives when `remote`, else every other drive with a readable serial.
        pub fn letters(remote: bool) -> Vec<(char, u32, u32)> {
            // SAFETY: no arguments; returns a bitmask.
            let mask = unsafe { GetLogicalDrives() };
            (0..26u8)
                .filter(|i| mask & (1 << i) != 0)
                .filter_map(|i| {
                    let letter = (b'A' + i) as char;
                    let root = format!("{letter}:\\");
                    let wide: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
                    // SAFETY: `wide` is NUL-terminated and outlives the call.
                    let kind = unsafe { GetDriveTypeW(wide.as_ptr()) };
                    ((kind == DRIVE_REMOTE) == remote).then_some(())?;
                    Some((letter, serial(&root)?, kind))
                })
                .collect()
        }
    }

    /// Volume serial number of the volume whose root is `root` (ends with a backslash).
    pub fn serial(root: &str) -> Option<u32> {
        let wide: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
        let mut serial = 0u32;
        // SAFETY: `wide` is NUL-terminated and outlives the call; every other out-pointer
        // is either null with a zero size or points at a live local.
        let ok = unsafe {
            GetVolumeInformationW(
                wide.as_ptr(),
                null_mut(),
                0,
                &mut serial,
                null_mut(),
                null_mut(),
                null_mut(),
                0,
            )
        };
        (ok != 0).then_some(serial)
    }
}

/// Non-Windows stub: the id is the path's root and `find` returns it if it exists.
#[cfg(not(windows))]
impl Volumes for SystemVolumes {
    fn is_online(&self, path: &Path) -> bool {
        online_now(path)
    }

    fn id_of(&self, path: &Path) -> Option<String> {
        path.ancestors()
            .last()
            .filter(|r| r.exists())
            .map(|r| r.to_string_lossy().into_owned())
    }

    fn find(&self, volume_id: &str) -> Option<PathBuf> {
        let p = PathBuf::from(volume_id);
        p.exists().then_some(p)
    }
}

/// Test double: a fixed map of volume id to root directory.
#[cfg(test)]
pub(crate) struct FakeVolumes(pub Vec<(String, PathBuf)>);

#[cfg(test)]
impl Volumes for FakeVolumes {
    fn id_of(&self, path: &Path) -> Option<String> {
        self.0
            .iter()
            .filter(|(_, root)| path.starts_with(root))
            .max_by_key(|(_, root)| root.components().count())
            .map(|(id, _)| id.clone())
    }

    fn find(&self, volume_id: &str) -> Option<PathBuf> {
        self.0
            .iter()
            .find(|(id, _)| id == volume_id)
            .map(|(_, root)| root.clone())
    }
}

/// `GetDriveTypeW` results.
const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;
const DRIVE_REMOTE: u32 = 4;

/// The letter of volume `serial` among `(letter, serial, drive type)`: a fixed or removable
/// drive before any other, a network drive last, then the earliest letter.
#[cfg_attr(not(windows), allow(dead_code))]
fn pick_letter(drives: &[(char, u32, u32)], serial: u32) -> Option<char> {
    let rank = |kind: u32| match kind {
        DRIVE_FIXED | DRIVE_REMOVABLE => 0,
        DRIVE_REMOTE => 2,
        _ => 1,
    };
    drives
        .iter()
        .filter(|d| d.1 == serial)
        .min_by_key(|&&(letter, _, kind)| (rank(kind), letter))
        .map(|d| d.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letter_choice_prefers_local_drives() {
        const CDROM: u32 = 5;
        let drives = [
            ('D', 7, DRIVE_REMOTE),
            ('E', 7, CDROM),
            ('F', 7, DRIVE_FIXED),
            ('G', 9, DRIVE_REMOVABLE),
        ];
        assert_eq!(pick_letter(&drives, 7), Some('F'));
        assert_eq!(pick_letter(&drives[..2], 7), Some('E'));
        // A source added on a network drive still resolves to it.
        assert_eq!(pick_letter(&drives[..1], 7), Some('D'));
        assert_eq!(pick_letter(&drives, 9), Some('G'));
        assert_eq!(pick_letter(&drives, 1), None);
    }

    #[test]
    fn system_volumes_round_trip_the_temp_dir() {
        let d = tempfile::tempdir().unwrap();
        let v = SystemVolumes::new();
        let id = v.id_of(d.path()).expect("temp dir has a volume");
        let root = v.find(&id).expect("volume is mounted");
        assert!(d.path().starts_with(&root), "{root:?}");
    }

    #[test]
    fn system_volumes_unknown_id_is_none() {
        assert_eq!(SystemVolumes::new().find("not-a-volume"), None);
    }
}
