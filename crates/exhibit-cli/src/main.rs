//! `exhibit`: every command prints one JSON document to stdout. Errors print
//! `{"error": {"kind", "message"}}` and exit 1. Progress goes to stderr, only on a terminal.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use exhibit_core::identify::IdentifyScope;
use exhibit_core::model::{Id, ItemKind};
use exhibit_core::query::{SearchQuery, Shelf, Sort};
use exhibit_core::scan::ScanEvent;
use exhibit_core::tools::Tool;
use exhibit_core::{Error, Library, Result};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Parser)]
#[command(
    name = "exhibit",
    version,
    about = "A local media gallery; every command prints JSON"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Library counts, sources, tools and which keys are set
    Status,
    #[command(subcommand)]
    Source(SourceCmd),
    /// Scan sources, then probe files, then identify when a TMDB key is set
    Scan {
        #[arg(long)]
        source: Option<Id>,
        #[arg(long)]
        no_probe: bool,
        #[arg(long)]
        no_identify: bool,
    },
    /// Match unmatched items against TMDB
    Identify {
        /// Also retry items waiting for review
        #[arg(long)]
        review: bool,
        #[arg(long)]
        item: Option<Id>,
    },
    Search(SearchArgs),
    Item {
        id: Id,
    },
    Shelf {
        shelf: ShelfArg,
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Play in mpv; blocks until it exits
    Play {
        id: Id,
        #[arg(long)]
        restart: bool,
    },
    Watched {
        id: Id,
        #[arg(long)]
        unset: bool,
    },
    Loved {
        id: Id,
        #[arg(long)]
        unset: bool,
    },
    #[command(subcommand)]
    Review(ReviewCmd),
    History {
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    #[command(subcommand)]
    Taste(TasteCmd),
    #[command(subcommand)]
    Tools(ToolsCmd),
    #[command(subcommand)]
    Key(KeyCmd),
}

#[derive(Subcommand)]
enum SourceCmd {
    Add { path: PathBuf },
    Remove { id: Id },
    List,
}

#[derive(Subcommand)]
enum ReviewCmd {
    List,
    /// Match an item to a TMDB id
    Match {
        item_id: Id,
        tmdb_id: i64,
        #[arg(long)]
        kind: MatchKind,
    },
    /// Keep an item as plain "other" media
    Other {
        item_id: Id,
    },
}

#[derive(Subcommand)]
enum TasteCmd {
    Get,
    Set { text: String },
}

#[derive(Subcommand)]
enum ToolsCmd {
    List,
    Set { tool: ToolArg, path: PathBuf },
    Clear { tool: ToolArg },
}

#[derive(Subcommand)]
enum KeyCmd {
    /// The value is never echoed back
    Set {
        name: KeyName,
        value: String,
    },
    Clear {
        name: KeyName,
    },
}

#[derive(clap::Args)]
struct SearchArgs {
    text: Option<String>,
    #[arg(long)]
    kind: Option<KindArg>,
    #[arg(long)]
    genre: Option<String>,
    #[arg(long)]
    year_min: Option<i32>,
    #[arg(long)]
    year_max: Option<i32>,
    #[arg(long)]
    runtime_max: Option<i32>,
    #[arg(long, conflicts_with = "unwatched")]
    watched: bool,
    #[arg(long)]
    unwatched: bool,
    #[arg(long)]
    loved: bool,
    #[arg(long)]
    min_height: Option<u32>,
    #[arg(long)]
    available: bool,
    #[arg(long)]
    sort: Option<SortArg>,
    #[arg(long)]
    limit: Option<u32>,
    #[arg(long)]
    offset: Option<u32>,
}

#[derive(Clone, Copy, ValueEnum)]
enum KindArg {
    Movie,
    Show,
    Other,
    Episode,
}

#[derive(Clone, Copy, ValueEnum)]
enum MatchKind {
    Movie,
    Show,
}

#[derive(Clone, Copy, ValueEnum)]
enum SortArg {
    Title,
    Added,
    Year,
    Runtime,
}

#[derive(Clone, Copy, ValueEnum)]
enum ShelfArg {
    Continue,
    Recent,
    Unwatched,
    Loved,
}

#[derive(Clone, Copy, ValueEnum)]
enum ToolArg {
    Mpv,
    Ffprobe,
    Ffmpeg,
}

#[derive(Clone, Copy, ValueEnum)]
enum KeyName {
    Tmdb,
    Agent,
}

impl KeyName {
    fn secret(self) -> &'static str {
        match self {
            KeyName::Tmdb => "tmdb",
            KeyName::Agent => "agent",
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = Library::open_default().and_then(|lib| run(&lib, cli.command));
    let (value, code) = match result {
        Ok(v) => (v, ExitCode::SUCCESS),
        Err(e) => (error_json(&e), ExitCode::FAILURE),
    };
    let text = if std::io::stdout().is_terminal() {
        serde_json::to_string_pretty(&value)
    } else {
        serde_json::to_string(&value)
    };
    println!("{}", text.expect("JSON values serialise"));
    code
}

fn error_json(e: &Error) -> Value {
    let kind = match e {
        Error::Db(_) => "db",
        Error::Io(_) => "io",
        Error::NotFound(_) => "not_found",
        Error::SourceOffline(_) => "source_offline",
        Error::Tool(_) => "tool",
        Error::Provider(_) => "provider",
        Error::Invalid(_) => "invalid",
    };
    json!({"error": {"kind": kind, "message": e.to_string()}})
}

fn ok() -> Result<Value> {
    Ok(json!({"ok": true}))
}

fn to_json<T: Serialize>(v: T) -> Result<Value> {
    serde_json::to_value(v).map_err(|e| Error::Invalid(format!("cannot serialise output: {e}")))
}

/// Writes one short line to stderr when it is a terminal.
fn progress(line: String) {
    if std::io::stderr().is_terminal() {
        eprintln!("{line}");
    }
}

fn run(lib: &Library, command: Command) -> Result<Value> {
    match command {
        Command::Status => to_json(lib.status()?),
        Command::Source(c) => source(lib, c),
        Command::Scan {
            source,
            no_probe,
            no_identify,
        } => scan(lib, source, no_probe, no_identify),
        Command::Identify { review, item } => {
            let scope = match (item, review) {
                (Some(id), _) => IdentifyScope::Item(id),
                (None, true) => IdentifyScope::UnmatchedAndReview,
                (None, false) => IdentifyScope::Unmatched,
            };
            to_json(lib.identify(scope, &mut |id| progress(format!("identified item {id}")))?)
        }
        Command::Search(args) => to_json(lib.search(&search_query(args))?),
        Command::Item { id } => to_json(lib.item(id)?),
        Command::Shelf { shelf, limit } => {
            let shelf = match shelf {
                ShelfArg::Continue => Shelf::ContinueWatching,
                ShelfArg::Recent => Shelf::RecentlyAdded,
                ShelfArg::Unwatched => Shelf::Unwatched,
                ShelfArg::Loved => Shelf::Loved,
            };
            to_json(lib.shelf(shelf, limit)?)
        }
        Command::Play { id, restart } => to_json(lib.play(id, !restart)?),
        Command::Watched { id, unset } => lib.set_watched(id, !unset).and_then(|_| ok()),
        Command::Loved { id, unset } => lib.set_loved(id, !unset).and_then(|_| ok()),
        Command::Review(c) => review(lib, c),
        Command::History { limit } => to_json(lib.history(limit)?),
        Command::Taste(TasteCmd::Get) => to_json(lib.taste_profile()?),
        Command::Taste(TasteCmd::Set { text }) => lib.set_taste_profile(&text).and_then(|_| ok()),
        Command::Tools(c) => tools(lib, c),
        Command::Key(KeyCmd::Set { name, value }) => {
            lib.set_secret(name.secret(), Some(&value))?;
            ok()
        }
        Command::Key(KeyCmd::Clear { name }) => {
            lib.set_secret(name.secret(), None)?;
            ok()
        }
    }
}

fn source(lib: &Library, cmd: SourceCmd) -> Result<Value> {
    match cmd {
        SourceCmd::Add { path } => to_json(lib.add_source(&path)?),
        SourceCmd::Remove { id } => lib.remove_source(id).and_then(|_| ok()),
        SourceCmd::List => to_json(lib.list_sources()?),
    }
}

/// Scan, then probe, then identify. Missing tools or a missing TMDB key skip that step
/// quietly: its field is `null`.
fn scan(lib: &Library, source: Option<Id>, no_probe: bool, no_identify: bool) -> Result<Value> {
    let scanned = lib.scan(source, &mut |event| match event {
        ScanEvent::Started { source_id } => progress(format!("scanning source {source_id}")),
        ScanEvent::FileAdded { rel_path, .. } => progress(format!("added {rel_path}")),
        ScanEvent::Finished(r) => {
            progress(format!("scanned source {}: {} added", r.source_id, r.added))
        }
    })?;
    let probed = if no_probe {
        None
    } else {
        match lib.probe_pending(&mut |id| progress(format!("probed file {id}"))) {
            Ok((ok, failed)) => Some(json!({"ok": ok, "failed": failed})),
            Err(Error::Tool(_)) => None,
            Err(e) => return Err(e),
        }
    };
    let identified = if no_identify || !lib.has_secret("tmdb")? {
        None
    } else {
        let on = &mut |id| progress(format!("identified item {id}"));
        Some(to_json(lib.identify(IdentifyScope::Unmatched, on)?)?)
    };
    Ok(json!({"scanned": scanned, "probed": probed, "identified": identified}))
}

fn search_query(a: SearchArgs) -> SearchQuery {
    SearchQuery {
        text: a.text,
        kind: a.kind.map(|k| match k {
            KindArg::Movie => ItemKind::Movie,
            KindArg::Show => ItemKind::Show,
            KindArg::Other => ItemKind::Other,
            KindArg::Episode => ItemKind::Episode,
        }),
        genre: a.genre,
        year_min: a.year_min,
        year_max: a.year_max,
        runtime_max_min: a.runtime_max,
        watched: (a.watched || a.unwatched).then_some(a.watched),
        loved: a.loved.then_some(true),
        min_height: a.min_height,
        available: a.available.then_some(true),
        sort: a.sort.map(|s| match s {
            SortArg::Title => Sort::Title,
            SortArg::Added => Sort::Added,
            SortArg::Year => Sort::Year,
            SortArg::Runtime => Sort::Runtime,
        }),
        limit: a.limit,
        offset: a.offset,
    }
}

fn review(lib: &Library, cmd: ReviewCmd) -> Result<Value> {
    match cmd {
        ReviewCmd::List => to_json(lib.needs_review()?),
        ReviewCmd::Match {
            item_id,
            tmdb_id,
            kind,
        } => {
            let kind = match kind {
                MatchKind::Movie => ItemKind::Movie,
                MatchKind::Show => ItemKind::Show,
            };
            lib.apply_match(item_id, tmdb_id, kind).and_then(|_| ok())
        }
        ReviewCmd::Other { item_id } => lib.move_to_other(item_id).and_then(|_| ok()),
    }
}

fn tools(lib: &Library, cmd: ToolsCmd) -> Result<Value> {
    let tool = |t: ToolArg| match t {
        ToolArg::Mpv => Tool::Mpv,
        ToolArg::Ffprobe => Tool::Ffprobe,
        ToolArg::Ffmpeg => Tool::Ffmpeg,
    };
    match cmd {
        ToolsCmd::List => to_json(lib.detect_tools()?),
        ToolsCmd::Set { tool: t, path } => {
            lib.set_tool_path(tool(t), Some(&path)).and_then(|_| ok())
        }
        ToolsCmd::Clear { tool: t } => lib.set_tool_path(tool(t), None).and_then(|_| ok()),
    }
}
