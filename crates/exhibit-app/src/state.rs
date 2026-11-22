//! What every command reaches: the quick-call library, the worker, the playing flag.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::thread;
use std::time::Duration;

use exhibit_core::Library;
use tauri::{AppHandle, Emitter};

use crate::downloads::Downloads;
use crate::worker::WorkerHandle;

pub struct AppState {
    /// For quick calls only; long work opens its own connection.
    pub libs: Pool,
    pub worker: WorkerHandle,
    pub downloads: Downloads,
    pub playing: AtomicBool,
    pub image_dir: PathBuf,
    pub changed: Changed,
    /// The device code of each sign-in under way, by service id. It stays here, never in the
    /// window.
    pub sign_ins: Mutex<HashMap<String, String>>,
}

impl AppState {
    pub fn lib(&self) -> MutexGuard<'_, Library> {
        self.libs.get()
    }
}

/// A few connections for the quick calls. The window asks for several things at once (every
/// shelf on Home after a change), and SQLite reads them side by side under WAL; one shared
/// connection would answer them one after another.
pub struct Pool {
    libs: Vec<Mutex<Library>>,
    next: AtomicUsize,
}

/// Connections in the pool.
pub const POOL_SIZE: usize = 4;

impl Pool {
    /// `first`, and connections opened by `open` up to `POOL_SIZE`. A connection that cannot be
    /// opened leaves the pool smaller.
    pub fn new(first: Library, open: impl Fn() -> exhibit_core::Result<Library>) -> Pool {
        let mut libs = vec![Mutex::new(first)];
        libs.extend((1..POOL_SIZE).filter_map(|_| open().ok().map(Mutex::new)));
        Pool {
            libs,
            next: AtomicUsize::new(0),
        }
    }

    /// A free connection, or else waits for one, taking turns.
    pub fn get(&self) -> MutexGuard<'_, Library> {
        for lib in &self.libs {
            match lib.try_lock() {
                Ok(guard) => return guard,
                Err(TryLockError::Poisoned(e)) => return e.into_inner(),
                Err(TryLockError::WouldBlock) => {}
            }
        }
        let i = self.next.fetch_add(1, Ordering::Relaxed) % self.libs.len();
        self.libs[i].lock().unwrap_or_else(|e| e.into_inner())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testlib::Fixture;

    #[test]
    fn the_pool_hands_out_a_free_connection_while_one_is_held() {
        let f = Fixture::new(&["Heat.1995.mkv"]);
        let data = f._data.path().to_path_buf();
        let open = || {
            Library::open(
                data.clone(),
                Box::new(exhibit_core::volume::SystemVolumes::new()),
                Box::new(exhibit_core::secrets::MemorySecrets::default()),
                exhibit_core::tools::Env::default(),
            )
        };
        let pool = Pool::new(open().unwrap(), open);
        assert_eq!(pool.libs.len(), POOL_SIZE);
        let held: Vec<_> = (0..POOL_SIZE).map(|_| pool.get()).collect();
        // Every connection is out, each a different one, and each reads the library.
        for lib in &held {
            assert_eq!(lib.list_sources().unwrap().len(), 1);
        }
        drop(held);
        assert_eq!(pool.get().list_sources().unwrap().len(), 1);
    }
}
