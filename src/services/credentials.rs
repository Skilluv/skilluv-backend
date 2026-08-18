//! Certifications somebody else issued, recorded here.
//!
//! AWS, Google, the CNCF, HashiCorp. Skilluv did not award any of these and
//! cannot take them back, which decides how the module behaves: nothing is
//! ever presented as verified until a person has opened the issuer's own
//! page and said so, and nothing lapsed is presented as current.
//!
//! ## What this module refuses to do
//!
//! Guess. There is no scraping of Credly, no inference of an expiry from a
//! programme name, and no "probably still valid". A credential is claimed
//! until a reviewer says otherwise, exactly like an unverified forge handle,
//! and the difference is visible on the profile rather than buried.
//!
//! ## Where the expiry notice fits
//!
//! Sent once, thirty days before, and only for credentials that were
//! verified — nagging somebody about a claim nobody checked would be asking
//! them to renew something the platform never counted.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::errors::AppError;

pub const ISSUERS: &[&str] = &[
    "aws",
    "google_cloud",
    "microsoft_azure",
    "cncf",
    "hashicorp",
    "red_hat",
    "oracle",
    "other",
];

pub const LEVELS: &[&str] = &["foundational", "associate", "professional", "specialty"];

/// How many days before expiry the holder is told.
///
/// Thirty rather than seven: re-sitting an AWS professional exam takes
/// booking a slot, and a notice that arrives after the last available slot is
/// a notice that only reports the loss.
pub const EXPIRY_NOTICE_DAYS: i64 = 30;

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct Credential {
    pub id: Uuid,
    pub issuer: String,
    pub name: String,
    pub level: String,
    pub credential_id: Option<String>,
    pub evidence_url: String,
    pub issued_on: NaiveDate,
    pub expires_on: Option<NaiveDate>,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_current: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CredentialInput {
    pub issuer: String,
    pub name: String,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub credential_id: Option<String>,
    pub evidence_url: String,
    pub issued_on: NaiveDate,
    #[serde(default)]
    pub expires_on: Option<NaiveDate>,
}

const SELECT: &str = r#"
    SELECT id, issuer, name, level, credential_id, evidence_url,
           issued_on, expires_on, verified_at, is_current
      FROM credentials_with_currency
"#;

/// Record a credential somebody holds.
///
/// Claimed on arrival, always. The person adding it is the person it belongs
/// to, which is precisely why their word is not enough.
pub async fn declare(
    db: &PgPool,
    user_id: Uuid,
    input: CredentialInput,
) -> Result<Credential, AppError> {
    if !ISSUERS.contains(&input.issuer.as_str()) {
        return Err(AppError::Validation(format!(
            "'{}' is not an issuer we record — use 'other' and name it in the \
             title if it is a smaller programme",
            input.issuer
        )));
    }
    let level = input.level.unwrap_or_else(|| "associate".into());
    if !LEVELS.contains(&level.as_str()) {
        return Err(AppError::Validation(format!(
            "'{level}' is not a level we record"
        )));
    }
    if input.name.trim().is_empty() {
        return Err(AppError::Validation(
            "name the certification as its issuer writes it".into(),
        ));
    }
    crate::validators::check_max_len(&input.name, "name", 160)?;
    if !input.evidence_url.starts_with("https://") {
        return Err(AppError::Validation(
            "a public https link to the credential page — a certification \
             nobody can open is a line on a CV"
                .into(),
        ));
    }
    if input.issued_on > chrono::Utc::now().date_naive() {
        return Err(AppError::Validation(
            "a certification cannot have been issued in the future".into(),
        ));
    }

    let sql = format!(
        "WITH inserted AS (
             INSERT INTO external_credentials
                 (user_id, issuer, name, level, credential_id, evidence_url,
                  issued_on, expires_on)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             RETURNING id
         )
         {SELECT} WHERE id = (SELECT id FROM inserted)"
    );
    let credential: Credential = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(user_id)
        .bind(&input.issuer)
        .bind(input.name.trim())
        .bind(&level)
        .bind(input.credential_id.as_deref())
        .bind(input.evidence_url.trim())
        .bind(input.issued_on)
        .bind(input.expires_on)
        .fetch_one(db)
        .await?;

    Ok(credential)
}

pub async fn for_user(db: &PgPool, user_id: Uuid) -> Result<Vec<Credential>, AppError> {
    let sql = format!("{SELECT} WHERE user_id = $1 ORDER BY is_current DESC, issued_on DESC");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(user_id)
        .fetch_all(db)
        .await
        .map_err(AppError::from)
}

/// What a reviewer has not yet looked at.
pub async fn awaiting_review(db: &PgPool, limit: i64) -> Result<Vec<serde_json::Value>, AppError> {
    sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'id', c.id, 'username', u.username,
                    'issuer', c.issuer, 'name', c.name, 'level', c.level,
                    'evidence_url', c.evidence_url,
                    'issued_on', c.issued_on, 'expires_on', c.expires_on,
                    'is_current', c.is_current)
           FROM credentials_with_currency c
           JOIN users u ON u.id = c.user_id
          WHERE c.verified_at IS NULL
          ORDER BY c.created_at
          LIMIT $1",
    )
    .bind(limit)
    .fetch_all(db)
    .await
    .map_err(AppError::from)
}

/// A reviewer opened the issuer's page and says the name on it matches.
///
/// The note is required and short answers are refused. "OK" is not a record
/// of a check; what was opened and what it said is.
pub async fn verify(
    db: &PgPool,
    credential_id: Uuid,
    reviewer: Uuid,
    note: &str,
) -> Result<(), AppError> {
    if note.trim().len() < 20 {
        return Err(AppError::Validation(
            "say what you opened and what it said — twenty characters at \
             least, because 'OK' is not a record of a check"
                .into(),
        ));
    }

    let affected = sqlx::query(
        "UPDATE external_credentials
            SET verified_by = $2, verified_at = NOW(), verification_note = $3
          WHERE id = $1 AND verified_at IS NULL",
    )
    .bind(credential_id)
    .bind(reviewer)
    .bind(note.trim())
    .execute(db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(
            "No credential waiting on a review under that id".into(),
        ));
    }

    metrics::counter!("skilluv_external_credentials_verified_total").increment(1);
    Ok(())
}

/// Refuse a claimed credential, and say why to the person who claimed it.
///
/// Deleted rather than flagged. A refused credential is one the platform has
/// no reason to keep — unlike a revoked attestation, which Skilluv issued and
/// therefore owes a public record of.
pub async fn refuse(db: &PgPool, credential_id: Uuid, reason: &str) -> Result<Uuid, AppError> {
    if reason.trim().len() < 20 {
        return Err(AppError::Validation(
            "a refusal says what was wrong and what would fix it".into(),
        ));
    }

    let owner: Option<Uuid> = sqlx::query_scalar(
        "DELETE FROM external_credentials
          WHERE id = $1 AND verified_at IS NULL
          RETURNING user_id",
    )
    .bind(credential_id)
    .fetch_optional(db)
    .await?;

    owner
        .ok_or_else(|| AppError::NotFound("No credential waiting on a review under that id".into()))
}

/// Verified credentials falling due, for the notice that goes out before they
/// lapse rather than after.
pub async fn expiring_soon(db: &PgPool) -> Result<Vec<(Uuid, String, NaiveDate)>, AppError> {
    sqlx::query_as(
        "SELECT user_id, name, expires_on
           FROM external_credentials
          WHERE verified_at IS NOT NULL
            AND expires_on IS NOT NULL
            AND expires_on = CURRENT_DATE + ($1 || ' days')::INTERVAL
          ORDER BY expires_on",
    )
    .bind(EXPIRY_NOTICE_DAYS.to_string())
    .fetch_all(db)
    .await
    .map_err(AppError::from)
}

/// Tell the holders of everything falling due today plus the notice window.
///
/// Run daily. Exactly one day of the window is selected rather than a range,
/// so a sweep that runs every day sends one notice per credential rather than
/// thirty. A day the process is down costs that day's notices — accepted,
/// because the alternative is a range and a `notified_at` column to keep it
/// from repeating, and the failure mode of the column is worse: a bug there
/// silences the notice permanently instead of once.
pub async fn notify_expiring(state: &crate::AppState) -> Result<usize, AppError> {
    let due = expiring_soon(&state.db).await?;
    let mut sent = 0usize;

    for (user_id, name, expires_on) in due {
        let ok = crate::services::notify::send(
            state,
            crate::services::notify::Recipient::User(user_id),
            "credential.expiring",
        )
        .arg("name", name)
        .arg("date", expires_on.to_string())
        .execute()
        .await;

        if ok.is_ok() {
            sent += 1;
        }
    }

    metrics::counter!("skilluv_credential_expiry_notices_total").increment(sent as u64);
    Ok(sent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vocabulary_is_the_one_the_column_allows() {
        // Both lists are restated in the CHECK constraints of migration
        // 0246. A value here the column refuses would fail at insert time,
        // which is the wrong place to find out.
        assert_eq!(ISSUERS.len(), 8);
        assert!(ISSUERS.contains(&"cncf"), "CKA and CKS live here");
        assert!(ISSUERS.contains(&"other"), "smaller programmes need a home");
        assert_eq!(LEVELS.len(), 4);
    }

    #[test]
    fn the_notice_arrives_early_enough_to_act_on() {
        // Seven days is not enough to book an exam slot, which would make
        // the notice a report of the loss rather than a warning.
        let days = EXPIRY_NOTICE_DAYS;
        assert!(days >= 30);
    }
}
