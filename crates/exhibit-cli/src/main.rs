//! `exhibit`: every command prints one JSON document to stdout. Errors print
//! `{"error": {"kind", "message"}}` and exit 1. Progress goes to stderr, only on a terminal.

mod commands;

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use exhibit_core::filter::{Picture, Resolution, Seen, Sound};
use exhibit_core::model::Id;
use exhibit_core::query::Sort;
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
    /// Serve the library to an outside agent over MCP, on stdin and stdout, until it goes
    Mcp {
        /// Start this exe as an MCP client would and say whether the server answers
        #[arg(long, conflicts_with = "setup")]
        check: bool,
        /// Write the instructions that set an AI agent up with this server for good, and print
        /// the line to give the agent (the window's "Copy setup prompt")
        #[arg(long)]
        setup: bool,
    },
    #[command(subcommand)]
    Source(SourceCmd),
    #[command(subcommand)]
    Service(ServiceCmd),
    /// Fetch TMDB's details of the oldest works again (TMDB's data is kept six months at most)
    Freshen {
        /// At most about this many requests
        #[arg(long, default_value_t = 80)]
        budget: u32,
    },
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
    Search(Box<SearchArgs>),
    /// What a listing can be narrowed by: genres, keywords, years, runtimes, picture, sound and
    /// languages
    Facets {
        #[arg(long)]
        kind: Option<KindArg>,
    },
    Item {
        id: Id,
    },
    /// The works most like one in the library, with what links each (director, keywords,
    /// TMDB's recommendation)
    Similar {
        id: Id,
        #[arg(long, default_value_t = 20)]
        limit: u32,
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
    /// Whether the window plays the intro as it opens
    Intro {
        state: OnOff,
    },
}

#[derive(Subcommand)]
enum SourceCmd {
    Add { path: PathBuf },
    Remove { id: Id },
    List,
}

/// Outside services whose facts about the owner's films Exhibit can show.
#[derive(Subcommand)]
enum ServiceCmd {
    /// Each service: what it provides and needs, and its state
    List,
    /// Set the account name (checked first, then synced), or forget the service with --clear
    User {
        id: String,
        #[arg(required_unless_present = "clear", conflicts_with = "clear")]
        name: Option<String>,
        #[arg(long)]
        clear: bool,
    },
    On {
        id: String,
    },
    Off {
        id: String,
    },
    /// Read the service's export (a zip) as its snapshot
    Import {
        id: String,
        zip: PathBuf,
    },
    /// Ask the service for what is new
    Sync {
        id: String,
    },
    /// Films the matcher could not place surely: their key and year
    Unmatched {
        id: String,
    },
    /// Sign in with a code you approve on the site, from any device
    SignIn {
        id: String,
    },
    /// Ask the service for the scores or awards of the works that are due
    Refresh {
        id: String,
    },
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
    Set {
        tool: ToolArg,
        path: PathBuf,
    },
    Clear {
        tool: ToolArg,
    },
    /// Download the latest mpv and ffmpeg for this PC from shinchiro's builds (unofficial,
    /// listed first on mpv.io) into Exhibit's tools folder; Windows only
    Download,
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
    /// Repeat for more; all are required unless --any-genre
    #[arg(long)]
    genre: Vec<String>,
    #[arg(long)]
    any_genre: bool,
    #[arg(long)]
    without_genre: Vec<String>,
    /// A TMDB keyword ("heist"); repeat for more; all are required unless --any-keyword
    #[arg(long)]
    keyword: Vec<String>,
    #[arg(long)]
    any_keyword: bool,
    /// Someone in the work, in any role (director, writer, cinematographer, composer, cast);
    /// repeat for more, all required
    #[arg(long)]
    person: Vec<String>,
    #[arg(long)]
    year_min: Option<i32>,
    #[arg(long)]
    year_max: Option<i32>,
    /// Minutes
    #[arg(long)]
    runtime_min: Option<i32>,
    #[arg(long)]
    runtime_max: Option<i32>,
    /// unwatched, started or watched; repeat for more
    #[arg(long, value_parser = named::<Seen>)]
    seen: Vec<Seen>,
    #[arg(long)]
    loved: bool,
    #[arg(long)]
    available: bool,
    /// Only films you have rated on a service
    #[arg(long)]
    rated: bool,
    #[arg(long)]
    reviewed: bool,
    /// Only films a service has logged as watched
    #[arg(long)]
    logged: bool,
    /// Only films on a service's watchlist
    #[arg(long)]
    watchlist: bool,
    /// Also list watchlisted films that have no file
    #[arg(long)]
    with_wanted: bool,
    /// uhd, full_hd, hd or sd; repeat for more
    #[arg(long, value_parser = named::<Resolution>)]
    resolution: Vec<Resolution>,
    /// dolby_vision, hdr10, hlg or sdr; repeat for more
    #[arg(long, value_parser = named::<Picture>)]
    picture: Vec<Picture>,
    /// atmos, dts_x, lossless or surround; repeat for more
    #[arg(long, value_parser = named::<Sound>)]
    sound: Vec<Sound>,
    /// A language a work can be heard in, as `facets` names it ("en"); repeat for more
    #[arg(long)]
    audio: Vec<String>,
    #[arg(long)]
    subtitles: Vec<String>,
    /// title, year, added, runtime, played, my-rating or rating (the public's)
    #[arg(long, value_parser = sort_named)]
    sort: Option<Sort>,
    /// Against the sort's own direction (titles A to Z, runtimes shortest first, the rest newest first)
    #[arg(long)]
    reverse: bool,
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

/// Parses a value by its name in the core's JSON (`full_hd`, `dts_x`).
fn named<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, String> {
    serde_json::from_value(Value::String(s.into())).map_err(|e| e.to_string())
}

/// A sort by its JSON name, with `-` allowed for `_` (`my-rating`).
fn sort_named(s: &str) -> Result<Sort, String> {
    named(&s.replace('-', "_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("exhibit").chain(args.iter().copied()))
    }

    #[test]
    fn search_takes_the_service_flags_and_my_rating() {
        let cli = parse(&[
            "search",
            "--rated",
            "--reviewed",
            "--logged",
            "--watchlist",
            "--with-wanted",
            "--sort",
            "my-rating",
        ])
        .unwrap();
        let Command::Search(a) = cli.command else {
            panic!("not a search")
        };
        assert!(a.rated && a.reviewed && a.logged && a.watchlist && a.with_wanted);
        assert_eq!(a.sort, Some(Sort::MyRating));
        assert!(parse(&["search", "--sort", "my_rating"]).is_ok());
        assert!(parse(&["search", "--sort", "nope"]).is_err());
    }

    #[test]
    fn service_user_wants_a_name_or_clear() {
        assert!(parse(&["service", "user", "letterboxd", "someone"]).is_ok());
        assert!(parse(&["service", "user", "letterboxd", "--clear"]).is_ok());
        assert!(parse(&["service", "user", "letterboxd"]).is_err());
        assert!(parse(&["service", "user", "letterboxd", "x", "--clear"]).is_err());
        assert!(parse(&["service", "import", "letterboxd", "a.zip"]).is_ok());
        assert!(parse(&["service", "list"]).is_ok());
        assert!(parse(&["service", "sign-in", "mdblist"]).is_ok());
        assert!(parse(&["service", "refresh", "wikidata"]).is_ok());
        assert!(parse(&["search", "--sort", "rating"]).is_ok());
    }
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
    Ffmpeg,
}

#[derive(Clone, Copy, ValueEnum)]
enum OnOff {
    On,
    Off,
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
    if let Command::Mcp { check, setup } = cli.command {
        let flag = if check {
            Some("--check")
        } else if setup {
            Some("--setup")
        } else {
            None
        };
        let code = exhibit_mcp::main(flag, Library::open_default, &exhibit_core::db::data_dir());
        return ExitCode::from(u8::try_from(code).unwrap_or(1));
    }
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
