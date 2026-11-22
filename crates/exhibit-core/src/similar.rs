//! What a work is like: TMDB's keywords and recommendations for it, kept per work, and the works
//! in the library most like a given one, each with what links them. Scored here, with no model
//! and no network, so "More like this" works offline and an outside agent can ask why.

use crate::error::Result;
use crate::filter::Count;
use crate::model::Id;
use crate::query::{self, ItemSummary, LIB_CTE, TREE_CTE, online_sources};
use crate::volume::Volumes;
use rusqlite::{Connection, params};
use serde::Serialize;
use std::collections::HashMap;

/// TMDB's keywords about the release rather than the work: they link nothing.
const NOT_THEMES: [&str; 4] = [
    "aftercreditsstinger",
    "duringcreditsstinger",
    "woman director",
    "short film",
];

/// Replaces the keywords and related works stored for `id`.
pub(crate) fn store(c: &Connection, id: Id, keywords: &[String], related: &[i64]) -> Result<()> {
    c.prepare_cached("DELETE FROM item_keywords WHERE item_id = ?1")?
        .execute([id])?;
    c.prepare_cached("DELETE FROM item_related WHERE item_id = ?1")?
        .execute([id])?;
    let mut k =
        c.prepare_cached("INSERT OR IGNORE INTO item_keywords(item_id, keyword) VALUES (?1, ?2)")?;
    for word in keywords
        .iter()
        .map(|w| w.trim().to_lowercase())
        .filter(|w| !w.is_empty() && !NOT_THEMES.contains(&w.as_str()))
    {
        k.execute(params![id, word])?;
    }
    let mut r = c.prepare_cached(
        "INSERT OR IGNORE INTO item_related(item_id, tmdb_id, rank) VALUES (?1, ?2, ?3)",
    )?;
    for (rank, tmdb_id) in related.iter().enumerate() {
        r.execute(params![id, tmdb_id, rank as i64])?;
    }
    Ok(())
}

/// The keywords stored for `id`, most specific first (the fewest works share them).
pub fn keywords(c: &Connection, id: Id) -> Result<Vec<String>> {
    Ok(c
        .prepare_cached(
            "SELECT k.keyword FROM item_keywords k WHERE k.item_id = ?1 \
             ORDER BY (SELECT count(*) FROM item_keywords o WHERE o.keyword = k.keyword), k.keyword",
        )?
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

/// The people with the most works in the library in `role` ("director"), most first.
pub fn top_people(
    c: &Connection,
    v: &dyn Volumes,
    role: &str,
    limit: u32,
) -> Result<Vec<Count<String>>> {
    let online = online_sources(c, v)?;
    let sql = format!(
        "WITH RECURSIVE {TREE_CTE}, {LIB_CTE} SELECT p.name, count(*) n FROM item_people p          JOIN lib ON lib.item_id = p.item_id JOIN items i ON i.id = p.item_id          WHERE p.role = ?2 AND i.kind IN ('movie', 'show') GROUP BY p.name ORDER BY n DESC, p.name LIMIT ?3"
    );
    Ok(c.prepare_cached(&sql)?
        .query_map(params![query::json_ids(&online), role, limit], |r| {
            Ok(Count {
                value: r.get(0)?,
                count: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
}

/// What links two works, strongest first in `Similar::because`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Link {
    /// TMDB recommends one after the other.
    Recommended,
    Keyword {
        keyword: String,
    },
    /// The same person in the same role ("director", "writer", "cinematographer", "composer",
    /// "editor", "cast").
    Person {
        name: String,
        role: String,
    },
    Genre {
        genre: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Similar {
    pub item: ItemSummary,
    pub score: f32,
    /// What links it to the work asked about, strongest first, at most `MAX_LINKS`.
    pub because: Vec<Link>,
}

/// Each link with its weight.
type Links = Vec<(f64, Link)>;

const MAX_LINKS: usize = 5;
/// Below this a work shares only a genre or a common keyword: not "like" it.
const THRESHOLD: f64 = 2.0;

/// The works in the library most like `id`, of the same kind (films for a film, shows for a
/// show), best first. Each link adds to the score: TMDB's recommendation most, then the same
/// director or writer, keywords by how few works share them, the rest of the crew and cast, and
/// a little for each genre.
pub fn similar(c: &Connection, v: &dyn Volumes, id: Id, limit: u32) -> Result<Vec<Similar>> {
    let online = online_sources(c, v)?;
    let sql = format!(
        "WITH RECURSIVE {TREE_CTE}, {LIB_CTE}, \
         me AS (SELECT id, kind, tmdb_id, genres FROM items WHERE id = ?2), \
         pool AS (SELECT i.id, i.tmdb_id, i.genres FROM items i JOIN lib ON lib.item_id = i.id, me \
           WHERE i.kind = me.kind AND i.id <> me.id), \
         df AS (SELECT keyword, count(*) n FROM item_keywords GROUP BY keyword) \
         SELECT p.id, 'recommended', NULL, NULL, 6.0 - min(r.rank, 20) * 0.2 \
           FROM item_related r JOIN pool p ON p.tmdb_id = r.tmdb_id WHERE r.item_id = ?2 \
         UNION ALL SELECT p.id, 'recommended', NULL, NULL, 3.0 \
           FROM pool p JOIN item_related r ON r.item_id = p.id, me WHERE r.tmdb_id = me.tmdb_id \
         UNION ALL SELECT p.id, 'keyword', k.keyword, NULL, 4.0 / df.n \
           FROM item_keywords m JOIN item_keywords k ON k.keyword = m.keyword \
           JOIN pool p ON p.id = k.item_id JOIN df ON df.keyword = m.keyword WHERE m.item_id = ?2 \
         UNION ALL SELECT p.id, 'person', o.name, o.role, \
           CASE o.role WHEN 'director' THEN 3.0 WHEN 'writer' THEN 2.0 WHEN 'cast' THEN 0.75 ELSE 1.25 END \
           FROM item_people m JOIN item_people o ON o.name = m.name AND o.role = m.role \
           JOIN pool p ON p.id = o.item_id WHERE m.item_id = ?2 \
         UNION ALL SELECT p.id, 'genre', g.value, NULL, 0.4 \
           FROM me, json_each(me.genres) g, pool p, json_each(p.genres) h WHERE h.value = g.value"
    );
    let mut scored: HashMap<Id, (f64, Links)> = HashMap::new();
    let mut s = c.prepare_cached(&sql)?;
    let rows = s.query_map(params![query::json_ids(&online), id], |r| {
        Ok((
            r.get::<_, Id>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, f64>(4)?,
        ))
    })?;
    for row in rows {
        let (other, kind, value, role, weight) = row?;
        let link = match kind.as_str() {
            "recommended" => Link::Recommended,
            "keyword" => Link::Keyword {
                keyword: value.unwrap_or_default(),
            },
            "person" => Link::Person {
                name: value.unwrap_or_default(),
                role: role.unwrap_or_default(),
            },
            _ => Link::Genre {
                genre: value.unwrap_or_default(),
            },
        };
        let entry = scored.entry(other).or_default();
        entry.0 += weight;
        // Both ways of a recommendation count, but it is said once.
        if !entry.1.iter().any(|(_, l)| *l == link) {
            entry.1.push((weight, link));
        }
    }
    let mut best: Vec<(Id, f64, Links)> = scored
        .into_iter()
        .filter(|(_, (score, _))| *score >= THRESHOLD)
        .map(|(id, (s, l))| (id, s, l))
        .collect();
    best.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    best.truncate(limit.min(query::MAX_LIMIT) as usize);
    let ids: Vec<Id> = best.iter().map(|b| b.0).collect();
    let mut items: HashMap<Id, ItemSummary> = query::summaries_by_ids(c, &online, &ids)?
        .into_iter()
        .map(|s| (s.id, s))
        .collect();
    Ok(best
        .into_iter()
        .filter_map(|(id, score, mut links)| {
            links.sort_by(|a, b| b.0.total_cmp(&a.0));
            Some(Similar {
                item: items.remove(&id)?,
                score: score as f32,
                because: links.into_iter().take(MAX_LINKS).map(|(_, l)| l).collect(),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::filter::Filters;
    use crate::model::Person;
    use crate::query::SearchQuery;
    use crate::scan;
    use crate::volume::FakeVolumes;

    struct Env {
        _data: tempfile::TempDir,
        _drive: tempfile::TempDir,
        c: Connection,
        v: FakeVolumes,
        src: Id,
    }

    fn env() -> Env {
        let data = tempfile::tempdir().unwrap();
        let drive = tempfile::tempdir().unwrap();
        let c = db::open(data.path()).unwrap();
        let v = FakeVolumes(vec![("VOL1".to_string(), drive.path().to_path_buf())]);
        let src = scan::add_source(&c, &v, drive.path()).unwrap().id;
        Env {
            _data: data,
            _drive: drive,
            c,
            v,
            src,
        }
    }

    fn person(name: &str, role: &str) -> Person {
        Person {
            name: name.into(),
            role: role.into(),
            ..Person::default()
        }
    }

    impl Env {
        /// A matched film, with a file unless `owned` is false (a watchlist film).
        #[allow(clippy::too_many_arguments)]
        fn film(
            &self,
            title: &str,
            tmdb: i64,
            genres: &[&str],
            people: &[Person],
            keywords: &[&str],
            related: &[i64],
            owned: bool,
        ) -> Id {
            self.c
                .execute(
                    "INSERT INTO items(kind, title, tmdb_id, genres, people, match_state, added_at) \
                     VALUES ('movie', ?1, ?2, ?3, ?4, 'matched', 0)",
                    params![title, tmdb, serde_json::to_string(genres).unwrap(), serde_json::to_string(people).unwrap()],
                )
                .unwrap();
            let id = self.c.last_insert_rowid();
            let words: Vec<String> = keywords.iter().map(|k| k.to_string()).collect();
            store(&self.c, id, &words, related).unwrap();
            if owned {
                self.c
                    .execute(
                        "INSERT INTO files(source_id, rel_path, item_id) VALUES (?1, ?2, ?3)",
                        params![self.src, format!("{title}.mkv"), id],
                    )
                    .unwrap();
            }
            id
        }

        fn titles(&self, filters: Filters) -> Vec<String> {
            let q = SearchQuery {
                filters,
                ..SearchQuery::default()
            };
            let mut t: Vec<String> = query::search(&self.c, &self.v, &q)
                .unwrap()
                .into_iter()
                .map(|s| s.title)
                .collect();
            t.sort();
            t
        }
    }

    #[test]
    fn the_closest_first_with_what_links_them_and_nothing_merely_of_the_same_genre() {
        let e = env();
        let conversation = e.film(
            "The Conversation",
            592,
            &["Drama", "Thriller"],
            &[
                person("Francis Ford Coppola", "director"),
                person("Gene Hackman", "cast"),
            ],
            &["surveillance", "paranoia", "san francisco"],
            &[1730],
            true,
        );
        let blow_out = e.film(
            "Blow Out",
            11644,
            &["Thriller"],
            &[person("Brian De Palma", "director")],
            &["surveillance", "paranoia"],
            &[],
            true,
        );
        let enemy = e.film(
            "Enemy of the State",
            9798,
            &["Action", "Thriller"],
            &[person("Gene Hackman", "cast")],
            &["surveillance"],
            &[],
            true,
        );
        e.film(
            "Apocalypse Now",
            28,
            &["Drama", "War"],
            &[person("Francis Ford Coppola", "director")],
            &["vietnam war"],
            &[],
            true,
        );
        e.film(
            "Some Drama",
            5,
            &["Drama", "Thriller"],
            &[],
            &["family"],
            &[],
            true,
        );
        // TMDB recommends it, but the owner does not have it.
        e.film(
            "The Parallax View",
            1730,
            &["Thriller"],
            &[],
            &["paranoia"],
            &[],
            false,
        );
        // Everyone has "surveillance": it counts for less than "paranoia".
        for n in 0..6 {
            e.film(
                &format!("Filler {n}"),
                1000 + n,
                &["Comedy"],
                &[],
                &["surveillance"],
                &[],
                true,
            );
        }

        let found = similar(&e.c, &e.v, conversation, 10).unwrap();
        let titles: Vec<&str> = found.iter().map(|s| s.item.title.as_str()).collect();
        assert_eq!(titles, ["Apocalypse Now", "Blow Out"], "{found:#?}");
        assert_eq!(
            found[0].because[0],
            Link::Person {
                name: "Francis Ford Coppola".into(),
                role: "director".into()
            }
        );
        assert!(found[1].because.contains(&Link::Keyword {
            keyword: "paranoia".into()
        }));
        assert!(
            found
                .iter()
                .all(|s| s.item.id != enemy && s.item.id != conversation)
        );
        let _ = blow_out;
    }

    #[test]
    fn a_recommendation_either_way_counts() {
        let e = env();
        let a = e.film("A", 1, &[], &[], &[], &[2], true);
        let b = e.film("B", 2, &[], &[], &[], &[], true);
        let c = e.film("C", 3, &[], &[], &[], &[1], true);
        let ids: Vec<Id> = similar(&e.c, &e.v, a, 10)
            .unwrap()
            .iter()
            .map(|s| s.item.id)
            .collect();
        assert_eq!(ids, [b, c]);
        assert_eq!(
            similar(&e.c, &e.v, c, 10).unwrap()[0].because,
            [Link::Recommended]
        );
    }

    #[test]
    fn keywords_and_people_filter_a_listing() {
        let e = env();
        e.film(
            "Heat",
            949,
            &[],
            &[
                person("Michael Mann", "director"),
                person("Al Pacino", "cast"),
            ],
            &["heist", "los angeles"],
            &[],
            true,
        );
        e.film(
            "Thief",
            11524,
            &[],
            &[person("Michael Mann", "director")],
            &["heist"],
            &[],
            true,
        );
        e.film(
            "Scarface",
            111,
            &[],
            &[person("Al Pacino", "cast")],
            &["drug cartel"],
            &[],
            true,
        );
        let kw = |words: &[&str], any: bool| Filters {
            keywords: words.iter().map(|w| w.to_string()).collect(),
            any_keyword: any,
            ..Filters::default()
        };
        assert_eq!(e.titles(kw(&["Heist"], false)), ["Heat", "Thief"]);
        assert_eq!(e.titles(kw(&["heist", "los angeles"], false)), ["Heat"]);
        assert_eq!(
            e.titles(kw(&["los angeles", "drug cartel"], true)),
            ["Heat", "Scarface"]
        );
        let people = |names: &[&str]| Filters {
            people: names.iter().map(|w| w.to_string()).collect(),
            ..Filters::default()
        };
        assert_eq!(e.titles(people(&["al pacino"])), ["Heat", "Scarface"]);
        assert_eq!(e.titles(people(&["Al Pacino", "Michael Mann"])), ["Heat"]);
    }

    #[test]
    fn the_people_table_follows_the_people_written_on_the_work() {
        let e = env();
        let id = e.film(
            "Heat",
            949,
            &[],
            &[person("Michael Mann", "director")],
            &[],
            &[],
            true,
        );
        e.c.execute(
            "UPDATE items SET people = ?2 WHERE id = ?1",
            params![
                id,
                serde_json::to_string(&[person("Dante Spinotti", "cinematographer")]).unwrap()
            ],
        )
        .unwrap();
        let rows: Vec<(String, String)> =
            e.c.prepare("SELECT role, name FROM item_people WHERE item_id = ?1")
                .unwrap()
                .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
        assert_eq!(
            rows,
            [("cinematographer".to_string(), "Dante Spinotti".to_string())]
        );
    }

    #[test]
    fn facets_list_the_keywords_most_works_share() {
        let e = env();
        e.film("Heat", 949, &[], &[], &["heist", "los angeles"], &[], true);
        e.film("Thief", 11524, &[], &[], &["heist"], &[], true);
        let f = crate::filter::facets(&e.c, &e.v, None).unwrap();
        let words: Vec<(&str, u32)> = f
            .keywords
            .iter()
            .map(|k| (k.value.as_str(), k.count))
            .collect();
        assert_eq!(words, [("heist", 2), ("los angeles", 1)]);
    }
}
