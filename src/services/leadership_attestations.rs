//! Attestations that rest on a leadership artefact.
//!
//! ## The gate this domain has and the others do not
//!
//! Audio withholds an attestation until the sources are declared. Quality
//! withholds one until the fix is confirmed. Here the gate is **redaction**:
//! an artefact declared `anonymised` is not attested until a reviewer has
//! confirmed that nobody in it is identifiable.
//!
//! It is the strictest gate on the platform, and it is strict in the
//! direction of the people who did not choose to be written about. A roadmap
//! naming an unreleased product, an RFC naming a system's weaknesses, a team
//! health audit naming a team — publishing any of those because somebody
//! ticked a box is a harm the author cannot take back, and it is not the
//! author's alone to risk.
//!
//! `public` needs no confirmation: the author is saying the document was
//! already publishable, which is a claim about their own material.
//! `confidential` needs no confirmation either, because nothing is published —
//! what the attestation carries is the abstract claim in
//! `leadership_context`.
//!
//! ## Two attestations from one column
//!
//! A written decision earns `leadership_decision_recorded` on being verified,
//! whatever happened to it. One the organisation adopted earns
//! `leadership_rfc_accepted` as well. Both, not one: a domain that attests
//! only accepted proposals teaches people to propose what will pass, and the
//! hardest technical writing anybody does here will be the rejected kind.
//!
//! ## What is not issued from a slice
//!
//! Two things, and both for the same reason — nothing in a slice can see
//! them.
//!
//! `leadership_cohort_completed` rests on a cohort having been run to its end
//! with most of the people who joined finishing. That is rows in
//! `cohort_outcomes`, not a document, and [`issue_cohort_outcomes`]
//! reads them.
//!
//! `leadership_community_initiative_impact` rests on a number having moved
//! somewhere we cannot see. A reviewer who followed the evidence issues it
//! through [`issue_community_impact`], the same way audio credits are issued.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Which basis a leadership artefact supports, from what it is.
///
/// Four subtypes share `leadership_roadmap_validated`, and that is deliberate
/// where the quality domain split its two plan bases. A roadmap, a delivery
/// plan, an OKR document and a product spec all answer one question — what a
/// period will be spent on, and what it will not be — at four granularities,
/// and they are read by the same reviewer family. Four counts for one
/// competence would tell a recruiter less, not more.
///
/// Three share `leadership_people_framework_validated` for the same reason: a
/// ladder, a hiring loop and a health audit are all a structure people are
/// assessed or grown inside.
pub fn basis_for_subtype(subtype: &str) -> Option<&'static str> {
    match subtype {
        "roadmap" | "delivery_plan" | "okrs_doc" | "prd" => Some("leadership_roadmap_validated"),
        "rfc" | "adr" => Some("leadership_decision_recorded"),
        "retrospective" => Some("leadership_retrospective_facilitated"),
        "playbook" | "community_strategy" | "cohort_curriculum" => {
            Some("leadership_playbook_published")
        }
        "career_ladder" | "hiring_process" | "team_health_audit" => {
            Some("leadership_people_framework_validated")
        }
        _ => None,
    }
}

/// Whether this artefact also earns the adoption basis.
///
/// Only a written decision can be adopted. A roadmap is followed or it is
/// not, and there is no moment where somebody says so.
fn adoption_basis_for_subtype(subtype: &str) -> Option<&'static str> {
    match subtype {
        "rfc" | "adr" => Some("leadership_rfc_accepted"),
        _ => None,
    }
}

/// Whether a redaction state means somebody other than the author has to have
/// read the document before it is attested.
///
/// Only `anonymised`. `public` is a claim about the author's own material;
/// `confidential` publishes nothing.
fn needs_redaction_confirmed(redaction_state: &str) -> bool {
    redaction_state == "anonymised"
}

/// What the slice says about itself, for the decisions above.
#[derive(sqlx::FromRow)]
struct SliceFacts {
    deliverable_id: Uuid,
    user_id: Uuid,
    slice_type: String,
    leadership_subtype: Option<String>,
    redaction_state: Option<String>,
    redaction_confirmed_at: Option<chrono::DateTime<chrono::Utc>>,
    leadership_adopted_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// The skills the slice already says it touches.
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

/// Put the attestation on the public feed, when there is something to open.
///
/// A confidential artefact is never announced. The feed line would have to
/// point at something, and the only honest link would be the profile — which
/// turns the feed into a stream of claims with nothing behind them, the exact
/// thing migration 0203 replaced.
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
        redaction_state: Option<String>,
    }

    let context: Result<Option<Context>, _> = sqlx::query_as(
        r#"
        SELECT u.username, d.artifact_url, ps.redaction_state
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
        artifact_url,
        redaction_state,
    })) = context
    else {
        return;
    };

    if redaction_state.as_deref() == Some("confidential") {
        return;
    }
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
            "leadership attestation issued but not announced"
        );
    }
}

/// Whether the retrospective behind this slice actually landed.
///
/// Reads `leadership_retrospective_followthrough`, which is where the
/// seventy-per-cent-in-ninety-days rule lives. Putting the rule in the view
/// rather than here means the attestation and any dashboard read the same
/// number.
async fn retrospective_followed_through(db: &PgPool, slice_id: Uuid) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar::<_, bool>(
        r#"
        SELECT COALESCE(BOOL_OR(followed_through), FALSE)
          FROM leadership_retrospective_followthrough
         WHERE slice_id = $1
        "#,
    )
    .bind(slice_id)
    .fetch_one(db)
    .await?)
}

/// Issue whatever the verified work on this slice earns.
///
/// Can return two bases: a written decision that was adopted earns both the
/// record and the acceptance.
pub async fn issue_for_slice(db: &PgPool, slice_id: Uuid) -> Result<Vec<String>, AppError> {
    let facts: Option<SliceFacts> = sqlx::query_as(
        r#"
        SELECT d.id AS deliverable_id, d.user_id, ps.slice_type,
               ps.leadership_subtype, ps.redaction_state,
               ps.redaction_confirmed_at, ps.leadership_adopted_at
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

    let Some(f) = facts else {
        return Ok(Vec::new());
    };
    if f.slice_type != "leadership_artifact" {
        return Ok(Vec::new());
    }

    let Some(subtype) = f.leadership_subtype.as_deref() else {
        return Ok(Vec::new());
    };
    let Some(basis) = basis_for_subtype(subtype) else {
        return Ok(Vec::new());
    };

    // The redaction gate. Not an error: the document is fine and a reviewer
    // has not read it yet, so the next pass issues it.
    let redaction = f.redaction_state.as_deref().unwrap_or("public");
    if needs_redaction_confirmed(redaction) && f.redaction_confirmed_at.is_none() {
        tracing::debug!(
            slice = %slice_id, basis,
            "leadership attestation withheld until the redaction is confirmed"
        );
        return Ok(Vec::new());
    }

    // A retrospective is not attested for having happened. It is attested for
    // its action items having been resolved.
    if basis == "leadership_retrospective_facilitated"
        && !retrospective_followed_through(db, slice_id).await?
    {
        tracing::debug!(
            slice = %slice_id,
            "retrospective attestation withheld until its actions are resolved"
        );
        return Ok(Vec::new());
    }

    let skills = skill_nodes_for_slice(db, slice_id).await?;
    let mut issued = Vec::new();

    if issue(db, f.user_id, basis, f.deliverable_id, &skills)
        .await?
        .is_some()
    {
        issued.push(basis.to_string());
    }

    // The second one, when the organisation took it up.
    if let Some(adoption) = adoption_basis_for_subtype(subtype)
        && f.leadership_adopted_at.is_some()
        && issue(db, f.user_id, adoption, f.deliverable_id, &skills)
            .await?
            .is_some()
    {
        issued.push(adoption.to_string());
    }

    Ok(issued)
}

/// Issue whatever every leadership artefact this person has earns.
///
/// Called from the proof orchestrator rather than at verification, and
/// deliberately: a redaction confirmation and a retrospective's action items
/// both arrive after the deliverable was verified, and hooking verification
/// alone would leave those permanently unattested.
pub async fn issue_for_user(db: &PgPool, user_id: Uuid) -> Result<Vec<String>, AppError> {
    let slices: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT ps.id
          FROM project_slices ps
          JOIN deliverables d ON d.slice_id = ps.id
         WHERE d.user_id = $1
           AND ps.slice_type = 'leadership_artifact'
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
        match issue_for_slice(db, slice_id).await {
            Ok(mut bases) => issued.append(&mut bases),
            Err(e) => tracing::warn!(
                slice = %slice_id, error = %e,
                "leadership attestation generator failed on one artefact"
            ),
        }
    }

    match issue_cohort_outcomes(db, user_id).await {
        Ok(mut bases) => issued.append(&mut bases),
        Err(e) => tracing::warn!(
            user_id = %user_id, error = %e,
            "cohort attestation generator failed"
        ),
    }

    Ok(issued)
}

/// Attest the cohorts this person led to their end.
///
/// Not derived from a slice, because a cohort is not a document. The rule —
/// concluded, at least three people, seventy per cent of the ones not lost to
/// a job finishing — lives in `cohort_outcomes`, so this reads a
/// boolean rather than reimplementing arithmetic a view already does.
///
/// The attestation links the curriculum's deliverable when the cohort names
/// one, and links nothing when it does not. `leadership_cohort_completed` has
/// `requires_deliverable = FALSE` for exactly that: a cohort run from a plan
/// nobody wrote down is still a cohort that graduated people.
pub async fn issue_cohort_outcomes(db: &PgPool, user_id: Uuid) -> Result<Vec<String>, AppError> {
    let cohorts: Vec<(Uuid, Option<Uuid>)> = sqlx::query_as(
        r#"
        SELECT o.cohort_id, o.curriculum_slice_id
          FROM cohort_outcomes o
         WHERE o.led_by_user_id = $1
           AND o.led_to_the_end
         LIMIT 100
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let basis = "leadership_cohort_completed";
    let mut issued = Vec::new();

    for (cohort_id, curriculum_slice_id) in cohorts {
        // The deliverable behind the curriculum, when there is one. A missing
        // one is normal and not an error.
        let deliverable: Option<Uuid> = match curriculum_slice_id {
            Some(slice_id) => {
                sqlx::query_scalar(
                    "SELECT id FROM deliverables
                      WHERE slice_id = $1 AND user_id = $2
                        AND verification_status = 'verified' AND revoked_at IS NULL
                      ORDER BY verified_at ASC LIMIT 1",
                )
                .bind(slice_id)
                .bind(user_id)
                .fetch_optional(db)
                .await?
            }
            None => None,
        };

        // Keyed on the cohort rather than on the deliverable: somebody who
        // runs the same curriculum three times has led three cohorts, and a
        // uniqueness rule on the document would have attested one.
        let already: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM attestations
                  WHERE user_id = $1 AND basis = $2 AND revoked_at IS NULL
                    AND $3 = ANY (linked_project_ids))",
        )
        .bind(user_id)
        .bind(basis)
        .bind(cohort_id)
        .fetch_one(db)
        .await?;

        if already {
            continue;
        }

        let (title, description) = crate::services::attestations::basis_wording(db, basis).await;
        let code = crate::services::attestations::AttestationsService::generate_verification_code();

        let deliverables: Vec<Uuid> = deliverable.into_iter().collect();

        let inserted: Option<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO attestations (
                user_id, attestation_type, title, description,
                linked_deliverable_ids, linked_project_ids,
                verification_code, basis
            )
            VALUES ($1, 'artefact', $2, $3, $4, ARRAY[$5], $6, $7)
            ON CONFLICT DO NOTHING
            RETURNING id
            "#,
        )
        .bind(user_id)
        .bind(&title)
        .bind(&description)
        .bind(&deliverables)
        .bind(cohort_id)
        .bind(&code)
        .bind(basis)
        .fetch_optional(db)
        .await?;

        if inserted.is_some() {
            issued.push(basis.to_string());
        }
    }

    Ok(issued)
}

/// What this domain may issue editorially or on a reviewer's word.
///
/// Two bases rather than the usual one. A community initiative's effect is a
/// number that moved somewhere we cannot see — a Discord's retention, a
/// project's contributor count, an event's attendance — and no row here can
/// read it. A reviewer who followed the evidence issues it, and the evidence
/// URL is stored on the attestation so a reader can follow it too.
const EDITORIAL: crate::services::artefact_attestations::Domain =
    crate::services::artefact_attestations::Domain {
        name: "leadership",
        bases: &["featured_leader", "leadership_community_initiative_impact"],
        artifact_bases: &[],
        allows_stored_objects: false,
    };

/// Featured.
pub async fn featured_leader(
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
        "featured_leader",
        &crate::services::artefact_attestations::Evidence {
            url: profile_url.to_string(),
            title: "Featured leader".into(),
            description: citation.trim().to_string(),
            deliverable_id: None,
            project_id: None,
            skill_node_ids: vec![],
        },
        &EDITORIAL,
    )
    .await
}

/// Attest a community initiative that moved a number.
///
/// Issued by a reviewer who followed the evidence, for the reason at
/// [`EDITORIAL`]. The caller is responsible for the permission check — this is
/// reached through an endpoint guarded by `leadership_reviewer:community`.
///
/// `what_moved` is required and is the whole substance of the claim: "the
/// community grew" is what this attestation exists to refuse.
pub async fn issue_community_impact(
    db: &PgPool,
    user_id: Uuid,
    evidence_url: &str,
    what_moved: &str,
) -> Result<crate::services::artefact_attestations::Issued, AppError> {
    crate::validators::validate_url(evidence_url, "evidence_url", 500)?;
    if what_moved.trim().len() < 20 {
        return Err(AppError::Validation(
            "say which number moved, from what to what, over what period — \
             \"the community grew\" is the claim this refuses"
                .into(),
        ));
    }

    crate::services::artefact_attestations::issue(
        db,
        user_id,
        "leadership_community_initiative_impact",
        &crate::services::artefact_attestations::Evidence {
            url: evidence_url.to_string(),
            title: "Community initiative with an effect".into(),
            description: what_moved.trim().to_string(),
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
        // Migration 0460's CHECK. A subtype added there and forgotten here
        // produces artefacts that verify and never attest — silently, and
        // visible only as a profile that stays empty.
        for subtype in [
            "roadmap",
            "prd",
            "rfc",
            "adr",
            "delivery_plan",
            "retrospective",
            "playbook",
            "career_ladder",
            "hiring_process",
            "team_health_audit",
            "community_strategy",
            "cohort_curriculum",
            "okrs_doc",
        ] {
            assert!(
                basis_for_subtype(subtype).is_some(),
                "{subtype} earns no attestation"
            );
        }
        assert_eq!(basis_for_subtype("something-else"), None);
    }

    #[test]
    fn four_plans_make_one_claim() {
        // A roadmap, a delivery plan, an OKR document and a product spec all
        // answer "what will this period be spent on, and what will it not".
        // Four counts for one competence tells a recruiter less, not more.
        let roadmap = basis_for_subtype("roadmap");
        for subtype in ["delivery_plan", "okrs_doc", "prd"] {
            assert_eq!(basis_for_subtype(subtype), roadmap);
        }
    }

    #[test]
    fn only_a_written_decision_can_be_adopted() {
        // A roadmap is followed or it is not, and there is no moment where
        // somebody says so.
        assert_eq!(
            adoption_basis_for_subtype("rfc"),
            Some("leadership_rfc_accepted")
        );
        assert_eq!(
            adoption_basis_for_subtype("adr"),
            Some("leadership_rfc_accepted")
        );
        assert_eq!(adoption_basis_for_subtype("roadmap"), None);
        assert_eq!(adoption_basis_for_subtype("playbook"), None);
    }

    #[test]
    fn only_an_anonymised_document_waits_for_a_second_reader() {
        assert!(needs_redaction_confirmed("anonymised"));
        // A claim about the author's own publishable material.
        assert!(!needs_redaction_confirmed("public"));
        // Nothing is published, so there is nothing to confirm.
        assert!(!needs_redaction_confirmed("confidential"));
    }
}
