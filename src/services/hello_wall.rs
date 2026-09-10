//! The wall of HELLOs, for the eleven rites that are not a pull request.
//!
//! The code rite already has one and it is GitHub: `hello_wall_entries`
//! mirrors a HELLO.md into a public repository, and three of its columns are
//! NOT NULL with CHECK constraints demanding a github.com URL and a pull
//! request. A designer cannot have a row in it, and making those columns
//! nullable would turn two constraints into decoration. It stays what it is.
//!
//! So this is a read rather than a table. The rows already exist: a rite
//! submission writes a `deliverables` row with the challenge attached, and
//! the artefact it carries is referenced in `artifact_metadata.attachments`
//! as `design_upload:<uuid>`.
//!
//! ## Why publication is decided here and nowhere else
//!
//! A `design_upload_sessions` row is private. `design_uploads::load_for_reader`
//! lets its owner read it, and a reviewer only while the review task is open
//! or claimed, so an upload stops being readable once the verdict is in.
//!
//! This module does not widen that. It signs a short lived URL itself, for
//! rows that pass three tests at once, and the three are the whole of the
//! publication rule:
//!
//!   1. the deliverable answers a published domain rite,
//!   2. it is public and verified and not revoked,
//!   3. its author has not hidden their profile.
//!
//! `public` alone would not do. `services::deliverables` sets it TRUE on
//! every challenge submission, so it says nothing about intent; it is read
//! here beside the other two rather than trusted on its own. And the rite
//! copy tells somebody this will happen before they upload (migration 0627),
//! because a file handed to a platform that promised privacy is not a file
//! somebody agreed to publish.

use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::storage::StorageService;

/// How long a wall URL lives.
///
/// Short, and re-signed on every render. The alternative, a public bucket
/// path, cannot be withdrawn: somebody who deletes their account would leave
/// a URL that keeps working for whoever wrote it down.
const URL_TTL_SECONDS: u32 = 15 * 60;

/// One person's HELLO, as the wall shows it.
#[derive(Debug, Serialize, ToSchema)]
pub struct WallEntry {
    pub deliverable_id: Uuid,
    pub username: String,
    pub display_name: Option<String>,
    /// The trade they declared, when they declared one.
    pub trade: Option<String>,
    /// What the artefact is: `interface`, `motion`, `three_d_scene` and the
    /// eight others. The wall needs it to decide what element to render.
    pub subtype: Option<String>,
    /// The text they wrote beside the artefact.
    pub said: Option<String>,
    /// A signed URL for the thing to show.
    ///
    /// `None` where there is nothing to show, which is honest rather than
    /// broken: somebody whose HELLO is a link pasted as text has no upload,
    /// and a wall that invented a placeholder for them would be showing
    /// something they did not make.
    pub preview_url: Option<String>,
    /// TRUE when `preview_url` points at the author's own preview rather than
    /// at the source file, which is the case for the four subtypes a browser
    /// cannot open.
    pub is_preview: bool,
    pub shown_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(sqlx::FromRow)]
struct Row {
    deliverable_id: Uuid,
    username: String,
    display_name: Option<String>,
    trade: Option<String>,
    said: Option<String>,
    attachments: Option<serde_json::Value>,
    shown_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// The wall for one domain, newest first.
pub async fn entries(
    db: &PgPool,
    storage: &StorageService,
    domain: &str,
    limit: i64,
) -> Result<Vec<WallEntry>, AppError> {
    entries_where(db, storage, domain, None, limit).await
}

/// The wall, optionally narrowed to one person.
///
/// One query rather than two, so the publication rule cannot be written twice
/// and drift. `$3` is NULL for the wall and a user id for a profile.
async fn entries_where(
    db: &PgPool,
    storage: &StorageService,
    domain: &str,
    only: Option<Uuid>,
    limit: i64,
) -> Result<Vec<WallEntry>, AppError> {
    let rows: Vec<Row> = sqlx::query_as(
        r#"
        SELECT d.id            AS deliverable_id,
               u.username,
               u.display_name,
               o.name          AS trade,
               d.artifact_metadata ->> 'code_content' AS said,
               d.artifact_metadata -> 'attachments'   AS attachments,
               d.verified_at   AS shown_at
          FROM deliverables d
          JOIN challenge_templates ct ON ct.id = d.challenge_id
          JOIN users u ON u.id = d.user_id
          LEFT JOIN LATERAL (
              SELECT o2.name
                FROM user_orientations uo
                JOIN orientations o2 ON o2.id = uo.orientation_id
               WHERE uo.user_id = d.user_id
                 AND uo.ended_at IS NULL
                 AND uo.mode = 'active'
               ORDER BY uo.is_primary DESC
               LIMIT 1
          ) o ON TRUE
         WHERE ct.is_domain_rite = TRUE
           AND ct.status = 'published'
           AND ct.skill_domain = $1
           AND d.public = TRUE
           AND d.verification_status = 'verified'
           AND d.revoked_at IS NULL
           AND u.profile_hidden = FALSE
           AND ($3::UUID IS NULL OR d.user_id = $3)
         ORDER BY d.verified_at DESC NULLS LAST
         LIMIT $2
        "#,
    )
    .bind(domain)
    .bind(limit)
    .bind(only)
    .fetch_all(db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let (subtype, preview_url, is_preview) = resolve_artefact(db, storage, &row).await;
        out.push(WallEntry {
            deliverable_id: row.deliverable_id,
            username: row.username,
            display_name: row.display_name,
            trade: row.trade,
            subtype,
            said: row.said,
            preview_url,
            is_preview,
            shown_at: row.shown_at,
        });
    }
    Ok(out)
}

/// One person's HELLO, for their own profile page.
///
/// The same three tests as the wall, asked about one person. Their profile is
/// where somebody looks for their own work, and the rite artefact was
/// invisible there too: the design profile joins `project_slices`, which a
/// rite deliverable does not have, and filters `artifact_type` to
/// `design_artifact`, which a challenge submission is not. It was excluded
/// twice over, on the one page whose whole job is to show what you have done.
///
/// `None` for somebody who has not passed it, or whose verdict has not come,
/// or who has hidden their profile. The last is not redundant: the caller may
/// be an admin who can see a hidden profile, and the HELLO is published
/// rather than merely visible.
pub async fn for_user(
    db: &PgPool,
    storage: &StorageService,
    user_id: Uuid,
    domain: &str,
) -> Result<Option<WallEntry>, AppError> {
    let mut found = entries_where(db, storage, domain, Some(user_id), 1).await?;
    Ok(found.pop())
}

/// The first upload attached to a HELLO, resolved to something showable.
///
/// Best effort on purpose: a signing failure or a half finished upload makes
/// one entry text only, rather than making the wall a 500. The wall is a page
/// of many people, and one of them having a bad row is not a reason to hide
/// the others.
async fn resolve_artefact(
    db: &PgPool,
    storage: &StorageService,
    row: &Row,
) -> (Option<String>, Option<String>, bool) {
    let Some(reference) = first_design_upload(row.attachments.as_ref()) else {
        return (None, None, false);
    };

    let session: Option<(String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT design_subtype, storage_key, preview_key, status
           FROM design_upload_sessions WHERE id = $1",
    )
    .bind(reference)
    .fetch_optional(db)
    .await
    .unwrap_or(None);

    let Some((subtype, storage_key, preview_key, status)) = session else {
        return (None, None, false);
    };
    if status != "completed" {
        return (Some(subtype), None, false);
    }

    // The author's preview when there is one, the file itself otherwise. The
    // four subtypes a browser cannot open are exactly the four that require a
    // preview, so this is the same rule read from the other side.
    let (key, is_preview) = match preview_key.as_deref() {
        Some(k) => (k.to_string(), true),
        None => (storage_key, false),
    };

    match storage.presigned_get_url(&key, URL_TTL_SECONDS).await {
        Ok(url) => (Some(subtype), Some(url), is_preview),
        Err(e) => {
            tracing::warn!(
                deliverable = %row.deliverable_id,
                error = %e,
                "could not sign a wall artefact, showing the entry without it"
            );
            (Some(subtype), None, is_preview)
        }
    }
}

/// The first `design_upload:<uuid>` in an attachments array.
///
/// One, not all of them: `MAX_ATTACHMENTS` is five so that a screen plus its
/// source file fits, and the rite asks for one artefact. A wall showing five
/// tiles for one person reads as a portfolio rather than an introduction.
fn first_design_upload(attachments: Option<&serde_json::Value>) -> Option<Uuid> {
    attachments?
        .as_array()?
        .iter()
        .filter_map(|v| v.as_str())
        .filter_map(|s| s.strip_prefix("design_upload:"))
        .find_map(|id| Uuid::parse_str(id).ok())
}

#[cfg(test)]
mod tests {
    use super::first_design_upload;
    use serde_json::json;

    #[test]
    fn the_first_design_upload_is_the_one_shown() {
        let a = "11111111-1111-4111-8111-111111111111";
        let b = "22222222-2222-4222-8222-222222222222";
        let picked = first_design_upload(Some(&json!([
            format!("design_upload:{a}"),
            format!("design_upload:{b}")
        ])));
        assert_eq!(picked.unwrap().to_string(), a);
    }

    /// An audio rite attaches `audio_file:<uuid>`, and this wall does not know
    /// how to show one. Skipping is right; assuming the id belongs to a design
    /// upload would sign a URL for a key in the wrong family.
    #[test]
    fn an_attachment_of_another_kind_is_not_mistaken_for_one() {
        let id = "33333333-3333-4333-8333-333333333333";
        assert!(first_design_upload(Some(&json!([format!("audio_file:{id}")]))).is_none());
        assert!(first_design_upload(Some(&json!([]))).is_none());
        assert!(first_design_upload(None).is_none());
        // A reference whose id is not a uuid is skipped rather than panicking.
        assert!(first_design_upload(Some(&json!(["design_upload:not-a-uuid"]))).is_none());
    }
}
