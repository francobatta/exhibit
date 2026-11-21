//! The Wikidata adapter: the awards a film won or was nominated for, from a fixed list of
//! bodies, found by the film's TMDB id. No account and no key. Only this crate names the site.
//!
//! Confirmed on query.wikidata.org on 2026-11-21: P4947 is "TMDB movie ID", P166 "award
//! received", P1411 "nominated for", P585 "point in time". An award belongs to its body through
//! "instance of" (P31: Academy Awards Q19020, Golden Globes Q1011547, Cannes Film Festival
//! Awards Q28444913), "conferred by" (P1027: Golden Lion, Golden Bear) or "part of" (P361).

use std::collections::HashMap;

use exhibit_core::error::{Detail, Error, Result};
use exhibit_core::model::ItemKind;
use exhibit_core::service::{
    Account, Award, BodyInfo, Lookup, Provides, Service, ServiceInfo, TmdbFilm,
};
use exhibit_core::tmdb::{Http, Request, url_encode};
use serde_json::Value;

const ENDPOINT: &str = "https://query.wikidata.org/sparql";
/// Wikimedia asks automated clients to name themselves. A generic name: no personal details.
const USER_AGENT: &str = "Exhibit/0.1 (a desktop media gallery)";

pub struct Wikidata;

/// The bodies a cinephile counts, most prominent first.
static BODIES: [BodyInfo; 9] = [
    body("academy", "Academy Awards"),
    body("cannes", "Cannes"),
    body("venice", "Venice"),
    body("berlin", "Berlin"),
    body("globes", "Golden Globes"),
    body("bafta", "BAFTA"),
    body("cesar", "César"),
    body("spirit", "Independent Spirit Awards"),
    body("sundance", "Sundance"),
];

const fn body(id: &'static str, name: &'static str) -> BodyInfo {
    BodyInfo { id, name }
}

/// The Wikidata items an award of each body hangs from (P31, P1027 or P361).
const FAMILIES: [(&str, &str); 11] = [
    ("academy", "Q19020"),
    ("cannes", "Q28444913"),
    ("cannes", "Q42369"),
    ("venice", "Q49024"),
    ("venice", "Q108773780"),
    ("berlin", "Q130871"),
    ("globes", "Q1011547"),
    ("bafta", "Q732997"),
    ("cesar", "Q174389"),
    ("spirit", "Q311836"),
    ("sundance", "Q189887"),
];

/// Each body's top prize, which leads its wreath.
const HEADLINES: [&str; 11] = [
    "Q102427",  // Academy Award for Best Picture
    "Q179808",  // Palme d'Or
    "Q209459",  // Golden Lion
    "Q154590",  // Golden Bear
    "Q1011509", // Golden Globe for Best Motion Picture – Drama
    "Q670282",  // Golden Globe for Best Motion Picture – Musical or Comedy
    "Q139184",  // BAFTA Award for Best Film
    "Q645595",  // César Award for Best Film
    "Q2544844", // Independent Spirit Award for Best Film
    "Q3774974", // Sundance U.S. Dramatic Grand Jury Prize
    "Q1337827", // Silver Lion
];

static INFO: ServiceInfo = ServiceInfo {
    id: "wikidata",
    name: "Wikidata",
    mark_svg: include_str!("mark.svg"),
    provides: &[Provides::Awards],
    needs: &[],
    export: None,
    origins: &[],
    bodies: &BODIES,
    batch: 50,
};

impl Service for Wikidata {
    fn info(&self) -> &ServiceInfo {
        &INFO
    }

    /// Nothing to check: the service needs no account. Asks one known film to see it answers.
    fn check(&self, _account: &Account, http: &dyn Http) -> Result<u32> {
        let taxi_driver = TmdbFilm {
            kind: ItemKind::Movie,
            tmdb_id: 103,
        };
        let got = self.awards(&[taxi_driver], &["en"], http)?;
        Ok(got.films.len() as u32)
    }

    fn awards(&self, films: &[TmdbFilm], langs: &[&str], http: &dyn Http) -> Result<Lookup<Award>> {
        let ids: Vec<i64> = films
            .iter()
            .filter(|f| f.kind == ItemKind::Movie)
            .map(|f| f.tmdb_id)
            .collect();
        if ids.is_empty() {
            return Ok(Lookup {
                films: Vec::new(),
                quota: None,
            });
        }
        let url = format!(
            "{ENDPOINT}?format=json&query={}",
            url_encode(&query(&ids, langs))
        );
        let req = Request {
            user_agent: Some(USER_AGENT),
            ..Request::get(&url)
        };
        let resp = http.send(&req)?;
        match resp.status {
            200 => {}
            429 => {
                let wait = resp
                    .header("retry-after")
                    .and_then(|v| v.trim().parse::<i64>().ok())
                    .unwrap_or(600);
                return Err(Error::Provider(
                    Detail::coded("service.limit_reached", "the query service asks to wait")
                        .with("until", now() + wait),
                ));
            }
            // A query that ran out of time is the service being unreachable for now.
            s => return Err(unreachable(format!("the query answered {s}"))),
        }
        let v: Value = serde_json::from_slice(&resp.body)
            .map_err(|e| unreachable(format!("unreadable answer: {e}")))?;
        Ok(Lookup {
            films: read(&v, langs),
            quota: None,
        })
    }
}

/// One query for every film in `ids`: their awards and nominations from the fixed bodies, with
/// the year and the award's name in each of `langs`.
fn query(ids: &[i64], langs: &[&str]) -> String {
    let tmdb: Vec<String> = ids.iter().map(|id| format!("\"{id}\"")).collect();
    let families: Vec<String> = FAMILIES.iter().map(|(_, q)| format!("wd:{q}")).collect();
    let (vars, labels): (Vec<String>, Vec<String>) = langs
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let l = l
                .chars()
                .filter(char::is_ascii_alphabetic)
                .collect::<String>();
            (
                format!("?l{i}"),
                format!("OPTIONAL {{ ?award rdfs:label ?l{i} FILTER(LANG(?l{i}) = \"{l}\") }}"),
            )
        })
        .unzip();
    format!(
        "SELECT ?tmdb ?award ?won ?time ?family {vars} WHERE {{ \
           VALUES ?tmdb {{ {tmdb} }} \
           VALUES ?family {{ {families} }} \
           ?film wdt:P4947 ?tmdb . \
           {{ ?film p:P166 ?st . ?st ps:P166 ?award . BIND(1 AS ?won) }} UNION \
           {{ ?film p:P1411 ?st . ?st ps:P1411 ?award . BIND(0 AS ?won) }} \
           ?award (wdt:P31|wdt:P1027|wdt:P361) ?family . \
           OPTIONAL {{ ?st pq:P585 ?time }} \
           {labels} \
         }}",
        vars = vars.join(" "),
        tmdb = tmdb.join(" "),
        families = families.join(" "),
        labels = labels.join(" "),
    )
}

/// The awards per film from the query's rows, one per award and year (a win over a
/// nomination for the same), in the order the films came.
fn read(v: &Value, langs: &[&str]) -> Vec<(TmdbFilm, Vec<Award>)> {
    let rows = v["results"]["bindings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut order: Vec<i64> = Vec::new();
    let mut by_film: HashMap<i64, Vec<Award>> = HashMap::new();
    for row in &rows {
        let text = |k: &str| row[k]["value"].as_str();
        let Some(tmdb_id) = text("tmdb").and_then(|s| s.parse::<i64>().ok()) else {
            continue;
        };
        let Some(award) = text("award").and_then(|u| u.rsplit('/').next()) else {
            continue;
        };
        let Some(body_id) = text("family")
            .and_then(|u| u.rsplit('/').next())
            .and_then(|q| FAMILIES.iter().find(|(_, f)| *f == q))
            .map(|(b, _)| *b)
        else {
            continue;
        };
        let won = text("won") == Some("1");
        let year = text("time")
            .and_then(|t| t.get(..4))
            .and_then(|y| y.parse().ok());
        let names: Vec<(String, String)> = langs
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                let label = text(&format!("l{i}"))?;
                Some((l.to_string(), short_name(label, l)))
            })
            .collect();
        let rank = rank(body_id, award, &names);
        let list = by_film.entry(tmdb_id).or_insert_with(|| {
            order.push(tmdb_id);
            Vec::new()
        });
        match list.iter_mut().find(|a| a.award == award && a.year == year) {
            Some(a) => {
                a.won |= won;
                a.rank = a.rank.min(rank);
            }
            None => list.push(Award {
                body: body_id.to_string(),
                award: award.to_string(),
                names,
                year,
                won,
                rank,
            }),
        }
    }
    order
        .into_iter()
        .map(|id| {
            let film = TmdbFilm {
                kind: ItemKind::Movie,
                tmdb_id: id,
            };
            (film, by_film.remove(&id).unwrap_or_default())
        })
        .collect()
}

/// A top prize before a directing award before the rest, then the body's place in the fixed order:
/// a Palme d'Or leads an Oscar for cinematography.
fn rank(body_id: &str, award: &str, names: &[(String, String)]) -> i32 {
    let body = BODIES
        .iter()
        .position(|b| b.id == body_id)
        .unwrap_or(BODIES.len()) as i32;
    let english = names
        .iter()
        .find(|(l, _)| l == "en")
        .map(|(_, n)| n.as_str())
        .unwrap_or("");
    let tier = if HEADLINES.contains(&award) {
        0
    } else if english.contains("Director") || english.contains("Direction") {
        1
    } else {
        2
    };
    tier * 100 + body
}

/// An award's name without its body: "Academy Award for Best Picture" is "Best Picture",
/// "Óscar a la mejor película" is "Mejor película". A festival's own prize keeps its name.
fn short_name(label: &str, lang: &str) -> String {
    let cut = match lang {
        "en" => label.split_once(" for ").map(|(_, rest)| rest),
        "es" => [" a la ", " al ", " a "]
            .iter()
            .find_map(|sep| label.split_once(sep).map(|(_, rest)| rest)),
        _ => None,
    };
    match cut.map(str::trim).filter(|s| !s.is_empty()) {
        Some(rest) => {
            let mut chars = rest.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
        None => label.to_string(),
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn unreachable(why: String) -> Error {
    Error::Provider(Detail::coded("service.unreachable", why))
}

#[cfg(test)]
mod tests {
    use super::*;
    use exhibit_core::tmdb::Response;
    use std::sync::Mutex;

    struct Fake {
        answer: Response,
        sent: Mutex<Vec<(String, Option<String>)>>,
    }

    impl Http for Fake {
        fn get(&self, _: &str, _: Option<&str>) -> Result<(u16, Vec<u8>)> {
            unreachable!("the adapter sends")
        }

        fn send(&self, req: &Request) -> Result<Response> {
            self.sent
                .lock()
                .unwrap()
                .push((req.url.to_string(), req.user_agent.map(str::to_string)));
            Ok(self.answer.clone())
        }
    }

    fn fake(status: u16, body: &str) -> Fake {
        Fake {
            answer: Response {
                status,
                headers: Vec::new(),
                body: body.as_bytes().to_vec(),
            },
            sent: Mutex::default(),
        }
    }

    fn films(ids: &[i64]) -> Vec<TmdbFilm> {
        ids.iter()
            .map(|&tmdb_id| TmdbFilm {
                kind: ItemKind::Movie,
                tmdb_id,
            })
            .collect()
    }

    #[test]
    fn one_query_names_each_award_in_each_language_and_a_win_beats_its_nomination() {
        let body = std::fs::read_to_string(format!(
            "{}/tests/fixtures/awards.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let f = fake(200, &body);
        let mut asked = films(&[103, 496243]);
        // A show is not asked about.
        asked.push(TmdbFilm {
            kind: ItemKind::Show,
            tmdb_id: 1,
        });
        let got = Wikidata.awards(&asked, &["en", "es"], &f).unwrap();
        let sent = f.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].1.as_deref(), Some(USER_AGENT));
        assert!(
            sent[0]
                .0
                .contains(&url_encode("VALUES ?tmdb { \"103\" \"496243\" }"))
        );

        assert_eq!(got.films.len(), 2);
        let of = |id: i64| &got.films.iter().find(|(f, _)| f.tmdb_id == id).unwrap().1;
        let taxi_awards = of(103);
        let palme = taxi_awards.iter().find(|a| a.award == "Q179808").unwrap();
        assert!(palme.won);
        assert_eq!((palme.body.as_str(), palme.year), ("cannes", Some(1976)));
        assert_eq!(
            palme.names,
            [
                ("en".to_string(), "Palme d'Or".to_string()),
                ("es".to_string(), "Palma de Oro".to_string())
            ]
        );
        assert_eq!(palme.rank, 1);
        let actor = taxi_awards.iter().find(|a| a.award == "Q103916").unwrap();
        assert!(!actor.won);
        assert_eq!(actor.names[0].1, "Best Actor");
        assert_eq!(actor.names[1].1, "Mejor actor");
        assert_eq!(actor.rank, 200);

        // Parasite's Best Picture is listed won and nominated: it is one win.
        let parasite = of(496243);
        let best: Vec<_> = parasite.iter().filter(|a| a.award == "Q102427").collect();
        assert_eq!(best.len(), 1);
        assert!(best[0].won);
        assert_eq!(best[0].rank, 0);
        assert_eq!(best[0].names[0].1, "Best Picture");
    }

    #[test]
    fn a_busy_or_failing_query_service_is_coded() {
        let f = fake(500, "");
        let e = Wikidata.awards(&films(&[1]), &["en"], &f).unwrap_err();
        assert_eq!(e.detail().unwrap().code, Some("service.unreachable"));
        let mut f = fake(429, "");
        f.answer.headers = vec![("retry-after".into(), "60".into())];
        let e = Wikidata.awards(&films(&[1]), &["en"], &f).unwrap_err();
        assert_eq!(e.detail().unwrap().code, Some("service.limit_reached"));
        // Nothing to ask, nothing sent.
        let f = fake(500, "");
        assert!(Wikidata.awards(&[], &["en"], &f).unwrap().films.is_empty());
        assert!(f.sent.lock().unwrap().is_empty());
    }

    #[test]
    fn short_names_lose_their_body() {
        assert_eq!(
            short_name("Academy Award for Best Picture", "en"),
            "Best Picture"
        );
        assert_eq!(short_name("Palme d'Or", "en"), "Palme d'Or");
        assert_eq!(
            short_name("Óscar a la mejor película", "es"),
            "Mejor película"
        );
        assert_eq!(
            short_name("Oso de Plata al mejor director", "es"),
            "Mejor director"
        );
        assert_eq!(short_name("León de Oro", "es"), "León de Oro");
    }
}
