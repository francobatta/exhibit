#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod downloads;
mod error;
mod folders;
mod playback;
mod state;
#[cfg(test)]
mod testlib;
mod watch;
mod worker;

use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use exhibit_core::Library;
use tauri::window::Color;
use tauri::{App, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::state::{AppState, Changed};
use crate::worker::{Job, WorkerHandle};

/// The wall colour, shown before the page paints.
const WALL: Color = Color(0x0f, 0x11, 0x13, 0xff);

fn main() {
    // Development and screenshots: a plain 1600x900 window that remembers nothing.
    let windowed = std::env::var_os("EXHIBIT_WINDOWED").is_some_and(|v| v == "1");
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        // Paste in Settings and first run: WebView2's own clipboard asks with a prompt no pad can answer.
        .plugin(tauri_plugin_clipboard_manager::init());
    if !windowed {
        builder = builder.plugin(tauri_plugin_window_state::Builder::default().build());
    }
    builder
        .setup(move |app| setup(app, windowed))
        .invoke_handler(tauri::generate_handler![
            commands::boot,
            commands::status,
            commands::search,
            commands::item,
            commands::shelf,
            commands::needs_review,
            commands::history,
            commands::set_watched,
            commands::set_loved,
            commands::apply_match,
            commands::move_to_other,
            commands::lookup,
            commands::add_source,
            commands::remove_source,
            commands::list_folders,
            commands::detect_tools,
            commands::set_tool_path,
            commands::download_tools,
            commands::tools_download_status,
            commands::set_secret,
            commands::refresh,
            commands::identify,
            commands::job_status,
            commands::play,
            commands::window_fullscreen,
            commands::window_is_fullscreen,
        ])
        .run(tauri::generate_context!())
        .expect("Exhibit could not start its window");
}

fn setup(app: &mut App, windowed: bool) -> Result<(), Box<dyn std::error::Error>> {
    let lib = Library::open_default()?;
    let image_dir = lib.image_dir();
    std::fs::create_dir_all(&image_dir)?;
    // Artwork is the only thing the page may load from disk.
    app.asset_protocol_scope()
        .allow_directory(&image_dir, true)?;

    let handle = app.handle().clone();
    let changed = Changed::spawn(handle.clone());
    let worker = WorkerHandle::spawn(handle, changed.clone());
    worker.enqueue(Job::Refresh(None));
    watch::spawn(worker.clone(), changed.clone());
    app.manage(AppState {
        lib: Mutex::new(lib),
        worker,
        downloads: Default::default(),
        playing: AtomicBool::new(false),
        image_dir,
        changed,
    });

    let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
        .title("Exhibit")
        .background_color(WALL)
        .min_inner_size(960.0, 540.0);
    let window = if windowed {
        window.inner_size(1600.0, 900.0).center()
    } else {
        window.inner_size(1600.0, 900.0).fullscreen(true)
    };
    window.build()?;
    Ok(())
}
