//! SKI-43 (Post-MVP T2-04) — rich, actionable notifications on promotion.
//!
//! A user can currently reach Ranger without ever being told. This module
//! turns each proof-engine outcome into a notification carrying a concrete
//! next step: what the promotion unlocked, and where to go next.
//!
//! ## Why this is not a plain `NotificationService::send` call
//!
//! `NotificationService::send` needs `db + redis + ws`. The proof engine
//! entry point, `proof_hooks::recompute_all_for_user`, only has `db` — and
//! most of its callers (`services::deliverables`, `services::reviews`,
//! `services::slice_validation`) are service-layer functions with no
//! access to `AppState` at all. Threading `AppState` down into them to
//! deliver a notification would invert the dependency between the service
//! and the HTTP layers.
//!
//! Instead the delivery degrades explicitly:
//!
//! * The **database row is always written** — that is the durable channel,
//!   the one `GET /api/notifications` reads, and it works with `db` alone.
//! * The **live channels** (Redis unread counter, WebSocket push, mobile
//!   push) are delivered when the caller can supply them via [`LiveChannel`],
//!   and skipped otherwise.
//!
//! So a promotion from a background webhook is always recorded and shows
//! up on next poll; a promotion from a request that carries `AppState`
//! additionally lights up in real time. Nothing is ever silently lost.

use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::proof_hooks::ProofRecomputeReport;
use crate::services::ranks;
use crate::websocket::{WsManager, WsMessage};

/// Typed notification kinds emitted by this module. Stored verbatim in
/// `notifications.notification_type`, which the front end switches on to
/// pick an icon and a CTA target.
pub const TYPE_RANK_PROMOTION: &str = "rank_promotion";
pub const TYPE_CAPABILITY_GRANTED: &str = "capability_granted";
pub const TYPE_BADGE_AWARDED: &str = "badge_awarded";
pub const TYPE_FIRST_VERIFIED_DELIVERABLE: &str = "first_verified_deliverable";
pub const TYPE_MILESTONE_REACHED: &str = "milestone_reached";

/// How many unlocked slices to name in a rank-promotion notification.
const UNLOCK_SAMPLE_SIZE: i64 = 3;

/// The live delivery channels, when the caller has them.
///
/// Borrowed rather than owned so a request handler can hand over its
/// existing `AppState` clones without extra allocation.
pub struct LiveChannel<'a> {
    pub redis: &'a mut ConnectionManager,
    pub ws: &'a WsManager,
}

/// One notification, before delivery.
struct Draft {
    notification_type: &'static str,
    title: String,
    body: String,
    data: serde_json::Value,
}

/// Persist a notification and, when a live channel is available, push it.
///
/// The DB insert is the only fallible step that matters: live delivery is
/// best-effort and its failures are logged, never propagated, so a Redis
/// blip cannot fail the proof recompute that triggered it.
async fn deliver(
    db: &PgPool,
    user_id: Uuid,
    draft: &Draft,
    live: &mut Option<LiveChannel<'_>>,
) -> Result<Uuid, AppError> {
    let notification_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO notifications (user_id, notification_type, title, body, data)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        "#,
    )
    .bind(user_id)
    .bind(draft.notification_type)
    .bind(&draft.title)
    .bind(&draft.body)
    .bind(&draft.data)
    .fetch_one(db)
    .await?;

    if let Some(channel) = live.as_mut() {
        let counter_key = format!("notifications:unread:{user_id}");
        if let Err(e) = channel.redis.incr::<_, _, i64>(&counter_key, 1).await {
            tracing::debug!(error = %e, user_id = %user_id, "SKI-43: unread counter bump failed");
        }

        channel
            .ws
            .send_to_user(
                user_id,
                WsMessage {
                    event: "notification".to_string(),
                    room: None,
                    payload: json!({
                        "id": notification_id,
                        "type": draft.notification_type,
                        "title": draft.title,
                        "body": draft.body,
                        "data": draft.data,
                    }),
                },
            )
            .await;

        let msg = crate::services::mobile_push::MobilePushMessage {
            title: &draft.title,
            body: &draft.body,
            data: Some(draft.data.clone()),
        };
        if let Err(e) = crate::services::mobile_push::push_to_user_mobile(db, user_id, msg).await {
            tracing::debug!(error = %e, user_id = %user_id, "SKI-43: mobile push failed");
        }
    }

    metrics::counter!(
        "skilluv_promotion_notifications_total",
        "type" => draft.notification_type,
    )
    .increment(1);

    Ok(notification_id)
}

/// Human-readable rank name for notification copy.
fn rank_label(rank: &str) -> &str {
    match rank {
        ranks::RANK_APPRENTI => "Apprenti",
        ranks::RANK_RANGER => "Ranger",
        ranks::RANK_ARTISAN => "Artisan",
        ranks::RANK_MAITRE => "Maître",
        ranks::RANK_DOYEN => "Doyen",
        other => other,
    }
}

/// Human-readable capability name for notification copy.
fn capability_label(slug: &str) -> &str {
    match slug {
        "challenger" => "Challenger",
        "mentor" => "Mentor",
        "project_steward" => "Steward de projet",
        "pr_reviewer" => "Reviewer de PR",
        "bounty_funder" => "Financeur de bounty",
        "issue_proposer" => "Proposeur d'issues",
        "jury_tournament" => "Jury de tournoi",
        "enterprise_recruiter" => "Recruteur entreprise",
        "community_moderator" => "Modérateur communauté",
        "community_curator" => "Curateur communauté",
        "forum_moderator" => "Modérateur forum",
        "plagiarism_reviewer" => "Reviewer anti-plagiat",
        "kyc_reviewer" => "Reviewer KYC",
        other => other,
    }
}

/// Slices that became claimable at `rank`, used as the promotion's payoff.
///
/// Only slices gated at exactly this rank are listed: a Ranger promotion
/// should show what Ranger unlocked, not re-list everything an Apprenti
/// could already claim.
async fn unlocked_slices(
    db: &PgPool,
    rank: &str,
) -> Result<(i64, Vec<serde_json::Value>), AppError> {
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM project_slices
          WHERE min_rank = $1 AND status = 'open' AND claimed_by_user_id IS NULL",
    )
    .bind(rank)
    .fetch_one(db)
    .await
    .unwrap_or(0);

    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, title FROM project_slices
          WHERE min_rank = $1 AND status = 'open' AND claimed_by_user_id IS NULL
          ORDER BY created_at DESC
          LIMIT $2",
    )
    .bind(rank)
    .bind(UNLOCK_SAMPLE_SIZE)
    .fetch_all(db)
    .await
    .unwrap_or_default();

    let sample = rows
        .into_iter()
        .map(|(id, title)| json!({ "slice_id": id, "title": title }))
        .collect();
    Ok((total, sample))
}

/// Emit notifications for everything a proof recompute changed.
///
/// Returns the ids of the notifications written. Called by
/// `proof_hooks::recompute_all_for_user`, so it must stay cheap when
/// nothing changed: a report with no promotions issues zero queries.
pub async fn notify_from_report(
    db: &PgPool,
    report: &ProofRecomputeReport,
    mut live: Option<LiveChannel<'_>>,
) -> Result<Vec<Uuid>, AppError> {
    let mut drafts: Vec<Draft> = Vec::new();

    if report.rank_promoted {
        let rank = &report.rank_computed;
        let (unlocked_count, sample) = unlocked_slices(db, rank).await?;
        let label = rank_label(rank);

        let body = if unlocked_count > 0 {
            format!(
                "Tu viens d'être promu {label}. {unlocked_count} slice(s) sont désormais à ta portée."
            )
        } else {
            format!(
                "Tu viens d'être promu {label}. Ton profil affiche désormais ce rang aux recruteurs."
            )
        };

        drafts.push(Draft {
            notification_type: TYPE_RANK_PROMOTION,
            title: format!("Nouveau rang : {label}"),
            body,
            data: json!({
                "from_rank": report.rank_previous,
                "to_rank": rank,
                // Enriched template fields the front end renders as a CTA.
                "unlock_hint": {
                    "unlocked_slices_count": unlocked_count,
                    "sample": sample,
                },
                "next_step_cta": {
                    "label": "Voir les slices débloquées",
                    "href": format!("/slices?min_rank={rank}"),
                },
            }),
        });
    }

    for slug in &report.capabilities_granted {
        let label = capability_label(slug);
        drafts.push(Draft {
            notification_type: TYPE_CAPABILITY_GRANTED,
            title: format!("Nouveau rôle débloqué : {label}"),
            body: format!(
                "Tes preuves t'ouvrent le rôle {label}. Il est actif immédiatement sur ton compte."
            ),
            data: json!({
                "capability": slug,
                "next_step_cta": {
                    "label": "Découvrir ce rôle",
                    "href": format!("/capabilities/{slug}"),
                },
            }),
        });
    }

    for slug in &report.badges_awarded {
        drafts.push(Draft {
            notification_type: TYPE_BADGE_AWARDED,
            title: "Nouveau badge obtenu".to_string(),
            body: format!("Le badge « {slug} » vient d'être ajouté à ton profil."),
            data: json!({
                "badge_slug": slug,
                "next_step_cta": {
                    "label": "Voir mes badges",
                    "href": "/profile/badges",
                },
            }),
        });
    }

    // First verified deliverable — the single most important moment in a
    // new user's life on the platform, and one that no rank threshold
    // catches (rank stays `apprenti` until the fourth).
    if let Some(draft) = first_deliverable_draft(db, report.user_id).await? {
        drafts.push(draft);
    }

    // Goals (SKI-38) that just crossed 100%.
    drafts.extend(milestone_drafts(db, report.user_id).await?);

    let mut ids = Vec::with_capacity(drafts.len());
    for draft in &drafts {
        match deliver(db, report.user_id, draft, &mut live).await {
            Ok(id) => ids.push(id),
            // Best-effort, consistent with the rest of the proof pipeline:
            // a failed notification must not roll back a real promotion.
            Err(e) => tracing::warn!(
                user_id = %report.user_id,
                notification_type = draft.notification_type,
                error = %e,
                "SKI-43: notification delivery failed"
            ),
        }
    }
    Ok(ids)
}

/// Draft for the very first verified deliverable, or `None`.
///
/// Guarded by an existence check on a prior notification of the same type
/// rather than by a flag column: the notification table already records
/// "we told them", and adding a column would be a second source of truth.
async fn first_deliverable_draft(db: &PgPool, user_id: Uuid) -> Result<Option<Draft>, AppError> {
    let verified: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deliverables
          WHERE user_id = $1 AND verification_status = 'verified'",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if verified != 1 {
        return Ok(None);
    }

    let already_told: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM notifications
              WHERE user_id = $1 AND notification_type = $2
         )",
    )
    .bind(user_id)
    .bind(TYPE_FIRST_VERIFIED_DELIVERABLE)
    .fetch_one(db)
    .await?;
    if already_told {
        return Ok(None);
    }

    Ok(Some(Draft {
        notification_type: TYPE_FIRST_VERIFIED_DELIVERABLE,
        title: "Ta première preuve est vérifiée".to_string(),
        body: "Elle est désormais opposable et visible sur ton profil public. \
               Trois de plus et tu passes Ranger."
            .to_string(),
        data: json!({
            "verified_count": verified,
            "next_step_cta": {
                "label": "Voir mon profil public",
                "href": "/profile",
            },
        }),
    }))
}

/// Drafts for goals that just reached 100%.
///
/// [`crate::services::goals::mark_achieved_goals`] stamps `achieved_at`
/// and returns only the goals it stamped this call, so each milestone
/// notifies exactly once without needing its own dedup check.
async fn milestone_drafts(db: &PgPool, user_id: Uuid) -> Result<Vec<Draft>, AppError> {
    let achieved = crate::services::goals::mark_achieved_goals(db, user_id).await?;
    if achieved.is_empty() {
        return Ok(Vec::new());
    }

    let goals: Vec<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, kind, target_value FROM user_goals WHERE id = ANY($1)")
            .bind(&achieved)
            .fetch_all(db)
            .await?;

    Ok(goals
        .into_iter()
        .map(|(id, kind, target)| {
            let what = match kind.as_str() {
                "rank" => format!("atteindre le rang {}", rank_label(&target)),
                "skill_level" => format!("atteindre le niveau {target} sur une compétence"),
                "capability" => format!("débloquer le rôle {}", capability_label(&target)),
                "artifact_count" => format!("publier {target} preuves vérifiées"),
                other => other.to_string(),
            };
            Draft {
                notification_type: TYPE_MILESTONE_REACHED,
                title: "Objectif atteint".to_string(),
                body: format!("Tu t'étais fixé de {what}. C'est fait."),
                data: json!({
                    "goal_id": id,
                    "kind": kind,
                    "target_value": target,
                    "next_step_cta": {
                        "label": "Fixer un nouvel objectif",
                        "href": "/profile/goals",
                    },
                }),
            }
        })
        .collect())
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn rank_labels_cover_every_rank() {
        for r in ranks::rank_order() {
            let label = rank_label(r);
            assert_ne!(label, *r, "rank {r} has no display label");
        }
        // Unknown values pass through rather than panicking on live data.
        assert_eq!(rank_label("legende"), "legende");
    }

    #[test]
    fn capability_labels_pass_unknown_slugs_through() {
        assert_eq!(capability_label("mentor"), "Mentor");
        assert_eq!(capability_label("brand_new_cap"), "brand_new_cap");
    }

    #[test]
    fn notification_types_are_distinct() {
        let all = [
            TYPE_RANK_PROMOTION,
            TYPE_CAPABILITY_GRANTED,
            TYPE_BADGE_AWARDED,
            TYPE_FIRST_VERIFIED_DELIVERABLE,
            TYPE_MILESTONE_REACHED,
        ];
        let unique: std::collections::HashSet<_> = all.iter().collect();
        assert_eq!(unique.len(), all.len());
        // notifications.notification_type is VARCHAR(50).
        for t in all {
            assert!(t.len() <= 50, "{t} exceeds the column width");
        }
    }
}
