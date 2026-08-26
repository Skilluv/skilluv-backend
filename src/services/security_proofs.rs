//! Proof files attached to a vulnerability report.
//!
//! ## Why these go in the private bucket and never come out with a stable URL
//!
//! A screenshot of an unfixed vulnerability is the vulnerability. Put it in the
//! public bucket and the embargo is decorative: anybody who guesses the key —
//! or finds it in a browser history, a chat log, a shared link — has the
//! disclosure, and there is no way to take it back.
//!
//! So: private bucket, a key rather than a URL in the report, and a signed URL
//! minted per request by the download endpoint after checking who is asking.
//! The link a reporter can share expires in an hour, which is long enough to
//! open and short enough that pasting it into a group chat is not a leak.
//!
//! ## What may be uploaded
//!
//! Images, recordings, captures and text. Not executables, and the check is on
//! the extension *and* on the first bytes, because the extension is chosen by
//! whoever is uploading. This is a security platform, and a report attachment
//! is the most obvious place to try to have somebody run something.
//!
//! Nothing here scans for malware. Saying so plainly is better than implying it:
//! a proof file is downloaded by a reviewer who is expected to open it in the
//! same isolated environment they would use for anything else that arrived from
//! a stranger, and `docs/security/REVIEWER-ONBOARDING.md` says that in the same
//! words.
//!
//! ## The orphan sweep
//!
//! An upload that no report references after thirty days is deleted. Uploads
//! happen before submission — that is the shape of the form — so an abandoned
//! draft leaves files behind, and a bucket that only grows is a bucket that
//! eventually holds somebody's proof of something they never reported.

use uuid::Uuid;

use crate::errors::AppError;
use crate::services::storage::StorageService;

/// Twenty megabytes. A screen recording of an exploit fits; a memory image does
/// not, and a memory image is not a proof of a finding.
pub const MAX_PROOF_BYTES: usize = 20 * 1024 * 1024;

/// How long a download link lives.
pub const SIGNED_URL_SECONDS: u32 = 3600;

/// Uploads a person may make in an hour.
pub const UPLOADS_PER_HOUR: u64 = 20;

/// Days an unreferenced upload survives.
pub const ORPHAN_DAYS: i64 = 30;

/// Extensions a proof may have, and the content type each is stored as.
///
/// An allow-list rather than a deny-list, because a deny-list of executable
/// formats is a list somebody adds a format to every year.
pub fn content_type_for(extension: &str) -> Option<&'static str> {
    Some(match extension {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "pcap" | "pcapng" => "application/vnd.tcpdump.pcap",
        "txt" | "log" => "text/plain",
        "md" => "text/markdown",
        "json" | "har" => "application/json",
        "csv" => "text/csv",
        "eml" => "message/rfc822",
        "pdf" => "application/pdf",
        _ => return None,
    })
}

/// Whether the first bytes say something the extension did not.
///
/// Catches the ordinary case: an executable renamed to `.png`. Not a general
/// content sniffer and not presented as one — it refuses the four signatures
/// that matter and lets text through, which is the shape of every proof format
/// on the list above that is not already checked by its own magic bytes.
pub fn looks_executable(bytes: &[u8]) -> bool {
    const SIGNATURES: &[&[u8]] = &[
        b"MZ",                     // Windows PE
        &[0x7f, b'E', b'L', b'F'], // ELF
        &[0xca, 0xfe, 0xba, 0xbe], // Mach-O fat / Java class
        &[0xfe, 0xed, 0xfa, 0xce], // Mach-O
        &[0xfe, 0xed, 0xfa, 0xcf],
        b"#!",                     // a script with a shebang
    ];
    SIGNATURES.iter().any(|sig| bytes.starts_with(sig))
}

/// Store one proof file, and return the key that goes in the report.
///
/// The key contains the uploader's id, which is what makes the ownership check
/// on download a string comparison rather than a lookup — and what makes an
/// orphaned file attributable when the sweep finds it.
pub async fn store(
    storage: &StorageService,
    uploader: Uuid,
    original_filename: &str,
    bytes: &[u8],
) -> Result<String, AppError> {
    if bytes.is_empty() {
        return Err(AppError::Validation("that file is empty".into()));
    }
    if bytes.len() > MAX_PROOF_BYTES {
        return Err(AppError::Validation(format!(
            "at most {} MB per proof. A recording of the exploit fits; a full \
             memory image is not a proof of a finding",
            MAX_PROOF_BYTES / (1024 * 1024)
        )));
    }

    let extension = original_filename
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();

    let Some(content_type) = content_type_for(&extension) else {
        return Err(AppError::Validation(format!(
            "'{extension}' is not an accepted proof format. Images, recordings, \
             captures, text and PDFs are"
        )));
    };

    if looks_executable(bytes) {
        return Err(AppError::Validation(
            "that file starts like an executable, whatever it is called. A \
             report attachment is not a delivery mechanism"
                .into(),
        ));
    }

    let key = format!("security-proofs/{uploader}/{}.{extension}", Uuid::new_v4());
    storage.upload_private(&key, bytes, content_type).await?;
    Ok(key)
}

/// Who may read a proof.
///
/// Four people: the reporter who uploaded it, a triager, a reviewer of this
/// domain, and an administrator. Not the owner of the system under test — they
/// are told about the finding through the disclosure, and handing them the
/// reporter's raw evidence is a decision for the reporter.
pub async fn may_read(db: &sqlx::PgPool, viewer: Uuid, key: &str) -> Result<bool, AppError> {
    // The uploader, from the key itself.
    if key.starts_with(&format!("security-proofs/{viewer}/")) {
        return Ok(true);
    }

    let privileged = crate::middleware::capabilities::require_any_capability(
        db,
        viewer,
        &[
            "admin",
            "security_triager",
            "security_reviewer:all",
            "challenge_validator:security",
        ],
    )
    .await
    .is_ok();
    if privileged {
        return Ok(true);
    }

    // A reviewer of any one security family may read the proofs of a finding
    // they are working on. Checked by family rather than by assignment, for the
    // reason the review queue works that way: a finding is picked up by
    // whoever is free.
    let family_reviewer: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM user_capabilities
              WHERE user_id = $1
                AND capability LIKE 'security_reviewer:%'
                AND revoked_at IS NULL
                AND (expires_at IS NULL OR expires_at > NOW()))",
    )
    .bind(viewer)
    .fetch_one(db)
    .await?;

    Ok(family_reviewer)
}

/// A link to one proof, valid for an hour.
pub async fn signed_url(storage: &StorageService, key: &str) -> Result<String, AppError> {
    storage.presigned_get_url(key, SIGNED_URL_SECONDS).await
}

/// Delete uploads no report references.
///
/// Reads the referenced set from `security_findings.proof_keys` and deletes what
/// is not in it and is old enough. Returns how many went.
///
/// The listing comes from the bucket rather than from a table of uploads,
/// deliberately: a table would be a second record of what exists, and the
/// question this sweep answers — "what is in the bucket that nothing points
/// at" — is answered wrongly by anything except the bucket.
pub async fn sweep_orphans(
    db: &sqlx::PgPool,
    storage: &StorageService,
    keys_in_bucket: &[(String, chrono::DateTime<chrono::Utc>)],
) -> Result<usize, AppError> {
    let referenced: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT unnest(proof_keys) FROM security_findings
          WHERE cardinality(proof_keys) > 0",
    )
    .fetch_all(db)
    .await?;

    let cutoff = chrono::Utc::now() - chrono::Duration::days(ORPHAN_DAYS);
    let mut deleted = 0;

    for (key, uploaded_at) in keys_in_bucket {
        if *uploaded_at > cutoff || referenced.contains(key) {
            continue;
        }
        match storage.delete_private(key).await {
            Ok(()) => deleted += 1,
            Err(e) => tracing::warn!(%key, error = %e, "orphaned proof not deleted"),
        }
    }
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_format_list_is_an_allow_list() {
        assert_eq!(content_type_for("png"), Some("image/png"));
        assert_eq!(content_type_for("pcapng"), Some("application/vnd.tcpdump.pcap"));
        // Every one of these has been somebody's "proof file".
        for ext in ["exe", "sh", "bat", "dll", "so", "jar", "ps1", "apk", "zip"] {
            assert_eq!(content_type_for(ext), None, "{ext} must be refused");
        }
        // Including the empty extension, which is what a file called `proof`
        // produces.
        assert_eq!(content_type_for(""), None);
    }

    #[test]
    fn a_renamed_executable_is_caught_by_its_first_bytes() {
        assert!(looks_executable(b"MZ\x90\x00 this is a PE"));
        assert!(looks_executable(&[0x7f, b'E', b'L', b'F', 0x02]));
        assert!(looks_executable(b"#!/bin/sh\necho hello"));
        // And an actual PNG is not.
        assert!(!looks_executable(&[
            0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a
        ]));
        assert!(!looks_executable(b"192.168.1.42 - - [01/Jan/2026]"));
        assert!(!looks_executable(b""));
    }
}
