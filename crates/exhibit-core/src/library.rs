//! The one surface the desktop app, the MCP server and the CLI call. Every method wires
//! one function from another module to the connection, volumes, tools and stored key;
//! behaviour belongs in those modules, not here.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db;
use crate::error::{Error, Result};
use crate::identify::{self, IdentifyReport, IdentifyScope};
use crate::model::{Candidate, Id, ItemKind, MatchSource, Source};
use crate::player::{self, PlayOutcome};
use crate::probe;
use crate::query::{
    self, HistoryEntry, ItemDetail, ItemSummary, LibraryCounts, SearchQuery, Shelf,
};
use crate::scan::{self, ScanEvent, ScanReport};
use crate::secrets::{OsSecrets, Secrets};
use crate::tmdb::{Http, Tmdb, UreqHttp};
use crate::tools::{self, Env, Tool, ToolStatus};
use crate::volume::{SystemVolumes, Volumes};

const TMDB_KEY: &str = "tmdb";
const AGENT_KEY: &str = "agent";

/// What the library holds and which keys are set. The media tools are not part of it: detecting
/// them runs each one, so they are asked for apart (`detect_tools`) when a screen shows them.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LibraryStatus {
    #[serde(flatten)]
    pub counts: LibraryCounts,
    pub sources: Vec<Source>,
    pub tmdb_key: bool,
    pub agent_key: bool,
}

/// The steps of `Library::refresh`, in the order they run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshPhase {
    Scanning,
    Identifying,
    Probing,
    Rescoring,
    Thumbnails,
}

/// Progress of a refresh: a phase starting, a scan event, an item identified or given a
/// thumbnail, a file probed (with the reason when probing it failed).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshEvent {
    Phase(RefreshPhase),
    Scan(ScanEvent),
    Item(Id),
    File { id: Id, error: Option<String> },
}

/// Which optional steps a refresh may run; each also needs its key or tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RefreshOptions {
    pub identify: bool,
    pub probe: bool,
}

impl Default for RefreshOptions {
    fn default() -> Self {
        RefreshOptions {
            identify: true,
            probe: true,
        }
    }
}

/// What a refresh did. A step that did not run is `None`; `probed` and `thumbnails` are
/// (done, failed). When identification failed outright (a rejected key, no network)
/// `identified` is `None` and `identify_error` says why.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RefreshReport {
    pub scanned: Vec<ScanReport>,
    pub identified: Option<IdentifyReport>,
    pub identify_error: Option<String>,
    pub probed: Option<(u32, u32)>,
    pub rescored: Option<IdentifyReport>,
    pub thumbnails: Option<(u32, u32)>,
}

pub struct Library {
    conn: Connection,
    data_dir: PathBuf,
    volumes: Box<dyn Volumes + Send>,
    secrets: Box<dyn Secrets + Send>,
    env: Env,
    http: Box<dyn Http + Send + Sync>,
}

impl Library {
    /// The owner's data directory, drives, credential store and machine.
    pub fn open_default() -> Result<Library> {
        Library::open(
            db::data_dir(),
            Box::new(SystemVolumes::new()),
            Box::new(OsSecrets::new()),
            Env::system(),
        )
    }

    pub fn open(
        data_dir: PathBuf,
        volumes: Box<dyn Volumes + Send>,
        secrets: Box<dyn Secrets + Send>,
        env: Env,
    ) -> Result<Library> {
        let conn = db::open(&data_dir)?;
        Ok(Library {
            conn,
            data_dir,
            volumes,
            secrets,
            env,
            http: Box::new(UreqHttp::new()),
        })
    }

    /// The same library over another HTTP layer, so tests can stand in for TMDB.
    #[cfg(test)]
    pub(crate) fn with_http(mut self, http: Box<dyn Http + Send + Sync>) -> Library {
        self.http = http;
        self
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Where artwork and thumbnails live; items store file names relative to it.
    pub fn image_dir(&self) -> PathBuf {
        self.data_dir.join("cache").join("images")
    }

    /// SQLite's `PRAGMA data_version`: changes when another connection commits.
    pub fn data_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("PRAGMA data_version", [], |r| r.get(0))?)
    }

    fn provider(&self) -> Result<Tmdb<&(dyn Http + Sync)>> {
        let key = self
            .secrets
            .get(TMDB_KEY)?
            .ok_or_else(|| Error::Invalid("TMDB key not set".into()))?;
        Ok(Tmdb::new(key, &*self.http))
    }

    pub fn add_source(&self, path: &Path) -> Result<Source> {
        scan::add_source(&self.conn, &*self.volumes, path)
    }

    pub fn remove_source(&self, id: Id) -> Result<()> {
        scan::remove_source(&self.conn, id)
    }

    pub fn list_sources(&self) -> Result<Vec<Source>> {
        scan::list_sources(&self.conn, &*self.volumes)
    }

    /// One source, or every source when `source` is `None`.
    pub fn scan(
        &self,
        source: Option<Id>,
        on: &mut dyn FnMut(ScanEvent),
    ) -> Result<Vec<ScanReport>> {
        let ids = match source {
            Some(id) => vec![id],
            None => self.list_sources()?.into_iter().map(|s| s.id).collect(),
        };
        ids.into_iter()
            .map(|id| scan::scan_source(&self.conn, &*self.volumes, id, on))
            .collect()
    }

    /// Returns (ok, failed); `Error::Tool` when neither ffprobe nor mpv is installed. `on`
    /// gets each file id and, for a failure, its reason.
    pub fn probe_pending(&self, on: &mut dyn FnMut(Id, Option<&str>)) -> Result<(u32, u32)> {
        let tools = tools::tools(&self.conn, &self.env)?;
        probe::probe_pending(&self.conn, &*self.volumes, &tools, on)
    }

    /// True when at least one tool that can probe a file (ffprobe or mpv) is usable.
    pub fn can_probe(&self) -> Result<bool> {
        let t = tools::tools(&self.conn, &self.env)?;
        Ok(t.ffprobe.is_some() || t.mpv.is_some())
    }

    /// True when a tool that can render a thumbnail (ffmpeg or mpv) is usable.
    pub fn can_thumbnail(&self) -> Result<bool> {
        let t = tools::tools(&self.conn, &self.env)?;
        Ok(t.ffmpeg.is_some() || t.mpv.is_some())
    }

    /// A frame for every `other` item without a backdrop, stored as its backdrop. Returns
    /// (made, failed); `on` gets each item tried. `Error::Tool` with neither ffmpeg nor mpv.
    pub fn thumbnails_pending(&self, on: &mut dyn FnMut(Id)) -> Result<(u32, u32)> {
        let tools = tools::tools(&self.conn, &self.env)?;
        probe::thumbnails_pending(&self.conn, &*self.volumes, &tools, &self.image_dir(), on)
    }

    pub fn identify(&self, scope: IdentifyScope, on: &mut dyn FnMut(Id)) -> Result<IdentifyReport> {
        identify::identify(
            &self.conn,
            &*self.volumes,
            &self.provider()?,
            &self.image_dir(),
            scope,
            on,
        )
    }

    /// TMDB search for picking a match by hand. `kind` is `Movie` or `Show`.
    pub fn lookup(&self, text: &str, kind: ItemKind, year: Option<i32>) -> Result<Vec<Candidate>> {
        identify::lookup(&self.provider()?, &self.image_dir(), text, kind, year)
    }

    /// Scan, identify (with a TMDB key), probe (with a tool), identify the items left for
    /// review again now that probing may have given them a duration, then make thumbnails
    /// (with a tool). The second identification runs only when the first ran and probing
    /// read at least one file. A failed identification is recorded and the rest still runs.
    pub fn refresh(
        &self,
        source: Option<Id>,
        on: &mut dyn FnMut(RefreshEvent),
        opts: RefreshOptions,
    ) -> Result<RefreshReport> {
        let mut report = RefreshReport::default();
        on(RefreshEvent::Phase(RefreshPhase::Scanning));
        report.scanned = self.scan(source, &mut |e| on(RefreshEvent::Scan(e)))?;

        if opts.identify && self.has_secret(TMDB_KEY)? {
            on(RefreshEvent::Phase(RefreshPhase::Identifying));
            let found = self.identify(IdentifyScope::Unmatched, &mut |id| {
                on(RefreshEvent::Item(id))
            });
            match recoverable(found)? {
                Ok(r) => report.identified = Some(r),
                Err(why) => report.identify_error = Some(why),
            }
        }

        if opts.probe && self.can_probe()? {
            on(RefreshEvent::Phase(RefreshPhase::Probing));
            report.probed = Some(self.probe_pending(&mut |id, error| {
                on(RefreshEvent::File {
                    id,
                    error: error.map(str::to_string),
                })
            })?);
        }

        if report.identified.is_some() && report.probed.is_some_and(|(ok, _)| ok > 0) {
            on(RefreshEvent::Phase(RefreshPhase::Rescoring));
            let found = self.identify(IdentifyScope::Review, &mut |id| on(RefreshEvent::Item(id)));
            match recoverable(found)? {
                Ok(r) => report.rescored = Some(r),
                Err(why) => report.identify_error = Some(why),
            }
        }

        if self.can_thumbnail()? {
            on(RefreshEvent::Phase(RefreshPhase::Thumbnails));
            report.thumbnails =
                Some(self.thumbnails_pending(&mut |id| on(RefreshEvent::Item(id)))?);
        }
        Ok(report)
    }

    /// A permanent (`Manual`) match.
    pub fn apply_match(&self, item_id: Id, tmdb_id: i64, kind: ItemKind) -> Result<()> {
        identify::apply_match(
            &self.conn,
            &self.provider()?,
            &self.image_dir(),
            item_id,
            tmdb_id,
            kind,
            MatchSource::Manual,
        )
    }

    pub fn move_to_other(&self, item_id: Id) -> Result<()> {
        identify::move_to_other(&self.conn, item_id)
    }

    pub fn search(&self, q: &SearchQuery) -> Result<Vec<ItemSummary>> {
        query::search(&self.conn, &*self.volumes, q)
    }

    pub fn item(&self, id: Id) -> Result<ItemDetail> {
        query::item(&self.conn, &*self.volumes, id)
    }

    pub fn shelf(&self, shelf: Shelf, limit: u32) -> Result<Vec<ItemSummary>> {
        query::shelf(&self.conn, &*self.volumes, shelf, limit)
    }

    pub fn needs_review(&self) -> Result<Vec<ItemDetail>> {
        query::needs_review(&self.conn, &*self.volumes)
    }

    pub fn set_watched(&self, id: Id, watched: bool) -> Result<()> {
        query::set_watched(&self.conn, id, watched)
    }

    pub fn set_loved(&self, id: Id, loved: bool) -> Result<()> {
        query::set_loved(&self.conn, id, loved)
    }

    pub fn history(&self, limit: u32) -> Result<Vec<HistoryEntry>> {
        query::history(&self.conn, &*self.volumes, limit)
    }

    pub fn taste_profile(&self) -> Result<String> {
        query::taste_profile(&self.conn)
    }

    pub fn set_taste_profile(&self, text: &str) -> Result<()> {
        query::set_taste_profile(&self.conn, text)
    }

    /// Blocks until the player exits.
    pub fn play(&self, item_id: Id, resume: bool) -> Result<PlayOutcome> {
        let tools = tools::tools(&self.conn, &self.env)?;
        player::play(&self.conn, &*self.volumes, &tools, item_id, resume)
    }

    pub fn detect_tools(&self) -> Result<Vec<ToolStatus>> {
        tools::detect(&self.conn, &self.env)
    }

    /// `None` clears the manual path.
    pub fn set_tool_path(&self, tool: Tool, path: Option<&Path>) -> Result<()> {
        tools::set_tool_path(&self.conn, tool, path)
    }

    /// `None` clears the secret. `name` is `"tmdb"` or `"agent"`.
    pub fn set_secret(&self, name: &str, value: Option<&str>) -> Result<()> {
        self.secrets.set(name, value)
    }

    pub fn has_secret(&self, name: &str) -> Result<bool> {
        Ok(self.secrets.get(name)?.is_some())
    }

    pub fn status(&self) -> Result<LibraryStatus> {
        Ok(LibraryStatus {
            counts: query::library_counts(&self.conn)?,
            sources: self.list_sources()?,
            tmdb_key: self.has_secret(TMDB_KEY)?,
            agent_key: self.has_secret(AGENT_KEY)?,
        })
    }
}

/// An identification that failed for a reason outside the library (a rejected key, the
/// network, TMDB itself) becomes its message; anything else (the database) stays an error.
fn recoverable(r: Result<IdentifyReport>) -> Result<std::result::Result<IdentifyReport, String>> {
    match r {
        Ok(r) => Ok(Ok(r)),
        Err(e @ (Error::Invalid(_) | Error::Provider(_) | Error::NotFound(_))) => {
            Ok(Err(e.to_string()))
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemorySecrets;
    use crate::volume::FakeVolumes;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn library_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Library>();
    }

    /// Answers every TMDB request with `status`; a 200 is an empty search result.
    struct Answer(u16);

    impl Http for Answer {
        fn get(&self, _url: &str, _bearer: Option<&str>) -> Result<(u16, Vec<u8>)> {
            Ok((self.0, br#"{"results":[]}"#.to_vec()))
        }
    }

    struct Fixture {
        data: TempDir,
        drive: TempDir,
        lib: Library,
    }

    /// A library over one source holding `Heat.1995.mkv`, TMDB answering `status`.
    fn env(status: u16) -> Fixture {
        let data = tempfile::tempdir().unwrap();
        let drive = tempfile::tempdir().unwrap();
        let media = drive.path().join("media");
        fs::create_dir_all(&media).unwrap();
        fs::write(media.join("Heat.1995.mkv"), b"x").unwrap();
        let v = FakeVolumes(vec![("VOL1".into(), drive.path().to_path_buf())]);
        let lib = Library::open(
            data.path().to_path_buf(),
            Box::new(v),
            Box::new(MemorySecrets::default()),
            Env::default(),
        )
        .unwrap()
        .with_http(Box::new(Answer(status)));
        lib.add_source(&media).unwrap();
        Fixture { data, drive, lib }
    }

    fn refresh(lib: &Library) -> (Vec<RefreshPhase>, RefreshReport) {
        let mut phases = Vec::new();
        let on = &mut |e| {
            if let RefreshEvent::Phase(p) = e {
                phases.push(p)
            }
        };
        let r = lib.refresh(None, on, RefreshOptions::default()).unwrap();
        (phases, r)
    }

    #[test]
    fn refresh_skips_identify_without_key() {
        let e = env(200);
        let (phases, r) = refresh(&e.lib);
        assert_eq!(phases, [RefreshPhase::Scanning]);
        assert_eq!(r.scanned.len(), 1);
        assert_eq!(r.scanned[0].added, 1);
        assert_eq!(
            (
                r.identified,
                r.identify_error,
                r.probed,
                r.rescored,
                r.thumbnails
            ),
            (None, None, None, None, None)
        );
    }

    #[test]
    fn refresh_survives_a_rejected_key() {
        let e = env(401);
        e.lib.set_secret(TMDB_KEY, Some("k")).unwrap();
        // A "tool" that cannot start: probing runs and every file fails.
        let broken = e.drive.path().join("ffprobe.txt");
        fs::write(&broken, b"not a program").unwrap();
        e.lib.set_tool_path(Tool::Ffprobe, Some(&broken)).unwrap();
        let (phases, r) = refresh(&e.lib);
        assert_eq!(
            phases,
            [
                RefreshPhase::Scanning,
                RefreshPhase::Identifying,
                RefreshPhase::Probing
            ]
        );
        assert_eq!(r.identified, None);
        let why = r.identify_error.unwrap();
        assert!(why.contains("rejected the key"), "{why}");
        assert_eq!(r.probed, Some((0, 1)));
        assert_eq!(r.rescored, None);
    }

    #[cfg(windows)]
    #[test]
    fn refresh_runs_phases_in_order() {
        let e = env(200);
        e.lib.set_secret(TMDB_KEY, Some("k")).unwrap();
        // `type` in a batch file needs backslashes.
        let fixture = ["tests", "fixtures", "ffprobe", "hdr10_2160p.json"]
            .iter()
            .fold(PathBuf::from(env!("CARGO_MANIFEST_DIR")), |p, s| p.join(s));
        let ffprobe = e.drive.path().join("ffprobe.bat");
        fs::write(&ffprobe, format!("@type \"{}\"\r\n", fixture.display())).unwrap();
        let ffmpeg = e.drive.path().join("ffmpeg.bat");
        fs::write(
            &ffmpeg,
            "@echo off\r\nfor %%a in (%*) do set \"last=%%~a\"\r\necho jpg> \"%last%\"\r\n",
        )
        .unwrap();
        e.lib.set_tool_path(Tool::Ffprobe, Some(&ffprobe)).unwrap();
        e.lib.set_tool_path(Tool::Ffmpeg, Some(&ffmpeg)).unwrap();

        let mut events = Vec::new();
        let r = e
            .lib
            .refresh(None, &mut |ev| events.push(ev), RefreshOptions::default())
            .unwrap();
        let phases: Vec<RefreshPhase> = events
            .iter()
            .filter_map(|ev| match ev {
                RefreshEvent::Phase(p) => Some(*p),
                _ => None,
            })
            .collect();
        use RefreshPhase::*;
        assert_eq!(
            phases,
            [Scanning, Identifying, Probing, Rescoring, Thumbnails],
            "{r:?} {events:?}"
        );
        assert!(events.iter().any(|ev| matches!(ev, RefreshEvent::Scan(_))));
        assert!(
            events
                .iter()
                .any(|ev| matches!(ev, RefreshEvent::File { error: None, .. }))
        );
        // TMDB knows nothing, so the item waits for review both times.
        assert_eq!(r.identified.map(|i| i.needs_review), Some(1));
        assert_eq!(r.probed, Some((1, 0)));
        assert_eq!(r.rescored.map(|i| i.needs_review), Some(1));
        assert_eq!(r.thumbnails, Some((1, 0)));
        let item = &e.lib.search(&SearchQuery::default()).unwrap()[0];
        let thumb = item.backdrop.clone().unwrap();
        assert!(e.lib.image_dir().join(thumb).is_file());

        // Without the optional steps only scanning and thumbnails remain.
        let mut phases = Vec::new();
        let none = RefreshOptions {
            identify: false,
            probe: false,
        };
        let on = &mut |ev| {
            if let RefreshEvent::Phase(p) = ev {
                phases.push(p)
            }
        };
        e.lib.refresh(None, on, none).unwrap();
        assert_eq!(phases, [Scanning, Thumbnails]);
    }

    #[test]
    fn data_version_changes_after_other_connection_writes() {
        let e = env(200);
        let before = e.lib.data_version().unwrap();
        let other = db::open(e.data.path()).unwrap();
        db::set_setting(&other, "k", Some("v")).unwrap();
        assert_ne!(e.lib.data_version().unwrap(), before);
    }

    #[test]
    fn image_dir_is_under_the_data_dir() {
        let e = env(200);
        assert_eq!(
            e.lib.image_dir(),
            e.data.path().join("cache").join("images")
        );
    }

    #[test]
    fn window_types_round_trip_through_json() {
        let scope: IdentifyScope = serde_json::from_str(r#"{"item":7}"#).unwrap();
        assert!(matches!(scope, IdentifyScope::Item(7)));
        for (text, json) in [
            (IdentifyScope::Unmatched, r#""unmatched""#),
            (IdentifyScope::Review, r#""review""#),
            (
                IdentifyScope::UnmatchedAndReview,
                r#""unmatched_and_review""#,
            ),
        ] {
            assert_eq!(serde_json::to_string(&text).unwrap(), json);
        }
        let ev = RefreshEvent::Phase(RefreshPhase::Rescoring);
        assert_eq!(
            serde_json::to_string(&ev).unwrap(),
            r#"{"phase":"rescoring"}"#
        );
        let ev = RefreshEvent::Scan(ScanEvent::Started { source_id: 3 });
        let back: RefreshEvent =
            serde_json::from_str(&serde_json::to_string(&ev).unwrap()).unwrap();
        assert_eq!(back, ev);
        let tool: Tool = serde_json::from_str(r#""ffprobe""#).unwrap();
        assert_eq!(tool, Tool::Ffprobe);
    }

    #[test]
    fn lookup_without_key_names_the_key() {
        let e = env(200);
        match e.lib.lookup("Heat", ItemKind::Movie, None) {
            Err(Error::Invalid(m)) => assert!(m.contains("TMDB key"), "{m}"),
            other => panic!("{other:?}"),
        }
    }
}
