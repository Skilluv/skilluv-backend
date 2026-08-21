//! Automatic checks on a design version.
//!
//! ## What these are for, and what they are not
//!
//! The design domain has no green CI. Every verdict is a person's, and that is
//! deliberate: nothing here can judge whether a mark is right for a
//! cooperative or whether a hierarchy reads. A version can pass every check
//! below and be rejected, and that is a correct outcome rather than a bug.
//!
//! What these do is take the mechanical half off a reviewer's hands. A
//! contrast ratio is arithmetic, and a human computing it by hand is a human
//! not looking at the drawing. Every check here is something that has one
//! right answer.
//!
//! ## Why nothing blocks
//!
//! Severity is `info`, `warning` or `error`, and none of them stops a
//! submission or forces a verdict. A check that blocked would be a check that
//! has to be right every time — and the first false positive on somebody's
//! deliberate choice would teach the whole community to work around it.
//!
//! The reviewer reads them beside the work. `error` means "this is almost
//! certainly wrong and worth a sentence in the critique", not "refuse this".
//!
//! ## Why some versions are reported as unchecked rather than green
//!
//! A Figma or Miro URL cannot be read without holding somebody's design
//! account, which the platform does not do. Those versions record an `info`
//! saying so. Recording nothing would be indistinguishable from passing, and
//! a reviewer would read silence as approval.

use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// How large a fetched artefact may be before it is not worth reading.
///
/// Two megabytes. Every check here works on text — SVG source, a Lottie
/// document, a token file — and a text artefact past this size is not a token
/// file, it is an export somebody mislabelled.
const MAX_FETCH_BYTES: usize = 2 * 1024 * 1024;

/// How long to wait for somebody else's host.
const FETCH_TIMEOUT_SECONDS: u64 = 15;

/// What one check found.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckResult {
    /// Which check spoke. Stable, because a reviewer's client groups by it.
    pub check_type: &'static str,
    pub severity: Severity,
    /// One sentence, addressed to the reviewer, in French like the grids.
    pub message: String,
    /// The numbers behind the sentence, for a client that wants to show them.
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Contrast
// ═══════════════════════════════════════════════════════════════════

/// Relative luminance, as WCAG 2 defines it.
///
/// The gamma expansion is not decorative: a naive average of the channels
/// gets the answer wrong on exactly the pairs people get wrong by eye — dark
/// blue on black, mid grey on white.
fn relative_luminance(rgb: (u8, u8, u8)) -> f64 {
    fn channel(value: u8) -> f64 {
        let v = value as f64 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(rgb.0) + 0.7152 * channel(rgb.1) + 0.0722 * channel(rgb.2)
}

/// The WCAG contrast ratio between two colours, from 1.0 to 21.0.
pub fn contrast_ratio(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (lighter, darker) = if la > lb { (la, lb) } else { (lb, la) };
    (lighter + 0.05) / (darker + 0.05)
}

/// Parse `#rgb`, `#rrggbb` or `#rrggbbaa`.
///
/// Alpha is parsed and discarded: a contrast ratio needs the colour that will
/// actually be seen, and computing one against a translucent value would
/// produce a confident wrong number. A translucent colour is reported as
/// unparseable instead, which is the honest answer.
pub fn parse_hex(value: &str) -> Option<(u8, u8, u8)> {
    let hex = value.trim().trim_start_matches('#');
    let digits: Vec<u8> = hex
        .chars()
        .map(|c| c.to_digit(16).map(|d| d as u8))
        .collect::<Option<Vec<u8>>>()?;

    match digits.len() {
        3 => Some((digits[0] * 17, digits[1] * 17, digits[2] * 17)),
        6 | 8 => Some((
            digits[0] * 16 + digits[1],
            digits[2] * 16 + digits[3],
            digits[4] * 16 + digits[5],
        )),
        _ => None,
    }
}

/// WCAG AA for body text.
const AA_NORMAL_TEXT: f64 = 4.5;
/// WCAG AA for large text and for graphical objects.
const AA_LARGE_TEXT: f64 = 3.0;

/// Every pair in a declared palette, against the AA thresholds.
///
/// A palette is checked pairwise rather than against white, because a brand
/// palette is used against itself: the failure a designer discovers late is
/// the secondary on the primary, not the primary on paper.
///
/// This reports rather than refuses. A palette legitimately contains colours
/// that are never set on each other, and a check that treated every pair as a
/// requirement would flag every palette ever made.
pub fn check_palette(palette: &[(String, String)]) -> Vec<CheckResult> {
    let parsed: Vec<(&str, (u8, u8, u8))> = palette
        .iter()
        .filter_map(|(name, value)| parse_hex(value).map(|rgb| (name.as_str(), rgb)))
        .collect();

    let unparseable = palette.len() - parsed.len();
    let mut results = Vec::new();

    if unparseable > 0 {
        results.push(CheckResult {
            check_type: "palette_contrast",
            severity: Severity::Info,
            message: format!(
                "{unparseable} couleur(s) n'ont pas pu être lues : le contraste n'est pas calculé dessus."
            ),
            details: None,
        });
    }

    if parsed.len() < 2 {
        return results;
    }

    let mut usable_pairs = 0;
    let mut worst: Option<(&str, &str, f64)> = None;

    for (i, (name_a, a)) in parsed.iter().enumerate() {
        for (name_b, b) in parsed.iter().skip(i + 1) {
            let ratio = contrast_ratio(*a, *b);
            if ratio >= AA_NORMAL_TEXT {
                usable_pairs += 1;
            }
            if worst.is_none_or(|(_, _, w)| ratio < w) {
                worst = Some((name_a, name_b, ratio));
            }
        }
    }

    // The useful question is not "does every pair pass" — it never does — but
    // "is there any pair somebody can set text in".
    let severity = if usable_pairs == 0 {
        Severity::Error
    } else {
        Severity::Info
    };
    let message = if usable_pairs == 0 {
        "Aucune paire de la palette n'atteint 4,5:1 : aucun texte n'est lisible en n'utilisant que ces couleurs.".to_string()
    } else {
        format!("{usable_pairs} paire(s) de la palette atteignent 4,5:1 pour du texte courant.")
    };

    results.push(CheckResult {
        check_type: "palette_contrast",
        severity,
        message,
        details: Some(serde_json::json!({
            "pairs_passing_aa": usable_pairs,
            "aa_normal_text": AA_NORMAL_TEXT,
            "aa_large_text": AA_LARGE_TEXT,
            "worst_pair": worst.map(|(a, b, ratio)| serde_json::json!({
                "a": a, "b": b, "ratio": (ratio * 100.0).round() / 100.0,
            })),
        })),
    });

    results
}

// ═══════════════════════════════════════════════════════════════════
// Design tokens
// ═══════════════════════════════════════════════════════════════════

/// The spacing step a scale is expected to be built on.
///
/// Four. Not a universal truth — some systems use 8, some use a modular
/// scale — which is why a value off the step is a `warning` naming the step
/// rather than an error. What it catches is the real failure: a scale that is
/// 4, 8, 12, 16, 22, 24, where one value was typed rather than derived.
const SPACING_STEP: i64 = 4;

/// Lint a design-token document.
///
/// Reads the flat `{ "name": value }` shape and the nested
/// `{ "color": { "primary": { "value": … } } }` shape, because both are in
/// the wild and refusing one would just move the argument to which is
/// correct.
pub fn check_tokens(tokens: &Value) -> Vec<CheckResult> {
    let mut flat: Vec<(String, Value)> = Vec::new();
    flatten_tokens("", tokens, &mut flat);

    if flat.is_empty() {
        return vec![CheckResult {
            check_type: "token_lint",
            severity: Severity::Warning,
            message: "Aucun jeton lisible dans le fichier fourni.".to_string(),
            details: None,
        }];
    }

    let mut problems: Vec<String> = Vec::new();

    for (path, value) in &flat {
        let leaf = path.rsplit('.').next().unwrap_or(path);

        if path.contains("opacity")
            && let Some(number) = value.as_f64()
            && !(0.0..=1.0).contains(&number)
        {
            problems.push(format!(
                "{path} = {number} : une opacité tient entre 0 et 1"
            ));
        }

        if (path.contains("radius") || path.contains("rayon"))
            && let Some(number) = value.as_f64()
            && number < 0.0
        {
            problems.push(format!("{path} = {number} : un rayon négatif n'existe pas"));
        }

        if (path.contains("spacing") || path.contains("espacement"))
            && let Some(number) = value.as_i64()
            && number % SPACING_STEP != 0
        {
            problems.push(format!(
                "{path} = {number} : hors du pas de {SPACING_STEP} suivi par le reste de l'échelle"
            ));
        }

        // A name in two conventions at once is the sign of two people, or of
        // one person and one copy-paste.
        if leaf.contains('_') && leaf.contains('-') {
            problems.push(format!(
                "{path} : mélange tiret et souligné dans le même nom"
            ));
        }
    }

    if problems.is_empty() {
        return vec![CheckResult {
            check_type: "token_lint",
            severity: Severity::Info,
            message: format!("{} jetons lus, rien à signaler.", flat.len()),
            details: None,
        }];
    }

    vec![CheckResult {
        check_type: "token_lint",
        severity: Severity::Warning,
        message: format!("{} jeton(s) à revoir.", problems.len()),
        details: Some(serde_json::json!({ "problems": problems })),
    }]
}

/// Walk a token document into `path -> leaf value` pairs.
///
/// A `{ "value": … }` wrapper is unwrapped, which is what the Design Tokens
/// format puts around every leaf; without that, every path would end in
/// `.value` and every name check would be looking at the wrong word.
fn flatten_tokens(prefix: &str, node: &Value, out: &mut Vec<(String, Value)>) {
    match node {
        Value::Object(map) => {
            if let Some(value) = map.get("value")
                && !value.is_object()
            {
                out.push((prefix.to_string(), value.clone()));
                return;
            }
            for (key, child) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_tokens(&path, child, out);
            }
        }
        other if !prefix.is_empty() => out.push((prefix.to_string(), other.clone())),
        _ => {}
    }
}

// ═══════════════════════════════════════════════════════════════════
// Motion
// ═══════════════════════════════════════════════════════════════════

/// Past this many layers, a Lottie file is expensive to play on the phones
/// most of our users have. Not a hard limit — a warning, and a number a
/// reviewer can weigh against what the animation does.
const LOTTIE_LAYER_BUDGET: usize = 60;
/// Past this many seconds, an interface animation is not an interface
/// animation.
const LOTTIE_SECONDS_BUDGET: f64 = 5.0;

/// Read a Lottie document and say what it will cost to play.
pub fn check_lottie(document: &Value) -> Vec<CheckResult> {
    let Some(frame_rate) = document.get("fr").and_then(Value::as_f64) else {
        return vec![CheckResult {
            check_type: "motion_cost",
            severity: Severity::Info,
            message: "Le fichier fourni n'est pas un document Lottie lisible.".to_string(),
            details: None,
        }];
    };

    let in_point = document.get("ip").and_then(Value::as_f64).unwrap_or(0.0);
    let out_point = document.get("op").and_then(Value::as_f64).unwrap_or(0.0);
    let seconds = if frame_rate > 0.0 {
        (out_point - in_point) / frame_rate
    } else {
        0.0
    };

    // Layers nested in precompositions count: they are rendered too, and a
    // file that hides forty layers in one precomp is exactly the file this
    // check exists to notice.
    let layers = count_lottie_layers(document);

    let mut notes = Vec::new();
    if layers > LOTTIE_LAYER_BUDGET {
        notes.push(format!(
            "{layers} calques (au-delà de {LOTTIE_LAYER_BUDGET}, le rendu devient coûteux sur un téléphone d'entrée de gamme)"
        ));
    }
    if seconds > LOTTIE_SECONDS_BUDGET {
        notes.push(format!(
            "{seconds:.1} s (au-delà de {LOTTIE_SECONDS_BUDGET} s, ce n'est plus une animation d'interface)"
        ));
    }

    let details = Some(serde_json::json!({
        "layers": layers,
        "seconds": (seconds * 10.0).round() / 10.0,
        "frame_rate": frame_rate,
    }));

    if notes.is_empty() {
        vec![CheckResult {
            check_type: "motion_cost",
            severity: Severity::Info,
            message: format!("{layers} calques, {seconds:.1} s à {frame_rate} images/s."),
            details,
        }]
    } else {
        vec![CheckResult {
            check_type: "motion_cost",
            severity: Severity::Warning,
            message: notes.join(" ; "),
            details,
        }]
    }
}

fn count_lottie_layers(node: &Value) -> usize {
    let mut total = 0;
    if let Some(layers) = node.get("layers").and_then(Value::as_array) {
        total += layers.len();
        for layer in layers {
            total += count_lottie_layers(layer);
        }
    }
    if let Some(assets) = node.get("assets").and_then(Value::as_array) {
        for asset in assets {
            total += count_lottie_layers(asset);
        }
    }
    total
}

// ═══════════════════════════════════════════════════════════════════
// SVG
// ═══════════════════════════════════════════════════════════════════

/// Look at one SVG the way an icon set is looked at.
///
/// Deliberately not a parser. What matters for a set is whether the drawings
/// share a coordinate system and a stroke weight, and both are readable from
/// the attributes without pulling in an XML dependency to be wrong about
/// namespaces.
pub fn check_svg(source: &str) -> Vec<CheckResult> {
    let view_box = attribute(source, "viewBox");
    let stroke_widths = all_attributes(source, "stroke-width");

    let mut results = Vec::new();

    match view_box {
        None => results.push(CheckResult {
            check_type: "svg_consistency",
            severity: Severity::Warning,
            message: "Pas de viewBox : le dessin ne se met pas à l'échelle proprement.".to_string(),
            details: None,
        }),
        Some(view_box) => results.push(CheckResult {
            check_type: "svg_consistency",
            severity: Severity::Info,
            message: format!("viewBox {view_box}."),
            details: Some(serde_json::json!({ "view_box": view_box })),
        }),
    }

    let mut distinct: Vec<&String> = Vec::new();
    for width in &stroke_widths {
        if !distinct.contains(&width) {
            distinct.push(width);
        }
    }
    if distinct.len() > 1 {
        results.push(CheckResult {
            check_type: "svg_consistency",
            severity: Severity::Warning,
            message: format!(
                "{} épaisseurs de trait différentes dans le même fichier.",
                distinct.len()
            ),
            details: Some(serde_json::json!({
                "stroke_widths": distinct.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            })),
        });
    }

    results
}

fn attribute(source: &str, name: &str) -> Option<String> {
    all_attributes(source, name).into_iter().next()
}

fn all_attributes(source: &str, name: &str) -> Vec<String> {
    let needle = format!("{name}=\"");
    source
        .match_indices(&needle)
        .filter_map(|(at, _)| {
            let rest = &source[at + needle.len()..];
            rest.find('"').map(|end| rest[..end].to_string())
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════
// Running them, and keeping the answers
// ═══════════════════════════════════════════════════════════════════

/// Fetch what can be fetched and run what applies.
///
/// The URL decides which checks run, and a URL nobody can read produces an
/// `info` saying so rather than nothing. Silence and success have to look
/// different: a reviewer reading an empty panel would conclude the version
/// passed.
pub async fn run_for_version(
    db: &PgPool,
    slice_id: Uuid,
    round: i16,
    artifact_url: &str,
) -> Result<Vec<CheckResult>, AppError> {
    let results = match fetchable(artifact_url) {
        None => vec![CheckResult {
            check_type: "fetch",
            severity: Severity::Info,
            message: "Cet hébergeur ne se lit pas sans compte : aucune vérification automatique n'a tourné."
                .to_string(),
            details: Some(serde_json::json!({ "url": artifact_url })),
        }],
        Some(kind) => match fetch_text(artifact_url).await {
            Err(message) => vec![CheckResult {
                check_type: "fetch",
                severity: Severity::Warning,
                message,
                details: Some(serde_json::json!({ "url": artifact_url })),
            }],
            Ok(body) => run_on_body(kind, &body),
        },
    };

    store(db, slice_id, round, &results).await?;
    Ok(results)
}

/// What kind of document an address promises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fetchable {
    Svg,
    Json,
}

/// Decide from the address alone, before spending a request.
pub fn fetchable(url: &str) -> Option<Fetchable> {
    let lowered = url.trim().to_ascii_lowercase();
    if !lowered.starts_with("https://") {
        return None;
    }
    // The path, without a query string that would hide the extension.
    let path = lowered.split(['?', '#']).next().unwrap_or(&lowered);
    if path.ends_with(".svg") {
        Some(Fetchable::Svg)
    } else if path.ends_with(".json") {
        Some(Fetchable::Json)
    } else {
        None
    }
}

fn run_on_body(kind: Fetchable, body: &str) -> Vec<CheckResult> {
    match kind {
        Fetchable::Svg => check_svg(body),
        Fetchable::Json => match serde_json::from_str::<Value>(body) {
            Err(_) => vec![CheckResult {
                check_type: "fetch",
                severity: Severity::Warning,
                message: "Le fichier annoncé en JSON ne se lit pas.".to_string(),
                details: None,
            }],
            // A Lottie document is recognised by its frame rate; anything
            // else with a JSON extension is treated as tokens. Guessing wrong
            // costs a useless line, not a wrong verdict.
            Ok(document) if document.get("fr").is_some() => check_lottie(&document),
            Ok(document) => check_tokens(&document),
        },
    }
}

async fn fetch_text(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(FETCH_TIMEOUT_SECONDS))
        .build()
        .map_err(|e| format!("client HTTP indisponible : {e}"))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "Le fichier n'a pas pu être téléchargé.".to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "Le fichier répond {} : personne d'autre ne pourra l'ouvrir non plus.",
            response.status().as_u16()
        ));
    }

    // Checked before reading rather than after: the point of a ceiling is not
    // to download two hundred megabytes and then decide against them.
    if let Some(length) = response.content_length()
        && length as usize > MAX_FETCH_BYTES
    {
        return Err(format!(
            "Fichier de {} Ko : trop gros pour être un fichier source lisible.",
            length / 1024
        ));
    }

    let body = response
        .text()
        .await
        .map_err(|_| "Le fichier n'a pas pu être lu.".to_string())?;

    if body.len() > MAX_FETCH_BYTES {
        return Err("Fichier trop gros pour être vérifié automatiquement.".to_string());
    }
    Ok(body)
}

/// Keep the answers beside the round they were computed for.
///
/// Replacing the round's previous results rather than appending: a version
/// re-checked has one truth, and a reviewer scrolling two contradictory
/// contrast readings has learned to ignore the panel.
async fn store(
    db: &PgPool,
    slice_id: Uuid,
    round: i16,
    results: &[CheckResult],
) -> Result<(), AppError> {
    let mut tx = db.begin().await?;

    sqlx::query("DELETE FROM design_auto_check_results WHERE slice_id = $1 AND round = $2")
        .bind(slice_id)
        .bind(round)
        .execute(&mut *tx)
        .await?;

    for result in results {
        sqlx::query(
            "INSERT INTO design_auto_check_results
                 (slice_id, round, check_type, severity, message, details)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(slice_id)
        .bind(round)
        .bind(result.check_type)
        .bind(result.severity.as_str())
        .bind(&result.message)
        .bind(&result.details)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// What the checks found, as a reviewer reads them.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct StoredCheck {
    pub round: i16,
    pub check_type: String,
    pub severity: String,
    pub message: String,
    pub details: Option<Value>,
    pub ran_at: chrono::DateTime<chrono::Utc>,
}

pub async fn results_for(db: &PgPool, slice_id: Uuid) -> Result<Vec<StoredCheck>, AppError> {
    let rows = sqlx::query_as::<_, StoredCheck>(
        "SELECT round, check_type, severity, message, details, ran_at
           FROM design_auto_check_results
          WHERE slice_id = $1
          ORDER BY round ASC, severity DESC, check_type ASC",
    )
    .bind(slice_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_ends_of_the_contrast_scale() {
        // The definition's own bounds: identical colours are 1, black on
        // white is 21. A luminance formula that drops the gamma expansion
        // still passes the first and fails the second.
        assert!((contrast_ratio((0, 0, 0), (0, 0, 0)) - 1.0).abs() < 0.001);
        assert!((contrast_ratio((0, 0, 0), (255, 255, 255)) - 21.0).abs() < 0.01);
    }

    #[test]
    fn a_pair_that_looks_fine_and_is_not() {
        // Mid grey on white reads as comfortable and sits at 3.9 — under AA
        // for body text. This is the pair the check exists for.
        let ratio = contrast_ratio((0x77, 0x77, 0x77), (0xFF, 0xFF, 0xFF));
        assert!(ratio < AA_NORMAL_TEXT, "{ratio}");
        assert!(ratio > AA_LARGE_TEXT, "{ratio}");
    }

    #[test]
    fn hex_in_its_three_written_forms() {
        assert_eq!(parse_hex("#fff"), Some((255, 255, 255)));
        assert_eq!(parse_hex("#FFFFFF"), Some((255, 255, 255)));
        // Alpha is read and dropped: the ratio needs the colour that will be
        // seen, and a confident number against a translucent value is worse
        // than none.
        assert_eq!(parse_hex("#00000080"), Some((0, 0, 0)));
        assert_eq!(parse_hex("rouge"), None);
        assert_eq!(parse_hex("#ff"), None);
    }

    #[test]
    fn a_palette_nobody_can_set_text_in_is_an_error() {
        let palette = vec![
            ("primaire".to_string(), "#777777".to_string()),
            ("secondaire".to_string(), "#8a8a8a".to_string()),
        ];
        let results = check_palette(&palette);
        assert_eq!(results[0].severity, Severity::Error);
    }

    #[test]
    fn a_palette_with_one_usable_pair_is_not_an_error() {
        // Palettes contain colours never set on each other. Treating every
        // pair as a requirement would flag every palette ever made.
        let palette = vec![
            ("encre".to_string(), "#111111".to_string()),
            ("papier".to_string(), "#ffffff".to_string()),
            ("accent".to_string(), "#f0f0f0".to_string()),
        ];
        let results = check_palette(&palette);
        assert_eq!(results[0].severity, Severity::Info);
    }

    #[test]
    fn tokens_are_read_in_both_shapes_in_the_wild() {
        let nested = serde_json::json!({
            "spacing": { "small": { "value": 4 }, "medium": { "value": 8 } }
        });
        let flat = serde_json::json!({ "spacing.small": 4, "spacing.medium": 8 });
        for document in [nested, flat] {
            let results = check_tokens(&document);
            assert_eq!(results[0].severity, Severity::Info, "{results:?}");
        }
    }

    #[test]
    fn a_value_typed_instead_of_derived_is_caught() {
        let document = serde_json::json!({
            "spacing": { "a": { "value": 4 }, "b": { "value": 22 } },
            "opacity": { "veil": { "value": 40 } }
        });
        let results = check_tokens(&document);
        assert_eq!(results[0].severity, Severity::Warning);
        let problems = results[0].details.as_ref().unwrap()["problems"]
            .as_array()
            .unwrap()
            .len();
        assert_eq!(problems, 2, "{results:?}");
    }

    #[test]
    fn layers_hidden_in_a_precomposition_still_count() {
        // The file this check exists to notice: forty layers behind one.
        let document = serde_json::json!({
            "fr": 60, "ip": 0, "op": 120,
            "layers": [{ "ty": 0 }],
            "assets": [{ "layers": (0..70).map(|_| serde_json::json!({"ty": 4})).collect::<Vec<_>>() }]
        });
        let results = check_lottie(&document);
        assert_eq!(results[0].severity, Severity::Warning);
        assert_eq!(results[0].details.as_ref().unwrap()["layers"], 71);
    }

    #[test]
    fn a_short_cheap_animation_passes() {
        let document = serde_json::json!({
            "fr": 60, "ip": 0, "op": 60, "layers": [{ "ty": 4 }]
        });
        assert_eq!(check_lottie(&document)[0].severity, Severity::Info);
    }

    #[test]
    fn mixed_stroke_widths_in_one_drawing_are_flagged() {
        let svg =
            r#"<svg viewBox="0 0 24 24"><path stroke-width="1.5"/><path stroke-width="2"/></svg>"#;
        let results = check_svg(svg);
        assert!(results.iter().any(|r| r.severity == Severity::Warning));
    }

    #[test]
    fn a_drawing_without_a_viewbox_does_not_scale() {
        let results = check_svg(r#"<svg width="24"><path/></svg>"#);
        assert_eq!(results[0].severity, Severity::Warning);
    }

    #[test]
    fn only_addresses_that_can_be_read_are_fetched() {
        assert_eq!(fetchable("https://x.test/icon.svg"), Some(Fetchable::Svg));
        assert_eq!(
            fetchable("https://x.test/tokens.json?v=2"),
            Some(Fetchable::Json)
        );
        // A design account is needed for these, and the platform holds none.
        assert_eq!(fetchable("https://figma.com/file/abc"), None);
        // Never plain HTTP: a check result is written against whatever came
        // back, and an intercepted answer would be recorded as fact.
        assert_eq!(fetchable("http://x.test/icon.svg"), None);
    }
}
