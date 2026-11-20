//! The cast of a work with their portraits, for the details screen. Portraits are fetched when a
//! work's details are opened, not while identifying, so a large library costs nothing until then.
//! A work matched before characters and portraits were kept gets its credits read again once.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{Error, Result};
use crate::identify::{download, json};
use crate::model::{CastMember, Id, Person};
use crate::tmdb::MetadataProvider;

/// TMDB's portrait size: 421 by 632, sharp at the size the details screen shows it on a 4K TV.
const PORTRAIT_SIZE: &str = "h632";

struct Work {
    id: Id,
    kind: String,
    tmdb_id: Option<i64>,
    people: Vec<Person>,
}

/// The work whose cast `id` shows: an episode or a season shows its show's.
fn work_of(c: &Connection, mut id: Id) -> Result<Option<Work>> {
    loop {
        let row = c
            .query_row(
                "SELECT kind, parent_id, tmdb_id, people FROM items WHERE id=?1",
                [id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<Id>>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((kind, parent, tmdb_id, people)) = row else {
            return Ok(None);
        };
        match parent {
            Some(p) if kind == "episode" || kind == "season" => id = p,
            _ => {
                return Ok(Some(Work {
                    id,
                    kind,
                    tmdb_id,
                    people: serde_json::from_str(&people).unwrap_or_default(),
                }));
            }
        }
    }
}

/// Reads the credits again for a work whose cast was stored without TMDB ids.
fn refresh(c: &Connection, p: &dyn MetadataProvider, w: &mut Work) -> Result<()> {
    let stale =
        w.people.iter().any(|x| x.role == "cast") && w.people.iter().all(|x| x.tmdb_id.is_none());
    let Some(tmdb_id) = w.tmdb_id.filter(|_| stale) else {
        return Ok(());
    };
    let people = match w.kind.as_str() {
        "movie" => p.movie(tmdb_id)?.people,
        "show" => p.show(tmdb_id)?.people,
        _ => return Ok(()),
    };
    c.prepare_cached("UPDATE items SET people=?2 WHERE id=?1")?
        .execute(params![w.id, json(&people)])?;
    w.people = people;
    Ok(())
}

/// The cast of `id` in billing order, each with a cached portrait when TMDB has one. Without a
/// provider (no key) only portraits already cached are given. Nothing TMDB does wrong fails it:
/// after its first error the rest come from the cache alone, so an outage costs one timeout.
pub fn cast(
    c: &Connection,
    p: Option<&dyn MetadataProvider>,
    cache_dir: &Path,
    id: Id,
) -> Result<Vec<CastMember>> {
    let Some(mut w) = work_of(c, id)? else {
        return Ok(Vec::new());
    };
    let mut reachable = p;
    if let Some(provider) = reachable {
        match refresh(c, provider, &mut w) {
            Err(Error::Db(e)) => return Err(Error::Db(e)),
            Err(_) => reachable = None,
            Ok(()) => {}
        }
    }
    Ok(w.people
        .into_iter()
        .filter(|x| x.role == "cast")
        .map(|x| {
            let portrait = x.tmdb_id.zip(x.profile.as_deref()).and_then(|(pid, path)| {
                let name = format!("person-{pid}.jpg");
                if cache_dir.join(&name).is_file() {
                    return Some(name);
                }
                let provider = reachable?;
                match download(provider, cache_dir, path, PORTRAIT_SIZE, &name) {
                    Ok(()) => Some(name),
                    Err(_) => {
                        reachable = None;
                        None
                    }
                }
            });
            CastMember {
                name: x.name,
                character: x.character,
                portrait,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    use crate::nfo::ExternalId;
    use crate::tmdb::{MovieMeta, SearchHit, SeasonMeta, ShowMeta};
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct Fake {
        people: Vec<Person>,
        images_fail: bool,
        calls: RefCell<Vec<String>>,
        images: Cell<u32>,
    }

    impl MetadataProvider for Fake {
        fn search_movie(&self, _: &str, _: Option<i32>, _: &str) -> Result<Vec<SearchHit>> {
            unreachable!()
        }
        fn search_show(&self, _: &str, _: Option<i32>, _: &str) -> Result<Vec<SearchHit>> {
            unreachable!()
        }
        fn find_external(&self, _: &ExternalId) -> Result<Option<SearchHit>> {
            unreachable!()
        }
        fn movie(&self, id: i64) -> Result<MovieMeta> {
            self.calls.borrow_mut().push(format!("movie {id}"));
            Ok(MovieMeta {
                tmdb_id: id,
                title: "Easy Rider".into(),
                year: Some(1969),
                overview: String::new(),
                runtime_min: None,
                genres: vec![],
                people: self.people.clone(),
                poster: None,
                backdrop: None,
            })
        }
        fn show(&self, _: i64) -> Result<ShowMeta> {
            unreachable!()
        }
        fn season(&self, _: i64, _: i32) -> Result<SeasonMeta> {
            unreachable!()
        }
        fn image(&self, path: &str, size: &str) -> Result<Vec<u8>> {
            self.images.set(self.images.get() + 1);
            if self.images_fail {
                return Err(Error::Provider("offline".into()));
            }
            Ok(format!("{size}{path}").into_bytes())
        }
        fn texts(&self, _: crate::model::ItemKind, _: i64, _: &str) -> Result<crate::tmdb::Texts> {
            unreachable!()
        }
        fn season_texts(&self, _: i64, _: i32, _: &str) -> Result<crate::tmdb::SeasonTexts> {
            unreachable!()
        }
    }

    fn actor(name: &str, id: i64, profile: Option<&str>) -> Person {
        Person {
            name: name.into(),
            role: "cast".into(),
            tmdb_id: Some(id),
            character: Some(format!("{name}'s part")),
            profile: profile.map(str::to_string),
        }
    }

    fn library(people: &[Person]) -> (tempfile::TempDir, Connection, Id) {
        let dir = tempfile::tempdir().unwrap();
        let c = db::open(dir.path()).unwrap();
        c.execute(
            "INSERT INTO items(kind, title, tmdb_id, people, match_state, added_at) \
             VALUES ('movie', 'Easy Rider', 624, ?1, 'matched', 0)",
            [serde_json::to_string(people).unwrap()],
        )
        .unwrap();
        let id = c.last_insert_rowid();
        (dir, c, id)
    }

    #[test]
    fn cast_in_order_with_portraits_cached_once() {
        let director = Person {
            name: "Dennis Hopper".into(),
            role: "director".into(),
            ..Person::default()
        };
        let people = [
            director,
            actor("Peter Fonda", 1, Some("/fonda.jpg")),
            actor("Jack Nicholson", 2, None),
        ];
        let (dir, c, id) = library(&people);
        let cache = dir.path().join("images");
        let p = Fake::default();
        let cast = cast(&c, Some(&p), &cache, id).unwrap();
        assert_eq!(
            cast,
            [
                CastMember {
                    name: "Peter Fonda".into(),
                    character: Some("Peter Fonda's part".into()),
                    portrait: Some("person-1.jpg".into()),
                },
                CastMember {
                    name: "Jack Nicholson".into(),
                    character: Some("Jack Nicholson's part".into()),
                    portrait: None,
                },
            ]
        );
        assert_eq!(
            std::fs::read(cache.join("person-1.jpg")).unwrap(),
            b"h632/fonda.jpg"
        );
        // Opened again, and without a key: from the cache.
        super::cast(&c, None, &cache, id).unwrap();
        super::cast(&c, Some(&p), &cache, id).unwrap();
        assert_eq!(p.images.get(), 1);
        assert!(p.calls.borrow().is_empty());
    }

    #[test]
    fn a_cast_stored_without_ids_is_read_again_once() {
        let old = Person {
            name: "Peter Fonda".into(),
            role: "cast".into(),
            ..Person::default()
        };
        let (dir, c, id) = library(&[old]);
        let p = Fake {
            people: vec![actor("Peter Fonda", 1, Some("/fonda.jpg"))],
            ..Fake::default()
        };
        let cache = dir.path().join("images");
        let first = cast(&c, Some(&p), &cache, id).unwrap();
        assert_eq!(first[0].portrait.as_deref(), Some("person-1.jpg"));
        cast(&c, Some(&p), &cache, id).unwrap();
        assert_eq!(*p.calls.borrow(), ["movie 624"]);
    }

    #[test]
    fn an_outage_costs_one_try_and_keeps_the_names() {
        let people = [
            actor("Peter Fonda", 1, Some("/fonda.jpg")),
            actor("Dennis Hopper", 2, Some("/hopper.jpg")),
        ];
        let (dir, c, id) = library(&people);
        let p = Fake {
            images_fail: true,
            ..Fake::default()
        };
        let cast = cast(&c, Some(&p), &dir.path().join("images"), id).unwrap();
        assert_eq!(cast.len(), 2);
        assert!(cast.iter().all(|m| m.portrait.is_none()));
        assert_eq!(p.images.get(), 1);
    }

    #[test]
    fn an_episode_shows_its_shows_cast_and_a_missing_item_none() {
        let (dir, c, show) = library(&[actor("Adam Scott", 9, None)]);
        c.execute(
            "INSERT INTO items(kind, title, parent_id, people, match_state, added_at) \
             VALUES ('episode', 'Hello, Ms. Cobel', ?1, '[]', 'matched', 0)",
            [show],
        )
        .unwrap();
        let episode = c.last_insert_rowid();
        let cache = dir.path().join("images");
        assert_eq!(
            cast(&c, None, &cache, episode).unwrap()[0].name,
            "Adam Scott"
        );
        assert!(cast(&c, None, &cache, 9999).unwrap().is_empty());
    }
}
