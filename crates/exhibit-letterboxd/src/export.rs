//! The account export: a ZIP of CSV files, read by header name.

use crate::{dated, film_ref, parse_date};
use exhibit_core::error::{Detail, Error, Result};
use exhibit_core::service::{ActivityBatch, Dated, Fact, FilmRef};
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

fn bad(why: impl Into<String>) -> Error {
    Error::Invalid(Detail::coded("service.bad_export", why))
}

type Archive = ZipArchive<std::fs::File>;

/// One CSV's rows as header-name lookups; a file the export lacks is an empty set.
struct Rows {
    headers: csv::StringRecord,
    records: Vec<csv::StringRecord>,
}

impl Rows {
    fn load(zip: &mut Archive, name: &str) -> Result<Rows> {
        let mut text = Vec::new();
        match zip.by_name(name) {
            Ok(mut f) => f.read_to_end(&mut text).map_err(Error::Io).map(|_| ())?,
            Err(_) => {
                return Ok(Rows {
                    headers: csv::StringRecord::new(),
                    records: Vec::new(),
                });
            }
        }
        let mut reader = csv::ReaderBuilder::new()
            .flexible(true)
            .from_reader(text.as_slice());
        let headers = reader
            .headers()
            .map_err(|e| bad(format!("{name} is not readable: {e}")))?
            .clone();
        let records = reader
            .records()
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| bad(format!("{name} is not readable: {e}")))?;
        Ok(Rows { headers, records })
    }

    fn each<'a>(&'a self) -> impl Iterator<Item = Row<'a>> {
        self.records.iter().map(|r| Row {
            rows: self,
            record: r,
        })
    }
}

struct Row<'a> {
    rows: &'a Rows,
    record: &'a csv::StringRecord,
}

impl Row<'_> {
    fn get(&self, column: &str) -> Option<&str> {
        let i = self.rows.headers.iter().position(|h| h.trim() == column)?;
        self.record.get(i).map(str::trim).filter(|v| !v.is_empty())
    }

    fn film(&self) -> Option<FilmRef> {
        let year = self.get("Year").and_then(|y| y.parse().ok());
        Some(film_ref(self.get("Name")?, year, None))
    }

    fn rating(&self) -> Option<Fact> {
        let value = self.get("Rating")?.parse().ok()?;
        Some(Fact::Rating { value, scale: 5.0 })
    }
}

pub fn read(path: &Path) -> Result<ActivityBatch> {
    let file = std::fs::File::open(path).map_err(Error::Io)?;
    let mut zip = ZipArchive::new(file).map_err(|_| bad("the file is not a ZIP"))?;
    if zip.by_name("watched.csv").is_err() {
        return Err(bad("the export has no watched.csv"));
    }
    let mut facts: Vec<Dated> = Vec::new();

    for row in Rows::load(&mut zip, "watched.csv")?.each() {
        if let Some(film) = row.film() {
            facts.push(dated(&film, Fact::Watch { rewatch: false }, None));
        }
    }
    for row in Rows::load(&mut zip, "ratings.csv")?.each() {
        if let (Some(film), Some(rating)) = (row.film(), row.rating()) {
            facts.push(dated(&film, rating, None));
        }
    }
    for row in Rows::load(&mut zip, "diary.csv")?.each() {
        let Some(film) = row.film() else { continue };
        let on = row.get("Watched Date").and_then(parse_date);
        let rewatch = row.get("Rewatch") == Some("Yes");
        facts.push(dated(&film, Fact::Watch { rewatch }, on));
        if let Some(rating) = row.rating() {
            facts.push(dated(&film, rating, on));
        }
    }
    for row in Rows::load(&mut zip, "reviews.csv")?.each() {
        if let (Some(film), Some(text)) = (row.film(), row.get("Review")) {
            let on = row.get("Watched Date").and_then(parse_date);
            let fact = Fact::Review {
                text: text.to_string(),
                spoiler: false,
            };
            facts.push(dated(&film, fact, on));
        }
    }
    for row in Rows::load(&mut zip, "likes/films.csv")?.each() {
        if let Some(film) = row.film() {
            facts.push(dated(&film, Fact::Like, None));
        }
    }
    for row in Rows::load(&mut zip, "watchlist.csv")?.each() {
        if let Some(film) = row.film() {
            let on = row.get("Date").and_then(parse_date);
            facts.push(dated(&film, Fact::Watchlisted, on));
        }
    }
    Ok(ActivityBatch { facts })
}
