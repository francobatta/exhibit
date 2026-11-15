//! Drive identity: a volume id survives a changed drive letter, so a source on an external
//! drive resolves wherever the drive is mounted next time.

use std::path::{Path, PathBuf};

pub trait Volumes {
    /// Id of the volume holding `path` (an absolute path), or `None` if it cannot be read.
    fn id_of(&self, path: &Path) -> Option<String>;
    /// Current root of the volume (for example `E:\`), or `None` if it is not mounted.
    fn find(&self, volume_id: &str) -> Option<PathBuf>;
}

/// The machine's real volumes. On Windows a drive is identified by its volume serial
/// number (`GetVolumeInformationW`), found again by trying every drive letter; a network
/// share (`\\server\share`) is identified by its UNC root.
pub struct SystemVolumes;

#[cfg(windows)]
impl Volumes for SystemVolumes {
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
        let drives = win::logical_drives();
        (0..26u8)
            .filter(|i| drives & (1 << i) != 0)
            .map(|i| format!("{}:\\", (b'A' + i) as char))
            .find(|root| {
                win::serial(root).map(|s| format!("{s:08X}")).as_deref() == Some(volume_id)
            })
            .map(PathBuf::from)
    }
}

#[cfg(windows)]
mod win {
    use std::ptr::null_mut;
    use windows_sys::Win32::Storage::FileSystem::{GetLogicalDrives, GetVolumeInformationW};

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

    pub fn logical_drives() -> u32 {
        // SAFETY: no arguments; returns a bitmask.
        unsafe { GetLogicalDrives() }
    }
}

/// Non-Windows stub: the id is the path's root and `find` returns it if it exists.
#[cfg(not(windows))]
impl Volumes for SystemVolumes {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_volumes_round_trip_the_temp_dir() {
        let d = tempfile::tempdir().unwrap();
        let v = SystemVolumes;
        let id = v.id_of(d.path()).expect("temp dir has a volume");
        let root = v.find(&id).expect("volume is mounted");
        assert!(d.path().starts_with(&root), "{root:?}");
    }

    #[test]
    fn system_volumes_unknown_id_is_none() {
        assert_eq!(SystemVolumes.find("not-a-volume"), None);
    }
}
