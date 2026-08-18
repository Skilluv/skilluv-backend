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
        .route(
            "/users/me/domain-profile/{domain}/questions",
            get(list_questions),
        )
}

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

/// Audio only. What the person wants to write *for*.
///
/// Not a skill domain, and the distinction matters: somebody scoring a podcast
/// practises audio, not podcasting. Recommending them a game jam because they
/// answered `podcast` would be the recommender reading a taxonomy it invented.
const AUDIO_DESTINATIONS: &[&str] = &["game", "motion", "podcast", "brand", "ui", "cross"];

/// Audio only. The stations somebody actually works in.
///
/// Asked because it is the single most useful thing to know when pairing a
/// beginner with a mentor: a session where one person cannot open the other's
/// project is an hour spent on file formats.
///
/// Plural for the same reason the AI frameworks are: a composer who writes in
/// Reaper and mixes in Ardour is one person, and forcing a choice would lose
/// half of what the matching needs.
const AUDIO_DAWS: &[&str] = &[
    "reaper",
    "ardour",
    "logic",
    "fl_studio",
    "ableton",
    "cubase",
    "pro_tools",
    "audacity",
    "other",
];

/// At most three. Somebody who selects everything has told us nothing while
/// believing they answered — the same cap the code wizard uses.
const MAX_SELECTIONS: usize = 3;

/// One question the wizard asks, and what it accepts as an answer.
///
/// A registry rather than a struct field per question. The wizard used to be a
/// typed body with an `Option<String>` per AI question and a comment saying
/// "AI only" above each; the second domain to need three questions of its own
/// would have made that struct a list of everybody's fields, each null for
/// everybody else — which is the shape migration 0306 removed from the `users`
/// table for exactly the same reason.
///
/// The wire format did not change: the body is still a flat object of the same
/// keys. What changed is that the keys a domain accepts are data, so the front
/// end can render the wizard from `GET .../questions` instead of shipping its
/// own copy of the list.
pub struct Question {
    pub key: &'static str,
    /// Several answers rather than one. Capped by [`MAX_SELECTIONS`].
    pub multi: bool,
    /// The closed vocabulary, or empty for free text — a username on somebody
    /// else's service, which we cannot enumerate.
    pub allowed: &'static [&'static str],
    /// Longest accepted free-text answer. Ignored when `allowed` is non-empty.
    pub max_len: usize,
}

const fn closed(key: &'static str, allowed: &'static [&'static str]) -> Question {
    Question { key, multi: false, allowed, max_len: 0 }
}

const fn closed_multi(key: &'static str, allowed: &'static [&'static str]) -> Question {
    Question { key, multi: true, allowed, max_len: 0 }
}

const fn free_text(key: &'static str, max_len: usize) -> Question {
    Question { key, multi: false, allowed: &[], max_len }
}

/// Asked in every domain.
const COMMON_QUESTIONS: &[Question] = &[
    closed("level", LEVELS),
    closed("weekly_hours", WEEKLY_HOURS),
    closed("goal", GOALS),
];

const AI_QUESTIONS: &[Question] = &[
    closed("compute", COMPUTE),
    closed_multi("main_frameworks", FRAMEWORKS),
    free_text("huggingface_username", 60),
];

const AUDIO_QUESTIONS: &[Question] = &[
    closed("audio_destination", AUDIO_DESTINATIONS),
    closed_multi("main_daws", AUDIO_DAWS),
    free_text("soundcloud_username", 60),
    free_text("bandcamp_username", 60),
];

/// What this domain asks, beyond the three everybody asks.
///
/// `preferred_families` is handled separately in every domain: its vocabulary
/// is a query against `orientations` rather than a constant.
pub fn questions_for(domain: &str) -> &'static [Question] {
    match domain {
        "ai" => AI_QUESTIONS,
        "audio" => AUDIO_QUESTIONS,
        _ => &[],
    }
}

/// The families a mentee wants to be matched in, per domain: reviewer groups,
/// the same ones the guides and the review capabilities use.
///
/// Read by the mentor matching, which is why an unknown one is refused rather
/// than stored: it would match nobody, silently, and look like an empty
/// platform.
async fn check_families(
    db: &sqlx::PgPool,
    domain: &str,
    families: &[String],
) -> Result<(), AppError> {
    if families.len() > MAX_SELECTIONS {
        return Err(AppError::Validation(format!(
            "at most {MAX_SELECTIONS} families — picking everything says nothing"
        )));
    }
    let known: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT reviewer_group FROM orientations
          WHERE reviewer_group IS NOT NULL AND primary_domain = $1",
    )
    .bind(domain)
    .fetch_all(db)
    .await?;

    for family in families {
        if !known.contains(family) {
            return Err(AppError::Validation(format!(
                "'{family}' is not a {domain} family — expected one of: {}",
                known.join(", ")
            )));
        }
    }
    Ok(())
}

/// The wizard's answers, as the flat object the wizard sends.
///
/// A map rather than a field per question. Which keys are accepted depends on
/// the domain and comes from [`questions_for`]; `GET .../questions` returns
/// the same list so a front end can render the form instead of hard-coding it.
///
/// `preferred_families` is accepted in every domain and validated against
/// `orientations` rather than against a constant.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct DomainProfileBody {
    #[schema(value_type = Object)]
    pub answers: serde_json::Map<String, serde_json::Value>,
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

/// Refuse an answer this question does not accept, naming what it does.
///
/// A closed question is checked against its vocabulary; a free-text one only
/// against its length. An unknown value is not stored as a curiosity: a
/// recommender that reads one has no branch for it and silently recommends
/// nothing, which looks like an empty platform rather than a bad answer.
fn check_answer(question: &Question, value: &str) -> Result<(), AppError> {
    if question.allowed.is_empty() {
        return crate::validators::check_max_len(value, question.key, question.max_len);
    }
    if !question.allowed.contains(&value) {
        return Err(AppError::Validation(format!(
            "{} must be one of: {}",
            question.key,
            question.allowed.join(", ")
        )));
    }
    Ok(())
}

/// Read a JSON array of strings, refusing anything else by name.
fn as_string_list(key: &str, value: &serde_json::Value) -> Result<Vec<String>, AppError> {
    let array = value
        .as_array()
        .ok_or_else(|| AppError::Validation(format!("'{key}' must be a list")))?;
    array
        .iter()
        .map(|v| {
            v.as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| AppError::Validation(format!("'{key}' must be a list of strings")))
        })
        .collect()
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

/// Turn the handles the wizard collected into linked portfolio rows.
///
/// Ticket O-01 asked the audio wizard to trigger the portfolio import when
/// somebody gives a SoundCloud or Bandcamp handle. Storing the handle and
/// doing nothing with it is the shape this codebase keeps removing: an answer
/// that reads as collected and is used by nothing.
///
/// The row is `figures_are_declared` with no counts, because a handle is not
/// an audience. What it gives a reader is a link they can follow, and what it
/// gives the person is one less form to fill in; the play counts come later,
/// from them, through `/api/audio/portfolios`.
///
/// Failures are logged and dropped. Somebody answering a wizard is not to be
/// shown an error because a convenience did not fire, and the endpoint that
/// does this properly is one click away.
async fn link_declared_handles(
    db: &sqlx::PgPool,
    user_id: uuid::Uuid,
    answers: &serde_json::Map<String, serde_json::Value>,
) {
    for (key, platform, base) in [
        (
            "soundcloud_username",
            "soundcloud",
            "https://soundcloud.com/",
        ),
        ("bandcamp_username", "bandcamp", "https://bandcamp.com/"),
    ] {
        let Some(handle) = answers.get(key).and_then(|v| v.as_str()) else {
            continue;
        };
        let handle = handle.trim();
        // A handle with a slash in it is a pasted URL rather than a name, and
        // concatenating it would produce a link that goes nowhere.
        if handle.is_empty() || handle.contains('/') {
            continue;
        }

        let outcome = sqlx::query(
            r#"
            INSERT INTO user_external_portfolios
                (user_id, platform, handle, profile_url, figures_are_declared, sync_enabled)
            VALUES ($1, $2, $3, $4, TRUE, FALSE)
            ON CONFLICT (user_id, platform, handle) DO UPDATE
                SET profile_url = EXCLUDED.profile_url, updated_at = NOW()
            "#,
        )
        .bind(user_id)
        .bind(platform)
        .bind(handle)
        .bind(format!("{base}{handle}"))
        .execute(db)
        .await;

        if let Err(e) = outcome {
            tracing::warn!(user = %user_id, platform, error = %e, "handle not linked");
        }
    }
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

    let asked = questions_for(&domain);

    // Only the answers actually given. A key present with a null value and an
    // absent key read the same to a recommender, and one of them is a lie
    // about having asked.
    let mut answers = serde_json::Map::new();

    for (key, value) in body.answers.iter() {
        if value.is_null() {
            continue;
        }

        // `preferred_families` is asked everywhere and checked against the
        // catalogue, so it is not in the per-domain list.
        if key == "preferred_families" {
            let families = as_string_list(key, value)?;
            check_families(&state.db, &domain, &families).await?;
            // Empty lists stay out, for the same reason an unanswered question
            // does: a key present with nothing in it and an absent key read
            // the same to a recommender, and one of them claims the question
            // was asked.
            if !families.is_empty() {
                answers.insert(key.clone(), json!(families));
            }
            continue;
        }

        let question = COMMON_QUESTIONS
            .iter()
            .chain(asked.iter())
            .find(|q| q.key == key)
            .ok_or_else(|| {
                let known: Vec<&str> = COMMON_QUESTIONS
                    .iter()
                    .chain(asked.iter())
                    .map(|q| q.key)
                    .chain(std::iter::once("preferred_families"))
                    .collect();
                AppError::Validation(format!(
                    "the {domain} wizard does not ask '{key}' — it asks: {}",
                    known.join(", ")
                ))
            })?;

        if question.multi {
            let values = as_string_list(key, value)?;
            if values.len() > MAX_SELECTIONS {
                return Err(AppError::Validation(format!(
                    "at most {MAX_SELECTIONS} answers to '{key}' — picking                      everything says nothing"
                )));
            }
            for v in &values {
                check_answer(question, v)?;
            }
            if !values.is_empty() {
                answers.insert(key.clone(), json!(values));
            }
        } else {
            let v = value.as_str().ok_or_else(|| {
                AppError::Validation(format!("'{key}' must be a string"))
            })?;
            check_answer(question, v)?;
            answers.insert(key.clone(), json!(v));
        }
    }

    let answers = serde_json::Value::Object(answers);

    if domain == "audio" {
        if let serde_json::Value::Object(map) = &answers {
            link_declared_handles(&state.db, auth.user_id, map).await;
        }
    }

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

#[derive(Debug, Serialize, ToSchema)]
pub struct QuestionSpec {
    pub key: String,
    /// `single`, `multi` or `text`.
    pub answer: String,
    /// The accepted values, empty for free text.
    pub allowed: Vec<String>,
    /// How many answers at most, for a multi-answer question.
    pub max_selections: Option<usize>,
    /// Longest accepted answer, for free text.
    pub max_len: Option<usize>,
}

/// What this domain's wizard asks.
///
/// Exists so a front end renders the form from the platform rather than from
/// its own copy of the list — the copy that goes stale the first time a domain
/// adds a question and nobody tells the web team.
///
/// `preferred_families` is included with its live vocabulary, read from the
/// catalogue: the families of a domain change when an operator edits an
/// orientation, so a constant would be wrong within a release.
#[utoipa::path(
    get, path = "/api/users/me/domain-profile/{domain}/questions", tag = "profile",
    params(("domain" = String, Path, description = "Domain slug")),
    responses(
        (status = 200, description = "Questions", body = ApiResponse<Vec<QuestionSpec>>),
        (status = 400, description = "Unknown domain", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_questions(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(domain): Path<String>,
) -> Result<Json<ApiResponse<Vec<QuestionSpec>>>, AppError> {
    check_domain(&domain)?;

    let mut specs: Vec<QuestionSpec> = COMMON_QUESTIONS
        .iter()
        .chain(questions_for(&domain).iter())
        .map(|q| QuestionSpec {
            key: q.key.to_string(),
            answer: if q.allowed.is_empty() {
                "text".into()
            } else if q.multi {
                "multi".into()
            } else {
                "single".into()
            },
            allowed: q.allowed.iter().map(|a| a.to_string()).collect(),
            max_selections: q.multi.then_some(MAX_SELECTIONS),
            max_len: q.allowed.is_empty().then_some(q.max_len),
        })
        .collect();

    let families: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT reviewer_group FROM orientations
          WHERE reviewer_group IS NOT NULL AND primary_domain = $1
          ORDER BY reviewer_group",
    )
    .bind(&domain)
    .fetch_all(&state.db)
    .await?;

    // Offered only where the domain has families to offer. A question with an
    // empty vocabulary is one nobody can answer, and showing it makes the
    // wizard look broken rather than short.
    if !families.is_empty() {
        specs.push(QuestionSpec {
            key: "preferred_families".into(),
            answer: "multi".into(),
            allowed: families,
            max_selections: Some(MAX_SELECTIONS),
            max_len: None,
        });
    }

    Ok(Json(ApiResponse::new(specs)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(domain: &str, key: &str) -> &'static Question {
        COMMON_QUESTIONS
            .iter()
            .chain(questions_for(domain).iter())
            .find(|q| q.key == key)
            .expect("question exists")
    }

    #[test]
    fn an_answer_outside_the_vocabulary_is_refused() {
        let level = question("ai", "level");
        assert!(check_answer(level, "Senior").is_err());
        // A trailing space is a different string, and a recommender reading it
        // has no branch for it.
        assert!(check_answer(level, "senior ").is_err());
        assert!(check_answer(level, "senior").is_ok());
    }

    #[test]
    fn free_text_is_bounded_and_not_enumerated() {
        let handle = question("audio", "soundcloud_username");
        assert!(check_answer(handle, "someone").is_ok());
        assert!(check_answer(handle, &"x".repeat(200)).is_err());
    }

    #[test]
    fn a_domain_only_accepts_the_questions_it_asks() {
        // `compute` is an AI question. Sending it to the audio wizard used to
        // be silently stored, because the body had a field for it whatever the
        // path said.
        assert!(questions_for("ai").iter().any(|q| q.key == "compute"));
        assert!(!questions_for("audio").iter().any(|q| q.key == "compute"));
        assert!(questions_for("audio").iter().any(|q| q.key == "main_daws"));
        assert!(questions_for("code").is_empty());
    }

    #[test]
    fn the_domains_are_the_ones_the_schema_accepts() {
        // Asserted against the table by `test_skill_domains`; this only checks
        // the handler refuses what the database would.
        assert!(check_domain("ai").is_ok());
        assert!(check_domain("audio").is_ok());
        assert!(check_domain("marketing").is_err());
    }

    #[test]
    fn a_list_of_something_else_is_refused_by_name() {
        assert!(as_string_list("main_frameworks", &json!(["jax"])).is_ok());
        assert!(as_string_list("main_frameworks", &json!("jax")).is_err());
        assert!(as_string_list("main_frameworks", &json!([1, 2])).is_err());
    }
}
