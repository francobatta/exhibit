//! Technical details of a video file (size, codec, HDR, tracks) read with mpv, and thumbnails
//! made with ffmpeg (or mpv without it). Every external process gets a 30 second timeout.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use rusqlite::Connection;
use serde_json::Value;

use crate::error::{Error, Result};
use crate::model::{AudioTrack, Id, SubTrack, TechInfo};
use crate::par;
use crate::scan::{join_rel, resolve, source_rows};
use crate::tools::{Tools, run_with_timeout};
use crate::volume::Volumes;

const TOOL_TIMEOUT: Duration = Duration::from_secs(30);

/// Tool processes run at once while probing or making thumbnails.
const TOOL_THREADS: usize = 4;

fn invalid(what: &str) -> Error {
    Error::Tool(format!("unreadable probe output: {what}").into())
}

fn lang_of(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

/// Whether a track's own name says `word` ("Forced", "English SDH"), as a whole word.
fn title_says(title: Option<&str>, word: &str) -> bool {
    title.is_some_and(|t| {
        t.split(|c: char| !c.is_alphanumeric())
            .any(|w| w.eq_ignore_ascii_case(word))
    })
}

/// Audio codecs whose profile names a richer format (DTS-HD, DTS:X, Atmos). mpv knows a
/// track's profile only once its decoder has opened, so the probe opens each of these.
pub const PROFILED_AUDIO: [&str; 3] = ["dts", "truehd", "eac3"];

/// The details from mpv's `track-list` and `video-params` properties. `profiles` holds the
/// profile of each audio track (by mpv's track id) read after opening its decoder; a track's own
/// `codec-profile` in the list is used when it is not there.
pub fn parse_mpv_properties(
    track_list: &Value,
    video_params: &Value,
    duration: f64,
    profiles: &HashMap<u64, String>,
) -> Result<TechInfo> {
    let tracks = track_list.as_array().map_or(&[][..], Vec::as_slice);
    let of_type = |t: &'static str| tracks.iter().filter(move |s| s["type"] == t);

    let video = of_type("video")
        .find(|t| t["albumart"] != true)
        .ok_or_else(|| invalid("no video track"))?;
    let dim = |param: &str, track: &str| {
        video_params[param]
            .as_u64()
            .or_else(|| video[track].as_u64())
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| invalid("no video size"))
    };
    // Dolby Vision is named by the video track itself; HDR10 and HLG by the picture's curve.
    let hdr = if video["dolby-vision-profile"].as_u64().is_some() {
        Some("Dolby Vision")
    } else {
        match video_params["gamma"].as_str() {
            Some("pq") => Some("HDR10"),
            Some("hlg") => Some("HLG"),
            _ => None,
        }
    };
    let flag = |t: &Value, name: &str| t[name].as_bool().unwrap_or(false);

    Ok(TechInfo {
        duration_s: duration,
        width: dim("w", "demux-w")?,
        height: dim("h", "demux-h")?,
        video_codec: video["codec"].as_str().unwrap_or_default().to_string(),
        hdr: hdr.map(str::to_string),
        audio: of_type("audio")
            .map(|t| {
                let title = lang_of(&t["title"]);
                let profile = t["id"]
                    .as_u64()
                    .and_then(|id| profiles.get(&id).cloned())
                    .or_else(|| lang_of(&t["codec-profile"]));
                AudioTrack {
                    codec: t["codec"].as_str().unwrap_or_default().to_string(),
                    channels: t["demux-channel-count"].as_u64().unwrap_or(0) as u32,
                    lang: lang_of(&t["lang"]),
                    profile,
                    default: flag(t, "default"),
                    original: flag(t, "original"),
                    commentary: flag(t, "commentary") || title_says(title.as_deref(), "commentary"),
                    title,
                }
            })
            .collect(),
        subtitles: of_type("sub")
            .map(|t| {
                let title = t["title"].as_str();
                SubTrack {
                    codec: t["codec"].as_str().unwrap_or_default().to_string(),
                    lang: lang_of(&t["lang"]),
                    forced: flag(t, "forced") || title_says(title, "forced"),
                    hearing_impaired: flag(t, "hearing-impaired") || title_says(title, "sdh"),
                }
            })
            .collect(),
    })
}

/// With mpv, else `Error::Tool`.
pub fn probe_file(tools: &Tools, file: &Path) -> Result<TechInfo> {
    match &tools.mpv {
        Some(mpv) => mpv_probe::probe(mpv, file),
        None => Err(Error::Tool("mpv is not installed".into())),
    }
}

/// Probe every file with `probed = 0` on an online source. Each file still on disk is marked
/// probed whether it worked or not, so a bad file is not retried on every run. `on` is called
/// after each attempt with the file id and, when it failed, why. Returns (ok, failed). Without
/// mpv it fails with `Error::Tool` before touching any row, so the files stay unprobed until
/// mpv is installed.
pub fn probe_pending(
    c: &Connection,
    v: &dyn Volumes,
    tools: &Tools,
    on: &mut dyn FnMut(Id, Option<&str>),
) -> Result<(u32, u32)> {
    if tools.mpv.is_none() {
        return Err(Error::Tool("mpv is not installed".into()));
    }
    let (mut ok, mut failed) = (0, 0);
    for source in source_rows(c)? {
        let Some(root) = resolve(v, &source).filter(|p| p.is_dir()) else {
            continue;
        };
        let files: Vec<(Id, String)> = c
            .prepare_cached(
                "SELECT id, rel_path FROM files WHERE source_id = ?1 AND probed = 0 ORDER BY id",
            )?
            .query_map([source.id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        // A few files are read at once; each mpv mostly waits on the disk.
        let read = |(_, rel): &(Id, String)| {
            let path = join_rel(&root, rel);
            let tech = probe_file(tools, &path).and_then(|t| {
                serde_json::to_string(&t).map_err(|e| Error::Tool(format!("{e}").into()))
            });
            let on_disk = tech.is_ok() || path.exists();
            (tech, on_disk)
        };
        par::each(&files, TOOL_THREADS, read, |(id, _), (tech, on_disk)| {
            match tech {
                Ok(_) => ok += 1,
                Err(_) => failed += 1,
            }
            // A file gone from disk is the scanner's business; it is not marked probed. A file
            // read again whose new read fails keeps the details it had.
            if on_disk {
                c.prepare_cached(
                    "UPDATE files SET tech = coalesce(?1, tech), probed = 1 WHERE id = ?2",
                )?
                .execute(rusqlite::params![tech.as_ref().ok(), id])?;
            }
            on(*id, tech.err().map(|e| e.to_string()).as_deref());
            Ok::<_, Error>(())
        })?;
    }
    Ok((ok, failed))
}

/// Share of the duration at which an item's thumbnail frame is taken.
const THUMBNAIL_AT: f64 = 0.10;

/// A thumbnail for every `other` item that has no backdrop, taken from its first file that
/// is on an online source and has a probed duration: one frame at 10% of the duration,
/// saved in `cache_dir` as `thumb-<item id>.jpg` and stored as the item's backdrop. `on`
/// gets each item tried. A failure leaves the backdrop empty, so a later call tries again.
/// Returns (made, failed); `Error::Tool` with neither ffmpeg nor mpv.
pub fn thumbnails_pending(
    c: &Connection,
    v: &dyn Volumes,
    tools: &Tools,
    cache_dir: &Path,
    on: &mut dyn FnMut(Id),
) -> Result<(u32, u32)> {
    if tools.ffmpeg.is_none() && tools.mpv.is_none() {
        return Err(Error::Tool("neither ffmpeg nor mpv is installed".into()));
    }
    let roots: HashMap<Id, PathBuf> = source_rows(c)?
        .iter()
        .filter_map(|s| Some((s.id, resolve(v, s).filter(|p| p.is_dir())?)))
        .collect();
    let files: Vec<(Id, Id, String, String)> = c
        .prepare_cached(
            "SELECT i.id, f.source_id, f.rel_path, f.tech FROM items i \
             JOIN files f ON f.item_id = i.id \
             WHERE i.kind = 'other' AND i.backdrop IS NULL AND f.tech IS NOT NULL \
             ORDER BY i.id, f.id",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<_>>()?;
    // Each item's first file that is online and has a duration: (item, file, duration).
    let mut todo: Vec<(Id, PathBuf, f64)> = Vec::new();
    for (item, source, rel, tech) in files {
        if todo.last().is_some_and(|t| t.0 == item) {
            continue;
        }
        let Some(root) = roots.get(&source) else {
            continue;
        };
        let Some(duration) = serde_json::from_str::<TechInfo>(&tech)
            .ok()
            .map(|t| t.duration_s)
            .filter(|d| *d > 0.0)
        else {
            continue;
        };
        todo.push((item, join_rel(root, &rel), duration));
    }
    let (mut made, mut failed) = (0, 0);
    let take = |(item, file, duration): &(Id, PathBuf, f64)| {
        let dest = cache_dir.join(format!("thumb-{item}.jpg"));
        thumbnail(tools, file, duration * THUMBNAIL_AT, &dest)
    };
    par::each(&todo, TOOL_THREADS, take, |&(item, _, _), done| {
        match done {
            Ok(()) => {
                c.prepare_cached("UPDATE items SET backdrop = ?2 WHERE id = ?1")?
                    .execute(rusqlite::params![item, format!("thumb-{item}.jpg")])?;
                made += 1;
            }
            Err(_) => failed += 1,
        }
        on(item);
        Ok::<_, Error>(())
    })?;
    Ok((made, failed))
}

/// One frame at `at_s` seconds, scaled to 640 pixels wide, written to `dest` (jpg or png).
/// ffmpeg if installed, else mpv.
pub fn thumbnail(tools: &Tools, file: &Path, at_s: f64, dest: &Path) -> Result<()> {
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let result = make_thumbnail(tools, file, at_s, dest);
    if result.is_err() {
        // A killed or failed tool may leave a partial image behind.
        let _ = std::fs::remove_file(dest);
    }
    result
}

fn make_thumbnail(tools: &Tools, file: &Path, at_s: f64, dest: &Path) -> Result<()> {
    if let Some(ffmpeg) = &tools.ffmpeg {
        let mut cmd = Command::new(ffmpeg);
        cmd.args(["-v", "error", "-y", "-ss", &at_s.to_string(), "-i"])
            .arg(file)
            .args(["-frames:v", "1", "-vf", "scale=640:-2"])
            .arg(dest);
        let done = run_with_timeout(&mut cmd, TOOL_TIMEOUT, 0).is_some_and(|o| o.success);
        if done && dest.is_file() {
            Ok(())
        } else {
            Err(Error::Tool(
                format!("ffmpeg made no thumbnail for {}", file.display()).into(),
            ))
        }
    } else if let Some(mpv) = &tools.mpv {
        mpv_thumbnail(mpv, file, at_s, dest)
    } else {
        Err(Error::Tool("neither ffmpeg nor mpv is installed".into()))
    }
}

/// mpv writes `00000001.<ext>` into a directory; do that beside `dest`, then move it.
fn mpv_thumbnail(mpv: &Path, file: &Path, at_s: f64, dest: &Path) -> Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ext = match dest.extension().and_then(|e| e.to_str()) {
        Some("png") => "png",
        _ => "jpg",
    };
    let dir: PathBuf = dest.with_file_name(format!(
        ".thumb-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir)?;
    let mut cmd = Command::new(mpv);
    cmd.args([
        "--no-config",
        "--no-audio",
        "--no-sub",
        "--frames=1",
        "--vo=image",
        "--vf=scale=640:-2",
    ])
    .arg(format!("--start={at_s}"))
    .arg(format!("--vo-image-format={ext}"))
    .arg(format!("--vo-image-outdir={}", dir.display()))
    .arg(file);
    let done = run_with_timeout(&mut cmd, TOOL_TIMEOUT, 0).is_some_and(|o| o.success);
    let made = dir.join(format!("00000001.{ext}"));
    let result = if done && made.is_file() {
        std::fs::rename(&made, dest).map_err(Error::from)
    } else {
        Err(Error::Tool(
            format!("mpv made no thumbnail for {}", file.display()).into(),
        ))
    };
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[cfg(windows)]
mod mpv_probe {
    use super::*;
    use crate::mpv_ipc;
    use serde_json::json;
    use std::io::Write;
    use std::process::Stdio;
    use std::time::Instant;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Start mpv paused with no output, read the properties over its pipe, then quit it.
    /// The pipe session runs on its own thread so a stuck mpv can be killed from here,
    /// which closes the pipe and ends that thread.
    pub fn probe(mpv: &Path, file: &Path) -> Result<TechInfo> {
        let pipe = format!(
            r"\\.\pipe\exhibit-probe-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let mut cmd = Command::new(mpv);
        cmd.args(["--no-config", "--pause", "--vo=null", "--ao=null"])
            .arg(format!("--input-ipc-server={pipe}"))
            .arg(file)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        crate::tools::quiet(&mut cmd);
        let mut child = cmd.spawn()?;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(query(&pipe));
        });
        let result = rx
            .recv_timeout(TOOL_TIMEOUT)
            .unwrap_or_else(|_| Err(Error::Tool("mpv timed out".into())));
        let _ = child.kill();
        let _ = child.wait();
        result
    }

    /// How long one audio track's decoder may take to open, and all of them together.
    const TRACK_OPEN: Duration = Duration::from_millis(1500);
    const PROFILES_BUDGET: Duration = Duration::from_secs(6);
    /// How long a profile may follow the decoder's opening.
    const PROFILE_AFTER_OPEN: Duration = Duration::from_millis(200);

    fn query(pipe: &str) -> Result<TechInfo> {
        let mut s = mpv_ipc::connect(pipe, Duration::from_secs(10))?;
        // Properties are unavailable until the file has loaded.
        let deadline = Instant::now() + Duration::from_secs(20);
        let duration = mpv_ipc::get_when_ready(&mut s, "duration", deadline)?
            .as_f64()
            .ok_or_else(|| invalid("duration"))?;
        let tracks = mpv_ipc::get_when_ready(&mut s, "track-list", deadline)?;
        let params = mpv_ipc::get_when_ready(&mut s, "video-params", deadline)?;
        let profiles = audio_profiles(&mut s, &tracks);
        let _ = writeln!(s, r#"{{"command":["quit"]}}"#);
        parse_mpv_properties(&tracks, &params, duration, &profiles)
    }

    /// Opens, one at a time, the decoder of each audio track whose codec may name a richer
    /// format, and reads the profile it reports. A track whose decoder opens without a profile
    /// is plain (DTS core, E-AC-3 without Atmos). Tracks left when the budget runs out stay
    /// unknown; the probe still succeeds.
    fn audio_profiles(s: &mut std::fs::File, tracks: &Value) -> HashMap<u64, String> {
        let mut found = HashMap::new();
        let budget = Instant::now() + PROFILES_BUDGET;
        let ids: Vec<u64> = tracks
            .as_array()
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .filter(|t| t["type"] == "audio" && t["codec-profile"].is_null())
            .filter(|t| PROFILED_AUDIO.contains(&t["codec"].as_str().unwrap_or_default()))
            .filter_map(|t| t["id"].as_u64())
            .collect();
        for id in ids {
            if Instant::now() >= budget || mpv_ipc::set_property(s, "aid", json!(id)).is_err() {
                break;
            }
            let mut until = (Instant::now() + TRACK_OPEN).min(budget);
            let mut opened = false;
            while Instant::now() < until {
                let Ok(list) = mpv_ipc::get_property(s, "track-list") else {
                    break;
                };
                let track = list
                    .as_array()
                    .and_then(|l| l.iter().find(|t| t["type"] == "audio" && t["id"] == id));
                if let Some(p) = track
                    .and_then(|t| t["codec-profile"].as_str())
                    .filter(|p| !p.is_empty())
                {
                    found.insert(id, p.to_string());
                    break;
                }
                // `codec-desc` appears as the decoder opens; the profile follows within a few
                // milliseconds, once it has read a frame, or never, for a plain track.
                if !opened && track.is_some_and(|t| !t["codec-desc"].is_null()) {
                    opened = true;
                    until = until.min(Instant::now() + PROFILE_AFTER_OPEN);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        found
    }
}

#[cfg(not(windows))]
mod mpv_probe {
    use super::*;

    pub fn probe(_mpv: &Path, _file: &Path) -> Result<TechInfo> {
        Err(Error::Tool(
            "mpv probing is not supported on this platform yet".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::add_source;
    use crate::volume::FakeVolumes;
    use serde_json::json;

    fn audio(codec: &str, channels: u32, lang: Option<&str>) -> AudioTrack {
        AudioTrack {
            codec: codec.into(),
            channels,
            lang: lang.map(Into::into),
            ..AudioTrack::default()
        }
    }

    fn sub(codec: &str, lang: Option<&str>) -> SubTrack {
        SubTrack {
            codec: codec.into(),
            lang: lang.map(Into::into),
            ..SubTrack::default()
        }
    }

    fn none() -> HashMap<u64, String> {
        HashMap::new()
    }

    #[test]
    fn mpv_properties() {
        let tracks = json!([
            {"type": "video", "codec": "hevc", "demux-w": 3840, "demux-h": 2160},
            {"type": "audio", "codec": "eac3", "demux-channel-count": 6, "lang": "eng"},
            {"type": "audio", "codec": "aac", "demux-channel-count": 2},
            {"type": "sub", "codec": "subrip", "lang": "fre"}
        ]);
        let params = json!({"w": 3840, "h": 2160, "gamma": "pq"});
        let t = parse_mpv_properties(&tracks, &params, 7384.5, &none()).unwrap();
        assert_eq!((t.width, t.height), (3840, 2160));
        assert_eq!(t.video_codec, "hevc");
        assert_eq!(t.hdr.as_deref(), Some("HDR10"));
        assert_eq!(t.duration_s, 7384.5);
        assert_eq!(
            t.audio,
            vec![audio("eac3", 6, Some("eng")), audio("aac", 2, None)]
        );
        assert_eq!(t.subtitles, vec![sub("subrip", Some("fre"))]);

        let hlg = json!({"w": 1920, "h": 1080, "gamma": "hlg"});
        let t = parse_mpv_properties(&tracks, &hlg, 1.0, &none()).unwrap();
        assert_eq!(t.hdr.as_deref(), Some("HLG"));
        let sdr = json!({"w": 1920, "h": 1080, "gamma": "bt.1886"});
        assert_eq!(
            parse_mpv_properties(&tracks, &sdr, 1.0, &none())
                .unwrap()
                .hdr,
            None
        );
        assert!(parse_mpv_properties(&json!([]), &sdr, 1.0, &none()).is_err());
    }

    #[test]
    fn richer_formats_and_track_flags() {
        let tracks = json!([
            {"type": "video", "id": 1, "codec": "hevc", "demux-w": 3840, "demux-h": 2160,
             "dolby-vision-profile": 8, "dolby-vision-level": 6},
            {"type": "audio", "id": 1, "codec": "ac3", "demux-channel-count": 6, "lang": "ru",
             "default": true, "codec-desc": "ATSC A/52A (AC-3)"},
            {"type": "audio", "id": 2, "codec": "eac3", "demux-channel-count": 6, "lang": "en",
             "title": "Original", "original": true},
            {"type": "audio", "id": 3, "codec": "dts", "demux-channel-count": 8, "lang": "en",
             "codec-profile": "DTS-HD MA + DTS:X"},
            {"type": "audio", "id": 4, "codec": "aac", "demux-channel-count": 2, "lang": "en",
             "title": "Director's Commentary"},
            {"type": "sub", "id": 1, "codec": "subrip", "lang": "en", "title": "Forced"},
            {"type": "sub", "id": 2, "codec": "subrip", "lang": "en", "title": "English SDH"},
            {"type": "sub", "id": 3, "codec": "subrip", "lang": "en", "forced": true,
             "hearing-impaired": true}
        ]);
        let params = json!({"w": 3840, "h": 2160, "gamma": "pq"});
        let profiles = HashMap::from([(2, "Dolby Digital Plus + Dolby Atmos".to_string())]);
        let t = parse_mpv_properties(&tracks, &params, 1.0, &profiles).unwrap();
        // Dolby Vision is named over the HDR10 curve it carries.
        assert_eq!(t.hdr.as_deref(), Some("Dolby Vision"));
        let a = &t.audio;
        assert!(a[0].default && a[0].profile.is_none());
        // A profile read by opening the decoder, and one already in the list.
        assert_eq!(
            a[1].profile.as_deref(),
            Some("Dolby Digital Plus + Dolby Atmos")
        );
        assert!(a[1].original && a[1].title.as_deref() == Some("Original"));
        assert_eq!(a[2].profile.as_deref(), Some("DTS-HD MA + DTS:X"));
        assert!(a[3].commentary && !a[2].commentary);
        let flags: Vec<_> = t
            .subtitles
            .iter()
            .map(|s| (s.forced, s.hearing_impaired))
            .collect();
        assert_eq!(flags, [(true, false), (false, true), (true, true)]);
    }

    #[test]
    fn records_from_before_the_new_fields_still_read() {
        let old = r#"{"duration_s":1.0,"width":1920,"height":1080,"video_codec":"h264","hdr":null,
            "audio":[{"codec":"dts","channels":6,"lang":"eng"}],"subtitles":[{"codec":"subrip","lang":"eng"}]}"#;
        let t: TechInfo = serde_json::from_str(old).unwrap();
        assert_eq!(t.audio, vec![audio("dts", 6, Some("eng"))]);
        assert_eq!(t.subtitles, vec![sub("subrip", Some("eng"))]);
    }

    #[test]
    fn no_tools_is_a_tool_error() {
        let e = probe_file(&Tools::default(), Path::new("x.mkv")).unwrap_err();
        assert!(matches!(e, Error::Tool(_)), "{e}");
        let e = thumbnail(
            &Tools::default(),
            Path::new("x.mkv"),
            1.0,
            Path::new("t.jpg"),
        );
        assert!(matches!(e, Err(Error::Tool(_))));
    }

    struct Lib {
        _data: tempfile::TempDir,
        drive: tempfile::TempDir,
        c: Connection,
        v: FakeVolumes,
    }

    /// One online source with two video files, none probed.
    fn lib() -> Lib {
        let data = tempfile::tempdir().unwrap();
        let c = crate::db::open(data.path()).unwrap();
        let drive = tempfile::tempdir().unwrap();
        let v = FakeVolumes(vec![("VOL1".into(), drive.path().to_path_buf())]);
        std::fs::create_dir_all(drive.path().join("media")).unwrap();
        let src = add_source(&c, &v, &drive.path().join("media")).unwrap();
        for name in ["a.mkv", "b.mkv"] {
            std::fs::write(drive.path().join("media").join(name), b"x").unwrap();
            c.execute(
                "INSERT INTO files (source_id, rel_path) VALUES (?1, ?2)",
                rusqlite::params![src.id, name],
            )
            .unwrap();
        }
        Lib {
            _data: data,
            drive,
            c,
            v,
        }
    }

    fn state(c: &Connection) -> Vec<(i64, bool)> {
        let mut s = c
            .prepare("SELECT probed, tech IS NOT NULL FROM files ORDER BY id")
            .unwrap();
        s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn offline_source_is_left_alone() {
        let l = lib();
        let none = FakeVolumes(vec![]);
        let tools = Tools {
            mpv: Some(PathBuf::from("mpv")),
            ..Tools::default()
        };
        assert_eq!(
            probe_pending(&l.c, &none, &tools, &mut |_, _| {}).unwrap(),
            (0, 0)
        );
        assert_eq!(state(&l.c), vec![(0, false), (0, false)]);
    }

    /// A tool that cannot start, so every probe fails.
    fn broken_tool(l: &Lib) -> Tools {
        Tools {
            mpv: Some(l.drive.path().join("no-such-mpv.exe")),
            ..Tools::default()
        }
    }

    #[test]
    fn missing_source_folder_is_left_alone() {
        let l = lib();
        std::fs::remove_dir_all(l.drive.path().join("media")).unwrap();
        assert_eq!(
            probe_pending(&l.c, &l.v, &broken_tool(&l), &mut |_, _| {}).unwrap(),
            (0, 0)
        );
        assert_eq!(state(&l.c), vec![(0, false), (0, false)]);
    }

    #[test]
    fn vanished_file_stays_unprobed() {
        let l = lib();
        std::fs::remove_file(l.drive.path().join("media").join("a.mkv")).unwrap();
        let mut reasons = Vec::new();
        let on = &mut |id, why: Option<&str>| reasons.push((id, why.map(str::to_string)));
        assert_eq!(
            probe_pending(&l.c, &l.v, &broken_tool(&l), on).unwrap(),
            (0, 2)
        );
        assert_eq!(state(&l.c), vec![(0, false), (1, false)]);
        // Each failure reaches `on` with its reason.
        assert_eq!(reasons.len(), 2);
        for (_, why) in &reasons {
            let why = why.as_deref().unwrap_or_default();
            assert!(!why.is_empty());
        }
    }

    #[test]
    fn a_failed_second_read_keeps_the_details() {
        let l = lib();
        l.c.execute("UPDATE files SET tech = '{\"old\":1}'", [])
            .unwrap();
        assert_eq!(
            probe_pending(&l.c, &l.v, &broken_tool(&l), &mut |_, _| {}).unwrap(),
            (0, 2)
        );
        assert_eq!(state(&l.c), vec![(1, true), (1, true)]);
    }

    /// Gives `file` (by name) an `other` item, and a probed duration when `duration` is set.
    fn other_item(c: &Connection, file: &str, duration: Option<f64>) -> Id {
        c.execute(
            "INSERT INTO items(kind, title, match_state, added_at) VALUES ('other', ?1, 'unmatched', 0)",
            [file],
        )
        .unwrap();
        let id = c.last_insert_rowid();
        let tech = duration.map(|d| {
            serde_json::to_string(&TechInfo {
                duration_s: d,
                width: 1920,
                height: 1080,
                video_codec: "h264".into(),
                hdr: None,
                audio: vec![],
                subtitles: vec![],
            })
            .unwrap()
        });
        c.execute(
            "UPDATE files SET item_id = ?1, tech = ?2, probed = 1 WHERE rel_path = ?3",
            rusqlite::params![id, tech, file],
        )
        .unwrap();
        id
    }

    fn backdrop(c: &Connection, id: Id) -> Option<String> {
        c.query_row("SELECT backdrop FROM items WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .unwrap()
    }

    #[cfg(windows)]
    #[test]
    fn thumbnails_fill_backdrop_for_other_items() {
        let l = lib();
        let media = l.drive.path().join("media");
        std::fs::write(media.join("bad.mkv"), b"x").unwrap();
        let src: Id =
            l.c.query_row("SELECT id FROM sources", [], |r| r.get(0))
                .unwrap();
        l.c.execute(
            "INSERT INTO files (source_id, rel_path) VALUES (?1, 'bad.mkv')",
            [src],
        )
        .unwrap();
        let a = other_item(&l.c, "a.mkv", Some(1000.0));
        let bad = other_item(&l.c, "bad.mkv", Some(50.0));
        // A movie, and an item that already has a backdrop, are left alone.
        let b = other_item(&l.c, "b.mkv", Some(10.0));
        l.c.execute("UPDATE items SET kind = 'movie' WHERE id = ?1", [b])
            .unwrap();
        // A fake ffmpeg: fails on bad.mkv, otherwise writes its last argument and logs the
        // seek position.
        let bat = l.drive.path().join("ffmpeg.bat");
        let log = l.drive.path().join("seeks.txt");
        std::fs::write(
            &bat,
            format!(
                "@echo off\r\n\
                 for %%a in (%*) do if /i \"%%~nxa\"==\"bad.mkv\" exit /b 1\r\n\
                 >> \"{}\" echo %5\r\n\
                 for %%a in (%*) do set \"last=%%~a\"\r\n\
                 echo jpg> \"%last%\"\r\n",
                log.display()
            ),
        )
        .unwrap();
        let tools = Tools {
            ffmpeg: Some(bat),
            ..Tools::default()
        };
        let cache = l.drive.path().join("cache");
        let mut seen = Vec::new();
        let r = thumbnails_pending(&l.c, &l.v, &tools, &cache, &mut |id| seen.push(id)).unwrap();
        assert_eq!(r, (1, 1));
        let name = format!("thumb-{a}.jpg");
        assert_eq!(backdrop(&l.c, a), Some(name.clone()));
        assert!(cache.join(&name).is_file());
        assert_eq!(backdrop(&l.c, bad), None);
        assert_eq!(backdrop(&l.c, b), None);
        // Made side by side: each item tried once, in any order.
        seen.sort();
        let mut tried = vec![a, bad];
        tried.sort();
        assert_eq!(seen, tried);
        // One frame at 10% of the duration.
        assert_eq!(std::fs::read_to_string(&log).unwrap().trim(), "100");
        // Done items are not redone; the failure is tried again on the next call.
        let r = thumbnails_pending(&l.c, &l.v, &tools, &cache, &mut |_| {}).unwrap();
        assert_eq!(r, (0, 1));
    }

    #[test]
    fn thumbnails_skip_offline_and_unprobed() {
        let l = lib();
        let unprobed = other_item(&l.c, "a.mkv", None);
        let zero = other_item(&l.c, "b.mkv", Some(0.0));
        let tools = Tools {
            ffmpeg: Some(l.drive.path().join("never-run.exe")),
            ..Tools::default()
        };
        let cache = l.drive.path().join("cache");
        let none = &mut |_| panic!("nothing is attempted");
        assert_eq!(
            thumbnails_pending(&l.c, &l.v, &tools, &cache, none).unwrap(),
            (0, 0)
        );
        // Probed, but its drive is gone.
        let offline = FakeVolumes(vec![]);
        let tech = serde_json::to_string(&TechInfo {
            duration_s: 60.0,
            width: 1,
            height: 1,
            video_codec: String::new(),
            hdr: None,
            audio: vec![],
            subtitles: vec![],
        })
        .unwrap();
        l.c.execute("UPDATE files SET tech = ?1", [tech]).unwrap();
        assert_eq!(
            thumbnails_pending(&l.c, &offline, &tools, &cache, none).unwrap(),
            (0, 0)
        );
        assert_eq!(
            (backdrop(&l.c, unprobed), backdrop(&l.c, zero)),
            (None, None)
        );
        // No tool at all is an error.
        assert!(matches!(
            thumbnails_pending(&l.c, &l.v, &Tools::default(), &cache, &mut |_| {}),
            Err(Error::Tool(_))
        ));
    }

    #[test]
    fn no_tool_is_an_error_and_leaves_files_unprobed() {
        let l = lib();
        let r = probe_pending(&l.c, &l.v, &Tools::default(), &mut |_, _| {});
        assert!(matches!(r, Err(Error::Tool(_))));
        assert_eq!(state(&l.c), vec![(0, false), (0, false)]);
    }
}
