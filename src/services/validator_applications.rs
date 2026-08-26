//! P26 v2 Phase D — validator candidacy (SKI-81) and admin invitation (SKI-82).
//!
//! Two entry points to the `challenge_validator:{domain}` capability:
//!
//! - `apply` — user self-nominates for a domain; stats thresholds must be
//!   met (see `stats_ok`). Ends up as `pending` awaiting admin approval.
//! - `invite` — admin creates a `pending` invitation the user must accept.
//!
//! Neither path grants the capability directly. `accept` (called by admin
//! approval on candidacies, and by the invitee themselves on invitations)
//! inserts into `user_capabilities` and calls the P18 engine to keep the
//! rest of the system in sync.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub use crate::validators::SKILL_DOMAINS as VALID_DOMAINS;

/// Minimum thresholds a self-nominated candidate must meet on a domain.
pub const MIN_RANK: &str = "artisan";
pub const MIN_MERGED_PRS: i64 = 10;
pub const MIN_REPOS_COVERED: i64 = 3;
pub const MIN_TENURE_DAYS: i32 = 90;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ValidatorApplication {
    pub id: Uuid,
    pub user_id: Uuid,
    pub domain: String,
    pub origin: String,
    pub status: String,
    pub motivation: Option<String>,
    pub admin_actor_id: Option<Uuid>,
    pub reviewed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub review_notes: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ApplyInput {
    pub domain: String,
    pub motivation: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct InviteInput {
    pub user_id: Uuid,
    pub domain: String,
    pub notes: Option<String>,
}

fn validate_domain(domain: &str) -> Result<(), AppError> {
    crate::validators::validate_skill_domain(domain, "validator domain")
}

/// SKI-81 — self-nomination. Checks stats before inserting a `pending`
/// row. `Forbidden` if the user does not meet the bar.
pub async fn apply(
    db: &PgPool,
    user_id: Uuid,
    input: ApplyInput,
) -> Result<ValidatorApplication, AppError> {
    validate_domain(&input.domain)?;
    if !stats_ok(db, user_id, &input.domain).await? {
        return Err(AppError::Forbidden);
    }
    insert_row(
        db,
        user_id,
        &input.domain,
        "candidacy",
        input.motivation,
        None,
    )
    .await
}

/// SKI-82 — admin invitation. No stats gate. Requires the caller to hold
/// the `admin` capability (enforced in the route layer via AdminGate).
pub async fn invite(
    db: &PgPool,
    admin_actor_id: Uuid,
    input: InviteInput,
) -> Result<ValidatorApplication, AppError> {
    validate_domain(&input.domain)?;
    insert_row(
        db,
        input.user_id,
        &input.domain,
        "invitation",
        input.notes,
        Some(admin_actor_id),
    )
    .await
}

/// Accept a pending row. For candidacies, called by admin approval; for
/// invitations, called by the invitee. The route layer distinguishes.
pub async fn accept(
    db: &PgPool,
    application_id: Uuid,
    actor_user_id: Uuid,
    reviewer_admin_id: Option<Uuid>,
) -> Result<ValidatorApplication, AppError> {
    // Load the row so we know (domain, origin, applicant).
    let app: Option<ValidatorApplication> =
        sqlx::query_as("SELECT * FROM validator_applications WHERE id = $1")
            .bind(application_id)
            .fetch_optional(db)
            .await?;
    let app = app.ok_or_else(|| AppError::NotFound("Application not found".into()))?;

    if app.status != "pending" {
        return Err(AppError::Validation(format!(
            "Application is {} — cannot accept",
            app.status
        )));
    }

    // Authorisation:
    //   invitation → the invitee must be the caller
    //   candidacy  → an admin (reviewer_admin_id present) must accept
    match app.origin.as_str() {
        "invitation" => {
            if app.user_id != actor_user_id {
                return Err(AppError::Forbidden);
            }
        }
        "candidacy" => {
            if reviewer_admin_id.is_none() {
                return Err(AppError::Forbidden);
            }
        }
        _ => return Err(AppError::Internal("invalid origin in DB".into())),
    }

    // Grant the capability. `uniq_user_capabilities_active` is a partial
    // unique index on (user_id, capability) WHERE revoked_at IS NULL, so
    // the ON CONFLICT clause must reference the same predicate.
    let capability = format!("challenge_validator:{}", app.domain);
    sqlx::query(
        r#"
        INSERT INTO user_capabilities
            (user_id, capability, granted_by, granted_reason)
        VALUES ($1, $2, $3, 'validator_application:accepted')
        ON CONFLICT (user_id, capability)
            WHERE revoked_at IS NULL
        DO UPDATE
           SET granted_by = EXCLUDED.granted_by,
               granted_reason = EXCLUDED.granted_reason
        "#,
    )
    .bind(app.user_id)
    .bind(&capability)
    .bind(reviewer_admin_id.or(Some(actor_user_id)))
    .execute(db)
    .await?;

    // Keep the P18 engine's view coherent for anything derived from caps.
    let _ = crate::services::capabilities_engine::recompute_capabilities_for_user(db, app.user_id)
        .await;

    let updated: ValidatorApplication = sqlx::query_as(
        r#"
        UPDATE validator_applications
           SET status = 'accepted',
               admin_actor_id = COALESCE($2, admin_actor_id),
               reviewed_at = NOW(),
               updated_at = NOW()
         WHERE id = $1
     RETURNING *
        "#,
    )
    .bind(application_id)
    .bind(reviewer_admin_id)
    .fetch_one(db)
    .await?;
    Ok(updated)
}

/// Admin-only rejection of a candidacy, or self-withdrawal by the user.
pub async fn resolve(
    db: &PgPool,
    application_id: Uuid,
    actor_user_id: Uuid,
    new_status: &str,
    review_notes: Option<String>,
    is_admin: bool,
) -> Result<ValidatorApplication, AppError> {
    if !matches!(new_status, "rejected" | "withdrawn") {
        return Err(AppError::Validation(
            "new_status must be rejected or withdrawn".into(),
        ));
    }

    let app: ValidatorApplication =
        sqlx::query_as("SELECT * FROM validator_applications WHERE id = $1")
            .bind(application_id)
            .fetch_optional(db)
            .await?
            .ok_or_else(|| AppError::NotFound("Application not found".into()))?;

    if app.status != "pending" {
        return Err(AppError::Validation("Application is not pending".into()));
    }

    match new_status {
        "rejected" => {
            if !is_admin {
                return Err(AppError::Forbidden);
            }
        }
        "withdrawn" => {
            if app.user_id != actor_user_id {
                return Err(AppError::Forbidden);
            }
        }
        _ => unreachable!(),
    }

    let updated: ValidatorApplication = sqlx::query_as(
        r#"
        UPDATE validator_applications
           SET status = $2,
               review_notes = COALESCE($3, review_notes),
               admin_actor_id = COALESCE(admin_actor_id, $4),
               reviewed_at = NOW(),
               updated_at = NOW()
         WHERE id = $1
     RETURNING *
        "#,
    )
    .bind(application_id)
    .bind(new_status)
    .bind(review_notes)
    .bind(if is_admin { Some(actor_user_id) } else { None })
    .fetch_one(db)
    .await?;
    Ok(updated)
}

async fn insert_row(
    db: &PgPool,
    user_id: Uuid,
    domain: &str,
    origin: &str,
    text: Option<String>,
    admin_actor_id: Option<Uuid>,
) -> Result<ValidatorApplication, AppError> {
    let row: ValidatorApplication = sqlx::query_as(
        r#"
        INSERT INTO validator_applications
            (user_id, domain, origin, motivation, admin_actor_id)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(domain)
    .bind(origin)
    .bind(text)
    .bind(admin_actor_id)
    .fetch_one(db)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(dbe) = &e
            && dbe.constraint().is_some()
        {
            return AppError::Conflict(
                "You already have a pending application for this domain".into(),
            );
        }
        AppError::Database(e)
    })?;
    Ok(row)
}

/// Stats-check for SKI-81. Rank >= artisan, ≥10 merged PRs on the domain,
/// ≥3 distinct repos, ≥90 days tenure. Returns true when ALL are met.
pub async fn stats_ok(db: &PgPool, user_id: Uuid, domain: &str) -> Result<bool, AppError> {
    let rank_row: Option<(String,)> =
        sqlx::query_as("SELECT rank FROM user_ranks WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    let rank = rank_row.map(|(r,)| r).unwrap_or_else(|| "apprenti".into());
    if crate::services::slices::rank_ordinal_public(&rank)
        < crate::services::slices::rank_ordinal_public(MIN_RANK)
    {
        return Ok(false);
    }

    // Tenure: days since users.created_at
    let tenure: Option<(i32,)> = sqlx::query_as(
        "SELECT EXTRACT(DAY FROM (NOW() - created_at))::int FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    let Some((days,)) = tenure else {
        return Ok(false);
    };
    if days < MIN_TENURE_DAYS {
        return Ok(false);
    }

    // Merged PRs on the domain + distinct repos, counted via project_slices
    // that reached `validated` (Skilluv success) for slices whose
    // primary_domain matches. `merged` (upstream bonus) also counts.
    let (prs, repos): (i64, i64) = sqlx::query_as(
        r#"
        SELECT COUNT(*)::bigint,
               COUNT(DISTINCT project_id)::bigint
          FROM project_slices
         WHERE claimed_by_user_id = $1
           AND primary_domain = $2
           AND status IN ('validated', 'merged')
        "#,
    )
    .bind(user_id)
    .bind(domain)
    .fetch_one(db)
    .await?;
    Ok(prs >= MIN_MERGED_PRS && repos >= MIN_REPOS_COVERED)
}
