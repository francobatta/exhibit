//! The one surface the desktop app, the MCP server and the CLI call. Every method wires
//! one function from another module to the connection, volumes, tools and stored key;
//! behaviour belongs in those modules, not here.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;

use crate::db;
use crate::error::{Error, Result};
use crate::identify::{self, IdentifyReport, IdentifyScope};
use crate::model::{Id, ItemKind, MatchSource, Source};
use crate::player::{self, PlayOutcome};
use crate::probe;
use crate::query::{
    self, HistoryEntry, ItemDetail, ItemSummary, LibraryCounts, SearchQuery, Shelf,
};
use crate::scan::{self, ScanEvent, ScanReport};
use crate::secrets::{OsSecrets, Secrets};
use crate::tmdb::{Tmdb, UreqHttp};
use crate::tools::{self, Env, Tool, ToolStatus};
use crate::volume::{SystemVolumes, Volumes};

const TMDB_KEY: &str = "tmdb";
const AGENT_KEY: &str = "agent";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LibraryStatus {
    #[serde(flatten)]
    pub counts: LibraryCounts,
    pub sources: Vec<Source>,
    pub tmdb_key: bool,
    pub agent_key: bool,
    pub tools: Vec<ToolStatus>,
}

pub struct Library {
    conn: Connection,
    data_dir: PathBuf,
    volumes: Box<dyn Volumes>,
    secrets: Box<dyn Secrets>,
    env: Env,
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
        volumes: Box<dyn Volumes>,
        secrets: Box<dyn Secrets>,
        env: Env,
    ) -> Result<Library> {
        let conn = db::open(&data_dir)?;
        Ok(Library {
            conn,
            data_dir,
            volumes,
            secrets,
            env,
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn cache_dir(&self) -> PathBuf {
        self.data_dir.join("cache").join("images")
    }

    fn provider(&self) -> Result<Tmdb<UreqHttp>> {
        let key = self
            .secrets
            .get(TMDB_KEY)?
            .ok_or_else(|| Error::Invalid("TMDB key not set".into()))?;
        Ok(Tmdb::new(key, UreqHttp::new()))
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

    pub fn identify(&self, scope: IdentifyScope, on: &mut dyn FnMut(Id)) -> Result<IdentifyReport> {
        identify::identify(
            &self.conn,
            &*self.volumes,
            &self.provider()?,
            &self.cache_dir(),
            scope,
            on,
        )
    }

    /// A permanent (`Manual`) match.
    pub fn apply_match(&self, item_id: Id, tmdb_id: i64, kind: ItemKind) -> Result<()> {
        identify::apply_match(
            &self.conn,
            &self.provider()?,
            &self.cache_dir(),
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
            tools: self.detect_tools()?,
        })
    }
}
