//! Attestations that rest on game work.
//!
//! ## More sources than one, like security
//!
//! Most domains attest one thing: a verified deliverable on a slice. Game,
//! like security, attests several genuinely different objects (migration 0574):
//!
//!   * a **slice** — a build, a design document, an asset, an animation, a
//!     level — carried through review and three playtests. The ordinary case.
//!   * a **jam** — a weekend's finished submission. Everyone who shipped one is
//!     a participant; the top-ranked one is the winner. No slice.
//!   * a **mod** — content hosted inside someone else's game, on a platform we
//!     do not own, confirmed by a reviewer (migration 0583).
//!   * a **shipped title** and an **open-source contribution** — a game that
//!     reached players, a pull request merged upstream — each confirmed by a
//!     reviewer against a deliverable the platform already holds.
//!   * a **playtest milestone** — twenty playtests given to other creators.
//!
//! ## Why the evidence is always a public link
//!
//! `allows_stored_objects` is false. A game's proof is a playable URL, a store
//! page, a repository, a hosting page with a download count — never a file in
//! our own bucket. The whole domain is built on work that reached players, and
//! a proof only we can serve is the opposite of that.

use sqlx::PgPool;
use uuid::Uuid;

use crate::config::PUBLIC_SITE_URL;
use crate::errors::AppError;
use crate::services::artefact_attestations::{self, Domain, Evidence, Issued, Links};

/// Every basis this domain issues, and which of them must name a deliverable.
/// The lists mirror `attestation_bases` (migration 0574); they are here as well
/// because the shared issuer refuses a basis it was not told about, which turns
/// a typo into a refusal at the call site instead of a dangling basis.
pub const GAME: Domain = Domain {
    name: "game",
    bases: &[
        "game_artifact_validated",
        "game_jam_winner",
        "game_jam_participant",
        "game_shipped_title",
        "game_mod_published",
        "game_playtest_hero",
        "game_open_source_contribution",
        "featured_game_creator",
    ],
    artifact_bases: &[
        "game_artifact_validated",
        "game_jam_winner",
        "game_shipped_title",
        "game_mod_published",
        "game_open_source_contribution",
    ],
    allows_stored_objects: false,
};

/// How many playtests given earns the playtest-hero attestation (migration
/// 0574's wording, and the `game_playtest_hero` badge in 0577).
pub const PLAYTEST_HERO_THRESHOLD: i64 = 20;

/// The one basis every game slice earns, whatever its subtype.
///
/// Unlike security — where each trade's artefact earns a different basis — a
/// game slice is a game slice: a validated build, document, asset, animation
/// or level all attest the same thing, that a game deliverable was carried
/// through review and playtesting. The subtype decides how it was reviewed,
/// not what it proves. `None` for anything that is not a game subtype.
pub fn basis_for_subtype(subtype: &str) -> Option<&'static str> {
    match subtype {
        "code_module" | "build_playable" | "gdd_document" | "asset_3d" | "asset_2d_sprite"
        | "animation_pack" | "level_pack" | "mod_package" => Some("game_artifact_validated"),
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════
// Slices
// ═══════════════════════════════════════════════════════════════════

/// Issue what the verified work on this game slice earns. Empty on a second
/// pass and empty for a slice that earns none — neither is an error.
pub async fn issue_for_slice(db: &PgPool, slice_id: Uuid) -> Result<Vec<String>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Ev {
        deliverable_id: Uuid,
        user_id: Uuid,
        artifact_url: String,
        subtype: Option<String>,
        slice_type: String,
        title: String,
        project_id: Option<Uuid>,
    }

    let row: Option<Ev> = sqlx::query_as(
        r#"
        SELECT d.id AS deliverable_id, d.user_id, d.artifact_url,
               ps.game_artifact_subtype AS subtype, ps.slice_type, ps.title,
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
    if ev.slice_type != "game_artifact" {
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
        &GAME,
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
// Mods
// ═══════════════════════════════════════════════════════════════════

/// Issue `game_mod_published` for a confirmed mod.
///
/// The deliverable is created by [`crate::services::game_mods::confirm`] the
/// moment a reviewer confirms the mod (migration 0585 made a confirmed mod a
/// deliverable so it counts toward the cross-domain rank). This reads that
/// deliverable and attests it. Empty if the mod is not confirmed or its
/// deliverable is not there yet.
pub async fn issue_for_mod(db: &PgPool, mod_id: Uuid) -> Result<Vec<String>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        author_user_id: Uuid,
        title: String,
        external_hosting_url: String,
        status: String,
        deliverable_id: Option<Uuid>,
    }

    let row: Option<Row> = sqlx::query_as(
        r#"
        SELECT gm.author_user_id, gm.title, gm.external_hosting_url, gm.status,
               -- A standalone mod's own deliverable, or the slice's when the
               -- mod was registered against a game_artifact slice (0583).
               COALESCE(dm.id, ds.id) AS deliverable_id
          FROM game_mods gm
          LEFT JOIN deliverables dm
                 ON dm.game_mod_id = gm.id
                AND dm.verification_status = 'verified'
                AND dm.revoked_at IS NULL
          LEFT JOIN deliverables ds
                 ON ds.slice_id = gm.slice_id
                AND ds.verification_status = 'verified'
                AND ds.revoked_at IS NULL
         WHERE gm.id = $1
        "#,
    )
    .bind(mod_id)
    .fetch_optional(db)
    .await?;

    let Some(m) = row else {
        return Ok(Vec::new());
    };
    if m.status != "confirmed" {
        return Ok(Vec::new());
    }
    let Some(deliverable_id) = m.deliverable_id else {
        // Confirmed but no deliverable — confirm() creates it in the same
        // transaction, so this is a torn write to log, not to paper over.
        tracing::warn!(mod_id = %mod_id, "confirmed mod has no deliverable to attest");
        return Ok(Vec::new());
    };

    let (title, description) =
        crate::services::attestations::basis_wording(db, "game_mod_published").await;

    let issued = artefact_attestations::issue_linked(
        db,
        m.author_user_id,
        "game_mod_published",
        &Evidence {
            url: m.external_hosting_url,
            title,
            description: format!("{description}\n\n{}", m.title),
            deliverable_id: Some(deliverable_id),
            project_id: None,
            skill_node_ids: vec![],
        },
        Links {
            game_mod_id: Some(mod_id),
            ..Default::default()
        },
        &GAME,
    )
    .await?;

    Ok(vec![issued.basis])
}

// ═══════════════════════════════════════════════════════════════════
// Reviewer-confirmed: shipped titles and upstream contributions
// ═══════════════════════════════════════════════════════════════════

/// Issue `game_shipped_title` — a game that reached players, confirmed by a
/// reviewer against a deliverable the person already has.
///
/// The store or itch page is both the evidence link and the value of
/// `external_publish_url` (migration 0585): the page the attestation vouches
/// for, kept as its own column so a listing can link to it without parsing the
/// description.
pub async fn issue_shipped_title(
    db: &PgPool,
    user_id: Uuid,
    deliverable_id: Uuid,
    store_url: &str,
    title_text: &str,
) -> Result<Issued, AppError> {
    let (title, description) =
        crate::services::attestations::basis_wording(db, "game_shipped_title").await;
    artefact_attestations::issue_linked(
        db,
        user_id,
        "game_shipped_title",
        &Evidence {
            url: store_url.to_string(),
            title,
            description: format!("{description}\n\n{title_text}"),
            deliverable_id: Some(deliverable_id),
            project_id: None,
            skill_node_ids: vec![],
        },
        Links {
            external_publish_url: Some(store_url.to_string()),
            ..Default::default()
        },
        &GAME,
    )
    .await
}

/// Issue `game_open_source_contribution` — a pull request merged into an engine
/// or an open-source game, confirmed against its merged-PR deliverable.
pub async fn issue_open_source_contribution(
    db: &PgPool,
    user_id: Uuid,
    deliverable_id: Uuid,
    pr_url: &str,
    what_changed: &str,
) -> Result<Issued, AppError> {
    let (title, description) =
        crate::services::attestations::basis_wording(db, "game_open_source_contribution").await;
    artefact_attestations::issue(
        db,
        user_id,
        "game_open_source_contribution",
        &Evidence {
            url: pr_url.to_string(),
            title,
            description: format!("{description}\n\n{what_changed}"),
            deliverable_id: Some(deliverable_id),
            project_id: None,
            skill_node_ids: vec![],
        },
        &GAME,
    )
    .await
}

// ═══════════════════════════════════════════════════════════════════
// Jams
// ═══════════════════════════════════════════════════════════════════

/// The users behind a tournament participant: the user themselves, or every
/// member of the guild. A team jam is won by a team, and each member earns the
/// attestation on their own profile.
async fn members_of(
    db: &PgPool,
    participant_type: &str,
    participant_id: Uuid,
) -> Result<Vec<Uuid>, AppError> {
    match participant_type {
        "user" => Ok(vec![participant_id]),
        "guild" => Ok(sqlx::query_scalar(
            "SELECT user_id FROM guild_members WHERE guild_id = $1 ORDER BY user_id",
        )
        .bind(participant_id)
        .fetch_all(db)
        .await?),
        other => {
            tracing::warn!(
                participant_type = other,
                "unknown participant type in a game jam"
            );
            Ok(Vec::new())
        }
    }
}

/// Issue what a concluded jam earns: `game_jam_participant` to everyone who
/// shipped a submission, and `game_jam_winner` to the members of the top-ranked
/// one. Idempotent — safe to call again on the same concluded jam.
///
/// The winner's basis rests on a deliverable, so one is created per member from
/// the winning submission (a jam game is a `playable_build`). The participant's
/// basis rests on the jam alone, keyed by `game_jam_id` so a person's many jams
/// each attest once (migration 0585).
pub async fn finalize_jam_attestations(db: &PgPool, jam_id: Uuid) -> Result<Vec<String>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Sub {
        submission_id: Uuid,
        participant_type: String,
        participant_id: Uuid,
        artifact_url: String,
        rank: Option<i32>,
    }

    let tournament_id: Option<Uuid> =
        sqlx::query_scalar("SELECT tournament_id FROM game_jams WHERE id = $1")
            .bind(jam_id)
            .fetch_optional(db)
            .await?;
    let Some(tournament_id) = tournament_id else {
        return Ok(Vec::new());
    };

    let subs: Vec<Sub> = sqlx::query_as(
        r#"
        SELECT s.id AS submission_id, s.participant_type, s.participant_id,
               s.artifact_url, p.rank
          FROM tournament_submissions s
          LEFT JOIN tournament_participants p
                 ON p.tournament_id = s.tournament_id
                AND p.participant_type = s.participant_type
                AND p.participant_id = s.participant_id
         WHERE s.tournament_id = $1
        "#,
    )
    .bind(tournament_id)
    .fetch_all(db)
    .await?;

    let (p_title, p_desc) =
        crate::services::attestations::basis_wording(db, "game_jam_participant").await;
    let (w_title, w_desc) =
        crate::services::attestations::basis_wording(db, "game_jam_winner").await;

    let mut issued = Vec::new();
    for sub in subs {
        let members = members_of(db, &sub.participant_type, sub.participant_id).await?;
        let is_winner = sub.rank == Some(1);

        for member in members {
            // Everyone who shipped is a participant.
            match artefact_attestations::issue_linked(
                db,
                member,
                "game_jam_participant",
                &Evidence {
                    url: format!("{PUBLIC_SITE_URL}/game/jams/{jam_id}"),
                    title: p_title.clone(),
                    description: p_desc.clone(),
                    deliverable_id: None,
                    project_id: None,
                    skill_node_ids: vec![],
                },
                Links {
                    game_jam_id: Some(jam_id),
                    ..Default::default()
                },
                &GAME,
            )
            .await
            {
                Ok(out) => issued.push(out.basis),
                Err(e) => tracing::warn!(jam = %jam_id, user = %member, error = %e,
                    "jam participant attestation failed for one member"),
            }

            if !is_winner {
                continue;
            }

            // The winner's basis rests on a deliverable — create it from the
            // winning submission, one per member, verified because the jam
            // conclusion is the verification.
            let deliverable_id: Option<Uuid> = sqlx::query_scalar(
                r#"
                INSERT INTO deliverables
                    (tournament_submission_id, user_id, artifact_type, artifact_url,
                     verifiable_by, verification_status, verified_at,
                     fragments_awarded, credits_awarded, public, submitted_at, created_at)
                VALUES ($1, $2, 'playable_build', $3, 'human_review', 'verified', NOW(),
                        0, 0, TRUE, NOW(), NOW())
                ON CONFLICT DO NOTHING
                RETURNING id
                "#,
            )
            .bind(sub.submission_id)
            .bind(member)
            .bind(&sub.artifact_url)
            .fetch_optional(db)
            .await?;

            // Already there on a second pass — read it back.
            let deliverable_id = match deliverable_id {
                Some(id) => id,
                None => {
                    sqlx::query_scalar(
                        "SELECT id FROM deliverables
                          WHERE tournament_submission_id = $1 AND user_id = $2
                            AND revoked_at IS NULL LIMIT 1",
                    )
                    .bind(sub.submission_id)
                    .bind(member)
                    .fetch_one(db)
                    .await?
                }
            };

            match artefact_attestations::issue_linked(
                db,
                member,
                "game_jam_winner",
                &Evidence {
                    url: sub.artifact_url.clone(),
                    title: w_title.clone(),
                    description: w_desc.clone(),
                    deliverable_id: Some(deliverable_id),
                    project_id: None,
                    skill_node_ids: vec![],
                },
                Links {
                    game_jam_id: Some(jam_id),
                    ..Default::default()
                },
                &GAME,
            )
            .await
            {
                Ok(out) => issued.push(out.basis),
                Err(e) => tracing::warn!(jam = %jam_id, user = %member, error = %e,
                    "jam winner attestation failed for one member"),
            }
        }
    }

    Ok(issued)
}

// ═══════════════════════════════════════════════════════════════════
// Playtest milestone
// ═══════════════════════════════════════════════════════════════════

/// Issue `game_playtest_hero` once a person has given the threshold of
/// playtests. No deliverable — it is recognition of service to the domain, and
/// it does not move a rank (migration 0574). Empty below the threshold or on a
/// second pass.
pub async fn issue_playtest_hero(db: &PgPool, user_id: Uuid) -> Result<Vec<String>, AppError> {
    let given: i64 =
        sqlx::query_scalar("SELECT count(*) FROM game_playtests WHERE playtester_user_id = $1")
            .bind(user_id)
            .fetch_one(db)
            .await?;
    if given < PLAYTEST_HERO_THRESHOLD {
        return Ok(Vec::new());
    }

    let (title, description) =
        crate::services::attestations::basis_wording(db, "game_playtest_hero").await;
    let issued = artefact_attestations::issue(
        db,
        user_id,
        "game_playtest_hero",
        &Evidence {
            url: format!("{PUBLIC_SITE_URL}/game/creators/{user_id}"),
            title,
            description,
            deliverable_id: None,
            project_id: None,
            skill_node_ids: vec![],
        },
        &GAME,
    )
    .await?;
    Ok(vec![issued.basis])
}

// ═══════════════════════════════════════════════════════════════════
// Featured
// ═══════════════════════════════════════════════════════════════════

/// Featured game creator of the week (migration 0584). Editorial, like every
/// other domain's featuring: no formula, a human's judgement, and an
/// attestation that says so. The basis is declared in `GAME` and counted by the
/// `featured_times` craft-score weight, so the featuring has to actually issue
/// it.
pub async fn featured_game_creator(
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
    artefact_attestations::issue(
        db,
        user_id,
        "featured_game_creator",
        &Evidence {
            url: profile_url.to_string(),
            title: "Featured game creator".into(),
            description: citation.trim().to_string(),
            deliverable_id: None,
            project_id: None,
            skill_node_ids: vec![],
        },
        &GAME,
    )
    .await
}

// ═══════════════════════════════════════════════════════════════════
// Sweep
// ═══════════════════════════════════════════════════════════════════

/// Issue whatever every piece of game work this person has earns. Called from
/// the proof orchestrator, for the reason P19 wrote down: a mod is confirmed
/// and a jam concludes well after the slice they started from was verified, and
/// hooking only the moment of verification would leave the later halves
/// unattested. Bounded — the next recompute reaches what this pass did not.
pub async fn issue_for_user(db: &PgPool, user_id: Uuid) -> Result<Vec<String>, AppError> {
    let mut issued = Vec::new();

    let slices: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT ps.id
           FROM project_slices ps
           JOIN deliverables d ON d.slice_id = ps.id
          WHERE d.user_id = $1
            AND ps.slice_type = 'game_artifact'
            AND d.verification_status = 'verified'
            AND d.revoked_at IS NULL
          LIMIT 200",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    for slice_id in slices {
        match issue_for_slice(db, slice_id).await {
            Ok(mut b) => issued.append(&mut b),
            Err(e) => {
                tracing::warn!(slice = %slice_id, error = %e, "game slice attestation failed")
            }
        }
    }

    let mods: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM game_mods
          WHERE author_user_id = $1 AND status = 'confirmed'
          ORDER BY registered_at DESC LIMIT 200",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    for mod_id in mods {
        match issue_for_mod(db, mod_id).await {
            Ok(mut b) => issued.append(&mut b),
            Err(e) => tracing::warn!(mod_id = %mod_id, error = %e, "game mod attestation failed"),
        }
    }

    match issue_playtest_hero(db, user_id).await {
        Ok(mut b) => issued.append(&mut b),
        Err(e) => tracing::warn!(user = %user_id, error = %e, "playtest hero attestation failed"),
    }

    Ok(issued)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_game_subtype_maps_to_the_validated_basis() {
        // The eight game_artifact subtypes, from migration 0575. A subtype
        // whose basis is not in `GAME.bases` would be refused by the shared
        // issuer at runtime, on somebody's account.
        for subtype in [
            "code_module",
            "build_playable",
            "gdd_document",
            "asset_3d",
            "asset_2d_sprite",
            "animation_pack",
            "level_pack",
            "mod_package",
        ] {
            let basis =
                basis_for_subtype(subtype).unwrap_or_else(|| panic!("{subtype} has no basis"));
            assert_eq!(basis, "game_artifact_validated");
            assert!(GAME.bases.contains(&basis));
        }
        assert!(basis_for_subtype("not_a_game_subtype").is_none());
    }

    #[test]
    fn the_recognition_bases_require_no_deliverable() {
        // Migration 0574's editorial position, asserted rather than trusted: a
        // jam entry and a playtest milestone must never create a deliverable,
        // because a deliverable is what moves a rank.
        for basis in [
            "game_jam_participant",
            "game_playtest_hero",
            "featured_game_creator",
        ] {
            assert!(GAME.bases.contains(&basis));
            assert!(
                !GAME.artifact_bases.contains(&basis),
                "{basis} must not require a deliverable"
            );
        }
    }

    #[test]
    fn the_shipped_work_bases_require_a_deliverable() {
        for basis in [
            "game_artifact_validated",
            "game_jam_winner",
            "game_shipped_title",
            "game_mod_published",
            "game_open_source_contribution",
        ] {
            assert!(
                GAME.artifact_bases.contains(&basis),
                "{basis} should rest on a deliverable"
            );
        }
    }

    #[test]
    fn every_artifact_basis_is_a_declared_basis() {
        for basis in GAME.artifact_bases {
            assert!(
                GAME.bases.contains(basis),
                "{basis} requires a deliverable but is not a declared game basis"
            );
        }
    }
}
