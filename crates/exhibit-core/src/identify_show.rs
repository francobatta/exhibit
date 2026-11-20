//! The show path of identification. Episode files are grouped by normalised title; each
//! group resolves to one TMDB show (from an `.nfo` id, or one scored `search_show`), and on
//! acceptance gets one `Show` item, `Season` items for seasons with local files and
//! `Episode` items for episodes with local files. Files are relinked to their episode and the
//! provisional items deleted.

use crate::db;
use crate::error::{Detail, Error, Result};
use crate::identify::{
    CandidatePosters, Counts, Fill, How, IdentifyReport, MAX_HITS, Pending, STILL_SIZE, absorb,
    artwork, candidate, confirm, download, find_item, insert_item, set_review, sort_candidates,
};
use crate::matcher::{self, Evidence};
use crate::model::{Candidate, Id, ItemKind, MatchSource};
use crate::parse::{normalize_title, parse_path};
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
    posters: &mut CandidatePosters,
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
                    posters.add(&mut scored, &hits)?;
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
    let stills: Vec<Option<String>> = g
        .members
        .iter()
        .map(|m| {
            let ep = show.episode(m.season, m.episode?)?;
            still(p, cache_dir, show.meta.tmdb_id, m.season, ep)
        })
        .collect();
    let added_at = g.members.iter().map(|m| m.pending.added_at).min();
    // Files whose episode the show lacks go to review with the show as their candidate.
    let missing = g
        .members
        .iter()
        .any(|m| m.episode.and_then(|e| show.episode(m.season, e)).is_none());
    let poster = if missing {
        posters.poster(
            ItemKind::Show,
            show.meta.tmdb_id,
            show.meta.poster.as_deref(),
        )?
    } else {
        None
    };

    let tx = db::write_tx(c)?;
    let show_item = ensure_show(&tx, &show, art, how, added_at.unwrap_or(0))?;
    let (mut matched, mut review) = (0, 0);
    for (m, still) in g.members.iter().zip(stills) {
        let item_id = m.pending.item_id;
        let joined = m.joined && how.source == MatchSource::Nfo;
        let m_how = if joined { How::joined() } else { how };
        match m.episode.and_then(|e| show.episode(m.season, e)) {
            Some(ep) => {
                let at = Spot {
                    show_item,
                    season: m.season,
                    ep,
                    still,
                };
                place(&tx, &at, m_how, m.pending.added_at, item_id)?;
                matched += 1;
            }
            None => {
                // The show is right but this episode is not in it: leave the file for review.
                let score = how.confidence.unwrap_or(1.0);
                set_review(&tx, item_id, &[show.candidate(score, poster.clone())])?;
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
                Error::Invalid(
                    Detail::coded(
                        "match.no_episode_number",
                        format!(
                            "cannot determine the episode of {rel}: its name has no episode number"
                        ),
                    )
                    .with("file", rel),
                )
            })?;
            Ok((n.season.unwrap_or(1), e))
        })
        .collect::<Result<Vec<_>>>()?;
    spots.sort();
    spots.dedup();
    let [(season, episode)] = spots[..] else {
        return Err(Error::Invalid(Detail::coded(
            "match.several_episodes",
            format!(
                "item {item_id} needs exactly one episode in its file names, found {}",
                spots.len()
            ),
        )));
    };
    let show = fetch(p, show_id, std::iter::once(season))?;
    let Some(ep) = show.episode(season, episode) else {
        return Err(Error::Invalid(
            Detail::coded(
                "match.no_such_episode",
                format!(
                    "{} has no season {season} episode {episode}",
                    show.meta.title
                ),
            )
            .with("show", &show.meta.title)
            .with("season", season)
            .with("episode", episode),
        ));
    };
    let art = show_artwork(c, p, cache_dir, &show)?;
    let still = still(p, cache_dir, show.meta.tmdb_id, season, ep);
    let tx = db::write_tx(c)?;
    let show_item = ensure_show(&tx, &show, art, how, added_at)?;
    let at = Spot {
        show_item,
        season,
        ep,
        still,
    };
    place(&tx, &at, how, added_at, item_id)?;
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

    fn candidate(&self, score: f32, poster: Option<String>) -> Candidate {
        Candidate {
            tmdb_id: self.meta.tmdb_id,
            kind: ItemKind::Show,
            title: self.meta.title.clone(),
            year: self.meta.year,
            score,
            poster,
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

/// Where an episode file goes: episode `ep` of `season` of `show_item`, with its still.
struct Spot<'a> {
    show_item: Id,
    season: i32,
    ep: &'a EpisodeMeta,
    /// The still's cached file name, kept as the episode's backdrop.
    still: Option<String>,
}

/// An episode's still, saved as `tv-<show id>-s<season>e<episode>-backdrop.jpg` and returned
/// by name; `None` when TMDB has none or the download failed, which never fails the match.
fn still(
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    show_id: i64,
    season: i32,
    ep: &EpisodeMeta,
) -> Option<String> {
    let name = still_name(show_id, season, ep.episode);
    download(p, cache_dir, ep.still.as_deref()?, STILL_SIZE, &name).ok()?;
    Some(name)
}

fn still_name(show_id: i64, season: i32, episode: i32) -> String {
    format!("tv-{show_id}-s{season}e{episode}-backdrop.jpg")
}

/// Moves the files of `from` onto the episode at `at`, creating the season and episode items
/// when missing (an episode without a backdrop takes the still), and deletes `from`.
fn place(tx: &Connection, at: &Spot, how: How, added_at: i64, from: Id) -> Result<()> {
    let Spot {
        show_item,
        season,
        ep,
        ..
    } = *at;
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
            tx.prepare_cached("UPDATE items SET backdrop = COALESCE(backdrop, ?2) WHERE id = ?1")?
                .execute(params![id, at.still])?;
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
                backdrop: at.still.clone(),
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
        ItemKind::Season => {
            "SELECT id FROM items WHERE parent_id=?1 AND kind='season' AND season=?2"
        }
        _ => "SELECT id FROM items WHERE parent_id=?1 AND kind='episode' AND episode=?2",
    };
    Ok(c.prepare_cached(sql)?
        .query_row(params![parent, number], |r| r.get(0))
        .optional()?)
}

/// Gives every episode of an identified show that has no backdrop yet its TMDB still (episodes
/// identified before stills were kept, or whose still could not be downloaded then). Each
/// season is fetched once. A failure is added to the report's errors and the run goes on; a
/// rejected key ends it. After the first provider error (TMDB out of reach) the rest wait for
/// the next run, so an outage costs one timeout. An episode TMDB has no still for keeps no
/// backdrop.
pub(crate) fn episode_stills(
    c: &Connection,
    p: &dyn MetadataProvider,
    cache_dir: &Path,
    report: &mut IdentifyReport,
) -> Result<()> {
    let missing: Vec<(Id, i64, i32, i32)> = c
        .prepare_cached(
            "SELECT e.id, s.tmdb_id, e.season, e.episode FROM items e \
             JOIN items up ON up.id = e.parent_id \
             JOIN items s ON s.id = CASE WHEN up.kind = 'show' THEN up.id ELSE up.parent_id END \
             WHERE e.kind = 'episode' AND e.backdrop IS NULL \
               AND e.season IS NOT NULL AND e.episode IS NOT NULL \
               AND s.kind = 'show' AND s.tmdb_id IS NOT NULL \
             ORDER BY s.tmdb_id, e.season, e.episode",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<_>>()?;
    // The season in hand, by (show, season number); None when it could not be read.
    let mut season: Option<((i64, i32), Option<SeasonMeta>)> = None;
    for (id, show_id, n, episode) in missing {
        if season.as_ref().is_none_or(|(key, _)| *key != (show_id, n)) {
            let meta = match p.season(show_id, n) {
                Ok(meta) => Some(meta),
                Err(e @ Error::Invalid(_)) => return Err(e),
                // A season TMDB does not list has no stills to give.
                Err(Error::NotFound(_)) => None,
                Err(e) => {
                    let unreachable = matches!(e, Error::Provider(_));
                    report
                        .errors
                        .push(format!("stills of {show_id} season {n}: {e}"));
                    if unreachable {
                        break;
                    }
                    None
                }
            };
            season = Some(((show_id, n), meta));
        }
        let Some((_, Some(meta))) = &season else {
            continue;
        };
        let Some(still) = meta
            .episodes
            .iter()
            .find(|e| e.episode == episode)
            .and_then(|e| e.still.as_deref())
        else {
            continue;
        };
        let name = still_name(show_id, n, episode);
        match download(p, cache_dir, still, STILL_SIZE, &name) {
            Ok(()) => {
                c.prepare_cached("UPDATE items SET backdrop = ?2 WHERE id = ?1")?
                    .execute(params![id, name])?;
            }
            Err(e @ Error::Invalid(_)) => return Err(e),
            Err(e) => {
                let unreachable = matches!(e, Error::Provider(_));
                report.errors.push(format!("still for {name}: {e}"));
                if unreachable {
                    break;
                }
            }
        }
    }
    Ok(())
}
