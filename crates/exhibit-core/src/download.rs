//! Downloads mpv and ffmpeg together from shinchiro's builds (the first Windows build mpv.io
//! lists), on the owner's request only. Each download is unpacked into its own folder in the
//! tools folder, named `<release>-<arch>` (`20261111-x86_64-v3`); the newest one is the copy
//! in use (`tools::locate`), and older ones are removed. Asset names change with every release,
//! so they are matched by pattern.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// shinchiro's latest release, as GitHub describes it.
pub const LATEST: &str =
    "https://api.github.com/repos/shinchiro/mpv-winbuild-cmake/releases/latest";

/// What is unfinished starts with this, and is never used.
const PARTIAL: &str = ".partial-";

/// The network, as a download needs it; injectable for tests.
pub trait Fetch {
    /// GET `url` as JSON.
    fn json(&self, url: &str) -> Result<Value>;
    /// GET `url` into `out`, calling `on` with the bytes written so far.
    fn copy(&self, url: &str, out: &mut dyn Write, on: &mut dyn FnMut(u64)) -> Result<()>;
}

pub struct UreqFetch {
    agent: ureq::Agent,
}

impl UreqFetch {
    pub fn new() -> UreqFetch {
        let agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .user_agent("Exhibit")
            .build()
            .into();
        UreqFetch { agent }
    }

    fn get(&self, url: &str) -> Result<ureq::http::Response<ureq::Body>> {
        let r = self.agent.get(url).call().map_err(|e| {
            Error::Provider(format!(
                "could not reach GitHub ({e}); check the internet connection, then try again"
            ))
        })?;
        match r.status().as_u16() {
            200 => Ok(r),
            403 | 429 => Err(Error::Provider(
                "GitHub is limiting downloads for now; try again in an hour".into(),
            )),
            s => Err(Error::Provider(format!(
                "GitHub answered {s}; try again later"
            ))),
        }
    }
}

impl Default for UreqFetch {
    fn default() -> Self {
        UreqFetch::new()
    }
}

impl Fetch for UreqFetch {
    fn json(&self, url: &str) -> Result<Value> {
        let mut r = self.get(url)?;
        let text = r.body_mut().read_to_string().map_err(|e| {
            Error::Provider(format!(
                "the download stopped ({e}); check the internet connection, then try again"
            ))
        })?;
        serde_json::from_str(&text).map_err(|_| {
            Error::Provider("GitHub sent something unreadable; try again later".into())
        })
    }

    fn copy(&self, url: &str, out: &mut dyn Write, on: &mut dyn FnMut(u64)) -> Result<()> {
        let mut r = self.get(url)?;
        let mut body = r.body_mut().as_reader();
        let mut buf = vec![0u8; 256 * 1024];
        let mut done = 0u64;
        loop {
            let n = body.read(&mut buf).map_err(|e| {
                Error::Provider(format!(
                    "the download stopped ({e}); check the internet connection, then try again"
                ))
            })?;
            if n == 0 {
                return Ok(());
            }
            out.write_all(&buf[..n])?;
            done += n as u64;
            on(done);
        }
    }
}

/// How a download is going.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum DownloadEvent {
    /// Asking GitHub for the latest release.
    Checking,
    /// Bytes of both archives so far, of their total.
    Downloading {
        done: u64,
        total: u64,
    },
    Unpacking,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DownloadReport {
    /// `YYYYMMDD`.
    pub release: String,
    pub arch: String,
    /// The newest download was already this release: nothing was fetched.
    pub already_latest: bool,
    pub dir: PathBuf,
}

/// The build this machine runs: `x86_64-v3` (CPUs with AVX2 and the rest of x86-64-v3),
/// `x86_64`, `aarch64` or `i686`. `None` off Windows: shinchiro builds only for Windows.
pub fn arch() -> Option<&'static str> {
    if !cfg!(windows) {
        return None;
    }
    #[cfg(target_arch = "x86_64")]
    {
        let v3 = std::arch::is_x86_feature_detected!("avx2")
            && std::arch::is_x86_feature_detected!("bmi1")
            && std::arch::is_x86_feature_detected!("bmi2")
            && std::arch::is_x86_feature_detected!("fma")
            && std::arch::is_x86_feature_detected!("f16c")
            && std::arch::is_x86_feature_detected!("lzcnt")
            && std::arch::is_x86_feature_detected!("movbe");
        return Some(if v3 { "x86_64-v3" } else { "x86_64" });
    }
    #[cfg(target_arch = "aarch64")]
    return Some("aarch64");
    #[cfg(target_arch = "x86")]
    return Some("i686");
    #[allow(unreachable_code)]
    None
}

#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// Lower-case hex SHA-256, when GitHub gives one.
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub release: String,
    pub mpv: Asset,
    pub ffmpeg: Asset,
}

/// The release's mpv and ffmpeg archives for `arch`. `mpv-dev-*` and `mpv-debug-*` never
/// match, and `x86_64` never matches `x86_64-v3`.
pub fn plan(release: &Value, arch: &str) -> Result<Plan> {
    let tag = release["tag_name"].as_str().unwrap_or_default();
    if !Regex::new(r"^\d{8}$").unwrap().is_match(tag) {
        return Err(Error::Provider(
            "the latest mpv release has an unexpected name; try again in a day or two".into(),
        ));
    }
    let a = regex::escape(arch);
    let pick = |pattern: String, what: &str| -> Result<Asset> {
        let re = Regex::new(&pattern).unwrap();
        release["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|x| x["name"].as_str().is_some_and(|n| re.is_match(n)))
            .and_then(|x| {
                Some(Asset {
                    name: x["name"].as_str()?.to_string(),
                    url: x["browser_download_url"].as_str()?.to_string(),
                    size: x["size"].as_u64()?,
                    sha256: x["digest"]
                        .as_str()
                        .and_then(|d| d.strip_prefix("sha256:"))
                        .map(str::to_ascii_lowercase),
                })
            })
            .ok_or_else(|| {
                Error::Provider(format!(
                    "the latest release has no {what} for this PC ({arch}); try again in a day or two"
                ))
            })
    };
    Ok(Plan {
        release: tag.to_string(),
        mpv: pick(format!(r"^mpv-{a}-\d{{8}}-git-[0-9a-f]+\.7z$"), "mpv")?,
        ffmpeg: pick(format!(r"^ffmpeg-{a}-git-[0-9a-f]+\.7z$"), "ffmpeg")?,
    })
}

/// A finished download folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Download {
    pub dir: PathBuf,
    /// `YYYYMMDD`.
    pub release: String,
}

/// Finished download folders in `tools_dir`, newest last.
fn downloads(tools_dir: &Path) -> Vec<Download> {
    let re = Regex::new(r"^(\d{8})-[a-z0-9_-]+$").unwrap();
    let mut found: Vec<(String, Download)> = std::fs::read_dir(tools_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let release = re.captures(&name)?[1].to_string();
            Some((
                name,
                Download {
                    dir: e.path(),
                    release,
                },
            ))
        })
        .collect();
    found.sort_by(|(a, x), (b, y)| x.release.cmp(&y.release).then(a.cmp(b)));
    found.into_iter().map(|(_, d)| d).collect()
}

/// The newest finished download, if any.
pub fn newest(tools_dir: &Path) -> Option<Download> {
    downloads(tools_dir).pop()
}

/// Removes every download but the newest, and anything unfinished. Best effort: a folder
/// still in use (mpv playing from it) stays until a later call.
pub fn tidy(tools_dir: &Path) {
    let mut all = downloads(tools_dir);
    all.pop();
    for old in all {
        let _ = std::fs::remove_dir_all(&old.dir);
    }
    for e in std::fs::read_dir(tools_dir).into_iter().flatten().flatten() {
        if e.file_name().to_string_lossy().starts_with(PARTIAL) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// Downloads the latest mpv and ffmpeg for this machine into `tools_dir`, unless the newest
/// download is already that release. A failure leaves any earlier download in use.
pub fn download(
    fetch: &dyn Fetch,
    tools_dir: &Path,
    on: &mut dyn FnMut(DownloadEvent),
) -> Result<DownloadReport> {
    let arch = arch().ok_or_else(|| {
        Error::Invalid("downloading mpv and ffmpeg is only offered on Windows".into())
    })?;
    download_for(fetch, tools_dir, arch, on)
}

pub(crate) fn download_for(
    fetch: &dyn Fetch,
    tools_dir: &Path,
    arch: &str,
    on: &mut dyn FnMut(DownloadEvent),
) -> Result<DownloadReport> {
    on(DownloadEvent::Checking);
    let plan = plan(&fetch.json(LATEST)?, arch)?;
    let name = format!("{}-{arch}", plan.release);
    let dir = tools_dir.join(&name);
    let report = |already_latest| DownloadReport {
        release: plan.release.clone(),
        arch: arch.to_string(),
        already_latest,
        dir: dir.clone(),
    };
    if newest(tools_dir).is_some_and(|d| d.dir == dir) && complete(&dir) {
        tidy(tools_dir);
        return Ok(report(true));
    }

    let staging = tools_dir.join(format!("{PARTIAL}{name}"));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    let result = fetch_and_unpack(fetch, &plan, &staging, on).and_then(|()| {
        // A leftover of the same name that was never complete gives way.
        let _ = std::fs::remove_dir_all(&dir);
        Ok(std::fs::rename(&staging, &dir)?)
    });
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result?;
    tidy(tools_dir);
    Ok(report(false))
}

fn complete(dir: &Path) -> bool {
    let exe = |n: &str| dir.join(format!("{n}.exe")).is_file();
    exe("mpv") && exe("ffmpeg")
}

fn fetch_and_unpack(
    fetch: &dyn Fetch,
    plan: &Plan,
    staging: &Path,
    on: &mut dyn FnMut(DownloadEvent),
) -> Result<()> {
    let total = plan.mpv.size + plan.ffmpeg.size;
    let mut before = 0;
    let mut archives = Vec::new();
    for asset in [&plan.mpv, &plan.ffmpeg] {
        let path = staging.join(&asset.name);
        fetch_one(fetch, asset, &path, &mut |n| {
            on(DownloadEvent::Downloading {
                done: before + n,
                total,
            })
        })?;
        before += asset.size;
        archives.push(path);
    }
    on(DownloadEvent::Unpacking);
    for archive in &archives {
        unpack(archive, staging)?;
        std::fs::remove_file(archive)?;
    }
    if !complete(staging) {
        return Err(Error::Provider(
            "the download did not hold mpv and ffmpeg; try again in a day or two".into(),
        ));
    }
    Ok(())
}

/// Hashes what it writes.
struct Hashed<W: Write> {
    inner: W,
    hash: Sha256,
    written: u64,
}

impl<W: Write> Write for Hashed<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hash.update(&buf[..n]);
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

fn fetch_one(fetch: &dyn Fetch, asset: &Asset, dest: &Path, on: &mut dyn FnMut(u64)) -> Result<()> {
    let file = std::fs::File::create(dest)?;
    let mut out = Hashed {
        inner: std::io::BufWriter::new(file),
        hash: Sha256::new(),
        written: 0,
    };
    fetch.copy(&asset.url, &mut out, on)?;
    out.flush()?;
    let damaged = || Error::Provider(format!("{} arrived damaged; try again", asset.name));
    if out.written != asset.size {
        return Err(damaged());
    }
    if let Some(want) = &asset.sha256 {
        let got: String = out
            .hash
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if &got != want {
            return Err(damaged());
        }
    }
    Ok(())
}

/// The files Exhibit runs: the archive's top-level programs and libraries, nothing in folders
/// (manuals, installers) and no scripts (shinchiro's updater).
fn wanted(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !lower.contains(['/', '\\']) && [".exe", ".com", ".dll"].iter().any(|e| lower.ends_with(e))
}

fn unpack(archive: &Path, into: &Path) -> Result<()> {
    let bad = |e: sevenz_rust2::Error| {
        Error::Provider(format!("could not unpack the download ({e}); try again"))
    };
    let mut reader =
        sevenz_rust2::ArchiveReader::open(archive, sevenz_rust2::Password::empty()).map_err(bad)?;
    reader
        .for_each_entries(|entry, data| {
            if !entry.is_directory && wanted(&entry.name) {
                let mut out = std::fs::File::create(into.join(&entry.name))?;
                std::io::copy(data, &mut out)?;
            }
            Ok(true)
        })
        .map_err(bad)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::RefCell;
    use std::collections::HashMap;

    fn release(tag: &str, names: &[&str]) -> Value {
        json!({
            "tag_name": tag,
            "assets": names.iter().map(|n| json!({
                "name": n,
                "browser_download_url": format!("https://example.test/{n}"),
                "size": 3,
                "digest": null,
            })).collect::<Vec<_>>()
        })
    }

    const NAMES: &[&str] = &[
        "ffmpeg-aarch64-git-cd4e96eb3.7z",
        "ffmpeg-x86_64-git-cd4e96eb3.7z",
        "ffmpeg-x86_64-v3-git-cd4e96eb3.7z",
        "mpv-debug-x86_64-v3-20261111-git-bcde6b93cc.7z",
        "mpv-dev-x86_64-v3-20261111-git-bcde6b93cc.7z",
        "mpv-x86_64-20261111-git-bcde6b93cc.7z",
        "mpv-x86_64-v3-20261111-git-bcde6b93cc.7z",
        "mpv-aarch64-20261111-git-bcde6b93cc.7z",
    ];

    #[test]
    fn plan_picks_the_build_for_the_machine() {
        let r = release("20261111", NAMES);
        let p = plan(&r, "x86_64-v3").unwrap();
        assert_eq!(p.release, "20261111");
        assert_eq!(p.mpv.name, "mpv-x86_64-v3-20261111-git-bcde6b93cc.7z");
        assert_eq!(p.ffmpeg.name, "ffmpeg-x86_64-v3-git-cd4e96eb3.7z");
        let p = plan(&r, "x86_64").unwrap();
        assert_eq!(p.mpv.name, "mpv-x86_64-20261111-git-bcde6b93cc.7z");
        assert_eq!(p.ffmpeg.name, "ffmpeg-x86_64-git-cd4e96eb3.7z");
        assert_eq!(
            plan(&r, "aarch64").unwrap().mpv.name,
            "mpv-aarch64-20261111-git-bcde6b93cc.7z"
        );
        // No i686 builds in this release: says what is missing.
        match plan(&r, "i686") {
            Err(Error::Provider(m)) => assert!(m.contains("i686"), "{m}"),
            other => panic!("{other:?}"),
        }
        assert!(plan(&release("latest", NAMES), "x86_64").is_err());
    }

    #[test]
    fn plan_reads_the_digest() {
        let mut r = release("20261111", NAMES);
        r["assets"][6]["digest"] = json!("sha256:ABCDEF");
        assert_eq!(
            plan(&r, "x86_64-v3").unwrap().mpv.sha256.as_deref(),
            Some("abcdef")
        );
    }

    #[test]
    fn only_top_level_programs_are_unpacked() {
        for yes in ["mpv.exe", "mpv.com", "d3dcompiler_43.dll", "ffmpeg.exe"] {
            assert!(wanted(yes), "{yes}");
        }
        for no in [
            "updater.bat",
            "mpv-register.bat",
            "doc/manual.pdf",
            "installer/mpv-install.bat",
            "mpv/fonts.conf",
            "installer\\x.exe",
        ] {
            assert!(!wanted(no), "{no}");
        }
    }

    #[test]
    fn newest_and_tidy() {
        let t = tempfile::tempdir().unwrap();
        for d in [
            "20261001-x86_64",
            "20261111-x86_64-v3",
            ".partial-20261212-x86_64",
            "unrelated",
        ] {
            std::fs::create_dir_all(t.path().join(d)).unwrap();
        }
        let n = newest(t.path()).unwrap();
        assert_eq!(n.release, "20261111");
        assert_eq!(n.dir, t.path().join("20261111-x86_64-v3"));
        tidy(t.path());
        let mut left: Vec<String> = std::fs::read_dir(t.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["20261111-x86_64-v3", "unrelated"]);
        assert_eq!(newest(&t.path().join("missing")), None);
    }

    /// GitHub as a map of URL to bytes; counts the files fetched.
    struct FakeFetch {
        release: Value,
        files: HashMap<String, Vec<u8>>,
        fetched: RefCell<u32>,
    }

    impl Fetch for FakeFetch {
        fn json(&self, _url: &str) -> Result<Value> {
            Ok(self.release.clone())
        }

        fn copy(&self, url: &str, out: &mut dyn Write, on: &mut dyn FnMut(u64)) -> Result<()> {
            *self.fetched.borrow_mut() += 1;
            let bytes = self
                .files
                .get(url)
                .ok_or_else(|| Error::Provider("404".into()))?;
            out.write_all(bytes)?;
            on(bytes.len() as u64);
            Ok(())
        }
    }

    #[test]
    fn a_damaged_file_fails_and_keeps_the_earlier_download() {
        let t = tempfile::tempdir().unwrap();
        let earlier = t.path().join("20261001-x86_64");
        std::fs::create_dir_all(&earlier).unwrap();
        std::fs::write(earlier.join("mpv.exe"), b"").unwrap();
        std::fs::write(earlier.join("ffmpeg.exe"), b"").unwrap();

        let r = release("20261111", NAMES);
        let p = plan(&r, "x86_64").unwrap();
        let fetch = FakeFetch {
            release: r,
            // One byte short of the stated size.
            files: HashMap::from([
                (p.mpv.url.clone(), b"ab".to_vec()),
                (p.ffmpeg.url.clone(), b"abc".to_vec()),
            ]),
            fetched: RefCell::new(0),
        };
        let mut events = Vec::new();
        let e = download_for(&fetch, t.path(), "x86_64", &mut |ev| events.push(ev)).unwrap_err();
        assert!(e.to_string().contains("damaged"), "{e}");
        assert_eq!(events.first(), Some(&DownloadEvent::Checking));
        assert_eq!(newest(t.path()).unwrap().dir, earlier);
        // Nothing unfinished is left behind.
        assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 1);
    }

    #[test]
    fn the_latest_release_already_here_is_not_fetched_again() {
        let t = tempfile::tempdir().unwrap();
        let have = t.path().join("20261111-x86_64");
        std::fs::create_dir_all(&have).unwrap();
        std::fs::write(have.join("mpv.exe"), b"").unwrap();
        std::fs::write(have.join("ffmpeg.exe"), b"").unwrap();
        let old = t.path().join("20261001-x86_64");
        std::fs::create_dir_all(&old).unwrap();
        let fetch = FakeFetch {
            release: release("20261111", NAMES),
            files: HashMap::new(),
            fetched: RefCell::new(0),
        };
        let r = download_for(&fetch, t.path(), "x86_64", &mut |_| {}).unwrap();
        assert!(r.already_latest);
        assert_eq!(r.release, "20261111");
        assert_eq!(*fetch.fetched.borrow(), 0);
        assert!(!old.exists());
    }

    /// Downloads the real latest release from GitHub, about 60 MB. Run by hand:
    /// `cargo test -p exhibit-core real_download -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_download() {
        let Some(arch) = arch() else { return };
        let t = tempfile::tempdir().unwrap();
        let mut last = None;
        let r = download_for(&UreqFetch::new(), t.path(), arch, &mut |e| last = Some(e)).unwrap();
        println!("{r:?} {last:?}");
        assert!(complete(&r.dir));
        let again = download_for(&UreqFetch::new(), t.path(), arch, &mut |_| {}).unwrap();
        assert!(again.already_latest);
    }
}
