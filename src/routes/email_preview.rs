//! See an email before anyone receives it.
//!
//! There are nine locale-and-theme combinations for every kind in the
//! catalogue, and the only way to look at one used to be to trigger the
//! event that sends it — register an account and wait a day for the drip,
//! or move real money for a receipt. So nobody looked, which is how an
//! email ships interpolating `{title}` from a column that does not exist.
//!
//! This renders any kind, in any language, in any theme, and sends
//! nothing. There is no code path from here to the mail provider: it calls
//! [`email_template::render`] and returns the string.
//!
//! ## Why the sample values are not real data
//!
//! A preview that pulled a real payout to fill `{amount}` would be an
//! endpoint that reads other people's money. The placeholders are filled
//! from a fixed table below, chosen to be obviously fake and long enough
//! to show where a layout breaks.

use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::{email_template, email_theme, i18n};

pub fn email_preview_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/email-preview", get(preview))
        .route("/admin/email-preview/index", get(index))
}

/// Stand-ins for the placeholders the catalogue's copy interpolates.
///
/// Deliberately fake and deliberately awkward: an accented name, a
/// right-to-left-hostile amount, a title long enough to wrap. A preview
/// filled with `Test` proves nothing about a layout.
const SAMPLE_ARGS: &[(&str, &str)] = &[
    ("after", "critique"),
    ("amount", "42 500 XOF"),
    ("applicant", "Kofi Mensah"),
    ("author", "Awa Kponou-Diallo"),
    ("badge", "Première contribution vérifiée"),
    ("before", "élevée"),
    ("brief", "Une identité pour une coopérative de karité"),
    ("capability", "Steward de projet"),
    ("challenger", "Awa Kponou-Diallo"),
    ("challenges_completed", "7"),
    ("company", "Atelier Numérique du Golfe"),
    ("contest", "Concours de design, édition de mars"),
    ("count", "3"),
    ("date", "jeudi 12 mars, 18h30"),
    ("days", "12"),
    ("decision", "acceptée"),
    ("defender", "Les Forgerons de Cotonou"),
    ("destination", "MTN Mobile Money"),
    (
        "excerpt",
        "regarde la review, il reste deux commentaires à traiter",
    ),
    (
        "feedback",
        "l'énoncé décrit deux exercices différents ; garde le premier et ouvre une seconde proposition pour l'autre",
    ),
    ("fragments_earned", "180"),
    ("goal", "atteindre le rang Ranger"),
    ("guild", "Les Forgerons de Cotonou"),
    ("hours", "48"),
    ("inviter", "Awa Kponou-Diallo"),
    ("level", "Artisan"),
    (
        "message",
        "on cherche quelqu'un sur de la revue Rust, ton dernier livrable correspond",
    ),
    ("mission", "Reprendre le pipeline de déploiement"),
    ("name", "Awa"),
    ("other", "Kofi Mensah"),
    ("others", "2"),
    ("payer", "Atelier Numérique du Golfe"),
    ("place", "2e"),
    ("provider", "FedaPay"),
    ("rank", "Ranger"),
    (
        "reason",
        "plusieurs livrables copiés depuis un dépôt public sans attribution",
    ),
    ("reviewer", "Kofi Mensah"),
    ("role", "steward"),
    ("rounds", "3"),
    ("slice", "Écran de connexion, deuxième version"),
    ("stake", "1 500 fragments"),
    ("severity", "élevée (CVSS 8.1)"),
    ("subject", "une soumission signalée automatiquement"),
    ("tag", "rust"),
    ("talent", "Awa Kponou-Diallo"),
    ("title", "Ajouter la locale wolof au sélecteur de langue"),
    ("tournament", "Tournoi de la Forge, édition de mars"),
    ("weeks", "6"),
];

/// Figures shown when previewing a kind that carries them.
const SAMPLE_STATS: &[(&str, &str)] = &[
    ("digest.stat.challenges", "4"),
    ("digest.stat.fragments", "1 250"),
    ("digest.stat.streak", "12"),
];

#[derive(Debug, Deserialize, IntoParams)]
pub struct PreviewQuery {
    /// Catalogue kind, e.g. `payout.sent`.
    pub kind: String,
    /// BCP-47 tag. Defaults to the platform default.
    pub locale: Option<String>,
    /// One of the five worlds. Defaults to the workshop.
    pub theme: Option<String>,
}

/// Render one email as HTML, without sending it.
#[utoipa::path(
    get,
    path = "/api/admin/email-preview",
    tag = "admin",
    params(PreviewQuery),
    responses(
        (status = 200, description = "The rendered email", content_type = "text/html"),
        (status = 400, description = "Unknown kind", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn preview(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<PreviewQuery>,
) -> Result<Html<String>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    // The kind has to exist, or the preview would render a page whose title
    // is the kind's own identifier and look like a bug in the template
    // rather than a typo in the query.
    let known: Option<(String, bool, Option<String>)> = sqlx::query_as(
        "SELECT category, transactional, cta_path FROM notification_kinds WHERE kind = $1",
    )
    .bind(&query.kind)
    .fetch_optional(&state.db)
    .await?;

    let Some((_category, transactional, cta_path)) = known else {
        return Err(AppError::Validation(format!(
            "unknown kind '{}' — see /api/admin/email-preview/index",
            query.kind
        )));
    };

    let locale = i18n::resolve(query.locale.as_deref(), None);
    let title = i18n::t_with(
        &locale,
        &format!("notification.{}.title", query.kind),
        SAMPLE_ARGS,
    );
    let body = i18n::t_with(
        &locale,
        &format!("notification.{}.body", query.kind),
        SAMPLE_ARGS,
    );

    // The button is shown exactly when a real send would show one: a kind
    // with no `cta_path` gets none, rather than a preview promising an
    // action the email does not carry.
    let cta_label = cta_path
        .as_ref()
        .map(|_| i18n::t(&locale, &format!("notification.{}.cta", query.kind)));
    let cta_url = cta_path.as_ref().map(|path| {
        format!(
            "{}{}",
            state.config.frontend_url.trim_end_matches('/'),
            // Placeholders in the path have no payload to fill them here.
            path.replace(['{', '}'], "")
        )
    });

    // Only the digest has figures today, and showing them everywhere would
    // make every preview lie about its own shape.
    let stats: Vec<(String, String)> = if query.kind == "digest.weekly" {
        SAMPLE_STATS
            .iter()
            .map(|(key, value)| (i18n::t(&locale, key), (*value).to_string()))
            .collect()
    } else {
        Vec::new()
    };

    let html = email_template::render(email_template::Email {
        locale: &locale,
        theme: query.theme.as_deref(),
        title: &title,
        body: &body,
        recipient_name: Some("Awa"),
        stats: &stats,
        cta_label: cta_label.as_deref(),
        cta_url: cta_url.as_deref(),
        // Transactional mail carries no unsubscribe, and a preview that
        // showed one would hide the thing worth checking: that a receipt
        // does not offer to opt out of receipts.
        unsubscribe_url: if transactional {
            None
        } else {
            Some("https://example.test/unsubscribe/preview")
        },
    });

    Ok(Html(html))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PreviewIndex {
    /// Every kind that can be previewed, with its category.
    pub kinds: Vec<PreviewableKind>,
    pub locales: Vec<String>,
    pub themes: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PreviewableKind {
    pub kind: String,
    pub category: String,
    /// False when the catalogue forbids email for this kind. Listed anyway,
    /// because "why does this one have no preview" is a question worth
    /// answering in the list rather than by silence.
    pub sends_email: bool,
    /// Locales missing a translation for this kind. Empty is the goal; a
    /// non-empty list is a subject line that would render as its own key.
    pub untranslated: Vec<String>,
}

/// What can be previewed, and in which languages and worlds.
#[utoipa::path(
    get,
    path = "/api/admin/email-preview/index",
    tag = "admin",
    responses(
        (status = 200, description = "Previewable kinds, locales and themes", body = ApiResponse<PreviewIndex>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn index(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<PreviewIndex>>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    let rows: Vec<(String, String, bool)> =
        sqlx::query_as("SELECT kind, category, allows_email FROM notification_kinds ORDER BY kind")
            .fetch_all(&state.db)
            .await?;

    let locales = i18n::available();
    let kinds = rows
        .into_iter()
        .map(|(kind, category, sends_email)| {
            // Computed here rather than left to a test: an operator opening
            // this list is the person who can act on a missing translation,
            // and the list is where they will look.
            let untranslated = locales
                .iter()
                .filter(|locale| {
                    let key = format!("notification.{kind}.title");
                    i18n::t(locale, &key) == key
                })
                .map(|l| (*l).to_string())
                .collect();
            PreviewableKind {
                kind,
                category,
                sends_email,
                untranslated,
            }
        })
        .collect();

    Ok(Json(ApiResponse::new(PreviewIndex {
        kinds,
        locales: locales.into_iter().map(str::to_string).collect(),
        themes: email_theme::ALL
            .iter()
            .map(|t| t.name.to_string())
            .collect(),
    })))
}
