//! The owner's activity merged from every source: what Exhibit saw, what the owner chose in
//! Exhibit, and what the services the owner has switched on say. This is the only place the
//! rule is written; every read of watched, loved, ratings and the watchlist goes through it.

use crate::error::Result;
use crate::model::Id;
use crate::service::EXHIBIT;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::Value;

/// A fact and the service that said it; `on` is the unix seconds of its day, UTC.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Sourced<T> {
    pub service: String,
    pub on: Option<i64>,
    #[serde(flatten)]
    pub fact: T,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RatingFact {
    pub value: f32,
    pub scale: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReviewFact {
    pub text: String,
    pub spoiler: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WatchFact {
    pub rewatch: bool,
}

/// Everything known about what the owner did with one work. Lists are newest first, undated
/// last; only switched-on services count.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Activity {
    pub watched: bool,
    pub loved: bool,
    pub watched_choice: Option<bool>,
    pub loved_choice: Option<bool>,
    pub ratings: Vec<Sourced<RatingFact>>,
    pub reviews: Vec<Sourced<ReviewFact>>,
    /// Exhibit's own watch is here as the service `exhibit`, on the day it was last played.
    pub watches: Vec<Sourced<WatchFact>>,
    pub liked_by: Vec<String>,
    pub watchlisted_by: Vec<String>,
}

/// Whether a work counts as watched, as SQL over its `items` row `item` and its `play_state`
/// row `play` (which may be missing): the owner's choice if made, else Exhibit played it to
/// the end, else a switched-on service saw it.
pub(crate) fn watched_sql(item: &str, play: &str) -> String {
    format!(
        "coalesce({item}.watched_choice, coalesce({play}.watched, 0) = 1 OR {})",
        logged_sql(item)
    )
}

/// Whether a work counts as loved, as SQL over its `items` row `item`: the owner's choice if
/// made, else a switched-on service liked it.
pub(crate) fn loved_sql(item: &str) -> String {
    format!(
        "coalesce({item}.loved_choice, {})",
        from_a_service(item, "like")
    )
}

/// A switched-on service has a rating of the work `item` (any row with `kind` and `tmdb_id`).
pub(crate) fn rated_sql(item: &str) -> String {
    from_a_service(item, "rating")
}

pub(crate) fn reviewed_sql(item: &str) -> String {
    from_a_service(item, "review")
}

/// A switched-on service logged a watch; Exhibit's own plays are not logs.
pub(crate) fn logged_sql(item: &str) -> String {
    from_a_service(item, "watch")
}

/// Whether the work is on the owner's watchlist: a switched-on service has it there and it is
/// not watched (`watched` is the work's merged watched state as SQL).
pub(crate) fn on_watchlist_sql(item: &str, watched: &str) -> String {
    format!(
        "({} AND NOT {watched})",
        from_a_service(item, "watchlisted")
    )
}

/// The day the newest switched-on service put the work on its watchlist.
pub(crate) fn watchlisted_on_sql(item: &str) -> String {
    format!(
        "(SELECT max(sf.on_day) FROM {FACTS} \
         WHERE sf.kind = {item}.kind AND sf.tmdb_id = {item}.tmdb_id AND sf.fact = 'watchlisted')"
    )
}

/// The owner's rating of the work from 0 to 1: the newest rating any switched-on service has,
/// over its own scale. NULL when none has.
pub(crate) fn my_rating_sql(item: &str) -> String {
    format!(
        "(SELECT 1.0 * (sf.value ->> '$.value') / nullif(sf.value ->> '$.scale', 0) FROM {FACTS} \
         WHERE sf.kind = {item}.kind AND sf.tmdb_id = {item}.tmdb_id AND sf.fact = 'rating' \
         ORDER BY sf.on_day DESC, sf.service LIMIT 1)"
    )
}

/// `service_facts sf` limited to the services that are switched on.
const FACTS: &str =
    "service_facts sf JOIN service_state ss ON ss.service = sf.service AND ss.enabled = 1";

fn from_a_service(item: &str, fact: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM {FACTS} \
         WHERE sf.kind = {item}.kind AND sf.tmdb_id = {item}.tmdb_id AND sf.fact = '{fact}')"
    )
}

/// The activity of the item `id`, or `None` when there is no such item. A show or season is
/// watched when every episode with a file beneath it is.
pub(crate) fn of_item(c: &Connection, id: Id) -> Result<Option<Activity>> {
    let row = c
        .prepare_cached(&format!(
            "WITH RECURSIVE d(id) AS (SELECT ?1 UNION ALL \
                 SELECT i.id FROM items i JOIN d ON i.parent_id = d.id) \
             SELECT i.kind, i.tmdb_id, i.watched_choice, i.loved_choice, \
               CASE WHEN i.kind IN ('show', 'season') THEN \
                 (SELECT count(*) > 0 AND sum({episode_watched}) = count(*) FROM d \
                  JOIN items e ON e.id = d.id AND e.kind = 'episode' \
                  LEFT JOIN play_state q ON q.item_id = e.id \
                  WHERE EXISTS (SELECT 1 FROM files f WHERE f.item_id = e.id)) \
               ELSE {watched} END, \
               {loved}, coalesce(p.watched, 0), p.last_played_at \
             FROM items i LEFT JOIN play_state p ON p.item_id = i.id WHERE i.id = ?1",
            episode_watched = watched_sql("e", "q"),
            watched = watched_sql("i", "p"),
            loved = loved_sql("i"),
        ))?
        .query_row([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<i64>>(1)?,
                Activity {
                    watched_choice: r.get(2)?,
                    loved_choice: r.get(3)?,
                    watched: r.get::<_, Option<bool>>(4)?.unwrap_or(false),
                    loved: r.get(5)?,
                    ..Default::default()
                },
                r.get::<_, bool>(6)?,
                r.get::<_, Option<i64>>(7)?,
            ))
        })
        .optional()?;
    let Some((kind, tmdb_id, mut a, played, last_played)) = row else {
        return Ok(None);
    };
    if played {
        a.watches.push(Sourced {
            service: EXHIBIT.to_string(),
            on: last_played.filter(|&t| t > 0),
            fact: WatchFact { rewatch: false },
        });
    }
    if let Some(tmdb_id) = tmdb_id {
        let mut s = c.prepare_cached(&format!(
            "SELECT sf.service, sf.fact, sf.on_day, sf.value FROM {FACTS} \
             WHERE sf.kind = ?1 AND sf.tmdb_id = ?2 ORDER BY sf.on_day DESC, sf.service"
        ))?;
        let mut rows = s.query(params![kind, tmdb_id])?;
        while let Some(r) = rows.next()? {
            let service: String = r.get(0)?;
            let fact: String = r.get(1)?;
            let on = Some(r.get::<_, i64>(2)?).filter(|&d| d != 0);
            let v: Value = serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or_default();
            let number = |k: &str| v[k].as_f64().map(|n| n as f32);
            let flag = |k: &str| v[k].as_bool().unwrap_or(false);
            let named = |list: &mut Vec<String>, service: String| {
                if !list.contains(&service) {
                    list.push(service);
                }
            };
            match fact.as_str() {
                "rating" => {
                    if let (Some(value), Some(scale)) = (number("value"), number("scale")) {
                        a.ratings.push(Sourced {
                            service,
                            on,
                            fact: RatingFact { value, scale },
                        });
                    }
                }
                "review" => a.reviews.push(Sourced {
                    service,
                    on,
                    fact: ReviewFact {
                        text: v["text"].as_str().unwrap_or_default().to_string(),
                        spoiler: flag("spoiler"),
                    },
                }),
                "watch" => a.watches.push(Sourced {
                    service,
                    on,
                    fact: WatchFact {
                        rewatch: flag("rewatch"),
                    },
                }),
                "like" => named(&mut a.liked_by, service),
                "watchlisted" => named(&mut a.watchlisted_by, service),
                _ => {}
            }
        }
    }
    // Exhibit's watch was put first; a stable sort sets it among the others by its day.
    a.watches.sort_by_key(|w| std::cmp::Reverse(w.on));
    Ok(Some(a))
}
