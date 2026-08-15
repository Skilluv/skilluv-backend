//! Translation lookup.
//!
//! `locales/*.yml` have existed since Phase 1.13 and nothing read them. This
//! module does, and it is what every user-facing string outside the API
//! contract should go through: notification titles, email subjects and
//! bodies, error messages shown to a person rather than to a client.
//!
//! ## Adding a language
//!
//! Drop a file in `locales/`. That is the whole procedure — no code, no
//! deployment, no template. The files are embedded at build time, so a new
//! language ships with the binary rather than depending on what happens to
//! be on disk next to it.
//!
//! What must *not* happen is a template per language. Twenty emails in ten
//! languages would be two hundred HTML files, and changing a button would
//! mean changing it two hundred times. One skeleton, N key files.
//!
//! ## Missing keys
//!
//! A translation falls back to the default locale, and then to the key
//! itself. A half-translated file therefore degrades to French rather than
//! rendering blanks, and a missing key is visible in the output instead of
//! silently producing an empty subject line — which is the failure that gets
//! shipped, because an empty string looks like a rendering choice.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Locale used when nothing else matches, and the fallback for any key a
/// translation is missing.
pub const DEFAULT_LOCALE: &str = "fr";

/// Locales shipped with the binary. Adding to `locales/` requires adding the
/// file here too — the only line of code a new language costs, and it exists
/// because `include_str!` needs a literal path.
const BUNDLED: &[(&str, &str)] = &[
    ("fr", include_str!("../../locales/fr.yml")),
    ("en", include_str!("../../locales/en.yml")),
    ("ar", include_str!("../../locales/ar.yml")),
];

/// Locales written right to left. Used to set `dir="rtl"` on emails, without
/// which Arabic renders as visibly broken Latin-order text.
const RTL: &[&str] = &["ar", "he", "fa", "ur"];

type Catalog = HashMap<String, HashMap<String, String>>;

/// Parsed catalogues, keyed by locale then by dotted key.
fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let mut all = Catalog::new();
        for (locale, raw) in BUNDLED {
            match serde_norway::from_str::<serde_norway::Value>(raw) {
                Ok(value) => {
                    let mut flat = HashMap::new();
                    flatten(&value, String::new(), &mut flat);
                    all.insert((*locale).to_string(), flat);
                }
                Err(e) => {
                    // Not fatal: one malformed file must not take the server
                    // down, and the fallback chain still produces readable
                    // output. It does need to be loud.
                    tracing::error!(
                        locale = locale,
                        error = %e,
                        "locale file failed to parse — its translations will fall back"
                    );
                }
            }
        }
        all
    })
}

/// Turn nested YAML into dotted keys: `auth.unauthorized`.
fn flatten(value: &serde_norway::Value, prefix: String, out: &mut HashMap<String, String>) {
    match value {
        serde_norway::Value::Mapping(map) => {
            for (k, v) in map {
                let Some(key) = k.as_str() else { continue };
                // `_version` and other metadata are not translations.
                if key.starts_with('_') {
                    continue;
                }
                let path = if prefix.is_empty() {
                    key.to_string()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten(v, path, out);
            }
        }
        serde_norway::Value::String(s) => {
            out.insert(prefix, s.clone());
        }
        // Numbers and booleans are legitimate leaves in a translation file
        // (a count, a flag); anything else is ignored rather than rejected.
        serde_norway::Value::Number(n) => {
            out.insert(prefix, n.to_string());
        }
        serde_norway::Value::Bool(b) => {
            out.insert(prefix, b.to_string());
        }
        _ => {}
    }
}

/// Is this locale one we can serve?
pub fn is_supported(locale: &str) -> bool {
    catalog().contains_key(base_language(locale))
}

/// Every locale that has a catalogue, sorted.
pub fn available() -> Vec<&'static str> {
    let mut codes: Vec<&'static str> = BUNDLED.iter().map(|(code, _)| *code).collect();
    codes.sort_unstable();
    codes
}

/// `fr-CA` and `fr` are the same catalogue. Region subtags carry real
/// differences in wording, but shipping `fr-CA` as a separate file before
/// anyone has written one would only produce misses.
fn base_language(locale: &str) -> &str {
    locale
        .split(['-', '_'])
        .next()
        .unwrap_or(DEFAULT_LOCALE)
        .trim()
}

/// Text direction for a locale: `"rtl"` or `"ltr"`.
pub fn direction(locale: &str) -> &'static str {
    if RTL.contains(&base_language(locale)) {
        "rtl"
    } else {
        "ltr"
    }
}

/// Look up a key, falling back to the default locale and then to the key.
///
/// Returning the key rather than an empty string is deliberate: a subject
/// line reading `email.payout_sent.subject` is obviously a bug and gets
/// fixed, where a blank one looks intentional and ships.
pub fn t(locale: &str, key: &str) -> String {
    let catalog = catalog();
    let lang = base_language(locale);

    if let Some(found) = catalog.get(lang).and_then(|c| c.get(key)) {
        return found.clone();
    }
    if lang != DEFAULT_LOCALE
        && let Some(found) = catalog.get(DEFAULT_LOCALE).and_then(|c| c.get(key))
    {
        tracing::debug!(
            locale = lang,
            key = key,
            "translation missing, using default"
        );
        return found.clone();
    }

    tracing::warn!(
        locale = lang,
        key = key,
        "translation missing in every locale — the key is being rendered as-is"
    );
    key.to_string()
}

/// Look up a key and substitute `{placeholders}`.
///
/// Interpolation is by name, not by position: a translator reordering a
/// sentence — which every translator does, because word order differs
/// between languages — must not silently swap two values.
pub fn t_with(locale: &str, key: &str, args: &[(&str, &str)]) -> String {
    let mut text = t(locale, key);
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

/// Best locale for a user: their stored preference, else the language of the
/// request, else the default.
///
/// An email sent from a background job has no request to read, which is why
/// the preference has to be stored on the account rather than resolved per
/// call.
pub fn resolve(stored: Option<&str>, accept_language: Option<&str>) -> String {
    if let Some(pref) = stored
        && is_supported(pref)
    {
        return base_language(pref).to_string();
    }
    if let Some(header) = accept_language {
        for part in header.split(',') {
            // `fr-CH;q=0.9` — the quality factor is dropped; the header is
            // already in preference order.
            let tag = part.split(';').next().unwrap_or("").trim();
            if !tag.is_empty() && is_supported(tag) {
                return base_language(tag).to_string();
            }
        }
    }
    DEFAULT_LOCALE.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_locale_parses() {
        for (code, _) in BUNDLED {
            let entries = catalog()
                .get(*code)
                .unwrap_or_else(|| panic!("{code} failed to parse"));
            assert!(!entries.is_empty(), "{code} has no keys");
        }
    }

    #[test]
    fn nested_keys_become_dotted_paths() {
        let text = t("fr", "auth.unauthorized");
        assert_ne!(text, "auth.unauthorized", "the key should have resolved");
        assert!(!text.is_empty());
    }

    #[test]
    fn a_missing_key_returns_the_key_not_a_blank() {
        // Loud on purpose: a blank subject line looks like a choice.
        assert_eq!(t("fr", "nope.not.here"), "nope.not.here");
    }

    #[test]
    fn an_unknown_locale_falls_back_to_the_default() {
        let unknown = t("sw", "auth.unauthorized");
        let default = t(DEFAULT_LOCALE, "auth.unauthorized");
        assert_eq!(unknown, default);
    }

    #[test]
    fn region_subtags_share_the_base_catalogue() {
        assert_eq!(
            t("fr-CA", "auth.unauthorized"),
            t("fr", "auth.unauthorized")
        );
        assert!(is_supported("fr-BE"));
    }

    #[test]
    fn placeholders_are_substituted_by_name() {
        // Word order differs between languages, so position cannot be
        // trusted; only the name can.
        let out = t_with("fr", "nope.hello.{name}.and.{other}", &[]);
        assert!(out.contains("{name}"), "unsubstituted names stay visible");

        let text = "Bonjour {name}, tu as {count} messages";
        let rendered = text.replace("{name}", "Ada").replace("{count}", "3");
        assert_eq!(rendered, "Bonjour Ada, tu as 3 messages");
    }

    #[test]
    fn arabic_is_right_to_left() {
        assert_eq!(direction("ar"), "rtl");
        assert_eq!(direction("fr"), "ltr");
        assert_eq!(direction("ar-MA"), "rtl");
    }

    #[test]
    fn a_stored_preference_wins_over_the_request() {
        assert_eq!(resolve(Some("en"), Some("fr")), "en");
        assert_eq!(resolve(None, Some("en-GB,en;q=0.9")), "en");
        // An unsupported preference must not strand the user on a language
        // we cannot render.
        assert_eq!(resolve(Some("sw"), Some("en")), "en");
        assert_eq!(resolve(None, None), DEFAULT_LOCALE);
    }
}
