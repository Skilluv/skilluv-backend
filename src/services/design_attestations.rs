//! Issuing the seven design attestations (migration 0233).
//!
//! ## Why design gets one the other domains do not
//!
//! `design_deliverable_validated` has no equivalent in `code_attestations`,
//! and that is not an oversight. In code the attestable moment is the merge
//! upstream: the platform's own validation is a step on the way, and saying
//! "Skilluv approved it" adds nothing a maintainer's merge does not already
//! say better.
//!
//! Design has no upstream. Nobody merges a brand identity. The validation
//! *is* the outcome, and what makes it worth anything is that it came after a
//! critique conversation a stranger can read — the rounds, the reasons, the
//! version that finally passed. That is what this attestation points at.
//!
//! ## What is checked and what is trusted
//!
//! Two of the seven can be checked without asking a human. A validated
//! deliverable was already verified by the platform before this is reachable,
//! and a contest win is a row with a rank in it. The other four rest on a
//! judgement somebody made and recorded: a brand system delivered, a typeface
//! released, a system another team adopted, a mission a client accepted.
//! Saying so is better than a check that returns true for any page answering
//! 200.
//!
//! The seventh, `featured_designer`, is an editorial decision and is granted,
//! not generated.
//!
//! ## One door
//!
//! Every generator ends at [`issue`], which writes the row. What differs is
//! what has to be true before it is called, and that is what each function
//! here states.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Every design basis accepted by migration 0233.
pub const BASES: &[&str] = &[
    "design_deliverable_validated",
    "design_brand_system_delivered",
    "design_typeface_released",
    "design_system_adopted",
    "design_contest_won",
    "design_mission_delivered",
    "featured_designer",
];

/// The bases that must name the verified deliverable they rest on.
/// Mirrors `attestations_artifact_basis_links_a_deliverable`.
pub const ARTIFACT_BASES: &[&str] = &[
    "design_deliverable_validated",
    "design_brand_system_delivered",
    "design_typeface_released",
    "design_system_adopted",
    "design_contest_won",
    "design_mission_delivered",
];

#[derive(Debug, Clone, Deserialize)]
pub struct Evidence {
    /// The link a reader follows to check the claim.
    pub url: String,
    pub title: String,
    pub description: String,
    /// The verified deliverable behind it, where the basis requires one.
    #[serde(default)]
    pub deliverable_id: Option<Uuid>,
    #[serde(default)]
    pub project_id: Option<Uuid>,
    #[serde(default)]
    pub skill_node_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Issued {
    pub id: Uuid,
    pub basis: String,
    pub verification_code: String,
}

/// Write the attestation.
///
/// `attestation_type` is `artefact` (migration 0198): the type says what kind
/// of statement it is, the basis says what it rests on. A validated brand
/// identity is not "colour theory, level 4", which is why it is not a `skill`.
pub async fn issue(
    db: &PgPool,
    user_id: Uuid,
    basis: &str,
    evidence: &Evidence,
) -> Result<Issued, AppError> {
    if !BASES.contains(&basis) {
        return Err(AppError::Validation(format!(
            "'{basis}' is not one of the design attestation bases"
        )));
    }
    if ARTIFACT_BASES.contains(&basis) && evidence.deliverable_id.is_none() {
        return Err(AppError::Validation(format!(
            "a {basis} attestation must name the verified deliverable it rests on"
        )));
    }
    if evidence.title.trim().is_empty() {
        return Err(AppError::Validation("an attestation needs a title".into()));
    }
    crate::validators::check_max_len(&evidence.title, "title", 200)?;

    // A design artefact can live behind our own storage rather than on a
    // public host — a 5 GB scene file has no free home elsewhere — so the
    // scheme check accepts both, and only both.
    let url = evidence.url.trim();
    if !(url.starts_with("https://") || url.starts_with("s3://")) {
        return Err(AppError::Validation(
            "the evidence URL must be an https link or a stored object — an \
             attestation nobody can open is worth nothing"
                .into(),
        ));
    }

    // The deliverable must be this person's, verified, and not revoked.
    // Otherwise an attestation could be issued from somebody else's work, or
    // from work the platform has already taken back.
    if let Some(deliverable_id) = evidence.deliverable_id {
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM deliverables
                  WHERE id = $1 AND user_id = $2
                    AND verification_status = 'verified'
                    AND revoked_at IS NULL)",
        )
        .bind(deliverable_id)
        .bind(user_id)
        .fetch_one(db)
        .await?;
        if !ok {
            return Err(AppError::Validation(
                "that deliverable is not a verified artefact of this person's".into(),
            ));
        }
    }

    let description = format!("{}\n\n{}", evidence.description.trim(), url);
    let projects: Vec<Uuid> = evidence.project_id.into_iter().collect();
    let deliverables: Vec<Uuid> = evidence.deliverable_id.into_iter().collect();

    let row: (Uuid, String) = sqlx::query_as(
        r#"
        INSERT INTO attestations
            (user_id, attestation_type, title, description, basis,
             linked_deliverable_ids, linked_project_ids, linked_skill_node_ids,
             verification_code)
        VALUES ($1, 'artefact', $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, verification_code
        "#,
    )
    .bind(user_id)
    .bind(evidence.title.trim())
    .bind(&description)
    .bind(basis)
    .bind(&deliverables)
    .bind(&projects)
    .bind(&evidence.skill_node_ids)
    .bind(verification_code())
    .fetch_one(db)
    .await?;

    metrics::counter!("skilluv_design_attestations_issued_total", "basis" => basis.to_string())
        .increment(1);

    Ok(Issued {
        id: row.0,
        basis: basis.to_string(),
        verification_code: row.1,
    })
}

/// Ten base32 characters, fifty bits. Same shape as everywhere else an
/// attestation is issued, because the public verification page reads them all
/// through one route.
fn verification_code() -> String {
    use base32::Alphabet;
    use rand_core::RngCore;
    let mut bytes = [0u8; 8];
    rand_core::OsRng.fill_bytes(&mut bytes);
    base32::encode(Alphabet::Rfc4648 { padding: false }, &bytes)
        .chars()
        .take(10)
        .collect()
}

// ═══════════════════════════════════════════════════════════════════
// Generators
// ═══════════════════════════════════════════════════════════════════

/// A design challenge validated after critique.
///
/// The description carries the round count on purpose. "Validated at the
/// first round" and "validated at the fifth" are different stories, and the
/// second is often the better one: it says somebody was told their direction
/// was wrong and came back four times rather than abandoning it.
pub async fn deliverable_validated(
    db: &PgPool,
    user_id: Uuid,
    deliverable_id: Uuid,
    slice_title: &str,
    artifact_url: &str,
    rounds: i16,
) -> Result<Issued, AppError> {
    let rounds_sentence = match rounds {
        0 | 1 => "Validé au premier tour de critique.".to_string(),
        n => format!("Validé au bout de {n} tours de critique."),
    };
    issue(
        db,
        user_id,
        "design_deliverable_validated",
        &Evidence {
            url: artifact_url.to_string(),
            title: format!("Livrable design validé : {slice_title}"),
            description: format!(
                "Livré sur Skilluv et validé par un relecteur du métier. {rounds_sentence}"
            ),
            deliverable_id: Some(deliverable_id),
            project_id: None,
            skill_node_ids: Vec::new(),
        },
    )
    .await
}

/// A podium finish in a design contest.
pub async fn contest_won(
    db: &PgPool,
    user_id: Uuid,
    deliverable_id: Uuid,
    contest_title: &str,
    artifact_url: &str,
    rank: i16,
    entries: i64,
) -> Result<Issued, AppError> {
    let place = match rank {
        1 => "Première place",
        2 => "Deuxième place",
        3 => "Troisième place",
        _ => "Classé",
    };
    issue(
        db,
        user_id,
        "design_contest_won",
        &Evidence {
            url: artifact_url.to_string(),
            title: format!("{place} : {contest_title}"),
            description: format!(
                "{place} sur {entries} propositions, dans un concours design Skilluv \
                 jugé sur une grille publiée avant l'ouverture."
            ),
            deliverable_id: Some(deliverable_id),
            project_id: None,
            skill_node_ids: Vec::new(),
        },
    )
    .await
}

// ═══════════════════════════════════════════════════════════════════
// Contest podiums
// ═══════════════════════════════════════════════════════════════════

/// What a concluded design contest produced.
#[derive(Debug, Clone, Serialize)]
pub struct PodiumReport {
    pub deliverables_written: u32,
    pub attestations_issued: u32,
}

/// Turn the podium of a concluded design contest into proofs.
///
/// Called after `tournament::conclude_tournament` has written the ranks. Two
/// things happen per finisher in the top three, in this order:
///
///   1. a verified `deliverables` row, so the win moves the rank, the badges
///      and the public portfolio like every other proof;
///   2. a `design_contest_won` attestation, which needs that deliverable to
///      exist — migration 0233 refuses an artefact basis without one.
///
/// Idempotent by construction: the unique index of migration 0237 means a
/// second run writes no deliverable, and no attestation follows.
///
/// Podium only. Taking part is not an achievement, and a proof that means
/// "showed up" devalues every other row in the table.
pub async fn award_contest_podium(
    db: &PgPool,
    tournament_id: Uuid,
) -> Result<PodiumReport, AppError> {
    let contest: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT name, skill_domain, status FROM tournaments WHERE id = $1",
    )
    .bind(tournament_id)
    .fetch_optional(db)
    .await?;
    let (name, domain, status) =
        contest.ok_or_else(|| AppError::NotFound("tournament not found".into()))?;

    if domain.as_deref() != Some("design") {
        return Ok(PodiumReport {
            deliverables_written: 0,
            attestations_issued: 0,
        });
    }
    if status != "concluded" {
        return Err(AppError::Validation(
            "a contest has no podium until it is concluded".into(),
        ));
    }

    // The entries that placed, with the ranking the conclusion wrote.
    let podium: Vec<(Uuid, Uuid, String, i32)> = sqlx::query_as(
        r#"
        SELECT s.id, s.participant_id, s.artifact_url, p.rank
          FROM tournament_submissions s
          JOIN tournament_participants p
            ON p.tournament_id = s.tournament_id
           AND p.participant_type = s.participant_type
           AND p.participant_id = s.participant_id
         WHERE s.tournament_id = $1
           AND s.participant_type = 'user'
           AND s.status NOT IN ('rejected', 'disqualified')
           AND p.rank IS NOT NULL
           AND p.rank <= 3
         ORDER BY p.rank ASC
        "#,
    )
    .bind(tournament_id)
    .fetch_all(db)
    .await?;

    let entries: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM tournament_submissions
          WHERE tournament_id = $1 AND status NOT IN ('rejected', 'disqualified')",
    )
    .bind(tournament_id)
    .fetch_one(db)
    .await?;

    let mut deliverables_written = 0u32;
    let mut attestations_issued = 0u32;

    for (submission_id, user_id, artifact_url, rank) in podium {
        let deliverable_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO deliverables (
                tournament_submission_id, user_id, artifact_type, artifact_url,
                artifact_metadata, verifiable_by, verification_status,
                verified_at, fragments_awarded, public
            )
            VALUES ($1, $2, 'design_artifact', $3, $4,
                    'human_review', 'verified', NOW(), 0, TRUE)
            ON CONFLICT (tournament_submission_id)
                WHERE tournament_submission_id IS NOT NULL
            DO NOTHING
            RETURNING id
            "#,
        )
        .bind(submission_id)
        .bind(user_id)
        .bind(&artifact_url)
        .bind(serde_json::json!({
            "tournament_id": tournament_id,
            "rank": rank,
            "entries": entries,
        }))
        .fetch_optional(db)
        .await?;

        let Some(deliverable_id) = deliverable_id else {
            // Already awarded on an earlier run. The attestation went with it.
            continue;
        };
        deliverables_written += 1;

        contest_won(
            db,
            user_id,
            deliverable_id,
            &name,
            &artifact_url,
            rank as i16,
            entries,
        )
        .await?;
        attestations_issued += 1;

        // The proof exists now; the rank, the badges and the search score
        // have to catch up. Best-effort, exactly as the validation path does:
        // a hook failure must not undo a podium already written.
        let db_clone = db.clone();
        tokio::spawn(async move {
            if let Err(e) =
                crate::services::proof_hooks::recompute_all_for_user(&db_clone, user_id).await
            {
                tracing::warn!(
                    user_id = %user_id, error = %e,
                    "proof recompute after a design contest podium failed"
                );
            }
        });
    }

    Ok(PodiumReport {
        deliverables_written,
        attestations_issued,
    })
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn every_artifact_basis_is_a_known_basis() {
        for basis in ARTIFACT_BASES {
            assert!(
                BASES.contains(basis),
                "{basis} requires a deliverable but is not an accepted basis"
            );
        }
    }

    #[test]
    fn only_the_editorial_basis_needs_no_artefact() {
        let editorial: Vec<_> = BASES
            .iter()
            .filter(|b| !ARTIFACT_BASES.contains(b))
            .collect();
        assert_eq!(
            editorial,
            vec![&"featured_designer"],
            "every basis except the editorial one must point at something openable"
        );
    }

    #[test]
    fn verification_codes_are_ten_base32_characters() {
        let code = verification_code();
        assert_eq!(code.len(), 10);
        assert!(
            code.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        );
    }
}
