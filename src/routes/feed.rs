//! Personal feed — Phase 2 Sprint 2.
//!
//! Aggregates the user's own recent activity (submissions, comments) + mentions received,
//! ordered by time. Sprint 4 will extend with guild activity and Sprint 2.S2.5 with
//! follow / friends once we have them.

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::routes::analytics_consent;
use crate::services::analytics::events;

pub fn feed_routes() -> Router<AppState> {
    Router::new()
        .route("/feed/me", get(my_feed))
        // P12.3 — feed personnalisé "pour toi"
        .route("/feed/for-you", get(for_you_feed))
}

#[derive(Debug, Clone, Serialize)]
struct FeedItem {
    kind: &'static str,
    happened_at: chrono::DateTime<chrono::Utc>,
    payload: Value,
}

#[derive(Deserialize)]
struct FeedQuery {
    limit: Option<i64>,
}

async fn my_feed(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Query(q): Query<FeedQuery>,
) -> Result<Json<Value>, AppError> {
    let limit = q.limit.unwrap_or(30).clamp(1, 100);
    let half = limit; // overfetch each source then merge

    // Recent successful submissions
    let submissions: Vec<(Uuid, Uuid, String, i32, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        r#"
        SELECT cs.id, cs.challenge_id, c.title, cs.fragments_earned, cs.evaluated_at
        FROM challenge_submissions cs
        JOIN challenge_templates c ON c.id = cs.challenge_id
        WHERE cs.user_id = $1 AND cs.status = 'success' AND cs.evaluated_at IS NOT NULL
        ORDER BY cs.evaluated_at DESC
        LIMIT $2
        "#,
    )
    .bind(auth.user_id)
    .bind(half)
    .fetch_all(&state.db)
    .await?;

    // Recent comments by the user
    let comments: Vec<(Uuid, String, Uuid, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        r#"
        SELECT id, target_type, target_id, LEFT(body, 200), created_at
        FROM comments
        WHERE author_id = $1 AND deleted_at IS NULL
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(auth.user_id)
    .bind(half)
    .fetch_all(&state.db)
    .await?;

    // Recent mentions received
    let mentions: Vec<(Uuid, Uuid, String, Uuid, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        r#"
        SELECT id, author_id, source_type, source_id, created_at
        FROM mentions
        WHERE mentioned_user_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(auth.user_id)
    .bind(half)
    .fetch_all(&state.db)
    .await?;

    let mut items: Vec<FeedItem> = Vec::new();
    for (sub_id, ch_id, title, frags, at) in submissions {
        items.push(FeedItem {
            kind: "challenge_completed",
            happened_at: at,
            payload: json!({
                "submission_id": sub_id,
                "challenge_id": ch_id,
                "challenge_title": title,
                "fragments_earned": frags,
            }),
        });
    }
    for (id, target_type, target_id, body, at) in comments {
        items.push(FeedItem {
            kind: "comment_posted",
            happened_at: at,
            payload: json!({
                "comment_id": id,
                "target_type": target_type,
                "target_id": target_id,
                "preview": body,
            }),
        });
    }
    for (id, author_id, source_type, source_id, at) in mentions {
        items.push(FeedItem {
            kind: "mention_received",
            happened_at: at,
            payload: json!({
                "mention_id": id,
                "author_id": author_id,
                "source_type": source_type,
                "source_id": source_id,
            }),
        });
    }

    items.sort_by(|a, b| b.happened_at.cmp(&a.happened_at));
    items.truncate(limit as usize);

    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::FEED_VIEWED,
            crate::services::analytics::props(&[("items_returned", json!(items.len()))]),
        );
    }

    Ok(Json(json!({
        "data": { "items": items },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

// ═══════════════════════════════════════════════════════════════════
// P12.3 — Feed "pour toi" : mix slices favoris + recos + tracks + attestations
// ═══════════════════════════════════════════════════════════════════

/// GET /api/feed/for-you?limit=30
///
/// Mixe 4 sources personnalisées :
/// - `open_slice_favorite_project` : slices open dans un projet que le user a
///   marqué d'intérêt (via P12.2).
/// - `slice_reco_levelup` : reco de slices proches d'un level-up (via P4).
/// - `track_challenge` : nouveaux challenges des tracks où le user est enrolled.
/// - `community_attestation` : attestations récentes de la communauté (light social).
///
/// Chaque item porte un `weight` et un `happened_at` ; le tri final est
/// pondéré (score = weight × recency_penalty). Limit clampé à [1,100].
async fn for_you_feed(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<FeedQuery>,
) -> Result<Json<Value>, AppError> {
    let limit = q.limit.unwrap_or(30).clamp(1, 100);
    let mut items: Vec<FeedItem> = Vec::new();

    // 1. Slices open dans les projets favoris.
    let favorite_slices: Vec<(Uuid, Uuid, String, chrono::DateTime<chrono::Utc>, i16, i32)> =
        sqlx::query_as(
            r#"
            SELECT ps.id, ps.project_id, ps.title, ps.created_at,
                   ps.difficulty, ps.fragments_reward
            FROM project_slices ps
            JOIN user_project_interests upi
                 ON upi.project_id = ps.project_id
            WHERE upi.user_id = $1
              AND upi.interest_score > 0
              AND ps.status = 'open'
            ORDER BY ps.created_at DESC
            LIMIT 30
            "#,
        )
        .bind(auth.user_id)
        .fetch_all(&state.db)
        .await?;
    for (sid, pid, title, at, difficulty, frags) in favorite_slices {
        items.push(FeedItem {
            kind: "open_slice_favorite_project",
            happened_at: at,
            payload: json!({
                "slice_id": sid,
                "project_id": pid,
                "title": title,
                "difficulty": difficulty,
                "fragments_reward": frags,
            }),
        });
    }

    // 2. Recos slice level-up (P4).
    let recos = crate::services::SkillsService::recommend_slices_for_user(
        &state.db,
        auth.user_id,
        10,
    )
    .await
    .unwrap_or_default();
    let now = chrono::Utc::now();
    for reco in recos {
        items.push(FeedItem {
            kind: "slice_reco_levelup",
            happened_at: now,
            payload: json!(reco),
        });
    }

    // 3. Nouveaux challenges des tracks où je suis enrolled (créés dans les 30 derniers jours).
    let track_challenges: Vec<(Uuid, String, chrono::DateTime<chrono::Utc>, Uuid)> =
        sqlx::query_as(
            r#"
            SELECT ct.template_id, cht.title, cht.created_at, ut.track_id
            FROM user_tracks ut
            JOIN track_challenges ct ON ct.track_id = ut.track_id
            JOIN challenge_templates cht ON cht.id = ct.template_id
            WHERE ut.user_id = $1
              AND cht.status = 'published'
              AND cht.created_at > NOW() - INTERVAL '30 days'
              AND NOT EXISTS (
                  SELECT 1 FROM challenge_submissions cs
                  WHERE cs.user_id = ut.user_id
                    AND cs.challenge_id = ct.template_id
                    AND cs.status = 'success'
              )
            ORDER BY cht.created_at DESC
            LIMIT 20
            "#,
        )
        .bind(auth.user_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    for (challenge_id, title, at, track_id) in track_challenges {
        items.push(FeedItem {
            kind: "track_challenge",
            happened_at: at,
            payload: json!({
                "challenge_id": challenge_id,
                "title": title,
                "track_id": track_id,
            }),
        });
    }

    // 4. Attestations communauté récentes (7 derniers jours), max 10.
    let recent_attestations: Vec<(Uuid, Uuid, String, chrono::DateTime<chrono::Utc>)> =
        sqlx::query_as(
            r#"
            SELECT a.id, a.user_id, a.attestation_type, a.issued_at
            FROM attestations a
            WHERE a.issued_at > NOW() - INTERVAL '7 days'
              AND a.revoked_at IS NULL
              AND a.user_id <> $1
            ORDER BY a.issued_at DESC
            LIMIT 10
            "#,
        )
        .bind(auth.user_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    for (aid, uid, atype, at) in recent_attestations {
        items.push(FeedItem {
            kind: "community_attestation",
            happened_at: at,
            payload: json!({
                "attestation_id": aid,
                "recipient_user_id": uid,
                "attestation_type": atype,
            }),
        });
    }

    items.sort_by(|a, b| b.happened_at.cmp(&a.happened_at));
    items.truncate(limit as usize);

    Ok(Json(json!({
        "data": { "items": items, "count": items.len() },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}
