//! Attestations that rest on an audio artefact.
//!
//! ## The same shape as the AI generators, and one difference that matters
//!
//! Like [`crate::services::ai_attestations`], every generator re-checks its
//! own precondition against the database before issuing, the attestations are
//! `artefact` ones (migration 0198), and re-running is free because
//! `uniq_attestations_artefact_per_deliverable` makes a second pass insert
//! nothing.
//!
//! The difference is the licence gate. In every other domain a provenance
//! problem makes work weaker; here it makes it unusable — one untraced loop
//! means a client cannot ship the track, and finds out months later. So a
//! composition and a sound pack are not attested until the author has declared
//! that the source list is complete. Refusing to issue is the honest
//! behaviour: the attestation is a claim a stranger relies on, and "the
//! sources are in order" is part of what it claims.
//!
//! A voice reel, an adaptive system and a programming contribution are not
//! gated, and deliberately: none of them redistributes third-party material by
//! nature, and gating them would make the declaration a formality people click
//! through — which is exactly how a gate stops meaning anything.
//!
//! ## `audio_project_credited` is issued by hand
//!
//! A credit is a fact about somebody else's released work: a name in a game's
//! end titles, a line in a podcast description. Nothing in this database can
//! see it, and a generator that inferred it from a delivered slice would
//! attest that work shipped whenever it was merely finished. It is issued by a
//! reviewer who followed the link, through [`issue_credit`].

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Which basis an audio artefact supports, from what it is.
///
/// `ambient_soundscape` maps to the sound-pack basis rather than to a basis of
/// its own. It is a delivery of designed sound like any other, and inventing
/// `audio_soundscape_delivered` would split one count across two words for a
/// distinction nobody hiring asks about.
fn basis_for_subtype(subtype: &str) -> Option<&'static str> {
    match subtype {
        "composition" => Some("audio_composition_published"),
        "sound_pack" => Some("audio_soundpack_delivered"),
        "ambient_soundscape" => Some("audio_soundpack_delivered"),
        "voice_reel" => Some("audio_voice_reel_validated"),
        "adaptive_music_system" => Some("audio_adaptive_system_shipped"),
        "audio_programming" => Some("audio_programming_contribution"),
        _ => None,
    }
}

/// Whether this basis may only be issued once the sources are declared.
fn needs_declared_sources(basis: &str) -> bool {
    matches!(
        basis,
        "audio_composition_published" | "audio_soundpack_delivered"
    )
}

/// The skills the slice already says it touches.
///
/// Attached when they exist, left empty when they do not. Nothing is derived:
/// an `artefact` attestation rests on the deliverable it names, so a slice
/// nobody tagged still produces one rather than being filed under a skill
/// chosen on its behalf.
async fn skill_nodes_for_slice(db: &PgPool, slice_id: Uuid) -> Result<Vec<Uuid>, AppError> {
    Ok(sqlx::query_scalar(
        r#"
        SELECT ss.skill_id
          FROM slice_skills ss
          JOIN skill_nodes sn ON sn.id = ss.skill_id
         WHERE ss.slice_id = $1
         ORDER BY sn.slug
        "#,
    )
    .bind(slice_id)
    .fetch_all(db)
    .await?)
}

/// Insert an attestation, or do nothing if this artefact already carries one
/// on the same basis.
///
/// Uniqueness is enforced by index rather than by a preceding SELECT, so two
/// concurrent hooks cannot both decide the attestation is missing.
async fn issue(
    db: &PgPool,
    user_id: Uuid,
    basis: &str,
    deliverable_id: Uuid,
    skill_node_ids: &[Uuid],
) -> Result<Option<Uuid>, AppError> {
    let (title, description) = crate::services::attestations::basis_wording(db, basis).await;
    let code = crate::services::attestations::AttestationsService::generate_verification_code();

    let id: Option<Uuid> = sqlx::query_scalar(
        r#"
        INSERT INTO attestations (
            user_id, attestation_type, title, description,
            linked_deliverable_ids, linked_skill_node_ids,
            verification_code, basis
        )
        VALUES ($1, 'artefact', $2, $3, ARRAY[$4], $5, $6, $7)
        ON CONFLICT DO NOTHING
        RETURNING id
        "#,
    )
    .bind(user_id)
    .bind(&title)
    .bind(&description)
    .bind(deliverable_id)
    .bind(skill_node_ids)
    .bind(&code)
    .bind(basis)
    .fetch_optional(db)
    .await?;

    if let Some(id) = id {
        announce(db, user_id, id, basis, &title, deliverable_id).await;
    }

    Ok(id)
}

/// Put the attestation on the public feed, if the person allows it.
///
/// Best-effort and never fatal. The address is the one the work already had —
/// the public hosting URL when the author named one, the deliverable's own
/// otherwise. A feed line with nothing to open is the fabricated social proof
/// migration 0203 exists to replace, and in this domain "nothing to open"
/// would also mean nothing to listen to.
async fn announce(
    db: &PgPool,
    user_id: Uuid,
    attestation_id: Uuid,
    basis: &str,
    title: &str,
    deliverable_id: Uuid,
) {
    /// Who published it and where it can be heard. Named rather than left as
    /// a three-tuple: two of the three are optional URLs, and a caller reading
    /// `.2` instead of `.1` would attach the wrong link.
    #[derive(sqlx::FromRow)]
    struct Context {
        username: String,
        audio_external_hosting_url: Option<String>,
        artifact_url: Option<String>,
    }

    let context: Result<Option<Context>, _> = sqlx::query_as(
        r#"
        SELECT u.username, ps.audio_external_hosting_url, d.artifact_url
          FROM deliverables d
          JOIN users u ON u.id = d.user_id
          LEFT JOIN project_slices ps ON ps.id = d.slice_id
         WHERE d.id = $1
        "#,
    )
    .bind(deliverable_id)
    .fetch_optional(db)
    .await;

    let Ok(Some(Context {
        username,
        audio_external_hosting_url: hosting_url,
        artifact_url,
    })) = context
    else {
        return;
    };
    let Some(url) = hosting_url.or(artifact_url) else {
        return;
    };

    let outcome = crate::services::public_feed::emit(
        db,
        crate::services::public_feed::Emission {
            kind: "attestation_issued",
            subject_type: "user",
            subject_id: user_id,
            subject_label: &username,
            headline: format!("{title} — {username}"),
            artifact_url: url,
            repository: None,
            amount: None,
            currency: None,
            source_type: "attestation",
            source_id: attestation_id,
        },
    )
    .await;

    if let Err(e) = outcome {
        tracing::warn!(
            attestation = %attestation_id, basis, error = %e,
            "audio attestation issued but not announced"
        );
    }
}

/// Whether the author has stated the source list is complete.
///
/// Reads the declaration, not the row count: a wholly original track has no
/// licence rows and a declaration, and a track nobody documented has neither.
/// Those two must not read the same to something about to assert publicly that
/// the sources are in order.
async fn sources_are_declared(db: &PgPool, slice_id: Uuid) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT audio_sources_declared_at IS NOT NULL
           FROM project_slices WHERE id = $1",
    )
    .bind(slice_id)
    .fetch_optional(db)
    .await?
    .unwrap_or(false))
}

/// Issue whatever the verified work on this slice earns.
///
/// Returns the bases actually issued, which is empty on a second pass and
/// empty for work that earns none — both normal, neither an error.
pub async fn issue_for_slice(db: &PgPool, slice_id: Uuid) -> Result<Vec<String>, AppError> {
    // The verified, unrevoked deliverable is the evidence. Without one there
    // is nothing to attest, whatever the slice claims about itself.
    let evidence: Option<(Uuid, Uuid, Option<String>, String)> = sqlx::query_as(
        r#"
        SELECT d.id, d.user_id, ps.audio_subtype, ps.slice_type
          FROM project_slices ps
          JOIN deliverables d ON d.slice_id = ps.id
         WHERE ps.id = $1
           AND d.verification_status = 'verified'
           AND d.revoked_at IS NULL
         ORDER BY d.verified_at ASC
         LIMIT 1
        "#,
    )
    .bind(slice_id)
    .fetch_optional(db)
    .await?;

    let Some((deliverable_id, user_id, subtype, slice_type)) = evidence else {
        return Ok(Vec::new());
    };
    if slice_type != "audio_artifact" {
        return Ok(Vec::new());
    }

    let Some(basis) = subtype.as_deref().and_then(basis_for_subtype) else {
        return Ok(Vec::new());
    };

    if needs_declared_sources(basis) && !sources_are_declared(db, slice_id).await? {
        // Not an error: the work is fine and the declaration is missing. The
        // next pass issues it, which is what makes filling the form in later
        // work without anybody re-triggering anything.
        tracing::debug!(
            slice = %slice_id, basis,
            "audio attestation withheld until the sources are declared"
        );
        return Ok(Vec::new());
    }

    let skills = skill_nodes_for_slice(db, slice_id).await?;
    let mut issued = Vec::new();
    if issue(db, user_id, basis, deliverable_id, &skills)
        .await?
        .is_some()
    {
        issued.push(basis.to_string());
    }
    Ok(issued)
}

/// Issue whatever every audio artefact this person has earns.
///
/// Called from the proof orchestrator rather than from the point a slice is
/// verified, and deliberately: the licence declaration usually arrives *after*
/// verification, and hooking the verification alone would leave every
/// composition permanently unattested — the dormant-engine failure P19 exists
/// to end.
///
/// Bounded: somebody with more audio artefacts than this has a profile that
/// needs a look rather than a longer loop, and the next recompute picks up
/// whatever this pass did not reach.
pub async fn issue_for_user(db: &PgPool, user_id: Uuid) -> Result<Vec<String>, AppError> {
    let slices: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT ps.id
          FROM project_slices ps
          JOIN deliverables d ON d.slice_id = ps.id
         WHERE d.user_id = $1
           AND ps.slice_type = 'audio_artifact'
           AND d.verification_status = 'verified'
           AND d.revoked_at IS NULL
         LIMIT 200
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let mut issued = Vec::new();
    for slice_id in slices {
        // One failing slice does not stop the others: a missing declaration on
        // one artefact must not cost somebody the attestation on another.
        match issue_for_slice(db, slice_id).await {
            Ok(mut bases) => issued.append(&mut bases),
            Err(e) => tracing::warn!(
                slice = %slice_id, error = %e,
                "audio attestation generator failed on one artefact"
            ),
        }
    }
    Ok(issued)
}

/// Attest a credit on somebody else's released work.
///
/// Separate from the automatic generators because nothing here can see a
/// credit: it lives in a game's end titles or a podcast description, and the
/// only way to know is that a person followed the link. `evidence_url` is that
/// link, and it is stored on the attestation so a reader can follow it too.
///
/// The caller is responsible for the permission check — this is reached
/// through an endpoint guarded by `audio_reviewer:*`.
pub async fn issue_credit(
    db: &PgPool,
    user_id: Uuid,
    deliverable_id: Uuid,
    evidence_url: &str,
) -> Result<Option<Uuid>, AppError> {
    crate::validators::validate_url(evidence_url, "evidence_url", 500)?;
    if evidence_url.trim().is_empty() {
        return Err(AppError::Validation(
            "a credit has to point at where the credit appears".into(),
        ));
    }

    // The deliverable has to be this person's, and verified. A credit attested
    // against somebody else's work would put their artefact on this profile.
    let owns: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM deliverables
             WHERE id = $1 AND user_id = $2
               AND verification_status = 'verified' AND revoked_at IS NULL
        )
        "#,
    )
    .bind(deliverable_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if !owns {
        return Err(AppError::Validation(
            "no verified deliverable of this person carries that id".into(),
        ));
    }

    let basis = "audio_project_credited";
    let (title, description) = crate::services::attestations::basis_wording(db, basis).await;
    let code = crate::services::attestations::AttestationsService::generate_verification_code();

    let id: Option<Uuid> = sqlx::query_scalar(
        r#"
        INSERT INTO attestations (
            user_id, attestation_type, title, description,
            linked_deliverable_ids, verification_code, basis, evidence_url
        )
        VALUES ($1, 'artefact', $2, $3, ARRAY[$4], $5, $6, $7)
        ON CONFLICT DO NOTHING
        RETURNING id
        "#,
    )
    .bind(user_id)
    .bind(&title)
    .bind(&description)
    .bind(deliverable_id)
    .bind(&code)
    .bind(basis)
    .bind(evidence_url)
    .fetch_optional(db)
    .await?;

    if let Some(id) = id {
        announce(db, user_id, id, basis, &title, deliverable_id).await;
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_soundscape_and_a_pack_make_the_same_claim() {
        // Both are a delivery of designed sound. A basis of its own would
        // split one count across two words for a distinction nobody hiring
        // asks about.
        assert_eq!(
            basis_for_subtype("ambient_soundscape"),
            Some("audio_soundpack_delivered")
        );
        assert_eq!(
            basis_for_subtype("sound_pack"),
            Some("audio_soundpack_delivered")
        );
    }

    #[test]
    fn every_subtype_the_schema_allows_earns_something() {
        // The list is migration 0508's CHECK. A subtype added there and
        // forgotten here would produce artefacts that verify and never attest.
        for subtype in [
            "composition",
            "sound_pack",
            "voice_reel",
            "adaptive_music_system",
            "audio_programming",
            "ambient_soundscape",
        ] {
            assert!(
                basis_for_subtype(subtype).is_some(),
                "{subtype} earns no attestation"
            );
        }
        assert_eq!(basis_for_subtype("something-else"), None);
    }

    #[test]
    fn only_what_redistributes_other_peoples_material_is_gated() {
        assert!(needs_declared_sources("audio_composition_published"));
        assert!(needs_declared_sources("audio_soundpack_delivered"));
        // Gating these would make the declaration a formality people click
        // through, which is how a gate stops meaning anything.
        assert!(!needs_declared_sources("audio_voice_reel_validated"));
        assert!(!needs_declared_sources("audio_adaptive_system_shipped"));
        assert!(!needs_declared_sources("audio_programming_contribution"));
    }
}
