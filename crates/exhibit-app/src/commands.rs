//! One command per `Library` operation. Quick calls share the state's library; calls that
//! wait on the network open their own, so they never hold up the others.

use std::path::{Path, PathBuf};

use exhibit_core::identify::IdentifyScope;
use exhibit_core::model::{Candidate, Id, ItemKind, Source};
use exhibit_core::query::{HistoryEntry, ItemDetail, ItemSummary, SearchQuery, Shelf};
use exhibit_core::tools::{Tool, ToolStatus};
use exhibit_core::{Library, LibraryStatus};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::error::ApiResult;
use crate::folders::{self, Folder};
use crate::playback;
use crate::state::AppState;
use crate::worker::{Job, JobStatus};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Boot {
    status: LibraryStatus,
    image_dir: String,
    first_run: bool,
}

#[tauri::command(async)]
pub fn boot(state: State<'_, AppState>) -> ApiResult<Boot> {
    let status = state.lib().status()?;
    Ok(Boot {
        first_run: status.sources.is_empty(),
        image_dir: state.image_dir.to_string_lossy().to_string(),
        status,
    })
}

#[tauri::command(async)]
pub fn status(state: State<'_, AppState>) -> ApiResult<LibraryStatus> {
    Ok(state.lib().status()?)
}

#[tauri::command(async)]
pub fn search(state: State<'_, AppState>, query: SearchQuery) -> ApiResult<Vec<ItemSummary>> {
    Ok(state.lib().search(&query)?)
}

#[tauri::command(async)]
pub fn item(state: State<'_, AppState>, id: Id) -> ApiResult<ItemDetail> {
    Ok(state.lib().item(id)?)
}

#[tauri::command(async)]
pub fn shelf(state: State<'_, AppState>, shelf: Shelf, limit: u32) -> ApiResult<Vec<ItemSummary>> {
    Ok(state.lib().shelf(shelf, limit)?)
}

#[tauri::command(async)]
pub fn needs_review(state: State<'_, AppState>) -> ApiResult<Vec<ItemDetail>> {
    Ok(state.lib().needs_review()?)
}

#[tauri::command(async)]
pub fn history(state: State<'_, AppState>, limit: u32) -> ApiResult<Vec<HistoryEntry>> {
    Ok(state.lib().history(limit)?)
}

#[tauri::command(async)]
pub fn set_watched(state: State<'_, AppState>, id: Id, watched: bool) -> ApiResult<()> {
    state.lib().set_watched(id, watched)?;
    state.changed.notify();
    Ok(())
}

#[tauri::command(async)]
pub fn set_loved(state: State<'_, AppState>, id: Id, loved: bool) -> ApiResult<()> {
    state.lib().set_loved(id, loved)?;
    state.changed.notify();
    Ok(())
}

#[tauri::command(async)]
pub fn apply_match(
    state: State<'_, AppState>,
    item_id: Id,
    tmdb_id: i64,
    kind: ItemKind,
) -> ApiResult<()> {
    Library::open_default()?.apply_match(item_id, tmdb_id, kind)?;
    state.changed.notify();
    Ok(())
}

#[tauri::command(async)]
pub fn move_to_other(state: State<'_, AppState>, id: Id) -> ApiResult<()> {
    state.lib().move_to_other(id)?;
    state.changed.notify();
    Ok(())
}

#[tauri::command(async)]
pub fn lookup(text: String, kind: ItemKind, year: Option<i32>) -> ApiResult<Vec<Candidate>> {
    Ok(Library::open_default()?.lookup(&text, kind, year)?)
}

/// Adds the folder and queues its first refresh.
#[tauri::command(async)]
pub fn add_source(state: State<'_, AppState>, path: String) -> ApiResult<Source> {
    let source = state.lib().add_source(Path::new(&path))?;
    state.changed.notify();
    state.worker.enqueue(Job::Refresh(Some(source.id)));
    Ok(source)
}

#[tauri::command(async)]
pub fn remove_source(state: State<'_, AppState>, id: Id) -> ApiResult<()> {
    state.lib().remove_source(id)?;
    state.changed.notify();
    Ok(())
}

#[tauri::command(async)]
pub fn list_folders(path: Option<String>) -> ApiResult<Vec<Folder>> {
    Ok(folders::list_folders(path.as_deref().map(Path::new))?)
}

#[tauri::command(async)]
pub fn detect_tools(state: State<'_, AppState>) -> ApiResult<Vec<ToolStatus>> {
    Ok(state.lib().detect_tools()?)
}

#[tauri::command(async)]
pub fn set_tool_path(
    state: State<'_, AppState>,
    tool: Tool,
    path: Option<String>,
) -> ApiResult<()> {
    state
        .lib()
        .set_tool_path(tool, path.map(PathBuf::from).as_deref())?;
    state.changed.notify();
    Ok(())
}

/// The value goes to the credential store and is never echoed back.
#[tauri::command(async)]
pub fn set_secret(
    state: State<'_, AppState>,
    name: String,
    value: Option<String>,
) -> ApiResult<()> {
    state.lib().set_secret(&name, value.as_deref())?;
    state.changed.notify();
    Ok(())
}

#[tauri::command(async)]
pub fn refresh(state: State<'_, AppState>, source: Option<Id>) {
    state.worker.enqueue(Job::Refresh(source));
}

#[tauri::command(async)]
pub fn identify(state: State<'_, AppState>, scope: IdentifyScope) {
    state.worker.enqueue(Job::Identify(scope));
}

#[tauri::command(async)]
pub fn job_status(state: State<'_, AppState>) -> JobStatus {
    state.worker.status()
}

#[tauri::command(async)]
pub fn play(app: AppHandle, id: Id, resume: bool) -> ApiResult<()> {
    playback::play(&app, id, resume)
}

#[tauri::command(async)]
pub fn window_fullscreen(app: AppHandle, on: bool) -> ApiResult<()> {
    if let Some(w) = app.get_webview_window("main") {
        w.set_fullscreen(on)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
    }
    Ok(())
}
