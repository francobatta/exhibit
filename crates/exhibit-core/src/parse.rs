//! File-name parser: turns rip-style relative paths into title, year, season and episode.

use regex::Regex;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KindHint {
    Movie,
    Episode,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedName {
    pub title: String,
    pub year: Option<i32>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub episode_end: Option<i32>,
    pub hint: KindHint,
}

/// `rel_path` is the path relative to the source root, with either slash.
pub fn parse_path(rel_path: &str) -> ParsedName {
    let mut parts: Vec<&str> = rel_path
        .split(['/', '\\'])
        .filter(|p| !p.is_empty())
        .collect();
    let file = parts.pop().unwrap_or("");
    let folders: Vec<Name> = parts.iter().rev().map(|f| parse_folder(f)).collect();
    let in_season_folder = folders.iter().any(|f| f.is_season_folder);

    let mut name = parse_name(strip_extension(file), in_season_folder);
    let file_title_usable = is_usable_title(&name.title);

    // Nearest folder first: season folders fill the season; the first folder with a usable
    // title is the show or movie folder, which supplies the title if the file had none and
    // the year when it names the same title.
    for folder in &folders {
        if name.season.is_none() {
            name.season = folder.season;
        }
        if folder.is_season_folder || !is_usable_title(&folder.title) {
            continue;
        }
        if !file_title_usable {
            name.title = folder.title.clone();
            name.year = name.year.or(folder.year);
        } else if normalize_title(&name.title) == normalize_title(&folder.title) {
            name.year = name.year.or(folder.year);
        }
        break;
    }

    let hint = if name.season.is_some() || name.episode.is_some() {
        KindHint::Episode
    } else if name.year.is_some() {
        KindHint::Movie
    } else {
        KindHint::Unknown
    };
    ParsedName {
        title: name.title,
        year: name.year,
        season: name.season,
        episode: name.episode,
        episode_end: name.episode_end,
        hint,
    }
}

/// Lowercase, strip punctuation and diacritics, collapse spaces. Used to group and compare titles.
pub fn normalize_title(s: &str) -> String {
    let cleaned: String = s
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .filter(|c| !matches!(c, '\'' | '\u{2019}' | '`'))
        .flat_map(char::to_lowercase)
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What one path component yields on its own.
#[derive(Default)]
struct Name {
    title: String,
    year: Option<i32>,
    season: Option<i32>,
    episode: Option<i32>,
    episode_end: Option<i32>,
    /// `Season 2`, `S02`, `Specials`: a folder that only names a season.
    is_season_folder: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TokKind {
    Word,
    /// `[..]`, `(..)` or `{..}`; `text` is the inner content.
    Bracket,
    /// Release group split off the final `-`.
    Group,
}

struct Tok {
    kind: TokKind,
    text: String,
    /// Text as written, used when the token ends up in a title.
    raw: String,
}

static RE_YEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?:19|20)\d\d$").unwrap());
static RE_SXE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^s(\d{1,3})e(\d{1,4})((?:-?e\d{1,4}|-\d{1,4})*)$").unwrap());
static RE_NXN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(\d{1,2})x(\d{2,3})(?:-(?:\d{1,2}x)?(\d{2,3}))?$").unwrap());
static RE_SEASON: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^s(\d{1,3})$").unwrap());
static RE_EPISODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^ep?(\d{1,4})$").unwrap());
static RE_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{1,4}$").unwrap());
static RE_DIGITS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());
/// A bare absolute episode number, optionally with a fansub version suffix (`12v2`).
static RE_ABSOLUTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(\d{1,4})(?:v\d{1,2})?$").unwrap());
static RE_SPECIALS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^specials?$").unwrap());
static RE_EXTENSION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9]{1,5}$").unwrap());
/// Release tags recognised in any letter case.
static RE_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)^(
            \d{3,4}[pi] | \d{3,4}x\d{3,4} | [48]k | uhd
          | blu-?ray | bdrip | brrip | bdremux | remux | web-?dl | web-?rip | hdtv | hdrip
          | dvd\d? | dvdrip | dvdscr | amzn | atvp | dsnp | hmax
          | [xh]26[45] | hevc | avc | xvid | divx | av1 | (?:8|10)bit | hi10p?
          | ddp?\d* | dd\+\d* | aac\d* | e?ac3\d* | dts | dts-hd | dts-x | dtshd | truehd
          | atmos | flac | mp3 | lpcm
          | hdr | hdr10\+? | hdr10plus | dovi | hlg
          | repack | proper | rerip | extended | remastered | criterion
        )$",
    )
    .unwrap()
});
/// Release tags that are also ordinary words, so they only count in capitals (`WEB`, not `Web`).
static RE_TAG_UPPER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)^(
            WEB | DV | NF | HULU | OPUS | UNCUT | UNRATED | IMAX | LIMITED | INTERNAL | COMPLETE
          | MULTI | MULTISUB | SUBBED | DUBBED
          | FRENCH | TRUEFRENCH | VOSTFR | VFF | GERMAN | ITALIAN | ITA | SPANISH | JAPANESE
          | KOREAN | CHINESE | ENGLISH | ENG | RUSSIAN
        )$",
    )
    .unwrap()
});

fn parse_folder(folder: &str) -> Name {
    if RE_SPECIALS.is_match(folder.trim()) {
        return Name {
            season: Some(0),
            is_season_folder: true,
            ..Name::default()
        };
    }
    let mut name = parse_name(folder, false);
    name.is_season_folder = name.season.is_some() && !is_usable_title(&name.title);
    name
}

fn strip_extension(file: &str) -> &str {
    match file.rsplit_once('.') {
        Some((stem, ext))
            if !stem.is_empty()
                && RE_EXTENSION.is_match(ext)
                && ext.chars().any(|c| c.is_ascii_alphabetic()) =>
        {
            stem
        }
        _ => file,
    }
}

/// A title is usable when it has at least two letters or digits.
fn is_usable_title(title: &str) -> bool {
    title.chars().filter(|c| c.is_alphanumeric()).count() >= 2
}

/// Parses one path component. `in_season_folder` allows `Title - 05` absolute numbering.
fn parse_name(name: &str, in_season_folder: bool) -> Name {
    let toks = tokenize(name);
    let n = toks.len();
    let mut out = Name::default();

    // Leading bracketed groups (`[SubsGroup]`) are skipped and mark a fansub-style name.
    let start = toks
        .iter()
        .position(|t| t.kind != TokKind::Bracket || is_year(t))
        .unwrap_or(n);
    let group_tagged = start > 0;
    let absolute_allowed = group_tagged || in_season_folder;

    // `boundary` is where the title ends: the first release tag, bracket or episode marker.
    let mut boundary = n;
    if let Some(ep) = leading_episode(&toks, start, absolute_allowed) {
        out.episode = Some(ep);
        boundary = start;
    } else {
        for i in start..n {
            let tok = &toks[i];
            if (tok.kind == TokKind::Bracket && !is_year(tok)) || is_tag(&toks, i) {
                boundary = i;
                break;
            }
            if let Some((season, episode, episode_end)) = episode_marker(&toks, i) {
                out.season = season;
                out.episode = episode;
                out.episode_end = episode_end;
                boundary = i;
                break;
            }
            // `Title - 05` in a fansub name or season folder; `Title 05 [1080p]` in a fansub name.
            let absolute = if absolute_allowed && is_dash(tok) {
                absolute_at(&toks, i + 1, true)
            } else if group_tagged && i > start && !is_year(tok) {
                absolute_at(&toks, i, false)
            } else {
                None
            };
            if absolute.is_some() {
                out.episode = absolute;
                boundary = i;
                break;
            }
        }
    }

    // The year is the last year-like token (never the first) that is followed by a tag,
    // bracket, episode marker or the end.
    let mut title_end = boundary;
    for i in start + 1..=boundary.min(n.saturating_sub(1)) {
        if is_year(&toks[i]) && is_terminated(&toks, i, boundary) {
            out.year = toks[i].text.parse().ok();
            title_end = i;
        }
    }

    let mut title = &toks[start.min(title_end)..title_end];
    while title.first().is_some_and(is_dash) {
        title = &title[1..];
    }
    while title.last().is_some_and(is_dash) {
        title = &title[..title.len() - 1];
    }
    out.title = title
        .iter()
        .map(|t| t.raw.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    out
}

/// `01 - Pilot` or a name that is only a number: the number is the episode.
/// A lone three-digit number needs folder or group context (`300` is also a film).
fn leading_episode(toks: &[Tok], start: usize, context: bool) -> Option<i32> {
    let first = toks.get(start).filter(|t| t.kind == TokKind::Word)?;
    if first.text.len() > 3 || !first.text.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let qualifies = match toks.get(start + 1) {
        None => first.text.len() <= 2 || context,
        Some(next) => is_dash(next),
    };
    if qualifies {
        first.text.parse().ok()
    } else {
        None
    }
}

/// An absolute episode number at `i`, followed by a tag, bracket or the end (or a dash,
/// when `dash_after` allows `- 05 - Episode Title`).
fn absolute_at(toks: &[Tok], i: usize, dash_after: bool) -> Option<i32> {
    let tok = toks.get(i).filter(|t| t.kind == TokKind::Word)?;
    let caps = RE_ABSOLUTE.captures(&tok.text)?;
    let ends = match toks.get(i + 1) {
        None => true,
        Some(next) => {
            next.kind != TokKind::Word || (dash_after && is_dash(next)) || is_tag(toks, i + 1)
        }
    };
    if ends { caps[1].parse().ok() } else { None }
}

type Marker = (Option<i32>, Option<i32>, Option<i32>);

/// Season/episode markers starting at `i`, as (season, episode, episode_end): `S02E04`,
/// `S03E01E02`, `S01E01-02`, `1x03`, `S02 E04`, `S02`, `Season 2 Episode 12`, `Episode 3`, `E04`.
fn episode_marker(toks: &[Tok], i: usize) -> Option<Marker> {
    let word = |j: usize| {
        toks.get(j)
            .filter(|t| t.kind == TokKind::Word)
            .map(|t| t.text.as_str())
    };
    let num = |s: &str| s.parse::<i32>().ok();
    let text = word(i)?;
    let lower = text.to_lowercase();

    if let Some(c) = RE_SXE.captures(text) {
        let end = RE_DIGITS
            .find_iter(&c[3])
            .last()
            .and_then(|m| num(m.as_str()));
        return Some((num(&c[1]), num(&c[2]), end));
    }
    if let Some(c) = RE_NXN.captures(text) {
        return Some((
            num(&c[1]),
            num(&c[2]),
            c.get(3).and_then(|m| num(m.as_str())),
        ));
    }
    if let Some(c) = RE_SEASON.captures(text) {
        let episode = word(i + 1)
            .and_then(|w| RE_EPISODE.captures(w))
            .and_then(|e| num(&e[1]));
        return Some((num(&c[1]), episode, None));
    }
    if lower == "season" || lower == "series" {
        let season = word(i + 1)
            .filter(|w| RE_NUMBER.is_match(w))
            .and_then(num)?;
        let episode = match word(i + 2).map(str::to_lowercase).as_deref() {
            Some("episode" | "ep") => word(i + 3).filter(|w| RE_NUMBER.is_match(w)).and_then(num),
            Some(w) => RE_EPISODE.captures(w).and_then(|e| num(&e[1])),
            None => None,
        };
        return Some((Some(season), episode, None));
    }
    if lower == "episode" || lower == "ep" {
        let episode = word(i + 1)
            .filter(|w| RE_NUMBER.is_match(w))
            .and_then(num)?;
        return Some((None, Some(episode), None));
    }
    let c = RE_EPISODE.captures(text)?;
    Some((None, num(&c[1]), None))
}

/// Whether the token after `i` (skipping dashes) ends the title.
fn is_terminated(toks: &[Tok], i: usize, boundary: usize) -> bool {
    let j = (i + 1..toks.len())
        .find(|&j| !is_dash(&toks[j]))
        .unwrap_or(toks.len());
    j >= boundary || toks[j].kind != TokKind::Word || is_tag(toks, j)
}

fn is_dash(tok: &Tok) -> bool {
    tok.kind == TokKind::Word && tok.text == "-"
}

fn is_year(tok: &Tok) -> bool {
    tok.kind != TokKind::Group && RE_YEAR.is_match(&tok.text)
}

fn is_tag(toks: &[Tok], i: usize) -> bool {
    let tok = &toks[i];
    match tok.kind {
        TokKind::Group => return true,
        TokKind::Bracket => return false,
        TokKind::Word => {}
    }
    if is_tag_word(&tok.text) {
        return true;
    }
    let lower = tok.text.to_lowercase();
    let next = toks
        .get(i + 1)
        .filter(|t| t.kind == TokKind::Word)
        .map(|t| t.text.to_lowercase());
    match (lower.as_str(), next.as_deref()) {
        // `H.264`, split by the dot tokenizer.
        ("h" | "x", Some("264" | "265")) => true,
        (
            "directors" | "director's" | "special" | "collectors" | "collector's" | "anniversary"
            | "ultimate" | "theatrical" | "extended",
            Some("cut" | "edition"),
        ) => true,
        _ => false,
    }
}

fn is_tag_word(text: &str) -> bool {
    RE_TAG.is_match(text) || (!text.chars().any(char::is_lowercase) && RE_TAG_UPPER.is_match(text))
}

/// Splits on spaces, dots and underscores; brackets become single tokens; a release group
/// after the final `-` (`x264-GROUP`) becomes its own token.
fn tokenize(name: &str) -> Vec<Tok> {
    fn word(text: String) -> Tok {
        Tok {
            kind: TokKind::Word,
            raw: text.clone(),
            text,
        }
    }
    let chars: Vec<char> = name.chars().collect();
    let mut toks = Vec::new();
    let mut current = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let close = match c {
            '[' => Some(']'),
            '(' => Some(')'),
            '{' => Some('}'),
            _ => None,
        };
        if let Some(len) = close.and_then(|close| chars[i + 1..].iter().position(|&x| x == close)) {
            if !current.is_empty() {
                toks.push(word(std::mem::take(&mut current)));
            }
            let inner: String = chars[i + 1..i + 1 + len].iter().collect();
            let raw: String = chars[i..i + 2 + len].iter().collect();
            toks.push(Tok {
                kind: TokKind::Bracket,
                text: inner.trim().to_string(),
                raw,
            });
            i += len + 2;
            continue;
        }
        if c.is_whitespace() || c == '.' || c == '_' {
            if !current.is_empty() {
                toks.push(word(std::mem::take(&mut current)));
            }
        } else {
            current.push(c);
        }
        i += 1;
    }
    if !current.is_empty() {
        toks.push(word(current));
    }

    let split = toks
        .last()
        .filter(|t| t.kind == TokKind::Word)
        .and_then(|last| {
            let (left, group) = last.text.rsplit_once('-')?;
            let left_is_release = is_tag_word(left) || left.chars().all(|c| c.is_ascii_digit());
            (!left.is_empty() && !group.is_empty() && left_is_release)
                .then(|| (left.to_string(), group.to_string()))
        });
    if let Some((left, group)) = split {
        toks.pop();
        toks.push(word(left));
        toks.push(Tok {
            kind: TokKind::Group,
            raw: group.clone(),
            text: group,
        });
    }
    toks
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORPUS: &str = include_str!("../tests/fixtures/names.tsv");

    fn cell_num(cell: &str) -> Option<i32> {
        if cell.is_empty() {
            None
        } else {
            Some(
                cell.parse()
                    .unwrap_or_else(|_| panic!("bad number cell {cell:?}")),
            )
        }
    }

    fn cell_hint(cell: &str) -> KindHint {
        match cell {
            "movie" => KindHint::Movie,
            "episode" => KindHint::Episode,
            "unknown" => KindHint::Unknown,
            other => panic!("bad hint cell {other:?}"),
        }
    }

    #[test]
    fn corpus() {
        let mut rows = 0;
        let mut failures = Vec::new();
        let lines = CORPUS
            .lines()
            .map(|l| l.trim_end_matches('\r'))
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .skip(1); // header
        for line in lines {
            let cells: Vec<&str> = line.split('\t').collect();
            assert_eq!(cells.len(), 7, "corpus row needs 7 columns: {line:?}");
            let expected = ParsedName {
                title: cells[1].to_string(),
                year: cell_num(cells[2]),
                season: cell_num(cells[3]),
                episode: cell_num(cells[4]),
                episode_end: cell_num(cells[5]),
                hint: cell_hint(cells[6]),
            };
            let actual = parse_path(cells[0]);
            rows += 1;
            if actual != expected {
                failures.push(format!(
                    "{}\n    expected: {expected:?}\n    actual:   {actual:?}",
                    cells[0]
                ));
            }
        }
        assert!(rows >= 60, "corpus has only {rows} rows");
        assert!(
            failures.is_empty(),
            "{} of {rows} corpus rows failed:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn normalize_strips_case_punctuation_diacritics() {
        assert_eq!(normalize_title("Amélie: The  Movie!"), "amelie the movie");
    }

    #[test]
    fn very_long_path_does_not_panic() {
        let path = format!(
            "{}/{}.S01E01.mkv",
            "Folder.Name".repeat(20),
            "x.".repeat(90)
        );
        assert!(path.len() >= 400);
        let parsed = parse_path(&path);
        assert_eq!(parsed.episode, Some(1));
    }
}
