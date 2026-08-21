//! Mentorship — Phase 5.11.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;

/// Part plateforme, en centièmes (20% = 2000).
const PLATFORM_FEE_BPS: i64 = 2000;

pub fn mentorship_routes() -> Router<AppState> {
    Router::new()
        .route("/mentors", get(list_mentors))
        .route("/mentors/{user_id}", get(get_mentor_profile))
        .route(
            "/mentors/me",
            put(upsert_my_mentor_profile).get(get_my_mentor_profile),
        )
        .route("/mentors/me/availability", post(add_availability))
        .route(
            "/mentors/me/connect/onboard",
            post(start_connect_onboarding),
        )
        .route("/mentors/me/connect/status", get(connect_status))
        .route(
            "/mentorship/sessions",
            post(book_session).get(list_my_sessions),
        )
        .route("/mentorship/sessions/{id}/cancel", post(cancel_session))
        .route("/mentorship/sessions/{id}/complete", post(mark_completed))
        // The student's side of the same event. The mentor says "done", the
        // student says "yes, it happened" — and that second word is what
        // pays them without waiting out the week.
        .route("/mentorship/sessions/{id}/confirm", post(confirm_session))
        .route("/mentorship/sessions/{id}/review", post(submit_review))
}

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

// ─── Profils mentors ────────────────────────────────────────────

#[derive(Deserialize)]
struct MentorListQuery {
    expertise: Option<String>,
    language: Option<String>,
    max_rate_cents: Option<i64>,
    page: Option<i64>,
    per_page: Option<i64>,
}

/// List available mentors.
#[utoipa::path(
    get, path = "/api/mentors", tag = "challenges",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_mentors(
    State(state): State<AppState>,
    Query(q): Query<MentorListQuery>,
) -> Result<Json<Value>, AppError> {
    let per_page = q.per_page.unwrap_or(20).clamp(1, 50);
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * per_page;
    let rows = sqlx::query(
        r#"
        SELECT m.user_id, m.headline, m.expertise_areas, m.languages_spoken,
               m.hourly_rate_eur_cents, m.avg_rating, m.total_sessions,
               u.username, u.display_name, u.country_iso2
        FROM mentor_profiles m
        JOIN users u ON u.id = m.user_id
        WHERE m.active = TRUE
          AND ($1::TEXT IS NULL OR $1 = ANY(m.expertise_areas))
          AND ($2::TEXT IS NULL OR $2 = ANY(m.languages_spoken))
          AND ($3::BIGINT IS NULL OR m.hourly_rate_eur_cents <= $3)
        ORDER BY m.avg_rating DESC NULLS LAST, m.total_sessions DESC
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(&q.expertise)
    .bind(&q.language)
    .bind(q.max_rate_cents)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "user_id": r.get::<Uuid, _>("user_id"),
                "username": r.get::<String, _>("username"),
                "display_name": r.get::<String, _>("display_name"),
                "country_iso2": r.get::<Option<String>, _>("country_iso2"),
                "headline": r.get::<String, _>("headline"),
                "expertise_areas": r.get::<Vec<String>, _>("expertise_areas"),
                "languages_spoken": r.get::<Vec<String>, _>("languages_spoken"),
                "hourly_rate_eur_cents": r.get::<i64, _>("hourly_rate_eur_cents"),
                "avg_rating": r.get::<Option<BigDecimal>, _>("avg_rating").map(|d| d.to_string()),
                "total_sessions": r.get::<i32, _>("total_sessions"),
            })
        })
        .collect();
    Ok(Json(build_response(json!({ "mentors": items }))))
}

/// Get a mentor's public profile.
#[utoipa::path(
    get, path = "/api/mentors/{user_id}", tag = "challenges",
    params(("user_id" = uuid::Uuid, Path)),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn get_mentor_profile(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT m.*, u.username, u.display_name, u.country_iso2, u.skill_domain
        FROM mentor_profiles m
        JOIN users u ON u.id = m.user_id
        WHERE m.user_id = $1 AND m.active = TRUE
        "#,
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("mentor not found".into()))?;
    Ok(Json(build_response(json!({
        "user_id": row.get::<Uuid, _>("user_id"),
        "username": row.get::<String, _>("username"),
        "display_name": row.get::<String, _>("display_name"),
        "country_iso2": row.get::<Option<String>, _>("country_iso2"),
        "skill_domain": row.get::<String, _>("skill_domain"),
        "headline": row.get::<String, _>("headline"),
        "bio": row.get::<String, _>("bio"),
        "expertise_areas": row.get::<Vec<String>, _>("expertise_areas"),
        "languages_spoken": row.get::<Vec<String>, _>("languages_spoken"),
        "hourly_rate_eur_cents": row.get::<i64, _>("hourly_rate_eur_cents"),
        "min_session_minutes": row.get::<i32, _>("min_session_minutes"),
        "avg_rating": row.get::<Option<BigDecimal>, _>("avg_rating").map(|d| d.to_string()),
        "total_sessions": row.get::<i32, _>("total_sessions"),
    }))))
}

#[derive(Deserialize)]
struct UpsertMentorBody {
    headline: String,
    bio: String,
    expertise_areas: Vec<String>,
    languages_spoken: Vec<String>,
    hourly_rate_eur_cents: i64,
    min_session_minutes: Option<i32>,
    active: Option<bool>,
}

/// Create or update my mentor profile.
#[utoipa::path(
    put, path = "/api/mentors/me", tag = "challenges",
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn upsert_my_mentor_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpsertMentorBody>,
) -> Result<Json<Value>, AppError> {
    if body.hourly_rate_eur_cents < 0 || body.hourly_rate_eur_cents > 10_000_000 {
        return Err(AppError::Validation("hourly_rate out of range".into()));
    }
    sqlx::query(
        r#"
        INSERT INTO mentor_profiles
            (user_id, headline, bio, expertise_areas, languages_spoken,
             hourly_rate_eur_cents, min_session_minutes, active)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (user_id) DO UPDATE SET
            headline = EXCLUDED.headline,
            bio = EXCLUDED.bio,
            expertise_areas = EXCLUDED.expertise_areas,
            languages_spoken = EXCLUDED.languages_spoken,
            hourly_rate_eur_cents = EXCLUDED.hourly_rate_eur_cents,
            min_session_minutes = EXCLUDED.min_session_minutes,
            active = EXCLUDED.active,
            updated_at = NOW()
        "#,
    )
    .bind(auth.user_id)
    .bind(&body.headline)
    .bind(&body.bio)
    .bind(&body.expertise_areas)
    .bind(&body.languages_spoken)
    .bind(body.hourly_rate_eur_cents)
    .bind(body.min_session_minutes.unwrap_or(30))
    .bind(body.active.unwrap_or(true))
    .execute(&state.db)
    .await?;
    Ok(Json(build_response(json!({ "updated": true }))))
}

/// Get my mentor profile.
#[utoipa::path(
    get, path = "/api/mentors/me", tag = "challenges",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn get_my_mentor_profile(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let row = sqlx::query("SELECT * FROM mentor_profiles WHERE user_id = $1")
        .bind(auth.user_id)
        .fetch_optional(&state.db)
        .await?;
    let Some(r) = row else {
        return Ok(Json(build_response(json!({ "profile": null }))));
    };
    Ok(Json(build_response(json!({
        "profile": {
            "headline": r.get::<String, _>("headline"),
            "bio": r.get::<String, _>("bio"),
            "expertise_areas": r.get::<Vec<String>, _>("expertise_areas"),
            "languages_spoken": r.get::<Vec<String>, _>("languages_spoken"),
            "hourly_rate_eur_cents": r.get::<i64, _>("hourly_rate_eur_cents"),
            "min_session_minutes": r.get::<i32, _>("min_session_minutes"),
            "active": r.get::<bool, _>("active"),
        }
    }))))
}

#[derive(Deserialize)]
struct AddAvailabilityBody {
    weekday: i32,
    start_time: String,
    end_time: String,
    timezone: Option<String>,
}

/// Add a mentor availability slot.
#[utoipa::path(
    post, path = "/api/mentors/me/availability", tag = "challenges",
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn add_availability(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<AddAvailabilityBody>,
) -> Result<Json<Value>, AppError> {
    if !(0..=6).contains(&body.weekday) {
        return Err(AppError::Validation("weekday must be 0-6".into()));
    }
    let start = chrono::NaiveTime::parse_from_str(&body.start_time, "%H:%M")
        .or_else(|_| chrono::NaiveTime::parse_from_str(&body.start_time, "%H:%M:%S"))
        .map_err(|_| AppError::Validation("invalid start_time".into()))?;
    let end = chrono::NaiveTime::parse_from_str(&body.end_time, "%H:%M")
        .or_else(|_| chrono::NaiveTime::parse_from_str(&body.end_time, "%H:%M:%S"))
        .map_err(|_| AppError::Validation("invalid end_time".into()))?;
    if end <= start {
        return Err(AppError::Validation("end_time must be > start_time".into()));
    }
    let inserted: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO mentor_availability (mentor_user_id, weekday, start_time, end_time, timezone)
        VALUES ($1, $2, $3, $4, $5) RETURNING id
        "#,
    )
    .bind(auth.user_id)
    .bind(body.weekday)
    .bind(start)
    .bind(end)
    .bind(body.timezone.unwrap_or_else(|| "UTC".into()))
    .fetch_one(&state.db)
    .await?;
    Ok(Json(build_response(
        json!({ "availability_id": inserted.0 }),
    )))
}

// ─── Réservation ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct BookSessionBody {
    mentor_user_id: Uuid,
    scheduled_at: chrono::DateTime<chrono::Utc>,
    duration_minutes: i32,
    mentee_notes: Option<String>,
}

/// Book a mentorship session.
#[utoipa::path(
    post, path = "/api/mentorship/sessions", tag = "challenges",
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn book_session(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<BookSessionBody>,
) -> Result<Json<Value>, AppError> {
    if body.mentor_user_id == auth.user_id {
        return Err(AppError::Validation("cannot mentor yourself".into()));
    }
    if body.duration_minutes < 15 || body.duration_minutes > 240 {
        return Err(AppError::Validation("duration_minutes 15-240".into()));
    }
    if body.scheduled_at < chrono::Utc::now() + chrono::Duration::hours(1) {
        return Err(AppError::Validation(
            "scheduled_at must be at least 1h in the future".into(),
        ));
    }
    let mentor = sqlx::query(
        "SELECT hourly_rate_eur_cents, min_session_minutes, active FROM mentor_profiles WHERE user_id = $1",
    )
    .bind(body.mentor_user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("mentor not found".into()))?;
    if !mentor.get::<bool, _>("active") {
        return Err(AppError::Validation("mentor not active".into()));
    }
    let rate: i64 = mentor.get("hourly_rate_eur_cents");
    let min_min: i32 = mentor.get("min_session_minutes");
    if body.duration_minutes < min_min {
        return Err(AppError::Validation(format!(
            "minimum session duration is {min_min} minutes"
        )));
    }

    // Prix = tarif horaire × durée / 60. Arrondi au centime.
    let total = (rate as f64 * body.duration_minutes as f64 / 60.0).round() as i64;
    let platform_cut = (total * PLATFORM_FEE_BPS) / 10_000;
    let mentor_cut = total - platform_cut;

    // Vérifier collision (mentor n'accepte pas 2 sessions qui se chevauchent).
    let end = body.scheduled_at + chrono::Duration::minutes(body.duration_minutes as i64);
    let collision: Option<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT id FROM mentorship_sessions
        WHERE mentor_user_id = $1
          AND status IN ('paid', 'confirmed', 'pending')
          AND scheduled_at < $2
          AND scheduled_at + (duration_minutes || ' minutes')::INTERVAL > $3
        LIMIT 1
        "#,
    )
    .bind(body.mentor_user_id)
    .bind(end)
    .bind(body.scheduled_at)
    .fetch_optional(&state.db)
    .await?;
    if collision.is_some() {
        return Err(AppError::Validation(
            "mentor already booked at that time".into(),
        ));
    }

    let inserted: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO mentorship_sessions
            (mentor_user_id, mentee_user_id, scheduled_at, duration_minutes,
             price_total_cents, price_mentor_cents, price_platform_cents,
             currency, status, mentee_notes)
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'EUR', 'pending', $8)
        RETURNING id
        "#,
    )
    .bind(body.mentor_user_id)
    .bind(auth.user_id)
    .bind(body.scheduled_at)
    .bind(body.duration_minutes)
    .bind(total)
    .bind(mentor_cut)
    .bind(platform_cut)
    .bind(&body.mentee_notes)
    .fetch_one(&state.db)
    .await?;

    // Which way of taking money reaches this payer. A card through Stripe;
    // Mobile Money through FedaPay for the franc zone, which Stripe cannot
    // serve at all — and which is how most people in Benin hold money.
    //
    // This used to build a fake `Pack` with a `Box::leak`ed slug, to squeeze
    // a session through a helper shaped for credit packs. It leaked a
    // string per booking and it only ever reached card payers.
    let payer: (String, String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT u.email, u.display_name, u.country_iso2, w.momo_phone
           FROM users u
           LEFT JOIN talent_wallets w ON w.user_id = u.id
          WHERE u.id = $1",
    )
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;
    let (email, display_name, country, phone) = payer;

    // Sessions are priced in EUR today. When they are not, the currency
    // comes from the row above and the rest of this needs no change — which
    // is the point of routing on it rather than on a provider name.
    let currency = crate::services::ledger::Currency::Eur;
    let method = if currency == crate::services::ledger::Currency::Xof && phone.is_some() {
        crate::services::collect::Method::MobileMoney
    } else {
        crate::services::collect::Method::Card
    };

    let registry = crate::services::collect_adapters::registry_from_env();
    let provider = registry
        .resolve(&state.db, country.as_deref(), currency, method)
        .await?;

    let amount = bigdecimal::BigDecimal::from(total) / bigdecimal::BigDecimal::from(100);
    let base = state.config.frontend_url.trim_end_matches('/').to_string();
    let success_url = format!("{base}/mentorship/sessions/{}?paid=1", inserted.0);
    let cancel_url = format!("{base}/mentorship/sessions/{}?canceled=1", inserted.0);
    let idempotency_key = format!("mentorship_session:{}", inserted.0);

    let checkout = crate::services::collect::start(
        &state.db,
        provider.as_ref(),
        method,
        crate::services::collect::CollectionRequest {
            payer_id: Some(auth.user_id),
            payer_enterprise_id: None,
            payer_email: &email,
            payer_name: &display_name,
            payer_country: country.as_deref(),
            payer_phone: phone.as_deref(),
            subject_type: "mentorship_session",
            subject_id: inserted.0,
            amount: &amount,
            currency,
            description: "Skilluv — session de mentorat",
            success_url: &success_url,
            cancel_url: &cancel_url,
            idempotency_key: &idempotency_key,
            operator: None,
            credits: None,
            merchant_reference: None,
        },
    )
    .await?;

    Ok(Json(build_response(json!({
        "session_id": inserted.0,
        "checkout_url": checkout.redirect_url,
        "payment_id": checkout.payment_id,
        "provider": checkout.provider,
        "price_total_cents": total,
        "mentor_share_cents": mentor_cut,
        "platform_share_cents": platform_cut,
    }))))
}

/// List my mentorship sessions.
#[utoipa::path(
    get, path = "/api/mentorship/sessions", tag = "challenges",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn list_my_sessions(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT s.id, s.scheduled_at, s.duration_minutes, s.status,
               s.price_total_cents, s.currency, s.meeting_url,
               s.mentor_user_id, s.mentee_user_id,
               mu.display_name AS mentor_name, meu.display_name AS mentee_name
        FROM mentorship_sessions s
        JOIN users mu ON mu.id = s.mentor_user_id
        JOIN users meu ON meu.id = s.mentee_user_id
        WHERE s.mentor_user_id = $1 OR s.mentee_user_id = $1
        ORDER BY s.scheduled_at DESC
        LIMIT 100
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let mentor_id: Uuid = r.get("mentor_user_id");
            json!({
                "id": r.get::<Uuid, _>("id"),
                "role": if mentor_id == auth.user_id { "mentor" } else { "mentee" },
                "scheduled_at": r.get::<chrono::DateTime<chrono::Utc>, _>("scheduled_at"),
                "duration_minutes": r.get::<i32, _>("duration_minutes"),
                "status": r.get::<String, _>("status"),
                "price_total_cents": r.get::<i64, _>("price_total_cents"),
                "currency": r.get::<String, _>("currency"),
                "meeting_url": r.get::<Option<String>, _>("meeting_url"),
                "counterparty_name": if mentor_id == auth.user_id {
                    r.get::<String, _>("mentee_name")
                } else {
                    r.get::<String, _>("mentor_name")
                },
            })
        })
        .collect();
    Ok(Json(build_response(json!({ "sessions": items }))))
}

/// Cancel a mentorship session.
#[utoipa::path(
    post, path = "/api/mentorship/sessions/{id}/cancel", tag = "challenges",
    params(("id" = uuid::Uuid, Path)),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn cancel_session(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT mentor_user_id, mentee_user_id, status, scheduled_at,
               price_total_cents, stripe_payment_intent_id
        FROM mentorship_sessions WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("session not found".into()))?;
    let mentor_id: Uuid = row.get("mentor_user_id");
    let mentee_id: Uuid = row.get("mentee_user_id");
    if auth.user_id != mentor_id && auth.user_id != mentee_id {
        return Err(AppError::Forbidden);
    }
    let status: String = row.get("status");
    if !matches!(status.as_str(), "pending" | "paid" | "confirmed") {
        return Err(AppError::Validation(format!(
            "cannot cancel session in status '{status}'"
        )));
    }
    let scheduled: chrono::DateTime<chrono::Utc> = row.get("scheduled_at");
    let price_total_cents: i64 = row.get("price_total_cents");
    let payment_intent: Option<String> = row.get("stripe_payment_intent_id");

    // Politique refund :
    //   - mentor annule → 100% refund
    //   - mentee annule ≥24h avant → 100% refund
    //   - mentee annule <24h avant → 50% refund
    //   - session pas encore payée → pas de refund à émettre
    let hours_before = (scheduled - chrono::Utc::now()).num_hours();
    let mentee_cancels = auth.user_id == mentee_id;
    let refund_ratio: f64 = if !mentee_cancels || hours_before >= 24 {
        1.0
    } else {
        0.5
    };
    let is_paid = matches!(status.as_str(), "paid" | "confirmed");
    let refund_amount_cents: i64 = ((price_total_cents as f64) * refund_ratio).round() as i64;

    let mut refund_id: Option<String> = None;
    if is_paid
        && refund_amount_cents > 0
        && let Some(pi) = payment_intent.as_deref()
        && let Some(cfg) = crate::services::stripe::StripeConfig::from_env()
    {
        match crate::services::stripe::create_refund(
            &cfg,
            pi,
            Some(refund_amount_cents),
            Some("requested_by_customer"),
        )
        .await
        {
            Ok(r) => {
                refund_id = Some(r.id);
                metrics::counter!(
                    "skilluv_stripe_refunds_total",
                    "kind" => "mentorship"
                )
                .increment(1);
            }
            Err(e) => {
                tracing::warn!(
                    session_id = %id,
                    error = %e,
                    "stripe refund failed — marking session cancelled anyway"
                );
            }
        }
    }

    let final_status = if is_paid && refund_amount_cents > 0 {
        "refunded"
    } else if mentee_cancels {
        "cancelled_by_mentee"
    } else {
        "cancelled_by_mentor"
    };

    sqlx::query("UPDATE mentorship_sessions SET status = $1 WHERE id = $2")
        .bind(final_status)
        .bind(id)
        .execute(&state.db)
        .await?;

    Ok(Json(build_response(json!({
        "status": final_status,
        "refund_amount_cents": refund_amount_cents,
        "refund_ratio": refund_ratio,
        "stripe_refund_id": refund_id,
    }))))
}

// ─── Stripe Connect onboarding ───────────────────────────────────

/// Start Stripe Connect onboarding for mentor payouts.
#[utoipa::path(
    post, path = "/api/mentors/me/connect/onboard", tag = "wallet",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn start_connect_onboarding(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let cfg = crate::services::stripe::StripeConfig::from_env().ok_or(
        AppError::ServiceUnavailable("payments are not configured on this deployment".into()),
    )?;
    let profile =
        sqlx::query("SELECT stripe_connect_account_id FROM mentor_profiles WHERE user_id = $1")
            .bind(auth.user_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::NotFound(
                "mentor profile not found — create one first".into(),
            ))?;
    let existing_account: Option<String> = profile.get("stripe_connect_account_id");

    let user_row = sqlx::query("SELECT email, country_iso2 FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;
    let email: String = user_row.get("email");
    let country: String = user_row
        .get::<Option<String>, _>("country_iso2")
        .unwrap_or_else(|| "FR".to_string());

    let account_id = if let Some(id) = existing_account {
        id
    } else {
        let account =
            crate::services::stripe::create_connect_account(&cfg, &email, &country).await?;
        sqlx::query("UPDATE mentor_profiles SET stripe_connect_account_id = $1 WHERE user_id = $2")
            .bind(&account.id)
            .bind(auth.user_id)
            .execute(&state.db)
            .await?;
        account.id
    };

    let base_url =
        std::env::var("APP_BASE_URL").unwrap_or_else(|_| crate::config::PUBLIC_SITE_URL.into());
    let link = crate::services::stripe::create_account_link(
        &cfg,
        &account_id,
        &format!("{base_url}/mentor/onboard/refresh"),
        &format!("{base_url}/mentor/onboard/complete"),
    )
    .await?;
    Ok(Json(build_response(json!({
        "onboarding_url": link.url,
        "expires_at": link.expires_at,
        "account_id": account_id,
    }))))
}

/// Get Stripe Connect onboarding status.
#[utoipa::path(
    get, path = "/api/mentors/me/connect/status", tag = "wallet",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn connect_status(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let cfg = crate::services::stripe::StripeConfig::from_env().ok_or(
        AppError::ServiceUnavailable("payments are not configured on this deployment".into()),
    )?;
    let profile =
        sqlx::query("SELECT stripe_connect_account_id FROM mentor_profiles WHERE user_id = $1")
            .bind(auth.user_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::NotFound("no mentor profile".into()))?;
    let account_id: Option<String> = profile.get("stripe_connect_account_id");
    let Some(account_id) = account_id else {
        return Ok(Json(build_response(json!({
            "onboarded": false,
            "message": "no Stripe Connect account yet"
        }))));
    };
    let account = crate::services::stripe::retrieve_connect_account(&cfg, &account_id).await?;
    Ok(Json(build_response(json!({
        "account_id": account.id,
        "onboarded": account.details_submitted,
        "charges_enabled": account.charges_enabled,
        "payouts_enabled": account.payouts_enabled,
    }))))
}

/// Record what a completed session owes, and hold it.
///
/// Split out of the handler so the arithmetic has one home. Getting an
/// amount wrong here is the kind of mistake that is only noticed by the
/// person who was underpaid.
///
/// ## Minor units
///
/// `price_*_cents` is a hundredth of the currency, which is right for EUR and
/// meaningless for XOF — the franc CFA has no subdivision. Dividing an XOF
/// price by a hundred would pay a mentor one percent of what they earned, so
/// the conversion is explicit per currency rather than a blanket `/ 100`.
async fn capture_session_funds(
    state: &AppState,
    session_id: Uuid,
    mentor_id: Uuid,
    // The student. They paid, so they are the one who may dispute — the
    // release window is their recourse and it needs to know whose it is.
    mentee_id: Uuid,
    mentor_cents: i64,
    platform_cents: i64,
    currency_str: &str,
) -> Result<(), AppError> {
    use crate::services::ledger::{self, Currency};
    use crate::services::release;

    let currency: Currency = currency_str.parse()?;
    let to_amount = |minor: i64| -> BigDecimal {
        match currency {
            Currency::Eur => BigDecimal::from(minor) / BigDecimal::from(100),
            // Already whole francs: the column name is a misnomer here.
            Currency::Xof => BigDecimal::from(minor),
        }
    };

    let mentor_share = to_amount(mentor_cents);
    let platform_share = to_amount(platform_cents);
    let gross = mentor_share.clone() + platform_share.clone();

    // The student paid at booking, so the money is already at the provider.
    // This records whose it is.
    let posted = ledger::capture_for_recipient(
        &state.db,
        "stripe",
        format!("mentorship_session:{session_id}"),
        mentor_id,
        gross,
        platform_share,
        currency,
        "mentorship_session",
        session_id,
    )
    .await?;

    // Nothing more to do on a replay: the hold already exists, and creating
    // a second would hold the same money twice.
    if posted.was_replay() {
        return Ok(());
    }

    let window = release::window_for(&state.db, "mentorship_session").await?;
    let mut tx = state.db.begin().await?;
    let release_at = release::hold(
        &mut tx,
        release::Hold {
            ledger_transaction_id: posted.transaction_id(),
            beneficiary_id: mentor_id,
            subject_type: "mentorship_session",
            subject_id: session_id,
            amount: &mentor_share,
            currency,
            hold_hours: window.hold_hours,
            payer_id: Some(mentee_id),
            payer_enterprise_id: None,
        },
    )
    .await?;
    tx.commit().await?;

    tracing::info!(
        session_id = %session_id,
        mentor = %mentor_id,
        amount = %mentor_share,
        currency = currency.as_str(),
        release_at = %release_at,
        "mentor share held pending release"
    );
    Ok(())
}

/// Mark a mentorship session complete (mentor).
#[utoipa::path(
    post, path = "/api/mentorship/sessions/{id}/complete", tag = "challenges",
    params(("id" = uuid::Uuid, Path)),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn mark_completed(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let row = sqlx::query(
        "SELECT mentor_user_id, mentee_user_id, status FROM mentorship_sessions WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("session not found".into()))?;
    let mentor_id: Uuid = row.get("mentor_user_id");
    // The student paid, so the hold records them as the one who may dispute.
    let mentee_id: Uuid = row.get("mentee_user_id");
    let status: String = row.get("status");
    if auth.user_id != mentor_id {
        return Err(AppError::Forbidden);
    }
    if !matches!(status.as_str(), "paid" | "confirmed") {
        return Err(AppError::Validation(format!(
            "session in state '{status}' cannot be completed"
        )));
    }
    // Record what the mentor is owed and hold it. No money is wired here.
    //
    // It used to be: complete the session, wire the mentor immediately. That
    // paid people before the student had any chance to say the session never
    // happened, and it only worked for mentors Stripe can reach — everyone
    // else silently got nothing at all.
    //
    // Now the amount lands in the mentor's `pending` account and waits out
    // the window from `release_windows` (seven days for a session, sooner if
    // the student confirms). Withdrawing is a separate act, over whichever
    // rail reaches them.
    let details = sqlx::query(
        r#"
        SELECT s.price_mentor_cents, s.price_platform_cents, s.currency
        FROM mentorship_sessions s
        WHERE s.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;

    let mut payout_status = "held";
    let mut payout_error: Option<String> = None;

    if let Some(row) = details {
        let mentor_cents: i64 = row.get("price_mentor_cents");
        let platform_cents: i64 = row.get("price_platform_cents");
        let currency_str: String = row.get("currency");

        if mentor_cents > 0 {
            if let Err(e) = capture_session_funds(
                &state,
                id,
                mentor_id,
                mentee_id,
                mentor_cents,
                platform_cents,
                &currency_str,
            )
            .await
            {
                // The session still completes: the mentoring happened, and
                // refusing to record it because our books hiccuped punishes
                // the wrong person. But it is not marked held, so it surfaces
                // as owed and unrecorded rather than disappearing.
                payout_status = "failed";
                payout_error = Some(e.to_string());
                metrics::counter!("skilluv_mentorship_capture_failed_total").increment(1);
                tracing::error!(
                    session_id = %id, mentor = %mentor_id, error = %e,
                    "failed to record what the mentor is owed - session completed, nothing held"
                );
            }
        } else {
            // A free session owes nobody anything, so there is no debt to
            // hold and nothing to release later.
            payout_status = "released";
        }
    }

    sqlx::query(
        r#"
        UPDATE mentorship_sessions
        SET status = 'completed',
            payout_status = $2,
            payout_error = $3,
            -- Stamped when the money becomes withdrawable, which is the
            -- release, not the completion.
            payout_released_at = CASE WHEN $2 = 'released' THEN NOW() ELSE NULL END
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(payout_status)
    .bind(payout_error.as_deref())
    .execute(&state.db)
    .await?;
    sqlx::query(
        "UPDATE mentor_profiles SET total_sessions = total_sessions + 1 WHERE user_id = $1",
    )
    .bind(mentor_id)
    .execute(&state.db)
    .await?;

    // P20.2 — Best-effort recompute proof engines pour le mentor : la 3ᵉ
    // session complétée peut débloquer la capability `mentor`
    // (capabilities_engine seuil).
    // SKI-43 — live variant: AppState is available here, so the mentor is
    // notified in real time as well as durably.
    let db_clone = state.db.clone();
    let mut redis_clone = state.redis.clone();
    let ws_clone = state.ws.clone();
    tokio::spawn(async move {
        let _ = crate::services::proof_hooks::recompute_all_for_user_live(
            &db_clone,
            &mut redis_clone,
            &ws_clone,
            mentor_id,
        )
        .await;
    });

    // `stripe_transfer_id` is gone: completing a session no longer wires
    // anything. What the caller wants to know now is whether the money is
    // held and when it frees up.
    Ok(Json(build_response(json!({
        "completed": true,
        "payout_status": payout_status,
    }))))
}

/// Payload of `POST /mentorship/sessions/{id}/confirm`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SessionConfirmed {
    pub confirmed: bool,
    /// True when this confirmation released the mentor's money immediately.
    pub funds_released: bool,
}

/// The student confirms the session took place, paying the mentor now.
///
/// Without this, the mentor waits out the full window even when both people
/// agree it went well. That wait exists to protect the student; a student who
/// says they do not need it should be able to waive it.
///
/// Only the student can call this. A mentor confirming their own session
/// would be the mentor releasing their own money, which is the thing the
/// window exists to prevent.
#[utoipa::path(
    post,
    path = "/api/mentorship/sessions/{id}/confirm",
    tag = "mentorship",
    params(("id" = Uuid, Path, description = "Session UUID")),
    responses(
        (status = 200, description = "Confirmed, funds released if they were held", body = ApiResponse<SessionConfirmed>),
        (status = 400, description = "Session not completed yet, or disputed", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Only the student can confirm", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such session", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn confirm_session(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let row = sqlx::query(
        "SELECT mentee_user_id, mentor_user_id, status, payout_status,
                confirmed_by_mentee_at
           FROM mentorship_sessions WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("session not found".into()))?;

    let mentee_id: Uuid = row.get("mentee_user_id");
    if auth.user_id != mentee_id {
        return Err(AppError::Forbidden);
    }

    let status: String = row.get("status");
    if status != "completed" {
        return Err(AppError::Validation(format!(
            "session is '{status}' — there is nothing to confirm until the \
             mentor marks it complete"
        )));
    }

    let already: Option<chrono::DateTime<chrono::Utc>> = row.get("confirmed_by_mentee_at");
    if already.is_some() {
        // Idempotent: a second click is the same intent, already satisfied.
        return Ok(Json(build_response(json!({
            "confirmed": true,
            "funds_released": false,
        }))));
    }

    sqlx::query("UPDATE mentorship_sessions SET confirmed_by_mentee_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    let released =
        crate::services::release::release_early(&state.db, "mentorship_session", id).await?;

    if released {
        sqlx::query("UPDATE mentorship_sessions SET payout_status = 'released', payout_released_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&state.db)
            .await?;

        let mentor_id: Uuid = row.get("mentor_user_id");
        let _ = crate::services::notify::send(
            &state,
            crate::services::notify::Recipient::User(mentor_id),
            "funds.released",
        )
        .payload(json!({ "session_id": id }))
        .execute()
        .await;
    }

    Ok(Json(build_response(json!({
        "confirmed": true,
        "funds_released": released,
    }))))
}

#[derive(Deserialize)]
struct ReviewBody {
    rating: i32,
    comment: Option<String>,
}

/// Submit a review for a completed session.
#[utoipa::path(
    post, path = "/api/mentorship/sessions/{id}/review", tag = "challenges",
    params(("id" = uuid::Uuid, Path)),
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
    operation_id = "mentorshipSubmitReview",
)]
pub async fn submit_review(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(session_id): Path<Uuid>,
    Json(body): Json<ReviewBody>,
) -> Result<Json<Value>, AppError> {
    if !(1..=5).contains(&body.rating) {
        return Err(AppError::Validation("rating must be 1-5".into()));
    }
    let row = sqlx::query(
        "SELECT mentee_user_id, mentor_user_id, status FROM mentorship_sessions WHERE id = $1",
    )
    .bind(session_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("session not found".into()))?;
    let mentee_id: Uuid = row.get("mentee_user_id");
    let mentor_id: Uuid = row.get("mentor_user_id");
    let status: String = row.get("status");
    if auth.user_id != mentee_id {
        return Err(AppError::Forbidden);
    }
    if status != "completed" {
        return Err(AppError::Validation(
            "can only review completed sessions".into(),
        ));
    }
    sqlx::query(
        r#"
        INSERT INTO mentorship_reviews (session_id, reviewer_user_id, rating, comment)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (session_id) DO UPDATE SET
            rating = EXCLUDED.rating,
            comment = EXCLUDED.comment
        "#,
    )
    .bind(session_id)
    .bind(auth.user_id)
    .bind(body.rating)
    .bind(&body.comment)
    .execute(&state.db)
    .await?;
    // Recalcul de la note moyenne du mentor
    sqlx::query(
        r#"
        UPDATE mentor_profiles SET avg_rating = (
            SELECT ROUND(AVG(r.rating)::NUMERIC, 2)
            FROM mentorship_reviews r
            JOIN mentorship_sessions s ON s.id = r.session_id
            WHERE s.mentor_user_id = $1
        )
        WHERE user_id = $1
        "#,
    )
    .bind(mentor_id)
    .execute(&state.db)
    .await?;
    Ok(Json(build_response(json!({ "review_saved": true }))))
}
