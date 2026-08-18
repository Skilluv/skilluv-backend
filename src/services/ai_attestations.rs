//! Attestations that rest on an AI artefact.
//!
//! ## What a generator is allowed to assume
//!
//! Nothing. Each one re-checks its own precondition against the database
//! before issuing, even when the caller has just checked the same thing: a
//! generator that trusts its caller produces attestations whose basis is
//! false the day somebody adds a second call site.
//!
//! ## Why they are `artefact` attestations
//!
//! Migration 0198 added that type for exactly this shape of claim: it rests
//! on a deliverable and names it, with `basis` saying what kind. The three
//! older types each carry an invariant written for a different story —
//! `gesture` and `skill` name one skill node, `compagnonnage` names a project
//! — and filing a shipped model under any of them meant either inventing a
//! skill node to point at or breaking a constraint.
//!
//! Skills stay optional and are attached when the slice already names them.
//! A model whose slice was never tagged is still attested; what it rests on
//! is the model, not a skill somebody chose for it.
//!
//! ## Re-running is free
//!
//! `uniq_attestations_artefact_per_deliverable` makes (user, basis,
//! deliverables) unique, so a second pass over already-attested work inserts
//! nothing. That is what lets this be called from a hook without the hook
//! having to remember what it did.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Which basis an AI artefact supports, from what it is.
///
/// `data_pipeline` is deliberately absent. A pipeline is real work and counts
/// as a verified artefact, but none of the seven bases describes it — and
/// inventing `ai_pipeline_built` would mean an attestation whose evidence is
/// a repository, which `code_project_shipped` already covers.
fn basis_for_subtype(subtype: &str) -> Option<&'static str> {
    match subtype {
        "ml_model" => Some("ai_model_shipped"),
        // A served model is a shipped model with an address. Same claim.
        "ai_service_api" => Some("ai_model_shipped"),
        "dataset" => Some("ai_dataset_published"),
        "llm_agent" => Some("ai_agent_system_deployed"),
        "ai_research_paper" => Some("ai_paper_published"),
        _ => None,
    }
}

/// Human-readable title and description for a basis, in the reader's default
/// language. Written here rather than in the database because an attestation
/// keeps the words it was issued with.
fn wording(basis: &str) -> (&'static str, &'static str) {
    match basis {
        "ai_model_shipped" => (
            "Modèle mis en service",
            "Un modèle publié à une adresse où un inconnu peut l'obtenir et l'exécuter.",
        ),
        "ai_dataset_published" => (
            "Jeu de données publié",
            "Un jeu de données publié avec sa fiche : provenance, licence et limites.",
        ),
        "ai_agent_system_deployed" => (
            "Système d'agents déployé",
            "Un système d'agents en service, avec ses évaluations et ses garde-fous.",
        ),
        "ai_paper_published" => (
            "Article publié",
            "Un article paru, préprint ou conférence, avec le code qui le soutient.",
        ),
        "ai_benchmark_result" => (
            "Résultat de banc reproduit",
            "Un résultat mesuré sur un banc public, qu'un relecteur a rejoué et retrouvé.",
        ),
        "ai_safety_finding_validated" => (
            "Trouvaille de sûreté validée",
            "Une trouvaille reproduite, évaluée en gravité et divulguée dans les règles.",
        ),
        _ => ("Contribution IA", "Un travail IA vérifié."),
    }
}

/// The skills the slice already says it touches.
///
/// Attached to the attestation when they exist, and left empty when they do
/// not. Nothing is derived: an `artefact` attestation rests on the deliverable
/// it names, so a slice nobody tagged still produces one rather than being
/// filed under a skill chosen on its behalf.
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
/// Returns the new id, or `None` when the person already had it. The
/// uniqueness is enforced by index rather than by a preceding SELECT, so two
/// concurrent hooks cannot both decide the attestation is missing.
async fn issue(
    db: &PgPool,
    user_id: Uuid,
    basis: &str,
    deliverable_id: Uuid,
    skill_node_ids: &[Uuid],
) -> Result<Option<Uuid>, AppError> {
    let (title, description) = wording(basis);
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
    .bind(title)
    .bind(description)
    .bind(deliverable_id)
    .bind(skill_node_ids)
    .bind(&code)
    .bind(basis)
    .fetch_optional(db)
    .await?;

    if let Some(id) = id {
        announce(db, user_id, id, basis, title, deliverable_id).await;
    }

    Ok(id)
}

/// Put the attestation on the public feed, if the person allows it.
///
/// Best-effort and never fatal: an attestation that was earned must not be
/// lost because a landing-page projection failed. `emit` decides visibility
/// from the person's own preferences, so nothing here overrides a withdrawal.
///
/// The artefact URL is the one the slice already had to name — a feed line
/// with nothing to open is the fabricated social proof migration 0203 exists
/// to replace.
async fn announce(
    db: &PgPool,
    user_id: Uuid,
    attestation_id: Uuid,
    basis: &str,
    title: &str,
    deliverable_id: Uuid,
) {
    /// Who published it and where it can be seen. Named rather than left as
    /// a three-tuple: two of the three are optional URLs, and a caller
    /// reading `.2` instead of `.1` would attach the wrong link.
    #[derive(sqlx::FromRow)]
    struct Context {
        username: String,
        external_hosting_url: Option<String>,
        artifact_url: Option<String>,
    }

    let context: Result<Option<Context>, _> = sqlx::query_as(
        r#"
        SELECT u.username, ps.ai_external_hosting_url, d.artifact_url
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
        external_hosting_url: hosting_url,
        artifact_url,
    })) = context
    else {
        return;
    };
    // The hub address first: it is where the thing actually is. The
    // deliverable URL is the fallback for a subtype that names no host.
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
            "AI attestation issued but not announced"
        );
    }
}

/// Issue whatever the verified work on this slice earns.
///
/// Called after a deliverable is verified. Returns the bases actually issued,
/// which is empty on a second pass and empty for work that earns none — both
/// normal, neither an error.
pub async fn issue_for_slice(db: &PgPool, slice_id: Uuid) -> Result<Vec<String>, AppError> {
    // The verified, unrevoked deliverable is the evidence. Without one there
    // is nothing to attest, whatever the slice claims about itself.
    let evidence: Option<(Uuid, Uuid, Option<String>, String)> = sqlx::query_as(
        r#"
        SELECT d.id, d.user_id, ps.ai_subtype, ps.slice_type
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

    let Some((deliverable_id, user_id, ai_subtype, slice_type)) = evidence else {
        return Ok(Vec::new());
    };
    if slice_type != "ai_artifact" {
        return Ok(Vec::new());
    }

    let skills = skill_nodes_for_slice(db, slice_id).await?;
    let mut issued = Vec::new();

    if let Some(basis) = ai_subtype.as_deref().and_then(basis_for_subtype)
        && issue(db, user_id, basis, deliverable_id, &skills)
            .await?
            .is_some()
    {
        issued.push(basis.to_string());
    }

    // A benchmark earns its own attestation, and only once somebody else has
    // re-run it. The claim is not the result; the reproduction is.
    let reproduced: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM benchmark_results
             WHERE slice_id = $1 AND reproduced_at IS NOT NULL
        )
        "#,
    )
    .bind(slice_id)
    .fetch_one(db)
    .await?;
    if reproduced
        && issue(db, user_id, "ai_benchmark_result", deliverable_id, &skills)
            .await?
            .is_some()
    {
        issued.push("ai_benchmark_result".to_string());
    }

    // A safety finding earns one when it has been reproduced *and* handled:
    // still private means the person who could fix it has not been told, and
    // attesting that would reward sitting on a vulnerability.
    let safety_ready: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM ai_safety_reports
             WHERE slice_id = $1
               AND reproduced_at IS NOT NULL
               AND disclosure_status <> 'private'
        )
        "#,
    )
    .bind(slice_id)
    .fetch_one(db)
    .await?;
    if safety_ready
        && issue(
            db,
            user_id,
            "ai_safety_finding_validated",
            deliverable_id,
            &skills,
        )
        .await?
        .is_some()
    {
        issued.push("ai_safety_finding_validated".to_string());
    }

    Ok(issued)
}

/// Issue whatever every AI artefact this person has earns.
///
/// Called from the proof orchestrator rather than from the point a slice is
/// verified, and deliberately: two of the six bases are earned by events that
/// happen *after* verification — a reviewer reproducing a benchmark, a vendor
/// agreeing a disclosure date. Hooking the verification alone would leave
/// those permanently unissued, which is the dormant-engine failure P19 was
/// written to end.
///
/// Bounded: somebody with more AI artefacts than this has a profile that
/// needs a look rather than a longer loop, and the next recompute picks up
/// whatever this pass did not reach.
pub async fn issue_for_user(db: &PgPool, user_id: Uuid) -> Result<Vec<String>, AppError> {
    let slices: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT ps.id
          FROM project_slices ps
          JOIN deliverables d ON d.slice_id = ps.id
         WHERE d.user_id = $1
           AND ps.slice_type = 'ai_artifact'
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
        // One failing slice does not stop the others: a missing skill tag on
        // one artefact must not cost somebody the attestation on another.
        match issue_for_slice(db, slice_id).await {
            Ok(mut bases) => issued.append(&mut bases),
            Err(e) => tracing::warn!(
                slice = %slice_id, error = %e,
                "AI attestation generator failed on one artefact"
            ),
        }
    }
    Ok(issued)
}

// ═══════════════════════════════════════════════════════════════════
// Featured
// ═══════════════════════════════════════════════════════════════════
//
// `featured_ai_researcher` has been a legal basis since the AI catalogue
// landed, is counted by `ai_profile`, and until now **no code path could
// issue it**. Dead schema: a column of a table nothing writes.
//
// It does not fit this module's other generators, and that is the reason it
// was missed. They write `skill` attestations, which need exactly one skill
// node — and being put forward by the platform names no skill. So this one is
// an `artefact` attestation like its two siblings in the code and design
// domains, and it goes through the shared door.

/// What this domain may issue editorially.
///
/// Only the one basis: everything else AI issues is a `skill` attestation
/// with a skill node, written by the generators above. `artifact_bases` is
/// empty because a featuring rests on nobody's deliverable — it rests on
/// somebody's judgement, and says so.
const EDITORIAL: crate::services::artefact_attestations::Domain =
    crate::services::artefact_attestations::Domain {
        name: "ai",
        bases: &["featured_ai_researcher"],
        artifact_bases: &[],
        allows_stored_objects: false,
    };

/// Featured.
///
/// Editorial, and named as such. There is no formula behind it, and inventing
/// one would make it a worse version of the craft score rather than the thing
/// it is: somebody chose to put this person forward, and put their name to it.
pub async fn featured_ai_researcher(
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
        "featured_ai_researcher",
        &crate::services::artefact_attestations::Evidence {
            url: profile_url.to_string(),
            title: "Chercheur IA mis en avant".into(),
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
    fn a_served_model_and_a_downloaded_one_make_the_same_claim() {
        assert_eq!(basis_for_subtype("ml_model"), Some("ai_model_shipped"));
        assert_eq!(
            basis_for_subtype("ai_service_api"),
            Some("ai_model_shipped")
        );
    }

    #[test]
    fn a_pipeline_earns_no_attestation_of_its_own() {
        // It counts as a verified artefact. None of the seven bases describes
        // it, and inventing one would attest a repository twice.
        assert_eq!(basis_for_subtype("data_pipeline"), None);
        assert_eq!(basis_for_subtype("something-else"), None);
    }

    #[test]
    fn every_basis_has_wording_of_its_own() {
        let bases = [
            "ai_model_shipped",
            "ai_dataset_published",
            "ai_agent_system_deployed",
            "ai_paper_published",
            "ai_benchmark_result",
            "ai_safety_finding_validated",
        ];
        let mut titles: Vec<&str> = bases.iter().map(|b| wording(b).0).collect();
        titles.sort_unstable();
        titles.dedup();
        assert_eq!(
            titles.len(),
            bases.len(),
            "two bases sharing a title means one of them fell through to the default"
        );
    }
}
