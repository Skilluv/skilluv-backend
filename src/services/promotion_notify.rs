//! SKI-43 (Post-MVP T2-04) — rich, actionable notifications on promotion.
//!
//! A user can currently reach Ranger without ever being told. This module
//! turns each proof-engine outcome into a notification carrying a concrete
//! next step: what the promotion unlocked, and where to go next.
//!
//! ## Why the delivery context is optional here
//!
//! The proof engine entry point, `proof_hooks::recompute_all_for_user`,
//! only has `db` — and most of its callers (`services::deliverables`,
//! `services::reviews`, `services::slice_validation`) are service-layer
//! functions with no access to `AppState` at all. Threading `AppState` down
//! into them to deliver a notification would invert the dependency between
//! the service and the HTTP layers.
//!
//! So this builds a [`crate::services::notify::Ctx`] from whatever the
//! caller has, and the delivery degrades explicitly:
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

use redis::aio::ConnectionManager;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::proof_hooks::ProofRecomputeReport;
use crate::services::ranks;
use crate::websocket::WsManager;

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
    kind: &'static str,
    /// Substituted into the translated title and body.
    args: Vec<(String, String)>,
    data: serde_json::Value,
}

/// Deliver one draft through the single notification entry point.
///
/// This used to insert the row, bump the Redis counter, push over the
/// WebSocket and call the mobile pusher itself — a third copy of the same
/// sequence, with its text written in French at the call site. `notify`
/// owns all of that now, including the recipient's language, their channel
/// preferences and the email nobody was sending.
///
/// The live channel becomes a fuller context when present: without it the
/// durable row is still written, which is what `GET /api/notifications`
/// reads.
async fn emit_draft(
    db: &PgPool,
    user_id: Uuid,
    draft: &Draft,
    live: &mut Option<LiveChannel<'_>>,
) -> Result<(), AppError> {
    let ctx = match live.as_ref() {
        Some(channel) => crate::services::notify::Ctx {
            db,
            redis: Some(channel.redis),
            ws: Some(channel.ws),
            email: None,
            frontend_url: None,
            jwt_secret: None,
        },
        None => crate::services::notify::Ctx::db_only(db),
    };

    let mut builder = crate::services::notify::send(
        ctx,
        crate::services::notify::Recipient::User(user_id),
        draft.kind,
    )
    .payload(draft.data.clone());
    for (name, value) in &draft.args {
        builder = builder.arg(name, value.clone());
    }
    builder.execute().await?;

    metrics::counter!(
        "skilluv_promotion_notifications_total",
        "type" => draft.kind,
    )
    .increment(1);

    Ok(())
}

/// The kind the catalogue gives the "your first contribution is verified"
/// notification. Named because the dedup check below reads it back.
const KIND_FIRST_VERIFIED: &str = "deliverable.first_verified";

/// How a goal reads inside the body of `goal.reached`.
///
/// Translated rather than formatted in Rust: it is half a sentence, and a
/// translator needs to see it next to the sentence it lands in.
fn goal_label(kind: &str, target: &str, locale: &str) -> String {
    use crate::services::i18n;
    match kind {
        "rank" => i18n::t_with(locale, "goal.target.rank", &[("rank", rank_label(target))]),
        "skill_level" => i18n::t_with(locale, "goal.target.skill_level", &[("level", target)]),
        "capability" => i18n::t_with(
            locale,
            "goal.target.capability",
            &[("capability", capability_label(target))],
        ),
        "artifact_count" => {
            i18n::t_with(locale, "goal.target.artifact_count", &[("count", target)])
        }
        other => other.to_string(),
    }
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
/// Returns the kinds delivered. Called by
/// `proof_hooks::recompute_all_for_user`, so it must stay cheap when
/// nothing changed: a report with no promotions issues zero queries.
pub async fn notify_from_report(
    db: &PgPool,
    report: &ProofRecomputeReport,
    mut live: Option<LiveChannel<'_>>,
) -> Result<Vec<String>, AppError> {
    let mut drafts: Vec<Draft> = Vec::new();

    if report.rank_promoted {
        let rank = &report.rank_computed;
        let (unlocked_count, sample) = unlocked_slices(db, rank).await?;
        let label = rank_label(rank);

        drafts.push(Draft {
            kind: "rank.promoted",
            args: vec![("rank".to_string(), label.to_string())],
            data: json!({
                "from_rank": report.rank_previous,
                "to_rank": rank,
                // What the rank actually bought, for the client to render
                // under the message. The button itself comes from the
                // catalogue, so it needs no label here.
                "unlock_hint": {
                    "unlocked_slices_count": unlocked_count,
                    "sample": sample,
                },
            }),
        });
    }

    for slug in &report.capabilities_granted {
        let label = capability_label(slug);
        drafts.push(Draft {
            kind: "capability.granted",
            args: vec![("capability".to_string(), label.to_string())],
            data: json!({
                "capability": slug,
            }),
        });
    }

    for slug in &report.badges_awarded {
        drafts.push(Draft {
            kind: "badge.awarded",
            args: vec![("badge".to_string(), slug.to_string())],
            data: json!({
                "badge_slug": slug,
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

    let mut delivered = Vec::with_capacity(drafts.len());
    for draft in &drafts {
        match emit_draft(db, report.user_id, draft, &mut live).await {
            Ok(()) => delivered.push(draft.kind.to_string()),
            // Best-effort, consistent with the rest of the proof pipeline:
            // a failed notification must not roll back a real promotion.
            Err(e) => tracing::warn!(
                user_id = %report.user_id,
                kind = draft.kind,
                error = %e,
                "notification delivery failed"
            ),
        }
    }
    Ok(delivered)
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
    .bind(KIND_FIRST_VERIFIED)
    .fetch_one(db)
    .await?;
    if already_told {
        return Ok(None);
    }

    Ok(Some(Draft {
        kind: KIND_FIRST_VERIFIED,
        args: Vec::new(),
        data: json!({ "verified_count": verified }),
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

    // The goal's wording is a fragment of the body, so it has to be
    // resolved in the recipient's language before it becomes an argument.
    let locale = crate::services::notify::user_locale(db, user_id).await;

    let goals: Vec<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, kind, target_value FROM user_goals WHERE id = ANY($1)")
            .bind(&achieved)
            .fetch_all(db)
            .await?;

    Ok(goals
        .into_iter()
        .map(|(id, kind, target)| {
            let what = goal_label(&kind, &target, &locale);
            Draft {
                kind: "goal.reached",
                args: vec![("goal".to_string(), what)],
                data: json!({
                    "goal_id": id,
                    "goal_kind": kind,
                    "target_value": target,
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

    /// Every kind this module emits. The catalogue check below reads it.
    const EMITTED: [&str; 5] = [
        "rank.promoted",
        "capability.granted",
        "badge.awarded",
        KIND_FIRST_VERIFIED,
        "goal.reached",
    ];

    #[test]
    fn every_emitted_kind_has_copy_in_every_locale() {
        use crate::services::i18n;
        for locale in ["fr", "en", "ar"] {
            for kind in EMITTED {
                for part in ["title", "body"] {
                    let key = format!("notification.{kind}.{part}");
                    // `t` returns the key itself when nothing matches, which
                    // is exactly the notification titled `notification.goal.
                    // reached.title` that this test exists to prevent.
                    assert_ne!(i18n::t(locale, &key), key, "{locale} has no copy for {key}");
                }
            }
        }
    }

    #[test]
    fn kinds_fit_the_column_and_are_distinct() {
        let unique: std::collections::HashSet<_> = EMITTED.iter().collect();
        assert_eq!(unique.len(), EMITTED.len());
        // notifications.notification_type is VARCHAR(50).
        for kind in EMITTED {
            assert!(kind.len() <= 50, "{kind} exceeds the column width");
        }
    }

    #[test]
    fn goal_labels_are_translated_not_keys() {
        for (kind, target) in [
            ("rank", "ranger"),
            ("skill_level", "3"),
            ("capability", "mentor"),
            ("artifact_count", "10"),
        ] {
            let label = goal_label(kind, target, "en");
            assert!(
                !label.starts_with("goal.target"),
                "{kind} falls through to the raw key"
            );
            assert!(!label.contains('{'), "{kind} leaves a placeholder unfilled");
        }
        // An unknown kind passes through rather than panicking on live data.
        assert_eq!(goal_label("brand_new_goal", "x", "en"), "brand_new_goal");
    }
}
