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
use serde::Serialize;
use sqlx::Row;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
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

// ─── Types de réponse ────────────────────────────────────────────

#[derive(Debug, Serialize, ToSchema)]
pub struct CertificationRow {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub skill_domain: String,
    /// `foundation`, `expert`, `master`.
    pub level: String,
    pub price_eur_cents: i64,
    pub duration_minutes: i32,
    pub passing_score: i32,
    pub validity_months: i32,
    pub challenges_count: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CertificationsListResponse {
    pub certifications: Vec<CertificationRow>,
}

/// Response for `POST /certifications/{slug}/purchase`. When an
/// in-progress attempt already exists, `checkout_url` is `None` and
/// `message` explains the situation.
#[derive(Debug, Serialize, ToSchema)]
pub struct PurchaseResponse {
    pub attempt_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkout_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StartAttemptResponse {
    pub attempt_id: Uuid,
    pub challenge_ids: Vec<Uuid>,
    pub duration_minutes: i32,
    /// Absolute deadline (RFC 3339). Client-side timer starts from now.
    pub deadline: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SubmitAttemptResponse {
    pub attempt_id: Uuid,
    /// `passed`, `failed`, `expired`.
    pub status: String,
    pub score: i32,
    pub passing_score: i32,
    pub passed: bool,
    pub overtime: bool,
    pub certification_title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diploma_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_code: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DiplomaHolder {
    pub username: String,
    pub display_name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DiplomaCertification {
    pub title: String,
    pub skill_domain: String,
    pub level: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VerifyDiplomaResponse {
    pub verification_code: String,
    /// `valid`, `expired`, `revoked`.
    pub status: String,
    pub holder: DiplomaHolder,
    pub certification: DiplomaCertification,
    pub issued_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub revoke_reason: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MyDiplomaRow {
    pub diploma_id: Uuid,
    pub verification_code: String,
    pub issued_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// `valid`, `expired`, `revoked`.
    pub status: String,
    pub certification: DiplomaCertification,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MyDiplomasResponse {
    pub diplomas: Vec<MyDiplomaRow>,
}

// ─── Catalogue ───────────────────────────────────────────────────

/// Public catalog of active certifications.
#[utoipa::path(
    get,
    path = "/api/certifications",
    tag = "challenges",
    responses(
        (status = 200, description = "Active certifications", body = ApiResponse<CertificationsListResponse>),
    ),
)]
pub async fn list_certifications(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<CertificationsListResponse>>, AppError> {
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
    let items: Vec<CertificationRow> = rows
        .iter()
        .map(|r| CertificationRow {
            id: r.get("id"),
            slug: r.get("slug"),
            title: r.get("title"),
            description: r.get("description"),
            skill_domain: r.get("skill_domain"),
            level: r.get("level"),
            price_eur_cents: r.get("price_eur_cents"),
            duration_minutes: r.get("duration_minutes"),
            passing_score: r.get("passing_score"),
            validity_months: r.get("validity_months"),
            challenges_count: r.get::<Option<i32>, _>("challenges_count").unwrap_or(0),
        })
        .collect();
    Ok(Json(ApiResponse::new(CertificationsListResponse {
        certifications: items,
    })))
}

// ─── Achat (Stripe direct) ───────────────────────────────────────

/// Buy a certification: creates a Stripe checkout session. If the
/// caller already has an in-progress attempt (`pending` / `paid` /
/// `started`), the existing attempt is echoed back rather than a new
/// one created — front should surface the resume flow.
#[utoipa::path(
    post,
    path = "/api/certifications/{slug}/purchase",
    tag = "wallet",
    params(("slug" = String, Path, description = "Certification slug")),
    responses(
        (status = 200, description = "Checkout session created OR existing attempt echoed", body = ApiResponse<PurchaseResponse>),
        (status = 400, description = "Certification exists but is inactive", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Certification / user not found", body = crate::api_response::ErrorResponse),
        (status = 500, description = "Stripe not configured", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn purchase_certification(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<PurchaseResponse>>, AppError> {
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
        return Ok(Json(ApiResponse::new(PurchaseResponse {
            attempt_id: existing_id,
            status: Some(status),
            message: Some("existing attempt already in progress".to_string()),
            checkout_url: None,
            session_id: None,
        })));
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

    // Two `Box::leak`ed strings per purchase, to build a synthetic "pack"
    // for a helper shaped around credit packs. Both leaked for the life of
    // the process, and neither reached anyone paying by Mobile Money.
    let payer: (String, String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT u.email, u.display_name, u.country_iso2, w.momo_phone
           FROM users u
           LEFT JOIN talent_wallets w ON w.user_id = u.id
          WHERE u.id = $1",
    )
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("user not found".into()))?;
    let (email, display_name, country, phone) = payer;

    let currency = crate::services::ledger::Currency::Eur;
    let method = if phone.is_some() && currency == crate::services::ledger::Currency::Xof {
        crate::services::collect::Method::MobileMoney
    } else {
        crate::services::collect::Method::Card
    };

    let registry = crate::services::collect_adapters::registry_from_env();
    let provider = registry
        .resolve(&state.db, country.as_deref(), currency, method)
        .await?;

    let amount = bigdecimal::BigDecimal::from(price_cents) / bigdecimal::BigDecimal::from(100);
    let base = state.config.frontend_url.trim_end_matches('/').to_string();
    let success_url = format!("{base}/certifications/{slug}?paid=1");
    let cancel_url = format!("{base}/certifications/{slug}?canceled=1");
    let idempotency_key = format!("certification_purchase:{}", attempt.0);
    let description = format!("Skilluv — {title}");

    let session = crate::services::collect::start(
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
            subject_type: "certification_purchase",
            subject_id: attempt.0,
            amount: &amount,
            currency,
            description: &description,
            success_url: &success_url,
            cancel_url: &cancel_url,
            idempotency_key: &idempotency_key,
            operator: None,
            credits: None,
            merchant_reference: None,
        },
    )
    .await?;

    Ok(Json(ApiResponse::new(PurchaseResponse {
        attempt_id: attempt.0,
        status: None,
        message: None,
        checkout_url: Some(session.redirect_url),
        session_id: Some(session.session_id),
    })))
}

// ─── Démarrer la tentative (après paiement) ─────────────────────

/// Start the timer on a paid attempt. Returns the challenge list and
/// the absolute deadline for the client-side timer.
#[utoipa::path(
    post,
    path = "/api/certifications/attempts/{id}/start",
    tag = "challenges",
    params(("id" = Uuid, Path, description = "Attempt UUID")),
    responses(
        (status = 200, description = "Timer started", body = ApiResponse<StartAttemptResponse>),
        (status = 400, description = "Attempt not in state 'paid'", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Attempt not found", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn start_attempt(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(attempt_id): Path<Uuid>,
) -> Result<Json<ApiResponse<StartAttemptResponse>>, AppError> {
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

    Ok(Json(ApiResponse::new(StartAttemptResponse {
        attempt_id,
        challenge_ids,
        duration_minutes: duration,
        deadline: chrono::Utc::now() + chrono::Duration::minutes(duration as i64),
    })))
}

// ─── Soumission finale + score ───────────────────────────────────

/// Submit an attempt: server-side recompute of the score from
/// `challenge_submissions` filed after `started_at`. Passing threshold
/// per certification; overtime beyond `duration_minutes + 2` yields
/// status `expired`. Passing issues a diploma with a fresh
/// verification_code.
#[utoipa::path(
    post,
    path = "/api/certifications/attempts/{id}/submit",
    tag = "challenges",
    params(("id" = Uuid, Path, description = "Attempt UUID")),
    responses(
        (status = 200, description = "Attempt scored", body = ApiResponse<SubmitAttemptResponse>),
        (status = 400, description = "Attempt not in state 'started'", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Attempt not found", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn submit_attempt(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(attempt_id): Path<Uuid>,
) -> Result<Json<ApiResponse<SubmitAttemptResponse>>, AppError> {
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

    Ok(Json(ApiResponse::new(SubmitAttemptResponse {
        attempt_id,
        status: final_status.to_string(),
        score,
        passing_score: passing,
        passed,
        overtime,
        certification_title: cert_title,
        diploma_id,
        verification_code,
    })))
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

/// Public diploma verification — no auth required. Any third party
/// with a verification_code can check the diploma's validity.
#[utoipa::path(
    get,
    path = "/api/diplomas/verify/{code}",
    tag = "profile",
    params(("code" = String, Path, description = "Verification code")),
    responses(
        (status = 200, description = "Diploma detail", body = ApiResponse<VerifyDiplomaResponse>),
        (status = 404, description = "Diploma not found", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn verify_diploma(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Json<ApiResponse<VerifyDiplomaResponse>>, AppError> {
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

    Ok(Json(ApiResponse::new(VerifyDiplomaResponse {
        verification_code: code,
        status: status.to_string(),
        holder: DiplomaHolder {
            username: row.get("username"),
            display_name: row.get("display_name"),
        },
        certification: DiplomaCertification {
            title: row.get("cert_title"),
            skill_domain: row.get("skill_domain"),
            level: row.get("level"),
        },
        issued_at: row.get("issued_at"),
        expires_at,
        revoked_at,
        revoke_reason: row.get("revoke_reason"),
    })))
}

/// List the caller's diplomas — ordered newest first.
#[utoipa::path(
    get,
    path = "/api/diplomas/my",
    tag = "profile",
    responses(
        (status = 200, description = "Caller's diplomas", body = ApiResponse<MyDiplomasResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_diplomas(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<MyDiplomasResponse>>, AppError> {
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
    let items: Vec<MyDiplomaRow> = rows
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
            MyDiplomaRow {
                diploma_id: r.get("id"),
                verification_code: r.get("verification_code"),
                issued_at: r.get("issued_at"),
                expires_at: expires,
                status: status.to_string(),
                certification: DiplomaCertification {
                    title: r.get("title"),
                    skill_domain: r.get("skill_domain"),
                    level: r.get("level"),
                },
            }
        })
        .collect();
    Ok(Json(ApiResponse::new(MyDiplomasResponse {
        diplomas: items,
    })))
}
