//! Playback runs on its own thread and its own `Library` connection, so the window stays
//! responsive; when the player exits the window hears `play-ended` and comes back to front.

use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard, mpsc};
use std::thread;
use std::time::Duration;

use exhibit_core::model::{Id, ItemKind};
use exhibit_core::player::PlayOutcome;
use exhibit_core::query::ItemDetail;
use exhibit_core::{Library, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::{ApiError, ApiResult, panic_message};
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

/// Decides who reports a play's result: the `play` command while it still waits, or
/// `play-ended` after. Exactly one of them reports an error; success always reaches
/// `play-ended` (and also releases a waiting command).
pub struct Handoff {
    waiter: Mutex<Option<mpsc::SyncSender<ApiResult<()>>>>,
    answer: Mutex<mpsc::Receiver<ApiResult<()>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Handoff {
    pub fn new() -> Handoff {
        let (tx, rx) = mpsc::sync_channel(1);
        Handoff {
            waiter: Mutex::new(Some(tx)),
            answer: Mutex::new(rx),
        }
    }

    /// Player side, once the result is known. Returns what is left for `play-ended`.
    /// The answer is sent while the lock is held, so `wait` can never stop waiting between
    /// this taking the sender and sending through it.
    pub fn deliver<T>(&self, result: ApiResult<T>) -> Option<ApiResult<T>> {
        let mut waiter = lock(&self.waiter);
        match (waiter.take(), result) {
            (Some(tx), Err(e)) => {
                let _ = tx.send(Err(e));
                None
            }
            (Some(tx), ok) => {
                let _ = tx.send(Ok(()));
                Some(ok)
            }
            (None, r) => Some(r),
        }
    }

    /// Command side: an error delivered within `wait` is returned; after that the command
    /// stops waiting and succeeds, and any later result belongs to `play-ended`.
    pub fn wait(&self, wait: Duration) -> ApiResult<()> {
        let answer = lock(&self.answer);
        if let Ok(r) = answer.recv_timeout(wait) {
            return r;
        }
        lock(&self.waiter).take();
        // A `deliver` that held the lock before us has already sent.
        answer.try_recv().unwrap_or(Ok(()))
    }
}

/// Starts playing `id` on its own thread. A failure within the first moments (the item's
/// drive is offline, no mpv) is returned; anything after that arrives with `play-ended`.
pub fn play(app: &AppHandle, id: Id, resume: bool) -> ApiResult<()> {
    let state = app.state::<AppState>();
    if state.playing.swap(true, Ordering::SeqCst) {
        return Err(ApiError::invalid("Something is already playing"));
    }
    let handoff = Arc::new(Handoff::new());
    let h = handoff.clone();
    let app2 = app.clone();
    thread::spawn(move || {
        let _guard = PanicGuard(app2.clone());
        let played = panic::catch_unwind(AssertUnwindSafe(|| played(id, resume)))
            .unwrap_or_else(|p| Err(stopped(&panic_message(p))));
        app2.state::<AppState>()
            .playing
            .store(false, Ordering::SeqCst);
        if let Some(result) = h.deliver(played) {
            finish(&app2, &ended(result));
        }
    });
    handoff.wait(START_WAIT)
}

/// Plays to the end and finds the episode to offer next.
fn played(id: Id, resume: bool) -> ApiResult<(PlayOutcome, Option<Id>)> {
    let lib = Library::open_default()?;
    let outcome = lib.play(id, resume)?;
    let next = next_after(&|i| lib.item(i), &outcome).unwrap_or_else(|e| {
        eprintln!("exhibit: cannot find the next episode: {e}");
        None
    });
    Ok((outcome, next))
}

fn stopped(why: &str) -> ApiError {
    std::io::Error::other(format!("playback stopped unexpectedly: {why}")).into()
}

fn ended(result: ApiResult<(PlayOutcome, Option<Id>)>) -> PlayEnded {
    match result {
        Ok((outcome, next_episode)) => PlayEnded {
            outcome: Some(outcome),
            error: None,
            next_episode,
        },
        Err(e) => PlayEnded {
            outcome: None,
            error: Some(e),
            next_episode: None,
        },
    }
}

/// A panic anywhere on the playback thread still frees the flag and tells the window.
struct PanicGuard(AppHandle);

impl Drop for PanicGuard {
    fn drop(&mut self) {
        if thread::panicking() {
            self.0
                .state::<AppState>()
                .playing
                .store(false, Ordering::SeqCst);
            let _ = self
                .0
                .emit("play-ended", ended(Err(stopped("see the log"))));
        }
    }
}

/// The player exited: tell the window, bring it back with keyboard focus.
fn finish(app: &AppHandle, ended: &PlayEnded) {
    if let Err(e) = app.emit("play-ended", ended) {
        eprintln!("exhibit: cannot emit play-ended: {e}");
    }
    app.state::<AppState>().changed.notify();
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

    fn failed() -> ApiResult<u32> {
        Err(ApiError::invalid("offline"))
    }

    #[test]
    fn an_early_error_is_returned_from_the_command() {
        let h = Handoff::new();
        assert_eq!(h.deliver(failed()), None);
        assert_eq!(h.wait(Duration::from_millis(10)), failed().map(|_| ()));
    }

    #[test]
    fn a_late_error_goes_to_play_ended() {
        let h = Handoff::new();
        assert_eq!(h.wait(Duration::from_millis(10)), Ok(()));
        assert_eq!(h.deliver(failed()), Some(failed()));
    }

    #[test]
    fn success_releases_the_command_and_reaches_play_ended() {
        let h = Handoff::new();
        assert_eq!(h.deliver(Ok(5)), Some(Ok(5)));
        assert_eq!(h.wait(Duration::from_millis(10)), Ok(()));
        let late = Handoff::new();
        assert_eq!(late.wait(Duration::from_millis(1)), Ok(()));
        assert_eq!(late.deliver(Ok(5)), Some(Ok(5)));
    }

    /// Racing the timeout: an error is always reported by exactly one side.
    #[test]
    fn an_error_is_reported_exactly_once_whatever_the_timing() {
        for i in 0..300u64 {
            let h = Arc::new(Handoff::new());
            let h2 = h.clone();
            let player = thread::spawn(move || {
                thread::sleep(Duration::from_micros(i * 7 % 1500));
                h2.deliver(failed())
            });
            let command = h.wait(Duration::from_millis(1));
            let ended = player.join().unwrap();
            assert!(
                command.is_err() != ended.is_some(),
                "round {i}: command {command:?}, play-ended {ended:?}"
            );
        }
    }
}
