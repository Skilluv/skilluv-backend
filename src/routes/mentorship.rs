//! Mentorship — Phase 5.11.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use bigdecimal::BigDecimal;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;
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

async fn list_mentors(
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

async fn get_mentor_profile(
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

async fn upsert_my_mentor_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpsertMentorBody>,
) -> Result<Json<Value>, AppError> {
    if body.hourly_rate_eur_cents < 0 || body.hourly_rate_eur_cents > 100_000_00 {
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

async fn get_my_mentor_profile(
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

async fn add_availability(
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

async fn book_session(
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

    // Stripe checkout (le webhook marquera 'paid').
    let cfg = crate::services::stripe::StripeConfig::from_env()
        .ok_or(AppError::Internal("Stripe not configured".into()))?;
    let pack = crate::services::stripe::Pack {
        slug: Box::leak(format!("mentorship_{}", inserted.0.simple()).into_boxed_str()),
        credits: 0,
        price_eur_cents: total,
        stripe_price_lookup_key: "skilluv_mentorship_session",
    };
    let email = sqlx::query_scalar::<_, String>("SELECT email FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;
    let checkout = crate::services::stripe::create_checkout_session(
        &cfg,
        &pack,
        &email,
        &inserted.0.to_string(),
        &[
            ("purpose", "mentorship".to_string()),
            ("session_id", inserted.0.to_string()),
            ("mentor_user_id", body.mentor_user_id.to_string()),
        ],
    )
    .await?;
    Ok(Json(build_response(json!({
        "session_id": inserted.0,
        "checkout_url": checkout.checkout_url,
        "price_total_cents": total,
        "mentor_share_cents": mentor_cut,
        "platform_share_cents": platform_cut,
    }))))
}

async fn list_my_sessions(
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

async fn cancel_session(
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
    let refund_ratio: f64 = if !mentee_cancels {
        1.0
    } else if hours_before >= 24 {
        1.0
    } else {
        0.5
    };
    let is_paid = matches!(status.as_str(), "paid" | "confirmed");
    let refund_amount_cents: i64 = ((price_total_cents as f64) * refund_ratio).round() as i64;

    let mut refund_id: Option<String> = None;
    if is_paid && refund_amount_cents > 0 {
        if let Some(pi) = payment_intent.as_deref() {
            if let Some(cfg) = crate::services::stripe::StripeConfig::from_env() {
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

async fn start_connect_onboarding(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let cfg = crate::services::stripe::StripeConfig::from_env()
        .ok_or(AppError::Internal("Stripe not configured".into()))?;
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

    let base_url = std::env::var("APP_BASE_URL").unwrap_or_else(|_| "https://skilluv.com".into());
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

async fn connect_status(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let cfg = crate::services::stripe::StripeConfig::from_env()
        .ok_or(AppError::Internal("Stripe not configured".into()))?;
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

async fn mark_completed(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let row = sqlx::query("SELECT mentor_user_id, status FROM mentorship_sessions WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("session not found".into()))?;
    let mentor_id: Uuid = row.get("mentor_user_id");
    let status: String = row.get("status");
    if auth.user_id != mentor_id {
        return Err(AppError::Forbidden);
    }
    if !matches!(status.as_str(), "paid" | "confirmed") {
        return Err(AppError::Validation(format!(
            "session in state '{status}' cannot be completed"
        )));
    }
    // Transfer 80% part mentor vers son compte Connect si configuré.
    let details = sqlx::query(
        r#"
        SELECT s.price_mentor_cents, s.currency, s.stripe_payment_intent_id,
               m.stripe_connect_account_id
        FROM mentorship_sessions s
        JOIN mentor_profiles m ON m.user_id = s.mentor_user_id
        WHERE s.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;

    let mut transfer_id: Option<String> = None;
    if let Some(row) = details {
        let mentor_cents: i64 = row.get("price_mentor_cents");
        let currency: String = row.get("currency");
        let connect_id: Option<String> = row.get("stripe_connect_account_id");
        if let Some(connect_id) = connect_id {
            if mentor_cents > 0 {
                if let Some(cfg) = crate::services::stripe::StripeConfig::from_env() {
                    match crate::services::stripe::create_transfer(
                        &cfg,
                        &connect_id,
                        mentor_cents,
                        &currency,
                        &format!("mentorship_session:{id}"),
                    )
                    .await
                    {
                        Ok(v) => {
                            transfer_id = v.get("id").and_then(|x| x.as_str()).map(String::from);
                            metrics::counter!("skilluv_stripe_connect_transfers_total")
                                .increment(1);
                        }
                        Err(e) => tracing::warn!(
                            session_id = %id,
                            error = %e,
                            "stripe connect transfer failed"
                        ),
                    }
                }
            }
        }
    }

    sqlx::query(
        r#"
        UPDATE mentorship_sessions
        SET status = 'completed', payout_released_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(id)
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
    let db_clone = state.db.clone();
    tokio::spawn(async move {
        let _ = crate::services::proof_hooks::recompute_all_for_user(&db_clone, mentor_id).await;
    });

    Ok(Json(build_response(json!({
        "completed": true,
        "stripe_transfer_id": transfer_id,
    }))))
}

#[derive(Deserialize)]
struct ReviewBody {
    rating: i32,
    comment: Option<String>,
}

async fn submit_review(
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
