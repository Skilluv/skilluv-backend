//! Issuing the seven code attestations (migration 0178).
//!
//! ## What an attestation is here
//!
//! Not a certificate of attendance. Each of these says a specific thing
//! happened somewhere public, and carries the link somebody can follow to
//! check it. That is the whole design: the platform's word is worth nothing
//! on its own, and the artefact behind it is worth everything.
//!
//! ## What is verified and what is trusted
//!
//! Three of the seven can be checked against a public API, and are:
//!
//!   * a merged pull request — the deliverable already carries the proof, and
//!     the platform verified it before this is reachable;
//!   * a published library — the registry is asked whether the package exists;
//!   * an adopted devtool — the same figures, against a threshold.
//!
//! Three cannot be checked by a machine, and are reviewed by a person instead:
//! an accepted RFC, a contribution to a standard, and a shipped project whose
//! URL resolves to something. Saying so is better than a check that returns
//! true for any page that answers 200.
//!
//! The seventh, `featured_coder`, is an editorial decision and is granted, not
//! generated.
//!
//! ## Why the shapes differ
//!
//! Every generator ends at `issue`, which does one thing: write the row with
//! its basis, its links and its verification code. What differs is what has
//! to be true before it is called, and that is exactly what each function
//! here states.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// The seven bases, as the CHECK constraint in migration 0178 spells them.
pub const BASES: &[&str] = &[
    "code_pr_merged_upstream",
    "code_project_shipped",
    "code_library_published",
    "code_rfc_accepted",
    "code_standard_contribution",
    "code_devtool_adopted",
    "featured_coder",
];

/// Bases whose evidence must point at a deliverable the platform verified.
/// Mirrors `attestations_artifact_basis_links_a_deliverable`.
pub const ARTIFACT_BASES: &[&str] = &[
    "code_pr_merged_upstream",
    "code_project_shipped",
    "code_library_published",
];

/// How many recent downloads make a tool "adopted".
///
/// A number, stated, rather than a judgement made case by case. It is low on
/// purpose: a tool five hundred people reached for last month is adopted, and
/// setting the bar at a hundred thousand would mean the attestation only ever
/// goes to projects that no longer need it.
pub const DEVTOOL_ADOPTION_THRESHOLD: i64 = 500;

// ═══════════════════════════════════════════════════════════════════
// Recognising where a contribution landed
// ═══════════════════════════════════════════════════════════════════

/// Which standards body a URL belongs to, if any.
///
/// Pure, and the part of `code_standard_contribution` worth testing: the rest
/// is a human reading a link. Returns `None` rather than guessing — a
/// contribution filed under the wrong body is a claim about a room the person
/// was never in.
pub fn standards_body(url: &str) -> Option<&'static str> {
    let url = url.trim().to_ascii_lowercase();
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.trim_start_matches("www.");

    match host {
        // TC39 works in the open on GitHub; the organisation is the signal.
        "github.com" if path.starts_with("tc39/") => Some("tc39"),
        "github.com" if path.starts_with("whatwg/") => Some("whatwg"),
        "github.com" if path.starts_with("w3c/") => Some("w3c"),
        "github.com" if path.starts_with("rust-lang/rfcs") => Some("rust"),
        "tc39.es" => Some("tc39"),
        "datatracker.ietf.org" | "ietf.org" | "rfc-editor.org" => Some("ietf"),
        "w3.org" => Some("w3c"),
        "whatwg.org" | "spec.whatwg.org" | "html.spec.whatwg.org" => Some("whatwg"),
        "khronos.org" | "registry.khronos.org" => Some("khronos"),
        "unicode.org" => Some("unicode"),
        "iso.org" => Some("iso"),
        "ecma-international.org" => Some("ecma"),
        _ => None,
    }
}

/// Whether a URL looks like an RFC or design proposal that can be accepted.
///
/// Deliberately narrow. A pull request against a repository called `rfcs` is
/// an RFC; a blog post arguing for one is not, however good it is.
pub fn is_proposal_url(url: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    if standards_body(&url).is_some() {
        return true;
    }
    // Most language and platform communities keep proposals in a repository
    // whose name says so.
    [
        "/rfcs/",
        "/rfc/",
        "/proposals/",
        "/peps/",
        "/pep-",
        "/enhancements/",
    ]
    .iter()
    .any(|marker| url.contains(marker))
}

// ═══════════════════════════════════════════════════════════════════
// Issuing
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize)]
pub struct Evidence {
    /// The public link a reader follows to check the claim.
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
/// The single door: every generator ends here, so the rules that apply to all
/// of them — the basis is one of the seven, an artefact basis names a
/// deliverable, the code is unique — are checked once.
///
/// `attestation_type` is `artefact` for all of these (migration 0198). The
/// type is what kind of statement it is; the basis is what it rests on. A
/// merged contribution to the Linux kernel is not "C, level 4", which is why
/// it is not a `skill`.
pub async fn issue(
    db: &PgPool,
    user_id: Uuid,
    basis: &str,
    evidence: &Evidence,
) -> Result<Issued, AppError> {
    if !BASES.contains(&basis) {
        return Err(AppError::Validation(format!(
            "'{basis}' is not one of the code attestation bases"
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
    if !evidence.url.trim().starts_with("https://") {
        return Err(AppError::Validation(
            "the evidence URL must be a public https link — an attestation nobody can check is worth nothing"
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

    let description = format!("{}\n\n{}", evidence.description.trim(), evidence.url.trim());
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

    metrics::counter!("skilluv_code_attestations_issued_total", "basis" => basis.to_string())
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
// The seven generators
// ═══════════════════════════════════════════════════════════════════

/// 1. A pull request merged into somebody else's project.
///
/// The most valuable of the seven and the easiest to verify, because the
/// platform already did: a deliverable of type `pr_merged` reaches `verified`
/// through the GitHub webhook, which is the merge event itself. Re-asking the
/// API here would be asking the same source the same question twice.
pub async fn pr_merged_upstream(
    db: &PgPool,
    user_id: Uuid,
    deliverable_id: Uuid,
) -> Result<Issued, AppError> {
    let row: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT d.artifact_type, d.artifact_url,
                COALESCE(s.title, ct.title) AS what
           FROM deliverables d
           LEFT JOIN project_slices s ON s.id = d.slice_id
           LEFT JOIN challenge_templates ct ON ct.id = d.challenge_id
          WHERE d.id = $1 AND d.user_id = $2",
    )
    .bind(deliverable_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    let (artifact_type, url, what) =
        row.ok_or_else(|| AppError::NotFound("deliverable not found".into()))?;

    if artifact_type != "pr_merged" {
        return Err(AppError::Validation(format!(
            "this deliverable is a {artifact_type}, not a merged pull request"
        )));
    }

    let what = what.unwrap_or_else(|| "un projet open source".into());
    issue(
        db,
        user_id,
        "code_pr_merged_upstream",
        &Evidence {
            url,
            title: format!("Contribution fusionnée : {what}"),
            description: "Une pull request ouverte sur un projet tiers, relue par ses \
                          mainteneurs et fusionnée dans la branche principale."
                .into(),
            deliverable_id: Some(deliverable_id),
            project_id: None,
            skill_node_ids: vec![],
        },
    )
    .await
}

/// 2. Something shipped and reachable.
///
/// Whether the URL answers is checked by a person, not by a request: a page
/// can answer 200 with an empty shell, and a machine that accepts that is a
/// machine issuing attestations for parked domains.
pub async fn project_shipped(
    db: &PgPool,
    user_id: Uuid,
    deliverable_id: Uuid,
    live_url: &str,
    what: &str,
) -> Result<Issued, AppError> {
    issue(
        db,
        user_id,
        "code_project_shipped",
        &Evidence {
            url: live_url.to_string(),
            title: format!("Projet livré : {what}"),
            description: "Une application livrée et accessible publiquement, vérifiée \
                          par un relecteur Skilluv."
                .into(),
            deliverable_id: Some(deliverable_id),
            project_id: None,
            skill_node_ids: vec![],
        },
    )
    .await
}

/// 3. A library on a registry.
///
/// The registry is asked whether the package exists, through the same
/// identification used by the slice-level statistics. A URL no registry
/// recognises is refused: "published" means published somewhere somebody else
/// can install from.
pub async fn library_published(
    db: &PgPool,
    client: &reqwest::Client,
    user_id: Uuid,
    deliverable_id: Uuid,
    package_url: &str,
) -> Result<Issued, AppError> {
    let package = crate::services::artifact_registry::identify(package_url).ok_or_else(|| {
        AppError::Validation(
            "that URL does not point at a package registry Skilluv knows how to read".into(),
        )
    })?;

    // A registry that answers about the package is the proof it is published.
    // A registry that cannot be reached is not evidence of absence, and is
    // reported as itself rather than as a refusal.
    let stats = crate::services::artifact_registry::fetch(client, &package).await?;

    let version = stats
        .latest_version
        .map(|v| format!(" (version {v})"))
        .unwrap_or_default();

    issue(
        db,
        user_id,
        "code_library_published",
        &Evidence {
            url: package_url.to_string(),
            title: format!("Bibliothèque publiée : {}{version}", package.name),
            description: format!(
                "Un paquet publié sur {}, installable par n'importe qui.",
                package.registry
            ),
            deliverable_id: Some(deliverable_id),
            project_id: None,
            skill_node_ids: vec![],
        },
    )
    .await
}

/// 4. An RFC or design proposal that was accepted.
///
/// Reviewed by a person. There is no API that answers "was this proposal
/// accepted" across the dozen communities that have proposals, and a
/// heuristic on the page text would be wrong in the cases that matter most.
pub async fn rfc_accepted(
    db: &PgPool,
    user_id: Uuid,
    rfc_url: &str,
    what: &str,
) -> Result<Issued, AppError> {
    if !is_proposal_url(rfc_url) {
        return Err(AppError::Validation(
            "that URL does not look like a proposal — an RFC lives in the repository or \
             tracker where its community discusses them"
                .into(),
        ));
    }
    issue(
        db,
        user_id,
        "code_rfc_accepted",
        &Evidence {
            url: rfc_url.to_string(),
            title: format!("RFC acceptée : {what}"),
            description: "Une proposition de conception soumise à une communauté \
                          technique, discutée puis acceptée."
                .into(),
            deliverable_id: None,
            project_id: None,
            skill_node_ids: vec![],
        },
    )
    .await
}

/// 5. A contribution to a standard.
///
/// The rarest of the seven and the one worth the most, which is exactly why
/// the body has to be recognised rather than typed: "I contributed to a
/// standard" is a claim that means nothing without saying which room.
pub async fn standard_contribution(
    db: &PgPool,
    user_id: Uuid,
    contribution_url: &str,
    what: &str,
) -> Result<Issued, AppError> {
    let body = standards_body(contribution_url).ok_or_else(|| {
        AppError::Validation(
            "that URL does not belong to a standards body Skilluv recognises — TC39, IETF, \
             W3C, WHATWG, Khronos, Unicode, ECMA or ISO"
                .into(),
        )
    })?;

    issue(
        db,
        user_id,
        "code_standard_contribution",
        &Evidence {
            url: contribution_url.to_string(),
            title: format!("Contribution à un standard ({body}) : {what}"),
            description: format!(
                "Une contribution retenue dans les travaux de {body}, la forme de \
                 contribution technique la plus durable qui soit."
            ),
            deliverable_id: None,
            project_id: None,
            skill_node_ids: vec![],
        },
    )
    .await
}

/// 6. A developer tool other people actually use.
///
/// Adoption is a number and the number has to clear a stated bar. Without one
/// this attestation would be "I wrote a CLI", which is not the claim it makes.
pub async fn devtool_adopted(
    db: &PgPool,
    client: &reqwest::Client,
    user_id: Uuid,
    package_url: &str,
    what: &str,
) -> Result<Issued, AppError> {
    let package = crate::services::artifact_registry::identify(package_url).ok_or_else(|| {
        AppError::Validation(
            "adoption is measured from a registry, and that URL is not one Skilluv reads".into(),
        )
    })?;

    let stats = crate::services::artifact_registry::fetch(client, &package).await?;
    let reach = stats
        .downloads_recent
        .or(stats.downloads_total)
        .ok_or_else(|| {
            AppError::Validation(format!(
                "{} publishes no usage figures, so adoption cannot be measured there",
                package.registry
            ))
        })?;

    if reach < DEVTOOL_ADOPTION_THRESHOLD {
        return Err(AppError::Validation(format!(
            "{reach} downloads is below the {DEVTOOL_ADOPTION_THRESHOLD} this attestation asks for"
        )));
    }

    issue(
        db,
        user_id,
        "code_devtool_adopted",
        &Evidence {
            url: package_url.to_string(),
            title: format!("Outil adopté : {what}"),
            description: format!(
                "Un outil de développement publié sur {} et repris par d'autres : \
                 {reach} téléchargements mesurés.",
                package.registry
            ),
            deliverable_id: None,
            project_id: None,
            skill_node_ids: vec![],
        },
    )
    .await
}

/// 7. Featured.
///
/// Editorial, and named as such. There is no formula behind it, and inventing
/// one would make it a worse version of the craft score rather than the thing
/// it is: somebody chose to put this person forward, and put their name to it.
pub async fn featured_coder(
    db: &PgPool,
    user_id: Uuid,
    profile_url: &str,
    citation: &str,
) -> Result<Issued, AppError> {
    if citation.trim().is_empty() {
        return Err(AppError::Validation(
            "featuring somebody without saying why is a decision nobody can question".into(),
        ));
    }
    issue(
        db,
        user_id,
        "featured_coder",
        &Evidence {
            url: profile_url.to_string(),
            title: "Développeur mis en avant".into(),
            description: citation.trim().to_string(),
            deliverable_id: None,
            project_id: None,
            skill_node_ids: vec![],
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_standards_body_is_recognised_not_guessed() {
        assert_eq!(
            standards_body("https://github.com/tc39/proposal-decorators"),
            Some("tc39")
        );
        assert_eq!(
            standards_body("https://datatracker.ietf.org/doc/rfc9110/"),
            Some("ietf")
        );
        assert_eq!(
            standards_body("https://www.w3.org/TR/wai-aria/"),
            Some("w3c")
        );
        assert_eq!(
            standards_body("https://html.spec.whatwg.org/multipage/"),
            Some("whatwg")
        );
    }

    #[test]
    fn a_repository_that_is_not_a_standards_body_is_not_one() {
        // The most valuable attestation of the seven must not be reachable by
        // pasting any github URL.
        assert_eq!(standards_body("https://github.com/someone/proposal"), None);
        assert_eq!(standards_body("https://myblog.example/i-fixed-http"), None);
        assert_eq!(standards_body("not a url"), None);
    }

    #[test]
    fn a_proposal_is_recognised_by_where_it_lives() {
        assert!(is_proposal_url(
            "https://github.com/rust-lang/rfcs/pull/3550"
        ));
        assert!(is_proposal_url("https://peps.python.org/pep-0703/"));
        assert!(is_proposal_url(
            "https://github.com/kubernetes/enhancements/issues/1"
        ));
        // An article about an RFC is not an RFC.
        assert!(!is_proposal_url("https://blog.example/why-async-is-hard"));
    }

    #[test]
    fn the_seven_bases_are_the_seven_the_column_allows() {
        assert_eq!(BASES.len(), 7);
        for basis in ARTIFACT_BASES {
            assert!(
                BASES.contains(basis),
                "{basis} must be one of the seven bases"
            );
        }
    }
}
