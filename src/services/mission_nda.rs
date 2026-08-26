//! Signing the confidentiality agreement a mission requires.
//!
//! ## What this is, in the terms that matter
//!
//! A simple electronic signature under eIDAS: the signer is authenticated, they
//! are shown a document, they accept it, and what is recorded is the hash of the
//! exact bytes they were shown, with the time, the address and the user agent.
//! Admissible, rebuttable, and the lowest of the three tiers. Migration 0557
//! says the same thing where the table is defined, and
//! `docs/security/LEGAL.md` says it where a person can read it.
//!
//! ## The hash is the whole point
//!
//! Without it a signature proves that somebody clicked yes, and the document
//! can be substituted afterwards. So the flow is: the client asks for the
//! agreement and gets the text *and its hash*; the signature quotes the hash
//! back; and this module refuses it if the hash is not the one it would serve
//! now. A signer cannot sign something they were not shown, and neither party
//! can later produce a different document.
//!
//! ## A client's own agreement has to be held here
//!
//! `missions.nda_document_url` accepts an external link, and a signature
//! against one is refused: an agreement this platform cannot read is an
//! agreement it cannot hash, and a record saying "they accepted whatever was at
//! this URL in March" is worth nothing in a dispute. A client bringing their
//! own has to upload it, which is one step and the difference between a record
//! and a gesture.

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::storage::StorageService;

/// The agreement as it would be shown right now.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct Agreement {
    /// `mutual_standard`, `mutual_extended` or `client_custom`.
    pub template: String,
    pub title: String,
    pub body_md: String,
    /// The hash of `body_md` exactly as served. Quoted back when signing.
    pub sha256: String,
    /// Which stored version this is, for the platform's own templates.
    pub version: Option<i16>,
    /// Whether a lawyer has read it. False for both drafts, and said out loud
    /// rather than left to be assumed.
    pub is_reviewed: bool,
    pub locale: String,
}

fn digest(body: &str) -> String {
    let mut h = Sha256::new();
    h.update(body.as_bytes());
    hex::encode(h.finalize())
}

/// What this mission asks somebody to sign.
///
/// `locale` is a preference: a template with no version in that language falls
/// back to English rather than refusing, because an agreement in the wrong
/// language is still readable and no agreement is not.
pub async fn agreement_for(
    db: &PgPool,
    storage: &StorageService,
    mission_id: Uuid,
    locale: &str,
) -> Result<Agreement, AppError> {
    let row: Option<(bool, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT nda_required, nda_template, nda_document_url
           FROM missions WHERE id = $1",
    )
    .bind(mission_id)
    .fetch_optional(db)
    .await?;

    let Some((required, template, document_url)) = row else {
        return Err(AppError::NotFound("no such mission".into()));
    };
    if !required {
        return Err(AppError::NotFound(
            "this mission asks for no confidentiality agreement".into(),
        ));
    }
    let Some(template) = template else {
        // Refused by a constraint since 0560, so this is defence in depth.
        return Err(AppError::Internal(
            "a mission requires an agreement and names none".into(),
        ));
    };

    if template == "client_custom" {
        let Some(url) = document_url else {
            return Err(AppError::Internal(
                "a custom agreement with no document".into(),
            ));
        };
        if url.starts_with("https://") {
            return Err(AppError::Validation(
                "this mission's agreement is hosted somewhere this platform \
                 cannot read. It has to be uploaded here before anybody can \
                 sign it — a record saying somebody accepted whatever was at a \
                 URL is worth nothing in a dispute"
                    .into(),
            ));
        }
        let key = url.trim_start_matches('/');
        let bytes = storage.get_private(key).await?;
        let body = String::from_utf8(bytes).map_err(|_| {
            AppError::Validation(
                "that agreement is not text this platform can display. Upload it \
                 as markdown or plain text"
                    .into(),
            )
        })?;
        return Ok(Agreement {
            template,
            title: "Confidentiality agreement (client's own)".to_string(),
            sha256: digest(&body),
            body_md: body,
            version: None,
            // Somebody else's document. This platform has no view on whether a
            // lawyer read it, and saying `true` would be inventing one.
            is_reviewed: false,
            locale: locale.to_string(),
        });
    }

    let wanted = if locale == "fr" { "fr" } else { "en" };
    let row: Option<(String, String, i16, bool, String)> = sqlx::query_as(
        "SELECT title, body_md, version, is_reviewed, locale
           FROM mission_nda_templates
          WHERE slug = $1 AND is_current
            AND locale = $2",
    )
    .bind(&template)
    .bind(wanted)
    .fetch_optional(db)
    .await?;

    // Fall back to English rather than refusing: an agreement in the wrong
    // language is readable, and none is not.
    let row = match row {
        Some(r) => Some(r),
        None => {
            sqlx::query_as(
                "SELECT title, body_md, version, is_reviewed, locale
                   FROM mission_nda_templates
                  WHERE slug = $1 AND is_current AND locale = 'en'",
            )
            .bind(&template)
            .fetch_optional(db)
            .await?
        }
    };

    let Some((title, body_md, version, is_reviewed, served_locale)) = row else {
        return Err(AppError::Internal(format!(
            "no current text for the {template} agreement"
        )));
    };

    Ok(Agreement {
        template,
        title,
        sha256: digest(&body_md),
        body_md,
        version: Some(version),
        is_reviewed,
        locale: served_locale,
    })
}

/// What a signature says.
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SignatureInput {
    /// The name the signer types. Not verified against anything, and kept
    /// because it is part of what was done.
    pub typed_name: String,
    /// The hash returned by the agreement endpoint. Refused if it is not the
    /// hash of what would be served now.
    pub document_sha256: String,
    #[serde(default)]
    pub locale: Option<String>,
}

/// Record the signature.
///
/// Idempotent in the useful direction: signing twice returns the existing
/// signature rather than an error, because a client that retried a request must
/// not be told it has done something wrong.
#[allow(clippy::too_many_arguments)]
pub async fn sign(
    db: &PgPool,
    storage: &StorageService,
    mission_id: Uuid,
    signer: Uuid,
    ip: Option<std::net::IpAddr>,
    user_agent: Option<&str>,
    input: SignatureInput,
) -> Result<Uuid, AppError> {
    if input.typed_name.trim().chars().count() < 2 {
        return Err(AppError::Validation(
            "type the name you are signing with".into(),
        ));
    }

    let agreement = agreement_for(
        db,
        storage,
        mission_id,
        input.locale.as_deref().unwrap_or("en"),
    )
    .await?;

    if !agreement
        .sha256
        .eq_ignore_ascii_case(input.document_sha256.trim())
    {
        return Err(AppError::Conflict(
            "the agreement has changed since it was shown to you. Read it again \
             — a signature has to name the text it agreed to, and this one would \
             have named the wrong one"
                .into(),
        ));
    }

    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM mission_nda_signatures
          WHERE mission_id = $1 AND signer_user_id = $2",
    )
    .bind(mission_id)
    .bind(signer)
    .fetch_optional(db)
    .await?;
    if let Some(id) = existing {
        return Ok(id);
    }

    let document_url: String = sqlx::query_scalar(
        "SELECT COALESCE(nda_document_url,
                         '/mission-nda/' || nda_template || '/v' ||
                         (SELECT version::TEXT FROM mission_nda_templates t
                           WHERE t.slug = m.nda_template AND t.is_current
                             AND t.locale = $2 LIMIT 1))
           FROM missions m WHERE m.id = $1",
    )
    .bind(mission_id)
    .bind(&agreement.locale)
    .fetch_one(db)
    .await?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO mission_nda_signatures
             (mission_id, signer_user_id, document_url, document_sha256,
              template, signer_ip, signer_user_agent, signer_typed_name)
         VALUES ($1, $2, $3, $4, $5, $6::INET, $7, $8)
         RETURNING id",
    )
    .bind(mission_id)
    .bind(signer)
    .bind(&document_url)
    .bind(&agreement.sha256)
    .bind(&agreement.template)
    // Bound as text and cast in SQL: the column is INET so that a range query
    // is possible if a signature is ever contested, and the `ipnetwork` sqlx
    // feature is not enabled — one cast is cheaper than a dependency.
    //
    // `None` where the deployment had no trustworthy address. Migration 0557
    // says why that is better than a placeholder.
    .bind(ip.map(|a| a.to_string()))
    .bind(user_agent)
    .bind(input.typed_name.trim())
    .fetch_one(db)
    .await?;

    metrics::counter!("skilluv_mission_nda_signatures_total",
        "template" => agreement.template.clone())
    .increment(1);

    Ok(id)
}

/// The signature this person gave, if they gave one.
pub async fn signature_of(
    db: &PgPool,
    mission_id: Uuid,
    signer: Uuid,
) -> Result<Option<serde_json::Value>, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'id', id, 'template', template,
                    'document_sha256', document_sha256,
                    'document_url', document_url,
                    'typed_name', signer_typed_name,
                    'signed_at', signed_at,
                    'released_at', released_at,
                    'released_reason', released_reason)
           FROM mission_nda_signatures
          WHERE mission_id = $1 AND signer_user_id = $2",
    )
    .bind(mission_id)
    .bind(signer)
    .fetch_optional(db)
    .await?)
}

/// Release somebody from the obligation, early.
///
/// Recorded rather than deleted: that the obligation existed is a fact, and
/// whether it still binds is a different question from whether it ever did.
pub async fn release(
    db: &PgPool,
    mission_id: Uuid,
    signer: Uuid,
    reason: &str,
) -> Result<(), AppError> {
    if reason.trim().chars().count() < 10 {
        return Err(AppError::Validation(
            "say why the obligation is being released".into(),
        ));
    }
    let affected = sqlx::query(
        "UPDATE mission_nda_signatures
            SET released_at = NOW(), released_reason = $3
          WHERE mission_id = $1 AND signer_user_id = $2 AND released_at IS NULL",
    )
    .bind(mission_id)
    .bind(signer)
    .bind(reason.trim())
    .execute(db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(
            "no live signature to release".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_of_the_bytes_as_served() {
        let a = "# Agreement\n\nOne clause.\n";
        let b = "# Agreement\n\nOne clause.";
        // A trailing newline changes the document, and therefore the hash. That
        // is the point: two texts that differ at all are different agreements.
        assert_ne!(digest(a), digest(b));
        assert_eq!(digest(a), digest(a));
        assert_eq!(digest(a).len(), 64);
    }
}
