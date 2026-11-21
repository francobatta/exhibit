//! Connected services (Letterboxd): their commands, the thread that keeps them current, and
//! the finder for an export the owner has just downloaded.

use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

use exhibit_core::Library;
use exhibit_core::service::{BodyInfo, Need, OriginInfo, Provides, Service, ServiceInfo};
use exhibit_core::signin::SignInState;
use exhibit_core::sync::ServiceState;
use serde::Serialize;
use tauri::State;

use crate::error::ApiResult;
use crate::state::AppState;
use crate::worker::{Job, WorkerHandle, unknown_service};

/// What a service is and how the owner stands with it, for Settings.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceView {
    id: &'static str,
    name: &'static str,
    mark_svg: &'static str,
    provides: &'static [Provides],
    needs: &'static [Need],
    export_page: Option<&'static str>,
    /// Whose scores it reports, in the order they are shown.
    origins: &'static [OriginInfo],
    /// The award bodies it reports, most prominent first.
    bodies: &'static [BodyInfo],
    state: ServiceState,
}

/// A sign-in under way, as the window shows it: the code, where to enter it, and the address
/// that carries it drawn as a QR code for a phone.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignInView {
    user_code: String,
    verify_url: String,
    qr_svg: String,
    /// Seconds between asks.
    interval: u64,
    /// Seconds the code stays good.
    expires_in: u64,
}

/// An export file found in Downloads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FoundExport {
    pub path: String,
    pub name: String,
    /// Unix seconds.
    pub modified: i64,
}

fn registered(id: &str) -> ApiResult<Box<dyn Service>> {
    exhibit_services::find(id).ok_or_else(|| unknown_service(id))
}

fn view(info: &ServiceInfo, states: &[ServiceState]) -> ServiceView {
    // A service nobody has touched has no row yet; it starts on.
    let state = states
        .iter()
        .find(|s| s.service == info.id)
        .cloned()
        .unwrap_or_else(|| blank_state(info.id));
    ServiceView {
        id: info.id,
        name: info.name,
        mark_svg: info.mark_svg,
        provides: info.provides,
        needs: info.needs,
        export_page: info.export.as_ref().map(|e| e.page),
        origins: info.origins,
        bodies: info.bodies,
        state,
    }
}

fn blank_state(id: &str) -> ServiceState {
    ServiceState {
        service: id.to_string(),
        enabled: true,
        username: None,
        export_name: None,
        export_at: None,
        synced_at: None,
        error: None,
        unmatched: 0,
        remaining: None,
        paused_until: None,
    }
}

#[tauri::command(async)]
pub fn services(state: State<'_, AppState>) -> ApiResult<Vec<ServiceView>> {
    let states = state.lib().service_states()?;
    Ok(exhibit_services::all()
        .iter()
        .map(|s| view(s.info(), &states))
        .collect())
}

#[tauri::command(async)]
pub fn set_service_enabled(state: State<'_, AppState>, id: String, on: bool) -> ApiResult<()> {
    registered(&id)?;
    state.lib().set_service_enabled(&id, on)?;
    state.changed.notify();
    Ok(())
}

/// Asks the site whether the name is good (on its own connection: this waits on the network);
/// only then is it saved and the first sync queued. Returns how many recent entries it shows.
#[tauri::command(async)]
pub fn connect_service(state: State<'_, AppState>, id: String, username: String) -> ApiResult<u32> {
    let svc = registered(&id)?;
    let lib = Library::open_default()?;
    let seen = lib.check_service(&*svc, &username)?;
    lib.set_service_username(&id, Some(&username))?;
    state.worker.enqueue(Job::SyncService(id));
    state.changed.notify();
    Ok(seen)
}

/// Starts signing in to the service (on its own connection: this waits on the network).
#[tauri::command(async)]
pub fn start_sign_in(state: State<'_, AppState>, id: String) -> ApiResult<SignInView> {
    let svc = registered(&id)?;
    let code = Library::open_default()?.start_sign_in(&*svc)?;
    let link = code.verify_url_full.as_deref().unwrap_or(&code.verify_url);
    let view = SignInView {
        qr_svg: qr_svg(link),
        user_code: code.user_code,
        verify_url: code.verify_url,
        interval: code.interval,
        expires_in: code.expires_in,
    };
    state
        .sign_ins
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, code.device_code);
    Ok(view)
}

/// Whether the owner has approved the sign-in yet. On approval the scores are fetched.
#[tauri::command(async)]
pub fn poll_sign_in(state: State<'_, AppState>, id: String) -> ApiResult<SignInState> {
    let svc = registered(&id)?;
    let device_code = state
        .sign_ins
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .cloned()
        .ok_or_else(|| unknown_service(&id))?;
    let done = Library::open_default()?.poll_sign_in(&*svc, &device_code);
    if !matches!(done, Ok(SignInState::Pending | SignInState::SlowDown)) {
        state
            .sign_ins
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
    }
    if let Ok(SignInState::Done) = done {
        state.worker.enqueue(Job::Reception);
        state.changed.notify();
    }
    Ok(done?)
}

/// `text` as a QR code: an SVG of dark modules on nothing, drawn in the text colour.
fn qr_svg(text: &str) -> String {
    use qrcode::render::svg;
    match qrcode::QrCode::new(text.as_bytes()) {
        Ok(code) => code
            .render::<svg::Color>()
            .quiet_zone(false)
            .dark_color(svg::Color("currentColor"))
            .light_color(svg::Color("transparent"))
            .build(),
        Err(_) => String::new(),
    }
}

#[tauri::command(async)]
pub fn forget_service(state: State<'_, AppState>, id: String) -> ApiResult<()> {
    registered(&id)?;
    state.lib().set_service_username(&id, None)?;
    state.changed.notify();
    Ok(())
}

#[tauri::command(async)]
pub fn import_service_export(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> ApiResult<()> {
    registered(&id)?;
    state
        .worker
        .enqueue(Job::ImportService(id, PathBuf::from(path)));
    Ok(())
}

/// Opens the service's own export page in the browser. The address is the adapter's, never
/// the window's.
#[tauri::command(async)]
pub fn open_export_page(id: String) -> ApiResult<()> {
    let svc = registered(&id)?;
    let page = svc
        .info()
        .export
        .as_ref()
        .map(|e| e.page)
        .ok_or_else(|| unknown_service(&id))?;
    open_in_browser(page)?;
    Ok(())
}

/// The newest export of the service in Downloads that is newer than the last one imported.
#[tauri::command(async)]
pub fn find_service_export(
    state: State<'_, AppState>,
    id: String,
) -> ApiResult<Option<FoundExport>> {
    let svc = registered(&id)?;
    let Some(help) = svc.info().export.as_ref() else {
        return Ok(None);
    };
    let last = state
        .lib()
        .service_states()?
        .into_iter()
        .find(|s| s.service == id)
        .and_then(|s| s.export_at);
    Ok(downloads_dir().and_then(|dir| newest_export(&dir, help.file_prefix, help.file_ext, last)))
}

#[tauri::command(async)]
pub fn service_unmatched(
    state: State<'_, AppState>,
    id: String,
) -> ApiResult<Vec<(String, Option<i32>)>> {
    registered(&id)?;
    Ok(state.lib().service_unmatched(&id)?)
}

/// The newest file in `dir` (not below it) named `prefix...ext`, in any case, modified after
/// `newer_than` (unix seconds). Reads the folder only.
pub fn newest_export(
    dir: &Path,
    prefix: &str,
    ext: &str,
    newer_than: Option<i64>,
) -> Option<FoundExport> {
    let prefix = prefix.to_lowercase();
    let suffix = format!(".{}", ext.trim_start_matches('.').to_lowercase());
    let mut best: Option<FoundExport> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let lower = name.to_lowercase();
        if !(lower.starts_with(&prefix) && lower.ends_with(&suffix)) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let Some(modified) = meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
        else {
            continue;
        };
        if newer_than.is_some_and(|t| modified <= t) {
            continue;
        }
        if best.as_ref().is_none_or(|b| modified > b.modified) {
            best = Some(FoundExport {
                path: entry.path().to_string_lossy().to_string(),
                name,
                modified,
            });
        }
    }
    best
}

/// The owner's Downloads folder.
fn downloads_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    if let Some(dir) = known_downloads() {
        return Some(dir);
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join("Downloads"))
}

#[cfg(windows)]
fn known_downloads() -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath};

    let mut raw: *mut u16 = std::ptr::null_mut();
    // SAFETY: on success `raw` is a NUL-terminated wide string we own and free once; on failure
    // it is null or still freed by CoTaskMemFree, which accepts null.
    unsafe {
        let hr = SHGetKnownFolderPath(&FOLDERID_Downloads, 0, std::ptr::null_mut(), &mut raw);
        let dir = if hr >= 0 && !raw.is_null() {
            let mut len = 0;
            while *raw.add(len) != 0 {
                len += 1;
            }
            Some(PathBuf::from(std::ffi::OsString::from_wide(
                std::slice::from_raw_parts(raw, len),
            )))
        } else {
            None
        };
        CoTaskMemFree(raw.cast());
        dir
    }
}

#[cfg(windows)]
fn open_in_browser(url: &str) -> std::io::Result<()> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(Some(0)).collect() };
    let (verb, url) = (wide("open"), wide(url));
    // SAFETY: both strings are NUL-terminated and outlive the call.
    let rc = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            url.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // A value above 32 means it worked.
    if rc as usize > 32 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(windows))]
fn open_in_browser(url: &str) -> std::io::Result<()> {
    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
}

/// How often the services are asked for anything new.
const EVERY: Duration = Duration::from_secs(30 * 60);
/// The first refresh gets the worker to itself for a moment.
const FIRST: Duration = Duration::from_secs(20);

/// Queues a sync of every enabled activity service that has a username, a lookup of scores
/// and awards, and a freshening of TMDB's oldest details, a little after the first refresh and
/// then every half hour. The queue drops one already waiting.
pub fn spawn(worker: WorkerHandle) {
    thread::spawn(move || {
        thread::sleep(FIRST);
        loop {
            match Library::open_default().and_then(|l| l.service_states()) {
                Ok(states) => {
                    for id in due(&states) {
                        worker.enqueue(Job::SyncService(id));
                    }
                    worker.enqueue(Job::Reception);
                    worker.enqueue(Job::Freshen);
                }
                Err(e) => eprintln!("exhibit: cannot list the services: {e}"),
            }
            thread::sleep(EVERY);
        }
    });
}

/// The ids to sync on a timer.
fn due(states: &[ServiceState]) -> Vec<String> {
    states
        .iter()
        .filter(|s| s.enabled && s.username.is_some())
        .filter(|s| {
            exhibit_services::find(&s.service)
                .is_some_and(|svc| svc.info().provides.contains(&Provides::Activity))
        })
        .map(|s| s.service.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{File, FileTimes};
    use std::time::SystemTime;

    fn make(dir: &Path, name: &str, age_secs: u64) {
        let f = File::create(dir.join(name)).unwrap();
        let at = SystemTime::now() - Duration::from_secs(age_secs);
        f.set_times(FileTimes::new().set_modified(at)).unwrap();
    }

    fn ago(secs: i64) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - secs
    }

    #[test]
    fn find_export_picks_the_newest_matching_file_newer_than_the_last_import() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        make(d, "letterboxd-rian-2026-11-01-10-00-utc.zip", 5000);
        make(d, "Letterboxd-rian-2026-11-18-10-00-UTC.ZIP", 1000);
        make(d, "letterboxd-notes.txt", 10);
        make(d, "photos.zip", 10);
        std::fs::create_dir(d.join("letterboxd-folder.zip")).unwrap();
        std::fs::create_dir(d.join("sub")).unwrap();
        File::create(d.join("sub").join("letterboxd-deep.zip")).unwrap();

        let found = newest_export(d, "letterboxd-", "zip", None).unwrap();
        assert_eq!(found.name, "Letterboxd-rian-2026-11-18-10-00-UTC.ZIP");
        assert!((found.modified - ago(1000)).abs() < 5);
        assert!(found.path.ends_with(&found.name));

        // Newer than the last import: only a file after it counts.
        assert!(newest_export(d, "letterboxd-", "zip", Some(ago(500))).is_none());
        let found = newest_export(d, "letterboxd-", "zip", Some(ago(3000))).unwrap();
        assert_eq!(found.name, "Letterboxd-rian-2026-11-18-10-00-UTC.ZIP");
        // A missing folder is no export.
        assert!(newest_export(&d.join("gone"), "letterboxd-", "zip", None).is_none());
    }

    #[test]
    fn only_connected_enabled_activity_services_sync_on_the_timer() {
        let st = |service: &str, enabled, user: Option<&str>| ServiceState {
            enabled,
            username: user.map(Into::into),
            ..blank_state(service)
        };
        assert_eq!(due(&[st("letterboxd", true, Some("rian"))]), ["letterboxd"]);
        assert!(due(&[st("other", true, Some("x"))]).is_empty());
        assert!(due(&[st("letterboxd", false, Some("rian"))]).is_empty());
        assert!(due(&[st("letterboxd", true, None)]).is_empty());
    }

    #[test]
    fn the_export_page_is_the_adapters_own() {
        let svc = registered("letterboxd").unwrap();
        let page = svc.info().export.as_ref().unwrap().page;
        assert!(page.starts_with("https://"));
        assert!(registered("nope").is_err());
    }
}
