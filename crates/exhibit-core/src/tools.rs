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
    Ffprobe,
    Ffmpeg,
}

const ALL: [Tool; 3] = [Tool::Mpv, Tool::Ffprobe, Tool::Ffmpeg];

impl Tool {
    fn name(self) -> &'static str {
        match self {
            Tool::Mpv => "mpv",
            Tool::Ffprobe => "ffprobe",
            Tool::Ffmpeg => "ffmpeg",
        }
    }

    fn setting_key(self) -> String {
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolStatus {
    pub tool: Tool,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
    pub manual: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tools {
    pub mpv: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
    pub ffmpeg: Option<PathBuf>,
}

/// Where to look for tools; injectable for tests.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub path_dirs: Vec<PathBuf>,
    pub extra_dirs: Vec<PathBuf>,
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
        }
    }
}

fn find_in(dir: &Path, tool: Tool) -> Option<PathBuf> {
    tool.file_names()
        .into_iter()
        .map(|n| dir.join(n))
        .find(|p| p.is_file())
}

/// (tool, path, manual) in enum order. A manual path is returned even when the file is gone.
fn locate(c: &Connection, env: &Env) -> Result<Vec<(Tool, Option<PathBuf>, bool)>> {
    let mut found = Vec::new();
    for tool in ALL {
        if let Some(manual) = get_setting(c, &tool.setting_key())? {
            found.push((tool, Some(PathBuf::from(manual)), true));
            continue;
        }
        let path = env
            .path_dirs
            .iter()
            .chain(&env.extra_dirs)
            .find_map(|d| find_in(d, tool));
        found.push((tool, path, false));
    }
    // Second pass: look beside the tools that were found.
    let dirs: Vec<PathBuf> = found
        .iter()
        .filter_map(|(_, p, _)| p.as_ref()?.parent().map(Path::to_path_buf))
        .collect();
    for (tool, path, manual) in &mut found {
        if path.is_none() && !*manual {
            *path = dirs.iter().find_map(|d| find_in(d, *tool));
        }
    }
    Ok(found)
}

/// Always three entries, in enum order.
pub fn detect(c: &Connection, env: &Env) -> Result<Vec<ToolStatus>> {
    Ok(locate(c, env)?
        .into_iter()
        .map(|(tool, path, manual)| {
            let version = path.as_deref().and_then(|p| version_of(tool, p));
            ToolStatus {
                tool,
                path,
                version,
                manual,
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
    for (tool, path, _) in locate(c, env)? {
        let path = path.filter(|p| p.is_file());
        match tool {
            Tool::Mpv => t.mpv = path,
            Tool::Ffprobe => t.ffprobe = path,
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
    cmd.arg(tool.version_flag())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000); // CREATE_NO_WINDOW
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        let _ = (&mut stdout).take(64 * 1024).read_to_string(&mut out);
        out
    });
    let deadline = Instant::now() + VERSION_TIMEOUT;
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
    let out = reader.join().ok()?;
    if !status?.success() {
        return None;
    }
    out.lines()
        .next()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
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
        }
    }

    fn paths(s: &[ToolStatus]) -> Vec<Option<PathBuf>> {
        s.iter().map(|t| t.path.clone()).collect()
    }

    #[test]
    fn found_on_path_dirs() {
        let (_d, c) = conn();
        let dir = TempDir::new().unwrap();
        let mpv = touch(dir.path(), "mpv");
        let s = detect(&c, &env(&[dir.path()], &[])).unwrap();
        assert_eq!(paths(&s), vec![Some(mpv), None, None]);
        assert!(!s[0].manual);
        assert_eq!(s[0].version, None); // zero-byte file: no version lookup
    }

    #[test]
    fn found_in_extra_dirs() {
        let (_d, c) = conn();
        let dir = TempDir::new().unwrap();
        let ffprobe = touch(dir.path(), "ffprobe");
        let s = detect(&c, &env(&[], &[dir.path()])).unwrap();
        assert_eq!(paths(&s), vec![None, Some(ffprobe), None]);
    }

    #[test]
    fn sibling_discovery() {
        let (_d, c) = conn();
        let dir = TempDir::new().unwrap();
        let mpv = touch(dir.path(), "mpv");
        let ffprobe = touch(dir.path(), "ffprobe");
        // Only mpv is known (set by hand); ffprobe is found beside it.
        set_tool_path(&c, Tool::Mpv, Some(&mpv)).unwrap();
        let s = detect(&c, &Env::default()).unwrap();
        assert_eq!(paths(&s), vec![Some(mpv), Some(ffprobe), None]);
        assert!(s[0].manual && !s[1].manual);
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
        assert!(s[0].manual);
        assert_eq!(tools(&c, &e).unwrap().mpv, Some(chosen));

        set_tool_path(&c, Tool::Mpv, None).unwrap();
        let s = detect(&c, &e).unwrap();
        assert!(!s[0].manual);
        assert_eq!(s[0].path, Some(auto.path().join(exe("mpv"))));
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
        assert!(s[0].manual);
        assert_eq!(s[0].version, None);
        assert_eq!(tools(&c, &e).unwrap().mpv, None);
    }

    #[test]
    fn nothing_found_gives_three_empty_entries() {
        let (_d, c) = conn();
        let s = detect(&c, &Env::default()).unwrap();
        let order: Vec<Tool> = s.iter().map(|t| t.tool).collect();
        assert_eq!(order, vec![Tool::Mpv, Tool::Ffprobe, Tool::Ffmpeg]);
        assert!(s.iter().all(|t| t.path.is_none() && !t.manual));
        assert_eq!(tools(&c, &Env::default()).unwrap(), Tools::default());
    }
}
