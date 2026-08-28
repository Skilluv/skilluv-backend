//! Paid missions, seen from outside the two parties.
//!
//! ## Why this is not `/admin/design-missions`
//!
//! Migration 0192 built missions, applications and billing for every domain,
//! keyed by `mission_types.skill_domain`. Design needed rows, not a mechanism,
//! and got twelve of them. A design mission is a mission with
//! `skill_domain = 'design'`, so a design admin surface is this one with a
//! filter — and the same surface serves security, code and the four others
//! without a second implementation to keep in step.
//!
//! ## What an admin is for here
//!
//! Not running missions. A mission belongs to the enterprise that posted it
//! and the person who took it, and both already have every action they need.
//! What neither of them has is a way out of the case where they disagree and
//! neither will move: the mission sits `in_progress` for ever and the money
//! sits in escrow.
//!
//! That is the whole of the write surface: one decision, taken by somebody
//! outside, recorded as having been decided rather than agreed.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn admin_mission_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/missions", get(list))
        .route("/admin/missions/{slug}", get(detail))
        .route("/admin/missions/{slug}/arbitrate", post(arbitrate))
        .route("/admin/missions/{slug}/status", post(take_down))
}

/// An admin, the curator of the mission's domain, or an arbiter.
///
/// A design curator reads design missions and not security ones, which is the
/// point of the scope.
///
/// An arbiter reads any of them, because deciding a case means opening it
/// first — and because the alternative is an arbiter who can end a mission
/// and then cannot see what they did.
async fn require_reader(state: &AppState, auth: &AuthUser, domain: &str) -> Result<(), AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        &[
            "admin",
            &format!("domain_curator:{domain}"),
            "domain_curator:all",
            "mission_arbiter",
        ],
    )
    .await
}

/// Somebody allowed to decide a mission neither side will end.
///
/// Not scoped by domain, deliberately: the question an arbiter answers is
/// whether a contract was honoured, and that is the same question about a
/// logotype and about a pull request. Scoping it would leave a stuck mission
/// nobody may unstick because its domain has no arbiter yet.
async fn require_arbiter(state: &AppState, auth: &AuthUser) -> Result<(), AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        &["admin", "mission_arbiter"],
    )
    .await
}

// ═══════════════════════════════════════════════════════════════════
// The list
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    /// One of the seven skill domains. Required for a curator, who may only
    /// read their own; optional for an admin.
    #[param(max_length = 30)]
    pub skill_domain: Option<String>,
    /// A `mission_types` slug — `brand_identity_design`, `website_design`…
    #[param(max_length = 60)]
    pub mission_type: Option<String>,
    #[param(max_length = 30)]
    pub status: Option<String>,
    /// Only missions where the two sides have stopped moving. The queue an
    /// arbiter actually works.
    #[serde(default)]
    pub stuck_only: bool,
    /// How long without a decision counts as stuck. Twenty-one days by
    /// default: long enough that a fortnight's holiday is not a dispute.
    #[serde(default = "default_stuck_days")]
    #[param(minimum = 1, maximum = 365)]
    pub stuck_after_days: i32,
    #[serde(default = "default_page")]
    #[param(minimum = 1)]
    pub page: i64,
    #[serde(default = "default_per_page")]
    #[param(minimum = 1, maximum = 200)]
    pub per_page: i64,
}

fn default_stuck_days() -> i32 {
    21
}
fn default_page() -> i64 {
    1
}
fn default_per_page() -> i64 {
    50
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct AdminMissionRow {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub skill_domain: String,
    pub mission_type_slug: String,
    pub status: String,
    pub enterprise_name: String,
    pub assigned_username: Option<String>,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The last hand-in, whether or not anybody answered it.
    pub last_delivered_at: Option<chrono::DateTime<chrono::Utc>>,
    /// How many rounds have been handed in.
    pub rounds: i64,
    /// True when a round is waiting and has been waiting too long. This is
    /// what "stuck" means: not a slow mission, an unanswered one.
    pub awaiting_decision: bool,
    /// Already decided by somebody outside. Shown so an arbiter does not open
    /// a case that has one.
    pub arbitrated: bool,
}

/// `GET /admin/missions`, in the shape every other admin listing answers.
///
/// `{data, pagination, meta}` rather than `ApiResponse<Vec<_>>`: SKI-58 settled
/// that convention and `tests/test_admin_listing_convention.rs` holds it. This
/// endpoint was the exception, and the cost was concrete — with no `total`, the
/// admin pager could only ever offer one more page when the current one came
/// back full, which is a guess about whether there is anything on it.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminMissionListResponse {
    pub data: Vec<AdminMissionRow>,
    pub pagination: crate::api_response::Pagination,
    pub meta: crate::api_response::MetaInfo,
}

/// Every mission, narrowed.
#[utoipa::path(
    get,
    path = "/api/admin/missions",
    operation_id = "adminMissionsList",
    tag = "admin",
    params(ListQuery),
    responses(
        (status = 200, body = AdminMissionListResponse),
        (status = 400, description = "Unknown domain or filter", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an admin or a curator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<AdminMissionListResponse>, AppError> {
    if let Some(domain) = &q.skill_domain {
        crate::validators::validate_skill_domain(domain, "skill_domain")?;
    }
    if q.page < 1 {
        return Err(AppError::Validation("page must be at least 1".into()));
    }
    if !(1..=200).contains(&q.per_page) {
        return Err(AppError::Validation(
            "per_page must be between 1 and 200".into(),
        ));
    }
    if !(1..=365).contains(&q.stuck_after_days) {
        return Err(AppError::Validation(
            "stuck_after_days must be between 1 and 365".into(),
        ));
    }

    // An admin may read every domain; a curator only their own, and they have
    // to name it. Answering "all of them" to a curator who left the filter
    // blank would hand them the domains they were not given.
    //
    // An arbiter reads the stuck queue across every domain and nothing else:
    // that queue *is* their job, and the rest of the mission board is not.
    if q.stuck_only {
        crate::middleware::capabilities::require_any_capability(
            &state.db,
            auth.user_id,
            &["admin", "mission_arbiter", "domain_curator:all"],
        )
        .await?;
    } else {
        match &q.skill_domain {
            Some(domain) => require_reader(&state, &auth, domain).await?,
            None => {
                crate::routes::admin::require_admin(&state, &auth).await?;
            }
        }
    }

    let rows = sqlx::query_as::<_, AdminMissionRow>(
        r#"
        SELECT m.id,
               m.slug,
               m.title,
               m.skill_domain,
               mt.slug AS mission_type_slug,
               m.status,
               e.company_name AS enterprise_name,
               u.username AS assigned_username,
               m.published_at,
               d.last_delivered_at,
               COALESCE(d.rounds, 0) AS rounds,
               COALESCE(d.waiting_since < NOW() - ($4::INTEGER * INTERVAL '1 day'), FALSE)
                   AS awaiting_decision,
               a.id IS NOT NULL AS arbitrated

          FROM missions m
          JOIN mission_types mt ON mt.id = m.mission_type_id
          JOIN enterprises e ON e.id = m.enterprise_id
          LEFT JOIN users u ON u.id = m.assigned_user_id
          LEFT JOIN mission_arbitrations a ON a.mission_id = m.id

          LEFT JOIN LATERAL (
              SELECT count(*) AS rounds,
                     max(md.delivered_at) AS last_delivered_at,
                     -- The oldest hand-in nobody has answered. One unanswered
                     -- round is what a dispute looks like from outside.
                     min(md.delivered_at) FILTER (WHERE md.decision IS NULL)
                         AS waiting_since
                FROM mission_deliveries md
               WHERE md.mission_id = m.id
          ) AS d ON TRUE

         WHERE ($1::TEXT IS NULL OR m.skill_domain = $1)
           AND ($2::TEXT IS NULL OR mt.slug = $2)
           AND ($3::TEXT IS NULL OR m.status = $3)
           AND (NOT $5::BOOLEAN
                OR (d.waiting_since IS NOT NULL
                    AND d.waiting_since < NOW() - ($4::INTEGER * INTERVAL '1 day')
                    AND a.id IS NULL))

         ORDER BY d.waiting_since ASC NULLS LAST, m.created_at DESC
         LIMIT $6 OFFSET $7
        "#,
    )
    .bind(q.skill_domain.as_deref())
    .bind(q.mission_type.as_deref())
    .bind(q.status.as_deref())
    .bind(q.stuck_after_days)
    .bind(q.stuck_only)
    .bind(q.per_page)
    .bind((q.page - 1) * q.per_page)
    .fetch_all(&state.db)
    .await?;

    // The same WHERE, without the window. Written out rather than derived from
    // the query above because the two differ only in what they select, and a
    // count that quietly drifts from the list it describes is worse than no
    // count: the pager would offer pages that are empty.
    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
          FROM missions m
          JOIN mission_types mt ON mt.id = m.mission_type_id
          LEFT JOIN mission_arbitrations a ON a.mission_id = m.id
          LEFT JOIN LATERAL (
              SELECT min(md.delivered_at) FILTER (WHERE md.decision IS NULL)
                         AS waiting_since
                FROM mission_deliveries md
               WHERE md.mission_id = m.id
          ) AS d ON TRUE
         WHERE ($1::TEXT IS NULL OR m.skill_domain = $1)
           AND ($2::TEXT IS NULL OR mt.slug = $2)
           AND ($3::TEXT IS NULL OR m.status = $3)
           AND (NOT $5::BOOLEAN
                OR (d.waiting_since IS NOT NULL
                    AND d.waiting_since < NOW() - ($4::INTEGER * INTERVAL '1 day')
                    AND a.id IS NULL))
        "#,
    )
    .bind(q.skill_domain.as_deref())
    .bind(q.mission_type.as_deref())
    .bind(q.status.as_deref())
    .bind(q.stuck_after_days)
    .bind(q.stuck_only)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(AdminMissionListResponse {
        data: rows,
        pagination: crate::api_response::Pagination {
            page: q.page,
            per_page: q.per_page,
            total,
            total_pages: Some((total + q.per_page - 1) / q.per_page),
        },
        meta: crate::api_response::MetaInfo::now(),
    }))
}

// ═══════════════════════════════════════════════════════════════════
// One mission, with its trail
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminMissionDetail {
    pub mission: AdminMissionRow,
    /// What the mission says about who owns the work. Shown because it is
    /// what an arbitration turns on, and because nobody reads a contract they
    /// have to go and find.
    pub ip_terms: String,
    pub nda_required: bool,
    /// Every round, in order, with what was decided and why.
    #[schema(value_type = Vec<Object>)]
    pub rounds: Vec<serde_json::Value>,
    /// The invoices raised against it, so the money is visible next to the
    /// work rather than on another page.
    #[schema(value_type = Vec<Object>)]
    pub invoices: Vec<serde_json::Value>,
    /// The arbitration, where there has been one.
    #[schema(value_type = Option<Object>)]
    pub arbitration: Option<serde_json::Value>,
}

/// One mission, and everything that happened to it.
#[utoipa::path(
    get,
    path = "/api/admin/missions/{slug}",
    operation_id = "adminMissionsDetail",
    tag = "admin",
    params(("slug" = String, Path, description = "Mission slug")),
    responses(
        (status = 200, body = ApiResponse<AdminMissionDetail>),
        (status = 403, description = "Not an admin or a curator of this domain", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such mission", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn detail(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<AdminMissionDetail>>, AppError> {
    // The domain is read before the permission is checked, because the
    // permission depends on it. A mission nobody may read still answers 404
    // rather than 403: which missions exist is not a curator's business.
    let head: Option<(Uuid, String, String, bool)> = sqlx::query_as(
        "SELECT id, skill_domain, ip_terms, nda_required FROM missions WHERE slug = $1",
    )
    .bind(&slug)
    .fetch_optional(&state.db)
    .await?;

    let Some((mission_id, domain, ip_terms, nda_required)) = head else {
        return Err(AppError::NotFound("no such mission".into()));
    };
    require_reader(&state, &auth, &domain).await?;

    load_detail(&state, mission_id, ip_terms, nda_required).await
}

/// Everything about one mission, with the permission already settled.
///
/// Split out so an arbiter can be handed the result of their own decision.
/// Calling the handler again would re-check a permission that has just been
/// established, and refuse the arbiter their own outcome — which is exactly
/// what it did.
async fn load_detail(
    state: &AppState,
    mission_id: Uuid,
    ip_terms: String,
    nda_required: bool,
) -> Result<Json<ApiResponse<AdminMissionDetail>>, AppError> {
    let mission = sqlx::query_as::<_, AdminMissionRow>(
        r#"
        SELECT m.id, m.slug, m.title, m.skill_domain, mt.slug AS mission_type_slug,
               m.status, e.company_name AS enterprise_name, u.username AS assigned_username,
               m.published_at,
               (SELECT max(delivered_at) FROM mission_deliveries WHERE mission_id = m.id)
                   AS last_delivered_at,
               (SELECT count(*) FROM mission_deliveries WHERE mission_id = m.id) AS rounds,
               EXISTS (SELECT 1 FROM mission_deliveries
                        WHERE mission_id = m.id AND decision IS NULL) AS awaiting_decision,
               EXISTS (SELECT 1 FROM mission_arbitrations WHERE mission_id = m.id) AS arbitrated
          FROM missions m
          JOIN mission_types mt ON mt.id = m.mission_type_id
          JOIN enterprises e ON e.id = m.enterprise_id
          LEFT JOIN users u ON u.id = m.assigned_user_id
         WHERE m.id = $1
        "#,
    )
    .bind(mission_id)
    .fetch_one(&state.db)
    .await?;

    let rounds: Vec<serde_json::Value> = sqlx::query_scalar(
        r#"
        SELECT jsonb_build_object(
                   'round', md.round,
                   'delivered_by', u.username,
                   'artifact_url', md.artifact_url,
                   'notes_md', md.notes_md,
                   'delivered_at', md.delivered_at,
                   'decision', md.decision,
                   'decision_reason', md.decision_reason,
                   'decided_at', md.decided_at,
                   'beyond_agreed_rounds', md.beyond_agreed_rounds
               )
          FROM mission_deliveries md
          LEFT JOIN users u ON u.id = md.delivered_by
         WHERE md.mission_id = $1
         ORDER BY md.round ASC
        "#,
    )
    .bind(mission_id)
    .fetch_all(&state.db)
    .await?;

    let invoices: Vec<serde_json::Value> = sqlx::query_scalar(
        r#"
        SELECT jsonb_build_object(
                   'id', mi.id,
                   'label', mi.label,
                   'amount', mi.amount,
                   'currency', mi.currency,
                   'status', mi.status,
                   'captured_at', mi.captured_at,
                   'released_at', mi.released_at,
                   'issued_at', mi.issued_at
               )
          FROM mission_invoices mi
         WHERE mi.mission_id = $1
         ORDER BY mi.sequence ASC
        "#,
    )
    .bind(mission_id)
    .fetch_all(&state.db)
    .await?;

    let arbitration: Option<serde_json::Value> = sqlx::query_scalar(
        r#"
        SELECT jsonb_build_object(
                   'outcome', a.outcome,
                   'reason_md', a.reason_md,
                   'arbiter', u.username,
                   'decided_at', a.decided_at
               )
          FROM mission_arbitrations a
          LEFT JOIN users u ON u.id = a.arbiter_id
         WHERE a.mission_id = $1
        "#,
    )
    .bind(mission_id)
    .fetch_optional(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(AdminMissionDetail {
        mission,
        ip_terms,
        nda_required,
        rounds,
        invoices,
        arbitration,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// The decision
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArbitrateBody {
    /// `accepted` — the delivery stands and the money is released.
    /// `cancelled` — the mission ends and the escrow goes back.
    #[schema(max_length = 20)]
    pub outcome: String,
    /// Read by both sides, one of whom has just lost. Eighty characters
    /// minimum, because "refusé" teaches nobody anything and cannot be
    /// argued with.
    #[schema(min_length = 80, max_length = 8000)]
    pub reason_md: String,
}

/// Decide a mission neither side will end.
///
/// Both outcomes already exist in the mission's own vocabulary — this endpoint
/// does not invent a third. What it adds is the record that the outcome was
/// decided rather than agreed, and by whom: a mission accepted by arbitration
/// and one accepted by a happy client look identical in `missions`, and they
/// must not read the same to anybody who later asks what happened.
///
/// Once. A second arbitration would re-open a decision that has already moved
/// money, and re-opening it is a new mission rather than a new row.
#[utoipa::path(
    post,
    path = "/api/admin/missions/{slug}/arbitrate",
    tag = "admin",
    params(("slug" = String, Path, description = "Mission slug")),
    request_body = ArbitrateBody,
    responses(
        (status = 200, description = "Decided", body = ApiResponse<AdminMissionDetail>),
        (status = 400, description = "Unknown outcome or a reason too short", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an arbiter", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such mission", body = crate::api_response::ErrorResponse),
        (status = 409, description = "Already arbitrated, or not in a state to be", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn arbitrate(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<ArbitrateBody>,
) -> Result<Json<ApiResponse<AdminMissionDetail>>, AppError> {
    require_arbiter(&state, &auth).await?;

    if !matches!(body.outcome.as_str(), "accepted" | "cancelled") {
        return Err(AppError::Validation(
            "outcome must be accepted or cancelled".into(),
        ));
    }
    let reason = body.reason_md.trim();
    if reason.chars().count() < 80 {
        return Err(AppError::Validation(
            "say why in at least eighty characters — both sides read this, and one of them \
             has just lost"
                .into(),
        ));
    }
    if reason.chars().count() > 8000 {
        return Err(AppError::Validation("that reason is too long".into()));
    }

    let mission: Option<(Uuid, String, String, bool)> =
        sqlx::query_as("SELECT id, status, ip_terms, nda_required FROM missions WHERE slug = $1")
            .bind(&slug)
            .fetch_optional(&state.db)
            .await?;
    let Some((mission_id, status, ip_terms, nda_required)) = mission else {
        return Err(AppError::NotFound("no such mission".into()));
    };

    // Only a mission with work in it can be arbitrated, and the check is up
    // front because the decision now moves money: discovering three statements
    // later that a published mission has no delivery to decide would leave an
    // arbitration written against a mission that never had one.
    //
    // `delivered` counts. A client who will not close is the commonest reason
    // this endpoint is called at all.
    if !matches!(status.as_str(), "in_progress" | "delivered") {
        return Err(AppError::Conflict(format!(
            "a {status} mission has no delivery to arbitrate — this decides a round \
             neither side will settle, and there is none"
        )));
    }

    let mut tx = state.db.begin().await?;

    // The round in dispute, where there is one. A designer who vanished
    // leaves none, and the mission is arbitrated without it.
    let delivery_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM mission_deliveries
          WHERE mission_id = $1 AND decision IS NULL
          ORDER BY round DESC LIMIT 1",
    )
    .bind(mission_id)
    .fetch_optional(&mut *tx)
    .await?;

    // The unique index refuses a second arbitration; the conflict is turned
    // into a 409 rather than a 500 because it is a thing a caller did, not a
    // thing that went wrong.
    let written = sqlx::query(
        "INSERT INTO mission_arbitrations
             (mission_id, delivery_id, arbiter_id, outcome, reason_md)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (mission_id) DO NOTHING",
    )
    .bind(mission_id)
    .bind(delivery_id)
    .bind(auth.user_id)
    .bind(&body.outcome)
    .bind(reason)
    .execute(&mut *tx)
    .await?;

    if written.rows_affected() == 0 {
        // Already arbitrated — but the mission is still open, which the state
        // check above just established. So the decision was written and the
        // settlement behind it did not finish: the provider refund failed, the
        // process died between the two, something.
        //
        // Answering 409 here would be the worst of both. The caller is told
        // the work is done, the money is still where it was, and there is no
        // second call that can finish it because every one of them 409s. So a
        // repeat finishes the settlement instead, provided it asks for the
        // same outcome — the ledger's idempotency keys make the money part
        // safe to repeat, and `set_status` returns early when the status is
        // already right.
        let decided: Option<String> =
            sqlx::query_scalar("SELECT outcome FROM mission_arbitrations WHERE mission_id = $1")
                .bind(mission_id)
                .fetch_one(&mut *tx)
                .await?;

        if decided.as_deref() != Some(body.outcome.as_str()) {
            return Err(AppError::Conflict(format!(
                "this mission was already arbitrated as {}, and a decision is not re-taken \
                 — reopening it is a new mission",
                decided.unwrap_or_else(|| "unknown".into())
            )));
        }
    }

    // The waiting round is answered, so the loop cannot be resumed behind the
    // decision. The reason is the arbiter's, in full: the round's own trail
    // has to say what happened without a second lookup.
    if let Some(delivery_id) = delivery_id {
        sqlx::query(
            "UPDATE mission_deliveries
                SET decision = $2, decision_reason = $3, decided_by = $4, decided_at = NOW()
              WHERE id = $1 AND decision IS NULL",
        )
        .bind(delivery_id)
        .bind(if body.outcome == "accepted" {
            "accepted"
        } else {
            "changes_requested"
        })
        .bind(format!("Arbitrage : {reason}"))
        .bind(auth.user_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    // The status moves through `set_status`, not through an UPDATE here.
    //
    // This endpoint used to write the status itself, and the money never
    // moved: releasing the escrow and returning it both live in `set_status`,
    // so a raw UPDATE set the word and skipped the transfer. The doc-comment
    // above says "the money is released" and "the escrow goes back", and
    // neither happened — an arbitration that reads as settled with the funds
    // still sitting where they were.
    //
    // `accepted` goes all the way to `closed` rather than stopping at
    // `delivered`. Closing is the client accepting delivery, and arbitration
    // exists precisely because the client will not: leaving the mission at
    // `delivered` leaves it waiting on the one act that was refused, with the
    // money waiting behind it. The arbiter has decided the delivery stands, so
    // it stands.
    let statuses: &[&str] = if body.outcome == "accepted" {
        // Through `delivered`, because that is the transition the workflow
        // allows and it is what stamps `delivered_at`.
        &["delivered", "closed"]
    } else {
        &["cancelled"]
    };

    for status in statuses {
        crate::services::missions::set_status_as(
            &state.db,
            mission_id,
            status,
            Some(reason),
            crate::services::missions::Decider::Arbiter,
        )
        .await?;
    }

    load_detail(&state, mission_id, ip_terms, nda_required).await
}

// ═══════════════════════════════════════════════════════════════════
// Taking one down
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TakeDownBody {
    /// Only `cancelled`. The field exists rather than being implied by the
    /// path so that the request says what it does, and so that the day a
    /// second platform-level status exists this endpoint does not have to
    /// change shape.
    pub status: String,
    /// Why, read by the enterprise and by whoever was working on it. Twenty
    /// characters minimum: "spam" is a verdict, not a reason, and the two
    /// people who receive it have no other account of what happened.
    #[schema(min_length = 20, max_length = 4000)]
    pub reason: String,
}

/// Take a mission off the board.
///
/// ## Why an admin needs this at all
///
/// Nothing reviews a mission before it is published. That is deliberate — the
/// control is the KYC an enterprise clears before it can post anything, which
/// checks *who* may publish once instead of *what* is published every time,
/// and a per-mission gate would put a person between a paying client and their
/// advert with nobody to staff the queue.
///
/// The price of not reviewing beforehand is being able to act afterwards, and
/// until now nobody could: `POST /api/missions/{slug}/status` is behind
/// `require_enterprise` plus an ownership check, so a mission that breached the
/// terms could only be removed by the account that posted it.
///
/// ## Why not `mission_arbiter`
///
/// An arbitration decides a case between two parties who both still want
/// something. This decides that the platform will not carry the listing, which
/// is not a judgement between them — it is a judgement about them, and it is
/// answered for by whoever answers for the platform.
///
/// ## Why a delivered mission is refused
///
/// Cancelling returns the escrow. Doing that to a mission whose work has been
/// handed in takes money back from somebody who did the work, on one person's
/// say-so and with no record that the delivery was ever weighed. That case has
/// an endpoint, and it is `arbitrate`: it writes down the decision, answers
/// the open round, and lets the outcome be argued with afterwards.
#[utoipa::path(
    post,
    path = "/api/admin/missions/{slug}/status",
    operation_id = "adminMissionsTakeDown",
    tag = "admin",
    params(("slug" = String, Path, description = "Mission slug")),
    request_body = TakeDownBody,
    responses(
        (status = 200, description = "Taken down", body = ApiResponse<AdminMissionDetail>),
        (status = 400, description = "Not `cancelled`, or a reason too short", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such mission", body = crate::api_response::ErrorResponse),
        (status = 409, description = "Already ended, or delivered and so an arbitration", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn take_down(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: axum::http::HeaderMap,
    Path(slug): Path<String>,
    Json(body): Json<TakeDownBody>,
) -> Result<Json<ApiResponse<AdminMissionDetail>>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    if body.status != "cancelled" {
        return Err(AppError::Validation(
            "the only status an administrator sets on somebody else's mission is \
             cancelled. Moving it forward is the two parties' to do"
                .into(),
        ));
    }
    let reason = body.reason.trim();
    if reason.chars().count() < 20 {
        return Err(AppError::Validation(
            "say why in at least twenty characters — the enterprise and whoever \
             was working on it both read this, and it is the only account they get"
                .into(),
        ));
    }
    crate::validators::check_max_len(reason, "reason", 4000)?;

    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        status: String,
        ip_terms: String,
        nda_required: bool,
        enterprise_id: Uuid,
        assigned_user_id: Option<Uuid>,
        title: String,
    }

    let mission: Option<Row> = sqlx::query_as(
        "SELECT id, status, ip_terms, nda_required, enterprise_id,
                assigned_user_id, title
           FROM missions WHERE slug = $1",
    )
    .bind(&slug)
    .fetch_optional(&state.db)
    .await?;
    let Some(m) = mission else {
        return Err(AppError::NotFound("no such mission".into()));
    };
    let (mission_id, status, assigned, title) =
        (m.id, m.status.clone(), m.assigned_user_id, m.title.clone());

    if status == "cancelled" || status == "closed" {
        return Err(AppError::Conflict(format!(
            "this mission is already {status}"
        )));
    }
    // The transition table refuses `delivered -> cancelled` for everybody but
    // an arbiter, and would say so. It is caught here to say *why*, and to
    // name the endpoint that does handle it — the alternative is an
    // administrator reading "a delivered mission cannot become cancelled" and
    // concluding nothing can be done about it.
    if status == "delivered" {
        return Err(AppError::Conflict(
            "the work on this one has been handed in. Cancelling it would take the \
             escrow back from whoever did it, which is a decision between the two \
             parties — use the arbitration endpoint, which records it as one"
                .into(),
        ));
    }

    // Through `set_status`, which is what returns the escrow. An UPDATE here
    // would set the word and leave the money where it was; the arbitration
    // endpoint above carries the same note and the same scar.
    crate::services::missions::set_status(&state.db, mission_id, "cancelled", Some(reason)).await?;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "mission.taken_down",
            target_type: Some("mission"),
            target_id: Some(mission_id),
            metadata: Some(serde_json::json!({
                "slug": slug,
                "from_status": status,
                "reason": reason,
            })),
            headers: Some(&headers),
        },
    )
    .await;

    // Both sides, same wording. A takedown explained one way to the client and
    // another way to the contractor is two decisions.
    let owner: Option<Uuid> = sqlx::query_scalar("SELECT owner_id FROM enterprises WHERE id = $1")
        .bind(m.enterprise_id)
        .fetch_optional(&state.db)
        .await?;
    for recipient in [owner, assigned].into_iter().flatten() {
        let _ = crate::services::notify::send(
            &state,
            crate::services::notify::Recipient::User(recipient),
            "mission.taken_down",
        )
        .arg("mission", title.clone())
        .arg("reason", reason.to_string())
        .payload(serde_json::json!({
            "mission_id": mission_id,
            "mission_slug": slug,
        }))
        .execute()
        .await;
    }

    metrics::counter!("skilluv_mission_takedowns_total").increment(1);

    load_detail(&state, mission_id, m.ip_terms, m.nda_required).await
}
