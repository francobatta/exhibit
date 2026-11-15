//! Long jobs (refresh, identify) run one at a time on their own thread and their own
//! `Library` connection, so quick commands never wait behind them.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use exhibit_core::identify::IdentifyScope;
use exhibit_core::model::Id;
use exhibit_core::scan::ScanEvent;
use exhibit_core::{Library, RefreshEvent, RefreshOptions, RefreshPhase};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::state::Changed;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Job {
    /// One source, or every source with `None`.
    Refresh(Option<Id>),
    Identify(IdentifyScope),
}

/// What the worker is doing. Idle is `phase: None` with `done: 0` and no message;
/// `last_error` keeps the failure of the most recent job until the next job ends.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct JobStatus {
    pub phase: Option<RefreshPhase>,
    /// Files or items handled so far in the current phase.
    pub done: u32,
    pub message: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Emit {
    Status(JobStatus),
    Changed,
}

/// Runs one job to its end, reporting progress through `emit`. Never fails: a failure
/// ends up in the final status's `last_error`.
pub fn run_job(lib: &Library, job: Job, emit: &mut dyn FnMut(Emit)) {
    let mut status = JobStatus::default();
    let last_error = match job {
        Job::Refresh(source) => {
            let mut on = |e: RefreshEvent| match e {
                RefreshEvent::Phase(p) => {
                    if status.phase.is_some() {
                        emit(Emit::Changed);
                    }
                    status.phase = Some(p);
                    status.done = 0;
                    status.message = Some(phase_message(p).to_string());
                    emit(Emit::Status(status.clone()));
                }
                RefreshEvent::Scan(ScanEvent::FileAdded { .. })
                | RefreshEvent::Item(_)
                | RefreshEvent::File { .. } => {
                    status.done += 1;
                    emit(Emit::Status(status.clone()));
                }
                RefreshEvent::Scan(_) => {}
            };
            match lib.refresh(source, &mut on, RefreshOptions::default()) {
                Ok(report) => report.identify_error,
                Err(e) => Some(e.to_string()),
            }
        }
        Job::Identify(scope) => {
            let phase = RefreshPhase::Identifying;
            status.phase = Some(phase);
            status.message = Some(phase_message(phase).to_string());
            emit(Emit::Status(status.clone()));
            let found = lib.identify(scope, &mut |_| {
                status.done += 1;
                emit(Emit::Status(status.clone()));
            });
            match found {
                Ok(report) => report.errors.last().cloned(),
                Err(e) => Some(e.to_string()),
            }
        }
    };
    emit(Emit::Status(JobStatus {
        last_error,
        ..JobStatus::default()
    }));
    emit(Emit::Changed);
}

/// What the owner reads while a phase runs.
fn phase_message(p: RefreshPhase) -> &'static str {
    match p {
        RefreshPhase::Scanning => "Looking for new and changed files",
        RefreshPhase::Identifying => "Identifying titles",
        RefreshPhase::Probing => "Reading technical details",
        RefreshPhase::Rescoring => "Identifying titles again with their running times",
        RefreshPhase::Thumbnails => "Making thumbnails",
    }
}

/// Jobs waiting for the worker. A job identical to one already waiting is dropped.
#[derive(Default)]
pub struct Queue {
    jobs: Mutex<VecDeque<Job>>,
    ready: Condvar,
}

impl Queue {
    /// False when an identical job was already waiting.
    pub fn push(&self, job: Job) -> bool {
        let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        if jobs.contains(&job) {
            return false;
        }
        jobs.push_back(job);
        self.ready.notify_one();
        true
    }

    /// Waits for the next job.
    pub fn pop(&self) -> Job {
        let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(job) = jobs.pop_front() {
                return job;
            }
            jobs = self.ready.wait(jobs).unwrap_or_else(|e| e.into_inner());
        }
    }
}

#[derive(Clone)]
pub struct WorkerHandle {
    queue: Arc<Queue>,
    status: Arc<Mutex<JobStatus>>,
}

/// Progress reaches the window at most this often within a phase.
const STATUS_EVERY: Duration = Duration::from_millis(150);

impl WorkerHandle {
    /// Starts the worker thread. It opens its own library on the first job.
    pub fn spawn(app: AppHandle, changed: Changed) -> WorkerHandle {
        let handle = WorkerHandle {
            queue: Arc::default(),
            status: Arc::default(),
        };
        let h = handle.clone();
        thread::spawn(move || {
            let mut lib: Option<Library> = None;
            let mut sent: Option<(Instant, Option<RefreshPhase>)> = None;
            loop {
                let job = h.queue.pop();
                let mut sink = |e: Emit| match e {
                    Emit::Changed => changed.notify(),
                    Emit::Status(s) => {
                        let s = h.store(s);
                        let due = match sent {
                            Some((at, phase)) => {
                                phase != s.phase
                                    || s.phase.is_none()
                                    || at.elapsed() >= STATUS_EVERY
                            }
                            None => true,
                        };
                        if due {
                            sent = Some((Instant::now(), s.phase));
                            if let Err(e) = app.emit("job-status", &s) {
                                eprintln!("exhibit: cannot emit job-status: {e}");
                            }
                        }
                    }
                };
                if lib.is_none() {
                    match Library::open_default() {
                        Ok(l) => lib = Some(l),
                        Err(e) => {
                            sink(Emit::Status(JobStatus {
                                last_error: Some(e.to_string()),
                                ..JobStatus::default()
                            }));
                            continue;
                        }
                    }
                }
                if let Some(lib) = &lib {
                    run_job(lib, job, &mut sink);
                }
            }
        });
        handle
    }

    /// False when the same job was already waiting.
    pub fn enqueue(&self, job: Job) -> bool {
        self.queue.push(job)
    }

    pub fn status(&self) -> JobStatus {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Keeps the previous job's error visible while the next one runs.
    fn store(&self, mut s: JobStatus) -> JobStatus {
        let mut cur = self.status.lock().unwrap_or_else(|e| e.into_inner());
        if s.phase.is_some() && s.last_error.is_none() {
            s.last_error = cur.last_error.clone();
        }
        *cur = s.clone();
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testlib::Fixture;

    #[test]
    fn run_job_refresh_emits_phases_then_changed() {
        let f = Fixture::new(&["Heat.1995.mkv", "Alien.1979.mkv"]);
        let mut emits = Vec::new();
        run_job(&f.lib, Job::Refresh(None), &mut |e| emits.push(e));

        let phases: Vec<Option<RefreshPhase>> = emits
            .iter()
            .filter_map(|e| match e {
                Emit::Status(s) => Some(s.phase),
                Emit::Changed => None,
            })
            .collect();
        assert_eq!(phases.first(), Some(&Some(RefreshPhase::Scanning)));
        assert_eq!(phases.last(), Some(&None));
        assert!(
            phases
                .iter()
                .all(|p| matches!(p, None | Some(RefreshPhase::Scanning))),
            "{phases:?}"
        );
        // Both files counted while scanning, and the scan message is plain words.
        let scanning: Vec<&JobStatus> = emits
            .iter()
            .filter_map(|e| match e {
                Emit::Status(s) if s.phase.is_some() => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(scanning.last().unwrap().done, 2);
        assert!(scanning.iter().all(|s| s.message.is_some()));
        // The job ends idle, without error, and says the library changed.
        assert_eq!(
            emits[emits.len() - 2],
            Emit::Status(JobStatus::default()),
            "{emits:?}"
        );
        assert_eq!(emits.last(), Some(&Emit::Changed));
        assert_eq!(f.lib.search(&Default::default()).unwrap().len(), 2);
    }

    #[test]
    fn run_job_identify_without_key_reports_the_error() {
        let f = Fixture::new(&["Heat.1995.mkv"]);
        let mut emits = Vec::new();
        run_job(&f.lib, Job::Identify(IdentifyScope::Unmatched), &mut |e| {
            emits.push(e)
        });
        let Some(Emit::Status(last)) = emits.iter().rev().find(|e| matches!(e, Emit::Status(_)))
        else {
            panic!("{emits:?}")
        };
        assert_eq!(last.phase, None);
        let why = last.last_error.as_deref().unwrap();
        assert!(why.contains("TMDB key"), "{why}");
    }

    #[test]
    fn identical_jobs_collapse() {
        let q = Queue::default();
        assert!(q.push(Job::Refresh(None)));
        assert!(!q.push(Job::Refresh(None)));
        assert!(q.push(Job::Refresh(Some(1))));
        assert!(q.push(Job::Identify(IdentifyScope::Review)));
        assert!(!q.push(Job::Identify(IdentifyScope::Review)));
        assert!(!q.push(Job::Refresh(Some(1))));
        assert_eq!(q.pop(), Job::Refresh(None));
        // Once taken, the same job may wait again.
        assert!(q.push(Job::Refresh(None)));
        assert_eq!(q.pop(), Job::Refresh(Some(1)));
        assert_eq!(q.pop(), Job::Identify(IdentifyScope::Review));
        assert_eq!(q.pop(), Job::Refresh(None));
    }
}
