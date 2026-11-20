//! Narrowing a listing: the filters a search carries, the SQL each one becomes, and the facets a
//! listing can be narrowed by. A work matches a technical filter when any file beneath it does
//! (a film in 4K and 1080p is both; a show with one 4K episode is 4K). Within a facet the choices
//! are alternatives (4K or 1080p); across facets they all apply. Genres also take exclusions, and
//! their choices can all be required instead.

use crate::activity::{logged_sql, on_watchlist_sql, rated_sql, reviewed_sql, watched_sql};
use crate::error::Result;
use crate::model::ItemKind;
use crate::query::{DEFAULT_KINDS, LIB_CTE, Params, TREE_CTE, online_sources};
use crate::rows::{LOCAL_GENRES, enum_str};
use crate::volume::Volumes;
use rusqlite::functions::FunctionFlags;
use rusqlite::{Connection, params_from_iter};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Seen {
    /// Not watched and not started.
    Unwatched,
    /// Started and not finished; for a show, some episodes watched.
    Started,
    Watched,
}

/// The picture's size, by the same bounds as the window's marks: a frame cropped to a wide ratio
/// counts by its width.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    Uhd,
    FullHd,
    Hd,
    Sd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Picture {
    DolbyVision,
    Hdr10,
    Hlg,
    Sdr,
}

/// What a sound track offers; a commentary does not count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sound {
    Atmos,
    DtsX,
    /// Dolby TrueHD, DTS-HD Master Audio, FLAC, PCM.
    Lossless,
    /// Six channels or more.
    Surround,
}

/// Every filter is optional: an empty list or `None` leaves the listing whole.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Filters {
    /// Genres in the library's language, as `facets` names them. All are required, or any one
    /// with `any_genre`.
    pub genres: Vec<String>,
    pub any_genre: bool,
    pub without_genres: Vec<String>,
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
    /// Minutes.
    pub runtime_min: Option<i32>,
    pub runtime_max: Option<i32>,
    pub seen: Vec<Seen>,
    pub loved: Option<bool>,
    /// On a source that is online now.
    pub available: Option<bool>,
    pub resolution: Vec<Resolution>,
    pub picture: Vec<Picture>,
    pub sound: Vec<Sound>,
    /// Languages a work can be heard in, as `facets` names them ("en", "es-419").
    pub audio: Vec<String>,
    pub subtitles: Vec<String>,
    /// A switched-on service has the owner's rating, review or logged watch of the work.
    pub rated: Option<bool>,
    pub reviewed: Option<bool>,
    pub logged: Option<bool>,
    /// On the owner's watchlist and not watched. `Some(true)` also lists the films the owner
    /// does not have yet.
    pub watchlist: Option<bool>,
}

/// One value of a facet and how many works have it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Count<T> {
    pub value: T,
    pub count: u32,
}

/// What a listing can be narrowed by, over all of it: the values present and how many works have
/// each. Genres and languages come most common first, years and runtimes in order.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Facets {
    pub genres: Vec<Count<String>>,
    pub years: Vec<Count<i32>>,
    pub runtimes: Vec<Count<i32>>,
    pub resolution: Vec<Count<String>>,
    pub picture: Vec<Count<String>>,
    pub sound: Vec<Count<String>>,
    pub audio: Vec<Count<String>>,
    pub subtitles: Vec<Count<String>>,
    pub activity: ActivityCounts,
}

/// How many works each "your activity" filter would list. `watchlist` includes the films not in
/// the library, and `not_in_library` counts those alone.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ActivityCounts {
    pub rated: u32,
    pub reviewed: u32,
    pub logged: u32,
    pub watchlist: u32,
    pub not_in_library: u32,
}

/// A file's picture size (`files f`); NULL for a file with no picture.
const RESOLUTION: &str = "CASE WHEN coalesce(f.tech ->> 'height', 0) = 0 THEN NULL \
     WHEN f.tech ->> 'height' >= 2000 OR f.tech ->> 'width' >= 3200 THEN 'uhd' \
     WHEN f.tech ->> 'height' >= 1000 OR f.tech ->> 'width' >= 1800 THEN 'full_hd' \
     WHEN f.tech ->> 'height' >= 700 OR f.tech ->> 'width' >= 1200 THEN 'hd' ELSE 'sd' END";

/// A file's dynamic range (`files f`).
const PICTURE: &str = "CASE WHEN f.tech ->> 'hdr' LIKE 'Dolby Vision%' THEN 'dolby_vision' \
     WHEN f.tech ->> 'hdr' LIKE 'HDR10%' THEN 'hdr10' WHEN f.tech ->> 'hdr' = 'HLG' THEN 'hlg' \
     ELSE 'sdr' END";

const SOUNDS: &str = r#"'["atmos","dts_x","lossless","surround"]'"#;

/// Whether the audio track `t` (a JSON object) offers the sound named by `class`.
fn sound_is(class: &str, t: &str) -> String {
    format!(
        "NOT coalesce({t} ->> 'commentary', 0) AND CASE {class} \
         WHEN 'atmos' THEN {t} ->> 'profile' LIKE '%Atmos%' \
         WHEN 'dts_x' THEN {t} ->> 'profile' LIKE '%DTS:X%' \
         WHEN 'lossless' THEN lower({t} ->> 'codec') IN ('truehd', 'mlp', 'flac', 'alac') \
           OR lower({t} ->> 'codec') LIKE 'pcm%' OR {t} ->> 'profile' LIKE '%DTS-HD MA%' \
         WHEN 'surround' THEN {t} ->> 'channels' >= 6 END"
    )
}

/// Some file at or beneath the work `b` meets `cond` (on `files f`).
fn any_file(cond: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM tree t JOIN files f ON f.item_id = t.id \
         WHERE t.root_id = b.id AND f.tech IS NOT NULL AND {cond})"
    )
}

/// `value IN` the JSON array bound for `list`.
fn among(p: &mut Params, value: &str, list: &impl Serialize) -> String {
    let json = serde_json::to_string(list).unwrap_or_else(|_| "[]".into());
    format!("{value} IN (SELECT value FROM json_each({}))", p.bind(json))
}

/// The conditions on the summary row `b` (see `query::list`) that `f` asks for.
pub(crate) fn conditions(f: &Filters, p: &mut Params) -> Vec<String> {
    let mut w = Vec::new();
    let lower = |list: &[String]| list.iter().map(|g| g.to_lowercase()).collect::<Vec<_>>();
    if !f.genres.is_empty() {
        let mut wanted = lower(&f.genres);
        wanted.sort();
        wanted.dedup();
        let has = among(p, "lower(g.value)", &wanted);
        let hits = format!(
            "(SELECT count(DISTINCT lower(g.value)) FROM json_each(b.genres) g WHERE {has})"
        );
        w.push(if f.any_genre {
            format!("{hits} > 0")
        } else {
            format!("{hits} = {}", wanted.len())
        });
    }
    if !f.without_genres.is_empty() {
        let has = among(p, "lower(g.value)", &lower(&f.without_genres));
        w.push(format!(
            "NOT EXISTS (SELECT 1 FROM json_each(b.genres) g WHERE {has})"
        ));
    }
    for (column, bound, op) in [
        ("b.year", f.year_min, ">="),
        ("b.year", f.year_max, "<="),
        ("b.runtime_min", f.runtime_min, ">="),
        ("b.runtime_min", f.runtime_max, "<="),
    ] {
        if let Some(v) = bound {
            w.push(format!("{column} {op} {}", p.bind(v)));
        }
    }
    if !f.seen.is_empty() {
        let any: Vec<&str> = f
            .seen
            .iter()
            .map(|s| match s {
                Seen::Watched => "b.watched",
                Seen::Started => "(NOT b.watched AND coalesce(b.progress, 0) > 0)",
                Seen::Unwatched => "(NOT b.watched AND coalesce(b.progress, 0) = 0)",
            })
            .collect();
        w.push(format!("({})", any.join(" OR ")));
    }
    if let Some(x) = f.loved {
        w.push(format!("b.loved = {}", p.bind(x)));
    }
    if let Some(x) = f.available {
        // Offline means a file the owner has is out of reach, not that there is no file.
        w.push(format!("b.available = {} AND b.in_library", p.bind(x)));
    }
    for (x, has) in [
        (f.rated, rated_sql("b")),
        (f.reviewed, reviewed_sql("b")),
        (f.logged, logged_sql("b")),
        (f.watchlist, on_watchlist_sql("b", "b.watched")),
    ] {
        if let Some(x) = x {
            w.push(format!("({has}) = {}", p.bind(x)));
        }
    }
    if !f.resolution.is_empty() {
        w.push(any_file(&among(p, RESOLUTION, &f.resolution)));
    }
    if !f.picture.is_empty() {
        w.push(any_file(&among(p, PICTURE, &f.picture)));
    }
    if !f.sound.is_empty() {
        let set = serde_json::to_string(&f.sound).unwrap_or_else(|_| "[]".into());
        let cond = format!(
            "EXISTS (SELECT 1 FROM json_each(f.tech, '$.audio') a, json_each({}) s WHERE {})",
            p.bind(set),
            sound_is("s.value", "a.value")
        );
        w.push(any_file(&cond));
    }
    for (tracks, langs) in [("audio", &f.audio), ("subtitles", &f.subtitles)] {
        if langs.is_empty() {
            continue;
        }
        let has = among(p, "track_lang(a.value ->> 'lang')", langs);
        w.push(any_file(&format!(
            "EXISTS (SELECT 1 FROM json_each(f.tech, '$.{tracks}') a \
             WHERE NOT coalesce(a.value ->> 'commentary', 0) AND {has})"
        )));
    }
    w
}

pub fn facets(c: &Connection, v: &dyn Volumes, kind: Option<ItemKind>) -> Result<Facets> {
    let mut p = Params::new(c, &online_sources(c, v)?)?;
    let kinds = match kind {
        Some(k) => format!("b.kind = {}", p.bind(enum_str(k))),
        None => DEFAULT_KINDS.to_string(),
    };
    let langs = |tracks: &str| {
        format!(
            "SELECT '{tracks}', track_lang(a.value ->> 'lang') l, count(DISTINCT f.root) FROM ff f, \
             json_each(f.tech, '$.{tracks}') a \
             WHERE NOT coalesce(a.value ->> 'commentary', 0) AND l IS NOT NULL GROUP BY 2"
        )
    };
    let sql = format!(
        "WITH RECURSIVE {TREE_CTE}, {LIB_CTE}, \
         roots(id) AS (SELECT b.id FROM items b JOIN lib ON lib.item_id = b.id WHERE {kinds}), \
         ff(root, tech) AS (SELECT r.id, f.tech FROM roots r JOIN tree t ON t.root_id = r.id \
           JOIN files f ON f.item_id = t.id WHERE f.tech IS NOT NULL), \
         act(in_lib, rated, reviewed, logged, watchlist) AS (SELECT lib.item_id IS NOT NULL, \
           {rated}, {reviewed}, {logged}, {watchlist} FROM items b \
           LEFT JOIN lib ON lib.item_id = b.id LEFT JOIN play_state p ON p.item_id = b.id \
           WHERE {kinds} AND (lib.item_id IS NOT NULL OR b.wanted = 1)) \
         SELECT 'genres', g.value, count(DISTINCT i.id) FROM roots r JOIN items i ON i.id = r.id \
           LEFT JOIN item_texts tx ON tx.item_id = i.id AND tx.lang = ?2, json_each({LOCAL_GENRES}) g \
           GROUP BY 2 \
         UNION ALL SELECT 'years', i.year, count(*) FROM roots r JOIN items i ON i.id = r.id \
           WHERE i.year IS NOT NULL GROUP BY 2 \
         UNION ALL SELECT 'runtimes', i.runtime_min, count(*) FROM roots r JOIN items i ON i.id = r.id \
           WHERE i.runtime_min > 0 GROUP BY 2 \
         UNION ALL SELECT 'resolution', {RESOLUTION} v, count(DISTINCT f.root) FROM ff f \
           WHERE v IS NOT NULL GROUP BY 2 \
         UNION ALL SELECT 'picture', {PICTURE}, count(DISTINCT f.root) FROM ff f GROUP BY 2 \
         UNION ALL SELECT 'sound', s.value, count(DISTINCT f.root) FROM ff f, \
           json_each(f.tech, '$.audio') a, json_each({SOUNDS}) s WHERE {sound} GROUP BY 2 \
         UNION ALL {audio} UNION ALL {subtitles} \
         UNION ALL SELECT 'activity', 'rated', count(*) FROM act WHERE in_lib AND rated \
         UNION ALL SELECT 'activity', 'reviewed', count(*) FROM act WHERE in_lib AND reviewed \
         UNION ALL SELECT 'activity', 'logged', count(*) FROM act WHERE in_lib AND logged \
         UNION ALL SELECT 'activity', 'watchlist', count(*) FROM act WHERE watchlist \
         UNION ALL SELECT 'activity', 'not_in_library', count(*) FROM act \
           WHERE watchlist AND NOT in_lib",
        rated = rated_sql("b"),
        reviewed = reviewed_sql("b"),
        logged = logged_sql("b"),
        watchlist = on_watchlist_sql("b", &watched_sql("b", "p")),
        sound = sound_is("s.value", "a.value"),
        audio = langs("audio"),
        subtitles = langs("subtitles"),
    );
    let mut out = Facets::default();
    let mut s = c.prepare_cached(&sql)?;
    let mut rows = s.query(params_from_iter(p.0.iter()))?;
    while let Some(r) = rows.next()? {
        let facet: String = r.get(0)?;
        let count: u32 = r.get(2)?;
        let text = || r.get::<_, String>(1).map(|value| Count { value, count });
        let number = || r.get::<_, i32>(1).map(|value| Count { value, count });
        match facet.as_str() {
            "genres" => out.genres.push(text()?),
            "years" => out.years.push(number()?),
            "runtimes" => out.runtimes.push(number()?),
            "resolution" => out.resolution.push(text()?),
            "picture" => out.picture.push(text()?),
            "sound" => out.sound.push(text()?),
            "audio" => out.audio.push(text()?),
            "activity" => {
                let n = &mut out.activity;
                match r.get::<_, String>(1)?.as_str() {
                    "rated" => n.rated = count,
                    "reviewed" => n.reviewed = count,
                    "logged" => n.logged = count,
                    "watchlist" => n.watchlist = count,
                    _ => n.not_in_library = count,
                }
            }
            _ => out.subtitles.push(text()?),
        }
    }
    let common_first = |list: &mut Vec<Count<String>>| {
        list.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.value.cmp(&b.value)))
    };
    for list in [&mut out.genres, &mut out.audio, &mut out.subtitles] {
        common_first(list);
    }
    for list in [&mut out.resolution, &mut out.picture, &mut out.sound] {
        list.sort_by(|a, b| a.value.cmp(&b.value));
    }
    out.years.sort_by_key(|c| c.value);
    out.runtimes.sort_by_key(|c| c.value);
    Ok(out)
}

/// ISO 639-2 codes (both the bibliographic and the terminology form) of the languages with a
/// two-letter code, to that code. Files tag tracks either way, and some with two letters.
const ISO_639_2: &[(&str, &str)] = &[
    ("afr", "af"),
    ("alb", "sq"),
    ("amh", "am"),
    ("ara", "ar"),
    ("arm", "hy"),
    ("aze", "az"),
    ("baq", "eu"),
    ("bel", "be"),
    ("ben", "bn"),
    ("bos", "bs"),
    ("bre", "br"),
    ("bul", "bg"),
    ("bur", "my"),
    ("cat", "ca"),
    ("ces", "cs"),
    ("chi", "zh"),
    ("cym", "cy"),
    ("cze", "cs"),
    ("dan", "da"),
    ("deu", "de"),
    ("dut", "nl"),
    ("ell", "el"),
    ("eng", "en"),
    ("epo", "eo"),
    ("est", "et"),
    ("eus", "eu"),
    ("fao", "fo"),
    ("fas", "fa"),
    ("fil", "tl"),
    ("fin", "fi"),
    ("fra", "fr"),
    ("fre", "fr"),
    ("fry", "fy"),
    ("geo", "ka"),
    ("ger", "de"),
    ("gla", "gd"),
    ("gle", "ga"),
    ("glg", "gl"),
    ("gre", "el"),
    ("guj", "gu"),
    ("hau", "ha"),
    ("heb", "he"),
    ("hin", "hi"),
    ("hrv", "hr"),
    ("hun", "hu"),
    ("hye", "hy"),
    ("ibo", "ig"),
    ("ice", "is"),
    ("ind", "id"),
    ("isl", "is"),
    ("ita", "it"),
    ("jav", "jv"),
    ("jpn", "ja"),
    ("kan", "kn"),
    ("kat", "ka"),
    ("kaz", "kk"),
    ("khm", "km"),
    ("kin", "rw"),
    ("kir", "ky"),
    ("kor", "ko"),
    ("kur", "ku"),
    ("lao", "lo"),
    ("lat", "la"),
    ("lav", "lv"),
    ("lit", "lt"),
    ("ltz", "lb"),
    ("mac", "mk"),
    ("mal", "ml"),
    ("mao", "mi"),
    ("mar", "mr"),
    ("may", "ms"),
    ("mkd", "mk"),
    ("mlt", "mt"),
    ("mon", "mn"),
    ("mri", "mi"),
    ("msa", "ms"),
    ("mya", "my"),
    ("nep", "ne"),
    ("nld", "nl"),
    ("nno", "nn"),
    ("nob", "nb"),
    ("nor", "no"),
    ("ori", "or"),
    ("pan", "pa"),
    ("per", "fa"),
    ("pol", "pl"),
    ("por", "pt"),
    ("pus", "ps"),
    ("que", "qu"),
    ("roh", "rm"),
    ("ron", "ro"),
    ("rum", "ro"),
    ("rus", "ru"),
    ("sin", "si"),
    ("slk", "sk"),
    ("slo", "sk"),
    ("slv", "sl"),
    ("som", "so"),
    ("spa", "es"),
    ("sqi", "sq"),
    ("srp", "sr"),
    ("swa", "sw"),
    ("swe", "sv"),
    ("tam", "ta"),
    ("tat", "tt"),
    ("tel", "te"),
    ("tgk", "tg"),
    ("tgl", "tl"),
    ("tha", "th"),
    ("tib", "bo"),
    ("bod", "bo"),
    ("tuk", "tk"),
    ("tur", "tr"),
    ("uig", "ug"),
    ("ukr", "uk"),
    ("urd", "ur"),
    ("uzb", "uz"),
    ("vie", "vi"),
    ("wel", "cy"),
    ("xho", "xh"),
    ("yid", "yi"),
    ("yor", "yo"),
    ("zho", "zh"),
    ("zul", "zu"),
];

/// A track's language tag in one spelling: the two-letter code where there is one, lower case,
/// a region kept ("spa" and "es" are "es"; "es-419" stays). Norwegian Bokmal is Norwegian. None
/// for no language, several or an unknown one ("und", "mul", "zxx").
pub fn track_lang(tag: &str) -> Option<String> {
    let tag = tag.trim().to_ascii_lowercase().replace('_', "-");
    let (head, tail) = tag.split_once('-').unwrap_or((&tag, ""));
    if head.is_empty() || matches!(head, "und" | "mul" | "zxx" | "mis") || head.starts_with("qa") {
        return None;
    }
    let short = ISO_639_2
        .iter()
        .find(|(long, _)| *long == head)
        .map_or(head, |(_, s)| s);
    let short = if short == "nb" { "no" } else { short };
    Some(if tail.is_empty() {
        short.to_string()
    } else {
        format!("{short}-{tail}")
    })
}

/// Makes `track_lang` callable from SQL on `c`.
pub(crate) fn register(c: &Connection) -> rusqlite::Result<()> {
    c.create_scalar_function(
        "track_lang",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            Ok(ctx
                .get::<Option<String>>(0)?
                .as_deref()
                .and_then(track_lang))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_languages_are_spelled_one_way() {
        let t = |s: &str| track_lang(s);
        assert_eq!(t("eng").as_deref(), Some("en"));
        assert_eq!(t("en").as_deref(), Some("en"));
        assert_eq!(t("ger").as_deref(), Some("de"));
        assert_eq!(t("deu").as_deref(), Some("de"));
        assert_eq!(t(" SPA ").as_deref(), Some("es"));
        assert_eq!(t("es_419").as_deref(), Some("es-419"));
        assert_eq!(t("nob").as_deref(), Some("no"));
        assert_eq!(t("nb").as_deref(), Some("no"));
        assert_eq!(t("haw").as_deref(), Some("haw"));
        for none in ["", "und", "mul", "zxx", "qaa"] {
            assert_eq!(t(none), None, "{none}");
        }
    }
}
