//! The owner's language and date format. The language decides which TMDB text the library
//! shows; English (`BASE`) is always kept, and fills any gap in a translation.

use crate::db;
use crate::error::{Detail, Error, Result};
use rusqlite::Connection;
use std::sync::OnceLock;

/// The language every matched item's own columns hold.
pub const BASE: &str = "en-US";

/// The languages Exhibit speaks, as TMDB and the window spell them.
pub const LANGUAGES: [&str; 3] = [BASE, "es-ES", "es-MX"];

/// How dates are written: following the language, day first, month first, or numeric.
pub const DATE_FORMATS: [&str; 4] = ["auto", "day", "month", "numeric"];

const LANGUAGE_KEY: &str = "language";
const DATE_FORMAT_KEY: &str = "date_format";

/// The language in effect: the owner's choice, or the one Windows suggests.
pub fn language(c: &Connection) -> Result<&'static str> {
    Ok(match db::get_setting(c, LANGUAGE_KEY)? {
        Some(tag) => supported(&tag).unwrap_or_else(system_language),
        None => system_language(),
    })
}

/// Stores the owner's choice of language.
pub fn set_language(c: &Connection, tag: &str) -> Result<()> {
    let tag = supported(tag).ok_or_else(|| {
        Error::Invalid(
            Detail::coded(
                "settings.unknown_language",
                format!(
                    "Exhibit does not speak {tag}; use one of {}",
                    LANGUAGES.join(", ")
                ),
            )
            .with("language", tag),
        )
    })?;
    db::set_setting(c, LANGUAGE_KEY, Some(tag))
}

pub fn date_format(c: &Connection) -> Result<&'static str> {
    Ok(db::get_setting(c, DATE_FORMAT_KEY)?
        .and_then(|f| DATE_FORMATS.into_iter().find(|d| *d == f))
        .unwrap_or(DATE_FORMATS[0]))
}

pub fn set_date_format(c: &Connection, format: &str) -> Result<()> {
    let format = DATE_FORMATS
        .into_iter()
        .find(|d| *d == format)
        .ok_or_else(|| {
            Error::Invalid(
                Detail::coded(
                    "settings.unknown_date_format",
                    format!(
                        "no date format is called {format}; use one of {}",
                        DATE_FORMATS.join(", ")
                    ),
                )
                .with("format", format),
            )
        })?;
    // Automatic is the default, so it is stored as nothing.
    db::set_setting(
        c,
        DATE_FORMAT_KEY,
        (format != DATE_FORMATS[0]).then_some(format),
    )
}

/// The supported spelling of `tag`, matched without regard to case.
fn supported(tag: &str) -> Option<&'static str> {
    LANGUAGES.into_iter().find(|l| l.eq_ignore_ascii_case(tag))
}

/// TMDB tags images by language alone: `es` for both Spanish choices.
pub fn image_language(tag: &str) -> &str {
    tag.split('-').next().unwrap_or(tag)
}

/// The language a library that never chose one speaks: Spain's Spanish for Spain, Latin
/// American Spanish for any other Spanish, English otherwise.
pub fn suggest(system_tag: &str) -> &'static str {
    let tag = system_tag.replace('_', "-");
    let mut parts = tag.split(['-', '.']);
    let lang = parts.next().unwrap_or_default();
    if !lang.eq_ignore_ascii_case("es") {
        return BASE;
    }
    // `es-ES`, or a script between: `es-Latn-ES`.
    if tag
        .split(['-', '.'])
        .skip(1)
        .any(|p| p.eq_ignore_ascii_case("ES"))
    {
        "es-ES"
    } else {
        "es-MX"
    }
}

/// The language Windows shows its own menus in, read once.
fn system_language() -> &'static str {
    static SYSTEM: OnceLock<&'static str> = OnceLock::new();
    SYSTEM.get_or_init(|| system_tag().map_or(BASE, |t| suggest(&t)))
}

#[cfg(windows)]
fn system_tag() -> Option<String> {
    use windows_sys::Win32::Globalization::{GetUserDefaultUILanguage, LCIDToLocaleName};
    // LOCALE_NAME_MAX_LENGTH.
    let mut buf = [0u16; 85];
    // SAFETY: the buffer is as long as the length passed; the call writes at most that much.
    let n = unsafe {
        LCIDToLocaleName(
            u32::from(GetUserDefaultUILanguage()),
            buf.as_mut_ptr(),
            buf.len() as i32,
            0,
        )
    };
    // The count includes the closing nul.
    (n > 1).then(|| String::from_utf16_lossy(&buf[..n as usize - 1]))
}

#[cfg(not(windows))]
fn system_tag() -> Option<String> {
    std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_languages_map_to_the_three_choices() {
        for (system, ours) in [
            ("es-ES", "es-ES"),
            ("es-es", "es-ES"),
            ("es_ES.UTF-8", "es-ES"),
            ("es-MX", "es-MX"),
            ("es-AR", "es-MX"),
            ("es-419", "es-MX"),
            ("es", "es-MX"),
            ("en-GB", "en-US"),
            ("fr-FR", "en-US"),
            ("", "en-US"),
        ] {
            assert_eq!(suggest(system), ours, "{system}");
        }
    }

    #[test]
    fn the_choice_is_stored_and_checked() {
        let d = tempfile::tempdir().unwrap();
        let c = db::open(d.path()).unwrap();
        assert!(LANGUAGES.contains(&language(&c).unwrap()));
        set_language(&c, "es-mx").unwrap();
        assert_eq!(language(&c).unwrap(), "es-MX");
        let e = set_language(&c, "fr-FR").unwrap_err();
        assert_eq!(
            e.detail().and_then(|d| d.code),
            Some("settings.unknown_language")
        );
        assert_eq!(language(&c).unwrap(), "es-MX");
    }

    #[test]
    fn the_date_format_defaults_to_automatic() {
        let d = tempfile::tempdir().unwrap();
        let c = db::open(d.path()).unwrap();
        assert_eq!(date_format(&c).unwrap(), "auto");
        set_date_format(&c, "numeric").unwrap();
        assert_eq!(date_format(&c).unwrap(), "numeric");
        set_date_format(&c, "auto").unwrap();
        assert_eq!(db::get_setting(&c, DATE_FORMAT_KEY).unwrap(), None);
        assert!(set_date_format(&c, "julian").is_err());
    }

    #[test]
    fn images_go_by_language_alone() {
        assert_eq!(image_language("es-MX"), "es");
        assert_eq!(image_language("en-US"), "en");
    }
}
