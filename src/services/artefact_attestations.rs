//! Issuing an artefact attestation, once.
//!
//! ## Why this exists
//!
//! `code_attestations::issue` and `design_attestations::issue` were the same
//! ninety lines twice: the same basis check, the same "an artefact basis must
//! name a deliverable", the same ownership check on that deliverable, the same
//! INSERT, the same verification code. They differed in three things — which
//! bases are legal, whether a stored object counts as evidence, and the metric
//! label.
//!
//! Two copies of a rule is one copy of a rule and one bug waiting. The
//! ownership check in particular is the one that stops an attestation being
//! issued from somebody else's work, and it is not a check that should exist
//! in two places where one can be edited without the other.
//!
//! A third domain — cybersecurity — is already in the backlog, and would have
//! been a third copy.
//!
//! ## What stays per domain
//!
//! [`Domain`]: the legal bases, which of them must rest on a deliverable, and
//! whether evidence may live in our own storage. Those are genuinely different
//! answers, and they are data rather than code.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// What an attestation points at.
#[derive(Debug, Clone, serde::Deserialize)]
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

/// What an attestation rests on when it does not rest on a deliverable.
///
/// A separate argument rather than two more fields on [`Evidence`], so that
/// the eighteen places that build an `Evidence` literal do not all have to
/// mention two links that only one domain uses. [`issue`] passes an empty one.
#[derive(Debug, Clone, Default)]
pub struct Links {
    /// The reported vulnerability this rests on. Security only, and needed on
    /// its own rather than through the deliverable because an independent
    /// co-discovery has a finding and no fix of its own (migration 0559).
    pub security_finding_id: Option<Uuid>,
    /// The catalogue challenge this rests on, where completing the challenge
    /// is itself the attestable act — a captured flag, a passed lab. Also the
    /// key the uniqueness rule of 0559 uses, since those produce no
    /// deliverable to be unique per.
    pub challenge_template_id: Option<Uuid>,
    /// The game jam this rests on (migration 0585). Game only, and needed on
    /// its own for `game_jam_participant`, which has no deliverable and so
    /// would otherwise collide across every jam a person entered.
    pub game_jam_id: Option<Uuid>,
    /// The registered mod this rests on (migration 0585). Game only.
    pub game_mod_id: Option<Uuid>,
    /// The itch / GameJolt / store page a `game_shipped_title` vouches for
    /// (migration 0585). Game only, and descriptive rather than a uniqueness
    /// key — the shipped title rests on its deliverable.
    pub external_publish_url: Option<String>,
}

/// What came back.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Issued {
    pub id: Uuid,
    pub basis: String,
    /// Ten characters somebody types into the public verification page.
    pub verification_code: String,
}

/// The rules that differ between one domain's attestations and another's.
#[derive(Debug, Clone, Copy)]
pub struct Domain {
    /// `code`, `design`, … Used in the refusal message and in the metric, so
    /// an operator can see which domain is issuing.
    pub name: &'static str,
    /// Every basis this domain may issue.
    pub bases: &'static [&'static str],
    /// Those that must name the verified deliverable they rest on. Mirrors
    /// the CHECK constraint on `attestations`.
    pub artifact_bases: &'static [&'static str],
    /// Whether evidence may live in our own storage rather than on a public
    /// host.
    ///
    /// True for design, where a five-gigabyte scene file has no free home
    /// elsewhere. False for code, where the artefact is a repository and an
    /// `s3://` link would mean the proof is a copy we made of it.
    pub allows_stored_objects: bool,
}

/// Write the attestation.
///
/// The single door for a domain's generators, so the rules that apply to all
/// of them are checked once.
///
/// `attestation_type` is `artefact` (migration 0198): the type says what kind
/// of statement it is, the basis says what it rests on. A merged contribution
/// to the Linux kernel is not "C, level 4", and a validated brand identity is
/// not "colour theory, level 4" — which is why neither is a `skill`.
pub async fn issue(
    db: &PgPool,
    user_id: Uuid,
    basis: &str,
    evidence: &Evidence,
    domain: &Domain,
) -> Result<Issued, AppError> {
    issue_linked(db, user_id, basis, evidence, Links::default(), domain).await
}

/// [`issue`], for an attestation that rests on something other than a
/// deliverable.
pub async fn issue_linked(
    db: &PgPool,
    user_id: Uuid,
    basis: &str,
    evidence: &Evidence,
    links: Links,
    domain: &Domain,
) -> Result<Issued, AppError> {
    if !domain.bases.contains(&basis) {
        return Err(AppError::Validation(format!(
            "'{basis}' is not one of the {} attestation bases",
            domain.name
        )));
    }
    if domain.artifact_bases.contains(&basis) && evidence.deliverable_id.is_none() {
        return Err(AppError::Validation(format!(
            "a {basis} attestation must name the verified deliverable it rests on"
        )));
    }
    if evidence.title.trim().is_empty() {
        return Err(AppError::Validation("an attestation needs a title".into()));
    }
    crate::validators::check_max_len(&evidence.title, "title", 200)?;

    let url = evidence.url.trim();
    let scheme_ok =
        url.starts_with("https://") || (domain.allows_stored_objects && url.starts_with("s3://"));
    if !scheme_ok {
        return Err(AppError::Validation(if domain.allows_stored_objects {
            "the evidence URL must be an https link or a stored object — an \
             attestation nobody can open is worth nothing"
                .into()
        } else {
            "the evidence URL must be a public https link — an attestation \
             nobody can check is worth nothing"
                .to_string()
        }));
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

    // `ON CONFLICT DO NOTHING`, and the existing row read back when it fires.
    //
    // Three partial unique indexes cover this table — per deliverable (0198),
    // per challenge and per finding (0559) — and every generator that reaches
    // here is re-runnable by design: the proof orchestrator sweeps the same
    // user repeatedly, and a hook can fire twice for one event. Erroring on the
    // second pass would mean a caller has to distinguish "already issued" from
    // "went wrong", and every caller would have to do it.
    let inserted: Option<(Uuid, String)> = sqlx::query_as(
        r#"
        INSERT INTO attestations
            (user_id, attestation_type, title, description, basis,
             linked_deliverable_ids, linked_project_ids, linked_skill_node_ids,
             verification_code, security_finding_id, challenge_template_id,
             game_jam_id, game_mod_id, external_publish_url)
        VALUES ($1, 'artefact', $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        ON CONFLICT DO NOTHING
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
    .bind(links.security_finding_id)
    .bind(links.challenge_template_id)
    .bind(links.game_jam_id)
    .bind(links.game_mod_id)
    .bind(links.external_publish_url.as_deref())
    .fetch_optional(db)
    .await?;

    let row: (Uuid, String) = match inserted {
        Some(row) => row,
        None => {
            // Already issued. Return the one that exists, so the caller sees
            // the same verification code it would have seen the first time.
            let existing: Option<(Uuid, String)> = sqlx::query_as(
                r#"
                SELECT id, verification_code
                  FROM attestations
                 WHERE user_id = $1
                   AND attestation_type = 'artefact'
                   AND basis = $2
                   AND revoked_at IS NULL
                   AND linked_deliverable_ids = $3
                   AND security_finding_id IS NOT DISTINCT FROM $4
                   AND challenge_template_id IS NOT DISTINCT FROM $5
                   AND game_jam_id IS NOT DISTINCT FROM $6
                   AND game_mod_id IS NOT DISTINCT FROM $7
                 LIMIT 1
                "#,
            )
            .bind(user_id)
            .bind(basis)
            .bind(&deliverables)
            .bind(links.security_finding_id)
            .bind(links.challenge_template_id)
            .bind(links.game_jam_id)
            .bind(links.game_mod_id)
            .fetch_optional(db)
            .await?;

            let Some(row) = existing else {
                // The insert was refused and nothing matching is there to
                // read. That means a constraint refused it for a reason other
                // than the uniqueness rules, and saying so is better than
                // returning something invented.
                return Err(AppError::Internal(format!(
                    "a {basis} attestation was refused and no existing one                      explains why"
                )));
            };
            // Not counted in the metric: nothing was issued.
            return Ok(Issued {
                id: row.0,
                basis: basis.to_string(),
                verification_code: row.1,
            });
        }
    };

    // One metric with a domain label, rather than one metric per domain. A
    // dashboard asking "how many attestations were issued this week" should
    // not have to know how many domains exist.
    metrics::counter!(
        "skilluv_attestations_issued_total",
        "domain" => domain.name,
        "basis" => basis.to_string(),
    )
    .increment(1);

    Ok(Issued {
        id: row.0,
        basis: basis.to_string(),
        verification_code: row.1,
    })
}

/// Ten base32 characters, fifty bits. Same shape everywhere an attestation is
/// issued, because the public verification page reads them all through one
/// route.
pub fn verification_code() -> String {
    use base32::Alphabet;
    let mut bytes = [0u8; 8];
    // `rand_core::OsRng` is gone in 0.10. `getrandom::fill` is what the rest of
    // this codebase already reaches for when it wants OS entropy for a token,
    // and it is the same source the old `OsRng` wrapped.
    getrandom::fill(&mut bytes).expect("OS RNG");
    base32::encode(Alphabet::Rfc4648 { padding: false }, &bytes)
        .chars()
        .take(10)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODE: Domain = Domain {
        name: "code",
        bases: &["code_pr_merged_upstream", "featured_coder"],
        artifact_bases: &["code_pr_merged_upstream"],
        allows_stored_objects: false,
    };
    const DESIGN: Domain = Domain {
        name: "design",
        bases: &["design_deliverable_validated"],
        artifact_bases: &["design_deliverable_validated"],
        allows_stored_objects: true,
    };

    #[test]
    fn a_code_is_ten_characters_of_base32() {
        let code = verification_code();
        assert_eq!(code.len(), 10);
        assert!(
            code.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()),
            "{code}"
        );
    }

    #[test]
    fn two_codes_are_not_the_same() {
        assert_ne!(verification_code(), verification_code());
    }

    #[test]
    fn stored_objects_are_a_design_allowance_and_not_a_code_one() {
        // A design artefact can be a five-gigabyte scene with no free home
        // elsewhere. A code artefact is a repository, and an `s3://` link
        // would mean the proof is a copy we made of it.
        const { assert!(DESIGN.allows_stored_objects) };
        const { assert!(!CODE.allows_stored_objects) };
    }

    #[test]
    fn every_artifact_basis_is_also_a_basis() {
        // The reverse is allowed — an editorial basis rests on nothing — but
        // an artefact basis missing from `bases` would be permanently
        // unissuable, and nothing would say so.
        for domain in [CODE, DESIGN] {
            for basis in domain.artifact_bases {
                assert!(
                    domain.bases.contains(basis),
                    "{}: {basis} requires a deliverable but is not issuable",
                    domain.name
                );
            }
        }
    }
}
