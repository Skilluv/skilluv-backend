//! P26 v2 SKI-108 — admin analytics on the validator population.
//!
//! Two endpoints powering sections 3 and 4 of the SKI-100 admin
//! analytics dashboard :
//!
//!   * `GET /admin/validators/stats` — per-validator activity
//!     (validations count, approve/reject ratio, time to decision,
//!     active domains).
//!   * `GET /admin/validators/collusion-matrix` — validator × claimant
//!     ratios, with a `flagged` flag when the ratio+count crosses a
//!     configurable threshold.
//!
//! Both read-only. No automated action — this is visibility, admins
//! decide.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn admin_validator_analytics_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/validators/stats", get(validator_stats))
        .route("/admin/validators/collusion-matrix", get(collusion_matrix))
}

fn wrap(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

#[derive(Debug, Deserialize)]
struct WindowQuery {
    #[serde(default)]
    window_days: Option<i32>,
    // SKI-115 (M-09) — pagination replaces the silent LIMIT 500.
    #[serde(default)]
    page: Option<i64>,
    #[serde(default)]
    per_page: Option<i64>,
}

/// SKI-108 endpoint 1 — activity per validator.
///
/// SKI-114: approve_count / reject_count now come from the exact
/// slice_validation_decisions journal (no more double-counting on
/// re-pickups). Field renamed from `reject_count_approx` to
/// `reject_count`.
///
/// SKI-115: pagination replaces the silent `LIMIT 500`. `active_domains`
/// is now an array of `{domain, granted_at}` objects so the admin roster
/// screen no longer needs an N+1 GET /users/{id}/capabilities per row.
#[utoipa::path(
    get, path = "/api/admin/validators/stats", tag = "admin",
    responses(
        (status = 200, description = "per-validator activity stats", body = serde_json::Value),
        (status = 403, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn validator_stats(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<WindowQuery>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    let window_days = q.window_days.unwrap_or(90).clamp(1, 730);
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * per_page;

    // Aggregates come from the append-only slice_validation_decisions
    // journal (SKI-114). approve_count / reject_count are exact even
    // when a slice is rejected → re-picked → validated. The avg latency
    // is computed on the journal's `picked_at` snapshot rather than the
    // slice's live column (which reject() clears).
    type StatsRow = (
        Uuid,           // user_id
        Option<String>, // username
        Option<String>, // display_name
        i64,            // approve_count
        i64,            // reject_count
        Option<f64>,    // avg_hours
    );
    let rows: Vec<StatsRow> = sqlx::query_as(
        r#"
        SELECT u.id,
               u.username,
               u.display_name,
               (SELECT COUNT(*)::bigint FROM slice_validation_decisions d
                 WHERE d.validator_id = u.id
                   AND d.decision = 'approve'
                   AND d.decided_at > NOW() - ($3 || ' days')::interval),
               (SELECT COUNT(*)::bigint FROM slice_validation_decisions d
                 WHERE d.validator_id = u.id
                   AND d.decision = 'reject'
                   AND d.decided_at > NOW() - ($3 || ' days')::interval),
               (SELECT AVG(EXTRACT(EPOCH FROM (d.decided_at - d.picked_at)) / 3600.0)
                  FROM slice_validation_decisions d
                 WHERE d.validator_id = u.id
                   AND d.picked_at IS NOT NULL
                   AND d.decided_at > NOW() - ($3 || ' days')::interval)
          FROM users u
         WHERE u.id IN (
             SELECT DISTINCT user_id
               FROM user_capabilities
              WHERE capability LIKE 'challenge_validator:%'
                AND revoked_at IS NULL
         )
         ORDER BY u.username
         LIMIT $1 OFFSET $2
        "#,
    )
    .bind(per_page)
    .bind(offset)
    .bind(window_days.to_string())
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(DISTINCT user_id)::bigint
          FROM user_capabilities
         WHERE capability LIKE 'challenge_validator:%'
           AND revoked_at IS NULL
        "#,
    )
    .fetch_one(&state.db)
    .await?;

    // Domain caps live in user_capabilities — one query batched by user.
    // SKI-115: carry granted_at per domain so the admin roster stops
    // making an N+1 GET /users/{id}/capabilities.
    let caps: Vec<(Uuid, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        r#"
        SELECT user_id, capability, granted_at
          FROM user_capabilities
         WHERE capability LIKE 'challenge_validator:%'
           AND revoked_at IS NULL
         ORDER BY granted_at ASC
        "#,
    )
    .fetch_all(&state.db)
    .await?;
    let mut by_user: std::collections::HashMap<Uuid, Vec<Value>> = std::collections::HashMap::new();
    for (uid, cap, granted_at) in caps {
        let domain = cap
            .strip_prefix("challenge_validator:")
            .unwrap_or(&cap)
            .to_string();
        by_user.entry(uid).or_default().push(json!({
            "domain": domain,
            "granted_at": granted_at.to_rfc3339(),
        }));
    }

    let items: Vec<Value> = rows
        .into_iter()
        .map(|(uid, username, display, approve, reject, avg_h)| {
            let total_decisions = approve + reject;
            let approve_ratio = if total_decisions > 0 {
                approve as f64 / total_decisions as f64
            } else {
                0.0
            };
            json!({
                "user": {
                    "id": uid,
                    "username": username,
                    "display_name": display,
                },
                "validations_count": total_decisions,
                "approve_count": approve,
                "reject_count": reject,
                "approve_ratio": approve_ratio,
                "avg_pickup_to_decision_hours": avg_h,
                "active_domains": by_user.remove(&uid).unwrap_or_default(),
            })
        })
        .collect();

    let total_pages = ((total as f64) / (per_page as f64)).ceil() as i64;

    Ok(Json(wrap(json!({
        "window_days": window_days,
        "validators": items,
        "pagination": {
            "page": page,
            "per_page": per_page,
            "total": total,
            "total_pages": total_pages,
        },
    }))))
}

#[derive(Debug, Deserialize)]
struct CollusionQuery {
    #[serde(default)]
    window_days: Option<i32>,
    #[serde(default)]
    min_count: Option<i64>,
}

/// SKI-108 endpoint 2 — validator × claimant matrix.
#[utoipa::path(
    get, path = "/api/admin/validators/collusion-matrix", tag = "admin",
    responses(
        (status = 200, description = "validator x claimant collusion matrix", body = serde_json::Value),
        (status = 403, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn collusion_matrix(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<CollusionQuery>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    let window_days = q.window_days.unwrap_or(90).clamp(1, 730);
    let min_count = q.min_count.unwrap_or(5).max(1);
    // Fixed flag threshold; keeps the API simple. Tune inside if needed.
    const FLAG_RATIO: f64 = 0.5;

    // For each validator, count validations of each claimant, then compute
    // ratio inside a window function. Filter to keep only meaningful pairs.
    type PairRow = (
        Uuid,           // validator_id
        Option<String>, // validator_username
        Uuid,           // claimant_id
        Option<String>, // claimant_username
        i64,            // count
        Option<f64>,    // ratio (COUNT / SUM OVER validator)
    );
    let rows: Vec<PairRow> = sqlx::query_as(
        r#"
        WITH pairs AS (
            SELECT s.validated_by_user_id AS validator_id,
                   s.claimed_by_user_id   AS claimant_id,
                   COUNT(*)::bigint       AS pair_count
              FROM project_slices s
             WHERE s.validated_by_user_id IS NOT NULL
               AND s.claimed_by_user_id IS NOT NULL
               AND s.validated_at > NOW() - ($1 || ' days')::interval
             GROUP BY s.validated_by_user_id, s.claimed_by_user_id
        ),
        totals AS (
            SELECT validator_id, SUM(pair_count) AS total FROM pairs GROUP BY validator_id
        )
        SELECT p.validator_id,
               vu.username,
               p.claimant_id,
               cu.username,
               p.pair_count,
               (p.pair_count::float8 / NULLIF(t.total, 0)::float8)
          FROM pairs p
          JOIN totals t ON t.validator_id = p.validator_id
          LEFT JOIN users vu ON vu.id = p.validator_id
          LEFT JOIN users cu ON cu.id = p.claimant_id
         WHERE p.pair_count >= $2
         ORDER BY p.pair_count DESC
         LIMIT 500
        "#,
    )
    .bind(window_days.to_string())
    .bind(min_count)
    .fetch_all(&state.db)
    .await?;

    // Group by validator so the front sees "Alice validated 8 of Bob,
    // 3 of Charlie" as a single record rather than 2 disconnected rows.
    let mut by_validator: std::collections::BTreeMap<Uuid, (Option<String>, Vec<Value>)> =
        std::collections::BTreeMap::new();
    for (vid, vname, cid, cname, count, ratio) in rows {
        let flagged = ratio.unwrap_or(0.0) > FLAG_RATIO && count > min_count;
        by_validator
            .entry(vid)
            .or_insert_with(|| (vname.clone(), Vec::new()))
            .1
            .push(json!({
                "claimant_id": cid,
                "claimant_username": cname,
                "count": count,
                "ratio": ratio,
                "flagged": flagged,
            }));
    }

    let items: Vec<Value> = by_validator
        .into_iter()
        .map(|(vid, (username, targets))| {
            json!({
                "validator": {
                    "id": vid,
                    "username": username,
                },
                "top_targets": targets,
            })
        })
        .collect();

    Ok(Json(wrap(json!({
        "window_days": window_days,
        "min_count": min_count,
        "flag_ratio_threshold": FLAG_RATIO,
        "note": "Phase 1 dogfooding: high ratios expected while Jérémie uses multiple accounts.",
        "matrix": items,
    }))))
}
