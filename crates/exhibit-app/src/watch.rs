//! Notices what changed outside the window: a drive unplugged or plugged in, another
//! process writing the library, files appearing in a source folder.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use exhibit_core::model::{Id, Source};
use exhibit_core::{Library, Result};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::state::Changed;
use crate::worker::{Job, WorkerHandle};

const TICK: Duration = Duration::from_secs(3);
/// Files settle (a copy finishes) before their folder is refreshed.
const QUIET: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(500);

/// Starts the watcher thread on its own library connection.
pub fn spawn(worker: WorkerHandle, changed: Changed) {
    thread::spawn(move || {
        let lib = loop {
            match Library::open_default() {
                Ok(l) => break l,
                Err(e) => {
                    eprintln!("exhibit: watcher cannot open the library: {e}");
                    thread::sleep(TICK);
                }
            }
        };
        let (tx, rx) = mpsc::channel::<Id>();
        let mut folders: HashMap<Id, (PathBuf, Option<RecommendedWatcher>)> = HashMap::new();
        let mut settling: HashMap<Id, Instant> = HashMap::new();
        let mut prev: Option<Snapshot> = None;
        let mut next_tick = Instant::now();
        loop {
            if let Ok(id) = rx.recv_timeout(POLL) {
                settling.insert(id, Instant::now());
            }
            for id in rx.try_iter() {
                settling.insert(id, Instant::now());
            }
            settling.retain(|&id, last| {
                let quiet = last.elapsed() >= QUIET;
                if quiet {
                    worker.enqueue(Job::Refresh(Some(id)));
                }
                !quiet
            });
            if Instant::now() < next_tick {
                continue;
            }
            next_tick = Instant::now() + TICK;
            let (now, sources) = match snapshot(&lib) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("exhibit: watcher cannot read the library: {e}");
                    continue;
                }
            };
            if let Some(prev) = &prev {
                for change in diff(prev, &now) {
                    match change {
                        WatchChange::Changed => changed.notify(),
                        WatchChange::CameOnline(id) => {
                            worker.enqueue(Job::Refresh(Some(id)));
                        }
                        WatchChange::WentOffline(_) => {}
                    }
                }
            }
            prev = Some(now);
            follow(&mut folders, &sources, &tx);
        }
    });
}

fn snapshot(lib: &Library) -> Result<(Snapshot, Vec<Source>)> {
    let sources = lib.list_sources()?;
    let snap = Snapshot {
        version: lib.data_version()?,
        sources: sources.iter().map(|s| (s.id, s.online)).collect(),
    };
    Ok((snap, sources))
}

/// One recursive watcher per online source root; a root that cannot be watched (some
/// network shares) is tried again only once its path changes or it comes back online.
fn follow(
    folders: &mut HashMap<Id, (PathBuf, Option<RecommendedWatcher>)>,
    sources: &[Source],
    tx: &mpsc::Sender<Id>,
) {
    folders.retain(|id, (path, _)| {
        sources
            .iter()
            .any(|s| s.id == *id && s.online && s.path == *path)
    });
    for s in sources.iter().filter(|s| s.online) {
        if folders.contains_key(&s.id) {
            continue;
        }
        let (id, tx) = (s.id, tx.clone());
        let watcher = notify::recommended_watcher(move |ev: notify::Result<notify::Event>| {
            if ev.is_ok_and(|ev| !ev.kind.is_access()) {
                let _ = tx.send(id);
            }
        })
        .and_then(|mut w| w.watch(&s.path, RecursiveMode::Recursive).map(|_| w));
        if let Err(e) = &watcher {
            eprintln!("exhibit: cannot watch {}: {e}", s.path.display());
        }
        folders.insert(s.id, (s.path.clone(), watcher.ok()));
    }
}

/// What one tick saw: the database version and each source's online flag, by id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub version: i64,
    pub sources: Vec<(Id, bool)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchChange {
    /// Anything the window shows may differ.
    Changed,
    /// A known source is reachable now that was not: it needs a refresh.
    CameOnline(Id),
    WentOffline(Id),
}

pub fn diff(prev: &Snapshot, now: &Snapshot) -> Vec<WatchChange> {
    if prev == now {
        return Vec::new();
    }
    let was = |id: Id| prev.sources.iter().find(|(s, _)| *s == id).map(|s| s.1);
    let mut changes = vec![WatchChange::Changed];
    for &(id, online) in &now.sources {
        match (was(id), online) {
            (Some(false), true) => changes.push(WatchChange::CameOnline(id)),
            (Some(true), false) => changes.push(WatchChange::WentOffline(id)),
            _ => {}
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(version: i64, sources: &[(Id, bool)]) -> Snapshot {
        Snapshot {
            version,
            sources: sources.to_vec(),
        }
    }

    #[test]
    fn watch_tick_reports_source_going_offline() {
        let before = snap(4, &[(1, true), (2, true)]);
        let after = snap(4, &[(1, true), (2, false)]);
        assert_eq!(
            diff(&before, &after),
            [WatchChange::Changed, WatchChange::WentOffline(2)]
        );
        assert_eq!(
            diff(&after, &before),
            [WatchChange::Changed, WatchChange::CameOnline(2)]
        );
    }

    #[test]
    fn watch_tick_is_quiet_when_nothing_changed() {
        let s = snap(4, &[(1, true), (2, false)]);
        assert_eq!(diff(&s, &s.clone()), []);
    }

    #[test]
    fn watch_tick_reports_other_writers_and_new_sources() {
        let before = snap(4, &[(1, true)]);
        assert_eq!(
            diff(&before, &snap(5, &[(1, true)])),
            [WatchChange::Changed]
        );
        // A new source is refreshed by whoever added it, not by the watcher.
        assert_eq!(
            diff(&before, &snap(5, &[(1, true), (3, true)])),
            [WatchChange::Changed]
        );
        assert_eq!(diff(&before, &snap(5, &[])), [WatchChange::Changed]);
    }
}
