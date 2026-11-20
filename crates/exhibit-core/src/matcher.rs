//! Confidence scoring for metadata search hits, and the accept-or-review decision.

use crate::model::Candidate;
use crate::parse::normalize_title;
use crate::tmdb::SearchHit;

/// A hit is accepted only at or above this score...
pub const ACCEPT_SCORE: f32 = 0.80;
/// ...and only when it leads the runner-up by at least this much.
pub const ACCEPT_MARGIN: f32 = 0.08;

const TITLE_WEIGHT: f32 = 0.65;
const YEAR_WEIGHT: f32 = 0.25;
const RUNTIME_WEIGHT: f32 = 0.10;

const YEAR_EXACT: f32 = 1.0;
const YEAR_OFF_BY_ONE: f32 = 0.6;
/// Score for a fact we cannot compare because our side does not know it.
const UNKNOWN: f32 = 0.5;
const RUNTIME_CLOSE: f64 = 0.05;
const RUNTIME_NEAR: f64 = 0.15;
const RUNTIME_NEAR_SCORE: f32 = 0.5;

/// What we know about a file: parsed (or `.nfo`) title and year, probed duration.
pub struct Evidence<'a> {
    pub title: &'a str,
    pub year: Option<i32>,
    pub duration_min: Option<f64>,
}

/// How well `hit` fits the evidence, 0.0..=1.0.
pub fn score(e: &Evidence, hit: &SearchHit, hit_runtime_min: Option<i32>) -> f32 {
    let ours = normalize_title(e.title);
    // Its English and original titles, and its title in every language that found it.
    let title = [&hit.title, &hit.original_title]
        .into_iter()
        .chain(hit.titles.iter().map(|(_, t)| t))
        .map(|t| strsim::jaro_winkler(&ours, &normalize_title(t)) as f32)
        .fold(0.0, f32::max);

    // A hit without a year cannot confirm ours, so it earns nothing for year.
    let year = match (e.year, hit.year) {
        (None, _) => UNKNOWN,
        (Some(a), Some(b)) if a == b => YEAR_EXACT,
        (Some(a), Some(b)) if (a - b).abs() == 1 => YEAR_OFF_BY_ONE,
        _ => 0.0,
    };

    let runtime = match (e.duration_min, hit_runtime_min) {
        (Some(d), Some(r)) if r > 0 => {
            let off = (d - f64::from(r)).abs() / f64::from(r);
            if off <= RUNTIME_CLOSE {
                1.0
            } else if off <= RUNTIME_NEAR {
                RUNTIME_NEAR_SCORE
            } else {
                0.0
            }
        }
        _ => UNKNOWN,
    };

    (TITLE_WEIGHT * title + YEAR_WEIGHT * year + RUNTIME_WEIGHT * runtime).clamp(0.0, 1.0)
}

/// The best candidate, if it scores at least `ACCEPT_SCORE` and leads the runner-up by at
/// least `ACCEPT_MARGIN`; otherwise `None` (needs review).
pub fn decide(scored: &[Candidate]) -> Option<&Candidate> {
    let best = scored.iter().max_by(|a, b| a.score.total_cmp(&b.score))?;
    let runner_up = scored
        .iter()
        .filter(|c| !std::ptr::eq(*c, best))
        .map(|c| c.score)
        .fold(0.0, f32::max);
    (best.score >= ACCEPT_SCORE && best.score - runner_up >= ACCEPT_MARGIN).then_some(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ItemKind;

    fn hit(id: i64, title: &str, year: Option<i32>) -> SearchHit {
        SearchHit {
            tmdb_id: id,
            kind: ItemKind::Movie,
            title: title.into(),
            original_title: title.into(),
            year,
            popularity: 1.0,
            poster: None,
            titles: Vec::new(),
        }
    }

    fn scored(e: &Evidence, hits: &[(SearchHit, Option<i32>)]) -> Vec<Candidate> {
        hits.iter()
            .map(|(h, runtime)| Candidate {
                tmdb_id: h.tmdb_id,
                kind: h.kind,
                title: h.title.clone(),
                year: h.year,
                score: score(e, h, *runtime),
                poster: None,
                titles: Default::default(),
            })
            .collect()
    }

    #[test]
    fn exact_title_and_year_accepts() {
        let e = Evidence {
            title: "The Matrix",
            year: Some(1999),
            duration_min: None,
        };
        let s = score(&e, &hit(603, "The Matrix", Some(1999)), Some(136));
        assert!(s >= 0.9, "{s}");
        let c = scored(&e, &[(hit(603, "The Matrix", Some(1999)), Some(136))]);
        assert_eq!(decide(&c).map(|c| c.tmdb_id), Some(603));
    }

    #[test]
    fn same_title_wrong_year_not_accepted() {
        let e = Evidence {
            title: "Dune",
            year: Some(1984),
            duration_min: None,
        };
        let c = scored(&e, &[(hit(438631, "Dune", Some(2021)), Some(155))]);
        assert_eq!(decide(&c), None);
    }

    #[test]
    fn ambiguous_remakes_need_review() {
        let e = Evidence {
            title: "Solaris",
            year: None,
            duration_min: None,
        };
        let c = scored(
            &e,
            &[
                (hit(593, "Solaris", Some(1972)), Some(167)),
                (hit(2103, "Solaris", Some(2002)), Some(99)),
            ],
        );
        assert_eq!(decide(&c), None);
    }

    #[test]
    fn runtime_breaks_tie() {
        let e = Evidence {
            title: "Solaris",
            year: None,
            duration_min: Some(167.0),
        };
        let c = scored(
            &e,
            &[
                (hit(2103, "Solaris", Some(2002)), Some(99)),
                (hit(593, "Solaris", Some(1972)), Some(167)),
            ],
        );
        assert_eq!(decide(&c).map(|c| c.tmdb_id), Some(593));
    }

    #[test]
    fn original_title_counts_and_off_by_one_year_scores_less() {
        let e = Evidence {
            title: "Sen to Chihiro no Kamikakushi",
            year: Some(2001),
            duration_min: None,
        };
        let mut h = hit(129, "Spirited Away", Some(2001));
        h.original_title = "Sen to Chihiro no Kamikakushi".into();
        let exact = score(&e, &h, None);
        h.year = Some(2002);
        let off = score(&e, &h, None);
        assert!(exact >= ACCEPT_SCORE && off < exact, "{exact} {off}");
    }

    #[test]
    fn empty_and_single_low_candidate() {
        assert_eq!(decide(&[]), None);
        let low = Candidate {
            tmdb_id: 1,
            kind: ItemKind::Movie,
            title: "x".into(),
            year: None,
            score: 0.79,
            poster: None,
            titles: Default::default(),
        };
        assert_eq!(decide(std::slice::from_ref(&low)), None);
    }
}
