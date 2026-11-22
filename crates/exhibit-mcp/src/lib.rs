//! Exhibit's MCP server: the library's operations as tools for an outside agent (Claude Code,
//! Claude Desktop, any MCP client), over stdio. It is a thin front end like the window and the
//! CLI: every tool calls `exhibit_core::Library`, so there is nothing here an agent can do that
//! the core does not check. Answers are compact JSON, with empty and false fields left out, so an
//! agent spends its tokens on the library and not on the wrapping.

use exhibit_core::exhibition::{
    Hang, MAX_NOTE, MAX_TITLE, MAX_WALL_TEXT, MAX_WORKS, MIN_WORKS, NewExhibition,
};
use exhibit_core::model::{Id, ItemKind, Person};
use exhibit_core::query::{ItemDetail, ItemSummary, SearchQuery, Shelf};
use exhibit_core::{Library, Result as CoreResult};
use rmcp::handler::server::router::prompt::PromptRouter;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, PromptMessage, Role, ServerCapabilities,
    ServerConfig,
};
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt, prompt, prompt_handler, prompt_router, tool,
    tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::sync::{Arc, Mutex, MutexGuard};

/// Runs the server on stdin and stdout until the client goes. `open` opens the library; it is
/// called again for each play, which runs mpv on a thread of its own.
pub fn serve(open: fn() -> CoreResult<Library>) -> std::io::Result<()> {
    let lib = open().map_err(std::io::Error::other)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let server = Exhibit::new(lib, open);
        let running = server
            .serve(rmcp::transport::stdio())
            .await
            .map_err(std::io::Error::other)?;
        running.waiting().await.map_err(std::io::Error::other)?;
        Ok(())
    })
}

const INSTRUCTIONS: &str = "Exhibit is the owner's home-theatre gallery of their own films and shows, played in mpv. \
These tools read and curate that library; none touches the media files. \
Every id comes from these tools: never make one up. Keywords are TMDB's, in English. \
Start with library_overview. To recommend, use search_library and similar_works and give the owner the facts they return as reasons. \
Writes are limited to: watched and loved marks, the taste profile, exhibitions (shelves on the owner's Home screen), \
matches in needs review, and play (starts mpv on the owner's screen; ask before playing).";

#[derive(Clone)]
pub struct Exhibit {
    lib: Arc<Mutex<Library>>,
    open: fn() -> CoreResult<Library>,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

impl Exhibit {
    pub fn new(lib: Library, open: fn() -> CoreResult<Library>) -> Self {
        Exhibit {
            lib: Arc::new(Mutex::new(lib)),
            open,
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }

    fn lib(&self) -> MutexGuard<'_, Library> {
        self.lib.lock().unwrap_or_else(|e| e.into_inner())
    }
}

// ---- Parameters. Their doc comments are what the agent reads. ----

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkKind {
    Movie,
    Show,
    /// Anything not confidently a film or a show (concerts, home videos).
    Other,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SeenArg {
    Unwatched,
    /// Started and not finished.
    Started,
    Watched,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortArg {
    Title,
    Year,
    /// When it came into the library.
    Added,
    Runtime,
    /// When it was last played.
    Played,
    /// The owner's own rating (from Letterboxd).
    MyRating,
    /// The public's scores (IMDb, Rotten Tomatoes, Metacritic, Letterboxd), averaged.
    Rating,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionArg {
    /// 4K.
    Uhd,
    /// 1080p.
    FullHd,
    /// 720p.
    Hd,
    Sd,
}

/// Every field is optional; an empty search lists the films, shows and other videos.
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct SearchParams {
    /// Words to find in titles (in any language the library holds), people, genres and descriptions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<WorkKind>,
    /// Genres as library_overview names them; all are required unless any_genre.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub genres: Vec<String>,
    #[serde(default)]
    pub any_genre: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub without_genres: Vec<String>,
    /// TMDB keywords in English ("heist", "neo-noir", "surveillance"); all are required unless any_keyword.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub any_keyword: bool,
    /// People by name, in any role (director, writer, cinematographer, composer, editor, cast); all are required.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub people: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year_min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year_max: Option<i32>,
    /// Minutes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_min: Option<i32>,
    /// Minutes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_max: Option<i32>,
    /// Any of these.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seen: Vec<SeenArg>,
    /// Only works the owner marked loved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loved: Option<bool>,
    /// Only works whose drive is connected now, so they can play.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available: Option<bool>,
    /// Any of these picture sizes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolution: Vec<ResolutionArg>,
    /// Languages it can be heard in, as library_overview names them ("en", "es").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio: Vec<String>,
    /// The owner rated it on a connected service (Letterboxd).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed: Option<bool>,
    /// Logged as watched on a connected service.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logged: Option<bool>,
    /// On the owner's watchlist and not watched yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watchlist: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<SortArg>,
    /// Each sort has its own natural direction (newest, best rated first); true or false overrides it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub descending: Option<bool>,
    /// At most this many (default 25, at most 100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
    /// Also list watchlist films the owner does not have (they cannot play).
    #[serde(default, rename = "with_wanted")]
    pub include_not_owned: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkId {
    /// A work's id, from search_library or another tool.
    pub id: Id,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SimilarParams {
    pub id: Id,
    /// At most this many (default 12).
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShelfArg {
    /// Started films and episodes, and the next episode of shows under way.
    ContinueWatching,
    RecentlyAdded,
    /// Films never started.
    Unwatched,
    Loved,
    /// The owner's watchlist from Letterboxd, owned films first.
    Watchlist,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShelfParams {
    pub shelf: ShelfArg,
    /// At most this many (default 20).
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LimitParams {
    /// At most this many (default 20).
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MarkParams {
    pub id: Id,
    pub on: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TasteParams {
    /// The whole profile, replacing the last: a few short paragraphs in plain words, at most about 200 words.
    pub text: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HangWork {
    /// A film or show the owner has, by id.
    pub id: Id,
    /// Why it hangs here, in a few words (at most 80 characters): shown under its poster.
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HangParams {
    /// Replace this exhibition; leave out to hang a new one (the oldest comes down beyond six).
    pub id: Option<Id>,
    /// At most 60 characters, a title for a gallery wall, not a sentence.
    pub title: String,
    /// The wall text: two or three sentences in a curator's plain voice, at most 600 characters.
    pub wall_text: String,
    /// 3 to 24 works, in the order they hang.
    pub works: Vec<HangWork>,
    /// Who curated it ("Claude"), shown as its curator.
    pub curator: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    Movie,
    Show,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MatchParams {
    /// The work in needs review.
    pub id: Id,
    /// TMDB's id of the right film or show, from its candidates.
    pub tmdb_id: i64,
    pub kind: MatchKind,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlayParams {
    pub id: Id,
    /// Resume where it was left; false starts from the beginning.
    #[serde(default = "yes")]
    pub resume: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CurateArgs {
    /// A theme to build around ("paranoia in the 1970s"); leave out to let the collection suggest one.
    pub theme: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecommendArgs {
    /// The owner's mood or request, in their words.
    pub mood: Option<String>,
    /// How many minutes they have.
    pub minutes: Option<u32>,
}

// ---- Answers. ----

/// The tool's answer: the value as compact JSON, or what went wrong as a tool error the agent can read.
fn reply(r: CoreResult<Value>) -> Result<CallToolResult, McpError> {
    Ok(match r {
        Ok(v) => CallToolResult::success(vec![ContentBlock::text(v.to_string())]),
        Err(e) => CallToolResult::error(vec![ContentBlock::text(e.to_string())]),
    })
}

/// `v` without nulls, falses, empty strings and empty lists, at any depth.
fn lean(v: Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.into_iter()
                .map(|(k, v)| (k, lean(v)))
                .filter(|(_, v)| {
                    !matches!(v, Value::Null | Value::Bool(false))
                        && v != &json!("")
                        && v != &json!([])
                        && v != &json!({})
                })
                .collect::<Map<_, _>>(),
        ),
        Value::Array(a) => Value::Array(a.into_iter().map(lean).collect()),
        v => v,
    }
}

/// A work in a list: what an agent needs to tell works apart and choose.
fn brief(s: &ItemSummary) -> Value {
    lean(json!({
        "id": s.id,
        "kind": s.kind,
        "title": s.title,
        "year": s.year,
        "show": s.show_title,
        "season": s.season,
        "episode": s.episode,
        "runtime_min": s.runtime_min,
        "genres": s.genres,
        "watched": s.watched,
        "progress": s.progress.filter(|p| *p > 0.0 && !s.watched).map(|p| (p * 100.0).round() / 100.0),
        "loved": s.loved,
        "offline": s.in_library && !s.available,
        "not_owned": !s.in_library,
    }))
}

fn names(people: &[Person], role: &str) -> Vec<String> {
    people
        .iter()
        .filter(|p| p.role == role)
        .map(|p| p.name.clone())
        .collect()
}

/// One work in full, for deciding and explaining.
fn full(d: &ItemDetail) -> Value {
    let i = &d.item;
    let cast: Vec<Value> = i
        .people
        .iter()
        .filter(|p| p.role == "cast")
        .map(|p| lean(json!({ "name": p.name, "as": p.character })))
        .collect();
    let files: Vec<Value> = d
        .files
        .iter()
        .filter_map(|f| f.tech.as_ref())
        .map(|t| {
            let langs = |l: Vec<Option<String>>| -> Vec<String> {
                let mut out: Vec<String> = l.into_iter().flatten().collect();
                out.dedup();
                out
            };
            lean(json!({
                "height": t.height,
                "hdr": t.hdr,
                "minutes": (t.duration_s / 60.0).round(),
                "audio": langs(t.audio.iter().map(|a| a.lang.clone()).collect()),
                "subtitles": langs(t.subtitles.iter().map(|s| s.lang.clone()).collect()),
            }))
        })
        .collect();
    let review = d.activity.reviews.first().map(|r| {
        let text: String = r.fact.text.chars().take(600).collect();
        json!({ "service": r.service, "text": text, "spoiler": r.fact.spoiler })
    });
    lean(json!({
        "id": i.id,
        "kind": i.kind,
        "title": i.title,
        "year": i.year,
        "runtime_min": i.runtime_min,
        "overview": i.overview,
        "genres": i.genres,
        "keywords": d.keywords,
        "director": names(&i.people, "director"),
        "writer": names(&i.people, "writer"),
        "cinematographer": names(&i.people, "cinematographer"),
        "composer": names(&i.people, "composer"),
        "editor": names(&i.people, "editor"),
        "cast": cast,
        "watched": d.activity.watched,
        "loved": d.activity.loved,
        "position_min": d.play.as_ref().filter(|p| !p.watched && p.position_s > 0.0).map(|p| (p.position_s / 60.0).round()),
        "last_played": d.play.as_ref().map(|p| p.last_played_at),
        "offline": !d.files.is_empty() && !d.available,
        "not_owned": d.files.is_empty() && matches!(i.kind, ItemKind::Movie),
        "scores": d.reception.iter().map(|s| json!({ "origin": s.origin, "value": s.value, "scale": s.scale })).collect::<Vec<_>>(),
        "awards": d.awards.iter().take(12).map(|a| lean(json!({ "body": a.body, "award": a.name, "year": a.year, "won": a.won }))).collect::<Vec<_>>(),
        "my_ratings": d.activity.ratings.iter().map(|r| json!({ "service": r.service, "value": r.fact.value, "scale": r.fact.scale })).collect::<Vec<_>>(),
        "my_review": review,
        "logged_on": d.activity.watches.iter().filter_map(|w| w.on).collect::<Vec<_>>(),
        "files": files,
        "seasons": d.children.iter().filter(|c| c.kind == ItemKind::Season).count(),
        "next_episode": d.next_episode.as_ref().map(brief),
    }))
}

fn list(items: &[ItemSummary]) -> Value {
    Value::Array(items.iter().map(brief).collect())
}

#[tool_router]
impl Exhibit {
    #[tool(
        description = "Start here. What the library holds: counts, the genres, keywords, directors and decades most present, what was watched lately, the owner's taste profile, the exhibitions hanging on Home, and which services are connected.",
        annotations(read_only_hint = true)
    )]
    fn library_overview(&self) -> Result<CallToolResult, McpError> {
        let lib = self.lib();
        reply((|| {
            let status = lib.status()?;
            let facets = lib.facets(None)?;
            let mut decades: Vec<(i32, u32)> = Vec::new();
            for y in &facets.years {
                let d = y.value / 10 * 10;
                match decades.last_mut() {
                    Some((at, n)) if *at == d => *n += y.count,
                    _ => decades.push((d, y.count)),
                }
            }
            let history = lib.history(10)?;
            let services: Vec<String> = lib
                .service_states()?
                .into_iter()
                .filter(|s| s.enabled)
                .map(|s| s.service)
                .collect();
            let top = |list: &[exhibit_core::filter::Count<String>], n: usize| -> Vec<Value> {
                list.iter()
                    .take(n)
                    .map(|c| json!([c.value, c.count]))
                    .collect()
            };
            Ok(lean(json!({
                "counts": { "movies": status.counts.movies, "shows": status.counts.shows, "other": status.counts.other, "needs_review": status.counts.needs_review },
                "language": status.language,
                "services": services,
                "genres": top(&facets.genres, 20),
                "keywords": top(&facets.keywords, 40),
                "directors": top(&lib.top_people("director", 15)?, 15),
                "decades": decades.iter().map(|(d, n)| json!([format!("{d}s"), n])).collect::<Vec<_>>(),
                "audio_languages": top(&facets.audio, 8),
                "recently_watched": history.iter().map(|h| brief(&h.item)).collect::<Vec<_>>(),
                "taste_profile": lib.taste_profile()?,
                "exhibitions": lib.exhibitions()?.iter().map(|e| json!({ "id": e.id, "title": e.title, "works": e.works.len() })).collect::<Vec<_>>(),
                "note": "Counts in lists are [value, how many works]."
            })))
        })())
    }

    #[tool(
        description = "Find works by text and filters (genres, TMDB keywords, people in any role, years, runtime, watched state, loved, availability, picture size, audio language, the owner's Letterboxd activity), sorted as asked. Answers brief records; use get_work for one in full.",
        annotations(read_only_hint = true)
    )]
    fn search_library(
        &self,
        Parameters(p): Parameters<SearchParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut q: SearchQuery = serde_json::from_value(
            serde_json::to_value(&p).map_err(|e| McpError::invalid_params(e.to_string(), None))?,
        )
        .map_err(|e| McpError::invalid_params(e.to_string(), None))?;
        q.limit = Some(q.limit.unwrap_or(25).min(100));
        reply(self.lib().search(&q).map(|r| list(&r)))
    }

    #[tool(
        description = "One work in full: description, genres, keywords, director, writer, cinematographer, composer, cast, public scores, awards, the owner's ratings and review, play state, files and languages, and for a show its next episode.",
        annotations(read_only_hint = true)
    )]
    fn get_work(&self, Parameters(p): Parameters<WorkId>) -> Result<CallToolResult, McpError> {
        reply(self.lib().item(p.id).map(|d| full(&d)))
    }

    #[tool(
        description = "The works in the library most like one, best first, each with what links them: TMDB recommends one after the other, the same person in the same role, shared keywords, shared genres. Scored offline.",
        annotations(read_only_hint = true)
    )]
    fn similar_works(
        &self,
        Parameters(p): Parameters<SimilarParams>,
    ) -> Result<CallToolResult, McpError> {
        reply(
            self.lib()
                .similar(p.id, p.limit.unwrap_or(12).min(50))
                .map(|r| {
                    Value::Array(
                        r.iter()
                            .map(|s| {
                                let mut v = brief(&s.item);
                                v["score"] = json!((s.score * 10.0).round() / 10.0);
                                v["because"] = serde_json::to_value(&s.because).unwrap_or_default();
                                v
                            })
                            .collect(),
                    )
                }),
        )
    }

    #[tool(
        description = "One of Home's shelves.",
        annotations(read_only_hint = true)
    )]
    fn list_shelf(
        &self,
        Parameters(p): Parameters<ShelfParams>,
    ) -> Result<CallToolResult, McpError> {
        let shelf = match p.shelf {
            ShelfArg::ContinueWatching => Shelf::ContinueWatching,
            ShelfArg::RecentlyAdded => Shelf::RecentlyAdded,
            ShelfArg::Unwatched => Shelf::Unwatched,
            ShelfArg::Loved => Shelf::Loved,
            ShelfArg::Watchlist => Shelf::Watchlist,
        };
        reply(
            self.lib()
                .shelf(shelf, p.limit.unwrap_or(20).min(100))
                .map(|r| list(&r)),
        )
    }

    #[tool(
        description = "What was played in Exhibit lately, newest first, with when (unix seconds).",
        annotations(read_only_hint = true)
    )]
    fn viewing_history(
        &self,
        Parameters(p): Parameters<LimitParams>,
    ) -> Result<CallToolResult, McpError> {
        reply(self.lib().history(p.limit.unwrap_or(20).min(200)).map(|h| {
            Value::Array(
                h.iter()
                    .map(|e| {
                        let mut v = brief(&e.item);
                        v["played_at"] = json!(e.started_at);
                        v
                    })
                    .collect(),
            )
        }))
    }

    #[tool(
        description = "The owner's taste profile: plain text kept by agents and readable and editable by the owner.",
        annotations(read_only_hint = true)
    )]
    fn get_taste_profile(&self) -> Result<CallToolResult, McpError> {
        reply(self.lib().taste_profile().map(|t| json!({ "text": t })))
    }

    #[tool(
        description = "Replace the owner's taste profile. Write what their history, loved works and ratings show, in plain words; never invent."
    )]
    fn set_taste_profile(
        &self,
        Parameters(p): Parameters<TasteParams>,
    ) -> Result<CallToolResult, McpError> {
        reply(
            self.lib()
                .set_taste_profile(p.text.trim())
                .map(|()| json!({ "saved": true })),
        )
    }

    #[tool(
        description = "The exhibitions hanging on the owner's Home screen, each with its works and notes.",
        annotations(read_only_hint = true)
    )]
    fn list_exhibitions(&self) -> Result<CallToolResult, McpError> {
        reply(self.lib().exhibitions().map(|all| {
            Value::Array(
                all.iter()
                    .map(|e| {
                        lean(json!({
                            "id": e.id,
                            "title": e.title,
                            "wall_text": e.wall_text,
                            "curator": e.curator,
                            "works": e.works.iter().map(|w| { let mut v = brief(&w.item); v["note"] = json!(w.note); v }).collect::<Vec<_>>(),
                        }))
                    })
                    .collect(),
            )
        }))
    }

    #[tool(
        description = "Hang an exhibition on the owner's Home screen: a title, a wall text and 3 to 24 films or shows the owner has, each with a short note. Give an id to replace that exhibition. Six hang at most; a new one takes down the oldest."
    )]
    fn hang_exhibition(
        &self,
        Parameters(p): Parameters<HangParams>,
    ) -> Result<CallToolResult, McpError> {
        let e = NewExhibition {
            id: p.id,
            title: p.title,
            wall_text: p.wall_text,
            works: p
                .works
                .into_iter()
                .map(|w| Hang {
                    id: w.id,
                    note: w.note,
                })
                .collect(),
            curator: p.curator,
        };
        reply(
            self.lib()
                .save_exhibition(&e)
                .map(|id| json!({ "id": id, "hung": true })),
        )
    }

    #[tool(description = "Take an exhibition down from Home.")]
    fn take_down_exhibition(
        &self,
        Parameters(p): Parameters<WorkId>,
    ) -> Result<CallToolResult, McpError> {
        reply(
            self.lib()
                .remove_exhibition(p.id)
                .map(|was| json!({ "taken_down": was })),
        )
    }

    #[tool(
        description = "Mark a work watched (on: true) or not watched (on: false). For a show, every episode.",
        annotations(idempotent_hint = true)
    )]
    fn set_watched(
        &self,
        Parameters(p): Parameters<MarkParams>,
    ) -> Result<CallToolResult, McpError> {
        reply(
            self.lib()
                .set_watched(p.id, p.on)
                .map(|()| json!({ "watched": p.on })),
        )
    }

    #[tool(
        description = "Mark a work loved (on: true) or not (on: false).",
        annotations(idempotent_hint = true)
    )]
    fn set_loved(&self, Parameters(p): Parameters<MarkParams>) -> Result<CallToolResult, McpError> {
        reply(
            self.lib()
                .set_loved(p.id, p.on)
                .map(|()| json!({ "loved": p.on })),
        )
    }

    #[tool(
        description = "Files Exhibit could not identify surely, each with its path and TMDB's best candidates. Resolve one with apply_match, or move it to Other with move_to_other; leave it when unsure.",
        annotations(read_only_hint = true)
    )]
    fn needs_review(&self) -> Result<CallToolResult, McpError> {
        reply(self.lib().needs_review().map(|all| {
            Value::Array(
                all.iter()
                    .map(|d| {
                        lean(json!({
                            "id": d.item.id,
                            "guess": d.item.title,
                            "year": d.item.year,
                            "path": d.files.first().map(|f| f.rel_path.clone()),
                            "minutes": d.files.first().and_then(|f| f.tech.as_ref()).map(|t| (t.duration_s / 60.0).round()),
                            "candidates": d.item.candidates.iter().take(5).map(|c| json!({ "tmdb_id": c.tmdb_id, "kind": c.kind, "title": c.title, "year": c.year, "score": (c.score * 100.0).round() / 100.0 })).collect::<Vec<_>>(),
                        }))
                    })
                    .collect(),
            )
        }))
    }

    #[tool(
        description = "Match a work in needs review to a TMDB film or show, for good. Only when the file's name, year and length agree with it."
    )]
    fn apply_match(
        &self,
        Parameters(p): Parameters<MatchParams>,
    ) -> Result<CallToolResult, McpError> {
        let kind = match p.kind {
            MatchKind::Movie => ItemKind::Movie,
            MatchKind::Show => ItemKind::Show,
        };
        reply(
            self.lib()
                .apply_match(p.id, p.tmdb_id, kind)
                .map(|()| json!({ "matched": true })),
        )
    }

    #[tool(
        description = "Move a work to Other: not a film or show (a concert, a home video, an extra)."
    )]
    fn move_to_other(&self, Parameters(p): Parameters<WorkId>) -> Result<CallToolResult, McpError> {
        reply(
            self.lib()
                .move_to_other(p.id)
                .map(|()| json!({ "moved": true })),
        )
    }

    #[tool(
        description = "Play a work in mpv on the owner's screen, resuming by default; for a show, its next episode. It answers at once; Exhibit records how far it got when mpv closes. Ask the owner first."
    )]
    fn play(&self, Parameters(p): Parameters<PlayParams>) -> Result<CallToolResult, McpError> {
        // A film that cannot play says why now, not from a thread no one hears.
        let detail = match self.lib().item(p.id) {
            Ok(d) => d,
            Err(e) => return reply(Err(e)),
        };
        if detail.files.is_empty() && detail.item.kind == ItemKind::Movie {
            return reply(Ok(
                json!({ "playing": false, "why": "the owner does not have this film" }),
            ));
        }
        if !detail.available {
            return reply(Ok(
                json!({ "playing": false, "why": "its drive is not connected" }),
            ));
        }
        let open = self.open;
        std::thread::spawn(move || {
            if let Err(e) = open().and_then(|lib| lib.play(p.id, p.resume)) {
                eprintln!("exhibit mcp: play {} failed: {e}", p.id);
            }
        });
        reply(Ok(json!({ "playing": true, "title": detail.item.title })))
    }
}

#[prompt_router]
impl Exhibit {
    /// Curate an exhibition from the owner's collection and hang it on Home.
    #[prompt(
        name = "curate_exhibition",
        description = "Curate a small exhibition from the owner's own films and shows and hang it on their Home screen."
    )]
    async fn curate_exhibition(&self, Parameters(a): Parameters<CurateArgs>) -> Vec<PromptMessage> {
        let theme = a.theme.map_or_else(
            || "a theme the collection itself suggests".to_string(),
            |t| format!("the theme \"{t}\""),
        );
        vec![PromptMessage::new_text(
            Role::User,
            format!(
                "Curate an exhibition for my Exhibit library around {theme}.\n\
                 1. Call library_overview and get_taste_profile.\n\
                 2. Explore with search_library (keywords, people, years) and similar_works. Use only works I have; prefer ones I have not watched, with a few I loved as anchors.\n\
                 3. Choose {MIN_WORKS} to 12 works (never more than {MAX_WORKS}) that belong together for a reason you can name from what the tools return.\n\
                 4. Write a title (at most {MAX_TITLE} characters, a gallery title, not a sentence), a wall text (two or three sentences, at most {MAX_WALL_TEXT} characters, plain words, no superlatives), and a note for each work (at most {MAX_NOTE} characters: why it hangs here).\n\
                 5. Call hang_exhibition with curator \"Claude\", then tell me what you hung and why, in a few lines."
            ),
        )]
    }

    /// Recommend what to watch now.
    #[prompt(
        name = "recommend",
        description = "Recommend a few works from the owner's library for right now, with reasons."
    )]
    async fn recommend(&self, Parameters(a): Parameters<RecommendArgs>) -> Vec<PromptMessage> {
        let mood = a
            .mood
            .map_or_else(String::new, |m| format!(" I'm in the mood for: {m}."));
        let time = a
            .minutes
            .map_or_else(String::new, |m| format!(" I have about {m} minutes."));
        vec![PromptMessage::new_text(
            Role::User,
            format!(
                "Recommend three things to watch from my Exhibit library.{mood}{time}\n\
                 Use library_overview, get_taste_profile, search_library (available: true, so they can play now) and similar_works. \
                 For each pick give the title, year and one line of why, from facts the tools returned. Do not play anything."
            ),
        )]
    }

    /// Write the owner's taste profile from their history.
    #[prompt(
        name = "update_taste_profile",
        description = "Write the owner's taste profile from what they watched, loved and rated."
    )]
    async fn update_taste_profile(&self) -> Vec<PromptMessage> {
        vec![PromptMessage::new_text(
            Role::User,
            "Update my taste profile in Exhibit. Read get_taste_profile, viewing_history, list_shelf loved, and search_library with rated: true sorted by my_rating; \
             look at a few of those works with get_work. Then call set_taste_profile with at most 200 words in plain sentences: \
             what I return to (directors, decades, kinds of story, keywords), what I rate low, and what I have not tried. Only what the data shows."
                .to_string(),
        )]
    }
}

#[tool_handler(router = self.tool_router)]
#[prompt_handler(router = self.prompt_router)]
impl ServerHandler for Exhibit {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
        )
        .with_server_info(Implementation::new("exhibit", env!("CARGO_PKG_VERSION")))
        .with_instructions(INSTRUCTIONS)
    }
}
