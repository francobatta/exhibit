//! Exhibitions: small shows hung from the owner's own collection by a curator (an outside agent
//! through the MCP server, later the built-in one). Each has a title, a short wall text and a
//! note on every work. Only works the owner has can hang, and every one is checked, so a curator
//! cannot hang a work that does not exist.

use crate::db;
use crate::error::{Detail, Error, Result};
use crate::model::Id;
use crate::query::{self, ItemSummary, online_sources};
use crate::volume::Volumes;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// How many works an exhibition holds, at least and at most.
pub const MIN_WORKS: usize = 3;
pub const MAX_WORKS: usize = 24;
pub const MAX_TITLE: usize = 60;
pub const MAX_WALL_TEXT: usize = 600;
pub const MAX_NOTE: usize = 80;
/// How many exhibitions hang at once: hanging another takes down the oldest.
pub const MAX_EXHIBITIONS: usize = 6;

/// One work to hang and the note beside it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hang {
    pub id: Id,
    #[serde(default)]
    pub note: String,
}

/// An exhibition as asked for: `id` replaces that exhibition, `None` hangs a new one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NewExhibition {
    #[serde(default)]
    pub id: Option<Id>,
    pub title: String,
    #[serde(default)]
    pub wall_text: String,
    pub works: Vec<Hang>,
    #[serde(default)]
    pub curator: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Exhibited {
    pub item: ItemSummary,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Exhibition {
    pub id: Id,
    pub title: String,
    pub wall_text: String,
    pub curator: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    /// In the order hung; a work no longer in the library is gone from it.
    pub works: Vec<Exhibited>,
}

fn invalid(code: &'static str, message: String) -> Error {
    Error::Invalid(Detail::coded(code, message))
}

/// Hangs `e`, or replaces the exhibition it names, and returns its id. Hanging a new one beyond
/// `MAX_EXHIBITIONS` takes down the oldest.
pub fn save(c: &Connection, e: &NewExhibition) -> Result<Id> {
    let title = e.title.trim();
    if title.is_empty() || title.chars().count() > MAX_TITLE {
        return Err(invalid(
            "exhibition.title",
            format!("a title of 1 to {MAX_TITLE} characters is needed"),
        ));
    }
    let wall = e.wall_text.trim();
    if wall.chars().count() > MAX_WALL_TEXT {
        return Err(invalid(
            "exhibition.wall_text",
            format!("the wall text is longer than {MAX_WALL_TEXT} characters"),
        ));
    }
    if !(MIN_WORKS..=MAX_WORKS).contains(&e.works.len()) {
        return Err(invalid(
            "exhibition.works",
            format!("an exhibition holds {MIN_WORKS} to {MAX_WORKS} works"),
        ));
    }
    let mut seen = HashSet::new();
    if let Some(dup) = e.works.iter().find(|w| !seen.insert(w.id)) {
        return Err(invalid(
            "exhibition.works",
            format!("work {} is hung twice", dup.id),
        ));
    }
    if let Some(long) = e
        .works
        .iter()
        .find(|w| w.note.trim().chars().count() > MAX_NOTE)
    {
        return Err(invalid(
            "exhibition.note",
            format!(
                "the note on work {} is longer than {MAX_NOTE} characters",
                long.id
            ),
        ));
    }
    // Every work must be a film or a show the owner has.
    let owned = owned_works(c, &e.works.iter().map(|w| w.id).collect::<Vec<_>>())?;
    if let Some(missing) = e.works.iter().find(|w| !owned.contains(&w.id)) {
        return Err(invalid(
            "exhibition.work",
            format!(
                "work {} is not a film or show in the library; use ids from search_library",
                missing.id
            ),
        ));
    }

    let now = db::now();
    let tx = db::write_tx(c)?;
    let id = match e.id {
        Some(id) => {
            let n = tx
                .prepare_cached("UPDATE exhibitions SET title = ?2, wall_text = ?3, curator = ?4, updated_at = ?5 WHERE id = ?1")?
                .execute(params![id, title, wall, e.curator, now])?;
            if n == 0 {
                return Err(Error::NotFound(Detail::coded(
                    "exhibition.unknown",
                    format!("no exhibition {id}"),
                )));
            }
            tx.prepare_cached("DELETE FROM exhibition_works WHERE exhibition_id = ?1")?
                .execute([id])?;
            id
        }
        None => {
            tx.prepare_cached("INSERT INTO exhibitions(title, wall_text, curator, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)")?
                .execute(params![title, wall, e.curator, now])?;
            let id = tx.last_insert_rowid();
            tx.prepare_cached(
                "DELETE FROM exhibitions WHERE id NOT IN \
                 (SELECT id FROM exhibitions ORDER BY updated_at DESC, id DESC LIMIT ?1)",
            )?
            .execute([MAX_EXHIBITIONS as i64])?;
            id
        }
    };
    let mut put = tx.prepare_cached("INSERT INTO exhibition_works(exhibition_id, item_id, position, note) VALUES (?1, ?2, ?3, ?4)")?;
    for (at, w) in e.works.iter().enumerate() {
        put.execute(params![id, w.id, at as i64, w.note.trim()])?;
    }
    drop(put);
    tx.commit()?;
    Ok(id)
}

/// Of `ids`, the films and shows that have a file.
fn owned_works(c: &Connection, ids: &[Id]) -> Result<HashSet<Id>> {
    let sql = format!(
        "SELECT i.id FROM items i WHERE i.kind IN ('movie', 'show') \
         AND i.id IN (SELECT value FROM json_each(?1)) AND {}",
        query::in_library("i.id")
    );
    Ok(c.prepare_cached(&sql)?
        .query_map(params![query::json_ids(ids)], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

/// Every exhibition hanging, the most recently hung first.
pub fn list(c: &Connection, v: &dyn Volumes) -> Result<Vec<Exhibition>> {
    let heads: Vec<(Id, String, String, Option<String>, i64, i64)> = c
        .prepare_cached("SELECT id, title, wall_text, curator, created_at, updated_at FROM exhibitions ORDER BY updated_at DESC, id DESC")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let hung: Vec<(Id, Id, String)> = c
        .prepare_cached("SELECT exhibition_id, item_id, note FROM exhibition_works ORDER BY exhibition_id, position")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let online = online_sources(c, v)?;
    let ids: Vec<Id> = hung.iter().map(|h| h.1).collect();
    let items: HashMap<Id, ItemSummary> = query::summaries_by_ids(c, &online, &ids)?
        .into_iter()
        .map(|s| (s.id, s))
        .collect();
    Ok(heads
        .into_iter()
        .map(
            |(id, title, wall_text, curator, created_at, updated_at)| Exhibition {
                works: hung
                    .iter()
                    .filter(|h| h.0 == id)
                    .filter_map(|(_, item, note)| {
                        Some(Exhibited {
                            item: items.get(item)?.clone(),
                            note: note.clone(),
                        })
                    })
                    .collect(),
                id,
                title,
                wall_text,
                curator,
                created_at,
                updated_at,
            },
        )
        .collect())
}

/// Takes down exhibition `id`; true when it was hanging.
pub fn remove(c: &Connection, id: Id) -> Result<bool> {
    Ok(c.prepare_cached("DELETE FROM exhibitions WHERE id = ?1")?
        .execute([id])?
        > 0)
}

/// The exhibition `id`, if it hangs.
pub fn get(c: &Connection, v: &dyn Volumes, id: Id) -> Result<Option<Exhibition>> {
    let exists: Option<Id> = c
        .prepare_cached("SELECT id FROM exhibitions WHERE id = ?1")?
        .query_row([id], |r| r.get(0))
        .optional()?;
    if exists.is_none() {
        return Ok(None);
    }
    Ok(list(c, v)?.into_iter().find(|e| e.id == id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;
    use crate::volume::FakeVolumes;

    struct Env {
        _data: tempfile::TempDir,
        _drive: tempfile::TempDir,
        c: Connection,
        v: FakeVolumes,
        films: Vec<Id>,
        wanted: Id,
    }

    fn env() -> Env {
        let data = tempfile::tempdir().unwrap();
        let drive = tempfile::tempdir().unwrap();
        let c = db::open(data.path()).unwrap();
        let v = FakeVolumes(vec![("VOL1".to_string(), drive.path().to_path_buf())]);
        let src = scan::add_source(&c, &v, drive.path()).unwrap().id;
        let mut films = Vec::new();
        for n in 0..5 {
            c.execute("INSERT INTO items(kind, title, match_state, added_at) VALUES ('movie', ?1, 'matched', 0)", [format!("F{n}")]).unwrap();
            let id = c.last_insert_rowid();
            c.execute(
                "INSERT INTO files(source_id, rel_path, item_id) VALUES (?1, ?2, ?3)",
                params![src, format!("f{n}.mkv"), id],
            )
            .unwrap();
            films.push(id);
        }
        c.execute("INSERT INTO items(kind, title, match_state, added_at, wanted) VALUES ('movie', 'Not owned', 'matched', 0, 1)", []).unwrap();
        let wanted = c.last_insert_rowid();
        Env {
            _data: data,
            _drive: drive,
            c,
            v,
            films,
            wanted,
        }
    }

    fn show(ids: &[Id], title: &str) -> NewExhibition {
        NewExhibition {
            id: None,
            title: title.into(),
            wall_text: "On the wall.".into(),
            works: ids
                .iter()
                .map(|id| Hang {
                    id: *id,
                    note: format!("note {id}"),
                })
                .collect(),
            curator: Some("test".into()),
        }
    }

    #[test]
    fn hangs_in_order_with_notes_and_replaces_by_id() {
        let e = env();
        let id = save(&e.c, &show(&[e.films[2], e.films[0], e.films[1]], "Three")).unwrap();
        let hung = list(&e.c, &e.v).unwrap();
        assert_eq!(hung.len(), 1);
        let titles: Vec<&str> = hung[0]
            .works
            .iter()
            .map(|w| w.item.title.as_str())
            .collect();
        assert_eq!(titles, ["F2", "F0", "F1"]);
        assert_eq!(hung[0].works[0].note, format!("note {}", e.films[2]));

        let again = NewExhibition {
            id: Some(id),
            ..show(&e.films[..4], "Four")
        };
        assert_eq!(save(&e.c, &again).unwrap(), id);
        let hung = list(&e.c, &e.v).unwrap();
        assert_eq!(
            (hung.len(), hung[0].title.as_str(), hung[0].works.len()),
            (1, "Four", 4)
        );
        assert!(remove(&e.c, id).unwrap());
        assert!(list(&e.c, &e.v).unwrap().is_empty());
    }

    #[test]
    fn refuses_works_that_are_not_the_owners_and_shows_out_of_shape() {
        let e = env();
        let err = |x: NewExhibition| save(&e.c, &x).unwrap_err().to_string();
        assert!(
            err(show(&[e.films[0], e.films[1], e.wanted], "Wanted"))
                .contains(&e.wanted.to_string())
        );
        assert!(err(show(&[e.films[0], e.films[1], 999], "Made up")).contains("999"));
        assert!(err(show(&e.films[..2], "Too few")).contains("3 to 24"));
        assert!(err(show(&[e.films[0], e.films[0], e.films[1]], "Twice")).contains("twice"));
        assert!(err(show(&e.films[..3], "")).contains("title"));
        assert!(list(&e.c, &e.v).unwrap().is_empty());
    }

    #[test]
    fn hanging_beyond_the_most_takes_down_the_oldest() {
        let e = env();
        let first = save(&e.c, &show(&e.films[..3], "First")).unwrap();
        for n in 0..MAX_EXHIBITIONS {
            save(&e.c, &show(&e.films[..3], &format!("Show {n}"))).unwrap();
        }
        let hung = list(&e.c, &e.v).unwrap();
        assert_eq!(hung.len(), MAX_EXHIBITIONS);
        assert!(hung.iter().all(|x| x.id != first));
    }
}
