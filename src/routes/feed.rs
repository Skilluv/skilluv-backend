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
    Router::new().route("/feed/me", get(my_feed))
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
