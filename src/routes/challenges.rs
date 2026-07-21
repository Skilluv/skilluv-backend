use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{AuthUser, AuthUserComplete, OptionalAuth};
use crate::models::{Badge, ChallengeSubmission, ChallengeTemplate};
use crate::services::LeaderboardService;
use crate::websocket::WsMessage;

pub fn challenge_routes() -> Router<AppState> {
    Router::new()
        .route("/challenges/onboarding", get(get_onboarding))
        .route("/challenges", get(list_challenges))
        .route("/challenges/{id}", get(get_challenge))
        .route("/challenges/{id}/start", post(start_challenge))
        .route("/challenges/{id}/submit", post(submit_challenge))
        .route("/challenges/{id}/submissions", get(my_submissions))
}

#[derive(Debug, Deserialize)]
struct OnboardingQuery {
    domain: String,
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    domain: Option<String>,
    difficulty: Option<i16>,
    page: Option<i64>,
    per_page: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SubmitRequest {
    code: String,
    language: Option<String>,
}

fn build_response(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

// GET /api/challenges/onboarding?domain=code
async fn get_onboarding(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<OnboardingQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Check if user already completed onboarding
    let user: crate::models::User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    if user.profile_active {
        return Err(AppError::Validation(
            "You have already completed onboarding".to_string(),
        ));
    }

    let challenge: ChallengeTemplate = sqlx::query_as(
        "SELECT * FROM challenge_templates WHERE is_onboarding = TRUE AND skill_domain = $1 AND status = 'published' LIMIT 1",
    )
    .bind(&query.domain)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound(format!(
        "No onboarding challenge found for domain: {}",
        query.domain
    )))?;

    Ok(Json(build_response(json!({ "challenge": challenge }))))
}

// GET /api/challenges (public — optional auth for locked/unlocked status)
async fn list_challenges(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    tenant: crate::middleware::TenantContext,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 50);
    let offset = (page - 1) * per_page;

    // Get user's total fragments for prerequisite check (if authenticated)
    let user_fragments = if let Some(ref auth) = auth {
        let user: Option<crate::models::User> = sqlx::query_as("SELECT * FROM users WHERE id = $1")
            .bind(auth.user_id)
            .fetch_optional(&state.db)
            .await?;
        user.map(|u| u.total_fragments).unwrap_or(0)
    } else {
        0
    };

    // Phase 5.9 : isolation tenant. Sur le tenant racine (`skilluv`), on montre
    // les challenges publics (tenant_id NULL) + ceux du tenant racine. Sur un
    // sous-tenant, on montre publics + ses propres challenges.
    let tenant_clause = if crate::routes::is_root_tenant(tenant.tenant_id) {
        " AND (tenant_id IS NULL OR tenant_id = '00000000-0000-0000-0000-000000000001'::uuid)"
    } else {
        " AND (tenant_id IS NULL OR tenant_id = $__TENANT__)"
    };
    let mut sql = format!(
        "SELECT * FROM challenge_templates WHERE status = 'published' AND is_onboarding = FALSE{tenant_clause}"
    );
    let mut count_sql = format!(
        "SELECT COUNT(*) FROM challenge_templates WHERE status = 'published' AND is_onboarding = FALSE{tenant_clause}"
    );
    // Le placeholder $__TENANT__ sera remplacé plus bas selon param_idx.
    let has_tenant_bind = !crate::routes::is_root_tenant(tenant.tenant_id);
    let mut param_idx = 0u32;
    let mut binds_domain: Option<String> = None;
    let mut binds_difficulty: Option<i16> = None;

    if has_tenant_bind {
        param_idx += 1;
        sql = sql.replace("$__TENANT__", &format!("${param_idx}"));
        count_sql = count_sql.replace("$__TENANT__", &format!("${param_idx}"));
    }

    if let Some(ref domain) = query.domain {
        param_idx += 1;
        let clause = format!(" AND skill_domain = ${param_idx}");
        sql.push_str(&clause);
        count_sql.push_str(&clause);
        binds_domain = Some(domain.clone());
    }

    if let Some(difficulty) = query.difficulty {
        param_idx += 1;
        let clause = format!(" AND difficulty = ${param_idx}");
        sql.push_str(&clause);
        count_sql.push_str(&clause);
        binds_difficulty = Some(difficulty);
    }

    sql.push_str(&format!(
        " ORDER BY difficulty ASC, created_at DESC LIMIT {} OFFSET {}",
        per_page, offset
    ));

    // Build queries dynamically
    let mut challenges_query = sqlx::query_as::<_, ChallengeTemplate>(&sql);
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);

    if has_tenant_bind {
        challenges_query = challenges_query.bind(tenant.tenant_id);
        count_query = count_query.bind(tenant.tenant_id);
    }

    if let Some(ref d) = binds_domain {
        challenges_query = challenges_query.bind(d);
        count_query = count_query.bind(d);
    }
    if let Some(diff) = binds_difficulty {
        challenges_query = challenges_query.bind(diff);
        count_query = count_query.bind(diff);
    }

    let challenges: Vec<ChallengeTemplate> = challenges_query.fetch_all(&state.db).await?;
    let total: i64 = count_query.fetch_one(&state.db).await?;

    // P8.3 : le flag `locked` est retiré du listing. La progression suit
    // désormais le DAG via GET /api/challenges/{id}/eligibility qui donne
    // une réponse détaillée (missing_required, missing_recommended, reason).
    // Le frontend interroge cet endpoint quand il a besoin du gate status.
    let _ = user_fragments; // conservé pour compat future (personnalisation feed)
    let challenges_with_status: Vec<serde_json::Value> = challenges
        .into_iter()
        .map(|c| json!({ "challenge": c }))
        .collect();

    Ok(Json(json!({
        "data": challenges_with_status,
        "pagination": {
            "page": page,
            "per_page": per_page,
            "total": total,
            "total_pages": (total as f64 / per_page as f64).ceil() as i64,
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

// GET /api/challenges/:id (public)
async fn get_challenge(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let challenge: ChallengeTemplate =
        sqlx::query_as("SELECT * FROM challenge_templates WHERE id = $1 AND status = 'published'")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::NotFound("Challenge not found".to_string()))?;

    Ok(Json(build_response(json!({ "challenge": challenge }))))
}

// POST /api/challenges/:id/start
async fn start_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(challenge_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let challenge: ChallengeTemplate =
        sqlx::query_as("SELECT * FROM challenge_templates WHERE id = $1 AND status = 'published'")
            .bind(challenge_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::NotFound("Challenge not found".to_string()))?;

    // Check prerequisites
    let user: crate::models::User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    // P8.3 : le check des prérequis est désormais 100% DAG (via
    // challenge_prerequisites + deliverables verified). La colonne
    // prerequisite_fragments a été supprimée. Un challenge sans entrée DAG
    // n'a aucun prérequis à vérifier — il est démarrable par tout user
    // profile_active.
    let _ = &user; // conservé pour compat future (rate limiting, etc.)
    let eligibility =
        crate::services::TracksService::check_eligibility(&state.db, auth.user_id, challenge_id)
            .await?;
    if !eligibility.eligible {
        return Err(AppError::ChallengePrerequisiteNotMet);
    }

    // Check for existing in-progress submission
    let existing: Option<ChallengeSubmission> = sqlx::query_as(
        "SELECT * FROM challenge_submissions WHERE user_id = $1 AND challenge_id = $2 AND status = 'in_progress'",
    )
    .bind(auth.user_id)
    .bind(challenge_id)
    .fetch_optional(&state.db)
    .await?;

    if let Some(submission) = existing {
        return Ok((
            StatusCode::OK,
            Json(build_response(json!({
                "submission": submission,
                "challenge": challenge,
                "message": "Resuming existing attempt"
            }))),
        ));
    }

    // Count previous attempts
    let attempt_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM challenge_submissions WHERE user_id = $1 AND challenge_id = $2",
    )
    .bind(auth.user_id)
    .bind(challenge_id)
    .fetch_one(&state.db)
    .await?;

    // Set timer if challenge has duration_minutes
    let expires_at = challenge
        .duration_minutes
        .map(|minutes| chrono::Utc::now() + chrono::Duration::minutes(minutes as i64));

    let submission: ChallengeSubmission = sqlx::query_as(
        r#"
        INSERT INTO challenge_submissions (challenge_id, user_id, attempt_number, expires_at)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(challenge_id)
    .bind(auth.user_id)
    .bind((attempt_count + 1) as i32)
    .bind(expires_at)
    .fetch_one(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({
            "submission": submission,
            "challenge": challenge,
        }))),
    ))
}

/// Deadline de retrait de l'endpoint legacy `/challenges/{id}/submit`.
///
/// La date suit la décision Q5 (session 2026-07-09) : les endpoints legacy
/// restent actifs jusqu'au 31 décembre 2027. À l'échéance, la route est
/// retirée et remplacée entièrement par `POST /deliverables` +
/// `POST /webhooks/github/slices/{project_id}`.
const SUBMIT_SUNSET_DATE: &str = "Fri, 31 Dec 2027 23:59:59 GMT";

/// Headers HTTP standards signalant la deprecation d'un endpoint.
///
/// Utilise les standards :
/// - RFC 8594 `Sunset` — date de retrait effective
/// - `Deprecation: true` — draft IETF, largement supporté par les proxies/API gateways
/// - RFC 8288 `Link` avec rel="successor-version" — pointe vers le remplaçant
fn submit_deprecation_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("Deprecation", HeaderValue::from_static("true"));
    headers.insert("Sunset", HeaderValue::from_static(SUBMIT_SUNSET_DATE));
    headers.insert(
        "Link",
        HeaderValue::from_static(
            "</deliverables>; rel=\"successor-version\", \
             </webhooks/github/slices/{project_id}>; rel=\"alternate\"",
        ),
    );
    headers
}

// POST /api/challenges/:id/submit
async fn submit_challenge(
    State(state): State<AppState>,
    auth: AuthUserComplete,
    Path(challenge_id): Path<Uuid>,
    Json(body): Json<SubmitRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Find the in-progress submission
    let submission: ChallengeSubmission = sqlx::query_as(
        "SELECT * FROM challenge_submissions WHERE user_id = $1 AND challenge_id = $2 AND status = 'in_progress'",
    )
    .bind(auth.user_id)
    .bind(challenge_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::Validation(
        "No in-progress submission found. Start the challenge first.".to_string(),
    ))?;

    let challenge: ChallengeTemplate =
        sqlx::query_as("SELECT * FROM challenge_templates WHERE id = $1")
            .bind(challenge_id)
            .fetch_one(&state.db)
            .await?;

    // Check timer expiration
    if let Some(expires_at) = submission.expires_at {
        if chrono::Utc::now() > expires_at {
            // Mark as expired
            sqlx::query(
                "UPDATE challenge_submissions SET status = 'failure', submitted_at = NOW(), evaluated_at = NOW() WHERE id = $1",
            )
            .bind(submission.id)
            .execute(&state.db)
            .await?;

            return Ok((
                submit_deprecation_headers(),
                Json(build_response(json!({
                    "submission": { "id": submission.id, "status": "failure" },
                    "fragments_earned": 0,
                    "message": "Time expired. Submission rejected."
                }))),
            ));
        }
    }

    // Evaluate submission
    let (eval_status, fragments_earned, exec_stdout, exec_stderr) =
        evaluate_submission(&state, &challenge, &body.code, body.language.as_deref()).await?;

    // Count previous failures for perseverance bonus
    let prev_failures: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM challenge_submissions WHERE user_id = $1 AND challenge_id = $2 AND status = 'failure'",
    )
    .bind(auth.user_id)
    .bind(challenge_id)
    .fetch_one(&state.db)
    .await?;

    let perseverance_bonus = if eval_status == "success" && prev_failures > 0 {
        (prev_failures as i32 * 2).min(challenge.reward_fragments / 2)
    } else {
        0
    };

    let total_fragments = fragments_earned + perseverance_bonus;

    // P9.1 : code/stdout/stderr sont désormais persistés dans le deliverable
    // (artifact_metadata) via create_from_challenge_submission ci-dessous.
    // La submission garde uniquement la trace de progression.
    let updated_submission: ChallengeSubmission = sqlx::query_as(
        r#"
        UPDATE challenge_submissions
        SET status = $1, language = $2, fragments_earned = $3,
            submitted_at = NOW(), evaluated_at = NOW()
        WHERE id = $4
        RETURNING *
        "#,
    )
    .bind(&eval_status)
    .bind(&body.language)
    .bind(total_fragments)
    .bind(submission.id)
    .fetch_one(&state.db)
    .await?;

    if eval_status == "success" {
        metrics::counter!(
            "skilluv_challenges_completed_total",
            "domain" => challenge.skill_domain.clone()
        )
        .increment(1);

        // P8.5a : dual-write vers la nouvelle table `deliverables`. Best-effort :
        // si l'INSERT échoue (ex: contrainte, DB blip), on log et on continue —
        // le pipeline legacy reste la source de vérité pour l'instant. En P8.7
        // le legacy sera droppé et deliverables deviendra unique source.
        match crate::services::DeliverablesService::create_from_challenge_submission(
            &state.db,
            auth.user_id,
            challenge.id,
            updated_submission.id,
            &body.code,
            total_fragments,
            body.language.as_deref(),
            exec_stdout.as_deref(),
            exec_stderr.as_deref(),
        )
        .await
        {
            Ok(_deliverable_id) => {
                metrics::counter!(
                    "skilluv_challenge_deliverables_created_total",
                    "domain" => challenge.skill_domain.clone()
                )
                .increment(1);
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    submission_id = %updated_submission.id,
                    user_id = %auth.user_id,
                    challenge_id = %challenge.id,
                    "P8.5a dual-write deliverable failed (best-effort, submission still succeeded)"
                );
            }
        }
    }
    metrics::counter!(
        "skilluv_fragments_awarded_total",
        "domain" => challenge.skill_domain.clone()
    )
    .increment(total_fragments as u64);

    // Guild GP (Phase 2 Sprint 4) — 10% of awarded fragments goes to the user's guild.
    if total_fragments > 0 {
        if let Ok(gp_added) =
            crate::services::guild::award_gp_for_fragments(&state.db, auth.user_id, total_fragments)
                .await
        {
            if gp_added > 0 {
                metrics::counter!("skilluv_gp_awarded_total").increment(gp_added as u64);
            }
        }
    }

    // Award fragments to user
    if total_fragments > 0 {
        // Update total_fragments on user
        sqlx::query(
            "UPDATE users SET total_fragments = total_fragments + $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(total_fragments)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

        // P8.7 : skill_fragments legacy retiré. Propagation user_skills seule.
        // P8.5c : best-effort vers user_skills quand un skill_node matche la
        // langue du challenge. Ne fail pas le submit si absent.
        match crate::services::SkillsService::propagate_legacy_challenge_success_to_user_skills(
            &state.db,
            auth.user_id,
            challenge.language.as_deref(),
            &challenge.skill_domain,
            total_fragments,
        )
        .await
        {
            Ok(Some(_)) => {
                metrics::counter!(
                    "skilluv_challenge_user_skills_propagated_total",
                    "domain" => challenge.skill_domain.clone()
                )
                .increment(1);
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    user_id = %auth.user_id,
                    challenge_id = %challenge.id,
                    "P8.5c user_skills propagation failed (best-effort, submission still succeeded)"
                );
            }
        }

        // Update title based on total fragments
        update_user_title(&state, auth.user_id).await?;

        // Update leaderboards in Redis
        let updated_user: crate::models::User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
            .bind(auth.user_id)
            .fetch_one(&state.db)
            .await?;

        LeaderboardService::update_score(
            &mut state.redis.clone(),
            &state.db,
            auth.user_id,
            updated_user.total_fragments,
            &challenge.skill_domain,
            total_fragments,
        )
        .await?;
    }

    // Activate profile on first successful completion (onboarding)
    let mut profile_just_activated = false;
    if eval_status == "success" {
        let user: crate::models::User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
            .bind(auth.user_id)
            .fetch_one(&state.db)
            .await?;

        if !user.profile_active {
            sqlx::query("UPDATE users SET profile_active = TRUE, updated_at = NOW() WHERE id = $1")
                .bind(auth.user_id)
                .execute(&state.db)
                .await?;
            profile_just_activated = true;
        }

        // Update streak (may award bonus fragments)
        let streak_bonus = update_streak(&state, auth.user_id).await?;

        // If streak awarded bonus fragments, update leaderboards too
        if streak_bonus > 0 {
            let post_streak_user: crate::models::User =
                sqlx::query_as("SELECT * FROM users WHERE id = $1")
                    .bind(auth.user_id)
                    .fetch_one(&state.db)
                    .await?;

            LeaderboardService::update_score(
                &mut state.redis.clone(),
                &state.db,
                auth.user_id,
                post_streak_user.total_fragments,
                &challenge.skill_domain,
                streak_bonus,
            )
            .await?;
        }

        // Log activity for heatmap
        log_activity(&state, auth.user_id, total_fragments).await?;

        // Check and award badges
        let newly_earned = check_and_award_badges(&state, auth.user_id).await?;
        if !newly_earned.is_empty() {
            state
                .ws
                .send_to_user(
                    auth.user_id,
                    WsMessage {
                        event: "badge.earned".to_string(),
                        room: None,
                        payload: serde_json::json!({
                            "badges": newly_earned.iter().map(|b| serde_json::json!({
                                "slug": b.slug,
                                "name": b.name,
                                "icon": b.icon,
                            })).collect::<Vec<_>>(),
                        }),
                    },
                )
                .await;
        }
    }

    // Fetch updated user
    let user: crate::models::User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    let mut response = json!({
        "submission": updated_submission,
        "fragments_earned": total_fragments,
        "perseverance_bonus": perseverance_bonus,
        "user": {
            "total_fragments": user.total_fragments,
            "title": user.title,
            "golden_stars": user.golden_stars,
            "streak_current": user.streak_current,
            "profile_active": user.profile_active,
        },
    });

    if profile_just_activated {
        response["profile_activated"] = json!(true);
        response["message"] = json!(format!(
            "Bienvenue, {} ! Ton profil est maintenant actif.",
            user.display_name
        ));
    }

    // Broadcast leaderboard update via WebSocket
    if eval_status == "success" {
        let ws_payload = json!({
            "user_id": auth.user_id,
            "display_name": user.display_name,
            "total_fragments": user.total_fragments,
            "title": user.title,
            "challenge_title": challenge.title,
        });

        // Broadcast to domain leaderboard room
        let leaderboard_room = format!("leaderboard:{}", challenge.skill_domain);
        state
            .ws
            .broadcast_to_room(
                &leaderboard_room,
                WsMessage {
                    event: "leaderboard.updated".to_string(),
                    room: Some(leaderboard_room.clone()),
                    payload: ws_payload.clone(),
                },
            )
            .await;

        // Broadcast to challenge room (if others are watching)
        let challenge_room = format!("challenge:{challenge_id}");
        state
            .ws
            .broadcast_to_room(
                &challenge_room,
                WsMessage {
                    event: "challenge.submission".to_string(),
                    room: Some(challenge_room.clone()),
                    payload: json!({
                        "user_id": auth.user_id,
                        "display_name": user.display_name,
                        "status": eval_status,
                        "fragments_earned": total_fragments,
                    }),
                },
            )
            .await;

        // Notify user personally (all their connections)
        state
            .ws
            .send_to_user(
                auth.user_id,
                WsMessage {
                    event: "fragment.earned".to_string(),
                    room: None,
                    payload: json!({
                        "fragments_earned": total_fragments,
                        "total_fragments": user.total_fragments,
                        "title": user.title,
                        "golden_stars": user.golden_stars,
                    }),
                },
            )
            .await;
    }

    Ok((submit_deprecation_headers(), Json(build_response(response))))
}

// GET /api/challenges/:id/submissions
async fn my_submissions(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(challenge_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let submissions: Vec<ChallengeSubmission> = sqlx::query_as(
        "SELECT * FROM challenge_submissions WHERE user_id = $1 AND challenge_id = $2 ORDER BY started_at DESC",
    )
    .bind(auth.user_id)
    .bind(challenge_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "submissions": submissions }))))
}

// ─── Evaluation engine ───────────────────────────────────────────

/// Returns (status, fragments_earned, stdout, stderr)
async fn evaluate_submission(
    state: &AppState,
    challenge: &ChallengeTemplate,
    code: &str,
    language: Option<&str>,
) -> Result<(String, i32, Option<String>, Option<String>), AppError> {
    if code.trim().is_empty() {
        return Ok((
            "failure".to_string(),
            failure_fragments(challenge),
            None,
            Some("Empty submission".to_string()),
        ));
    }

    // For code domain: use Judge0 for real execution
    if challenge.skill_domain == "code" {
        let lang = language.unwrap_or("python");

        let result = state
            .sandbox
            .execute(
                code,
                lang,
                None,
                challenge.expected_output.as_deref(),
                None,
                None,
            )
            .await;

        match result {
            Ok(exec) => {
                let stdout = exec.stdout.clone();
                let stderr = exec.stderr.clone().or(exec.compile_output.clone());

                // Judge0 status 3 = Accepted
                if exec.status.id == 3 {
                    return Ok((
                        "success".to_string(),
                        challenge.reward_fragments,
                        stdout,
                        stderr,
                    ));
                }

                // For onboarding: also check stdout content
                if challenge.is_onboarding {
                    if let Some(ref out) = stdout {
                        if out.trim().contains("Hello, Skilluv!") {
                            return Ok((
                                "success".to_string(),
                                challenge.reward_fragments,
                                stdout,
                                stderr,
                            ));
                        }
                    }
                }

                Ok((
                    "failure".to_string(),
                    failure_fragments(challenge),
                    stdout,
                    stderr,
                ))
            }
            Err(_) => evaluate_basic(challenge, code),
        }
    } else {
        evaluate_basic(challenge, code)
    }
}

/// Fallback when Judge0 is unavailable or for non-code domains
fn evaluate_basic(
    challenge: &ChallengeTemplate,
    code: &str,
) -> Result<(String, i32, Option<String>, Option<String>), AppError> {
    if challenge.is_onboarding && challenge.skill_domain == "code" {
        if code.contains("Hello, Skilluv!") {
            return Ok((
                "success".to_string(),
                challenge.reward_fragments,
                None,
                None,
            ));
        }
        return Ok((
            "failure".to_string(),
            failure_fragments(challenge),
            None,
            None,
        ));
    }

    if challenge.skill_domain != "code" {
        if code.len() >= 100 {
            return Ok((
                "success".to_string(),
                challenge.reward_fragments,
                None,
                None,
            ));
        }
        return Ok((
            "failure".to_string(),
            failure_fragments(challenge),
            None,
            None,
        ));
    }

    if let Some(ref expected) = challenge.expected_output {
        if code.contains(expected.trim()) {
            return Ok((
                "success".to_string(),
                challenge.reward_fragments,
                None,
                None,
            ));
        }
        return Ok((
            "failure".to_string(),
            failure_fragments(challenge),
            None,
            None,
        ));
    }

    Ok((
        "success".to_string(),
        challenge.reward_fragments,
        None,
        None,
    ))
}

fn failure_fragments(challenge: &ChallengeTemplate) -> i32 {
    (challenge.reward_fragments / 5).max(1)
}

// ─── Helpers ─────────────────────────────────────────────────────

async fn update_user_title(state: &AppState, user_id: Uuid) -> Result<(), AppError> {
    let total: i32 = sqlx::query_scalar("SELECT total_fragments FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;

    let (title, golden_stars) = match total {
        0..=499 => ("apprenti", 0),
        500..=1999 => ("artisan", 0),
        2000..=4999 => ("maitre", 0),
        _ => ("legende", (total - 5000) / 100),
    };

    sqlx::query("UPDATE users SET title = $1, golden_stars = $2, updated_at = NOW() WHERE id = $3")
        .bind(title)
        .bind(golden_stars)
        .bind(user_id)
        .execute(&state.db)
        .await?;

    Ok(())
}

/// Returns the streak bonus fragments awarded (0 if none).
async fn update_streak(state: &AppState, user_id: Uuid) -> Result<i32, AppError> {
    let today = chrono::Utc::now().date_naive();

    let user: crate::models::User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;

    let new_streak = match user.streak_last_activity {
        Some(last) if last == today => {
            // Already active today
            return Ok(0);
        }
        Some(last) if last == today - chrono::Duration::days(1) => {
            // Consecutive day
            user.streak_current + 1
        }
        _ => {
            // Streak broken or first activity
            1
        }
    };

    // Streak bonus fragments at milestones
    let streak_bonus = match new_streak {
        7 => 25,
        30 => 100,
        100 => 500,
        365 => 2000,
        _ => 0,
    };

    if streak_bonus > 0 {
        sqlx::query("UPDATE users SET total_fragments = total_fragments + $1 WHERE id = $2")
            .bind(streak_bonus)
            .bind(user_id)
            .execute(&state.db)
            .await?;
    }

    sqlx::query(
        "UPDATE users SET streak_current = $1, streak_last_activity = $2, updated_at = NOW() WHERE id = $3",
    )
    .bind(new_streak)
    .bind(today)
    .bind(user_id)
    .execute(&state.db)
    .await?;

    Ok(streak_bonus)
}

async fn check_and_award_badges(state: &AppState, user_id: Uuid) -> Result<Vec<Badge>, AppError> {
    // Fetch user stats
    let user: crate::models::User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;

    let challenges_completed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM challenge_submissions WHERE user_id = $1 AND status = 'success'",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await?;

    // Fetch all badges user doesn't have yet
    let unearned: Vec<Badge> = sqlx::query_as(
        "SELECT b.* FROM badges b WHERE b.id NOT IN (SELECT badge_id FROM user_badges WHERE user_id = $1)",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    let mut newly_earned = Vec::new();

    for badge in unearned {
        let earned = match badge.condition_type.as_str() {
            "challenges_completed" => challenges_completed >= badge.condition_value as i64,
            "total_fragments" => user.total_fragments >= badge.condition_value,
            "streak_days" => user.streak_current >= badge.condition_value,
            _ => false,
        };

        if earned {
            sqlx::query(
                "INSERT INTO user_badges (user_id, badge_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(user_id)
            .bind(badge.id)
            .execute(&state.db)
            .await?;
            newly_earned.push(badge);
        }
    }

    Ok(newly_earned)
}

#[cfg(test)]
mod submit_deprecation_tests {
    use super::*;

    #[test]
    fn deprecation_header_is_true() {
        let headers = submit_deprecation_headers();
        assert_eq!(headers.get("Deprecation").unwrap(), "true");
    }

    #[test]
    fn sunset_date_is_rfc7231_format() {
        let headers = submit_deprecation_headers();
        let sunset = headers.get("Sunset").unwrap().to_str().unwrap();
        assert_eq!(sunset, SUBMIT_SUNSET_DATE);
        // Format RFC 7231 : "day-name, day month year hour:minute:second GMT"
        assert!(sunset.contains("GMT"));
        assert!(sunset.contains("2027"));
    }

    #[test]
    fn link_header_contains_successor_and_alternate() {
        let headers = submit_deprecation_headers();
        let link = headers.get("Link").unwrap().to_str().unwrap();
        assert!(link.contains("rel=\"successor-version\""));
        assert!(link.contains("rel=\"alternate\""));
        assert!(link.contains("/deliverables"));
        assert!(link.contains("/webhooks/github/slices"));
    }

    #[test]
    fn all_three_deprecation_headers_are_present() {
        let headers = submit_deprecation_headers();
        assert!(headers.contains_key("Deprecation"));
        assert!(headers.contains_key("Sunset"));
        assert!(headers.contains_key("Link"));
    }
}

async fn log_activity(state: &AppState, user_id: Uuid, fragments: i32) -> Result<(), AppError> {
    let today = chrono::Utc::now().date_naive();

    sqlx::query(
        r#"
        INSERT INTO user_activity (user_id, activity_date, challenges_completed, fragments_earned)
        VALUES ($1, $2, 1, $3)
        ON CONFLICT (user_id, activity_date)
        DO UPDATE SET
            challenges_completed = user_activity.challenges_completed + 1,
            fragments_earned = user_activity.fragments_earned + $3
        "#,
    )
    .bind(user_id)
    .bind(today)
    .bind(fragments)
    .execute(&state.db)
    .await?;

    Ok(())
}
