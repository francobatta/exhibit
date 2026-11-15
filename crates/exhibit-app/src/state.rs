//! What every command reaches: the quick-call library, the worker, the playing flag.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use exhibit_core::Library;
use tauri::{AppHandle, Emitter};

use crate::worker::WorkerHandle;

pub struct AppState {
    /// For quick calls only; long work opens its own connection.
    pub lib: Mutex<Library>,
    pub worker: WorkerHandle,
    pub playing: AtomicBool,
    pub image_dir: PathBuf,
    pub changed: Changed,
}

impl AppState {
    pub fn lib(&self) -> MutexGuard<'_, Library> {
        self.lib.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Tells the window the library changed: `library-changed`, at most once per 400 ms,
/// always followed by one more when changes arrived during the pause.
#[derive(Clone)]
pub struct Changed(mpsc::Sender<()>);

const COALESCE: Duration = Duration::from_millis(400);

impl Changed {
    pub fn spawn(app: AppHandle) -> Changed {
        let (tx, rx) = mpsc::channel::<()>();
        thread::spawn(move || {
            while rx.recv().is_ok() {
                loop {
                    if let Err(e) = app.emit("library-changed", ()) {
                        eprintln!("exhibit: cannot emit library-changed: {e}");
                    }
                    thread::sleep(COALESCE);
                    if rx.try_iter().count() == 0 {
                        break;
                    }
                }
            }
        });
        Changed(tx)
    }

    pub fn notify(&self) {
        // Fails only once the app is closing.
        let _ = self.0.send(());
    }
}
