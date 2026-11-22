//! Slow work (a tool process, a request to a service) spread over a few threads. The database
//! stays on the calling thread: each result is handed back there as it arrives.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;

/// Runs `work` on every item, at most `threads` at a time, and hands each result to `done` on
/// the calling thread as it arrives, not in order. When `done` fails no new item is started; the
/// ones under way finish, their results are dropped, and the failure is returned.
pub(crate) fn each<T: Sync, R: Send, E>(
    items: &[T],
    threads: usize,
    work: impl Fn(&T) -> R + Sync,
    mut done: impl FnMut(&T, R) -> Result<(), E>,
) -> Result<(), E> {
    if items.is_empty() {
        return Ok(());
    }
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    std::thread::scope(|s| {
        let (tx, rx) = mpsc::channel();
        for _ in 0..threads.clamp(1, items.len()) {
            let (tx, next, stop, work) = (tx.clone(), &next, &stop, &work);
            s.spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    if tx.send((i, work(item))).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        for (i, r) in rx {
            if let Err(e) = done(&items[i], r) {
                stop.store(true, Ordering::Relaxed);
                return Err(e);
            }
        }
        Ok(())
    })
}

/// `work` over every item, at most `threads` at a time; the results in the items' order.
pub(crate) fn map<T: Sync, R: Send>(
    items: &[T],
    threads: usize,
    work: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let mut out: Vec<Option<R>> = std::iter::repeat_with(|| None).take(items.len()).collect();
    let indexed: Vec<(usize, &T)> = items.iter().enumerate().collect();
    let all = each(
        &indexed,
        threads,
        |(_, item)| work(item),
        |&(i, _), r| -> Result<(), ()> {
            out[i] = Some(r);
            Ok(())
        },
    );
    debug_assert!(all.is_ok());
    out.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    #[test]
    fn map_keeps_the_order_and_runs_side_by_side() {
        let items: Vec<u64> = (0..8).collect();
        let t = Instant::now();
        let out = map(&items, 8, |&n| {
            std::thread::sleep(Duration::from_millis(100));
            n * 2
        });
        assert_eq!(out, [0, 2, 4, 6, 8, 10, 12, 14]);
        assert!(
            t.elapsed() < Duration::from_millis(500),
            "{:?}",
            t.elapsed()
        );
    }

    #[test]
    fn a_failure_stops_new_work_and_is_returned() {
        let items: Vec<u32> = (0..100).collect();
        let started = Mutex::new(0);
        let r = each(
            &items,
            2,
            |&n| {
                *started.lock().unwrap() += 1;
                std::thread::sleep(Duration::from_millis(5));
                n
            },
            |_, n| if n == 3 { Err("three") } else { Ok(()) },
        );
        assert_eq!(r, Err("three"));
        assert!(*started.lock().unwrap() < 100);
    }

    #[test]
    fn nothing_to_do_starts_nothing() {
        let none: [u8; 0] = [];
        assert_eq!(map(&none, 4, |_| 1), Vec::<i32>::new());
    }
}
