//! The public RSS feed: one entry per film watched, rated or reviewed.

use crate::{dated, film_ref, parse_date};
use exhibit_core::service::{Dated, Fact};
use roxmltree::{Document, Node};

const SPOILER_LINE: &str = "This review may contain spoilers.";

pub struct Feed {
    pub facts: Vec<Dated>,
    /// Entries about a film; list entries and the like do not count.
    pub films: u32,
}

fn child_text<'a>(item: Node<'a, 'a>, name: &str) -> Option<&'a str> {
    item.children()
        .find(|n| n.is_element() && n.tag_name().name() == name)
        .and_then(|n| n.text())
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

pub fn parse(xml: &str) -> Result<Feed, String> {
    let doc = Document::parse(xml).map_err(|e| format!("the feed is not readable: {e}"))?;
    let mut feed = Feed {
        facts: Vec::new(),
        films: 0,
    };
    for item in doc.descendants().filter(|n| n.has_tag_name("item")) {
        let guid = child_text(item, "guid").unwrap_or_default();
        let is_review = guid.starts_with("letterboxd-review-");
        if !is_review && !guid.starts_with("letterboxd-watch-") {
            continue;
        }
        let Some(title) = child_text(item, "filmTitle") else {
            continue;
        };
        let year = child_text(item, "filmYear").and_then(|y| y.parse().ok());
        let tmdb_id = child_text(item, "movieId").and_then(|y| y.parse().ok());
        let film = film_ref(title, year, tmdb_id);
        let on = child_text(item, "watchedDate").and_then(parse_date);
        feed.films += 1;
        feed.facts.push(dated(
            &film,
            Fact::Watch {
                rewatch: child_text(item, "rewatch") == Some("Yes"),
            },
            on,
        ));
        if let Some(value) = child_text(item, "memberRating").and_then(|r| r.parse::<f32>().ok()) {
            feed.facts
                .push(dated(&film, Fact::Rating { value, scale: 5.0 }, on));
        }
        if child_text(item, "memberLike") == Some("Yes") {
            feed.facts.push(dated(&film, Fact::Like, on));
        }
        if is_review
            && let Some((text, spoiler)) = child_text(item, "description").and_then(review_text)
        {
            feed.facts
                .push(dated(&film, Fact::Review { text, spoiler }, on));
        }
    }
    Ok(feed)
}

/// The review in an entry's description: its paragraphs without the poster, the spoiler line
/// and the "Watched on" line, blank-line separated, with tags removed and entities decoded.
pub fn review_text(html: &str) -> Option<(String, bool)> {
    let mut spoiler = false;
    let mut paragraphs = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<p>") {
        let after = &rest[start + 3..];
        let end = after.find("</p>").unwrap_or(after.len());
        let text = decode_entities(strip_tags(&after[..end]).trim());
        rest = &after[end..];
        if text.is_empty() || text.starts_with("Watched on ") {
            continue;
        }
        if text == SPOILER_LINE {
            spoiler = true;
        } else {
            paragraphs.push(text);
        }
    }
    (!paragraphs.is_empty()).then(|| (paragraphs.join("\n\n"), spoiler))
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            return out;
        };
        let tag = &rest[open + 1..open + close];
        if tag.starts_with("br") {
            out.push('\n');
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

/// `&amp; &lt; &gt; &quot; &#39; &#NNN; &#xHH; &nbsp;`; anything else stays as written.
pub fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest.find(';').filter(|&semi| semi <= 10).and_then(|semi| {
            let name = &rest[1..semi];
            let ch = match name {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                "nbsp" => '\u{a0}',
                _ => {
                    let num = name.strip_prefix('#')?;
                    let code = match num.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => num.parse().ok()?,
                    };
                    char::from_u32(code)?
                }
            };
            Some((ch, semi + 1))
        });
        match decoded {
            Some((ch, used)) => {
                out.push(ch);
                rest = &rest[used..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}
