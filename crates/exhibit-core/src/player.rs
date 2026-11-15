//! Playback through the owner's own mpv. Exhibit never plays media and never sends mpv a
//! playback command: it launches mpv with the owner's configuration untouched, observes
//! how far playback got over mpv's JSON control channel, and records it when mpv exits.

use std::cmp::Reverse;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::model::{Id, ItemKind, MediaFile, PlayState};
use crate::rows::{
    FILE_COLUMNS, PLAY_STATE_COLUMNS, enum_str, media_file_from_row, play_state_from_row,
};
use crate::tools::{Tool, Tools};
use crate::volume::Volumes;
use crate::{db, mpv_ipc, query, scan};

pub use crate::model::WATCHED_FRACTION;

/// Playback resumes only from a stored position past this many seconds.
const RESUME_MIN_S: f64 = 5.0;
/// How long mpv has to open its control channel after it starts.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

static LAUNCHES: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct PlayOutcome {
    pub item_id: Id,
    pub position_s: f64,
    pub duration_s: f64,
    pub watched: bool,
}

/// Reads mpv's JSON control channel until it closes. Returns the last known (position, duration).
/// The only commands sent are the two `observe_property` requests.
pub fn track<S: Read + Write>(mut stream: S) -> (f64, f64) {
    let (mut pos, mut dur) = (0.0, 0.0);
    for (id, name) in [(1, "time-pos"), (2, "duration")] {
        let cmd = json!({ "command": ["observe_property", id, name] });
        let sent = stream.write_all(format!("{cmd}\n").as_bytes());
        if sent.and_then(|_| stream.flush()).is_err() {
            return (pos, dur);
        }
    }
    let mut reader = BufReader::new(stream);
    let mut line = Vec::new();
    loop {
        line.clear();
        if !matches!(reader.read_until(b'\n', &mut line), Ok(n) if n > 0) {
            return (pos, dur);
        }
        let Ok(msg) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        match msg["event"].as_str() {
            Some("property-change") => match (msg["name"].as_str(), msg["data"].as_f64()) {
                (Some("time-pos"), Some(x)) => pos = x,
                (Some("duration"), Some(x)) => dur = x,
                _ => {}
            },
            Some("end-file") if msg["reason"] == "eof" => pos = dur,
            _ => {}
        }
    }
}

/// mpv with the owner's own configuration (never `--no-config`), full screen, its control
/// channel on `pipe`, starting at `start_s` when that is past five seconds.
pub fn build_command(mpv: &Path, file: &Path, start_s: Option<f64>, pipe: &str) -> Command {
    let mut cmd = Command::new(mpv);
    cmd.arg("--fs").arg(format!("--input-ipc-server={pipe}"));
    if let Some(s) = start_s.filter(|s| *s > RESUME_MIN_S) {
        cmd.arg(format!("--start={s}"));
    }
    cmd.arg(file);
    cmd
}

/// Resolves a Show or Season to its next episode, picks the item's best available file,
/// launches, tracks, records. Blocks until the player exits. A player whose file name does
/// not start with `mpv` gets only the file path and nothing is tracked or recorded.
pub fn play(
    c: &Connection,
    v: &dyn Volumes,
    tools: &Tools,
    item_id: Id,
    resume: bool,
) -> Result<PlayOutcome> {
    let Some(player) = &tools.mpv else {
        return Err(no_mpv(c)?);
    };
    let id = target(c, item_id)?;
    let file = choose_file(c, v, id)?;
    let start = start_position(c, id, resume)?;
    let previous = outcome(id, play_state(c, id)?);
    let cannot_start = |e| Error::Tool(format!("cannot start {}: {e}", player.display()));
    let is_mpv = player
        .file_name()
        .is_some_and(|n| n.to_string_lossy().to_lowercase().starts_with("mpv"));
    if !is_mpv {
        Command::new(player)
            .arg(&file)
            .status()
            .map_err(cannot_start)?;
        return Ok(previous);
    }
    let pipe = pipe_name();
    let started_at = scan::now();
    let mut child = build_command(player, &file, start, &pipe)
        .spawn()
        .map_err(cannot_start)?;
    let tracked = connect_while_running(&pipe, &mut child).map(track);
    child.wait()?;
    match tracked {
        Some((pos, dur)) if dur > 0.0 => record(c, id, started_at, pos, dur),
        _ => Ok(previous),
    }
}

/// Saves one play session: the resume state (watched at `WATCHED_FRACTION`, which resets the
/// position; an item already watched stays watched) and a `play_log` row ending now.
pub fn record(
    c: &Connection,
    item_id: Id,
    started_at: i64,
    position_s: f64,
    duration_s: f64,
) -> Result<PlayOutcome> {
    if !c
        .prepare_cached("SELECT 1 FROM items WHERE id = ?1")?
        .exists([item_id])?
    {
        return Err(Error::NotFound(format!("item {item_id}")));
    }
    let finished = duration_s > 0.0 && position_s / duration_s >= WATCHED_FRACTION;
    let now = scan::now();
    let tx = c.unchecked_transaction()?;
    let (stored, watched) = tx
        .prepare_cached(
            "INSERT INTO play_state(item_id, position_s, duration_s, watched, last_played_at) \
             VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(item_id) DO UPDATE SET \
                 position_s = CASE WHEN max(watched, excluded.watched) = 1 THEN 0 \
                     ELSE excluded.position_s END, \
                 duration_s = excluded.duration_s, \
                 watched = max(watched, excluded.watched), \
                 last_played_at = excluded.last_played_at \
             RETURNING position_s, watched",
        )?
        .query_row(
            params![
                item_id,
                if finished { 0.0 } else { position_s },
                duration_s,
                finished,
                now
            ],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
    tx.prepare_cached(
        "INSERT INTO play_log(item_id, started_at, ended_at, position_s) VALUES (?1, ?2, ?3, ?4)",
    )?
    .execute(params![item_id, started_at, now, position_s])?;
    tx.commit()?;
    Ok(PlayOutcome {
        item_id,
        position_s: stored,
        duration_s,
        watched,
    })
}

/// Unique per launch, so two players never share a control channel.
fn pipe_name() -> String {
    let n = LAUNCHES.fetch_add(1, Ordering::Relaxed);
    format!(r"\\.\pipe\exhibit-mpv-{}-{n}", std::process::id())
}

fn no_mpv(c: &Connection) -> Result<Error> {
    let key = Tool::Mpv.setting_key();
    Ok(Error::Tool(match db::get_setting(c, &key)? {
        Some(p) => format!("mpv not found at {p}"),
        None => format!(
            "mpv not found on PATH or in the usual install folders; \
             set its location in the `{key}` setting"
        ),
    }))
}

/// Opens mpv's control channel, retrying while mpv runs, for up to `CONNECT_TIMEOUT`.
fn connect_while_running(pipe: &str, child: &mut Child) -> Option<std::fs::File> {
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        if let Ok(f) = mpv_ipc::connect(pipe, Duration::ZERO) {
            return Some(f);
        }
        if Instant::now() >= deadline || !matches!(child.try_wait(), Ok(None)) {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The item to play: the item itself, or for a Show or Season its next unwatched episode
/// (the first episode when all are watched).
fn target(c: &Connection, id: Id) -> Result<Id> {
    let kind: String = c
        .prepare_cached("SELECT kind FROM items WHERE id = ?1")?
        .query_row([id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("item {id}")))?;
    if kind != enum_str(ItemKind::Show) && kind != enum_str(ItemKind::Season) {
        return Ok(id);
    }
    match query::next_episode(c, id)? {
        Some(next) => Ok(next),
        None => query::first_episode(c, id, false)?
            .ok_or_else(|| Error::NotFound(format!("no episode with a file under item {id}"))),
    }
}

/// Absolute path of the item's best file: on an online source, tallest picture, then largest.
fn choose_file(c: &Connection, v: &dyn Volumes, id: Id) -> Result<PathBuf> {
    let files: Vec<MediaFile> = c
        .prepare_cached(&format!(
            "SELECT {FILE_COLUMNS} FROM files WHERE item_id = ?1 ORDER BY id"
        ))?
        .query_map([id], media_file_from_row)?
        .collect::<rusqlite::Result<_>>()?;
    let sources = scan::list_sources(c, v)?;
    let source_of = |f: &MediaFile| sources.iter().find(|s| s.id == f.source_id);
    let best = files
        .iter()
        .filter_map(|f| Some((f, source_of(f).filter(|s| s.online)?)))
        .max_by_key(|(f, _)| {
            (
                f.tech.as_ref().map_or(0, |t| t.height),
                f.size,
                Reverse(f.id),
            )
        });
    if let Some((f, s)) = best {
        return Ok(f
            .rel_path
            .split('/')
            .filter(|seg| !seg.is_empty())
            .fold(s.path.clone(), |p, seg| p.join(seg)));
    }
    match files
        .iter()
        .find_map(|f| source_of(f).filter(|s| !s.online))
    {
        Some(s) => Err(Error::SourceOffline(s.path.display().to_string())),
        None => Err(Error::NotFound(format!("no playable file for item {id}"))),
    }
}

/// Where to start: the stored position of an unwatched item past five seconds, if resuming.
fn start_position(c: &Connection, id: Id, resume: bool) -> Result<Option<f64>> {
    if !resume {
        return Ok(None);
    }
    Ok(play_state(c, id)?
        .filter(|p| !p.watched && p.position_s > RESUME_MIN_S)
        .map(|p| p.position_s))
}

fn play_state(c: &Connection, id: Id) -> Result<Option<PlayState>> {
    Ok(c.prepare_cached(&format!(
        "SELECT {PLAY_STATE_COLUMNS} FROM play_state WHERE item_id = ?1"
    ))?
    .query_row([id], play_state_from_row)
    .optional()?)
}

fn outcome(item_id: Id, p: Option<PlayState>) -> PlayOutcome {
    PlayOutcome {
        item_id,
        position_s: p.as_ref().map_or(0.0, |p| p.position_s),
        duration_s: p.as_ref().map_or(0.0, |p| p.duration_s),
        watched: p.is_some_and(|p| p.watched),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::model::{ItemKind, MatchState, TechInfo};
    use crate::rows::enum_str;
    use crate::scan;
    use crate::volume::FakeVolumes;
    use rusqlite::params;
    use serde_json::Value;
    use std::io::Cursor;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// An mpv control channel that replays `input` and keeps everything written to it.
    struct Script {
        input: Cursor<Vec<u8>>,
        written: Vec<u8>,
    }

    fn script(lines: &[&str]) -> Script {
        let mut text = lines.join("\n");
        text.push('\n');
        Script {
            input: Cursor::new(text.into_bytes()),
            written: Vec::new(),
        }
    }

    impl Read for Script {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.input.read(buf)
        }
    }

    impl Write for Script {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.written.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn change(name: &str, data: &str) -> String {
        let id = if name == "time-pos" { 1 } else { 2 };
        format!(r#"{{"event":"property-change","id":{id},"name":"{name}","data":{data}}}"#)
    }

    #[test]
    fn track_keeps_latest_position() {
        let lines = [
            r#"{"error":"success","request_id":0}"#.to_string(),
            r#"{"error":"success","request_id":0}"#.to_string(),
            change("time-pos", "null"),
            change("duration", "6000"),
            change("time-pos", "10"),
            change("time-pos", "20"),
            change("time-pos", "3725.5"),
            change("time-pos", "null"),
            r#"{"event":"end-file","reason":"quit"}"#.to_string(),
        ];
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        assert_eq!(track(script(&lines)), (3725.5, 6000.0));
    }

    #[test]
    fn track_eof_event_means_finished() {
        let lines = [
            change("duration", "6000"),
            change("time-pos", "5990.2"),
            r#"{"event":"end-file","reason":"eof"}"#.to_string(),
        ];
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        assert_eq!(track(script(&lines)), (6000.0, 6000.0));
    }

    #[test]
    fn track_only_sends_observe_commands() {
        let lines = [change("duration", "100"), change("time-pos", "50")];
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        let mut s = script(&lines);
        track(&mut s);
        let written = String::from_utf8(s.written).unwrap();
        let mut observed = Vec::new();
        for line in written.lines() {
            let v: Value = serde_json::from_str(line).unwrap();
            let cmd = v["command"].as_array().unwrap();
            assert_eq!(cmd[0], "observe_property", "{line}");
            observed.push(cmd[2].as_str().unwrap().to_string());
        }
        observed.sort();
        assert_eq!(observed, ["duration", "time-pos"]);
    }

    #[test]
    fn track_ignores_malformed_lines() {
        let lines = [
            change("duration", "100"),
            "not json".to_string(),
            "{\"event\":".to_string(),
            r#"{"event":"property-change","name":"time-pos","data":"soon"}"#.to_string(),
            r#"{"event":"property-change","name":"time-pos"}"#.to_string(),
            "[1,2,3]".to_string(),
            change("time-pos", "42"),
            "\u{fffd}garbage".to_string(),
        ];
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        assert_eq!(track(script(&lines)), (42.0, 100.0));
    }

    #[test]
    fn track_of_silent_channel_is_zero() {
        assert_eq!(track(script(&[])), (0.0, 0.0));
    }

    fn args(cmd: &Command) -> Vec<String> {
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn command_has_no_no_config_and_resumes() {
        let file = Path::new(r"E:\Films\Heat (1995).mkv");
        let cmd = build_command(Path::new("mpv.exe"), file, Some(3725.5), "PIPE");
        assert_eq!(cmd.get_program(), "mpv.exe");
        assert_eq!(
            args(&cmd),
            [
                "--fs",
                "--input-ipc-server=PIPE",
                "--start=3725.5",
                r"E:\Films\Heat (1995).mkv"
            ]
        );
        assert!(!args(&cmd).iter().any(|a| a.contains("no-config")));
    }

    #[test]
    fn command_skips_start_below_five_seconds() {
        let file = Path::new("a.mkv");
        for start in [None, Some(0.0), Some(4.9), Some(5.0)] {
            let cmd = build_command(Path::new("mpv"), file, start, "P");
            assert_eq!(args(&cmd), ["--fs", "--input-ipc-server=P", "a.mkv"]);
        }
    }

    #[test]
    fn pipe_names_are_unique_per_launch() {
        let (a, b) = (pipe_name(), pipe_name());
        let prefix = format!(r"\\.\pipe\exhibit-mpv-{}-", std::process::id());
        assert!(a.starts_with(&prefix) && b.starts_with(&prefix), "{a}");
        assert_ne!(a, b);
    }

    struct Env {
        _data: TempDir,
        drive: TempDir,
        tools_dir: TempDir,
        c: Connection,
        v: FakeVolumes,
        src: Id,
    }

    fn env() -> Env {
        let data = tempfile::tempdir().unwrap();
        let drive = tempfile::tempdir().unwrap();
        let c = db::open(data.path()).unwrap();
        let v = FakeVolumes(vec![("VOL1".to_string(), drive.path().to_path_buf())]);
        let src = scan::add_source(&c, &v, drive.path()).unwrap().id;
        Env {
            _data: data,
            drive,
            tools_dir: tempfile::tempdir().unwrap(),
            c,
            v,
            src,
        }
    }

    impl Env {
        fn add(&self, kind: ItemKind, title: &str, parent: Option<Id>) -> Id {
            self.c
                .execute(
                    "INSERT INTO items(kind, title, parent_id, match_state) VALUES (?1, ?2, ?3, ?4)",
                    params![enum_str(kind), title, parent, enum_str(MatchState::Matched)],
                )
                .unwrap();
            self.c.last_insert_rowid()
        }

        fn file(&self, source: Id, item: Id, rel: &str, height: Option<u32>, size: i64) {
            let tech = height.map(|h| {
                serde_json::to_string(&TechInfo {
                    duration_s: 100.0,
                    width: h * 16 / 9,
                    height: h,
                    video_codec: "hevc".into(),
                    hdr: None,
                    audio: vec![],
                    subtitles: vec![],
                })
                .unwrap()
            });
            self.c
                .execute(
                    "INSERT INTO files(source_id, rel_path, item_id, tech, size) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![source, rel, item, tech, size],
                )
                .unwrap();
        }

        fn episode(&self, season: Id, s: i32, e: i32) -> Id {
            let id = self.add(ItemKind::Episode, &format!("S{s}E{e}"), Some(season));
            self.c
                .execute(
                    "UPDATE items SET season = ?2, episode = ?3 WHERE id = ?1",
                    params![id, s, e],
                )
                .unwrap();
            self.file(self.src, id, &format!("s{s}e{e}.mkv"), None, 1);
            id
        }

        /// A source whose drive is not mounted.
        fn offline_source(&self) -> Id {
            self.c
                .execute(
                    "INSERT INTO sources(path, volume_id, last_path) \
                     VALUES ('Films', 'GONE', 'X:\\Films')",
                    [],
                )
                .unwrap();
            self.c.last_insert_rowid()
        }

        fn player(&self, name: &str) -> Tools {
            let p = self.tools_dir.path().join(name);
            std::fs::write(&p, "@exit /b 0\r\n").unwrap();
            Tools {
                mpv: Some(p),
                ..Tools::default()
            }
        }

        fn state(&self, id: Id) -> Option<(f64, f64, bool)> {
            self.c
                .query_row(
                    "SELECT position_s, duration_s, watched FROM play_state WHERE item_id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .ok()
        }

        fn log(&self) -> Vec<(Id, i64, i64, f64)> {
            let mut s = self
                .c
                .prepare(
                    "SELECT item_id, started_at, ended_at, position_s FROM play_log ORDER BY id",
                )
                .unwrap();
            s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap()
        }
    }

    #[test]
    fn record_partial_sets_resume() {
        let e = env();
        let id = e.add(ItemKind::Movie, "Heat", None);
        let out = record(&e.c, id, 1_000, 1000.0, 6000.0).unwrap();
        assert_eq!(
            out,
            PlayOutcome {
                item_id: id,
                position_s: 1000.0,
                duration_s: 6000.0,
                watched: false
            }
        );
        assert_eq!(e.state(id), Some((1000.0, 6000.0, false)));
        let log = e.log();
        assert_eq!(log.len(), 1);
        assert_eq!((log[0].0, log[0].1, log[0].3), (id, 1_000, 1000.0));
        assert!(
            log[0].2 >= scan::now() - 5,
            "ended_at is the time record runs"
        );
    }

    #[test]
    fn record_ninety_percent_marks_watched_and_resets() {
        let e = env();
        let id = e.add(ItemKind::Movie, "Heat", None);
        let out = record(&e.c, id, 1, 5400.0, 6000.0).unwrap();
        assert!(out.watched);
        assert_eq!(out.position_s, 0.0);
        assert_eq!(e.state(id), Some((0.0, 6000.0, true)));
        assert_eq!(e.log()[0].3, 5400.0);
    }

    #[test]
    fn record_keeps_watched_on_later_partial_replay() {
        let e = env();
        let id = e.add(ItemKind::Movie, "Heat", None);
        crate::query::set_watched(&e.c, id, true).unwrap();
        let out = record(&e.c, id, 1, 300.0, 6000.0).unwrap();
        assert!(out.watched);
        assert!(e.state(id).unwrap().2);
        assert_eq!(e.log().len(), 1);
    }

    #[test]
    fn record_unknown_item_is_not_found() {
        let e = env();
        assert!(matches!(
            record(&e.c, 999, 1, 10.0, 100.0),
            Err(crate::error::Error::NotFound(_))
        ));
    }

    /// Show with one season of three episodes: (show, season, episodes).
    fn show(e: &Env) -> (Id, Id, Vec<Id>) {
        let show = e.add(ItemKind::Show, "Show", None);
        let season = e.add(ItemKind::Season, "Season 1", Some(show));
        let eps = (1..=3).map(|n| e.episode(season, 1, n)).collect();
        (show, season, eps)
    }

    #[test]
    fn show_and_season_resolve_to_an_episode() {
        let e = env();
        let (show, season, eps) = show(&e);
        assert_eq!(target(&e.c, show).unwrap(), eps[0]);
        crate::query::set_watched(&e.c, eps[0], true).unwrap();
        assert_eq!(target(&e.c, show).unwrap(), eps[1]);
        assert_eq!(target(&e.c, season).unwrap(), eps[1]);
        // Everything watched: start again from the first episode.
        crate::query::set_watched(&e.c, show, true).unwrap();
        assert_eq!(target(&e.c, show).unwrap(), eps[0]);
        assert_eq!(target(&e.c, season).unwrap(), eps[0]);
        assert_eq!(target(&e.c, eps[2]).unwrap(), eps[2]);
    }

    #[test]
    fn file_choice_prefers_online_then_height_then_size() {
        let e = env();
        let id = e.add(ItemKind::Movie, "Heat", None);
        let off = e.offline_source();
        e.file(off, id, "Heat 2160p.mkv", Some(2160), 90);
        e.file(e.src, id, "Heat 720p.mkv", Some(720), 80);
        e.file(e.src, id, "Heat 1080p small.mkv", Some(1080), 10);
        e.file(e.src, id, "Heat 1080p big.mkv", Some(1080), 20);
        e.file(e.src, id, "Heat unknown.mkv", None, 99);
        assert_eq!(
            choose_file(&e.c, &e.v, id).unwrap(),
            e.drive.path().join("Heat 1080p big.mkv")
        );
    }

    #[test]
    fn resume_position_only_for_unwatched_past_five_seconds() {
        let e = env();
        let id = e.add(ItemKind::Movie, "Heat", None);
        assert_eq!(start_position(&e.c, id, true).unwrap(), None);
        record(&e.c, id, 1, 4.0, 6000.0).unwrap();
        assert_eq!(start_position(&e.c, id, true).unwrap(), None);
        record(&e.c, id, 1, 1000.0, 6000.0).unwrap();
        assert_eq!(start_position(&e.c, id, true).unwrap(), Some(1000.0));
        assert_eq!(start_position(&e.c, id, false).unwrap(), None);
        crate::query::set_watched(&e.c, id, true).unwrap();
        assert_eq!(start_position(&e.c, id, true).unwrap(), None);
    }

    #[cfg(windows)]
    #[test]
    fn play_show_resolves_next_episode() {
        let e = env();
        let (show, _, eps) = show(&e);
        crate::query::set_watched(&e.c, eps[0], true).unwrap();
        let tools = e.player("mpv.bat");
        let out = play(&e.c, &e.v, &tools, show, true).unwrap();
        assert_eq!(
            out,
            PlayOutcome {
                item_id: eps[1],
                position_s: 0.0,
                duration_s: 0.0,
                watched: false
            }
        );
        // mpv exited before its pipe opened: nothing recorded.
        assert_eq!(e.state(eps[1]), None);
        assert!(e.log().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn play_without_tracking_keeps_previous_state() {
        let e = env();
        let id = e.add(ItemKind::Movie, "Heat", None);
        e.file(e.src, id, "Heat.mkv", None, 1);
        record(&e.c, id, 1, 1000.0, 6000.0).unwrap();
        let before = e.log();
        let out = play(&e.c, &e.v, &e.player("vlc.bat"), id, true).unwrap();
        assert_eq!(
            (out.position_s, out.duration_s, out.watched),
            (1000.0, 6000.0, false)
        );
        assert_eq!(e.log(), before);
        assert_eq!(e.state(id), Some((1000.0, 6000.0, false)));
    }

    #[test]
    fn play_offline_item_errors() {
        let e = env();
        let id = e.add(ItemKind::Movie, "Heat", None);
        let off = e.offline_source();
        e.file(off, id, "Heat.mkv", None, 1);
        let tools = Tools {
            mpv: Some(PathBuf::from("mpv.exe")),
            ..Tools::default()
        };
        match play(&e.c, &e.v, &tools, id, true) {
            Err(crate::error::Error::SourceOffline(m)) => assert!(m.contains(r"X:\Films"), "{m}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn play_item_without_files_is_not_found() {
        let e = env();
        let id = e.add(ItemKind::Movie, "Heat", None);
        let tools = Tools {
            mpv: Some(PathBuf::from("mpv.exe")),
            ..Tools::default()
        };
        assert!(matches!(
            play(&e.c, &e.v, &tools, id, true),
            Err(crate::error::Error::NotFound(_))
        ));
    }

    #[test]
    fn play_without_mpv_errors() {
        let e = env();
        let id = e.add(ItemKind::Movie, "Heat", None);
        e.file(e.src, id, "Heat.mkv", None, 1);
        let gone = e.tools_dir.path().join("mpv.exe");
        crate::tools::set_tool_path(&e.c, crate::tools::Tool::Mpv, Some(&gone)).unwrap();
        match play(&e.c, &e.v, &Tools::default(), id, true) {
            Err(crate::error::Error::Tool(m)) => {
                assert!(m.contains(&*gone.to_string_lossy()), "{m}")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn play_without_any_mpv_names_the_setting() {
        let e = env();
        let id = e.add(ItemKind::Movie, "Heat", None);
        e.file(e.src, id, "Heat.mkv", None, 1);
        match play(&e.c, &e.v, &Tools::default(), id, true) {
            Err(crate::error::Error::Tool(m)) => {
                assert!(m.contains("PATH") && m.contains("tool.mpv"), "{m}")
            }
            other => panic!("{other:?}"),
        }
    }
}
