//! One small function per command group; each returns the JSON to print.

use exhibit_core::download::DownloadEvent;
use exhibit_core::error::Detail;
use exhibit_core::filter::Filters;
use exhibit_core::identify::IdentifyScope;
use exhibit_core::model::{Id, ItemKind};
use exhibit_core::query::{SearchQuery, Shelf};
use exhibit_core::scan::ScanEvent;
use exhibit_core::service::{Service, SyncInput};
use exhibit_core::signin::SignInState;
use exhibit_core::tools::Tool;
use exhibit_core::{Error, Library, RefreshEvent, RefreshOptions, RefreshPhase, Result};
use serde::Serialize;
use serde_json::{Value, json};
use std::io::{BufRead, IsTerminal};
use std::time::Duration;

use crate::{
    Command, KeyCmd, KindArg, MatchKind, OnOff, ReviewCmd, SearchArgs, ServiceCmd, ShelfArg,
    SourceCmd, TasteCmd, ToolArg, ToolsCmd,
};

fn ok() -> Result<Value> {
    Ok(json!({"ok": true}))
}

fn to_json<T: Serialize>(v: T) -> Result<Value> {
    serde_json::to_value(v)
        .map_err(|e| Error::Invalid(format!("cannot serialise output: {e}").into()))
}

/// Writes one short line to stderr when it is a terminal.
fn progress(line: String) {
    if std::io::stderr().is_terminal() {
        eprintln!("{line}");
    }
}

pub fn run(lib: &Library, command: Command) -> Result<Value> {
    match command {
        Command::Status => to_json(lib.status()?),
        Command::Source(c) => source(lib, c),
        Command::Service(c) => service(lib, c),
        Command::Freshen { budget } => {
            to_json(lib.freshen(budget, Duration::from_millis(150), &mut |id| {
                progress(format!("refreshed item {id}"))
            })?)
        }
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
        Command::Search(args) => to_json(lib.search(&search_query(*args))?),
        Command::Facets { kind } => to_json(lib.facets(kind.map(kind_of))?),
        Command::Item { id } => to_json(lib.item(id)?),
        Command::Similar { id, limit } => to_json(lib.similar(id, limit)?),
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
        Command::Intro { state } => lib.set_intro(matches!(state, OnOff::On)).and_then(|_| ok()),
        Command::Key(KeyCmd::Set { name }) => {
            let stdin = std::io::stdin();
            if stdin.is_terminal() {
                eprintln!("Paste the {} key, then press Enter:", name.secret());
            }
            let value = read_secret(stdin.lock())?;
            lib.set_secret(name.secret(), Some(&value))?;
            ok()
        }
        Command::Key(KeyCmd::Clear { name }) => {
            lib.set_secret(name.secret(), None)?;
            ok()
        }
    }
}

/// The first line of `input`, trimmed; empty is `Error::Invalid`.
fn read_secret(mut input: impl BufRead) -> Result<String> {
    let mut line = String::new();
    input.read_line(&mut line)?;
    match line.trim() {
        "" => Err(Error::Invalid(
            "no key given: pipe or paste it on standard input".into(),
        )),
        value => Ok(value.to_string()),
    }
}

fn source(lib: &Library, cmd: SourceCmd) -> Result<Value> {
    match cmd {
        SourceCmd::Add { path } => to_json(lib.add_source(&path)?),
        SourceCmd::Remove { id } => lib.remove_source(id).and_then(|_| ok()),
        SourceCmd::List => to_json(lib.list_sources()?),
    }
}

/// The library's refresh: scan, identify, probe, identify the items left for review again,
/// thumbnails. Missing tools or a missing TMDB key skip that step quietly: its field is
/// `null`. A failed identification (a rejected key, no network) is `identify_error`; the
/// other steps still run and the command succeeds.
fn scan(lib: &Library, source: Option<Id>, no_probe: bool, no_identify: bool) -> Result<Value> {
    let opts = RefreshOptions {
        identify: !no_identify,
        probe: !no_probe,
    };
    let mut phase = RefreshPhase::Scanning;
    let r = lib.refresh(
        source,
        &mut |event| match event {
            RefreshEvent::Phase(p) => phase = p,
            RefreshEvent::Scan(ScanEvent::Started { source_id }) => {
                progress(format!("scanning source {source_id}"))
            }
            RefreshEvent::Scan(ScanEvent::FileAdded { rel_path, .. }) => {
                progress(format!("added {rel_path}"))
            }
            RefreshEvent::Scan(ScanEvent::Finished(r)) => {
                progress(format!("scanned source {}: {} added", r.source_id, r.added))
            }
            RefreshEvent::Item(id) if phase == RefreshPhase::Thumbnails => {
                progress(format!("thumbnail for item {id}"))
            }
            RefreshEvent::Item(id) if phase == RefreshPhase::Translating => {
                progress(format!("translated item {id}"))
            }
            RefreshEvent::Item(id) => progress(format!("identified item {id}")),
            // A failure's reason is printed whether or not stderr is a terminal.
            RefreshEvent::File {
                id,
                error: Some(why),
            } => {
                eprintln!("probing file {id} failed: {why}")
            }
            RefreshEvent::File { id, error: None } => progress(format!("probed file {id}")),
        },
        opts,
    )?;
    Ok(json!({
        "scanned": r.scanned,
        "identified": r.identified,
        "identify_error": r.identify_error,
        "probed": r.probed.map(|(ok, failed)| json!({"ok": ok, "failed": failed})),
        "rescored": r.rescored,
        "translated": r.translated,
        "thumbnails": r.thumbnails.map(|(made, failed)| json!({"made": made, "failed": failed})),
    }))
}

fn kind_of(k: KindArg) -> ItemKind {
    match k {
        KindArg::Movie => ItemKind::Movie,
        KindArg::Show => ItemKind::Show,
        KindArg::Other => ItemKind::Other,
        KindArg::Episode => ItemKind::Episode,
    }
}

fn search_query(a: SearchArgs) -> SearchQuery {
    let sort = a.sort;
    SearchQuery {
        text: a.text,
        kind: a.kind.map(kind_of),
        filters: Filters {
            genres: a.genre,
            any_genre: a.any_genre,
            without_genres: a.without_genre,
            keywords: a.keyword,
            any_keyword: a.any_keyword,
            people: a.person,
            year_min: a.year_min,
            year_max: a.year_max,
            runtime_min: a.runtime_min,
            runtime_max: a.runtime_max,
            seen: a.seen,
            loved: a.loved.then_some(true),
            available: a.available.then_some(true),
            rated: a.rated.then_some(true),
            reviewed: a.reviewed.then_some(true),
            logged: a.logged.then_some(true),
            watchlist: a.watchlist.then_some(true),
            resolution: a.resolution,
            picture: a.picture,
            sound: a.sound,
            audio: a.audio,
            subtitles: a.subtitles,
        },
        sort,
        descending: a.reverse.then(|| {
            !sort
                .unwrap_or(exhibit_core::query::Sort::Title)
                .descending()
        }),
        limit: a.limit,
        offset: a.offset,
        with_wanted: a.with_wanted,
    }
}

fn find_service(id: &str) -> Result<Box<dyn Service>> {
    exhibit_services::find(id).ok_or_else(|| {
        Error::Invalid(
            Detail::coded("service.unknown", format!("no service called \"{id}\""))
                .with("service", id),
        )
    })
}

fn service(lib: &Library, cmd: ServiceCmd) -> Result<Value> {
    match cmd {
        ServiceCmd::List => {
            let states = lib.service_states()?;
            let list: Vec<Value> = exhibit_services::all()
                .iter()
                .map(|s| {
                    let i = s.info();
                    let state = states.iter().find(|st| st.service == i.id);
                    json!({
                        "id": i.id,
                        "name": i.name,
                        "provides": i.provides,
                        "needs": i.needs,
                        "origins": i.origins.iter().map(|o| o.id).collect::<Vec<_>>(),
                        "state": state,
                    })
                })
                .collect();
            Ok(Value::Array(list))
        }
        ServiceCmd::User { id, name, clear } => {
            let svc = find_service(&id)?;
            match name.filter(|_| !clear) {
                None => lib.set_service_username(&id, None).and_then(|_| ok()),
                Some(name) => {
                    let checked = lib.check_service(&*svc, &name)?;
                    lib.set_service_username(&id, Some(&name))?;
                    let report = sync(lib, &*svc, SyncInput::Since(None))?;
                    Ok(json!({"checked": checked, "report": report}))
                }
            }
        }
        ServiceCmd::On { id } => {
            find_service(&id)?;
            lib.set_service_enabled(&id, true).and_then(|_| ok())
        }
        ServiceCmd::Off { id } => {
            find_service(&id)?;
            lib.set_service_enabled(&id, false).and_then(|_| ok())
        }
        ServiceCmd::Import { id, zip } => {
            let svc = find_service(&id)?;
            sync(lib, &*svc, SyncInput::Snapshot(&zip))
        }
        ServiceCmd::Sync { id } => {
            let svc = find_service(&id)?;
            sync(lib, &*svc, SyncInput::Since(None))
        }
        ServiceCmd::SignIn { id } => {
            let svc = find_service(&id)?;
            let code = lib.start_sign_in(&*svc)?;
            progress(format!(
                "Go to {} and enter {} (good for {} minutes)",
                code.verify_url,
                code.user_code,
                code.expires_in / 60
            ));
            let mut wait = code.interval;
            loop {
                std::thread::sleep(std::time::Duration::from_secs(wait));
                match lib.poll_sign_in(&*svc, &code.device_code)? {
                    SignInState::Pending => {}
                    SignInState::SlowDown => wait += 5,
                    SignInState::Done => break,
                }
            }
            let report =
                lib.refresh_reception(&*svc, &mut |n| progress(format!("asked about {n} works")))?;
            Ok(
                json!({"signed_in": lib.service_states()?.into_iter().find(|s| s.service == id).and_then(|s| s.username), "report": report}),
            )
        }
        ServiceCmd::Refresh { id } => {
            let svc = find_service(&id)?;
            to_json(
                lib.refresh_reception(&*svc, &mut |n| progress(format!("asked about {n} works")))?,
            )
        }
        ServiceCmd::Unmatched { id } => {
            find_service(&id)?;
            let films = lib.service_unmatched(&id)?;
            Ok(Value::Array(
                films
                    .into_iter()
                    .map(|(key, year)| json!({"key": key, "year": year}))
                    .collect(),
            ))
        }
    }
}

fn sync(lib: &Library, svc: &dyn Service, input: SyncInput) -> Result<Value> {
    to_json(lib.sync_activity(svc, input, &mut |id| progress(format!("matched item {id}")))?)
}

fn review(lib: &Library, cmd: ReviewCmd) -> Result<Value> {
    match cmd {
        ReviewCmd::List => to_json(lib.needs_review()?),
        ReviewCmd::Match {
            item_id,
            tmdb_id,
            kind,
        } => lib
            .apply_match(item_id, tmdb_id, item_kind(kind))
            .and_then(|_| ok()),
        ReviewCmd::Other { item_id } => lib.move_to_other(item_id).and_then(|_| ok()),
        ReviewCmd::Search { text, kind, year } => {
            to_json(lib.lookup(&text, item_kind(kind), year)?)
        }
    }
}

fn item_kind(kind: MatchKind) -> ItemKind {
    match kind {
        MatchKind::Movie => ItemKind::Movie,
        MatchKind::Show => ItemKind::Show,
    }
}

fn tools(lib: &Library, cmd: ToolsCmd) -> Result<Value> {
    let tool = |t: ToolArg| match t {
        ToolArg::Mpv => Tool::Mpv,
        ToolArg::Ffmpeg => Tool::Ffmpeg,
    };
    match cmd {
        ToolsCmd::List => to_json(lib.detect_tools()?),
        ToolsCmd::Set { tool: t, path } => {
            lib.set_tool_path(tool(t), Some(&path)).and_then(|_| ok())
        }
        ToolsCmd::Clear { tool: t } => lib.set_tool_path(tool(t), None).and_then(|_| ok()),
        ToolsCmd::Download => {
            let mut shown = 0;
            to_json(lib.download_tools(&mut |e| match e {
                DownloadEvent::Checking => progress("asking GitHub for the latest release".into()),
                DownloadEvent::Downloading { done, total } => {
                    // A line per tenth.
                    let tenth = done * 10 / total.max(1);
                    if tenth > shown {
                        shown = tenth;
                        progress(format!("downloaded {} of {} MB", done >> 20, total >> 20));
                    }
                }
                DownloadEvent::Unpacking => progress("unpacking".into()),
            })?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_secret_takes_one_trimmed_line() {
        let key = read_secret(&b"  abc123 \r\nsecond line\n"[..]).unwrap();
        assert_eq!(key, "abc123");
        for empty in [&b""[..], b"\n", b"   \r\n"] {
            assert!(matches!(read_secret(empty), Err(Error::Invalid(_))));
        }
    }
}
