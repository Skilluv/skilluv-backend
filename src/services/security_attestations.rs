//! Attestations that rest on security work.
//!
//! ## Four sources, not one
//!
//! Every other domain attests one thing: a verified deliverable on a slice.
//! This one attests four, and they are genuinely different objects:
//!
//!   * a **slice** — an audit, a threat model, a policy set. The ordinary case,
//!     and the only one the other domains have.
//!   * a **finding** — a reported vulnerability. Confirmed, then published, then
//!     possibly co-credited: three bases from one row, at three moments.
//!   * a **challenge** — a captured flag, a passed lab. No deliverable by
//!     design (migration 0546), so the uniqueness key is the challenge itself.
//!   * a **mission** — a paid engagement, which the mission machinery closes.
//!
//! ## What is redacted, and why the attestation still means something
//!
//! A confirmed finding is usually under embargo when its attestation is issued.
//! The evidence URL points at the finding's public card, which during an embargo
//! shows the severity, the weakness class, the date and the reporter and
//! withholds the reproduction. That is not a weaker proof — it is what a
//! coordinated disclosure looks like from outside, and it is exactly what a
//! recruiter needs: somebody found a critical in March and the details are not
//! yours to read yet.
//!
//! The alternative — waiting for publication before attesting — would mean a
//! researcher whose finding is embargoed for ninety days has nothing to show for
//! three months, and findings on systems that are never patched would never be
//! attested at all.

use sqlx::PgPool;
use uuid::Uuid;

use crate::config::PUBLIC_SITE_URL;
use crate::errors::AppError;
use crate::services::artefact_attestations::{self, Domain, Evidence, Links};

/// Every basis this domain issues, and which of them must name a deliverable.
///
/// The lists mirror `attestation_bases` (migration 0546). They are here as well
/// because the shared issuer refuses a basis it was not told about, which turns
/// a typo into a refusal at the call site instead of a row with a dangling
/// basis.
pub const SECURITY: Domain = Domain {
    name: "security",
    bases: &[
        "security_finding_confirmed",
        "security_finding_published",
        "security_finding_co_credit",
        "security_ctf_solved",
        "security_blue_lab_completed",
        "security_machine_walkthrough_validated",
        "security_training_completed",
        "security_code_audit_delivered",
        "security_threat_model_validated",
        "security_detection_shipped",
        "security_incident_analysis_validated",
        "security_policy_validated",
        "security_purple_exercise_facilitated",
        "security_external_bounty_confirmed",
        "security_competition_won",
        "security_mission_delivered",
        "featured_security_researcher",
    ],
    artifact_bases: &[
        "security_finding_confirmed",
        "security_finding_published",
        "security_machine_walkthrough_validated",
        "security_training_completed",
        "security_code_audit_delivered",
        "security_threat_model_validated",
        "security_detection_shipped",
        "security_incident_analysis_validated",
        "security_policy_validated",
        "security_competition_won",
        "security_mission_delivered",
    ],
    // False. Security evidence is a public page, a repository or a write-up —
    // never a file in our own bucket. A proof of an unfixed vulnerability
    // living behind a signed URL of ours would be a proof only we can show.
    allows_stored_objects: false,
};

/// Which basis a security slice earns, from its trade.
///
/// `finding_hunt` earns none, and that is not an omission: a hunt is attested
/// through the findings it produced, each on its own merits. A hunt that found
/// nothing is honest work and not an artefact.
pub fn basis_for_subtype(subtype: &str) -> Option<&'static str> {
    match subtype {
        "code_audit" => Some("security_code_audit_delivered"),
        "threat_model" => Some("security_threat_model_validated"),
        "governance_review" => Some("security_policy_validated"),
        "detection_engineering" => Some("security_detection_shipped"),
        "incident_analysis" => Some("security_incident_analysis_validated"),
        "purple_exercise" => Some("security_purple_exercise_facilitated"),
        "finding_hunt" => None,
        _ => None,
    }
}

/// Which basis a completed practice challenge earns.
pub fn basis_for_challenge_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "ctf_flag" => Some("security_ctf_solved"),
        "defensive_lab" => Some("security_blue_lab_completed"),
        "machine_walkthrough" => Some("security_machine_walkthrough_validated"),
        "training_ground" | "analysis_exercise" | "audit_exercise" => {
            Some("security_training_completed")
        }
        _ => None,
    }
}

/// The public card of a finding. Readable while the finding is embargoed, with
/// the reproduction withheld — see the module header.
fn finding_url(finding_id: Uuid) -> String {
    format!("{PUBLIC_SITE_URL}/security/findings/{finding_id}")
}

// ═══════════════════════════════════════════════════════════════════
// Slices
// ═══════════════════════════════════════════════════════════════════

/// Issue whatever the verified work on this security slice earns.
///
/// Returns the bases actually issued: empty on a second pass, and empty for
/// work that earns none. Neither is an error.
pub async fn issue_for_slice(db: &PgPool, slice_id: Uuid) -> Result<Vec<String>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Evidence_ {
        deliverable_id: Uuid,
        user_id: Uuid,
        artifact_url: String,
        subtype: Option<String>,
        slice_type: String,
        title: String,
        project_id: Option<Uuid>,
    }

    let row: Option<Evidence_> = sqlx::query_as(
        r#"
        SELECT d.id AS deliverable_id, d.user_id, d.artifact_url,
               ps.security_subtype AS subtype, ps.slice_type, ps.title,
               ps.project_id
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

    let Some(ev) = row else {
        return Ok(Vec::new());
    };
    if ev.slice_type != "security_artifact" {
        return Ok(Vec::new());
    }
    let Some(basis) = ev.subtype.as_deref().and_then(basis_for_subtype) else {
        return Ok(Vec::new());
    };

    let skills = skill_nodes_for_slice(db, slice_id).await?;
    let (title, description) = crate::services::attestations::basis_wording(db, basis).await;

    let issued = artefact_attestations::issue(
        db,
        ev.user_id,
        basis,
        &Evidence {
            url: ev.artifact_url,
            title,
            description: format!("{description}\n\n{}", ev.title),
            deliverable_id: Some(ev.deliverable_id),
            project_id: ev.project_id,
            skill_node_ids: skills,
        },
        &SECURITY,
    )
    .await?;

    Ok(vec![issued.basis])
}

async fn skill_nodes_for_slice(db: &PgPool, slice_id: Uuid) -> Result<Vec<Uuid>, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT ss.skill_id
           FROM slice_skills ss
           JOIN skill_nodes sn ON sn.id = ss.skill_id
          WHERE ss.slice_id = $1
          ORDER BY sn.slug",
    )
    .bind(slice_id)
    .fetch_all(db)
    .await?)
}

// ═══════════════════════════════════════════════════════════════════
// Findings
// ═══════════════════════════════════════════════════════════════════

/// Issue what this finding earns, from where it has got to.
///
/// Called after every transition that could earn something, and safe to call
/// after every transition that could not. Three bases can come out of one row
/// over its life, and each is issued once:
///
///   * `security_finding_confirmed` — as soon as somebody reproduced it.
///   * `security_finding_published` — when it goes public with a write-up.
///   * `security_finding_co_credit` — instead of the first two, when it was
///     ruled a duplicate.
pub async fn issue_for_finding(db: &PgPool, finding_id: Uuid) -> Result<Vec<String>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        reporter_user_id: Uuid,
        status: String,
        dedup_state: String,
        severity_tier: String,
        cwe_id: Option<String>,
        title: String,
        writeup_url: Option<String>,
        deliverable_id: Option<Uuid>,
    }

    let Some(f): Option<Row> = sqlx::query_as(
        r#"
        SELECT sf.reporter_user_id, sf.status, sf.dedup_state, sf.severity_tier,
               sf.cwe_id, sf.title, sf.writeup_url,
               (SELECT d.id FROM deliverables d
                 WHERE d.security_finding_id = sf.id
                   AND d.verification_status = 'verified'
                   AND d.revoked_at IS NULL
                 LIMIT 1) AS deliverable_id
          FROM security_findings sf
         WHERE sf.id = $1
        "#,
    )
    .bind(finding_id)
    .fetch_optional(db)
    .await?
    else {
        return Ok(Vec::new());
    };

    let class = f
        .cwe_id
        .as_deref()
        .map(|c| format!(" ({c})"))
        .unwrap_or_default();
    let subject = format!("{} — {}{}", f.title, f.severity_tier, class);

    let mut issued = Vec::new();

    // A duplicate earns the co-credit and nothing else. It has no fix of its
    // own and no deliverable, and pretending otherwise would pay twice for one
    // vulnerability — which is the thing first-to-file exists to prevent.
    if f.dedup_state == "duplicate_confirmed" {
        let basis = "security_finding_co_credit";
        let (title, description) = crate::services::attestations::basis_wording(db, basis).await;
        let out = artefact_attestations::issue_linked(
            db,
            f.reporter_user_id,
            basis,
            &Evidence {
                url: finding_url(finding_id),
                title,
                description: format!("{description}\n\n{subject}"),
                deliverable_id: None,
                project_id: None,
                skill_node_ids: Vec::new(),
            },
            Links {
                security_finding_id: Some(finding_id),
                challenge_template_id: None,
            },
            &SECURITY,
        )
        .await?;
        issued.push(out.basis);
        return Ok(issued);
    }

    // Everything else needs the deliverable, which `security_findings` creates
    // when the finding is confirmed. Without it there is nothing verified to
    // rest on, and the basis requires one.
    let Some(deliverable_id) = f.deliverable_id else {
        return Ok(issued);
    };

    if matches!(f.status.as_str(), "confirmed" | "fixed" | "published") {
        let basis = "security_finding_confirmed";
        let (title, description) = crate::services::attestations::basis_wording(db, basis).await;
        let out = artefact_attestations::issue_linked(
            db,
            f.reporter_user_id,
            basis,
            &Evidence {
                url: finding_url(finding_id),
                title,
                description: format!("{description}\n\n{subject}"),
                deliverable_id: Some(deliverable_id),
                project_id: None,
                skill_node_ids: Vec::new(),
            },
            Links {
                security_finding_id: Some(finding_id),
                challenge_template_id: None,
            },
            &SECURITY,
        )
        .await?;
        issued.push(out.basis);
    }

    if f.status == "published" {
        let basis = "security_finding_published";
        let (title, description) = crate::services::attestations::basis_wording(db, basis).await;
        // The write-up if there is one, the card otherwise. A published finding
        // has a write-up by constraint, so the fallback is defensive.
        let url = f
            .writeup_url
            .clone()
            .unwrap_or_else(|| finding_url(finding_id));
        let url = if url.starts_with("https://") {
            url
        } else {
            // A relative path — a write-up committed to this repository. Made
            // absolute so the attestation carries something a stranger can
            // open, which is the whole test the shared issuer applies.
            format!(
                "{PUBLIC_SITE_URL}{}",
                if url.starts_with('/') {
                    url
                } else {
                    format!("/{url}")
                }
            )
        };
        let out = artefact_attestations::issue_linked(
            db,
            f.reporter_user_id,
            basis,
            &Evidence {
                url,
                title,
                description: format!("{description}\n\n{subject}"),
                deliverable_id: Some(deliverable_id),
                project_id: None,
                skill_node_ids: Vec::new(),
            },
            Links {
                security_finding_id: Some(finding_id),
                challenge_template_id: None,
            },
            &SECURITY,
        )
        .await?;
        issued.push(out.basis);
    }

    Ok(issued)
}

// ═══════════════════════════════════════════════════════════════════
// Practice challenges
// ═══════════════════════════════════════════════════════════════════

/// Attest a completed practice challenge.
///
/// No deliverable, on purpose (migration 0546): the answer was planted, and a
/// planted answer does not move a rank. The uniqueness key is the challenge, so
/// What a challenge has to say about itself for an attestation to name it:
/// its security kind, its title, its difficulty tier and the target it was
/// solved against. Named because four optional strings in a row read the same
/// whichever order they are in, and this is the row a mistake would silently
/// reorder.
type ChallengeRow = (Option<String>, String, Option<String>, Option<String>);

/// re-running this after a second attempt issues nothing.
pub async fn issue_for_challenge(
    db: &PgPool,
    user_id: Uuid,
    challenge_id: Uuid,
) -> Result<Option<String>, AppError> {
    let row: Option<ChallengeRow> = sqlx::query_as(
        "SELECT security_kind, title, security_difficulty_tier, security_target_url
           FROM challenge_templates WHERE id = $1",
    )
    .bind(challenge_id)
    .fetch_optional(db)
    .await?;

    let Some((kind, title_text, tier, target)) = row else {
        return Ok(None);
    };
    let Some(basis) = kind.as_deref().and_then(basis_for_challenge_kind) else {
        return Ok(None);
    };
    // The write-up-reviewed kinds go through the slice or deliverable path
    // instead: their attestation rests on something a person read.
    if SECURITY.artifact_bases.contains(&basis) {
        return Ok(None);
    }

    let (title, description) = crate::services::attestations::basis_wording(db, basis).await;
    let tier_text = tier.map(|t| format!(" — {t}")).unwrap_or_default();

    let out = artefact_attestations::issue_linked(
        db,
        user_id,
        basis,
        &Evidence {
            url: target.unwrap_or_else(|| format!("{PUBLIC_SITE_URL}/challenges/{challenge_id}")),
            title,
            description: format!("{description}\n\n{title_text}{tier_text}"),
            deliverable_id: None,
            project_id: None,
            skill_node_ids: Vec::new(),
        },
        Links {
            security_finding_id: None,
            challenge_template_id: Some(challenge_id),
        },
        &SECURITY,
    )
    .await?;

    Ok(Some(out.basis))
}

// ═══════════════════════════════════════════════════════════════════
// The sweep the orchestrator calls
// ═══════════════════════════════════════════════════════════════════

/// Issue whatever every piece of security work this person has earns.
///
/// Called from the proof orchestrator rather than only from the point of each
/// transition, for the reason P19 wrote down: a finding is published weeks
/// after it is confirmed, a fix lands after a review, and hooking only the
/// moment of verification leaves the later halves permanently unattested.
///
/// Bounded. Somebody with more security artefacts than these limits has a
/// profile that needs a look rather than a longer loop, and the next recompute
/// reaches what this pass did not.
pub async fn issue_for_user(db: &PgPool, user_id: Uuid) -> Result<Vec<String>, AppError> {
    let mut issued = Vec::new();

    let slices: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT ps.id
           FROM project_slices ps
           JOIN deliverables d ON d.slice_id = ps.id
          WHERE d.user_id = $1
            AND ps.slice_type = 'security_artifact'
            AND d.verification_status = 'verified'
            AND d.revoked_at IS NULL
          LIMIT 200",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    for slice_id in slices {
        // One failing artefact must not cost somebody the attestation on
        // another.
        match issue_for_slice(db, slice_id).await {
            Ok(mut bases) => issued.append(&mut bases),
            Err(e) => tracing::warn!(
                slice = %slice_id, error = %e,
                "security attestation generator failed on one artefact"
            ),
        }
    }

    let findings: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM security_findings
          WHERE reporter_user_id = $1
            AND status IN ('confirmed', 'fixed', 'published', 'duplicate')
          ORDER BY created_at DESC
          LIMIT 200",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    for finding_id in findings {
        match issue_for_finding(db, finding_id).await {
            Ok(mut bases) => issued.append(&mut bases),
            Err(e) => tracing::warn!(
                finding = %finding_id, error = %e,
                "security attestation generator failed on one finding"
            ),
        }
    }

    Ok(issued)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_subtype_maps_to_a_declared_basis() {
        // The five trades' artefact subtypes, from migration 0550. A subtype
        // whose basis is not in `SECURITY.bases` would be refused by the
        // shared issuer at runtime, on somebody's account, long after this
        // migration merged.
        for subtype in [
            "code_audit",
            "threat_model",
            "governance_review",
            "detection_engineering",
            "incident_analysis",
            "purple_exercise",
        ] {
            let basis =
                basis_for_subtype(subtype).unwrap_or_else(|| panic!("{subtype} has no basis"));
            assert!(
                SECURITY.bases.contains(&basis),
                "{subtype} maps to {basis}, which is not a security basis"
            );
        }
        // And the one that deliberately earns nothing.
        assert!(basis_for_subtype("finding_hunt").is_none());
    }

    #[test]
    fn every_challenge_kind_maps_to_a_declared_basis() {
        for kind in [
            "ctf_flag",
            "defensive_lab",
            "machine_walkthrough",
            "training_ground",
            "analysis_exercise",
            "audit_exercise",
        ] {
            let basis =
                basis_for_challenge_kind(kind).unwrap_or_else(|| panic!("{kind} has no basis"));
            assert!(SECURITY.bases.contains(&basis));
        }
    }

    #[test]
    fn the_machine_graded_bases_require_no_deliverable() {
        // The editorial position of migration 0546, asserted rather than
        // trusted: a captured flag must never create a deliverable, because a
        // deliverable is what moves a rank.
        for basis in ["security_ctf_solved", "security_blue_lab_completed"] {
            assert!(
                !SECURITY.artifact_bases.contains(&basis),
                "{basis} must not require a deliverable"
            );
        }
    }

    #[test]
    fn a_co_credit_needs_no_deliverable_and_a_confirmation_does() {
        assert!(
            !SECURITY
                .artifact_bases
                .contains(&"security_finding_co_credit")
        );
        assert!(
            SECURITY
                .artifact_bases
                .contains(&"security_finding_confirmed")
        );
    }
}
