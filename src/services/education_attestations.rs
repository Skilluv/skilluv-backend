//! Attestations that rest on an education artefact.
//!
//! ## The same shape as the other generators, and two gates that are new
//!
//! Like [`crate::services::ai_attestations`],
//! [`crate::services::audio_attestations`] and
//! [`crate::services::communication_attestations`], every generator re-checks
//! its own precondition against the database before issuing, the attestations
//! are `artefact` ones (migration 0198), and re-running is free because
//! `uniq_attestations_artefact_per_deliverable` makes a second pass insert
//! nothing.
//!
//! Two things are specific to this domain, and both are refusals.
//!
//! **The learner-data gate.** Every artefact here is about real people who
//! are not members, are sometimes minors, and never asked to be evidence in
//! somebody's portfolio. A cohort report with twenty names in it cannot be
//! published however good the teaching was. So nothing is attested until the
//! author has stated that no identifiable learner remains — the declaration
//! migration 0523 added, on the model of audio's source list in 0410. It is a
//! statement rather than an inferred check for the reason 0410 gave: a report
//! with no names and a declaration, and a report nobody looked at, have the
//! same row count.
//!
//! **The outcome gate.** `education_cohort_delivered` claims a cohort ran to
//! the end with measured outcomes, and that claim is checkable: the cohort is
//! a row, its learners are rows, and completion is a count.
//! `education_cohort_meets_threshold` (migration 0531) is where the policy
//! lives, in the schema rather than here, so an operator can move it without
//! a deployment.
//!
//! ## What is deliberately not attested
//!
//! Teaching hours. The backlog asked for `education_students_taught`, issued
//! on a measured headcount; migrations 0521 and 0522 wrote out at length why
//! there is nothing to measure. Teaching done off this platform is a
//! portfolio entry marked as declared and a craft-score term at a discount,
//! and that is the honest home for it.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Which basis an education artefact supports, from what it is.
///
/// `lesson_plan_series` earns nothing on its own, and that is not an
/// oversight. A set of lesson plans is a design artefact that nobody has run:
/// it is reviewable, it belongs on a profile, and the claim "this person
/// teaches" is not one it supports. It counts through the review grid and the
/// deliverable count, like any other verified work.
///
/// `students_outcome_report` maps to the cohort basis, because that is what
/// it reports on. It reaches the same gate.
fn basis_for_subtype(subtype: &str) -> Option<&'static str> {
    match subtype {
        "course_delivered" => Some("education_cohort_delivered"),
        "students_outcome_report" => Some("education_cohort_delivered"),
        "workshop_material" => Some("education_workshop_delivered"),
        "assessment_framework" => Some("education_assessment_framework_published"),
        // Issued through the adoption count rather than from the subtype: a
        // curriculum is attested when somebody else has run it.
        "curriculum_document" => None,
        "lesson_plan_series" => None,
        _ => None,
    }
}

/// How many other trainers have to have run a curriculum before it is
/// attested.
///
/// One. The basis says "authored and adopted", and the fact worth attesting
/// is that the author is not the only person who trusted it — which is true
/// at one and does not become more true at three. The badge of migration 0522
/// is where a higher bar belongs, because a badge can be rewritten without
/// reissuing what anybody was already told.
const ADOPTIONS_REQUIRED: i64 = 1;

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
    evidence_url: Option<&str>,
) -> Result<Option<Uuid>, AppError> {
    let (title, description) = crate::services::attestations::basis_wording(db, basis).await;
    let code = crate::services::attestations::AttestationsService::generate_verification_code();

    let id: Option<Uuid> = sqlx::query_scalar(
        r#"
        INSERT INTO attestations (
            user_id, attestation_type, title, description,
            linked_deliverable_ids, linked_skill_node_ids,
            verification_code, basis, evidence_url
        )
        VALUES ($1, 'artefact', $2, $3, ARRAY[$4], $5, $6, $7, $8)
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
    .bind(evidence_url)
    .fetch_optional(db)
    .await?;

    if let Some(id) = id {
        announce(db, user_id, id, basis, &title, deliverable_id).await;
    }

    Ok(id)
}

/// Put the attestation on the public feed, if the work has somewhere to point.
///
/// Best-effort and never fatal. A feed line with nothing to open is the
/// fabricated social proof migration 0203 exists to replace.
///
/// What is *not* put on the feed is anything about the learners. The headline
/// names the educator and the artefact; the cohort's completion rate and its
/// members stay in the tables that hold them.
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
        published_artifact_url: Option<String>,
        artifact_url: Option<String>,
    }

    let context: Result<Option<Context>, _> = sqlx::query_as(
        r#"
        SELECT u.username, ps.published_artifact_url, d.artifact_url
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
        published_artifact_url,
        artifact_url,
    })) = context
    else {
        return;
    };
    let Some(url) = published_artifact_url.or(artifact_url) else {
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
            "education attestation issued but not announced"
        );
    }
}

/// What one verified education slice is, as far as the generators care.
#[derive(sqlx::FromRow)]
struct Evidence {
    deliverable_id: Uuid,
    user_id: Uuid,
    subtype: Option<String>,
    slice_type: String,
    published_artifact_url: Option<String>,
    cohort_id: Option<Uuid>,
    learner_data_cleared: bool,
}

async fn evidence_for(db: &PgPool, slice_id: Uuid) -> Result<Option<Evidence>, AppError> {
    // The verified, unrevoked deliverable is the evidence. Without one there
    // is nothing to attest, whatever the slice claims about itself.
    Ok(sqlx::query_as::<_, Evidence>(
        r#"
        SELECT d.id AS deliverable_id, d.user_id, ps.education_subtype AS subtype,
               ps.slice_type, ps.published_artifact_url,
               ps.education_cohort_id AS cohort_id,
               ps.education_learner_data_cleared_at IS NOT NULL AS learner_data_cleared
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
    .await?)
}

/// Whether this basis may only be issued once the learner data is declared
/// clear.
///
/// The two that report on real learners. A workshop's materials — slides,
/// exercises, solutions — contain nobody by nature, and gating them would
/// make the declaration a formality people click through, which is how a gate
/// stops meaning anything.
fn needs_learner_data_declaration(subtype: &str) -> bool {
    matches!(subtype, "course_delivered" | "students_outcome_report")
}

/// Issue whatever the verified work on this slice earns.
///
/// Returns the bases actually issued, which is empty on a second pass and
/// empty for work that earns none — both normal, neither an error.
pub async fn issue_for_slice(db: &PgPool, slice_id: Uuid) -> Result<Vec<String>, AppError> {
    let Some(ev) = evidence_for(db, slice_id).await? else {
        return Ok(Vec::new());
    };
    if ev.slice_type != "education_artifact" {
        return Ok(Vec::new());
    }
    let Some(subtype) = ev.subtype.as_deref() else {
        return Ok(Vec::new());
    };

    // A curriculum is attested when somebody else has run it, which is a fact
    // about a different table.
    if subtype == "curriculum_document" {
        return issue_curriculum(db, &ev, slice_id).await;
    }

    let Some(basis) = basis_for_subtype(subtype) else {
        return Ok(Vec::new());
    };

    if needs_learner_data_declaration(subtype) && !ev.learner_data_cleared {
        // Not an error: the work may be fine and the declaration missing. The
        // next pass issues it, which is what makes signing it later work
        // without anybody re-triggering anything.
        tracing::debug!(
            slice = %slice_id, basis,
            "education attestation withheld until the learner data is declared clear"
        );
        return Ok(Vec::new());
    }

    if basis == "education_cohort_delivered" {
        let Some(cohort_id) = ev.cohort_id else {
            tracing::debug!(
                slice = %slice_id,
                "a delivered-course artefact naming no cohort attests nothing"
            );
            return Ok(Vec::new());
        };
        if !cohort_is_attestable(db, cohort_id, ev.user_id).await? {
            return Ok(Vec::new());
        }
    }

    let skills = skill_nodes_for_slice(db, slice_id).await?;
    let mut issued = Vec::new();
    if issue(
        db,
        ev.user_id,
        basis,
        ev.deliverable_id,
        &skills,
        ev.published_artifact_url.as_deref(),
    )
    .await?
    .is_some()
    {
        issued.push(basis.to_string());
    }
    Ok(issued)
}

/// Whether a cohort supports the claim its report makes.
///
/// Three things, and each is a different way the claim can be false: the
/// person attesting has to have led it, it has to have been concluded rather
/// than abandoned, and enough of the learners have to have finished with
/// somebody recording it. The threshold itself is
/// `education_cohort_meets_threshold` (migration 0531) — a policy, kept in the
/// schema where an operator can read and move it.
async fn cohort_is_attestable(
    db: &PgPool,
    cohort_id: Uuid,
    claimant: Uuid,
) -> Result<bool, AppError> {
    #[derive(sqlx::FromRow)]
    struct CohortState {
        led_by_claimant: bool,
        concluded: bool,
        meets_threshold: bool,
    }

    let state: Option<CohortState> = sqlx::query_as(
        r#"
        SELECT c.led_by_user_id = $2 AS led_by_claimant,
               c.concluded_at IS NOT NULL AS concluded,
               education_cohort_meets_threshold(c.id) AS meets_threshold
          FROM cohorts c
         WHERE c.id = $1
        "#,
    )
    .bind(cohort_id)
    .bind(claimant)
    .fetch_optional(db)
    .await?;

    let Some(state) = state else {
        return Ok(false);
    };

    if !state.led_by_claimant {
        // Somebody attesting a cohort they did not lead would put another
        // person's work on their profile.
        tracing::debug!(cohort = %cohort_id, "cohort attestation refused: not the teacher");
        return Ok(false);
    }
    if !state.concluded {
        tracing::debug!(cohort = %cohort_id, "cohort attestation withheld: not concluded");
        return Ok(false);
    }
    if !state.meets_threshold {
        tracing::debug!(
            cohort = %cohort_id,
            "cohort attestation withheld: outcomes unrecorded or below the threshold"
        );
        return Ok(false);
    }
    Ok(true)
}

/// A curriculum is attested once somebody else has run it.
///
/// The learner-data gate does not apply: a curriculum is a design document
/// and contains no learner by nature. What it waits for instead is adoption,
/// which is the fact the basis names.
async fn issue_curriculum(
    db: &PgPool,
    ev: &Evidence,
    slice_id: Uuid,
) -> Result<Vec<String>, AppError> {
    let adoptions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM education_curriculum_adoptions
          WHERE curriculum_slice_id = $1",
    )
    .bind(slice_id)
    .fetch_one(db)
    .await?;

    if adoptions < ADOPTIONS_REQUIRED {
        tracing::debug!(
            slice = %slice_id, adoptions,
            "curriculum attestation withheld until somebody else has run it"
        );
        return Ok(Vec::new());
    }

    let skills = skill_nodes_for_slice(db, slice_id).await?;
    let mut issued = Vec::new();
    if issue(
        db,
        ev.user_id,
        "education_curriculum_authored",
        ev.deliverable_id,
        &skills,
        ev.published_artifact_url.as_deref(),
    )
    .await?
    .is_some()
    {
        issued.push("education_curriculum_authored".to_string());
    }
    Ok(issued)
}

/// Issue whatever every education artefact this person has earns.
///
/// Called from the proof orchestrator rather than from the point a slice is
/// verified, and deliberately: the learner-data declaration, the cohort's
/// conclusion and the first adoption all arrive *after* verification — a
/// curriculum is published before anybody runs it — and hooking the
/// verification alone would leave every one of them permanently unattested,
/// which is the dormant-engine failure P19 exists to end.
///
/// Bounded: somebody with more education artefacts than this has a profile
/// that needs a look rather than a longer loop, and the next recompute picks
/// up whatever this pass did not reach.
pub async fn issue_for_user(db: &PgPool, user_id: Uuid) -> Result<Vec<String>, AppError> {
    let slices: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT ps.id
          FROM project_slices ps
          JOIN deliverables d ON d.slice_id = ps.id
         WHERE d.user_id = $1
           AND ps.slice_type = 'education_artifact'
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
                "education attestation generator failed on one artefact"
            ),
        }
    }
    Ok(issued)
}

// ════════════════════════════════════════════════════════════════════
// Featured
// ════════════════════════════════════════════════════════════════════

const EDITORIAL: crate::services::artefact_attestations::Domain =
    crate::services::artefact_attestations::Domain {
        name: "education",
        bases: &["featured_educator"],
        artifact_bases: &[],
        allows_stored_objects: false,
    };

/// Featured.
///
/// Migration 0521 declared `featured_educator` and
/// `education_profile` counts it, and until now nothing issued it: a featuring was
/// recorded, the announcement went out, and the profile term stayed at zero.
/// The same defect ops and audio carried, caught this time by the test that
/// was written for them.
pub async fn featured_educator(
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
        "featured_educator",
        &crate::services::artefact_attestations::Evidence {
            url: profile_url.to_string(),
            title: "Featured educator".into(),
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
    fn a_delivery_and_its_report_make_the_same_claim() {
        // Both say a cohort ran and it worked. A basis each would split one
        // count across two words, and both would be quoted.
        assert_eq!(
            basis_for_subtype("course_delivered"),
            Some("education_cohort_delivered")
        );
        assert_eq!(
            basis_for_subtype("students_outcome_report"),
            Some("education_cohort_delivered")
        );
    }

    #[test]
    fn design_artefacts_nobody_has_run_attest_nothing_by_themselves() {
        // A curriculum is attested through adoption, which is a different
        // table. A set of lesson plans is reviewable work and not a claim
        // that anybody was taught.
        assert_eq!(basis_for_subtype("curriculum_document"), None);
        assert_eq!(basis_for_subtype("lesson_plan_series"), None);
    }

    #[test]
    fn only_what_reports_on_real_learners_is_gated() {
        assert!(needs_learner_data_declaration("course_delivered"));
        assert!(needs_learner_data_declaration("students_outcome_report"));
        // Gating these would make the declaration a formality people click
        // through, which is how a gate stops meaning anything.
        assert!(!needs_learner_data_declaration("workshop_material"));
        assert!(!needs_learner_data_declaration("curriculum_document"));
        assert!(!needs_learner_data_declaration("lesson_plan_series"));
        assert!(!needs_learner_data_declaration("assessment_framework"));
    }

    #[test]
    fn every_subtype_the_schema_allows_is_accounted_for() {
        // The list is migration 0523's CHECK. A subtype added there and
        // forgotten here would produce artefacts that verify and never
        // attest, with nothing saying so.
        for subtype in [
            "course_delivered",
            "curriculum_document",
            "workshop_material",
            "lesson_plan_series",
            "assessment_framework",
            "students_outcome_report",
        ] {
            let handled = basis_for_subtype(subtype).is_some()
                || matches!(subtype, "curriculum_document" | "lesson_plan_series");
            assert!(handled, "{subtype} is not accounted for");
        }
    }
}
