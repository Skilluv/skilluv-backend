//! What an attestation looks like when somebody wants to show it.
//!
//! ## Two surfaces, one for each way a proof travels
//!
//! A share card, because an attestation pasted into a message is a link with
//! no picture otherwise, and a link with no picture is not opened. And a
//! printable sheet, because a candidate attaches a document to an application
//! and sometimes prints it.
//!
//! ## Why the sheet is SVG and not a rendered PDF
//!
//! The slice attestation's PDF posts HTML to an external renderer
//! (`PDF_RENDERER_URL`), which is a second service to run, pay for and watch —
//! and which returns 503 today because nobody runs it.
//!
//! An A4 SVG is printed to PDF by the browser, losslessly, with the fonts and
//! the vectors intact. It is a better PDF than a rasterised one, it needs no
//! service, and it works offline. The only thing lost is the server deciding
//! the filename.
//!
//! ## Why the layout changes with the basis
//!
//! An attestation for a validated deliverable, one for a contest won and one
//! for an editorial featuring are three different claims. A single template
//! with a swapped title would make them look interchangeable, which is
//! exactly what a reader should not conclude — the featuring rests on
//! somebody's judgement, the other two rest on an artefact.

use std::sync::OnceLock;

use resvg::tiny_skia;
use resvg::usvg;

use crate::errors::AppError;

/// Card dimensions. 1200×630 is what X, LinkedIn and Facebook expect; other
/// ratios get cropped unpredictably.
pub const CARD_WIDTH: u32 = 1200;
pub const CARD_HEIGHT: u32 = 630;

/// A4 at 96 dpi, in the units the browser prints from. Not 300 dpi: the SVG
/// is vector, so the print resolution is the printer's, not ours.
pub const SHEET_WIDTH: u32 = 794;
pub const SHEET_HEIGHT: u32 = 1123;

// Design system tokens, Forge theme. Mirrors `src/app.css` in the frontend,
// and `og_card`, which is where these came from.
const SURFACE: &str = "#18130f";
const SURFACE_ELEVATED: &str = "#2f2620";
const BORDER: &str = "#5a4738";
const TEXT: &str = "#f4ede0";
const TEXT_MUTED: &str = "#c2b195";
const LUV: &str = "#e63946";

// The printed sheet is the other way round: it is a document, and a document
// somebody photocopies has to be dark on light.
const PAPER: &str = "#ffffff";
const INK: &str = "#1a1a1a";
const INK_MUTED: &str = "#5c5c5c";
const RULE: &str = "#d8d2c8";

const SANS: &str = "Bricolage Grotesque";
const MONO: &str = "JetBrains Mono";

const SANS_TTF: &[u8] = include_bytes!("../../assets/fonts/BricolageGrotesque-Variable.ttf");
const MONO_TTF: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Variable.ttf");

/// How an attestation presents itself, decided by what it rests on.
///
/// Three families rather than one template per basis: the differences that
/// matter to a reader are *what kind of claim is this*, not which of the
/// twenty bases produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presentation {
    /// Rests on a verified artefact somebody can open. The strongest of the
    /// three, and the one the platform exists to produce.
    Artefact,
    /// Rests on a ranking: a contest won, a podium.
    Contest,
    /// Rests on somebody's judgement, and says so. Never dressed up as the
    /// other two.
    Editorial,
}

impl Presentation {
    /// Which family a basis belongs to.
    ///
    /// Unknown bases present as [`Presentation::Artefact`], which is the
    /// conservative default: every basis in the schema except the three
    /// editorial ones requires a deliverable, so a new one almost certainly
    /// does too. Getting it wrong that way understates nothing.
    pub fn for_basis(basis: &str) -> Self {
        match basis {
            "featured_coder" | "featured_designer" | "featured_ai_researcher" => Self::Editorial,
            b if b.ends_with("_contest_won") => Self::Contest,
            _ => Self::Artefact,
        }
    }

    /// The line above the name. In French, like the review grids and the
    /// attestation wording itself.
    pub fn kicker(self) -> &'static str {
        match self {
            Self::Artefact => "ATTESTATION — TRAVAIL VÉRIFIÉ",
            Self::Contest => "ATTESTATION — CONCOURS REMPORTÉ",
            Self::Editorial => "MISE EN AVANT ÉDITORIALE",
        }
    }

    /// The accent. Editorial is deliberately the quiet one: it is the claim
    /// with the least behind it, and it should not be the loudest thing on
    /// the page.
    pub fn accent(self) -> &'static str {
        match self {
            Self::Artefact => "#32b8ab",
            Self::Contest => "#ea8a3d",
            Self::Editorial => "#c2b195",
        }
    }

    /// What the sheet says about the basis of the claim, in one sentence a
    /// stranger can act on.
    pub fn footing(self) -> &'static str {
        match self {
            Self::Artefact => {
                "Cette attestation repose sur un livrable vérifié par un relecteur \
                 compétent dans le métier concerné."
            }
            Self::Contest => {
                "Cette attestation repose sur un classement public, établi sur des \
                 propositions que chacun peut consulter."
            }
            Self::Editorial => {
                "Cette mise en avant est un choix éditorial de Skilluv, assumé comme \
                 tel. Elle ne repose sur aucune formule."
            }
        }
    }
}

/// Everything the card and the sheet show. Built by the route from one query.
#[derive(Debug, Default, Clone)]
pub struct CertificateData {
    pub display_name: String,
    pub username: String,
    /// The attestation's own title, as it was issued. An attestation keeps
    /// the words it was issued with.
    pub title: String,
    pub basis: String,
    /// Already formatted for display — this module does no localisation.
    pub issued_on: Option<String>,
    /// The ten characters somebody types into the verification page.
    pub verification_code: String,
    /// Where a reader goes to check it. Also what the QR code encodes.
    pub verify_url: String,
    /// True when the attestation has been taken back. A revoked attestation
    /// still renders — somebody holding an old copy has to be able to find
    /// out that it no longer holds.
    pub revoked: bool,
}

impl CertificateData {
    pub fn presentation(&self) -> Presentation {
        Presentation::for_basis(&self.basis)
    }
}

/// Font database, built once. Parsing two variable fonts per request would be
/// wasted work on a response that is otherwise string formatting.
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
/// Every dynamic value here is user-controlled — a display name and an
/// attestation title are whatever somebody typed. Interpolating one raw would
/// let `</text>` inside a name rewrite the document.
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

/// Cut to `max` characters on a character boundary, with an ellipsis. A long
/// title must not run off the page.
fn truncate(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        return input.to_string();
    }
    let kept: String = input.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// Break a string into lines of at most `width` characters, on word
/// boundaries, up to `max_lines`.
///
/// SVG has no text wrapping. Without this a title of eighty characters is one
/// line running off the sheet, and the failure is invisible until somebody
/// prints it.
fn wrap(input: &str, width: usize, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();

    for word in input.split_whitespace() {
        let candidate_len = if current.is_empty() {
            word.chars().count()
        } else {
            current.chars().count() + 1 + word.chars().count()
        };
        if candidate_len > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            if lines.len() == max_lines {
                break;
            }
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }

    if lines.len() < max_lines && !current.is_empty() {
        lines.push(current);
    }

    // Anything that did not fit is signalled rather than dropped silently.
    if lines.len() == max_lines
        && input.split_whitespace().count() > lines.iter().flat_map(|l| l.split(' ')).count()
        && let Some(last) = lines.last_mut()
    {
        *last = truncate(last, width);
    }
    lines
}

/// The share card: what a link to this attestation looks like when pasted.
pub fn build_card_svg(data: &CertificateData) -> String {
    let presentation = data.presentation();
    let accent = presentation.accent();
    let kicker = presentation.kicker();

    let name = escape(&truncate(&data.display_name, 28));
    let handle = escape(&truncate(&data.username, 32));
    let title = escape(&truncate(&data.title, 46));

    let date_line = data
        .issued_on
        .as_deref()
        .map(|d| format!("Émise le {}", escape(d)))
        .unwrap_or_default();

    let code = escape(&data.verification_code);

    // A revoked attestation is still rendered, and says so loudly. Somebody
    // holding an old copy has to be able to find out that it no longer holds.
    let (badge_fill, badge_text) = if data.revoked {
        (LUV, "RÉVOQUÉE")
    } else {
        (accent, kicker)
    };

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{CARD_WIDTH}" height="{CARD_HEIGHT}" viewBox="0 0 {CARD_WIDTH} {CARD_HEIGHT}">
  <rect width="{CARD_WIDTH}" height="{CARD_HEIGHT}" fill="{SURFACE}"/>
  <rect x="56" y="56" width="1088" height="518" rx="24" fill="{SURFACE_ELEVATED}" stroke="{BORDER}" stroke-width="2"/>
  <rect x="56" y="56" width="1088" height="8" rx="4" fill="{badge_fill}"/>

  <g font-family="{SANS}">
    <circle cx="112" cy="132" r="7" fill="{badge_fill}"/>
    <text x="132" y="139" font-size="22" font-weight="600" fill="{badge_fill}" letter-spacing="2">{badge_text}</text>

    <text x="104" y="238" font-size="64" font-weight="700" fill="{TEXT}">{name}</text>
    <text x="104" y="286" font-size="28" fill="{TEXT_MUTED}">@{handle}</text>

    <text x="104" y="372" font-size="30" fill="{TEXT}">{title}</text>
    <text x="104" y="422" font-size="24" fill="{TEXT_MUTED}">{date_line}</text>

    <text x="104" y="524" font-size="22" font-weight="700" fill="{TEXT}">skill<tspan fill="{LUV}">uv</tspan></text>
    <text x="1096" y="524" font-size="20" font-family="{MONO}" fill="{TEXT_MUTED}" text-anchor="end">{code}</text>
  </g>
</svg>"##
    )
}

/// Rasterise the card to PNG bytes.
pub fn render_card_png(data: &CertificateData) -> Result<Vec<u8>, AppError> {
    let svg = build_card_svg(data);
    let tree = usvg::Tree::from_str(&svg, options())
        .map_err(|e| AppError::Internal(format!("attestation card: invalid svg: {e}")))?;

    let mut pixmap = tiny_skia::Pixmap::new(CARD_WIDTH, CARD_HEIGHT)
        .ok_or_else(|| AppError::Internal("attestation card: could not allocate pixmap".into()))?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    pixmap
        .encode_png()
        .map_err(|e| AppError::Internal(format!("attestation card: could not encode png: {e}")))
}

/// The printable sheet: A4, dark on light, with a QR to the verification
/// page.
///
/// Printed by the browser to PDF. Vector all the way through, so it is
/// sharper than anything a rasteriser would produce and it needs no service.
pub fn build_certificate_svg(data: &CertificateData) -> String {
    let presentation = data.presentation();
    let accent = presentation.accent();

    let name = escape(&truncate(&data.display_name, 34));
    let handle = escape(&truncate(&data.username, 34));
    let code = escape(&data.verification_code);
    let verify_url = escape(&truncate(&data.verify_url, 64));

    let title_lines = wrap(&data.title, 42, 3);
    let title_svg: String = title_lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            format!(
                r#"<text x="80" y="{y}" font-size="30" font-weight="600" fill="{INK}">{line}</text>"#,
                y = 470 + i * 44,
                line = escape(line)
            )
        })
        .collect::<Vec<_>>()
        .join("\n    ");

    let footing_lines = wrap(presentation.footing(), 68, 3);
    let footing_svg: String = footing_lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            format!(
                r#"<text x="80" y="{y}" font-size="16" fill="{INK_MUTED}">{line}</text>"#,
                y = 690 + i * 24,
                line = escape(line)
            )
        })
        .collect::<Vec<_>>()
        .join("\n    ");

    let date_line = data
        .issued_on
        .as_deref()
        .map(|d| format!("Émise le {}", escape(d)))
        .unwrap_or_default();

    // The QR encodes the verification URL, so a printed sheet is checkable
    // without anybody typing ten characters correctly.
    let qr = qr_svg(&data.verify_url);

    let revoked_banner = if data.revoked {
        format!(
            r##"<rect x="60" y="150" width="674" height="52" rx="8" fill="{LUV}"/>
    <text x="397" y="184" font-size="24" font-weight="700" fill="#ffffff" text-anchor="middle">CETTE ATTESTATION A ÉTÉ RÉVOQUÉE</text>"##
        )
    } else {
        String::new()
    };

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{SHEET_WIDTH}" height="{SHEET_HEIGHT}" viewBox="0 0 {SHEET_WIDTH} {SHEET_HEIGHT}">
  <rect width="{SHEET_WIDTH}" height="{SHEET_HEIGHT}" fill="{PAPER}"/>
  <rect x="0" y="0" width="{SHEET_WIDTH}" height="10" fill="{accent}"/>

  <g font-family="{SANS}">
    <text x="80" y="90" font-size="22" font-weight="700" fill="{INK}">skill<tspan fill="{LUV}">uv</tspan></text>
    <text x="714" y="90" font-size="14" font-family="{MONO}" fill="{INK_MUTED}" text-anchor="end">{code}</text>

    <text x="80" y="140" font-size="15" font-weight="600" fill="{accent}" letter-spacing="2">{kicker}</text>
    {revoked_banner}

    <line x1="80" y1="238" x2="714" y2="238" stroke="{RULE}" stroke-width="1"/>

    <text x="80" y="316" font-size="18" fill="{INK_MUTED}">Délivrée à</text>
    <text x="80" y="376" font-size="46" font-weight="700" fill="{INK}">{name}</text>
    <text x="80" y="412" font-size="20" fill="{INK_MUTED}">@{handle}</text>

    <text x="80" y="440" font-size="18" fill="{INK_MUTED}">Pour</text>
    {title_svg}

    <text x="80" y="640" font-size="18" fill="{INK_MUTED}">{date_line}</text>

    <line x1="80" y1="664" x2="714" y2="664" stroke="{RULE}" stroke-width="1"/>
    {footing_svg}

    <g transform="translate(80, 820)">
      <text x="0" y="0" font-size="16" font-weight="600" fill="{INK}">Vérifier cette attestation</text>
      <text x="0" y="26" font-size="13" font-family="{MONO}" fill="{INK_MUTED}">{verify_url}</text>
      <text x="0" y="52" font-size="13" fill="{INK_MUTED}">ou saisir le code {code} sur skill-uv.com</text>
    </g>

    <g transform="translate(574, 800)">{qr}</g>

    <text x="80" y="1060" font-size="12" fill="{INK_MUTED}">Une attestation Skilluv nomme un travail qu'un inconnu peut ouvrir et juger. Elle ne remplace pas un diplôme : elle dit ce qui a été fait.</text>
  </g>
</svg>"##,
        kicker = presentation.kicker()
    )
}

/// A QR code as inline SVG, sized to fit the sheet's corner.
///
/// On failure — a payload too long for the largest version — the sheet keeps
/// the printed URL and the code, and loses only the convenience. Better than
/// a 500 on a document somebody is trying to print.
fn qr_svg(payload: &str) -> String {
    match qrcode::QrCode::new(payload) {
        Ok(qr) => qr
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(140, 140)
            .max_dimensions(140, 140)
            .quiet_zone(true)
            .build()
            // The crate emits a standalone document; the sheet needs a
            // fragment, so its own root is dropped.
            .replace(r#"<?xml version="1.0" standalone="yes"?>"#, "")
            .replace(r#"<svg xmlns="http://www.w3.org/2000/svg""#, r#"<svg"#),
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_certificate() -> CertificateData {
        CertificateData {
            display_name: "Aïcha Traoré".into(),
            username: "aicha".into(),
            title: "Identité complète pour une coopérative de transformation d'anacarde".into(),
            basis: "design_deliverable_validated".into(),
            issued_on: Some("14/03/2027".into()),
            verification_code: "K4M2P9XZQ7".into(),
            verify_url: "https://skill-uv.com/attestations/verify/K4M2P9XZQ7".into(),
            revoked: false,
        }
    }

    #[test]
    fn an_editorial_featuring_never_looks_like_a_verified_artefact() {
        // The featuring rests on somebody's judgement and the other two rest
        // on an artefact. A single template with a swapped title would make
        // them look interchangeable, which is what a reader must not
        // conclude.
        assert_eq!(
            Presentation::for_basis("featured_designer"),
            Presentation::Editorial
        );
        assert_eq!(
            Presentation::for_basis("design_deliverable_validated"),
            Presentation::Artefact
        );
        assert_eq!(
            Presentation::for_basis("design_contest_won"),
            Presentation::Contest
        );

        // And the editorial one is the quiet colour: it is the claim with the
        // least behind it.
        assert_ne!(
            Presentation::Editorial.accent(),
            Presentation::Artefact.accent()
        );
        assert!(Presentation::Editorial.footing().contains("éditorial"));
    }

    #[test]
    fn an_unknown_basis_presents_as_an_artefact() {
        // Every basis in the schema except the three editorial ones requires
        // a deliverable, so a new one almost certainly does too.
        assert_eq!(
            Presentation::for_basis("something_invented_next_year"),
            Presentation::Artefact
        );
    }

    #[test]
    fn a_name_cannot_rewrite_the_document() {
        // A display name is whatever its owner typed.
        let mut data = a_certificate();
        data.display_name = "</text><script>alert(1)</script>".into();
        let svg = build_card_svg(&data);
        assert!(!svg.contains("<script>"), "{svg}");
        assert!(svg.contains("&lt;script&gt;"));
    }

    #[test]
    fn a_long_title_wraps_instead_of_running_off_the_sheet() {
        // SVG has no text wrapping, and the failure is invisible until
        // somebody prints it.
        let lines = wrap(&a_certificate().title, 42, 3);
        assert!(lines.len() > 1, "{lines:?}");
        for line in &lines {
            assert!(line.chars().count() <= 42, "{line}");
        }
    }

    #[test]
    fn a_word_longer_than_the_line_still_produces_a_line() {
        let lines = wrap("Designsystemsupercalifragilisticexpialidocious", 20, 3);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn wrapping_nothing_produces_nothing() {
        assert!(wrap("", 40, 3).is_empty());
        assert!(wrap("   ", 40, 3).is_empty());
    }

    #[test]
    fn a_revoked_attestation_still_renders_and_says_so() {
        // Somebody holding an old copy has to be able to find out that it no
        // longer holds. A 404 would leave them believing it.
        let mut data = a_certificate();
        data.revoked = true;

        let card = build_card_svg(&data);
        assert!(card.contains("RÉVOQUÉE"), "{card}");

        let sheet = build_certificate_svg(&data);
        assert!(sheet.contains("A ÉTÉ RÉVOQUÉE"), "{sheet}");
    }

    #[test]
    fn both_surfaces_render_without_panicking() {
        let data = a_certificate();
        assert!(render_card_png(&data).is_ok());

        let sheet = build_certificate_svg(&data);
        assert!(usvg::Tree::from_str(&sheet, options()).is_ok(), "{sheet}");
        // The code is printed as well as encoded, so a sheet photocopied
        // badly enough to lose the QR is still checkable by hand.
        assert!(sheet.contains("K4M2P9XZQ7"));
    }
}
