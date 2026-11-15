//! The show path of identification. Episode files are grouped by normalised title; each
//! group resolves to one TMDB show (from an `.nfo` id, or one scored `search_show`), and on
//! acceptance gets one `Show` item, `Season` items for seasons with local files and
//! `Episode` items for episodes with local files. Files are relinked to their episode and the
//! provisional items deleted.

use crate::db;
use crate::error::{Error, Result};
use crate::identify::{
    Counts, Fill, How, MAX_HITS, Pending, absorb, artwork, candidate, confirm, find_item,
    insert_item, set_review, sort_candidates,
};
use crate::matcher::{self, Evidence};
use crate::model::{Candidate, Id, ItemKind, MatchSource};
use crate::parse::{normalize_title, parse_path};
use crate::rows::enum_str;
use crate::tmdb::{EpisodeMeta, MetadataProvider, SeasonMeta, ShowMeta};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

/// An episode file waiting for its show.
pub(crate) struct Member {
    pub(crate) pending: Pending,
    title: String,
    year: Option<i32>,
    /// From an `.nfo`.
    show_id: Option<i64>,
    season: i32,
    episode: Option<i32>,
    /// Joined an nfo group without an nfo of its own: matched by lookup, not by the nfo.
    joined: bool,
}

pub(crate) fn member(
    pending: Pending,
    title: String,
    year: Option<i32>,
    show_id: Option<i64>,
    season: i32,
    episode: Option<i32>,
) -> Member {
    Member {
        pending,
        title,
        year,
        show_id,
        season,
        episode,
        joined: false,
    }
}

/// Episode files that name the same show.
pub(crate) struct Group {
    key: String,
    pub(crate) title: String,
    year: Option<i32>,
    /// From an `.nfo`; skips the search.
    show_id: Option<i64>,
    pub(crate) members: Vec<Member>,
}

/// Groups episode files by show. A file with an nfo show id groups by (title, id); a file
/// with a year in its name by (title, year). A file with neither joins the only such group
/// with its title, or, when there are none or several, a group of its own that is searched.
pub(crate) fn groups(members: Vec<Member>) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    let mut loose = Vec::new();
    for m in members {
        let key = normalize_title(&m.title);
        if m.show_id.is_none() && m.year.is_none() {
            loose.push((key, m));
            continue;
        }
        let i = groups.iter().position(|g| {
            g.key == key && g.show_id == m.show_id && (m.show_id.is_some() || g.year == m.year)
        });
        push(&mut groups, i, key, m);
    }
    let anchored = groups.len();
    for (key, mut m) in loose {
        let same: Vec<usize> = (0..anchored).filter(|&i| groups[i].key == key).collect();
        let i = match same[..] {
            [i] => {
                m.joined = groups[i].show_id.is_some();
                Some(i)
            }
            _ => (anchored..groups.len()).find(|&i| groups[i].key == key),
        };
        push(&mut groups, i, key, m);
    }
    groups
}

fn push(groups: &mut Vec<Group>, at: Option<usize>, key: String, m: Member) {
    match at {
        Some(i) => groups[i].members.push(m),
        None => groups.push(Group {
            key,
            title: m.title.clone(),
            year: m.year,
            show_id: m.show_id,
            members: vec![m],
        }),
    }
}

pub(crate) fn identify_group(
    c: &Connection,
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    g: &Group,
) -> Result<Counts> {
    let seasons = || g.members.iter().map(|m| m.season);
    // An nfo id TMDB does not know is ignored: the group is searched by name instead.
    let from_nfo = match g.show_id.map(|id| fetch(p, id, seasons())) {
        Some(Ok(show)) => Some(show),
        None | Some(Err(Error::NotFound(_))) => None,
        Some(Err(e)) => return Err(e),
    };
    let (show, how) = match from_nfo {
        Some(show) => (show, How::certain(MatchSource::Nfo)),
        None => {
            let ev = Evidence {
                title: &g.title,
                year: g.year,
                duration_min: None,
            };
            let hits = p.search_show(&g.title, g.year)?;
            let mut scored: Vec<Candidate> = hits
                .iter()
                .take(MAX_HITS)
                .map(|h| candidate(h, matcher::score(&ev, h, None)))
                .collect();
            sort_candidates(&mut scored);
            match matcher::decide(&scored) {
                Some(best) => (fetch(p, best.tmdb_id, seasons())?, How::lookup(best.score)),
                None => {
                    let tx = db::write_tx(c)?;
                    for m in &g.members {
                        set_review(&tx, m.pending.item_id, &scored)?;
                    }
                    tx.commit()?;
                    return Ok((0, g.members.len() as u32));
                }
            }
        }
    };
    let art = show_artwork(c, p, cache_dir, &show)?;
    let added_at = g.members.iter().map(|m| m.pending.added_at).min();

    let tx = db::write_tx(c)?;
    let show_item = ensure_show(&tx, &show, art, how, added_at.unwrap_or(0))?;
    let (mut matched, mut review) = (0, 0);
    for m in &g.members {
        let item_id = m.pending.item_id;
        let joined = m.joined && how.source == MatchSource::Nfo;
        let m_how = if joined { How::joined() } else { how };
        match m.episode.and_then(|e| show.episode(m.season, e)) {
            Some(ep) => {
                place(
                    &tx,
                    show_item,
                    m.season,
                    ep,
                    m_how,
                    m.pending.added_at,
                    item_id,
                )?;
                matched += 1;
            }
            None => {
                // The show is right but this episode is not in it: leave the file for review.
                let score = how.confidence.unwrap_or(1.0);
                set_review(&tx, item_id, &[show.candidate(score)])?;
                review += 1;
            }
        }
    }
    tx.commit()?;
    Ok((matched, review))
}

/// Places the files of `item_id` on their episode of show `show_id`, per the file name.
pub(crate) fn apply(
    c: &Connection,
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    item_id: Id,
    show_id: i64,
    how: How,
) -> Result<()> {
    let (rels, added_at): (Vec<String>, i64) = {
        let mut s = c.prepare_cached("SELECT rel_path FROM files WHERE item_id=?1 ORDER BY id")?;
        let rels = s
            .query_map([item_id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let added_at = c.query_row("SELECT added_at FROM items WHERE id=?1", [item_id], |r| {
            r.get(0)
        })?;
        (rels, added_at)
    };
    let mut spots = rels
        .iter()
        .map(|rel| {
            let n = parse_path(rel);
            let e = n.episode.ok_or_else(|| {
                Error::Invalid(format!(
                    "cannot determine the episode of {rel}: its name has no episode number"
                ))
            })?;
            Ok((n.season.unwrap_or(1), e))
        })
        .collect::<Result<Vec<_>>>()?;
    spots.sort();
    spots.dedup();
    let [(season, episode)] = spots[..] else {
        return Err(Error::Invalid(format!(
            "item {item_id} needs exactly one episode in its file names, found {}",
            spots.len()
        )));
    };
    let show = fetch(p, show_id, std::iter::once(season))?;
    let Some(ep) = show.episode(season, episode) else {
        return Err(Error::Invalid(format!(
            "{} has no season {season} episode {episode}",
            show.meta.title
        )));
    };
    let art = show_artwork(c, p, cache_dir, &show)?;
    let tx = db::write_tx(c)?;
    let show_item = ensure_show(&tx, &show, art, how, added_at)?;
    place(&tx, show_item, season, ep, how, added_at, item_id)?;
    tx.commit()?;
    Ok(())
}

/// A show with the seasons this run needs.
struct ShowData {
    meta: ShowMeta,
    seasons: Vec<SeasonMeta>,
}

impl ShowData {
    fn episode(&self, season: i32, episode: i32) -> Option<&EpisodeMeta> {
        let s = self.seasons.iter().find(|s| s.season == season)?;
        s.episodes.iter().find(|e| e.episode == episode)
    }

    fn candidate(&self, score: f32) -> Candidate {
        Candidate {
            tmdb_id: self.meta.tmdb_id,
            kind: ItemKind::Show,
            title: self.meta.title.clone(),
            year: self.meta.year,
            score,
        }
    }
}

/// Fetches the show and, once each, the wanted seasons it lists (a season it does not list
/// is not requested; its files go to review).
fn fetch(
    p: &dyn MetadataProvider,
    show_id: i64,
    wanted: impl Iterator<Item = i32>,
) -> Result<ShowData> {
    let meta = p.show(show_id)?;
    let mut numbers: Vec<i32> = wanted
        .filter(|n| meta.seasons.iter().any(|(s, _)| s == n))
        .collect();
    numbers.sort();
    numbers.dedup();
    let seasons = numbers
        .into_iter()
        .map(|n| p.season(show_id, n))
        .collect::<Result<_>>()?;
    Ok(ShowData { meta, seasons })
}

/// Artwork for a show that has no item yet.
fn show_artwork(
    c: &Connection,
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    show: &ShowData,
) -> Result<(Option<String>, Option<String>)> {
    let m = &show.meta;
    Ok(match find_item(c, ItemKind::Show, m.tmdb_id)? {
        Some(_) => (None, None),
        None => artwork(
            p,
            cache_dir,
            "tv",
            m.tmdb_id,
            m.poster.as_deref(),
            m.backdrop.as_deref(),
        ),
    })
}

/// The show's item, created if it does not exist.
fn ensure_show(
    tx: &Connection,
    show: &ShowData,
    (poster, backdrop): (Option<String>, Option<String>),
    how: How,
    added_at: i64,
) -> Result<Id> {
    let m = &show.meta;
    if let Some(id) = find_item(tx, ItemKind::Show, m.tmdb_id)? {
        return Ok(id);
    }
    let fill = Fill {
        year: m.year,
        overview: &m.overview,
        genres: &m.genres,
        people: &m.people,
        poster,
        backdrop,
        tmdb_id: Some(m.tmdb_id),
        ..Fill::new(ItemKind::Show, &m.title)
    };
    insert_item(tx, &fill, how, added_at)
}

/// Moves the files of `from` onto episode `ep` of `season`, creating the season and episode
/// items when missing, and deletes `from`.
fn place(
    tx: &Connection,
    show_item: Id,
    season: i32,
    ep: &EpisodeMeta,
    how: How,
    added_at: i64,
    from: Id,
) -> Result<()> {
    let season_id = match child(tx, show_item, ItemKind::Season, season)? {
        Some(id) => id,
        None => {
            let title = match season {
                0 => "Specials".to_string(),
                n => format!("Season {n}"),
            };
            let fill = Fill {
                parent_id: Some(show_item),
                season: Some(season),
                ..Fill::new(ItemKind::Season, &title)
            };
            insert_item(tx, &fill, how, added_at)?
        }
    };
    let episode_id = match child(tx, season_id, ItemKind::Episode, ep.episode)? {
        Some(id) => {
            confirm(tx, id, how)?;
            id
        }
        None => {
            let fallback = format!("Episode {}", ep.episode);
            let title = if ep.title.is_empty() {
                &fallback
            } else {
                &ep.title
            };
            let fill = Fill {
                parent_id: Some(season_id),
                season: Some(season),
                episode: Some(ep.episode),
                overview: &ep.overview,
                runtime_min: ep.runtime_min,
                ..Fill::new(ItemKind::Episode, title)
            };
            insert_item(tx, &fill, how, added_at)?
        }
    };
    absorb(tx, from, episode_id)
}

/// The season (by season number) or episode (by episode number) under `parent`.
fn child(c: &Connection, parent: Id, kind: ItemKind, number: i32) -> Result<Option<Id>> {
    let sql = match kind {
        ItemKind::Season => "SELECT id FROM items WHERE parent_id=?1 AND kind=?2 AND season=?3",
        _ => "SELECT id FROM items WHERE parent_id=?1 AND kind=?2 AND episode=?3",
    };
    Ok(c.prepare_cached(sql)?
        .query_row(params![parent, enum_str(kind), number], |r| r.get(0))
        .optional()?)
}
