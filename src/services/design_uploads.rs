//! Handing in a file too large to send through the API.
//!
//! ## The shape
//!
//! Three calls. `init` reserves a key and returns one presigned PUT URL per
//! part; the client uploads each part straight to the object store and keeps
//! the ETag it gets back; `complete` sends the part list and the backend asks
//! the store to assemble them.
//!
//! The bytes never pass through this process. Five gigabytes through an axum
//! handler on a small VPS is how an API falls over, and it falls over for
//! everybody at once — one upload holds a connection and a buffer for as long
//! as somebody's rural connection takes to push a Blender scene.
//!
//! It is also what makes the upload resumable without any bookkeeping: a part
//! that failed is one presigned URL away from being retried, and nothing here
//! has to remember a byte offset.
//!
//! ## Previews are supplied, not rendered
//!
//! The backlog asked for ffmpeg, a thumbnailer and headless Blender behind a
//! Docker socket. That is three heavy binaries and a privileged socket on a
//! machine this project cannot afford, to produce a still frame that the
//! person who made the file could pick better than any heuristic.
//!
//! The backlog already concedes the principle for After Effects — nothing can
//! parse an `.aep`, so a preview MP4 is required alongside. [`REQUIRES_PREVIEW`]
//! applies that rule to every subtype whose source a browser cannot open.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::DesignSubtype;
use crate::services::storage::StorageService;

/// The smallest part S3 accepts for anything but the last one. Not ours to
/// choose: a smaller part is refused at completion, after the whole upload.
pub const MIN_PART_SIZE: i64 = 5 * 1024 * 1024;

/// What we hand out.
///
/// Three times the floor. Every part costs a presigned URL to generate, a
/// round trip to deliver and an ETag to carry back, so at the five-megabyte
/// minimum a five-gigabyte file would be a thousand of each. At sixteen it is
/// three hundred and twenty, and a part lost on a bad connection still costs
/// under a minute to retry.
pub const PART_SIZE: i64 = 16 * 1024 * 1024;

/// The most parts S3 allows in one multipart upload.
///
/// Checked at `init`, and it never fires at the ceilings above — even the
/// five-gigabyte ones are three hundred parts. It is here because the
/// ceilings are ours to change and this limit is not.
pub const MAX_PARTS: i64 = 10_000;

/// How long a presigned part URL lives.
///
/// Six hours. Long enough for a large file on a slow connection, short enough
/// that a URL found in a log tomorrow is worthless. A client that runs out
/// asks for the remaining parts again — which is the same call it makes to
/// resume.
pub const PART_URL_TTL_SECONDS: u32 = 6 * 60 * 60;

/// How long an unfinished upload is kept before the sweep abandons it.
///
/// Seven days. An abandoned multipart upload keeps its parts, and the object
/// store bills for them; a week is enough for somebody to come back after a
/// holiday, and short enough that it is not a landfill.
pub const SESSION_TTL_HOURS: i64 = 24 * 7;

/// How large a file of each kind may be.
///
/// These come from what the artefact actually is. A brand kit is vectors and
/// a document; an icon set is a few hundred kilobytes of SVG pretending to be
/// bigger. A scene file and a rendered video are genuinely enormous and there
/// is no honest way to ask somebody to shrink them.
///
/// The ceiling is a refusal, not a warning: a subtype that grew past it is
/// usually the wrong subtype, and telling somebody after five gigabytes have
/// moved is telling them too late.
pub fn max_bytes(subtype: DesignSubtype) -> i64 {
    const MB: i64 = 1024 * 1024;
    const GB: i64 = 1024 * MB;
    match subtype {
        // Vectors, documents, words. Anything past this is an export that
        // should have been a source.
        DesignSubtype::BrandKit | DesignSubtype::IconSet | DesignSubtype::TypeFamily => 200 * MB,
        DesignSubtype::CopyDeck | DesignSubtype::ResearchDocument => 100 * MB,
        // Screens and drawings: layered files get heavy honestly.
        DesignSubtype::Interface | DesignSubtype::DesignSystem => 500 * MB,
        DesignSubtype::IllustrationSet => GB,
        // Audio is small until it is uncompressed, and uncompressed is what a
        // sound designer delivers.
        DesignSubtype::Sound => 500 * MB,
        // The three that are genuinely enormous.
        DesignSubtype::Motion => 2 * GB,
        DesignSubtype::Video | DesignSubtype::ThreeDScene => 5 * GB,
    }
}

/// Subtypes whose source file a browser cannot open, and which therefore have
/// to arrive with a preview.
///
/// Not a nicety: a reviewer's queue of unopenable files is a queue nobody
/// works. The rule the backlog wrote for After Effects, applied to everything
/// in the same situation.
pub const REQUIRES_PREVIEW: &[DesignSubtype] = &[
    DesignSubtype::Motion,
    DesignSubtype::Video,
    DesignSubtype::ThreeDScene,
    DesignSubtype::Sound,
];

pub fn requires_preview(subtype: DesignSubtype) -> bool {
    REQUIRES_PREVIEW.contains(&subtype)
}

/// An upload waiting to be finished.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Session {
    pub id: Uuid,
    pub user_id: Uuid,
    pub slice_id: Option<Uuid>,
    pub design_subtype: String,
    pub filename: String,
    pub content_type: String,
    pub declared_bytes: i64,
    pub stored_bytes: Option<i64>,
    pub part_size: i32,
    pub part_count: i32,
    pub storage_key: String,
    pub preview_key: Option<String>,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// One part, and where to PUT it.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct PartUrl {
    pub part_number: i32,
    pub url: String,
    /// How many bytes this part takes. The last one is short; every other one
    /// must be exactly this or the store refuses the assembly.
    pub bytes: i64,
}

/// What a part upload returned, as the client read it back.
#[derive(Debug, Clone, serde::Deserialize, utoipa::ToSchema)]
pub struct CompletedPart {
    pub part_number: i32,
    /// The `ETag` header the object store returned for that part, quotes and
    /// all. Passed through untouched: the store compares it with its own.
    pub etag: String,
}

/// How many parts a file of this size needs.
///
/// Written out rather than `div_ceil`, which is not stable for `i64` on the
/// toolchain this builds with.
fn part_count_for(bytes: i64) -> i64 {
    ((bytes + PART_SIZE - 1) / PART_SIZE).max(1)
}

/// Where the object lands.
///
/// Under the private bucket rather than a new public one. A design deliverable
/// can be under NDA — `docs/design/DATA-GOVERNANCE.md` describes the private
/// deliverable case — so the default has to be "readable through a presigned
/// URL", which is what the private bucket already is. A second bucket would
/// have added a configuration knob whose only correct value is the same
/// policy.
fn key_for(session_id: Uuid, filename: &str) -> String {
    format!("design/{session_id}/{}", sanitise(filename))
}

fn preview_key_for(session_id: Uuid) -> String {
    format!("design/{session_id}/preview")
}

/// Strip anything that would let a filename escape its prefix or confuse a
/// content-disposition header.
///
/// Not a security boundary on its own — the key is built from a UUID we chose
/// — but a filename containing `../` or a newline is a filename that will
/// eventually be interpolated somewhere it should not be.
fn sanitise(filename: &str) -> String {
    let cleaned: String = filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('.').to_string();
    if trimmed.is_empty() {
        "fichier".to_string()
    } else {
        trimmed.chars().take(200).collect()
    }
}

/// What a caller asks for.
#[derive(Debug, Clone, serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct InitInput {
    pub design_subtype: String,
    pub filename: String,
    pub content_type: String,
    /// How many bytes are coming. Checked against the subtype ceiling before
    /// anything moves.
    pub declared_bytes: i64,
    #[serde(default)]
    pub slice_id: Option<Uuid>,
}

/// What `init` hands back.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct Initiated {
    pub session_id: Uuid,
    pub part_size: i64,
    pub parts: Vec<PartUrl>,
    /// Whether this subtype has to arrive with a preview. Returned so a client
    /// learns it before the upload rather than at completion.
    pub preview_required: bool,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Reserve a key, open a multipart upload, and hand out the part URLs.
pub async fn init(
    db: &PgPool,
    storage: &StorageService,
    user_id: Uuid,
    input: InitInput,
) -> Result<Initiated, AppError> {
    let subtype = DesignSubtype::parse(&input.design_subtype).ok_or_else(|| {
        AppError::Validation(format!(
            "'{}' is not a design subtype",
            input.design_subtype
        ))
    })?;

    crate::validators::check_max_len(&input.filename, "filename", 255)?;
    crate::validators::check_max_len(&input.content_type, "content_type", 120)?;

    let ceiling = max_bytes(subtype);
    if input.declared_bytes <= 0 {
        return Err(AppError::Validation("declared_bytes must be positive".into()));
    }
    if input.declared_bytes > ceiling {
        return Err(AppError::Validation(format!(
            "un {} tient en {} Mo au plus ; celui-ci en annonce {}",
            subtype.as_str(),
            ceiling / (1024 * 1024),
            input.declared_bytes / (1024 * 1024)
        )));
    }

    let part_count = part_count_for(input.declared_bytes);
    if part_count > MAX_PARTS {
        return Err(AppError::Validation(
            "ce fichier demande plus de parties que le stockage n'en accepte".into(),
        ));
    }

    // The slice has to be the caller's own claimed challenge. Otherwise an
    // upload could be parked against somebody else's work, and the reviewer
    // would read it as theirs.
    if let Some(slice_id) = input.slice_id {
        let mine: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM project_slices
                             WHERE id = $1 AND claimed_by_user_id = $2)",
        )
        .bind(slice_id)
        .bind(user_id)
        .fetch_one(db)
        .await?;
        if !mine {
            return Err(AppError::Forbidden);
        }
    }

    let session_id = Uuid::new_v4();
    let storage_key = key_for(session_id, &input.filename);

    let upload_id = storage
        .begin_multipart(&storage_key, &input.content_type)
        .await?;

    let expires_at = chrono::Utc::now() + chrono::Duration::hours(SESSION_TTL_HOURS);

    sqlx::query(
        r#"
        INSERT INTO design_upload_sessions
            (id, user_id, slice_id, design_subtype, filename, content_type,
             declared_bytes, part_size, part_count, storage_key, s3_upload_id, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
    )
    .bind(session_id)
    .bind(user_id)
    .bind(input.slice_id)
    .bind(subtype.as_str())
    .bind(&input.filename)
    .bind(&input.content_type)
    .bind(input.declared_bytes)
    .bind(PART_SIZE as i32)
    .bind(part_count as i32)
    .bind(&storage_key)
    .bind(&upload_id)
    .bind(expires_at)
    .execute(db)
    .await?;

    let parts = presign_parts(
        storage,
        &storage_key,
        &upload_id,
        1..=part_count,
        input.declared_bytes,
    )
    .await?;

    Ok(Initiated {
        session_id,
        part_size: PART_SIZE,
        parts,
        preview_required: requires_preview(subtype),
        expires_at,
    })
}

/// Fresh URLs for some of the parts.
///
/// This is both "my URLs expired" and "I am resuming after a crash": the
/// client asks for the parts it has no ETag for, and gets them. Nothing on
/// this side needs to know which those are, which is the point.
pub async fn part_urls(
    db: &PgPool,
    storage: &StorageService,
    user_id: Uuid,
    session_id: Uuid,
    from: i32,
    to: i32,
) -> Result<Vec<PartUrl>, AppError> {
    let session = load_pending(db, user_id, session_id).await?;

    if from < 1 || to < from || to > session.part_count {
        return Err(AppError::Validation(format!(
            "les parties vont de 1 à {}",
            session.part_count
        )));
    }

    presign_parts(
        storage,
        &session.storage_key,
        &session_upload_id(db, session_id).await?,
        from as i64..=to as i64,
        session.declared_bytes,
    )
    .await
}

async fn presign_parts(
    storage: &StorageService,
    storage_key: &str,
    upload_id: &str,
    range: std::ops::RangeInclusive<i64>,
    total_bytes: i64,
) -> Result<Vec<PartUrl>, AppError> {
    let mut parts = Vec::new();
    for number in range {
        let url = storage
            .presign_part_put(storage_key, upload_id, number as u32, PART_URL_TTL_SECONDS)
            .await?;
        // The last part is whatever is left; every other one is exactly
        // `PART_SIZE`, and the store refuses the assembly if it is not.
        let offset = (number - 1) * PART_SIZE;
        let bytes = (total_bytes - offset).clamp(0, PART_SIZE);
        parts.push(PartUrl {
            part_number: number as i32,
            url,
            bytes,
        });
    }
    Ok(parts)
}

/// Assemble the parts and close the session.
///
/// The size is read back from the object store rather than trusted: the
/// ceiling checked at `init` was checked against a number the client chose.
pub async fn complete(
    db: &PgPool,
    storage: &StorageService,
    user_id: Uuid,
    session_id: Uuid,
    parts: Vec<CompletedPart>,
) -> Result<Session, AppError> {
    let session = load_pending(db, user_id, session_id).await?;

    if parts.is_empty() {
        return Err(AppError::Validation("no parts were uploaded".into()));
    }
    if parts.len() as i32 != session.part_count {
        return Err(AppError::Validation(format!(
            "{} parties annoncées, {} reçues",
            session.part_count,
            parts.len()
        )));
    }

    let upload_id = session_upload_id(db, session_id).await?;
    storage
        .finish_multipart(&session.storage_key, &upload_id, &parts)
        .await?;

    let stored_bytes = storage.object_size(&session.storage_key).await?;
    let subtype = DesignSubtype::parse(&session.design_subtype)
        .ok_or_else(|| AppError::Internal("a stored subtype stopped being one".into()))?;

    if stored_bytes > max_bytes(subtype) {
        // The store already holds it, so it goes rather than being kept as a
        // trophy. A client that lies about a size is a client whose next lie
        // should also cost it the upload.
        let _ = storage.delete_private(&session.storage_key).await;
        abort(db, session_id).await;
        return Err(AppError::Validation(format!(
            "le fichier reçu fait {} Mo, au-delà de la limite du sous-type",
            stored_bytes / (1024 * 1024)
        )));
    }

    let session = sqlx::query_as::<_, Session>(
        r#"
        UPDATE design_upload_sessions
           SET status = 'completed', stored_bytes = $2, completed_at = NOW()
         WHERE id = $1 AND status = 'pending'
     RETURNING id, user_id, slice_id, design_subtype, filename, content_type,
               declared_bytes, stored_bytes, part_size, part_count, storage_key,
               preview_key, status, created_at, completed_at, expires_at
        "#,
    )
    .bind(session_id)
    .bind(stored_bytes)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::Conflict("this upload was finished by somebody else".into()))?;

    Ok(session)
}

/// A presigned PUT for the preview of a completed upload.
///
/// Separate from the file itself because it is a separate object with a
/// separate lifetime: a preview can be replaced without re-uploading five
/// gigabytes, and on the subtypes that require one it is the only thing a
/// reviewer will actually open.
pub async fn preview_upload_url(
    db: &PgPool,
    storage: &StorageService,
    user_id: Uuid,
    session_id: Uuid,
) -> Result<String, AppError> {
    let session = load(db, user_id, session_id).await?;
    let key = preview_key_for(session.id);

    sqlx::query("UPDATE design_upload_sessions SET preview_key = $2 WHERE id = $1")
        .bind(session_id)
        .bind(&key)
        .execute(db)
        .await?;

    storage.presign_put_url(&key, PART_URL_TTL_SECONDS).await
}

/// A link somebody can open, for a limited time.
pub async fn download_url(
    db: &PgPool,
    storage: &StorageService,
    user_id: Uuid,
    session_id: Uuid,
    ttl_seconds: u32,
) -> Result<String, AppError> {
    let session = load(db, user_id, session_id).await?;
    if session.status != "completed" {
        return Err(AppError::Conflict(
            "this upload is not finished, so there is nothing to download".into(),
        ));
    }
    storage
        .presigned_get_url(&session.storage_key, ttl_seconds)
        .await
}

async fn load(db: &PgPool, user_id: Uuid, session_id: Uuid) -> Result<Session, AppError> {
    sqlx::query_as::<_, Session>(
        r#"
        SELECT id, user_id, slice_id, design_subtype, filename, content_type,
               declared_bytes, stored_bytes, part_size, part_count, storage_key,
               preview_key, status, created_at, completed_at, expires_at
          FROM design_upload_sessions
         WHERE id = $1 AND user_id = $2
        "#,
    )
    .bind(session_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound("no such upload".into()))
}

async fn load_pending(db: &PgPool, user_id: Uuid, session_id: Uuid) -> Result<Session, AppError> {
    let session = load(db, user_id, session_id).await?;
    if session.status != "pending" {
        return Err(AppError::Conflict(format!(
            "this upload is already {}",
            session.status
        )));
    }
    if session.expires_at < chrono::Utc::now() {
        return Err(AppError::Conflict(
            "this upload expired; start a new one".into(),
        ));
    }
    Ok(session)
}

/// The object store's handle, kept out of [`Session`] on purpose: it is a
/// credential-shaped string and it has no business in a JSON response.
async fn session_upload_id(db: &PgPool, session_id: Uuid) -> Result<String, AppError> {
    sqlx::query_scalar("SELECT s3_upload_id FROM design_upload_sessions WHERE id = $1")
        .bind(session_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("no such upload".into()))
}

async fn abort(db: &PgPool, session_id: Uuid) {
    if let Err(e) = sqlx::query(
        "UPDATE design_upload_sessions SET status = 'aborted' WHERE id = $1 AND status = 'pending'",
    )
    .bind(session_id)
    .execute(db)
    .await
    {
        tracing::warn!(%session_id, error = %e, "upload session not marked aborted");
    }
}

/// Give up on the uploads nobody finished.
///
/// An abandoned multipart upload keeps the parts already sent, and the object
/// store bills for them whether or not anybody ever completes it. This is the
/// only thing standing between a slow month and a storage invoice nobody can
/// explain.
///
/// Returns how many were abandoned.
pub async fn sweep_expired(db: &PgPool, storage: &StorageService) -> Result<u64, AppError> {
    let stale: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, storage_key, s3_upload_id
           FROM design_upload_sessions
          WHERE status = 'pending' AND expires_at < NOW()
          ORDER BY expires_at ASC
          LIMIT 200",
    )
    .fetch_all(db)
    .await?;

    let mut swept = 0;
    for (id, key, upload_id) in stale {
        // The store first: if this fails the row stays pending and the next
        // pass tries again, which is what stops the parts being orphaned with
        // nothing left pointing at them.
        if let Err(e) = storage.abort_multipart(&key, &upload_id).await {
            tracing::warn!(%id, error = %e, "expired upload not abandoned at the store");
            continue;
        }
        if let Err(e) = sqlx::query(
            "UPDATE design_upload_sessions SET status = 'expired' WHERE id = $1",
        )
        .bind(id)
        .execute(db)
        .await
        {
            tracing::warn!(%id, error = %e, "expired upload not marked");
            continue;
        }
        swept += 1;
    }
    Ok(swept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ceilings_reflect_what_the_artefact_is() {
        const GB: i64 = 1024 * 1024 * 1024;
        // A scene file and a rendered video are genuinely enormous and there
        // is no honest way to ask somebody to shrink them.
        assert_eq!(max_bytes(DesignSubtype::ThreeDScene), 5 * GB);
        assert_eq!(max_bytes(DesignSubtype::Video), 5 * GB);
        // Words are not.
        assert!(max_bytes(DesignSubtype::CopyDeck) < max_bytes(DesignSubtype::Motion));
        // Every subtype has one: a missing arm would let an unbounded upload
        // through, and the match is exhaustive so this cannot rot silently.
        for subtype in DesignSubtype::ALL {
            assert!(max_bytes(*subtype) > 0, "{}", subtype.as_str());
        }
    }

    #[test]
    fn only_the_unopenable_subtypes_demand_a_preview() {
        // A reviewer's queue of files a browser cannot open is a queue nobody
        // works.
        assert!(requires_preview(DesignSubtype::ThreeDScene));
        assert!(requires_preview(DesignSubtype::Sound));
        // An SVG or a PDF opens on its own; demanding a preview for it would
        // be busywork.
        assert!(!requires_preview(DesignSubtype::IconSet));
        assert!(!requires_preview(DesignSubtype::Interface));
    }

    #[test]
    fn the_largest_allowed_file_fits_inside_the_part_limit() {
        // Every ceiling, not just the biggest: a ceiling raised later is
        // exactly when this stops being obvious.
        for subtype in DesignSubtype::ALL {
            let parts = part_count_for(max_bytes(*subtype));
            assert!(
                parts <= MAX_PARTS,
                "{} needs {parts} parts",
                subtype.as_str()
            );
        }
        // Sixteen megabytes is not what keeps us under the limit — five would
        // too, at four times the round trips. It is the round trips.
        let five_gb = 5i64 * 1024 * 1024 * 1024;
        assert_eq!(part_count_for(five_gb), 320);
        const { assert!(PART_SIZE >= MIN_PART_SIZE) };
    }

    #[test]
    fn an_empty_file_still_gets_one_part() {
        assert_eq!(part_count_for(1), 1);
        assert_eq!(part_count_for(PART_SIZE), 1);
        assert_eq!(part_count_for(PART_SIZE + 1), 2);
    }

    #[test]
    fn a_filename_cannot_escape_its_prefix() {
        // The property, not the exact spelling: what matters is that nothing
        // survives that could climb out of the prefix or split a header.
        // `../../etc/passwd` comes back as `_.._etc_passwd` — ugly and inert,
        // which is the right trade.
        for hostile in [
            "../../etc/passwd",
            "..\\..\\windows\\system32",
            "a\nb",
            "a\rb",
            "a\0b",
        ] {
            let clean = sanitise(hostile);
            assert!(
                !clean.contains('/')
                    && !clean.contains('\\')
                    && !clean.contains('\n')
                    && !clean.contains('\r')
                    && !clean.contains('\0'),
                "{hostile:?} -> {clean:?}"
            );
        }

        assert_eq!(sanitise("scène finale.blend"), "sc_ne_finale.blend");
        // A name that is nothing but dots would leave an empty segment.
        assert_eq!(sanitise("..."), "fichier");
        assert_eq!(sanitise(""), "fichier");
    }
}
