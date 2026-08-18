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

/// Domain-agnostic answers. Every domain asks these three.
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

/// At most three. Somebody who selects everything has told us nothing while
/// believing they answered — the same cap the code wizard uses.
const MAX_SELECTIONS: usize = 3;

/// Design only. Whether somebody wants to be given a brief alone or to enter
/// against other people decides which half of the catalogue is worth showing
/// them: a contest and an individual challenge are different weeks.
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
    /// The families to be matched in. Reviewer groups in code and AI,
    /// orientation slugs in design — the two domains ask the question at
    /// different granularity, and both are validated against the catalogue
    /// rather than a list here. At most three: an answer naming everything
    /// sorts nothing.
    pub preferred_families: Option<Vec<String>>,
    /// AI only, and plural: nobody uses exactly one. `pytorch`, `jax`,
    /// `tensorflow`, `candle`, `mlx`, `other`.
    pub main_frameworks: Option<Vec<String>>,
    /// AI only. A link, not an import: models count when they arrive as
    /// reviewed work.
    #[schema(max_length = 60)]
    pub huggingface_username: Option<String>,
    /// Design only: `individual`, `contest`, `both`, `undecided`.
    pub challenge_preference: Option<String>,
    /// Design only: `figma`, `adobe`, `sketch`, `blender`, `after_effects`,
    /// `other`.
    pub main_tool: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DomainProfileResponse {
    pub domain: String,
    #[schema(value_type = Object)]
    pub answers: serde_json::Value,
    /// When the wizard was answered. Absent means never.
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When somebody said stop asking. Different from having answered
    /// nothing: the first means "stop", the second means "ask again".
    pub skipped_at: Option<chrono::DateTime<chrono::Utc>>,
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
/// The families somebody wants to be matched in, validated against the
/// catalogue of the domain they answered for.
///
/// Two granularities, because the wizards genuinely differ: code and AI ask
/// for review families - the groups the guides and the reviewer capabilities
/// use - while design asks for the trade itself. Both are checked against live
/// rows rather than a list in this file, because a hard-coded copy of a
/// catalogue that arrives by migration goes stale without anybody noticing.
///
/// Unknown values are refused rather than stored. Stored, they would match
/// nobody, silently, and look like an empty platform.
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
            "preferred_families accepts at most {MAX_PREFERRED_FAMILIES} entries: \
             picking everything says nothing"
        )));
    }

    // Duplicates would weight one family twice in whatever reads this.
    let mut deduped: Vec<String> = Vec::with_capacity(families.len());
    for family in families {
        if !deduped.iter().any(|kept| kept == family) {
            deduped.push(family.clone());
        }
    }

    if deduped.is_empty() {
        return Ok(Some(deduped));
    }

    let known: Vec<String> = if domain == "design" {
        sqlx::query_scalar(
            "SELECT slug FROM orientations
              WHERE primary_domain = $1 AND is_archived = FALSE",
        )
        .bind(domain)
        .fetch_all(db)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT DISTINCT reviewer_group FROM orientations
              WHERE reviewer_group IS NOT NULL AND primary_domain = $1",
        )
        .bind(domain)
        .fetch_all(db)
        .await?
    };

    let unknown: Vec<&str> = deduped
        .iter()
        .filter(|value| !known.contains(value))
        .map(|value| value.as_str())
        .collect();

    if !unknown.is_empty() {
        return Err(AppError::Validation(format!(
            "preferred_families names nothing in {domain}: {}. Expected one of: {}",
            unknown.join(", "),
            known.join(", ")
        )));
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

    type Row = (
        serde_json::Value,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    );
    let row: Option<Row> = sqlx::query_as(
        "SELECT answers, completed_at, skipped_at
           FROM user_domain_profiles WHERE user_id = $1 AND domain = $2",
    )
    .bind(auth.user_id)
    .bind(&domain)
    .fetch_optional(&state.db)
    .await?;

    let (answers, completed_at, skipped_at) = row.unwrap_or((json!({}), None, None));

    Ok(Json(ApiResponse::new(DomainProfileResponse {
        domain,
        answers,
        completed_at,
        skipped_at,
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

    let level = checked("level", body.level.as_ref(), LEVELS)?;
    let weekly_hours = checked("weekly_hours", body.weekly_hours.as_ref(), WEEKLY_HOURS)?;
    let goal = checked("goal", body.goal.as_ref(), GOALS)?;

    // Domain-owned answers are refused where they mean nothing, before their
    // vocabulary is looked at: "compute belongs to ai" is the useful message,
    // not "compute must be one of ...".
    belongs_to("compute", body.compute.is_some(), "ai", &domain)?;
    belongs_to(
        "main_frameworks",
        body.main_frameworks.is_some(),
        "ai",
        &domain,
    )?;
    belongs_to(
        "huggingface_username",
        body.huggingface_username.is_some(),
        "ai",
        &domain,
    )?;
    belongs_to(
        "challenge_preference",
        body.challenge_preference.is_some(),
        "design",
        &domain,
    )?;
    belongs_to("main_tool", body.main_tool.is_some(), "design", &domain)?;

    let compute = checked("compute", body.compute.as_ref(), COMPUTE)?;
    let frameworks = body.main_frameworks.clone().unwrap_or_default();
    if frameworks.len() > MAX_SELECTIONS {
        return Err(AppError::Validation(format!(
            "at most {MAX_SELECTIONS} frameworks — picking everything says nothing"
        )));
    }
    for framework in &frameworks {
        checked("main_frameworks", Some(framework), FRAMEWORKS)?;
    }
    crate::validators::check_max_len_opt(&body.huggingface_username, "huggingface_username", 60)?;
    let challenge_preference = checked(
        "challenge_preference",
        body.challenge_preference.as_ref(),
        CHALLENGE_PREFERENCES,
    )?;
    let main_tool = checked("main_tool", body.main_tool.as_ref(), MAIN_TOOLS)?;
    let preferred_families =
        check_preferred_families(&state.db, &domain, body.preferred_families.as_deref()).await?;

    // Only the answers actually given. A key present with a null value and an
    // absent key read the same to a recommender, and one of them is a lie
    // about having asked.
    let mut answers = serde_json::Map::new();
    for (key, value) in [
        ("level", level),
        ("weekly_hours", weekly_hours),
        ("goal", goal),
        ("compute", compute),
        ("huggingface_username", body.huggingface_username.as_ref()),
        ("challenge_preference", challenge_preference),
        ("main_tool", main_tool),
    ] {
        if let Some(v) = value {
            answers.insert(key.to_string(), json!(v));
        }
    }
    // Empty lists stay out, for the same reason an unanswered question does:
    // a key present with nothing in it and an absent key read the same to a
    // recommender, and one of them claims the question was asked.
    if !frameworks.is_empty() {
        answers.insert("main_frameworks".to_string(), json!(frameworks));
    }
    // An empty list is kept when it was sent, unlike an absent key: somebody
    // clearing their families has answered the question, and dropping it here
    // would leave the previous answer standing.
    if let Some(families) = preferred_families {
        answers.insert("preferred_families".to_string(), json!(families));
    }
    let answers = serde_json::Value::Object(answers);

    sqlx::query(
        r#"
        INSERT INTO user_domain_profiles (user_id, domain, answers, completed_at)
        VALUES ($1, $2, $3, NOW())
        ON CONFLICT (user_id, domain) DO UPDATE
            SET answers      = EXCLUDED.answers,
                completed_at = NOW(),
                -- Answering is un-skipping: somebody who said "stop asking"
                -- and then answered has changed their mind.
                skipped_at   = NULL
        "#,
    )
    .bind(auth.user_id)
    .bind(&domain)
    .bind(&answers)
    .execute(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(DomainProfileResponse {
        domain,
        answers,
        completed_at: Some(chrono::Utc::now()),
        skipped_at: None,
    })))
}

/// Stop asking.
///
/// Recorded separately from "answered nothing". Without the distinction the
/// wizard reappears forever for exactly the people who least wanted it, and a
/// missing key cannot carry that difference — which is why 0235 keeps these
/// two as columns while the answers stay a blob.
#[utoipa::path(
    post, path = "/api/users/me/domain-profile/{domain}/skip", tag = "profile",
    params(("domain" = String, Path, description = "Domain slug")),
    responses(
        (status = 200, description = "Recorded", body = ApiResponse<DomainProfileResponse>),
        (status = 400, description = "Unknown domain", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn skip_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(domain): Path<String>,
) -> Result<Json<ApiResponse<DomainProfileResponse>>, AppError> {
    check_domain(&domain)?;

    sqlx::query(
        r#"
        INSERT INTO user_domain_profiles (user_id, domain, skipped_at)
        VALUES ($1, $2, NOW())
        ON CONFLICT (user_id, domain) DO UPDATE SET skipped_at = NOW()
        "#,
    )
    .bind(auth.user_id)
    .bind(&domain)
    .execute(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(DomainProfileResponse {
        domain,
        answers: json!({}),
        completed_at: None,
        skipped_at: Some(chrono::Utc::now()),
    })))
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
