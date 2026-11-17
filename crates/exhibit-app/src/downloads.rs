//! The mpv and ffmpeg download runs on its own thread and its own `Library`, apart from the
//! worker, so it never waits behind a refresh. One at a time; leaving the screen does not stop
//! it. Its status reaches the window as `tools-download`.

use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use exhibit_core::Library;
use exhibit_core::download::{DownloadEvent, DownloadReport};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::error::panic_message;
use crate::state::Changed;
use crate::worker::{Job, WorkerHandle};

/// How the most recent download ended.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Done {
        release: String,
        already_latest: bool,
    },
    Failed {
        message: String,
    },
}

/// Idle is `running: false`; `outcome` stays until the next download starts.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct DownloadStatus {
    pub running: bool,
    /// The current step while running.
    pub step: Option<DownloadEvent>,
    pub outcome: Option<Outcome>,
}

impl DownloadStatus {
    fn started() -> DownloadStatus {
        DownloadStatus {
            running: true,
            step: None,
            outcome: None,
        }
    }

    fn ended(r: &exhibit_core::Result<DownloadReport>) -> DownloadStatus {
        DownloadStatus {
            running: false,
            step: None,
            outcome: Some(match r {
                Ok(r) => Outcome::Done {
                    release: r.release.clone(),
                    already_latest: r.already_latest,
                },
                Err(e) => Outcome::Failed {
                    message: e.to_string(),
                },
            }),
        }
    }
}

/// Progress reaches the window at most this often within a step.
const EMIT_EVERY: Duration = Duration::from_millis(150);

#[derive(Clone, Default)]
pub struct Downloads {
    status: Arc<Mutex<DownloadStatus>>,
}

impl Downloads {
    pub fn status(&self) -> DownloadStatus {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn set(&self, s: DownloadStatus, app: &AppHandle) {
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = s.clone();
        if let Err(e) = app.emit("tools-download", &s) {
            eprintln!("exhibit: cannot emit tools-download: {e}");
        }
    }

    /// Starts a download; false when one is already running. A new download queues a refresh
    /// once it is in place, so files waiting for mpv are read.
    pub fn start(&self, app: AppHandle, worker: WorkerHandle, changed: Changed) -> bool {
        {
            let mut s = self.status.lock().unwrap_or_else(|e| e.into_inner());
            if s.running {
                return false;
            }
            *s = DownloadStatus::started();
        }
        let _ = app.emit("tools-download", self.status());
        let me = self.clone();
        thread::spawn(move || {
            let mut sent: Option<(Instant, std::mem::Discriminant<DownloadEvent>)> = None;
            let run = || {
                Library::open_default()?.download_tools(&mut |e| {
                    let kind = std::mem::discriminant(&e);
                    let due = sent.is_none_or(|(at, k)| k != kind || at.elapsed() >= EMIT_EVERY);
                    if due {
                        sent = Some((Instant::now(), kind));
                        me.set(
                            DownloadStatus {
                                step: Some(e),
                                ..DownloadStatus::started()
                            },
                            &app,
                        );
                    }
                })
            };
            let result = panic::catch_unwind(AssertUnwindSafe(run)).unwrap_or_else(|p| {
                Err(exhibit_core::Error::Invalid(format!(
                    "the download stopped unexpectedly: {}",
                    panic_message(p)
                )))
            });
            me.set(DownloadStatus::ended(&result), &app);
            if result.is_ok_and(|r| !r.already_latest) {
                changed.notify();
                worker.enqueue(Job::Refresh(None));
            }
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reads_as_the_window_expects() {
        let s = DownloadStatus {
            step: Some(DownloadEvent::Downloading { done: 5, total: 9 }),
            ..DownloadStatus::started()
        };
        assert_eq!(
            serde_json::to_value(&s).unwrap(),
            serde_json::json!({
                "running": true,
                "step": {"step": "downloading", "done": 5, "total": 9},
                "outcome": null,
            })
        );
        let failed = DownloadStatus::ended(&Err(exhibit_core::Error::Provider("down".into())));
        assert_eq!(
            serde_json::to_value(&failed).unwrap()["outcome"],
            serde_json::json!({"kind": "failed", "message": "provider error: down"})
        );
        let done = DownloadStatus::ended(&Ok(DownloadReport {
            release: "20261111".into(),
            arch: "x86_64".into(),
            already_latest: true,
            dir: "x".into(),
        }));
        assert_eq!(
            serde_json::to_value(&done).unwrap()["outcome"],
            serde_json::json!({"kind": "done", "release": "20261111", "already_latest": true})
        );
    }
}
