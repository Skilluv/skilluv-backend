//! Attestations that rest on a quality artefact.
//!
//! ## The same shape as the AI and audio generators, and one difference
//!
//! Like [`crate::services::audio_attestations`], every generator re-checks its
//! own precondition against the database before issuing, the attestations are
//! `artefact` ones (migration 0198), and re-running is free because
//! `uniq_attestations_artefact_per_deliverable` makes a second pass insert
//! nothing.
//!
//! The difference is what gates a bug report. Audio withholds an attestation
//! until the sources are declared, because a track with untraced samples
//! cannot be shipped. Here the gate is the **fix**: a defect report is not a
//! quality artefact because it was written well, it is one because somebody
//! else's fix shipped and the person who found it went back and checked. Until
//! `fix_confirmed_at` is set, the work may be perfectly good and there is
//! nothing to attest yet.
//!
//! That is the one place in this domain where the attestation waits on
//! somebody who is not the author, and it is deliberate: it is also the only
//! basis here whose claim is about an outcome rather than a document.
//!
//! ## Why `test_strategy` and `test_plan` do not share a basis
//!
//! They are the same shape of document at two scales, and the backlog mapped
//! both onto `quality_test_plan_validated`. A recruiter filtering for somebody
//! who can set a team's testing direction would then have been handed every
//! person who has ever covered one feature. Two bases, and the weights say
//! which one commits more.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Which basis a quality artefact supports, from what it is.
///
/// `a11y_audit` has its own rather than folding into the usability one. They
/// are different methods against different references — one is a protocol with
/// participants, the other is a standard with numbered criteria — and the only
/// thing they share is that a person did the work rather than a tool.
pub fn basis_for_subtype(subtype: &str) -> Option<&'static str> {
    match subtype {
        "test_plan" => Some("quality_test_plan_validated"),
        "test_strategy" => Some("quality_test_strategy_validated"),
        "test_automation" => Some("quality_automation_shipped"),
        "bug_report" => Some("quality_bug_report_validated"),
        "usability_study" => Some("quality_usability_study_completed"),
        "a11y_audit" => Some("quality_a11y_audit_delivered"),
        "playtest_report" => Some("quality_playtest_report_validated"),
        "coverage_analysis" => Some("quality_coverage_analysis_accepted"),
        _ => None,
    }
}

/// Whether this basis waits on somebody other than the author.
///
/// One basis does. See the module header.
fn needs_confirmed_fix(basis: &str) -> bool {
    basis == "quality_bug_report_validated"
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

/// Whether this slice carries a bug report whose fix has been confirmed and
/// which no reviewer rejected.
///
/// Reads the confirmation, not the fix link: a merged pull request is somebody
/// else's claim, and `fix_confirmed_at` is the reporter having gone back and
/// looked. Those two must not read the same to something about to assert
/// publicly that a defect was real.
async fn fix_is_confirmed(db: &PgPool, slice_id: Uuid) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM quality_bug_reports
             WHERE slice_id = $1
               AND fix_confirmed_at IS NOT NULL
               AND rejected_reason IS NULL
        )
        "#,
    )
    .bind(slice_id)
    .fetch_one(db)
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

/// Put the attestation on the public feed, if there is something to open.
///
/// Best-effort and never fatal. A feed line with nothing behind it is the
/// fabricated social proof migration 0203 exists to replace, and in this
/// domain "nothing to open" would mean a claim about testing that a reader
/// cannot check — which is the one thing this domain sells against.
async fn announce(
    db: &PgPool,
    user_id: Uuid,
    attestation_id: Uuid,
    basis: &str,
    title: &str,
    deliverable_id: Uuid,
) {
    #[derive(sqlx::FromRow)]
    struct Context {
        username: String,
        artifact_url: Option<String>,
    }

    let context: Result<Option<Context>, _> = sqlx::query_as(
        r#"
        SELECT u.username, d.artifact_url
          FROM deliverables d
          JOIN users u ON u.id = d.user_id
         WHERE d.id = $1
        "#,
    )
    .bind(deliverable_id)
    .fetch_optional(db)
    .await;

    let Ok(Some(Context {
        username,
        artifact_url,
    })) = context
    else {
        return;
    };
    let Some(url) = artifact_url else {
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
            "quality attestation issued but not announced"
        );
    }
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
        SELECT d.id, d.user_id, ps.qa_subtype, ps.slice_type
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
    if slice_type != "qa_report" {
        return Ok(Vec::new());
    }

    let Some(basis) = subtype.as_deref().and_then(basis_for_subtype) else {
        return Ok(Vec::new());
    };

    if needs_confirmed_fix(basis) && !fix_is_confirmed(db, slice_id).await? {
        // Not an error: the report may be excellent and the fix has not
        // shipped yet, or the reporter has not gone back to look. The next
        // pass issues it, which is what makes confirming later work without
        // anybody re-triggering anything.
        tracing::debug!(
            slice = %slice_id, basis,
            "quality attestation withheld until the fix is confirmed"
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

/// Issue whatever every quality artefact this person has earns.
///
/// Called from the proof orchestrator rather than from the point a slice is
/// verified, and deliberately: a fix confirmation usually arrives *after*
/// verification, and hooking the verification alone would leave every
/// confirmed defect permanently unattested — the dormant-engine failure P19
/// exists to end.
///
/// Bounded: somebody with more quality artefacts than this has a profile that
/// needs a look rather than a longer loop, and the next recompute picks up
/// whatever this pass did not reach.
pub async fn issue_for_user(db: &PgPool, user_id: Uuid) -> Result<Vec<String>, AppError> {
    let slices: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT ps.id
          FROM project_slices ps
          JOIN deliverables d ON d.slice_id = ps.id
         WHERE d.user_id = $1
           AND ps.slice_type = 'qa_report'
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
        // One failing slice does not stop the others: an unconfirmed fix on
        // one report must not cost somebody the attestation on another.
        match issue_for_slice(db, slice_id).await {
            Ok(mut bases) => issued.append(&mut bases),
            Err(e) => tracing::warn!(
                slice = %slice_id, error = %e,
                "quality attestation generator failed on one artefact"
            ),
        }
    }
    Ok(issued)
}

/// What this domain may issue editorially.
///
/// Only the one basis. Everything else quality issues rests on a verified
/// deliverable and is written by the generators above; a featuring rests on
/// somebody's judgement, and says so.
const EDITORIAL: crate::services::artefact_attestations::Domain =
    crate::services::artefact_attestations::Domain {
        name: "quality",
        bases: &["featured_quality_engineer"],
        artifact_bases: &[],
        allows_stored_objects: false,
    };

/// Featured.
///
/// Editorial, and named as such. There is no formula behind it, and inventing
/// one would make it a worse version of the craft score rather than the thing
/// it is: somebody chose to put this person forward, and put their name to it.
pub async fn featured_quality_engineer(
    db: &PgPool,
    user_id: Uuid,
    profile_url: &str,
    citation: &str,
) -> Result<crate::services::artefact_attestations::Issued, AppError> {
    if citation.trim().is_empty() {
        return Err(AppError::Validation(
            "featuring somebody without saying why is a decision nobody can question".into(),
        ));
    }
    crate::services::artefact_attestations::issue(
        db,
        user_id,
        "featured_quality_engineer",
        &crate::services::artefact_attestations::Evidence {
            url: profile_url.to_string(),
            title: "Featured quality engineer".into(),
            description: citation.trim().to_string(),
            deliverable_id: None,
            project_id: None,
            skill_node_ids: vec![],
        },
        &EDITORIAL,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_subtype_the_schema_allows_earns_something() {
        // The list is migration 0450's CHECK. A subtype added there and
        // forgotten here would produce artefacts that verify and never attest
        // — silently, and only visible as a profile that stays empty.
        for subtype in [
            "test_plan",
            "test_automation",
            "bug_report",
            "usability_study",
            "a11y_audit",
            "playtest_report",
            "coverage_analysis",
            "test_strategy",
        ] {
            assert!(
                basis_for_subtype(subtype).is_some(),
                "{subtype} earns no attestation"
            );
        }
        assert_eq!(basis_for_subtype("something-else"), None);
    }

    #[test]
    fn a_strategy_and_a_plan_do_not_make_the_same_claim() {
        // The backlog mapped both onto the plan basis. A recruiter looking for
        // somebody who can set a team's direction would then have been handed
        // everybody who has ever covered one feature.
        assert_ne!(
            basis_for_subtype("test_strategy"),
            basis_for_subtype("test_plan")
        );
    }

    #[test]
    fn an_audit_and_a_study_do_not_make_the_same_claim() {
        // Different method, different reference, different evidence.
        assert_ne!(
            basis_for_subtype("a11y_audit"),
            basis_for_subtype("usability_study")
        );
    }

    #[test]
    fn only_the_outcome_basis_waits_on_somebody_else() {
        assert!(needs_confirmed_fix("quality_bug_report_validated"));
        // These rest on a document the author produced. Gating them on
        // anything external would make the gate a formality people wait out,
        // which is how a gate stops meaning anything.
        assert!(!needs_confirmed_fix("quality_test_plan_validated"));
        assert!(!needs_confirmed_fix("quality_automation_shipped"));
        assert!(!needs_confirmed_fix("quality_usability_study_completed"));
        assert!(!needs_confirmed_fix("quality_playtest_report_validated"));
    }
}
