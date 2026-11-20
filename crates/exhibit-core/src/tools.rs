use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::{get_setting, set_setting};
use crate::error::Result;

const VERSION_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Mpv,
    Ffmpeg,
}

const ALL: [Tool; 2] = [Tool::Mpv, Tool::Ffmpeg];

impl Tool {
    fn name(self) -> &'static str {
        match self {
            Tool::Mpv => "mpv",
            Tool::Ffmpeg => "ffmpeg",
        }
    }

    pub(crate) fn setting_key(self) -> String {
        format!("tool.{}", self.name())
    }

    fn version_flag(self) -> &'static str {
        match self {
            Tool::Mpv => "--version",
            _ => "-version",
        }
    }

    /// File names to look for, in preference order.
    fn file_names(self) -> Vec<String> {
        let n = self.name();
        if !cfg!(windows) {
            return vec![n.to_string()];
        }
        let mut v = vec![format!("{n}.exe")];
        if self == Tool::Mpv {
            v.push("mpv.com".to_string());
        }
        v
    }
}

/// Where the copy in use came from, in the order they win.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolSource {
    /// A path the owner set by hand. Never replaced.
    Manual,
    /// Exhibit's own download, in the tools folder.
    Downloaded,
    /// Found on the PATH, in a usual install folder or beside another tool.
    Detected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolStatus {
    pub tool: Tool,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
    /// `None` when nothing was found.
    pub source: Option<ToolSource>,
    /// The release (`YYYYMMDD`) of the downloaded copy in use.
    pub release: Option<String>,
    /// A downloaded copy exists, in use or not: it takes over when a hand-set path is cleared.
    pub downloaded: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tools {
    pub mpv: Option<PathBuf>,
    pub ffmpeg: Option<PathBuf>,
}

/// Where to look for tools; injectable for tests.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub path_dirs: Vec<PathBuf>,
    pub extra_dirs: Vec<PathBuf>,
    /// Exhibit's own downloads (`crate::download`); looked in before anywhere else.
    pub tools_dir: Option<PathBuf>,
}

/// Exhibit's tools folder: `<EXHIBIT_DATA_DIR>\tools` when that is set (dev, tests), else
/// `%LOCALAPPDATA%\exhibit\tools`. Not roaming `%APPDATA%`: the tools are large and per machine.
/// The uninstaller and a clean install remove it (`crates/exhibit-app/installer-hooks.nsh`).
pub fn default_tools_dir() -> PathBuf {
    if std::env::var_os("EXHIBIT_DATA_DIR").is_some() {
        return crate::db::data_dir().join("tools");
    }
    match std::env::var_os("LOCALAPPDATA") {
        Some(l) => PathBuf::from(l).join("exhibit").join("tools"),
        None => crate::db::data_dir().join("tools"),
    }
}

impl Env {
    pub fn system() -> Env {
        let path_dirs = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        let var =
            |name: &str, rest: &str| std::env::var_os(name).map(|v| PathBuf::from(v).join(rest));
        let extra_dirs = [
            var("USERPROFILE", r"scoop\shims"),
            var("ProgramData", r"chocolatey\bin"),
            var("LOCALAPPDATA", r"Microsoft\WinGet\Links"),
            var("ProgramFiles", "mpv"),
            var("ProgramFiles", "MPV Player"),
            var("LOCALAPPDATA", r"Programs\mpv"),
            var("ProgramFiles", r"ffmpeg\bin"),
            Some(PathBuf::from(r"C:\ffmpeg\bin")),
        ]
        .into_iter()
        .flatten()
        .collect();
        Env {
            path_dirs,
            extra_dirs,
            tools_dir: Some(default_tools_dir()),
        }
    }
}

fn find_in(dir: &Path, tool: Tool) -> Option<PathBuf> {
    tool.file_names()
        .into_iter()
        .map(|n| dir.join(n))
        .find(|p| p.is_file())
}

struct Located {
    tool: Tool,
    path: Option<PathBuf>,
    source: Option<ToolSource>,
    release: Option<String>,
    downloaded: bool,
}

/// One entry per tool, in enum order. A manual path is returned even when the file is gone.
fn locate(c: &Connection, env: &Env) -> Result<Vec<Located>> {
    let download = env.tools_dir.as_deref().and_then(crate::download::newest);
    let mut found = Vec::new();
    for tool in ALL {
        let in_download = download
            .as_ref()
            .and_then(|d| Some((find_in(&d.dir, tool)?, d.release.clone())));
        let downloaded = in_download.is_some();
        let (path, source, release) = if let Some(manual) = get_setting(c, &tool.setting_key())? {
            (Some(PathBuf::from(manual)), Some(ToolSource::Manual), None)
        } else if let Some((path, release)) = in_download {
            (Some(path), Some(ToolSource::Downloaded), Some(release))
        } else {
            let path = env
                .path_dirs
                .iter()
                .chain(&env.extra_dirs)
                .find_map(|d| find_in(d, tool));
            let source = path.as_ref().map(|_| ToolSource::Detected);
            (path, source, None)
        };
        found.push(Located {
            tool,
            path,
            source,
            release,
            downloaded,
        });
    }
    // Second pass: look beside the tools that were found.
    let dirs: Vec<PathBuf> = found
        .iter()
        .filter_map(|l| l.path.as_ref()?.parent().map(Path::to_path_buf))
        .collect();
    for l in &mut found {
        if l.source.is_none() {
            l.path = dirs.iter().find_map(|d| find_in(d, l.tool));
            l.source = l.path.as_ref().map(|_| ToolSource::Detected);
        }
    }
    Ok(found)
}

/// Always one entry per tool, in enum order.
pub fn detect(c: &Connection, env: &Env) -> Result<Vec<ToolStatus>> {
    Ok(locate(c, env)?
        .into_iter()
        .map(|l| {
            let version = l.path.as_deref().and_then(|p| version_of(l.tool, p));
            ToolStatus {
                tool: l.tool,
                path: l.path,
                version,
                source: l.source,
                release: l.release,
                downloaded: l.downloaded,
            }
        })
        .collect())
}

/// `None` clears the manual path.
pub fn set_tool_path(c: &Connection, tool: Tool, path: Option<&Path>) -> Result<()> {
    let value = path.map(|p| p.to_string_lossy().into_owned());
    set_setting(c, &tool.setting_key(), value.as_deref())
}

/// Usable paths only: a manual path that no longer exists is left out.
pub fn tools(c: &Connection, env: &Env) -> Result<Tools> {
    let mut t = Tools::default();
    for l in locate(c, env)? {
        let path = l.path.filter(|p| p.is_file());
        match l.tool {
            Tool::Mpv => t.mpv = path,
            Tool::Ffmpeg => t.ffmpeg = path,
        }
    }
    Ok(t)
}

/// First line of the tool's version output. None when the file is missing or empty,
/// the tool fails, or it outlives the timeout (the child is killed).
fn version_of(tool: Tool, path: &Path) -> Option<String> {
    if std::fs::metadata(path).ok()?.len() == 0 {
        return None;
    }
    let mut cmd = Command::new(path);
    cmd.arg(tool.version_flag());
    let out = run_with_timeout(&mut cmd, VERSION_TIMEOUT, 64 * 1024)?;
    if !out.success {
        return None;
    }
    short_version(&String::from_utf8_lossy(&out.stdout))
}

/// The first line of version output, cut before ` Copyright` and trimmed; `None` if empty.
fn short_version(output: &str) -> Option<String> {
    let line = output.lines().next()?;
    let line = line.split(" Copyright").next().unwrap_or(line).trim();
    (!line.is_empty()).then(|| line.to_string())
}

/// Starts `cmd` without a console window on Windows, so a windowed app never flashes one.
/// Every tool process goes through it except mpv for playback, which must show its window.
pub fn quiet(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::os::windows::process::CommandExt::creation_flags(cmd, CREATE_NO_WINDOW);
    }
    cmd
}

pub(crate) struct Captured {
    pub success: bool,
    pub stdout: Vec<u8>,
}

/// Run `cmd` with no stdin, no console window and stderr discarded; capture at most
/// `limit` bytes of stdout. `None` if it cannot start or outlives `timeout` (the child is
/// killed).
pub(crate) fn run_with_timeout(
    cmd: &mut Command,
    timeout: Duration,
    limit: u64,
) -> Option<Captured> {
    quiet(cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = (&mut stdout).take(limit).read_to_end(&mut out);
        // Keep draining so a chatty child never blocks on a full pipe.
        let _ = std::io::copy(&mut stdout, &mut std::io::sink());
        out
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let stdout = reader.join().ok()?;
    Some(Captured {
        success: status?.success(),
        stdout,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn exe(name: &str) -> String {
        if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_string()
        }
    }

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(exe(name));
        std::fs::write(&p, b"").unwrap();
        p
    }

    fn conn() -> (TempDir, Connection) {
        let d = TempDir::new().unwrap();
        let c = crate::db::open(d.path()).unwrap();
        (d, c)
    }

    fn env(path: &[&Path], extra: &[&Path]) -> Env {
        Env {
            path_dirs: path.iter().map(|p| p.to_path_buf()).collect(),
            extra_dirs: extra.iter().map(|p| p.to_path_buf()).collect(),
            tools_dir: None,
        }
    }

    fn paths(s: &[ToolStatus]) -> Vec<Option<PathBuf>> {
        s.iter().map(|t| t.path.clone()).collect()
    }

    fn sources(s: &[ToolStatus]) -> Vec<Option<ToolSource>> {
        s.iter().map(|t| t.source).collect()
    }

    #[test]
    fn found_on_path_dirs() {
        let (_d, c) = conn();
        let dir = TempDir::new().unwrap();
        let mpv = touch(dir.path(), "mpv");
        let s = detect(&c, &env(&[dir.path()], &[])).unwrap();
        assert_eq!(paths(&s), vec![Some(mpv), None]);
        assert_eq!(sources(&s), vec![Some(ToolSource::Detected), None]);
        assert_eq!(s[0].version, None); // zero-byte file: no version lookup
    }

    #[test]
    fn found_in_extra_dirs() {
        let (_d, c) = conn();
        let dir = TempDir::new().unwrap();
        let ffmpeg = touch(dir.path(), "ffmpeg");
        let s = detect(&c, &env(&[], &[dir.path()])).unwrap();
        assert_eq!(paths(&s), vec![None, Some(ffmpeg)]);
    }

    #[test]
    fn sibling_discovery() {
        let (_d, c) = conn();
        let dir = TempDir::new().unwrap();
        let mpv = touch(dir.path(), "mpv");
        let ffmpeg = touch(dir.path(), "ffmpeg");
        // Only mpv is known (set by hand); ffmpeg is found beside it.
        set_tool_path(&c, Tool::Mpv, Some(&mpv)).unwrap();
        let s = detect(&c, &Env::default()).unwrap();
        assert_eq!(paths(&s), vec![Some(mpv), Some(ffmpeg)]);
        assert_eq!(
            sources(&s),
            vec![Some(ToolSource::Manual), Some(ToolSource::Detected)]
        );
    }

    #[test]
    fn manual_path_wins() {
        let (_d, c) = conn();
        let auto = TempDir::new().unwrap();
        let manual = TempDir::new().unwrap();
        touch(auto.path(), "mpv");
        let chosen = touch(manual.path(), "mpv");
        let e = env(&[auto.path()], &[]);
        set_tool_path(&c, Tool::Mpv, Some(&chosen)).unwrap();
        let s = detect(&c, &e).unwrap();
        assert_eq!(s[0].path, Some(chosen.clone()));
        assert_eq!(s[0].source, Some(ToolSource::Manual));
        assert_eq!(tools(&c, &e).unwrap().mpv, Some(chosen));

        set_tool_path(&c, Tool::Mpv, None).unwrap();
        let s = detect(&c, &e).unwrap();
        assert_eq!(s[0].source, Some(ToolSource::Detected));
        assert_eq!(s[0].path, Some(auto.path().join(exe("mpv"))));
    }

    #[test]
    fn newest_download_beats_detection_but_not_a_hand_set_path() {
        let (_d, c) = conn();
        let found = TempDir::new().unwrap();
        touch(found.path(), "mpv");
        touch(found.path(), "ffmpeg");
        let tools_dir = TempDir::new().unwrap();
        for release in ["20261001-x86_64", "20261111-x86_64-v3"] {
            let d = tools_dir.path().join(release);
            std::fs::create_dir_all(&d).unwrap();
            touch(&d, "mpv");
        }
        // Unfinished, and not a release: never used.
        let partial = tools_dir.path().join(".partial-20261231-x86_64");
        std::fs::create_dir_all(&partial).unwrap();
        touch(&partial, "mpv");
        let e = Env {
            tools_dir: Some(tools_dir.path().to_path_buf()),
            ..env(&[found.path()], &[])
        };
        let s = detect(&c, &e).unwrap();
        let newest = tools_dir.path().join("20261111-x86_64-v3").join(exe("mpv"));
        assert_eq!(s[0].path, Some(newest.clone()));
        assert_eq!(s[0].source, Some(ToolSource::Downloaded));
        assert_eq!(s[0].release.as_deref(), Some("20261111"));
        assert!(s[0].downloaded);
        // The download holds no ffmpeg, so detection supplies it.
        assert_eq!(s[1].source, Some(ToolSource::Detected));
        assert!(!s[1].downloaded);

        let mine = touch(found.path(), "mine");
        set_tool_path(&c, Tool::Mpv, Some(&mine)).unwrap();
        let s = detect(&c, &e).unwrap();
        assert_eq!(s[0].path, Some(mine));
        assert_eq!(s[0].source, Some(ToolSource::Manual));
        assert_eq!(s[0].release, None);
        // Clearing the path would hand over to the download.
        assert!(s[0].downloaded);
    }

    #[test]
    fn missing_manual_path_is_reported() {
        let (_d, c) = conn();
        let auto = TempDir::new().unwrap();
        touch(auto.path(), "mpv");
        let e = env(&[auto.path()], &[]);
        let gone = auto.path().join("nowhere").join("mpv.exe");
        set_tool_path(&c, Tool::Mpv, Some(&gone)).unwrap();
        let s = detect(&c, &e).unwrap();
        assert_eq!(s[0].path, Some(gone));
        assert_eq!(s[0].source, Some(ToolSource::Manual));
        assert_eq!(s[0].version, None);
        assert_eq!(tools(&c, &e).unwrap().mpv, None);
    }

    #[test]
    fn versions_are_cut_before_copyright() {
        assert_eq!(
            short_version(
                "mpv v0.42.0-93-g3d73d07e6 Copyright © 2000-2025 mpv/MPlayer/mplayer2 projects\n built on ..."
            )
            .as_deref(),
            Some("mpv v0.42.0-93-g3d73d07e6")
        );
        assert_eq!(
            short_version("ffmpeg version N-128915-g3d16b774d Copyright (c) 2000-2025 the FFmpeg developers\r\nbuilt with gcc")
                .as_deref(),
            Some("ffmpeg version N-128915-g3d16b774d")
        );
        assert_eq!(short_version("  tool 1.0  \n").as_deref(), Some("tool 1.0"));
        assert_eq!(short_version(" Copyright nobody"), None);
        assert_eq!(short_version(""), None);
    }

    #[cfg(windows)]
    #[test]
    fn detected_version_is_short() {
        let (_d, c) = conn();
        let dir = TempDir::new().unwrap();
        let bat = dir.path().join("mpv.bat");
        std::fs::write(
            &bat,
            "@echo mpv v0.42.0-93-g3d73d07e6 Copyright (C) 2000-2025 mpv/MPlayer/mplayer2 projects\r\n\
             @echo  built on Nov 1 2025\r\n",
        )
        .unwrap();
        set_tool_path(&c, Tool::Mpv, Some(&bat)).unwrap();
        let s = detect(&c, &Env::default()).unwrap();
        assert_eq!(s[0].version.as_deref(), Some("mpv v0.42.0-93-g3d73d07e6"));
    }

    #[test]
    fn nothing_found_gives_empty_entries() {
        let (_d, c) = conn();
        let s = detect(&c, &Env::default()).unwrap();
        let order: Vec<Tool> = s.iter().map(|t| t.tool).collect();
        assert_eq!(order, vec![Tool::Mpv, Tool::Ffmpeg]);
        assert!(
            s.iter()
                .all(|t| t.path.is_none() && t.source.is_none() && !t.downloaded)
        );
        assert_eq!(tools(&c, &Env::default()).unwrap(), Tools::default());
    }
}
