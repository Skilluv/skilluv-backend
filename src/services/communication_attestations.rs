//! Attestations that rest on a communication artefact.
//!
//! ## The same shape as the AI and audio generators, and one difference
//!
//! Like [`crate::services::ai_attestations`] and
//! [`crate::services::audio_attestations`], every generator re-checks its own
//! precondition against the database before issuing, the attestations are
//! `artefact` ones (migration 0198), and re-running is free because
//! `uniq_attestations_artefact_per_deliverable` makes a second pass insert
//! nothing.
//!
//! The difference is what counts as evidence. In audio the gate is provenance:
//! an untraced sample makes the work unusable. Here the gate is *reach* — not
//! how far the work travelled, but whether a stranger can reach it at all.
//!
//! Four of the five bases claim something published: a talk with a recording,
//! an article at an address, a paper anybody can open. So they are withheld
//! until the slice names where. That is not bureaucracy: an attestation
//! saying "this person published a tutorial" with nothing to open is the
//! fabricated social proof this platform sells against.
//!
//! The two that are not gated on a URL are the two that land in somebody
//! else's repository — a documentation change and a translation. Their
//! evidence is the merge, which `pr_url` carries, and demanding a second
//! address would mean the author inventing one.
//!
//! ## Why a translation needs a bilingual reviewer, and how that is enforced
//!
//! Ticket W-04 asked for one review capability per language —
//! `translator-fr`, `translator-pt`, and so on. That does not scale and does
//! not survive: there are seven thousand languages, `capability_catalog`
//! would grow a row per language anybody ever asks for, and a capability
//! nobody can be granted is a gate nobody passes.
//!
//! What is actually needed is narrower: at the moment a translation is
//! attested, somebody who reads the target language has to have signed off.
//! The reviewer holds `communication_reviewer:translation` — one capability,
//! granted the normal way — and the language competence is a fact about the
//! review, recorded when it happens. [`validate_translation`] is where both
//! are checked, and it is the only door to the basis.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Which basis a communication artefact supports, from what it is.
///
/// `blog_post` and `video_content` map to the same basis. Both are a piece
/// published at an address for an audience that could have left, and the
/// review family that judges them is one family for the same reason —
/// inventing `communication_video_published` next to
/// `communication_article_published` would split one count across two words
/// for a distinction nobody hiring asks about.
fn basis_for_subtype(subtype: &str) -> Option<&'static str> {
    match subtype {
        "documentation" => Some("communication_docs_contribution"),
        "devrel_talk" => Some("communication_talk_delivered"),
        "blog_post" => Some("communication_content_published"),
        "video_content" => Some("communication_content_published"),
        "research_paper" => Some("communication_research_published"),
        // Issued through `validate_translation` rather than from a subtype: a
        // translation is not attested until somebody who reads the target
        // language has said it is right.
        "translation" => None,
        _ => None,
    }
}

/// Whether this basis may only be issued once the work has a public address.
///
/// False for the documentation contribution, which lives in the pull request
/// that carried it. True for everything else: a talk nobody can watch, an
/// article nobody can open and a paper nobody can download are claims with
/// nothing behind them.
fn needs_public_address(basis: &str) -> bool {
    !matches!(basis, "communication_docs_contribution")
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
/// Best-effort and never fatal. The address is the one the work already had —
/// the published URL when the slice names one, the pull request otherwise. A
/// feed line with nothing to open is the fabricated social proof migration
/// 0203 exists to replace, and in this domain it would also be a claim about
/// writing nobody can read.
async fn announce(
    db: &PgPool,
    user_id: Uuid,
    attestation_id: Uuid,
    basis: &str,
    title: &str,
    deliverable_id: Uuid,
) {
    /// Who published it and where it can be read. Named rather than left as a
    /// four-tuple: three of the four are optional URLs, and a caller reading
    /// `.2` instead of `.1` would attach the wrong link.
    #[derive(sqlx::FromRow)]
    struct Context {
        username: String,
        published_artifact_url: Option<String>,
        pr_url: Option<String>,
        artifact_url: Option<String>,
    }

    let context: Result<Option<Context>, _> = sqlx::query_as(
        r#"
        SELECT u.username, ps.published_artifact_url, ps.pr_url, d.artifact_url
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
        pr_url,
        artifact_url,
    })) = context
    else {
        return;
    };
    let Some(url) = published_artifact_url.or(pr_url).or(artifact_url) else {
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
            "communication attestation issued but not announced"
        );
    }
}

/// What one verified communication slice is, as far as the generators care.
#[derive(sqlx::FromRow)]
struct Evidence {
    deliverable_id: Uuid,
    user_id: Uuid,
    subtype: Option<String>,
    slice_type: String,
    published_artifact_url: Option<String>,
    pr_url: Option<String>,
}

async fn evidence_for(db: &PgPool, slice_id: Uuid) -> Result<Option<Evidence>, AppError> {
    // The verified, unrevoked deliverable is the evidence. Without one there
    // is nothing to attest, whatever the slice claims about itself.
    Ok(sqlx::query_as::<_, Evidence>(
        r#"
        SELECT d.id AS deliverable_id, d.user_id, ps.communication_subtype AS subtype,
               ps.slice_type, ps.published_artifact_url, ps.pr_url
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

/// Issue whatever the verified work on this slice earns.
///
/// Returns the bases actually issued, which is empty on a second pass and
/// empty for work that earns none — both normal, neither an error.
pub async fn issue_for_slice(db: &PgPool, slice_id: Uuid) -> Result<Vec<String>, AppError> {
    let Some(ev) = evidence_for(db, slice_id).await? else {
        return Ok(Vec::new());
    };
    if ev.slice_type != "communication_artifact" {
        return Ok(Vec::new());
    }

    let Some(basis) = ev.subtype.as_deref().and_then(basis_for_subtype) else {
        return Ok(Vec::new());
    };

    if needs_public_address(basis) && ev.published_artifact_url.is_none() {
        // Not an error: the work may be fine and the address missing. The next
        // pass issues it, which is what makes filling the field in later work
        // without anybody re-triggering anything.
        tracing::debug!(
            slice = %slice_id, basis,
            "communication attestation withheld until the work names where it is published"
        );
        return Ok(Vec::new());
    }

    let skills = skill_nodes_for_slice(db, slice_id).await?;
    let evidence_url = ev
        .published_artifact_url
        .as_deref()
        .or(ev.pr_url.as_deref());

    let mut issued = Vec::new();
    if issue(
        db,
        ev.user_id,
        basis,
        ev.deliverable_id,
        &skills,
        evidence_url,
    )
    .await?
    .is_some()
    {
        issued.push(basis.to_string());
    }
    Ok(issued)
}

/// Issue whatever every communication artefact this person has earns.
///
/// Called from the proof orchestrator rather than from the point a slice is
/// verified, and deliberately: the published address often arrives *after*
/// verification — an article is reviewed, then it goes out — and hooking the
/// verification alone would leave those permanently unattested, which is the
/// dormant-engine failure P19 exists to end.
///
/// Bounded: somebody with more communication artefacts than this has a
/// profile that needs a look rather than a longer loop, and the next
/// recompute picks up whatever this pass did not reach.
pub async fn issue_for_user(db: &PgPool, user_id: Uuid) -> Result<Vec<String>, AppError> {
    let slices: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT ps.id
          FROM project_slices ps
          JOIN deliverables d ON d.slice_id = ps.id
         WHERE d.user_id = $1
           AND ps.slice_type = 'communication_artifact'
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
        // One failing slice does not stop the others: a missing address on one
        // artefact must not cost somebody the attestation on another.
        match issue_for_slice(db, slice_id).await {
            Ok(mut bases) => issued.append(&mut bases),
            Err(e) => tracing::warn!(
                slice = %slice_id, error = %e,
                "communication attestation generator failed on one artefact"
            ),
        }
    }
    Ok(issued)
}

/// Attest a translation, on the word of somebody who reads the target
/// language.
///
/// Separate from the automatic generators, and the only door to
/// `communication_translation_validated`. Nothing in this database can tell a
/// good translation from a fluent wrong one; the only instrument is a person
/// who reads both languages, and this function records that they did.
///
/// The caller is responsible for the capability check — this is reached
/// through an endpoint guarded by `communication_reviewer:translation`. What
/// is checked *here* is the part a capability cannot express: that the
/// reviewer declared the target language, that the slice is a translation,
/// and that the reviewer is not the translator.
pub async fn validate_translation(
    db: &PgPool,
    reviewer_id: Uuid,
    slice_id: Uuid,
    target_language: &str,
    notes_md: &str,
) -> Result<Option<Uuid>, AppError> {
    let language = target_language.trim();
    crate::validators::check_max_len(language, "target_language", 20)?;
    if language.is_empty() {
        return Err(AppError::Validation(
            "a translation review has to name the language it was read in".into(),
        ));
    }
    crate::validators::check_max_len(notes_md, "notes_md", 8000)?;

    let Some(ev) = evidence_for(db, slice_id).await? else {
        return Err(AppError::Validation(
            "no verified deliverable on that slice".into(),
        ));
    };
    if ev.subtype.as_deref() != Some("translation") {
        return Err(AppError::Validation(
            "that slice is not a translation".into(),
        ));
    }
    if ev.user_id == reviewer_id {
        // The whole value of the basis is that a second person read it.
        return Err(AppError::Validation(
            "a translation is not validated by the person who translated it".into(),
        ));
    }

    // The language has to be one the slice claims to have been carried into.
    // A review in a language the artefact never targeted attests something
    // that did not happen.
    let targeted: bool = sqlx::query_scalar(
        "SELECT $2 = ANY (communication_target_languages)
           FROM project_slices WHERE id = $1",
    )
    .bind(slice_id)
    .bind(language)
    .fetch_optional(db)
    .await?
    .unwrap_or(false);

    if !targeted {
        return Err(AppError::Validation(format!(
            "this translation does not target '{language}'"
        )));
    }

    // And the reviewer has to have said they read it. Declared rather than
    // proven, and that is the honest level: nothing here can test somebody's
    // Swahili, but a person who claimed it in writing can be held to it.
    let reads_it: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM user_review_languages
              WHERE user_id = $1 AND language = $2
         )",
    )
    .bind(reviewer_id)
    .bind(language)
    .fetch_one(db)
    .await?;

    if !reads_it {
        return Err(AppError::Validation(format!(
            "you have not declared that you read '{language}' well enough to review in it"
        )));
    }

    let skills = skill_nodes_for_slice(db, slice_id).await?;
    let evidence_url = ev
        .published_artifact_url
        .as_deref()
        .or(ev.pr_url.as_deref());

    let issued = issue(
        db,
        ev.user_id,
        "communication_translation_validated",
        ev.deliverable_id,
        &skills,
        evidence_url,
    )
    .await?;

    if let Some(attestation_id) = issued {
        // The review itself is kept, so the claim can be traced to a person
        // and a date rather than to an attestation that appeared.
        sqlx::query(
            "INSERT INTO translation_reviews
                 (slice_id, reviewer_user_id, language, notes_md, attestation_id)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (slice_id, reviewer_user_id, language) DO NOTHING",
        )
        .bind(slice_id)
        .bind(reviewer_id)
        .bind(language)
        .bind(notes_md.trim())
        .bind(attestation_id)
        .execute(db)
        .await?;
    }

    Ok(issued)
}

// ════════════════════════════════════════════════════════════════════
// Featured
// ════════════════════════════════════════════════════════════════════

const EDITORIAL: crate::services::artefact_attestations::Domain =
    crate::services::artefact_attestations::Domain {
        name: "communication",
        bases: &["featured_communicator"],
        artifact_bases: &[],
        allows_stored_objects: false,
    };

/// Featured.
///
/// Migration 0504 declared `featured_communicator` and
/// `communication_profile` counts it, and until now nothing issued it: a featuring was
/// recorded, the announcement went out, and the profile term stayed at zero.
/// The same defect ops and audio carried, caught this time by the test that
/// was written for them.
pub async fn featured_communicator(
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
        "featured_communicator",
        &crate::services::artefact_attestations::Evidence {
            url: profile_url.to_string(),
            title: "Featured communicator".into(),
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
    fn an_article_and_a_video_make_the_same_claim() {
        // Both are a piece published for an audience that could have left. A
        // basis each would split one count across two words for a distinction
        // nobody hiring asks about.
        assert_eq!(
            basis_for_subtype("blog_post"),
            Some("communication_content_published")
        );
        assert_eq!(
            basis_for_subtype("video_content"),
            Some("communication_content_published")
        );
    }

    #[test]
    fn a_translation_is_not_attested_automatically() {
        // It has exactly one door, and a person is standing in it.
        assert_eq!(basis_for_subtype("translation"), None);
    }

    #[test]
    fn every_other_subtype_the_schema_allows_earns_something() {
        // The list is migration 0506's CHECK, minus the translation above. A
        // subtype added there and forgotten here would produce artefacts that
        // verify and never attest.
        for subtype in [
            "documentation",
            "devrel_talk",
            "blog_post",
            "video_content",
            "research_paper",
        ] {
            assert!(
                basis_for_subtype(subtype).is_some(),
                "{subtype} earns no attestation"
            );
        }
        assert_eq!(basis_for_subtype("something-else"), None);
    }

    #[test]
    fn only_what_claims_to_be_published_has_to_name_an_address() {
        // A documentation change lives in the pull request that carried it,
        // and demanding a second address would mean inventing one.
        assert!(!needs_public_address("communication_docs_contribution"));
        assert!(needs_public_address("communication_talk_delivered"));
        assert!(needs_public_address("communication_content_published"));
        assert!(needs_public_address("communication_research_published"));
    }
}
