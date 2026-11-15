//! `exhibit`: every command prints one JSON document to stdout. Errors print
//! `{"error": {"kind", "message"}}` and exit 1. Progress goes to stderr, only on a terminal.

mod commands;

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use exhibit_core::model::Id;
use exhibit_core::{Error, Library};
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
    /// Scan sources, identify (when a TMDB key is set), probe, retry items in review, then
    /// make thumbnails for other items
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
    /// Search TMDB by hand for a match; candidates are scored, best first
    Search {
        text: String,
        #[arg(long)]
        kind: MatchKind,
        #[arg(long)]
        year: Option<i32>,
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
    /// Reads the value from standard input (one line), so it stays out of shell history;
    /// it is never echoed back
    Set {
        name: KeyName,
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
    min_width: Option<u32>,
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
    let result = Library::open_default().and_then(|lib| commands::run(&lib, cli.command));
    let (value, code) = match result {
        Ok(v) => (v, ExitCode::SUCCESS),
        Err(e) => (error_json(&e), ExitCode::FAILURE),
    };
    let text = if std::io::stdout().is_terminal() {
        serde_json::to_string_pretty(&value)
    } else {
        serde_json::to_string(&value)
    };
    let mut out = std::io::stdout().lock();
    // A closed pipe (for example `| head`) is not an error worth a panic.
    let _ = writeln!(out, "{}", text.expect("JSON values serialise"));
    code
}

fn error_json(e: &Error) -> Value {
    json!({"error": {"kind": e.kind(), "message": e.to_string()}})
}
