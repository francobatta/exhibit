//! Technical details of a video file (size, codec, HDR, tracks) read with ffprobe when it
//! is installed, otherwise with mpv. Every external process gets a 30 second timeout.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use rusqlite::Connection;
use serde_json::Value;

use crate::error::{Error, Result};
use crate::model::{AudioTrack, Id, SubTrack, TechInfo};
use crate::scan::{resolve, source_rows};
use crate::tools::{Tools, run_with_timeout};
use crate::volume::Volumes;

const TOOL_TIMEOUT: Duration = Duration::from_secs(30);
/// ffprobe output for a film with many tracks is a few hundred KB at most.
const OUTPUT_LIMIT: u64 = 16 * 1024 * 1024;

fn invalid(what: &str) -> Error {
    Error::Tool(format!("unreadable probe output: {what}"))
}

fn lang_of(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

/// Parse `ffprobe -print_format json -show_format -show_streams` output.
pub fn parse_ffprobe(json: &str) -> Result<TechInfo> {
    let root: Value = serde_json::from_str(json).map_err(|_| invalid("not JSON"))?;
    let streams = root["streams"].as_array().map_or(&[][..], Vec::as_slice);
    let of_type = |t: &'static str| streams.iter().filter(move |s| s["codec_type"] == t);

    let video = of_type("video")
        .find(|s| s["disposition"]["attached_pic"] != 1)
        .ok_or_else(|| invalid("no video stream"))?;
    let dim = |k: &str| {
        video[k]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| invalid("no video size"))
    };
    let duration_s = root["format"]["duration"]
        .as_str()
        .and_then(|d| d.parse::<f64>().ok())
        .or_else(|| video["duration"].as_str()?.parse().ok())
        .ok_or_else(|| invalid("no duration"))?;

    let dovi = of_type("video").any(|s| {
        s["side_data_list"].as_array().is_some_and(|l| {
            l.iter().any(|d| {
                d["side_data_type"]
                    .as_str()
                    .is_some_and(|t| t.contains("DOVI"))
            })
        })
    });
    let hdr = if dovi {
        Some("Dolby Vision")
    } else {
        match video["color_transfer"].as_str() {
            Some("smpte2084") => Some("HDR10"),
            Some("arib-std-b67") => Some("HLG"),
            _ => None,
        }
    };

    Ok(TechInfo {
        duration_s,
        width: dim("width")?,
        height: dim("height")?,
        video_codec: video["codec_name"].as_str().unwrap_or_default().to_string(),
        hdr: hdr.map(str::to_string),
        audio: of_type("audio")
            .map(|s| AudioTrack {
                codec: s["codec_name"].as_str().unwrap_or_default().to_string(),
                channels: s["channels"].as_u64().unwrap_or(0) as u32,
                lang: lang_of(&s["tags"]["language"]),
            })
            .collect(),
        subtitles: of_type("subtitle")
            .map(|s| SubTrack {
                codec: s["codec_name"].as_str().unwrap_or_default().to_string(),
                lang: lang_of(&s["tags"]["language"]),
            })
            .collect(),
    })
}

/// Build the same details from mpv's `track-list` and `video-params` properties.
pub fn parse_mpv_properties(
    track_list: &Value,
    video_params: &Value,
    duration: f64,
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
    let hdr = match video_params["gamma"].as_str() {
        Some("pq") => Some("HDR10"),
        Some("hlg") => Some("HLG"),
        _ => None,
    };

    Ok(TechInfo {
        duration_s: duration,
        width: dim("w", "demux-w")?,
        height: dim("h", "demux-h")?,
        video_codec: video["codec"].as_str().unwrap_or_default().to_string(),
        hdr: hdr.map(str::to_string),
        audio: of_type("audio")
            .map(|t| AudioTrack {
                codec: t["codec"].as_str().unwrap_or_default().to_string(),
                channels: t["demux-channel-count"].as_u64().unwrap_or(0) as u32,
                lang: lang_of(&t["lang"]),
            })
            .collect(),
        subtitles: of_type("sub")
            .map(|t| SubTrack {
                codec: t["codec"].as_str().unwrap_or_default().to_string(),
                lang: lang_of(&t["lang"]),
            })
            .collect(),
    })
}

/// ffprobe if it is installed, else mpv, else `Error::Tool`.
pub fn probe_file(tools: &Tools, file: &Path) -> Result<TechInfo> {
    if let Some(ffprobe) = &tools.ffprobe {
        let mut cmd = Command::new(ffprobe);
        cmd.args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(file);
        let out = run_with_timeout(&mut cmd, TOOL_TIMEOUT, OUTPUT_LIMIT)
            .filter(|o| o.success)
            .ok_or_else(|| Error::Tool(format!("ffprobe failed on {}", file.display())))?;
        parse_ffprobe(&String::from_utf8_lossy(&out.stdout))
    } else if let Some(mpv) = &tools.mpv {
        mpv_probe::probe(mpv, file)
    } else {
        Err(Error::Tool("neither ffprobe nor mpv is installed".into()))
    }
}

/// Probe every file with `probed = 0` on an online source. Each file still on disk is marked
/// probed whether it worked or not, so a bad file is not retried on every run. `on` is called
/// after each attempt with the file id and, when it failed, why. Returns (ok, failed). With
/// neither ffprobe nor mpv
/// available it fails with `Error::Tool` before touching any row, so the files stay
/// unprobed until a tool is installed.
pub fn probe_pending(
    c: &Connection,
    v: &dyn Volumes,
    tools: &Tools,
    on: &mut dyn FnMut(Id, Option<&str>),
) -> Result<(u32, u32)> {
    if tools.ffprobe.is_none() && tools.mpv.is_none() {
        return Err(Error::Tool("neither ffprobe nor mpv is installed".into()));
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
        for (id, rel) in files {
            let path = rel.split('/').fold(root.clone(), |p, s| p.join(s));
            let tech = probe_file(tools, &path)
                .and_then(|t| serde_json::to_string(&t).map_err(|e| Error::Tool(format!("{e}"))));
            match tech {
                Ok(_) => ok += 1,
                Err(_) => failed += 1,
            }
            // A file gone from disk is the scanner's business; it is not marked probed.
            if tech.is_ok() || path.exists() {
                c.prepare_cached("UPDATE files SET tech = ?1, probed = 1 WHERE id = ?2")?
                    .execute(rusqlite::params![tech.as_ref().ok(), id])?;
            }
            on(id, tech.err().map(|e| e.to_string()).as_deref());
        }
    }
    Ok((ok, failed))
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
            Err(Error::Tool(format!(
                "ffmpeg made no thumbnail for {}",
                file.display()
            )))
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
        Err(Error::Tool(format!(
            "mpv made no thumbnail for {}",
            file.display()
        )))
    };
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[cfg(windows)]
mod mpv_probe {
    use super::*;
    use crate::mpv_ipc;
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
        std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000);
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

    fn query(pipe: &str) -> Result<TechInfo> {
        let mut s = mpv_ipc::connect(pipe, Duration::from_secs(10))?;
        // Properties are unavailable until the file has loaded.
        let deadline = Instant::now() + Duration::from_secs(20);
        let duration = mpv_ipc::get_when_ready(&mut s, "duration", deadline)?
            .as_f64()
            .ok_or_else(|| invalid("duration"))?;
        let tracks = mpv_ipc::get_when_ready(&mut s, "track-list", deadline)?;
        let params = mpv_ipc::get_when_ready(&mut s, "video-params", deadline)?;
        let _ = writeln!(s, r#"{{"command":["quit"]}}"#);
        parse_mpv_properties(&tracks, &params, duration)
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

    fn fixture(name: &str) -> String {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/ffprobe")
            .join(name);
        std::fs::read_to_string(p).unwrap()
    }

    fn audio(codec: &str, channels: u32, lang: Option<&str>) -> AudioTrack {
        AudioTrack {
            codec: codec.into(),
            channels,
            lang: lang.map(Into::into),
        }
    }

    fn sub(codec: &str, lang: Option<&str>) -> SubTrack {
        SubTrack {
            codec: codec.into(),
            lang: lang.map(Into::into),
        }
    }

    #[test]
    fn sdr_1080p() {
        let t = parse_ffprobe(&fixture("sdr_1080p.json")).unwrap();
        assert_eq!((t.width, t.height), (1920, 1080));
        assert_eq!(t.video_codec, "h264");
        assert_eq!(t.hdr, None);
        assert_eq!(t.duration_s, 5400.25);
        assert_eq!(
            t.audio,
            vec![audio("aac", 2, Some("jpn")), audio("ac3", 6, Some("eng"))]
        );
        assert_eq!(
            t.subtitles,
            vec![sub("subrip", Some("eng")), sub("ass", None)]
        );
    }

    #[test]
    fn hdr10_2160p() {
        let t = parse_ffprobe(&fixture("hdr10_2160p.json")).unwrap();
        assert_eq!((t.width, t.height), (3840, 2160));
        assert_eq!(t.video_codec, "hevc");
        assert_eq!(t.hdr.as_deref(), Some("HDR10"));
        assert_eq!(t.audio, vec![audio("eac3", 8, Some("eng"))]);
        assert_eq!(t.subtitles, vec![sub("hdmv_pgs_subtitle", Some("fre"))]);
    }

    #[test]
    fn dolby_vision_beats_hdr10() {
        let t = parse_ffprobe(&fixture("dolby_vision.json")).unwrap();
        assert_eq!(t.hdr.as_deref(), Some("Dolby Vision"));
        assert_eq!(t.audio, vec![audio("truehd", 8, Some("eng"))]);
        assert_eq!(t.subtitles, vec![sub("subrip", Some("spa"))]);
    }

    #[test]
    fn hlg() {
        let t = parse_ffprobe(&fixture("hlg.json")).unwrap();
        assert_eq!(t.hdr.as_deref(), Some("HLG"));
        assert_eq!(t.video_codec, "av1");
        assert_eq!(t.audio, vec![audio("opus", 2, None)]);
        assert!(t.subtitles.is_empty());
    }

    #[test]
    fn bad_json_is_an_error() {
        assert!(parse_ffprobe("{}").is_err());
        assert!(parse_ffprobe("not json").is_err());
        // Audio only: no video stream.
        let audio_only = r#"{"streams":[{"codec_type":"audio","codec_name":"mp3","channels":2}],"format":{"duration":"10"}}"#;
        assert!(parse_ffprobe(audio_only).is_err());
    }

    #[test]
    fn cover_art_is_not_the_video() {
        let j = r#"{"streams":[
            {"codec_type":"video","codec_name":"mjpeg","width":500,"height":500,"disposition":{"attached_pic":1}},
            {"codec_type":"video","codec_name":"h264","width":1280,"height":720}],
            "format":{"duration":"1"}}"#;
        let t = parse_ffprobe(j).unwrap();
        assert_eq!((t.video_codec.as_str(), t.width), ("h264", 1280));
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
        let t = parse_mpv_properties(&tracks, &params, 7384.5).unwrap();
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
        let t = parse_mpv_properties(&tracks, &hlg, 1.0).unwrap();
        assert_eq!(t.hdr.as_deref(), Some("HLG"));
        let sdr = json!({"w": 1920, "h": 1080, "gamma": "bt.1886"});
        assert_eq!(parse_mpv_properties(&tracks, &sdr, 1.0).unwrap().hdr, None);
        assert!(parse_mpv_properties(&json!([]), &sdr, 1.0).is_err());
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

    #[cfg(windows)]
    #[test]
    fn failing_tool_marks_probed_and_continues() {
        let l = lib();
        let bat = l.drive.path().join("fail.bat");
        std::fs::write(&bat, "@exit /b 1\r\n").unwrap();
        let tools = Tools {
            ffprobe: Some(bat),
            ..Tools::default()
        };
        let mut seen = Vec::new();
        let r = probe_pending(&l.c, &l.v, &tools, &mut |id, _| seen.push(id)).unwrap();
        assert_eq!(r, (0, 2));
        assert_eq!(state(&l.c), vec![(1, false), (1, false)]);
        assert_eq!(seen.len(), 2);
        // Nothing left to do.
        assert_eq!(
            probe_pending(&l.c, &l.v, &tools, &mut |_, _| {}).unwrap(),
            (0, 0)
        );
    }

    #[cfg(windows)]
    #[test]
    fn working_tool_stores_tech() {
        let l = lib();
        let json = l.drive.path().join("out.json");
        std::fs::write(&json, fixture("hdr10_2160p.json")).unwrap();
        let bat = l.drive.path().join("ok.bat");
        std::fs::write(&bat, format!("@type \"{}\"\r\n", json.display())).unwrap();
        let tools = Tools {
            ffprobe: Some(bat),
            ..Tools::default()
        };
        assert_eq!(
            probe_pending(&l.c, &l.v, &tools, &mut |_, _| {}).unwrap(),
            (2, 0)
        );
        let tech: String =
            l.c.query_row("SELECT tech FROM files LIMIT 1", [], |r| r.get(0))
                .unwrap();
        let t: TechInfo = serde_json::from_str(&tech).unwrap();
        assert_eq!(t.hdr.as_deref(), Some("HDR10"));
        assert_eq!(state(&l.c), vec![(1, true), (1, true)]);
    }

    #[test]
    fn offline_source_is_left_alone() {
        let l = lib();
        let none = FakeVolumes(vec![]);
        let tools = Tools {
            ffprobe: Some(PathBuf::from("ffprobe")),
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
            ffprobe: Some(l.drive.path().join("no-such-ffprobe.exe")),
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
            assert!(why.contains("ffprobe failed"), "{why}");
        }
    }

    #[test]
    fn no_tool_is_an_error_and_leaves_files_unprobed() {
        let l = lib();
        let r = probe_pending(&l.c, &l.v, &Tools::default(), &mut |_, _| {});
        assert!(matches!(r, Err(Error::Tool(_))));
        assert_eq!(state(&l.c), vec![(0, false), (0, false)]);
    }
}
