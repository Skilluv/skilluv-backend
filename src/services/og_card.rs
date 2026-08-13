//! SKI-292 — OpenGraph share cards, rendered server-side.
//!
//! `/verify/{hash}` exists to be pasted into LinkedIn or X by a candidate and
//! opened by a recruiter. The page used to advertise `og:image` pointing at
//! the landing page's SVG, and none of the three major platforms render SVG
//! in a card preview — so the link appeared with no image at all.
//!
//! A static PNG would fix the rendering and say nothing. A card that names
//! the contributor is what makes the link worth clicking, so it is generated
//! per attestation.
//!
//! ## Why the fonts are embedded
//!
//! Text is rasterised here, not by the viewer's browser. Falling back to
//! whatever the host image happens to ship would produce different output per
//! environment, and a container with no fonts renders an empty card that
//! still returns 200 and still gets cached by the platform that fetched it.
//! See `assets/fonts/README.md`.

use std::sync::OnceLock;

use resvg::tiny_skia;
use resvg::usvg;

use crate::errors::AppError;

/// Card dimensions. 1200×630 is what X, LinkedIn and Facebook expect; other
/// ratios get cropped unpredictably.
pub const CARD_WIDTH: u32 = 1200;
pub const CARD_HEIGHT: u32 = 630;

// Design system tokens, Forge theme. Mirrors `src/app.css` in the frontend.
const SURFACE: &str = "#18130f";
const SURFACE_ELEVATED: &str = "#2f2620";
const BORDER: &str = "#5a4738";
const TEXT: &str = "#f4ede0";
const TEXT_MUTED: &str = "#c2b195";
const ACCENT: &str = "#ea8a3d";
const SUCCESS: &str = "#32b8ab";
const LUV: &str = "#e63946";

const SANS: &str = "Bricolage Grotesque";
const MONO: &str = "JetBrains Mono";

const SANS_TTF: &[u8] = include_bytes!("../../assets/fonts/BricolageGrotesque-Variable.ttf");
const MONO_TTF: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Variable.ttf");

/// Everything the card shows. Built by the route from one query.
#[derive(Debug, Default)]
pub struct CardData {
    pub display_name: String,
    pub username: String,
    /// `owner/name` when the slice is attached to a GitHub project.
    pub repository: Option<String>,
    pub domain: Option<String>,
    pub difficulty: Option<i16>,
    /// Already formatted for display — this module does no localisation.
    pub validated_on: Option<String>,
    /// Shown truncated; the full hash is in the URL.
    pub hash: Option<String>,
}

/// Font database, built once. Parsing two variable fonts on every crawler
/// request would be wasted work on a response that is otherwise pure string
/// formatting.
fn options() -> &'static usvg::Options<'static> {
    static OPTIONS: OnceLock<usvg::Options<'static>> = OnceLock::new();
    OPTIONS.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_font_data(SANS_TTF.to_vec());
        db.load_font_data(MONO_TTF.to_vec());
        db.set_sans_serif_family(SANS);
        db.set_monospace_family(MONO);

        let mut opt = usvg::Options {
            font_family: SANS.to_string(),
            ..Default::default()
        };
        opt.fontdb = std::sync::Arc::new(db);
        opt
    })
}

/// Escape the five characters that would otherwise break out of XML text or
/// an attribute value.
///
/// Every dynamic value on the card is user-controlled — a display name is
/// whatever its owner typed. Interpolating it raw would let `</text>` inside
/// a username rewrite the document.
fn escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Cut a string to `max` characters, on a character boundary, with an
/// ellipsis. Long display names must not run off the card.
fn truncate(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        return input.to_string();
    }
    let kept: String = input.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// Compose the card as SVG. Split out from rendering so the markup can be
/// asserted on in tests without rasterising.
pub fn build_svg(data: &CardData) -> String {
    let name = escape(&truncate(&data.display_name, 28));
    let handle = escape(&truncate(&data.username, 32));

    // Facts line: repository, domain and difficulty, whichever are known.
    let mut facts: Vec<String> = Vec::new();
    if let Some(repo) = &data.repository {
        facts.push(escape(&truncate(repo, 40)));
    }
    if let Some(domain) = &data.domain {
        facts.push(escape(domain));
    }
    if let Some(d) = data.difficulty {
        facts.push(format!("difficulté {d}/5"));
    }
    let facts_line = facts.join("  ·  ");

    let date_line = data
        .validated_on
        .as_deref()
        .map(|d| format!("Validée le {}", escape(d)))
        .unwrap_or_default();

    // 12 characters is enough to recognise the hash next to the URL without
    // pretending the card is itself a proof.
    let hash_line = data
        .hash
        .as_deref()
        .map(|h| escape(&h.chars().take(12).collect::<String>()))
        .unwrap_or_default();

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{CARD_WIDTH}" height="{CARD_HEIGHT}" viewBox="0 0 {CARD_WIDTH} {CARD_HEIGHT}">
  <rect width="{CARD_WIDTH}" height="{CARD_HEIGHT}" fill="{SURFACE}"/>
  <rect x="56" y="56" width="1088" height="518" rx="24" fill="{SURFACE_ELEVATED}" stroke="{BORDER}" stroke-width="2"/>
  <rect x="56" y="56" width="1088" height="8" rx="4" fill="{ACCENT}"/>

  <g font-family="{SANS}">
    <circle cx="112" cy="132" r="7" fill="{SUCCESS}"/>
    <text x="132" y="139" font-size="24" font-weight="600" fill="{SUCCESS}" letter-spacing="2">ATTESTATION VÉRIFIÉE</text>

    <text x="104" y="248" font-size="68" font-weight="700" fill="{TEXT}">{name}</text>
    <text x="104" y="300" font-size="32" fill="{TEXT_MUTED}">@{handle}</text>

    <text x="104" y="382" font-size="28" fill="{TEXT}">{facts_line}</text>
    <text x="104" y="430" font-size="26" fill="{TEXT_MUTED}">{date_line}</text>

    <text x="104" y="524" font-size="22" font-weight="700" fill="{TEXT}">skill<tspan fill="{LUV}">uv</tspan></text>
    <text x="1096" y="524" font-size="20" font-family="{MONO}" fill="{TEXT_MUTED}" text-anchor="end">{hash_line}</text>
  </g>
</svg>"##
    )
}

/// Rasterise the card to PNG bytes.
pub fn render_png(data: &CardData) -> Result<Vec<u8>, AppError> {
    let svg = build_svg(data);
    let tree = usvg::Tree::from_str(&svg, options())
        .map_err(|e| AppError::Internal(format!("og card: invalid svg: {e}")))?;

    let mut pixmap = tiny_skia::Pixmap::new(CARD_WIDTH, CARD_HEIGHT)
        .ok_or_else(|| AppError::Internal("og card: could not allocate pixmap".into()))?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    pixmap
        .encode_png()
        .map_err(|e| AppError::Internal(format!("og card: png encode failed: {e}")))
}

/// The card shown when the hash is unknown or malformed.
///
/// Deliberately not a 404: a crawler that gets one renders no image at all,
/// and the person who shared the link never finds out why. A generic card
/// still carries the brand and tells a human the link is not a valid
/// attestation.
pub fn fallback_card() -> CardData {
    CardData {
        display_name: "Attestation introuvable".to_string(),
        username: "skilluv".to_string(),
        validated_on: Some("Ce lien ne correspond à aucune attestation".to_string()),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markup_in_user_controlled_values() {
        let data = CardData {
            display_name: "</text><script>alert(1)</script>".to_string(),
            username: "a\"b".to_string(),
            ..Default::default()
        };
        let svg = build_svg(&data);
        assert!(!svg.contains("<script>"), "markup must not survive: {svg}");
        assert!(svg.contains("&lt;script&gt;"));
        assert!(svg.contains("a&quot;b"));
    }

    #[test]
    fn truncates_names_that_would_overflow_the_card() {
        let long = "a".repeat(200);
        let data = CardData {
            display_name: long.clone(),
            username: long,
            ..Default::default()
        };
        let svg = build_svg(&data);
        assert!(svg.contains('…'));
        assert!(!svg.contains(&"a".repeat(40)));
    }

    #[test]
    fn truncate_respects_character_boundaries() {
        // Would panic on a byte slice.
        let s = "éèêëàâäôöûü";
        assert_eq!(truncate(s, 4).chars().count(), 4);
    }

    #[test]
    fn renders_a_png_of_the_expected_size() {
        let data = CardData {
            display_name: "Ada Lovelace".to_string(),
            username: "ada".to_string(),
            repository: Some("skilluv/skilluv-backend".to_string()),
            domain: Some("code".to_string()),
            difficulty: Some(3),
            validated_on: Some("12 août 2026".to_string()),
            hash: Some("a".repeat(64)),
        };
        let png = render_png(&data).expect("render");

        // PNG magic number, then the IHDR width/height as big-endian u32.
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
        let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
        assert_eq!((width, height), (CARD_WIDTH, CARD_HEIGHT));
    }

    #[test]
    fn the_card_is_not_blank() {
        // A missing font renders an image that is uniformly the background
        // colour — still a valid PNG, still 200, and completely useless.
        // Counting distinct pixels catches that.
        let data = CardData {
            display_name: "Ada Lovelace".to_string(),
            username: "ada".to_string(),
            ..Default::default()
        };
        let svg = build_svg(&data);
        let tree = usvg::Tree::from_str(&svg, options()).unwrap();
        let mut pixmap = tiny_skia::Pixmap::new(CARD_WIDTH, CARD_HEIGHT).unwrap();
        resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

        let distinct: std::collections::HashSet<[u8; 4]> = pixmap
            .pixels()
            .iter()
            .map(|p| [p.red(), p.green(), p.blue(), p.alpha()])
            .collect();
        assert!(
            distinct.len() > 8,
            "only {} distinct colours — the text probably did not render",
            distinct.len()
        );
    }

    #[test]
    fn the_fallback_card_renders_too() {
        let png = render_png(&fallback_card()).expect("fallback must render");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }
}
