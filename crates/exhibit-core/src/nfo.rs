//! Reader for `.nfo` sidecar files (Kodi/Jellyfin XML or scene-style text).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExternalId {
    /// A TMDB id whose kind is unknown or a movie (XML tags, `themoviedb.org/movie/<n>`).
    Tmdb(i64),
    /// A TMDB TV show id (`themoviedb.org/tv/<n>`).
    TmdbTv(i64),
    Imdb(String),
    Tvdb(i64),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NfoInfo {
    pub ids: Vec<ExternalId>,
    pub title: Option<String>,
    pub year: Option<i32>,
    /// Root element is `tvshow` or `episodedetails`.
    pub is_show: bool,
}

pub fn read_nfo(text: &str) -> NfoInfo {
    static RES: OnceLock<Res> = OnceLock::new();
    let re = RES.get_or_init(Res::new);

    let mut ids = Vec::new();
    let mut push = |id: ExternalId| {
        if !ids.contains(&id) {
            ids.push(id);
        }
    };

    for c in re.uniqueid.captures_iter(text) {
        let Some(t) = re.type_attr.captures(&c[1]) else {
            continue;
        };
        let value = c[2].trim();
        match t[1].to_ascii_lowercase().as_str() {
            "tmdb" => value
                .parse()
                .map(ExternalId::Tmdb)
                .into_iter()
                .for_each(&mut push),
            "tvdb" => value
                .parse()
                .map(ExternalId::Tvdb)
                .into_iter()
                .for_each(&mut push),
            "imdb" if re.imdb_id.is_match(value) => push(ExternalId::Imdb(value.to_string())),
            _ => {}
        }
    }
    for c in re.tmdbid.captures_iter(text) {
        if let Ok(n) = c[1].parse() {
            push(ExternalId::Tmdb(n));
        }
    }
    for c in re.tvdbid.captures_iter(text) {
        if let Ok(n) = c[1].parse() {
            push(ExternalId::Tvdb(n));
        }
    }
    for c in re
        .imdbid
        .captures_iter(text)
        .chain(re.id_tag.captures_iter(text))
    {
        push(ExternalId::Imdb(c[1].to_string()));
    }
    for c in re.tmdb_url.captures_iter(text) {
        if let Ok(n) = c[2].parse() {
            if c[1].eq_ignore_ascii_case("tv") {
                push(ExternalId::TmdbTv(n));
            } else {
                push(ExternalId::Tmdb(n));
            }
        }
    }
    for c in re.imdb_url.captures_iter(text) {
        push(ExternalId::Imdb(c[1].to_string()));
    }

    let title = re
        .title
        .captures(text)
        .map(|c| unescape(c[1].trim()))
        .filter(|t| !t.is_empty());
    let year = re.year.captures(text).and_then(|c| c[1].parse().ok());
    let is_show = re.root.captures(text).is_some_and(|c| {
        matches!(
            c[1].to_ascii_lowercase().as_str(),
            "tvshow" | "episodedetails"
        )
    });

    NfoInfo {
        ids,
        title,
        year,
        is_show,
    }
}

/// Reads and parses an nfo file. Scene nfo files are often not valid UTF-8, so bytes are
/// decoded lossily. Returns `None` when the file cannot be read.
pub fn read_nfo_file(path: &Path) -> Option<NfoInfo> {
    let bytes = std::fs::read(path).ok()?;
    Some(read_nfo(&String::from_utf8_lossy(&bytes)))
}

/// Looks for `<stem>.nfo`, then `movie.nfo`, then `tvshow.nfo` beside the file and up to two
/// folders above (nearest folder first).
pub fn find_nfo(media_file: &Path) -> Option<PathBuf> {
    let stem = media_file.file_stem()?.to_string_lossy().into_owned();
    let names = [
        format!("{stem}.nfo"),
        "movie.nfo".into(),
        "tvshow.nfo".into(),
    ];
    media_file
        .parent()?
        .ancestors()
        .take(3)
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|p| p.is_file())
}

struct Res {
    uniqueid: Regex,
    type_attr: Regex,
    tmdbid: Regex,
    tvdbid: Regex,
    imdbid: Regex,
    id_tag: Regex,
    imdb_id: Regex,
    tmdb_url: Regex,
    imdb_url: Regex,
    title: Regex,
    year: Regex,
    root: Regex,
}

impl Res {
    fn new() -> Self {
        let r = |p: &str| Regex::new(p).expect("valid regex");
        Self {
            uniqueid: r(r"(?is)<uniqueid\b([^>]*)>([^<]*)</uniqueid\s*>"),
            type_attr: r(r#"(?i)\btype\s*=\s*["']([^"']+)["']"#),
            tmdbid: r(r"(?i)<tmdbid\s*>\s*(\d+)\s*</tmdbid\s*>"),
            tvdbid: r(r"(?i)<tvdbid\s*>\s*(\d+)\s*</tvdbid\s*>"),
            imdbid: r(r"(?i)<imdbid\s*>\s*(tt\d+)\s*</imdbid\s*>"),
            id_tag: r(r"(?i)<id\s*>\s*(tt\d+)\s*</id\s*>"),
            imdb_id: r(r"^tt\d+$"),
            tmdb_url: r(r"(?i)themoviedb\.org/(movie|tv)/(\d+)"),
            imdb_url: r(r"(?i)imdb\.com/title/(tt\d+)"),
            title: r(r"(?is)<title\s*>([^<]*)</title\s*>"),
            year: r(r"(?i)<year\s*>\s*(\d{4})\s*</year\s*>"),
            root: r(r"<([A-Za-z][\w.-]*)"),
        }
    }
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn kodi_movie_xml() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<movie>
  <title>The Matrix</title>
  <year>1999</year>
  <uniqueid type="tmdb" default="true">603</uniqueid>
  <uniqueid type="imdb">tt0133093</uniqueid>
</movie>"#;
        let n = read_nfo(xml);
        assert_eq!(
            n.ids,
            vec![ExternalId::Tmdb(603), ExternalId::Imdb("tt0133093".into())]
        );
        assert_eq!(n.title.as_deref(), Some("The Matrix"));
        assert_eq!(n.year, Some(1999));
        assert!(!n.is_show);
    }

    #[test]
    fn tvshow_root_sets_is_show() {
        let n = read_nfo("<tvshow><title>Lost</title><tvdbid>73739</tvdbid></tvshow>");
        assert!(n.is_show);
        assert_eq!(n.ids, vec![ExternalId::Tvdb(73739)]);
        assert!(read_nfo("<episodedetails><title>Pilot</title></episodedetails>").is_show);
    }

    #[test]
    fn legacy_id_tags_and_dedup() {
        let n = read_nfo(
            "<movie><tmdbid>603</tmdbid><imdbid>tt0133093</imdbid><id>tt0133093</id><id>12</id></movie>",
        );
        assert_eq!(
            n.ids,
            vec![ExternalId::Tmdb(603), ExternalId::Imdb("tt0133093".into())]
        );
    }

    #[test]
    fn plain_text_imdb_url() {
        let n = read_nfo("Release info\nIMDb: https://www.imdb.com/title/tt0113277/\n");
        assert_eq!(n.ids, vec![ExternalId::Imdb("tt0113277".into())]);
        assert_eq!(n.title, None);
        assert!(!n.is_show);
    }

    #[test]
    fn broken_xml_with_tmdb_url() {
        let n = read_nfo(
            "<movie><title>Oops<url>https://www.themoviedb.org/movie/603-the-matrix</url>",
        );
        assert_eq!(n.ids, vec![ExternalId::Tmdb(603)]);
    }

    #[test]
    fn tmdb_tv_url() {
        let n = read_nfo("https://www.themoviedb.org/tv/1396");
        assert_eq!(n.ids, vec![ExternalId::TmdbTv(1396)]);
        let n = read_nfo("https://www.themoviedb.org/movie/603");
        assert_eq!(n.ids, vec![ExternalId::Tmdb(603)]);
    }

    #[test]
    fn no_ids_keeps_title() {
        let n = read_nfo("<movie><title>Tom &amp; Jerry</title></movie>");
        assert!(n.ids.is_empty());
        assert_eq!(n.title.as_deref(), Some("Tom & Jerry"));
    }

    #[test]
    fn find_prefers_stem_over_movie() {
        let d = tempfile::tempdir().unwrap();
        let media = d.path().join("Film.mkv");
        fs::write(&media, b"").unwrap();
        fs::write(d.path().join("movie.nfo"), "x").unwrap();
        assert_eq!(find_nfo(&media), Some(d.path().join("movie.nfo")));
        fs::write(d.path().join("Film.nfo"), "x").unwrap();
        assert_eq!(find_nfo(&media), Some(d.path().join("Film.nfo")));
    }

    #[test]
    fn find_looks_two_folders_up() {
        let d = tempfile::tempdir().unwrap();
        let deep = d.path().join("Show").join("Season 1");
        fs::create_dir_all(&deep).unwrap();
        let media = deep.join("S01E01.mkv");
        fs::write(&media, b"").unwrap();
        assert_eq!(find_nfo(&media), None);
        fs::write(d.path().join("Show").join("tvshow.nfo"), "x").unwrap();
        assert_eq!(
            find_nfo(&media),
            Some(d.path().join("Show").join("tvshow.nfo"))
        );
    }

    #[test]
    fn find_stops_after_two_folders() {
        let d = tempfile::tempdir().unwrap();
        let deep = d.path().join("a").join("b").join("c").join("d");
        fs::create_dir_all(&deep).unwrap();
        let media = deep.join("f.mkv");
        fs::write(&media, b"").unwrap();
        fs::write(d.path().join("a").join("movie.nfo"), "x").unwrap();
        assert_eq!(find_nfo(&media), None);
    }

    #[test]
    fn file_with_invalid_utf8() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("scene.nfo");
        let mut bytes = vec![0xC9, 0xCD, 0xFF, b'\n'];
        bytes.extend_from_slice(b"https://www.imdb.com/title/tt0113277/");
        bytes.extend_from_slice(&[0xFE, 0xBB]);
        fs::write(&p, bytes).unwrap();
        let n = read_nfo_file(&p).unwrap();
        assert_eq!(n.ids, vec![ExternalId::Imdb("tt0113277".into())]);
        assert!(read_nfo_file(&d.path().join("missing.nfo")).is_none());
    }
}
