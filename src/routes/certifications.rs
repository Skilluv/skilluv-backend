//! Certifications payantes — Phase 5.10.
//!
//! Endpoints :
//!   GET  /api/certifications                   catalogue public
//!   POST /api/certifications/{slug}/purchase   crée Stripe checkout
//!   POST /api/certifications/attempts/{id}/start  démarre le timer après paiement
//!   POST /api/certifications/attempts/{id}/submit {answers} finalise + score
//!   GET  /api/diplomas/verify/{code}           vérif publique (no-auth)
//!   GET  /api/diplomas/my                       liste diplômes du user

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn certification_routes() -> Router<AppState> {
    Router::new()
        .route("/certifications", get(list_certifications))
        .route(
            "/certifications/{slug}/purchase",
            post(purchase_certification),
        )
        .route("/certifications/attempts/{id}/start", post(start_attempt))
        .route("/certifications/attempts/{id}/submit", post(submit_attempt))
        .route("/diplomas/verify/{code}", get(verify_diploma))
        .route("/diplomas/my", get(my_diplomas))
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

// ─── Catalogue ───────────────────────────────────────────────────

async fn list_certifications(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT id, slug, title, description, skill_domain, level, price_eur_cents,
               duration_minutes, passing_score, validity_months,
               array_length(challenge_ids, 1) AS challenges_count
        FROM certifications WHERE active = TRUE
        ORDER BY skill_domain, level, price_eur_cents
        "#,
    )
    .fetch_all(&state.db)
    .await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<Uuid, _>("id"),
                "slug": r.get::<String, _>("slug"),
                "title": r.get::<String, _>("title"),
                "description": r.get::<String, _>("description"),
                "skill_domain": r.get::<String, _>("skill_domain"),
                "level": r.get::<String, _>("level"),
                "price_eur_cents": r.get::<i64, _>("price_eur_cents"),
                "duration_minutes": r.get::<i32, _>("duration_minutes"),
                "passing_score": r.get::<i32, _>("passing_score"),
                "validity_months": r.get::<i32, _>("validity_months"),
                "challenges_count": r.get::<Option<i32>, _>("challenges_count").unwrap_or(0),
            })
        })
        .collect();
    Ok(Json(build_response(json!({ "certifications": items }))))
}

// ─── Achat (Stripe direct) ───────────────────────────────────────

async fn purchase_certification(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let cert = sqlx::query(
        "SELECT id, title, price_eur_cents, active FROM certifications WHERE slug = $1",
    )
    .bind(&slug)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("certification not found".into()))?;

    let active: bool = cert.get("active");
    if !active {
        return Err(AppError::Validation("certification not active".into()));
    }
    let cert_id: Uuid = cert.get("id");
    let price_cents: i64 = cert.get("price_eur_cents");
    let title: String = cert.get("title");

    // Anti-duplication : bloquer si une tentative pending/paid/started existe.
    let existing: Option<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT id, status FROM certification_attempts
        WHERE user_id = $1 AND certification_id = $2
          AND status IN ('pending', 'paid', 'started')
        ORDER BY created_at DESC LIMIT 1
        "#,
    )
    .bind(auth.user_id)
    .bind(cert_id)
    .fetch_optional(&state.db)
    .await?;
    if let Some((existing_id, status)) = existing {
        return Ok(Json(build_response(json!({
            "attempt_id": existing_id,
            "status": status,
            "message": "existing attempt already in progress"
        }))));
    }

    let attempt: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO certification_attempts
            (user_id, certification_id, amount_paid_cents, currency, status)
        VALUES ($1, $2, $3, 'EUR', 'pending')
        RETURNING id
        "#,
    )
    .bind(auth.user_id)
    .bind(cert_id)
    .bind(price_cents)
    .fetch_one(&state.db)
    .await?;

    // Stripe checkout via helper existant. Ici on utilise Stripe pour un pack
    // synthétique "certification" — le webhook `checkout.session.completed`
    // marquera l'attempt comme 'paid'.
    let cfg = crate::services::stripe::StripeConfig::from_env()
        .ok_or(AppError::Internal("Stripe not configured".into()))?;
    let pack = crate::services::stripe::Pack {
        slug: Box::leak(format!("cert_{slug}").into_boxed_str()),
        credits: 0,
        price_eur_cents: price_cents,
        stripe_price_lookup_key: Box::leak(format!("skilluv_cert_{slug}").into_boxed_str()),
    };
    let user_email: Option<(String,)> = sqlx::query_as("SELECT email FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_optional(&state.db)
        .await?;
    let email = user_email
        .map(|(e,)| e)
        .ok_or(AppError::NotFound("user not found".into()))?;

    let session = crate::services::stripe::create_checkout_session(
        &cfg,
        &pack,
        &email,
        &attempt.0.to_string(),
        &[
            ("purpose", "certification".to_string()),
            ("attempt_id", attempt.0.to_string()),
            ("certification_id", cert_id.to_string()),
            ("certification_title", title),
        ],
    )
    .await?;

    Ok(Json(build_response(json!({
        "attempt_id": attempt.0,
        "checkout_url": session.checkout_url,
        "session_id": session.session_id,
    }))))
}

// ─── Démarrer la tentative (après paiement) ─────────────────────

async fn start_attempt(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(attempt_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT a.status, a.certification_id, c.duration_minutes, c.challenge_ids
        FROM certification_attempts a
        JOIN certifications c ON c.id = a.certification_id
        WHERE a.id = $1 AND a.user_id = $2
        "#,
    )
    .bind(attempt_id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("attempt not found".into()))?;

    let status: String = row.get("status");
    if status != "paid" {
        return Err(AppError::Validation(format!(
            "attempt not in state 'paid' (current: {status})"
        )));
    }
    let challenge_ids: Vec<Uuid> = row.get("challenge_ids");
    let duration: i32 = row.get("duration_minutes");

    sqlx::query(
        "UPDATE certification_attempts SET status = 'started', started_at = NOW() WHERE id = $1",
    )
    .bind(attempt_id)
    .execute(&state.db)
    .await?;

    Ok(Json(build_response(json!({
        "attempt_id": attempt_id,
        "challenge_ids": challenge_ids,
        "duration_minutes": duration,
        "deadline": chrono::Utc::now() + chrono::Duration::minutes(duration as i64),
    }))))
}

// ─── Soumission finale + score ───────────────────────────────────

async fn submit_attempt(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(attempt_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT a.status, a.started_at, a.certification_id,
               c.passing_score, c.validity_months, c.title,
               c.duration_minutes, c.challenge_ids
        FROM certification_attempts a
        JOIN certifications c ON c.id = a.certification_id
        WHERE a.id = $1 AND a.user_id = $2
        "#,
    )
    .bind(attempt_id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("attempt not found".into()))?;

    let status: String = row.get("status");
    if status != "started" {
        return Err(AppError::Validation(format!(
            "attempt not 'started' (current: {status})"
        )));
    }
    let passing: i32 = row.get("passing_score");
    let validity_months: i32 = row.get("validity_months");
    let cert_id: Uuid = row.get("certification_id");
    let cert_title: String = row.get("title");
    let duration_minutes: i32 = row.get("duration_minutes");
    let challenge_ids: Vec<Uuid> = row.get("challenge_ids");
    let started_at: chrono::DateTime<chrono::Utc> = row.get("started_at");

    // Timeout check : si on est au-delà de duration_minutes depuis started_at,
    // on marque expired plutôt que passed.
    let deadline = started_at + chrono::Duration::minutes(duration_minutes as i64);
    let overtime = chrono::Utc::now() > deadline + chrono::Duration::minutes(2);

    // Recalcul SERVEUR du score depuis les soumissions du user pour les
    // challenges de la cert, faites APRÈS started_at.
    let score = if challenge_ids.is_empty() {
        0
    } else {
        let per_challenge: Vec<(Uuid, Option<i32>)> = sqlx::query_as(
            r#"
            SELECT cs.challenge_id, MAX(cs.score) AS best_score
            FROM challenge_submissions cs
            WHERE cs.user_id = $1
              AND cs.challenge_id = ANY($2)
              AND cs.evaluated_at >= $3
              AND cs.status = 'evaluated'
            GROUP BY cs.challenge_id
            "#,
        )
        .bind(auth.user_id)
        .bind(&challenge_ids)
        .bind(started_at)
        .fetch_all(&state.db)
        .await?;
        let total: i32 = per_challenge.iter().filter_map(|(_, s)| *s).sum();
        let denom = challenge_ids.len() as i32;
        if denom > 0 {
            (total / denom).clamp(0, 100)
        } else {
            0
        }
    };

    let passed = !overtime && score >= passing;
    let final_status = if overtime {
        "expired"
    } else if passed {
        "passed"
    } else {
        "failed"
    };

    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE certification_attempts SET status = $1, score = $2, completed_at = NOW() WHERE id = $3",
    )
    .bind(final_status)
    .bind(score)
    .bind(attempt_id)
    .execute(&mut *tx)
    .await?;

    let mut diploma_id: Option<Uuid> = None;
    let mut verification_code: Option<String> = None;
    if passed {
        let code = generate_verification_code(&mut tx).await?;
        let expires_at = chrono::Utc::now() + chrono::Duration::days(validity_months as i64 * 30);
        let inserted: (Uuid,) = sqlx::query_as(
            r#"
            INSERT INTO certification_diplomas
                (attempt_id, user_id, certification_id, verification_code, expires_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#,
        )
        .bind(attempt_id)
        .bind(auth.user_id)
        .bind(cert_id)
        .bind(&code)
        .bind(expires_at)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("UPDATE certification_attempts SET diploma_id = $1 WHERE id = $2")
            .bind(inserted.0)
            .bind(attempt_id)
            .execute(&mut *tx)
            .await?;
        diploma_id = Some(inserted.0);
        verification_code = Some(code);
    }

    tx.commit().await?;

    metrics::counter!(
        "skilluv_certification_attempts_total",
        "status" => final_status
    )
    .increment(1);

    Ok(Json(build_response(json!({
        "attempt_id": attempt_id,
        "status": final_status,
        "score": score,
        "passing_score": passing,
        "passed": passed,
        "overtime": overtime,
        "certification_title": cert_title,
        "diploma_id": diploma_id,
        "verification_code": verification_code,
    }))))
}

async fn generate_verification_code(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<String, AppError> {
    // Base32 Crockford (sans I, L, O, U). Chaque tentative dérive 8 chars du
    // hash SHA-256 d'un UUID v4, ce qui donne ~40 bits d'entropie utile.
    use sha2::{Digest, Sha256};
    const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTVWXYZ23456789";
    for _ in 0..20 {
        let uuid = Uuid::new_v4();
        let mut h = Sha256::new();
        h.update(uuid.as_bytes());
        let digest = h.finalize();
        let code: String = digest[..8]
            .iter()
            .map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char)
            .collect();
        let exists: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM certification_diplomas WHERE verification_code = $1")
                .bind(&code)
                .fetch_optional(&mut **tx)
                .await?;
        if exists.is_none() {
            return Ok(code);
        }
    }
    Err(AppError::Internal(
        "could not generate unique verification code".into(),
    ))
}

// ─── Vérification publique du diplôme ────────────────────────────

async fn verify_diploma(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Json<Value>, AppError> {
    let code = code.trim().to_uppercase();
    let row = sqlx::query(
        r#"
        SELECT d.issued_at, d.expires_at, d.revoked_at, d.revoke_reason,
               d.certification_id,
               u.username, u.display_name,
               c.title AS cert_title, c.skill_domain, c.level
        FROM certification_diplomas d
        JOIN users u ON u.id = d.user_id
        JOIN certifications c ON c.id = d.certification_id
        WHERE d.verification_code = $1
        "#,
    )
    .bind(&code)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("diploma not found".into()))?;

    let revoked_at: Option<chrono::DateTime<chrono::Utc>> = row.get("revoked_at");
    let expires_at: chrono::DateTime<chrono::Utc> = row.get("expires_at");
    let now = chrono::Utc::now();
    let status = if revoked_at.is_some() {
        "revoked"
    } else if expires_at < now {
        "expired"
    } else {
        "valid"
    };

    Ok(Json(build_response(json!({
        "verification_code": code,
        "status": status,
        "holder": {
            "username": row.get::<String, _>("username"),
            "display_name": row.get::<String, _>("display_name"),
        },
        "certification": {
            "title": row.get::<String, _>("cert_title"),
            "skill_domain": row.get::<String, _>("skill_domain"),
            "level": row.get::<String, _>("level"),
        },
        "issued_at": row.get::<chrono::DateTime<chrono::Utc>, _>("issued_at"),
        "expires_at": expires_at,
        "revoked_at": revoked_at,
        "revoke_reason": row.get::<Option<String>, _>("revoke_reason"),
    }))))
}

async fn my_diplomas(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT d.id, d.verification_code, d.issued_at, d.expires_at, d.revoked_at,
               c.title, c.skill_domain, c.level
        FROM certification_diplomas d
        JOIN certifications c ON c.id = d.certification_id
        WHERE d.user_id = $1
        ORDER BY d.issued_at DESC
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let revoked: Option<chrono::DateTime<chrono::Utc>> = r.get("revoked_at");
            let expires: chrono::DateTime<chrono::Utc> = r.get("expires_at");
            let status = if revoked.is_some() {
                "revoked"
            } else if expires < chrono::Utc::now() {
                "expired"
            } else {
                "valid"
            };
            json!({
                "diploma_id": r.get::<Uuid, _>("id"),
                "verification_code": r.get::<String, _>("verification_code"),
                "issued_at": r.get::<chrono::DateTime<chrono::Utc>, _>("issued_at"),
                "expires_at": expires,
                "status": status,
                "certification": {
                    "title": r.get::<String, _>("title"),
                    "skill_domain": r.get::<String, _>("skill_domain"),
                    "level": r.get::<String, _>("level"),
                }
            })
        })
        .collect();
    Ok(Json(build_response(json!({ "diplomas": items }))))
}
