//! The domain wizard: five questions whose answers shape what gets
//! recommended.
//!
//! ## Why the vocabulary is here and not in a CHECK
//!
//! Every answer is closed — a level is one of five values, not free text —
//! and refusing an unknown one matters, because a recommender reading
//! `"senior "` with a trailing space silently recommends nothing. But the
//! list changes as the wizard is reworded, and a CHECK would make each
//! rewording a migration. So the table stores an object and this module owns
//! the words.
//!
//! ## What this is not
//!
//! Not a claim about anybody. Declared level and declared framework are used
//! to sort what to show, never to credit: rank, badges and craft score read
//! proofs, and nothing here is one.
//!
//! ## Why a field belongs to a domain
//!
//! `compute` means something for AI and nothing for design; `main_tool` is
//! the reverse. Until now every field was checked against its vocabulary
//! whatever domain was in the path, so `PUT /domain-profile/design` with a
//! `compute` answer was stored happily and read by nobody — a wizard bug that
//! looked like a working save. A field now names the domain it belongs to,
//! and arriving on another one is a 400 that says which.
//!
//! ## Why the vocabulary is per domain
//!
//! Code asks for a level in `beginner..staff` and design in
//! `debutant..researcher`; code asks what languages you write and design what
//! tools you draw in. These are the same six questions asked of different
//! trades, not six different questions — so one endpoint, and a vocabulary
//! that is looked up by domain.
//!
//! Flattening the two ladders into one would have meant inventing a word for
//! a rank neither wizard asks about.
//!
//! ## HuggingFace
//!
//! The wizard collects a HuggingFace username, and does not import that
//! account's models. Importing them would put artefacts on a profile with no
//! verified deliverable behind them — a list somebody typed, which is exactly
//! what the charter refuses. The username is a link a reader can follow, and
//! a model counts here when it arrives as work that was reviewed.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn domain_profile_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/users/me/domain-profile/{domain}",
            get(get_profile).put(put_profile),
        )
        .route(
            "/users/me/domain-profile/{domain}/skip",
            axum::routing::post(skip_profile),
        )
}

/// The domains a profile can be filled in for — the platform's list, not a
/// copy of it. The CHECK on `user_domain_profiles.domain` carries the same
/// seven; drifting apart means a request refused by the database as a 500
/// instead of by this handler as a 400.
use crate::validators::SKILL_DOMAINS as DOMAINS;

/// Every domain asks these three, in its own words.
const LEVELS: &[&str] = &[
    "debutant",
    "apprentissage",
    "practitioner",
    "senior",
    "researcher",
];
const WEEKLY_HOURS: &[&str] = &["lt3", "3_10", "gt10", "fulltime"];
const GOALS: &[&str] = &[
    "learning",
    "portfolio",
    "paid_missions",
    "academic_research",
    "startup",
];

/// The code wizard's own ladder. Kept as it was rather than mapped onto the
/// five above: `staff` is a rank the design ladder does not have a word for,
/// and inventing one to make a single list would lose the distinction the
/// question exists to draw.
const CODE_LEVELS: &[&str] = &["beginner", "junior", "mid", "senior", "staff"];
const CODE_WEEKLY_HOURS: &[&str] = &["under_5", "5_to_15", "15_to_40", "fulltime"];
const CODE_GOALS: &[&str] = &[
    "learn",
    "build_portfolio",
    "find_paid_work",
    "contribute_upstream",
    "publish_library",
    "become_mentor",
    "ship_own_product",
];
const CODE_CHALLENGE_PREFERENCES: &[&str] = &[
    "upstream_contributions",
    "solo_shipped_apps",
    "published_libraries",
    "long_team_projects",
    "short_hackathons",
];

fn levels_for(domain: &str) -> &'static [&'static str] {
    if domain == "code" { CODE_LEVELS } else { LEVELS }
}

fn weekly_hours_for(domain: &str) -> &'static [&'static str] {
    if domain == "code" {
        CODE_WEEKLY_HOURS
    } else {
        WEEKLY_HOURS
    }
}

fn goals_for(domain: &str) -> &'static [&'static str] {
    if domain == "code" { CODE_GOALS } else { GOALS }
}

fn challenge_preferences_for(domain: &str) -> &'static [&'static str] {
    if domain == "code" {
        CODE_CHALLENGE_PREFERENCES
    } else {
        CHALLENGE_PREFERENCES
    }
}

/// AI only. What somebody can actually run decides which challenges are
/// worth showing them — recommending a seventy-billion-parameter fine-tune to
/// someone on free Colab wastes their week.
const COMPUTE: &[&str] = &[
    "none",         // free Colab or Kaggle, interrupted sessions
    "personal_gpu", // a card of their own
    "cloud_small",  // under 500 $ a month
    "cloud_large",  // over
    "enterprise",   // an employer's cluster
];
const FRAMEWORKS: &[&str] = &["pytorch", "jax", "tensorflow", "candle", "mlx", "other"];

/// Design's answers. Whether somebody wants to be given a brief alone or to
/// enter against other people decides which half of the catalogue is worth
/// showing them: a contest and an individual challenge are different weeks.
/// Code asks the same question with its own five answers.
const CHALLENGE_PREFERENCES: &[&str] = &["individual", "contest", "both", "undecided"];

/// Design only, and the analogue of `main_framework`. It sorts what gets
/// shown — somebody who works in Blender is not helped by a Figma auto-layout
/// brief — and it is a declaration, never a credential.
const MAIN_TOOLS: &[&str] = &[
    "figma",
    "adobe",
    "sketch",
    "blender",
    "after_effects",
    "other",
];

/// How many families somebody may say they are drawn to.
///
/// Three, the same cap `user_orientations` puts on declared trades. A wizard
/// answer that listed all thirteen would sort nothing, which is the failure
/// mode of asking a question whose answer can be "everything".
const MAX_PREFERRED_FAMILIES: usize = 3;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DomainProfileBody {
    pub level: Option<String>,
    pub weekly_hours: Option<String>,
    pub goal: Option<String>,
    /// AI only: `none`, `personal_gpu`, `cloud_small`, `cloud_large`,
    /// `enterprise`.
    pub compute: Option<String>,
    /// AI only: `pytorch`, `jax`, `tensorflow`, `candle`, `mlx`, `other`.
    pub main_framework: Option<String>,
    /// AI only. A link, not an import: models count when they arrive as
    /// reviewed work.
    #[schema(max_length = 60)]
    pub huggingface_username: Option<String>,
    /// Up to three of the families or trades the person is drawn to.
    /// Validated against the catalogue for the domain in the path, because a
    /// slug that names no trade recommends nothing.
    pub preferred_families: Option<Vec<String>>,
    /// Design: `individual`, `contest`, `both`, `undecided`. Code has its own
    /// five. Both answer the same question.
    pub challenge_preference: Option<String>,
    /// Design only, and one value: `figma`, `adobe`, `sketch`, `blender`,
    /// `after_effects`, `other`.
    pub main_tool: Option<String>,
    /// What somebody works in — languages for code, and up to three. Stored
    /// under the same key as `main_tool` for a reader that does not care
    /// which word the wizard used.
    pub main_tools: Option<Vec<String>>,
    /// Optional, code only. A GitHub handle here means "import what I already
    /// have", and waiting for the weekly sweep would make the wizard look
    /// inert. Claimed, never verified: typing a name proves nothing.
    #[schema(max_length = 60)]
    pub github_username: Option<String>,
    /// Optional, design only. A portfolio URL, recorded unconfirmed — the
    /// backend does not fetch arbitrary user-supplied URLs, and a moderator
    /// confirms it before it counts as evidence.
    #[schema(max_length = 500)]
    pub portfolio_url: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DomainProfileResponse {
    pub domain: String,
    #[schema(value_type = Object)]
    pub answers: serde_json::Value,
    /// What to do first, given what was just said. Present on a save,
    /// absent on a read: it is the answer to the wizard, not a property of
    /// the profile, and recomputing it on every GET would let it drift from
    /// the words the person actually saw.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<crate::services::onboarding_recommendation::Recommendation>,
}

/// Refuse an answer outside the vocabulary, naming what was allowed.
///
/// An unknown value is not stored as a curiosity: a recommender that reads
/// one has no branch for it and silently recommends nothing, which looks like
/// an empty platform rather than a bad answer.
fn checked<'a>(
    field: &str,
    value: Option<&'a String>,
    allowed: &[&str],
) -> Result<Option<&'a String>, AppError> {
    match value {
        Some(v) if !allowed.contains(&v.as_str()) => Err(AppError::Validation(format!(
            "{field} must be one of: {}",
            allowed.join(", ")
        ))),
        other => Ok(other),
    }
}

/// Refuse a field that belongs to another domain.
///
/// The alternative — storing it — is worse than it looks: the object is read
/// by whichever recommender the domain has, and nobody's design recommender
/// reads `compute`. The answer would sit there looking saved.
fn belongs_to(field: &str, present: bool, owner: &str, domain: &str) -> Result<(), AppError> {
    if present && domain != owner {
        return Err(AppError::Validation(format!(
            "{field} belongs to the {owner} domain, not to {domain}"
        )));
    }
    Ok(())
}

/// Validate the declared families against the catalogue.
///
/// Checked in the database rather than against a list in this file because
/// the twenty-six design trades are seeded by migration and grow by
/// migration; a hard-coded copy would go stale the first time one is added,
/// and go stale silently — the answer would be refused with no way to tell
/// why from the code.
///
/// An empty array is a real answer: "none in particular", which a wizard
/// distinguishes from an unanswered question and a recommender should too.
async fn check_preferred_families(
    db: &sqlx::PgPool,
    domain: &str,
    families: Option<&[String]>,
) -> Result<Option<Vec<String>>, AppError> {
    let Some(families) = families else {
        return Ok(None);
    };

    if families.len() > MAX_PREFERRED_FAMILIES {
        return Err(AppError::Validation(format!(
            "preferred_families accepts at most {MAX_PREFERRED_FAMILIES} entries"
        )));
    }

    // Duplicates would weight one family twice in whatever reads this.
    let mut deduped: Vec<String> = Vec::with_capacity(families.len());
    for family in families {
        if !deduped.iter().any(|kept| kept == family) {
            deduped.push(family.clone());
        }
    }

    if !deduped.is_empty() {
        // Two shapes, both real: design names trades by slug, code names
        // families by reviewer group. Both are seeded by migration and grow
        // by migration, so both are checked against the catalogue rather than
        // against a list in this file that would go stale silently.
        let known: Vec<String> = sqlx::query_scalar(
            "SELECT slug FROM orientations
              WHERE slug = ANY($1) AND primary_domain = $2 AND is_archived = FALSE
             UNION
             SELECT DISTINCT reviewer_group FROM orientations
              WHERE reviewer_group = ANY($1) AND primary_domain = $2
                AND is_archived = FALSE",
        )
        .bind(&deduped)
        .bind(domain)
        .fetch_all(db)
        .await?;

        let unknown: Vec<&str> = deduped
            .iter()
            .filter(|slug| !known.contains(slug))
            .map(|slug| slug.as_str())
            .collect();
        if !unknown.is_empty() {
            return Err(AppError::Validation(format!(
                "preferred_families names no live {domain} trade or family: {}",
                unknown.join(", ")
            )));
        }
    }

    Ok(Some(deduped))
}

fn check_domain(domain: &str) -> Result<(), AppError> {
    if !DOMAINS.contains(&domain) {
        return Err(AppError::Validation(format!(
            "unknown domain '{domain}': one of {}",
            DOMAINS.join(", ")
        )));
    }
    Ok(())
}

/// The caller's answers for one domain. Empty object when never filled in.
#[utoipa::path(
    get, path = "/api/users/me/domain-profile/{domain}", tag = "profile",
    params(("domain" = String, Path, description = "Domain slug")),
    responses(
        (status = 200, description = "Answers", body = ApiResponse<DomainProfileResponse>),
        (status = 400, description = "Unknown domain", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn get_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(domain): Path<String>,
) -> Result<Json<ApiResponse<DomainProfileResponse>>, AppError> {
    check_domain(&domain)?;

    let answers: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT answers FROM user_domain_profiles WHERE user_id = $1 AND domain = $2",
    )
    .bind(auth.user_id)
    .bind(&domain)
    .fetch_optional(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(DomainProfileResponse {
        domain,
        answers: answers.unwrap_or_else(|| json!({})),
        recommendation: None,
    })))
}

/// Save the wizard's answers.
///
/// Replaces the whole object rather than merging. A wizard sends every
/// question it asked, and merging would keep an answer the person has just
/// cleared — which is how somebody who lost access to a GPU keeps being shown
/// challenges they can no longer run.
#[utoipa::path(
    put, path = "/api/users/me/domain-profile/{domain}", tag = "profile",
    params(("domain" = String, Path, description = "Domain slug")),
    request_body = DomainProfileBody,
    responses(
        (status = 200, description = "Saved", body = ApiResponse<DomainProfileResponse>),
        (status = 400, description = "Unknown domain or answer", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn put_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(domain): Path<String>,
    Json(body): Json<DomainProfileBody>,
) -> Result<Json<ApiResponse<DomainProfileResponse>>, AppError> {
    check_domain(&domain)?;

    let level = checked("level", body.level.as_ref(), levels_for(&domain))?;
    let weekly_hours = checked(
        "weekly_hours",
        body.weekly_hours.as_ref(),
        weekly_hours_for(&domain),
    )?;
    let goal = checked("goal", body.goal.as_ref(), goals_for(&domain))?;

    // Domain-owned answers are refused where they mean nothing, before their
    // vocabulary is looked at: "compute belongs to ai" is the useful message,
    // not "compute must be one of ...".
    belongs_to("compute", body.compute.is_some(), "ai", &domain)?;
    belongs_to("main_framework", body.main_framework.is_some(), "ai", &domain)?;
    belongs_to(
        "huggingface_username",
        body.huggingface_username.is_some(),
        "ai",
        &domain,
    )?;
    belongs_to("main_tool", body.main_tool.is_some(), "design", &domain)?;
    belongs_to(
        "github_username",
        body.github_username.is_some(),
        "code",
        &domain,
    )?;
    belongs_to(
        "portfolio_url",
        body.portfolio_url.is_some(),
        "design",
        &domain,
    )?;

    let compute = checked("compute", body.compute.as_ref(), COMPUTE)?;
    let framework = checked("main_framework", body.main_framework.as_ref(), FRAMEWORKS)?;
    crate::validators::check_max_len_opt(&body.huggingface_username, "huggingface_username", 60)?;
    let challenge_preference = checked(
        "challenge_preference",
        body.challenge_preference.as_ref(),
        challenge_preferences_for(&domain),
    )?;
    let main_tool = checked("main_tool", body.main_tool.as_ref(), MAIN_TOOLS)?;
    let preferred_families =
        check_preferred_families(&state.db, &domain, body.preferred_families.as_deref()).await?;
    crate::validators::check_max_len_opt(&body.github_username, "github_username", 60)?;
    crate::validators::check_max_len_opt(&body.portfolio_url, "portfolio_url", 500)?;

    // Free text, so capped in count and in length. Three, like the families:
    // an answer that lists everything sorts nothing.
    let main_tools = match body.main_tools.as_deref() {
        Some(tools) if tools.len() > MAX_PREFERRED_FAMILIES => {
            return Err(AppError::Validation(format!(
                "main_tools accepts at most {MAX_PREFERRED_FAMILIES} entries"
            )));
        }
        Some(tools) if tools.iter().any(|tool| tool.len() > 40) => {
            return Err(AppError::Validation(
                "each entry in main_tools is at most 40 characters".into(),
            ));
        }
        other => other,
    };

    // Only the answers actually given. A key present with a null value and an
    // absent key read the same to a recommender, and one of them is a lie
    // about having asked.
    let mut answers = serde_json::Map::new();
    for (key, value) in [
        ("level", level),
        ("weekly_hours", weekly_hours),
        ("goal", goal),
        ("compute", compute),
        ("main_framework", framework),
        ("huggingface_username", body.huggingface_username.as_ref()),
        ("challenge_preference", challenge_preference),
        ("main_tool", main_tool),
        ("github_username", body.github_username.as_ref()),
        ("portfolio_url", body.portfolio_url.as_ref()),
    ] {
        if let Some(v) = value {
            answers.insert(key.to_string(), json!(v));
        }
    }
    if let Some(families) = preferred_families {
        answers.insert("preferred_families".to_string(), json!(families));
    }
    if let Some(tools) = main_tools {
        answers.insert("main_tools".to_string(), json!(tools));
    }
    let answers = serde_json::Value::Object(answers);

    // `completed_at` is written here. The column has existed since the table
    // did and nothing set it, so "has this person done the wizard" had no
    // answer — which is also why the skip below had nowhere to record itself.
    sqlx::query(
        r#"
        INSERT INTO user_domain_profiles (user_id, domain, answers, completed_at)
        VALUES ($1, $2, $3, NOW())
        ON CONFLICT (user_id, domain) DO UPDATE
            SET answers = EXCLUDED.answers,
                completed_at = NOW(),
                -- Answering after skipping is no longer skipping.
                skipped_at = NULL
        "#,
    )
    .bind(auth.user_id)
    .bind(&domain)
    .bind(&answers)
    .execute(&state.db)
    .await?;

    // A handle given here means "import what I already have", and waiting for
    // the next weekly sweep would make the wizard look inert. Both are
    // best-effort and neither is verified — typing a name proves nothing, and
    // connecting an account is a separate, deliberate act.
    if let Some(username) = body.github_username.as_deref().map(str::trim)
        && !username.is_empty()
        && let Err(err) = crate::services::code_portfolio::claim(
            &state.db,
            auth.user_id,
            &format!("https://github.com/{username}"),
        )
        .await
    {
        tracing::info!(%err, "github username from the wizard not recorded");
    }
    if let Some(url) = body.portfolio_url.as_deref().map(str::trim)
        && !url.is_empty()
    {
        claim_portfolio_signal(&state.db, auth.user_id, url).await;
    }

    let recommendation =
        crate::services::onboarding_recommendation::recommend(&domain, &answers);

    Ok(Json(ApiResponse::new(DomainProfileResponse {
        domain,
        answers,
        recommendation: Some(recommendation),
    })))
}

/// Record a declared portfolio, unconfirmed.
///
/// `external_signals` is where portfolios on platforms Skilluv does not own
/// live, and a row without `verified_at` is exactly what an unconfirmed one
/// is: visible to a moderator, invisible to a recruiter search. The backend
/// does not fetch the URL — fetching arbitrary user-supplied addresses is how
/// a server becomes somebody's proxy.
async fn claim_portfolio_signal(db: &sqlx::PgPool, user_id: uuid::Uuid, url: &str) {
    // The provider is read off the host rather than asked for: somebody
    // pasting a Behance link has already said which platform it is.
    let provider = if url.contains("behance.net") {
        "behance"
    } else if url.contains("dribbble.com") {
        "dribbble"
    } else if url.contains("artstation.com") {
        "artstation"
    } else if url.contains("vimeo.com") {
        "vimeo"
    } else {
        // A personal site is not one of the known platforms, and inventing a
        // provider for it would put it in a filter it does not belong in.
        return;
    };

    let result = sqlx::query(
        "INSERT INTO external_signals (user_id, provider, url, title)
         VALUES ($1, $2, $3, 'Portfolio')
         ON CONFLICT (user_id, url) DO NOTHING",
    )
    .bind(user_id)
    .bind(provider)
    .bind(url)
    .execute(db)
    .await;

    if let Err(err) = result {
        tracing::info!(%err, "portfolio url from the wizard not recorded");
    }
}

/// Skip the wizard.
///
/// Recorded rather than ignored. Somebody who declined is not somebody who
/// has not got round to it, and asking them again every week is how a prompt
/// becomes noise.
#[utoipa::path(
    post, path = "/api/users/me/domain-profile/{domain}/skip", tag = "profile",
    params(("domain" = String, Path, description = "Domain slug")),
    responses(
        (status = 204, description = "Skipped"),
        (status = 400, description = "Unknown domain", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn skip_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(domain): Path<String>,
) -> Result<axum::http::StatusCode, AppError> {
    check_domain(&domain)?;

    sqlx::query(
        r#"
        INSERT INTO user_domain_profiles (user_id, domain, answers, skipped_at)
        VALUES ($1, $2, '{}'::jsonb, NOW())
        ON CONFLICT (user_id, domain) DO UPDATE SET skipped_at = NOW()
        "#,
    )
    .bind(auth.user_id)
    .bind(&domain)
    .execute(&state.db)
    .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_outside_the_vocabulary_is_refused() {
        let bad = "Senior".to_string();
        assert!(checked("level", Some(&bad), LEVELS).is_err());
        let padded = "senior ".to_string();
        assert!(checked("level", Some(&padded), LEVELS).is_err());
    }

    #[test]
    fn an_unanswered_question_is_allowed() {
        // The wizard can be abandoned halfway, and half an answer is worth
        // more than none.
        assert!(checked("level", None, LEVELS).is_ok());
    }

    #[test]
    fn the_domains_are_the_ones_the_schema_accepts() {
        // The CHECK on `user_domain_profiles.domain` carries the same seven.
        // Drifting apart means a request refused by the database as a 500
        // instead of by this handler as a 400.
        assert_eq!(DOMAINS.len(), 7);
        assert!(check_domain("ai").is_ok());
        assert!(check_domain("marketing").is_err());
    }
}
