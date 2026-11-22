//! A provider that keeps what it fetched for one run. Work done ahead on other threads (see
//! `identify`'s look-ahead) is then there when the run asks for it in order, so the run keeps
//! its order and its decisions while the waiting on TMDB happens side by side.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};

use crate::error::{Detail, Error, Result};
use crate::model::ItemKind;
use crate::nfo::ExternalId;
use crate::tmdb::{
    MetadataProvider, MovieMeta, SearchHit, SeasonMeta, SeasonTexts, ShowMeta, Texts,
};

/// What TMDB answered: the thing, or that it has no such thing.
type Answer<V> = std::result::Result<V, Detail>;

/// The answers to one kind of request. A request under way has no answer yet (`None`); whoever
/// asks the same meanwhile waits for it rather than asking TMDB a second time.
struct Kept<K, V> {
    answers: Mutex<HashMap<K, Option<Answer<V>>>>,
    ready: Condvar,
}

impl<K, V> Default for Kept<K, V> {
    fn default() -> Self {
        Kept {
            answers: Mutex::new(HashMap::new()),
            ready: Condvar::new(),
        }
    }
}

impl<K: Eq + Hash + Clone, V: Clone> Kept<K, V> {
    /// The answer kept under `key`, or `fetch`'s. "TMDB has none" is an answer too; any other
    /// failure is not kept, so whoever asks next meets it again, as without the memo.
    fn get(&self, key: K, fetch: impl FnOnce() -> Result<V>) -> Result<V> {
        let lock = || self.answers.lock().unwrap_or_else(|e| e.into_inner());
        let mut answers = lock();
        loop {
            match answers.get(&key) {
                Some(Some(answer)) => return answer.clone().map_err(Error::NotFound),
                Some(None) => {
                    answers = self.ready.wait(answers).unwrap_or_else(|e| e.into_inner());
                }
                None => break,
            }
        }
        answers.insert(key.clone(), None);
        drop(answers);
        let answer = match fetch() {
            Ok(v) => Ok(v),
            Err(Error::NotFound(d)) => Err(d),
            Err(e) => {
                lock().remove(&key);
                self.ready.notify_all();
                return Err(e);
            }
        };
        lock().insert(key, Some(answer.clone()));
        self.ready.notify_all();
        answer.map_err(Error::NotFound)
    }
}

pub(crate) struct Memo<'a> {
    p: &'a dyn MetadataProvider,
    searches: Kept<(ItemKind, String, Option<i32>, String), Vec<SearchHit>>,
    external: Kept<ExternalId, Option<SearchHit>>,
    movies: Kept<i64, MovieMeta>,
    shows: Kept<i64, ShowMeta>,
    seasons: Kept<(i64, i32), SeasonMeta>,
    /// TMDB refused the key or could not be reached.
    failed: AtomicBool,
}

impl<'a> Memo<'a> {
    pub(crate) fn new(p: &'a dyn MetadataProvider) -> Self {
        Memo {
            p,
            searches: Kept::default(),
            external: Kept::default(),
            movies: Kept::default(),
            shows: Kept::default(),
            seasons: Kept::default(),
            failed: AtomicBool::new(false),
        }
    }

    /// Whether a request was refused for the key or could not reach TMDB: work done ahead
    /// stops, and the run meets the failure itself, in order.
    pub(crate) fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    fn watch<V>(&self, r: Result<V>) -> Result<V> {
        if matches!(r, Err(Error::Invalid(_) | Error::Provider(_))) {
            self.failed.store(true, Ordering::Relaxed);
        }
        r
    }
}

impl MetadataProvider for Memo<'_> {
    fn search_movie(&self, title: &str, year: Option<i32>, lang: &str) -> Result<Vec<SearchHit>> {
        let key = (ItemKind::Movie, title.to_string(), year, lang.to_string());
        self.searches
            .get(key, || self.watch(self.p.search_movie(title, year, lang)))
    }

    fn search_show(&self, title: &str, year: Option<i32>, lang: &str) -> Result<Vec<SearchHit>> {
        let key = (ItemKind::Show, title.to_string(), year, lang.to_string());
        self.searches
            .get(key, || self.watch(self.p.search_show(title, year, lang)))
    }

    fn find_external(&self, id: &ExternalId) -> Result<Option<SearchHit>> {
        self.external
            .get(id.clone(), || self.watch(self.p.find_external(id)))
    }

    fn movie(&self, tmdb_id: i64) -> Result<MovieMeta> {
        self.movies
            .get(tmdb_id, || self.watch(self.p.movie(tmdb_id)))
    }

    fn show(&self, tmdb_id: i64) -> Result<ShowMeta> {
        self.shows.get(tmdb_id, || self.watch(self.p.show(tmdb_id)))
    }

    fn season(&self, show_id: i64, season: i32) -> Result<SeasonMeta> {
        self.seasons.get((show_id, season), || {
            self.watch(self.p.season(show_id, season))
        })
    }

    /// Images are kept on disk by whoever asks (see `identify::download`), not here.
    fn image(&self, path: &str, size: &str) -> Result<Vec<u8>> {
        self.watch(self.p.image(path, size))
    }

    fn texts(&self, kind: ItemKind, tmdb_id: i64, lang: &str) -> Result<Texts> {
        self.watch(self.p.texts(kind, tmdb_id, lang))
    }

    fn season_texts(&self, show_id: i64, season: i32, lang: &str) -> Result<SeasonTexts> {
        self.watch(self.p.season_texts(show_id, season, lang))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;
    use std::time::Duration;

    #[test]
    fn one_request_answers_everyone_who_asks_meanwhile() {
        let kept: Kept<i64, i64> = Kept::default();
        let asked = AtomicU32::new(0);
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| {
                    let got = kept.get(7, || {
                        asked.fetch_add(1, Ordering::Relaxed);
                        std::thread::sleep(Duration::from_millis(50));
                        Ok(49)
                    });
                    assert_eq!(got.unwrap(), 49);
                });
            }
        });
        assert_eq!(asked.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn none_found_is_kept_and_an_outage_is_not() {
        let kept: Kept<i64, i64> = Kept::default();
        let missing = || Err(Error::NotFound(Detail::from("no such film")));
        assert!(matches!(kept.get(1, missing), Err(Error::NotFound(_))));
        assert!(matches!(
            kept.get(1, || unreachable!()),
            Err(Error::NotFound(_))
        ));
        let down = || Err(Error::Provider(Detail::from("unreachable")));
        assert!(matches!(kept.get(2, down), Err(Error::Provider(_))));
        assert_eq!(kept.get(2, || Ok(4)).unwrap(), 4);
    }
}
