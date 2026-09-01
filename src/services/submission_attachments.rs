//! What a submission hands in besides its text.
//!
//! `POST /challenges/{id}/submit` takes `{ code, language }` — a text field.
//! That is the whole contract, and it is enough for the domains whose artifact
//! *is* text. It is not enough for a screen or for twenty seconds of sound: a
//! designer uploads through `POST /design/uploads`, a sound engineer through
//! the audio surface, and until now nothing tied either to the submission. The
//! reviewer opened the deliverable and found a URL somebody had pasted into a
//! paragraph, if they had thought to.
//!
//! An attachment is a reference to something already uploaded, never a URL the
//! client invents. Two reasons. A free-text URL is an open redirect and an
//! SSRF invitation the moment anything fetches it, and it lets somebody attach
//! a file they do not own — including one belonging to another candidate.
//! Referencing rows the platform already stores means ownership is checkable,
//! and it is checked here.

use uuid::Uuid;

use crate::errors::AppError;

/// At most this many per submission. A rite hands in one artifact; the ceiling
/// exists so a stems folder or a screen plus its source file fits, not so that
/// somebody can attach a hundred files a reviewer is expected to open.
pub const MAX_ATTACHMENTS: usize = 5;

/// Something already uploaded, named by the surface that holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attachment {
    /// A `design_upload_sessions` row, which must be `completed`.
    DesignUpload(Uuid),
    /// An `audio_artifact_files` row.
    AudioFile(Uuid),
}

impl Attachment {
    /// `design_upload:<uuid>` or `audio_file:<uuid>`.
    pub fn parse(raw: &str) -> Result<Self, AppError> {
        let (kind, id) = raw.split_once(':').ok_or_else(|| {
            AppError::Validation(format!(
                "attachment '{raw}' must be 'design_upload:<uuid>' or 'audio_file:<uuid>'"
            ))
        })?;
        let id = Uuid::parse_str(id)
            .map_err(|_| AppError::Validation(format!("attachment '{raw}' has no valid id")))?;
        match kind {
            "design_upload" => Ok(Self::DesignUpload(id)),
            "audio_file" => Ok(Self::AudioFile(id)),
            other => Err(AppError::Validation(format!(
                "attachment kind '{other}' is not one of: design_upload, audio_file"
            ))),
        }
    }

    pub fn as_ref_string(&self) -> String {
        match self {
            Self::DesignUpload(id) => format!("design_upload:{id}"),
            Self::AudioFile(id) => format!("audio_file:{id}"),
        }
    }
}

/// Parse every reference, refuse anything the caller does not own, and hand
/// back the canonical strings to store.
///
/// Ownership is the whole point of this function. Without it, a reference is
/// just a URL with extra steps: somebody could attach another candidate's
/// screen and be reviewed on it.
pub async fn validate_owned(
    db: &sqlx::PgPool,
    user_id: Uuid,
    raw: &[String],
) -> Result<Vec<String>, AppError> {
    if raw.len() > MAX_ATTACHMENTS {
        return Err(AppError::Validation(format!(
            "at most {MAX_ATTACHMENTS} attachments per submission, got {}",
            raw.len()
        )));
    }

    let mut out = Vec::with_capacity(raw.len());
    for entry in raw {
        let attachment = Attachment::parse(entry)?;
        match attachment {
            Attachment::DesignUpload(id) => {
                // `completed` as well as owned: a pending upload has no bytes
                // in the store yet, so attaching one hands the reviewer a link
                // to nothing.
                let ok: Option<String> = sqlx::query_scalar(
                    "SELECT status FROM design_upload_sessions WHERE id = $1 AND user_id = $2",
                )
                .bind(id)
                .bind(user_id)
                .fetch_optional(db)
                .await?;
                match ok.as_deref() {
                    Some("completed") => {}
                    Some(other) => {
                        return Err(AppError::Validation(format!(
                            "upload {id} is {other}, so there is nothing to hand in yet"
                        )));
                    }
                    None => return Err(AppError::NotFound(format!("no upload {id} of yours"))),
                }
            }
            Attachment::AudioFile(id) => {
                // `audio_artifact_files` carries no uploader of its own — it
                // hangs off a slice, and the slice's claimant is who delivered
                // it. That is the ownership this checks.
                let owned: bool = sqlx::query_scalar(
                    "SELECT EXISTS (
                       SELECT 1 FROM audio_artifact_files f
                         JOIN project_slices s ON s.id = f.slice_id
                        WHERE f.id = $1 AND s.claimed_by_user_id = $2)",
                )
                .bind(id)
                .bind(user_id)
                .fetch_one(db)
                .await?;
                if !owned {
                    return Err(AppError::NotFound(format!("no audio file {id} of yours")));
                }
            }
        }
        let canonical = attachment.as_ref_string();
        if !out.contains(&canonical) {
            out.push(canonical);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_names_a_surface_and_an_id() {
        let id = Uuid::new_v4();
        assert_eq!(
            Attachment::parse(&format!("design_upload:{id}")).unwrap(),
            Attachment::DesignUpload(id)
        );
        assert_eq!(
            Attachment::parse(&format!("audio_file:{id}")).unwrap(),
            Attachment::AudioFile(id)
        );
    }

    /// A URL is refused rather than stored, which is the point: the reviewer
    /// must be shown something the platform holds and the submitter owns.
    #[test]
    fn a_url_is_not_an_attachment() {
        for bad in [
            "https://example.com/screen.png",
            "design_upload:not-a-uuid",
            "wat:00000000-0000-0000-0000-000000000000",
            "no-separator",
        ] {
            assert!(Attachment::parse(bad).is_err(), "{bad} was accepted");
        }
    }

    #[test]
    fn the_canonical_form_round_trips() {
        let id = Uuid::new_v4();
        let a = Attachment::DesignUpload(id);
        assert_eq!(Attachment::parse(&a.as_ref_string()).unwrap(), a);
    }
}
