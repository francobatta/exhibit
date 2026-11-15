//! Playback runs on its own thread and its own `Library` connection, so the window stays
//! responsive; when the player exits the window hears `play-ended` and comes back to front.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use exhibit_core::model::{Id, ItemKind};
use exhibit_core::player::PlayOutcome;
use exhibit_core::query::ItemDetail;
use exhibit_core::{Library, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// How long `play` waits for a failure that happens before the player starts.
const START_WAIT: Duration = Duration::from_millis(1500);

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlayEnded {
    pub outcome: Option<PlayOutcome>,
    pub error: Option<ApiError>,
    #[serde(rename = "nextEpisode")]
    pub next_episode: Option<Id>,
}

/// Starts playing `id` on its own thread. A failure within the first moments (the item's
/// drive is offline, no mpv) is returned; anything after that arrives with `play-ended`.
pub fn play(app: &AppHandle, id: Id, resume: bool) -> ApiResult<()> {
    let state = app.state::<AppState>();
    if state.playing.swap(true, Ordering::SeqCst) {
        return Err(ApiError::invalid("Something is already playing"));
    }
    let (tx, rx) = mpsc::sync_channel::<ApiResult<()>>(1);
    // Holds the sender while `play` still waits; the thread answers through it if so.
    let waiting = Arc::new(Mutex::new(Some(tx)));
    let w = waiting.clone();
    let app2 = app.clone();
    thread::spawn(move || {
        let result = Library::open_default().and_then(|lib| {
            let outcome = lib.play(id, resume)?;
            let next = next_after(&|i| lib.item(i), &outcome).unwrap_or_else(|e| {
                eprintln!("exhibit: cannot find the next episode: {e}");
                None
            });
            Ok((outcome, next))
        });
        let waiter = w.lock().unwrap_or_else(|e| e.into_inner()).take();
        match (result, waiter) {
            // Failed before the player started: the command reports it.
            (Err(e), Some(tx)) => {
                app2.state::<AppState>()
                    .playing
                    .store(false, Ordering::SeqCst);
                let _ = tx.send(Err(e.into()));
            }
            (result, waiter) => {
                if let Some(tx) = waiter {
                    let _ = tx.send(Ok(()));
                }
                let ended = match result {
                    Ok((outcome, next_episode)) => PlayEnded {
                        outcome: Some(outcome),
                        error: None,
                        next_episode,
                    },
                    Err(e) => PlayEnded {
                        outcome: None,
                        error: Some(e.into()),
                        next_episode: None,
                    },
                };
                finish(&app2, &ended);
            }
        }
    });
    match rx.recv_timeout(START_WAIT) {
        Ok(r) => r,
        Err(_) => {
            // Stop waiting; an answer sent just before this still counts.
            waiting.lock().unwrap_or_else(|e| e.into_inner()).take();
            rx.try_recv().unwrap_or(Ok(()))
        }
    }
}

/// The player exited: free the flag, tell the window, bring it back with keyboard focus.
fn finish(app: &AppHandle, ended: &PlayEnded) {
    let state = app.state::<AppState>();
    state.playing.store(false, Ordering::SeqCst);
    if let Err(e) = app.emit("play-ended", ended) {
        eprintln!("exhibit: cannot emit play-ended: {e}");
    }
    state.changed.notify();
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// The episode to offer next: when `outcome` is an episode that ended watched, its show's
/// next episode. `item` reads an item (normally `Library::item`).
pub fn next_after(
    item: &dyn Fn(Id) -> Result<ItemDetail>,
    outcome: &PlayOutcome,
) -> Result<Option<Id>> {
    if !outcome.watched {
        return Ok(None);
    }
    let mut d = item(outcome.item_id)?;
    if d.item.kind != ItemKind::Episode {
        return Ok(None);
    }
    // Episode, then season (when there is one), then show: two steps at most.
    for _ in 0..2 {
        match d.item.parent_id {
            Some(parent) if d.item.kind != ItemKind::Show => d = item(parent)?,
            _ => break,
        }
    }
    Ok(match d.item.kind {
        ItemKind::Show => d.next_episode.map(|e| e.id),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use exhibit_core::Error;
    use exhibit_core::model::{Item, MatchState};
    use exhibit_core::query::ItemSummary;

    fn summary(id: Id) -> ItemSummary {
        ItemSummary {
            id,
            kind: ItemKind::Episode,
            title: format!("Episode {id}"),
            year: None,
            season: Some(1),
            episode: Some(id as i32),
            show_title: Some("Show".into()),
            poster: None,
            backdrop: None,
            runtime_min: None,
            width: None,
            height: None,
            hdr: None,
            watched: false,
            progress: None,
            loved: false,
            available: true,
            match_state: MatchState::Matched,
        }
    }

    fn detail(id: Id, kind: ItemKind, parent_id: Option<Id>, next: Option<Id>) -> ItemDetail {
        ItemDetail {
            item: Item {
                id,
                kind,
                parent_id,
                title: format!("{kind:?} {id}"),
                year: None,
                season: None,
                episode: None,
                overview: None,
                runtime_min: None,
                genres: vec![],
                people: vec![],
                poster: None,
                backdrop: None,
                tmdb_id: None,
                match_state: MatchState::Matched,
                match_source: None,
                confidence: None,
                candidates: vec![],
                loved: false,
                added_at: 0,
            },
            files: vec![],
            play: None,
            children: vec![],
            available: true,
            next_episode: next.map(summary),
        }
    }

    /// Show 1 > season 2 > episodes 3 and 4 (4 unwatched); movie 9.
    fn library(id: Id) -> Result<ItemDetail> {
        match id {
            1 => Ok(detail(1, ItemKind::Show, None, Some(4))),
            2 => Ok(detail(2, ItemKind::Season, Some(1), None)),
            3 => Ok(detail(3, ItemKind::Episode, Some(2), None)),
            4 => Ok(detail(4, ItemKind::Episode, Some(2), None)),
            9 => Ok(detail(9, ItemKind::Movie, None, None)),
            _ => Err(Error::NotFound(format!("item {id}"))),
        }
    }

    fn outcome(item_id: Id, watched: bool) -> PlayOutcome {
        PlayOutcome {
            item_id,
            position_s: 0.0,
            duration_s: 100.0,
            watched,
        }
    }

    #[test]
    fn play_ended_names_next_episode() {
        assert_eq!(next_after(&library, &outcome(3, true)).unwrap(), Some(4));
        // Stopped half way: no next episode.
        assert_eq!(next_after(&library, &outcome(3, false)).unwrap(), None);
        // Not an episode.
        assert_eq!(next_after(&library, &outcome(9, true)).unwrap(), None);
        // The last episode, watched: the show has nothing next.
        let done = |id| {
            let mut d = library(id)?;
            d.next_episode = None;
            Ok(d)
        };
        assert_eq!(next_after(&done, &outcome(4, true)).unwrap(), None);
    }

    #[test]
    fn next_after_an_episode_without_a_show_is_none() {
        let orphan = |id| Ok(detail(id, ItemKind::Episode, None, None));
        assert_eq!(next_after(&orphan, &outcome(5, true)).unwrap(), None);
    }
}
