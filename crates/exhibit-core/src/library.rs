//! The one surface the desktop app, the MCP server and the CLI call. Every method wires
//! one function from another module to the connection, volumes, tools and stored key;
//! behaviour belongs in those modules, not here.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::cast;
use crate::db;
use crate::download::{self, DownloadEvent, DownloadReport, UreqFetch};
use crate::error::{Detail, Error, Fault, Result};
use crate::exhibition::{self, Exhibition, NewExhibition};
use crate::filter::{self, Count, Facets};
use crate::freshen::{self, FreshenReport};
use crate::identify::{self, IdentifyReport, IdentifyScope};
use crate::lang;
use crate::model::{Candidate, CastMember, Id, ItemKind, MatchSource, Source};
use crate::player::{self, PlayOutcome};
use crate::probe;
use crate::query::{
    self, HistoryEntry, ItemDetail, ItemSummary, LibraryCounts, SearchQuery, Shelf,
};
use crate::reception::{self, ReceptionReport};
use crate::scan::{self, ScanEvent, ScanReport};
use crate::secrets::{OsSecrets, Secrets};
use crate::service::{Account, DeviceCode, Need, Service, SyncInput, normalize_username};
use crate::signin::{self, SignInState};
use crate::similar::{self, Similar};
use crate::sync::{self, ServiceState, SyncReport};
use crate::tmdb::{self, Http, MetadataProvider, Tmdb, UreqHttp};
use crate::tools::{self, Env, Tool, ToolStatus};
use crate::translate::{self, TranslateReport};
use crate::volume::{SystemVolumes, Volumes};

const TMDB_KEY: &str = "tmdb";
const AGENT_KEY: &str = "agent";
/// A setting: `"off"` when the owner turned the intro off.
const INTRO_KEY: &str = "intro";

/// What the library holds and which keys are set. The media tools are not part of it: detecting
/// them runs each one, so they are asked for apart (`detect_tools`) when a screen shows them.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LibraryStatus {
    #[serde(flatten)]
    pub counts: LibraryCounts,
    pub sources: Vec<Source>,
    /// Exhibit can reach TMDB: with the owner's own key, or with Exhibit's.
    pub tmdb_key: bool,
    /// The owner set their own TMDB key; it is used instead of Exhibit's.
    pub tmdb_own_key: bool,
    /// This build carries a TMDB key of its own.
    pub tmdb_builtin: bool,
    pub agent_key: bool,
    /// Whether the window plays the intro as it opens.
    pub intro: bool,
    /// The language in effect (`lang::LANGUAGES`).
    pub language: &'static str,
    /// How dates are written (`lang::DATE_FORMATS`).
    pub date_format: &'static str,
}

/// The steps of `Library::refresh`, in the order they run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshPhase {
    Scanning,
    Identifying,
    Probing,
    Rescoring,
    /// Fetching the text of matched items in the owner's language.
    Translating,
    Thumbnails,
    /// Asking services for the public's scores and the awards.
    Reception,
    /// Fetching TMDB's details of older works again (`freshen`).
    Freshening,
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
    pub identify_error: Option<Fault>,
    pub probed: Option<(u32, u32)>,
    pub rescored: Option<IdentifyReport>,
    pub translated: Option<TranslateReport>,
    pub thumbnails: Option<(u32, u32)>,
}

pub struct Library {
    conn: Connection,
    data_dir: PathBuf,
    volumes: Box<dyn Volumes + Send>,
    secrets: Box<dyn Secrets + Send>,
    env: Env,
    http: Box<dyn Http + Send + Sync>,
    /// Exhibit's own TMDB key, used when the owner has not set theirs.
    builtin_tmdb: Option<String>,
}

impl Library {
    /// The owner's data directory, drives, credential store and machine, and Exhibit's own TMDB
    /// key when the build has one (`EXHIBIT_TMDB_BUILTIN=off` leaves it out, for tests).
    pub fn open_default() -> Result<Library> {
        let mut lib = Library::open(
            db::data_dir(),
            Box::new(SystemVolumes::new()),
            Box::new(OsSecrets::new()),
            Env::system(),
        )?;
        if std::env::var("EXHIBIT_TMDB_BUILTIN").as_deref() != Ok("off") {
            lib.builtin_tmdb = tmdb::builtin_key();
        }
        Ok(lib)
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
            builtin_tmdb: None,
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

    /// The same library with a TMDB key of its own, as a build that carries one has.
    #[cfg(test)]
    pub(crate) fn with_builtin_tmdb(mut self, key: &str) -> Library {
        self.builtin_tmdb = Some(key.to_string());
        self
    }

    /// The TMDB key in use: the owner's own, else Exhibit's.
    fn tmdb_key(&self) -> Result<Option<String>> {
        Ok(self
            .secrets
            .get(TMDB_KEY)?
            .or_else(|| self.builtin_tmdb.clone()))
    }

    fn has_tmdb(&self) -> Result<bool> {
        Ok(self.tmdb_key()?.is_some())
    }

    fn provider(&self) -> Result<Tmdb<&(dyn Http + Sync)>> {
        let key = self
            .tmdb_key()?
            .ok_or_else(|| Error::Invalid(Detail::coded("tmdb.no_key", "TMDB key not set")))?;
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

    /// Returns (ok, failed); `Error::Tool` when mpv is not installed. `on` gets each file id
    /// and, for a failure, its reason.
    pub fn probe_pending(&self, on: &mut dyn FnMut(Id, Option<&str>)) -> Result<(u32, u32)> {
        let tools = tools::tools(&self.conn, &self.env)?;
        probe::probe_pending(&self.conn, &*self.volumes, &tools, on)
    }

    /// True when mpv, which reads files' details, is usable.
    pub fn can_probe(&self) -> Result<bool> {
        Ok(tools::tools(&self.conn, &self.env)?.mpv.is_some())
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

    /// Identifies the items in `scope`. In a language other than English, what it matched
    /// is then translated, so new arrivals show in the owner's language at once.
    pub fn identify(&self, scope: IdentifyScope, on: &mut dyn FnMut(Id)) -> Result<IdentifyReport> {
        let p = self.provider()?;
        let mut report =
            identify::identify(&self.conn, &*self.volumes, &p, &self.image_dir(), scope, on)?;
        let lang = self.language()?;
        if report.matched > 0 && lang != lang::BASE {
            let t = translate::translate(&self.conn, &p, &self.image_dir(), lang, &mut |_| {})?;
            report.errors.extend(t.errors);
        }
        Ok(report)
    }

    /// Fetches the text of every matched item the library does not hold in the language in
    /// effect. Nothing is matched again. `on` gets each item done.
    pub fn translate(&self, on: &mut dyn FnMut(Id)) -> Result<TranslateReport> {
        let lang = self.language()?;
        if lang == lang::BASE {
            return Ok(TranslateReport::default());
        }
        translate::translate(&self.conn, &self.provider()?, &self.image_dir(), lang, on)
    }

    /// Whether the library holds the text of every matched item in the language in effect.
    pub fn translated(&self) -> Result<bool> {
        translate::complete(&self.conn, self.language()?)
    }

    /// TMDB search for picking a match by hand. `kind` is `Movie` or `Show`.
    pub fn lookup(&self, text: &str, kind: ItemKind, year: Option<i32>) -> Result<Vec<Candidate>> {
        identify::lookup(
            &self.provider()?,
            &self.image_dir(),
            text,
            kind,
            year,
            self.language()?,
        )
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

        if opts.identify && self.has_tmdb()? {
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

        // A language chosen, or suggested by Windows, that the library does not hold yet.
        if opts.identify && self.has_tmdb()? && !self.translated()? {
            on(RefreshEvent::Phase(RefreshPhase::Translating));
            let done = self.translate(&mut |id| on(RefreshEvent::Item(id)));
            match recoverable(done)? {
                Ok(r) => report.translated = Some(r),
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

    pub fn facets(&self, kind: Option<ItemKind>) -> Result<Facets> {
        filter::facets(&self.conn, &*self.volumes, kind)
    }

    pub fn item(&self, id: Id) -> Result<ItemDetail> {
        query::item(&self.conn, &*self.volumes, id)
    }

    /// The cast of `id` with portraits, fetched and cached on first ask; see `cast::cast`.
    pub fn cast(&self, id: Id) -> Result<Vec<CastMember>> {
        let tmdb = if self.has_tmdb()? {
            Some(self.provider()?)
        } else {
            None
        };
        cast::cast(
            &self.conn,
            tmdb.as_ref().map(|t| t as &dyn MetadataProvider),
            &self.image_dir(),
            id,
        )
    }

    /// The works most like `id` in the library, with what links each; see `similar::similar`.
    pub fn similar(&self, id: Id, limit: u32) -> Result<Vec<Similar>> {
        similar::similar(&self.conn, &*self.volumes, id, limit)
    }

    /// The people with the most works in the library in `role`; see `similar::top_people`.
    pub fn top_people(&self, role: &str, limit: u32) -> Result<Vec<Count<String>>> {
        similar::top_people(&self.conn, &*self.volumes, role, limit)
    }

    /// Every exhibition hanging, the most recently hung first.
    pub fn exhibitions(&self) -> Result<Vec<Exhibition>> {
        exhibition::list(&self.conn, &*self.volumes)
    }

    pub fn exhibition(&self, id: Id) -> Result<Option<Exhibition>> {
        exhibition::get(&self.conn, &*self.volumes, id)
    }

    /// Hangs an exhibition, or replaces the one it names; see `exhibition::save`.
    pub fn save_exhibition(&self, e: &NewExhibition) -> Result<Id> {
        exhibition::save(&self.conn, &*self.volumes, e)
    }

    pub fn remove_exhibition(&self, id: Id) -> Result<bool> {
        exhibition::remove(&self.conn, id)
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

    pub fn service_states(&self) -> Result<Vec<ServiceState>> {
        sync::service_states(&self.conn)
    }

    /// Switched off, a service's facts count nowhere; they are kept for when it is back on.
    pub fn set_service_enabled(&self, service: &str, on: bool) -> Result<()> {
        sync::set_service_enabled(&self.conn, service, on)
    }

    /// `None` forgets the service: its facts, scores, films, state, `wanted` rows and any
    /// sign-in go.
    pub fn set_service_username(&self, service: &str, username: Option<&str>) -> Result<()> {
        sync::set_service_username(&self.conn, service, username)?;
        if username.is_none() {
            self.secrets.set(service, None)?;
        }
        Ok(())
    }

    /// Starts signing in to `svc`: the code the owner enters and where.
    pub fn start_sign_in(&self, svc: &dyn Service) -> Result<DeviceCode> {
        svc.sign_in(&*self.http)
    }

    /// Whether the owner has approved the sign-in yet; on approval it is kept.
    pub fn poll_sign_in(&self, svc: &dyn Service, device_code: &str) -> Result<SignInState> {
        signin::poll(&self.conn, &*self.secrets, svc, device_code, &*self.http)
    }

    /// Asks `svc` for the scores or awards of the works that are due. A service that needs a
    /// sign-in or a key the owner has not given does nothing.
    pub fn refresh_reception(
        &self,
        svc: &dyn Service,
        on: &mut dyn FnMut(u32),
    ) -> Result<ReceptionReport> {
        let info = svc.info();
        let key = self.service_key(svc)?;
        if key.is_none()
            && info
                .needs
                .iter()
                .any(|n| matches!(n, Need::SignIn | Need::Key))
        {
            return Ok(ReceptionReport::default());
        }
        let username = sync::service_username(&self.conn, info.id)?;
        let account = Account {
            username: username.as_deref(),
            key: key.as_deref(),
        };
        reception::refresh(&self.conn, svc, &account, &*self.http, db::now(), on)
    }

    /// What `svc` is reached with: its sign-in's access token (renewed when due) or its key.
    fn service_key(&self, svc: &dyn Service) -> Result<Option<String>> {
        if svc.info().needs.contains(&Need::SignIn) {
            signin::access(&self.conn, &*self.secrets, svc, &*self.http, db::now())
        } else {
            self.secrets.get(svc.info().id)
        }
    }

    /// Asks the service whether `username` is good: how many recent entries it shows, or a
    /// coded error that says what is wrong.
    pub fn check_service(&self, svc: &dyn Service, username: &str) -> Result<u32> {
        let username = normalize_username(username);
        let key = self.service_key(svc)?;
        let account = Account {
            username: Some(&username),
            key: key.as_deref(),
        };
        svc.check(&account, &*self.http)
    }

    /// Snapshot replaces the service's facts, then reads `Since(None)` at once; Since adds and
    /// updates. Without a TMDB key, films are not searched and no wanted rows are made.
    pub fn sync_activity(
        &self,
        svc: &dyn Service,
        input: SyncInput,
        on: &mut dyn FnMut(Id),
    ) -> Result<SyncReport> {
        let id = svc.info().id;
        let username = sync::service_username(&self.conn, id)?;
        let key = self.service_key(svc)?;
        let account = Account {
            username: username.as_deref(),
            key: key.as_deref(),
        };
        let tmdb = if self.has_tmdb()? {
            Some(self.provider()?)
        } else {
            None
        };
        let images = self.image_dir();
        let tmdb = tmdb
            .as_ref()
            .map(|t| (t as &dyn MetadataProvider, images.as_path()));
        sync::sync(&self.conn, svc, &account, input, &*self.http, tmdb, on)
    }

    /// Fetches TMDB's details of works that are getting old again, at most about `budget`
    /// requests, `pace` apart. Nothing to do without a TMDB key.
    pub fn freshen(
        &self,
        budget: u32,
        pace: std::time::Duration,
        on: &mut dyn FnMut(Id),
    ) -> Result<FreshenReport> {
        if !self.has_tmdb()? {
            return Ok(FreshenReport::default());
        }
        freshen::freshen(
            &self.conn,
            &self.provider()?,
            &self.image_dir(),
            db::now(),
            budget,
            pace,
            on,
        )
    }

    /// The films of `service` left out because no match was sure, by title.
    pub fn service_unmatched(&self, service: &str) -> Result<Vec<(String, Option<i32>)>> {
        sync::service_unmatched(&self.conn, service)
    }

    /// Blocks until the player exits.
    pub fn play(&self, item_id: Id, resume: bool) -> Result<PlayOutcome> {
        let tools = tools::tools(&self.conn, &self.env)?;
        player::play(&self.conn, &*self.volumes, &tools, item_id, resume)
    }

    /// Also clears away downloads the newest has replaced, once nothing runs from them.
    pub fn detect_tools(&self) -> Result<Vec<ToolStatus>> {
        if let Some(dir) = &self.env.tools_dir {
            download::tidy(dir);
        }
        tools::detect(&self.conn, &self.env)
    }

    /// Downloads the latest mpv and ffmpeg for this machine into Exhibit's tools folder, where
    /// they are used ahead of any found on the PC (a path set by hand still wins). Asks GitHub
    /// first and fetches nothing when the newest download is already that release.
    pub fn download_tools(&self, on: &mut dyn FnMut(DownloadEvent)) -> Result<DownloadReport> {
        let dir = self.env.tools_dir.as_deref().ok_or_else(|| {
            Error::Invalid(Detail::coded(
                "download.no_tools_folder",
                "this library has no tools folder",
            ))
        })?;
        download::download(&UreqFetch::new(), dir, on)
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

    /// Whether the window plays the intro as it opens. On unless turned off.
    pub fn intro(&self) -> Result<bool> {
        Ok(db::get_setting(&self.conn, INTRO_KEY)?.as_deref() != Some("off"))
    }

    pub fn set_intro(&self, on: bool) -> Result<()> {
        db::set_setting(&self.conn, INTRO_KEY, (!on).then_some("off"))
    }

    /// The language in effect: the owner's choice, or the one Windows suggests.
    pub fn language(&self) -> Result<&'static str> {
        lang::language(&self.conn)
    }

    /// Chooses the language. Text the library does not hold in it yet comes with
    /// `translate`.
    pub fn set_language(&self, tag: &str) -> Result<()> {
        lang::set_language(&self.conn, tag)
    }

    pub fn date_format(&self) -> Result<&'static str> {
        lang::date_format(&self.conn)
    }

    pub fn set_date_format(&self, format: &str) -> Result<()> {
        lang::set_date_format(&self.conn, format)
    }

    pub fn status(&self) -> Result<LibraryStatus> {
        Ok(LibraryStatus {
            counts: query::library_counts(&self.conn)?,
            sources: self.list_sources()?,
            tmdb_key: self.has_tmdb()?,
            tmdb_own_key: self.has_secret(TMDB_KEY)?,
            tmdb_builtin: self.builtin_tmdb.is_some(),
            agent_key: self.has_secret(AGENT_KEY)?,
            intro: self.intro()?,
            language: self.language()?,
            date_format: self.date_format()?,
        })
    }
}

/// An identification or translation that failed for a reason outside the library (a rejected key, the
/// network, TMDB itself) becomes its message; anything else (the database) stays an error.
fn recoverable<T>(r: Result<T>) -> Result<std::result::Result<T, Fault>> {
    match r {
        Ok(r) => Ok(Ok(r)),
        Err(e @ (Error::Invalid(_) | Error::Provider(_) | Error::NotFound(_))) => {
            Ok(Err(Fault::from(e)))
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

    /// Answers like TMDB with nothing found, and keeps every URL asked.
    #[derive(Clone, Default)]
    struct Seen(std::sync::Arc<std::sync::Mutex<Vec<String>>>);

    impl Http for Seen {
        fn get(&self, url: &str, _bearer: Option<&str>) -> Result<(u16, Vec<u8>)> {
            self.0.lock().unwrap().push(url.to_string());
            Ok((200, br#"{"results":[]}"#.to_vec()))
        }
    }

    #[test]
    fn exhibits_own_key_works_until_the_owner_sets_theirs() {
        let e = env(200);
        let seen = Seen::default();
        let lib = Library::open(
            e.data.path().to_path_buf(),
            Box::new(FakeVolumes(vec![(
                "VOL1".into(),
                e.drive.path().to_path_buf(),
            )])),
            Box::new(MemorySecrets::default()),
            Env::default(),
        )
        .unwrap()
        .with_http(Box::new(seen.clone()))
        .with_builtin_tmdb("builtin");
        let st = lib.status().unwrap();
        assert_eq!(
            (st.tmdb_key, st.tmdb_own_key, st.tmdb_builtin),
            (true, false, true)
        );
        // With no key of the owner's, identifying asks with Exhibit's.
        let (phases, _) = refresh(&lib);
        assert!(phases.contains(&RefreshPhase::Identifying));
        let asked = std::mem::take(&mut *seen.0.lock().unwrap());
        assert!(
            asked.iter().any(|u| u.contains("api_key=builtin")),
            "{asked:?}"
        );
        // The owner's own key is used instead, and removing it falls back.
        lib.set_secret(TMDB_KEY, Some("mine")).unwrap();
        lib.identify(IdentifyScope::Unmatched, &mut |_| {}).unwrap();
        let asked = std::mem::take(&mut *seen.0.lock().unwrap());
        assert!(
            asked.iter().all(|u| u.contains("api_key=mine")),
            "{asked:?}"
        );
        let st = lib.status().unwrap();
        assert_eq!((st.tmdb_key, st.tmdb_own_key), (true, true));
        lib.set_secret(TMDB_KEY, None).unwrap();
        assert!(lib.status().unwrap().tmdb_key);
        // A build without a key of its own, and no key set: nothing to reach TMDB with.
        let st = e.lib.status().unwrap();
        assert_eq!(
            (st.tmdb_key, st.tmdb_own_key, st.tmdb_builtin),
            (false, false, false)
        );
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
        let broken = e.drive.path().join("mpv.txt");
        fs::write(&broken, b"not a program").unwrap();
        e.lib.set_tool_path(Tool::Mpv, Some(&broken)).unwrap();
        let (phases, r) = refresh(&e.lib);
        assert_eq!(
            phases,
            [
                RefreshPhase::Scanning,
                RefreshPhase::Identifying,
                RefreshPhase::Probing,
                // mpv also makes thumbnails; with nothing probed there is no frame to take.
                RefreshPhase::Thumbnails
            ]
        );
        assert_eq!(r.identified, None);
        let why = r.identify_error.unwrap();
        assert_eq!(why.code, Some("tmdb.key_rejected"), "{why:?}");
        assert_eq!(r.probed, Some((0, 1)));
        assert_eq!(r.rescored, None);
    }

    #[cfg(windows)]
    #[test]
    fn refresh_runs_phases_in_order() {
        let e = env(200);
        e.lib.set_secret(TMDB_KEY, Some("k")).unwrap();
        // An mpv that cannot start: probing runs, reads nothing, so nothing is rescored.
        let mpv = e.drive.path().join("mpv.txt");
        fs::write(&mpv, b"not a program").unwrap();
        let ffmpeg = e.drive.path().join("ffmpeg.bat");
        fs::write(&ffmpeg, "@exit /b 0\r\n").unwrap();
        e.lib.set_tool_path(Tool::Mpv, Some(&mpv)).unwrap();
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
            [Scanning, Identifying, Probing, Thumbnails],
            "{r:?} {events:?}"
        );
        assert!(events.iter().any(|ev| matches!(ev, RefreshEvent::Scan(_))));
        // TMDB knows nothing, so the item waits for review.
        assert_eq!(r.identified.map(|i| i.needs_review), Some(1));
        assert_eq!(r.probed, Some((0, 1)));
        assert_eq!(r.rescored, None);
        // No details, so no frame to take.
        assert_eq!(r.thumbnails, Some((0, 0)));

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
    fn intro_is_on_until_turned_off() {
        let e = env(200);
        assert!(e.lib.intro().unwrap());
        e.lib.set_intro(false).unwrap();
        assert!(!e.lib.intro().unwrap());
        assert!(!e.lib.status().unwrap().intro);
        e.lib.set_intro(true).unwrap();
        assert!(e.lib.intro().unwrap());
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
        let tool: Tool = serde_json::from_str(r#""ffmpeg""#).unwrap();
        assert_eq!(tool, Tool::Ffmpeg);
        assert!(serde_json::from_str::<Tool>(r#""ffprobe""#).is_err());
        let ev = DownloadEvent::Downloading { done: 1, total: 2 };
        assert_eq!(
            serde_json::to_string(&ev).unwrap(),
            r#"{"step":"downloading","done":1,"total":2}"#
        );
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
