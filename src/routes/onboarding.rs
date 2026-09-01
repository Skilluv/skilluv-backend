//! Bonjour Skilluv — onboarding flow endpoints.
//!
//! Flow overview (see content strategy doc §9):
//!   1. `POST /api/onboarding/bonjour-skilluv/start` — auto-selects starter template
//!      based on user's primary orientation, calls GitHub API to fork the template
//!      onto the user's account, persists tracking in `onboarding_bonjour_skilluv`.
//!   2. User clones locally, edits HELLO.md, commits, opens PR on their own fork.
//!   3. `GET /api/onboarding/bonjour-skilluv/status` — user polls to see where they
//!      are in the flow (or the frontend uses WebSocket for real-time updates).
//!   4. Webhook handler (not this file) marks `status = completed` when PR is
//!      opened and touches HELLO.md, unlocks the "Bonjour Skilluv" badge, and
//!      creates a hello_wall_entries row.
//!
//! Idempotence: calling `start` twice returns the existing fork instead of
//! creating a duplicate. This matches GitHub's own fork endpoint behavior.

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::github as gh;
use crate::services::onboarding_rite::{self, RiteForm};

/// The GitHub organization hosting the `starter-*` template repositories.
const STARTER_ORG: &str = "skilluv-community";

/// Default starter used when we can't map the user's primary orientation to a
/// specific starter. Broad-appeal Node fullstack is the safest fallback.
const DEFAULT_STARTER_SLUG: &str = "starter-fullstack-node";

pub fn onboarding_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/onboarding/bonjour-skilluv/start",
            post(start_bonjour_skilluv),
        )
        .route(
            "/onboarding/bonjour-skilluv/status",
            get(get_bonjour_skilluv_status),
        )
        .route("/onboarding/rites", get(list_rites))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OnboardingProgress {
    /// The domain whose rite this is, and therefore which of the twelve
    /// gestures the other fields describe.
    pub skill_domain: String,
    /// `fork` or `submission`. Every field below that is `None` for one form
    /// is `None` because of this one.
    pub rite_form: RiteForm,
    /// The `is_domain_rite` template the gesture is against — the brief a
    /// person reads, and for a `submission` rite the challenge to submit to.
    pub challenge_id: Option<Uuid>,
    /// `None` on every rite that is not a fork.
    pub starter_slug: Option<String>,
    pub fork_full_name: Option<String>,
    pub fork_html_url: Option<String>,
    /// fork: `forked`, `hello_committed`, `pr_opened`, `completed`.
    /// submission: `started`, `submitted`, `completed`.
    pub status: String,
    pub pr_number: Option<i32>,
    pub pr_url: Option<String>,
    /// The submission that carries the artifact, on a `submission` rite that
    /// has been handed in.
    pub submission_id: Option<Uuid>,
    /// RFC 3339 timestamp.
    pub started_at: String,
    pub pr_opened_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StartNextSteps {
    /// `git@github.com:owner/repo.git` — hand it to the user for cloning.
    /// `None` on every rite that is not a fork.
    pub clone_url: Option<String>,
    /// What to hand in, in the words of the trade. Straight from the rite
    /// catalogue, so the front renders one contract rather than eleven
    /// hardcoded strings (SKI-362).
    pub expected_artifact: String,
    /// Where to hand it in, for a `submission` rite: `POST
    /// /api/challenges/{id}/start` then `/submit`.
    pub challenge_id: Option<Uuid>,
    /// i18n key the front resolves to render the step-by-step
    /// instructions in the caller's locale.
    pub instructions_key: String,
}

/// A domain's rite, as the front is served it.
#[derive(Debug, Serialize, ToSchema)]
pub struct RiteDescriptor {
    pub domain: String,
    pub form: RiteForm,
    /// One line: what the person does.
    pub gesture: String,
    pub expected_artifact: String,
    /// Which loop reads the artifact — `github_webhook`, `design_critique`,
    /// `playtest_verdicts`, and so on.
    pub review_loop: String,
    /// Whether starting this rite needs a connected GitHub account. True for
    /// exactly one of the twelve.
    pub requires_github: bool,
    /// The published `is_domain_rite` template, when the domain has one.
    /// `None` means the brief has not been published for that domain — the
    /// front should not offer the gesture.
    pub challenge_id: Option<Uuid>,
    pub challenge_title: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RitesCatalogueResponse {
    pub rites: Vec<RiteDescriptor>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StartBonjourResponse {
    /// True when a fork already exists — the response echoes the
    /// existing tracking row instead of calling GitHub again.
    pub already_started: bool,
    pub onboarding: OnboardingProgress,
    /// Present only when `already_started == false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_steps: Option<StartNextSteps>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StatusBonjourResponse {
    /// True when this user has started their rite, in whichever form.
    pub started: bool,
    /// `None` when `started == false`.
    pub onboarding: Option<OnboardingProgress>,
    /// What the rite asks of this caller: the gesture, what to hand in, who
    /// reads it. Present whether or not they have started — it is what the
    /// "first gesture" screen renders, and answering it only after the fact
    /// is what forced the front to hardcode eleven shapes (SKI-362).
    ///
    /// `None` only when the caller has no domain yet and named none.
    pub rite: Option<RiteDescriptor>,
}

fn row_to_progress(row: &OnboardingRow) -> OnboardingProgress {
    OnboardingProgress {
        skill_domain: row.skill_domain.clone(),
        // The column is a string and the enum is the contract; a row whose
        // form is neither is impossible (CHECK), so `fork` is a fallback that
        // never runs rather than a decision.
        rite_form: if row.rite_form == "submission" {
            RiteForm::Submission
        } else {
            RiteForm::Fork
        },
        challenge_id: row.challenge_id,
        starter_slug: row.starter_slug.clone(),
        fork_full_name: row.fork_full_name.clone(),
        fork_html_url: row.fork_html_url.clone(),
        status: row.status.clone(),
        pr_number: row.pr_number,
        pr_url: row.pr_url.clone(),
        submission_id: row.submission_id,
        started_at: row.started_at.to_rfc3339(),
        pr_opened_at: row.pr_opened_at.map(|d| d.to_rfc3339()),
        completed_at: row.completed_at.map(|d| d.to_rfc3339()),
    }
}

/// Public wrapper : resolve a starter slug from the user's primary
/// orientation, falling back to `DEFAULT_STARTER_SLUG` if no explicit
/// mapping exists.
fn starter_for_orientation(orientation_slug: &str) -> &'static str {
    explicit_starter_for_orientation(orientation_slug).unwrap_or(DEFAULT_STARTER_SLUG)
}

/// Resolve a starter slug from the user's primary orientation, returning
/// `None` if the orientation has no explicit mapping.
///
/// Every orientation slug declared in migration `0002_initial_content.sql`
/// (plus additions in 0105, 0106) is explicitly mapped here — no orientation
/// falls through today. Coverage is enforced by unit test
/// `every_db_orientation_maps_to_a_known_starter`.
///
/// The user can override this choice via the `?starter=starter-{slug}` query
/// param on the start endpoint.
///
/// See content strategy annex G for the full list of 15 starters.
fn explicit_starter_for_orientation(orientation_slug: &str) -> Option<&'static str> {
    Some(match orientation_slug {
        // ── Fullstack — Rust default (Skilluv signature)
        "dev-fullstack" | "dev-backend" | "systems-programmer" => "starter-fullstack-rust",

        // ── Frontend
        "dev-frontend" | "web-designer" => "starter-frontend-svelte",

        // ── Mobile
        "mobile-android" => "starter-mobile-kotlin",
        "mobile-cross" => "starter-mobile-react-native",
        "mobile-ios" => "starter-mobile-react-native", // Swift starter not day-1
        // Mobile designers land on RN — closest ecosystem where design ↔ code
        // dialogue happens naturally (Figma → RN component).
        "mobile-designer" => "starter-mobile-react-native",

        // ── Game
        "game-programmer" => "starter-game-godot",
        "game-designer" | "game-artist-2d" | "game-artist-3d" | "game-sound-engineer" => {
            "starter-game-godot"
        }
        // 3D artist : Godot's 3D pipeline is the most Skilluv-relevant entry
        // (open-source, tuto-friendly, doc trans-culturelle). Blender workflow
        // se branche naturellement dessus.
        "3d-artist" => "starter-game-godot",

        // ── Data / AI
        "data-engineer" | "data-analyst" | "ml-engineer" | "prompt-engineer" => {
            "starter-data-python"
        }

        // ── Embarqué / IoT (new orientation added by migration 0105)
        "dev-embarque-iot" => "starter-iot-esp32",

        // ── Ops
        "devops-engineer" | "sre" | "cloud-architect" => "starter-devops",

        // ── Soft skills / misc
        "tech-writer" | "open-source-maintainer" => "starter-frontend-htmx",

        // ── Design roles (illustrator, motion) — Svelte starter is the
        // Skilluv-signature UI environment, and its docs/getting-started is
        // le premier terrain naturel où un·e designer peut contribuer
        // (assets SVG, animations d'illustration, motion tokens).
        "illustrator" | "motion-designer" => "starter-frontend-svelte",

        // ── Security (web, engineer, SOC) — Node fullstack expose la surface
        // d'attaque web classique (Express/Nest + JWT + upload) que ces
        // profils apprennent à auditer. Doc OWASP + secrets management y sont
        // les mieux documentés côté écosystème.
        "pentester-web" | "security-engineer" | "soc-analyst" => "starter-fullstack-node",

        // ── Pentester mobile — RN pour le même raisonnement côté surface
        // d'attaque mobile (JS bridge, storage, in-app deep links).
        "pentester-mobile" => "starter-mobile-react-native",

        // ── Smart contracts — toolchain Solidity mainstream (Hardhat/Foundry)
        // vit dans l'écosystème Node. Le starter Node offre donc le hors-code
        // le plus proche (package.json + scripts npm) sans imposer une chaîne
        // blockchain complète day-1.
        "smart-contract-dev" => "starter-fullstack-node",

        // ── Fallback : future-added orientations. Aujourd'hui aucune ne
        // devrait passer par ici — test unitaire dédié.
        _ => return None,
    })
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/onboarding/bonjour-skilluv/start
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
pub struct StartQuery {
    /// Optional override for the starter slug. If omitted, we auto-select from
    /// the user's primary orientation. Meaningful only on the `code` rite —
    /// it is the only one that forks anything.
    #[serde(default)]
    pub starter: Option<String>,
    /// Which domain's rite to start. Defaults to the caller's own
    /// `skill_domain`, which is what the signup flow sets; passing it is for
    /// somebody who has not completed a profile yet.
    #[param(value_type = Option<crate::validators::SkillDomain>)]
    #[serde(default)]
    pub domain: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct OnboardingRow {
    user_id: Uuid,
    skill_domain: String,
    rite_form: String,
    challenge_id: Option<Uuid>,
    submission_id: Option<Uuid>,
    starter_slug: Option<String>,
    fork_full_name: Option<String>,
    fork_html_url: Option<String>,
    github_fork_id: Option<i64>,
    status: String,
    pr_number: Option<i32>,
    pr_url: Option<String>,
    started_at: chrono::DateTime<chrono::Utc>,
    pr_opened_at: Option<chrono::DateTime<chrono::Utc>>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Kick off the Bonjour Skilluv rite for the caller's domain.
///
/// Twelve gestures, one moment. `code` forks a starter on GitHub and waits for
/// a pull request; the other eleven open a submission against the domain's
/// published rite template, and never touch GitHub. Which is which comes from
/// `services::onboarding_rite`, so the endpoint has no per-domain branch
/// beyond the two forms.
///
/// Idempotent — calling twice returns what already exists, whichever form it
/// took.
#[utoipa::path(
    post,
    path = "/api/onboarding/bonjour-skilluv/start",
    tag = "profile",
    params(StartQuery),
    responses(
        (status = 200, description = "Started (or already started)", body = ApiResponse<StartBonjourResponse>),
        (status = 400, description = "Unknown domain, no domain to infer, invalid starter slug, or — on the code rite only — GitHub not connected", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 404, description = "The domain has no published rite template", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn start_bonjour_skilluv(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<StartQuery>,
) -> Result<Json<ApiResponse<StartBonjourResponse>>, AppError> {
    // ── 1. Idempotence: existing row = return it, no side effect
    if let Some(existing) = load_row(&state.db, auth.user_id).await? {
        return Ok(Json(ApiResponse::new(StartBonjourResponse {
            already_started: true,
            onboarding: row_to_progress(&existing),
            next_steps: None,
        })));
    }

    // ── 2. Which rite. The caller's own domain unless they name one.
    let domain = match q.domain {
        Some(explicit) => {
            crate::validators::check_skill_domain(&explicit, "domain")?;
            explicit
        }
        None => {
            let own: Option<String> =
                sqlx::query_scalar("SELECT skill_domain FROM users WHERE id = $1")
                    .bind(auth.user_id)
                    .fetch_one(&state.db)
                    .await?;
            own.ok_or_else(|| {
                AppError::Validation(
                    "No skill_domain on this account yet. Complete the profile \
                     (POST /api/auth/complete-profile) or pass ?domain="
                        .into(),
                )
            })?
        }
    };

    // `check_skill_domain` passed, and the rite catalogue is tested against the
    // same constant, so this cannot be `None` on a shipped build.
    let rite = onboarding_rite::for_domain(&domain).ok_or_else(|| {
        AppError::Validation(format!(
            "No Bonjour Skilluv rite declared for domain: {domain}"
        ))
    })?;

    // The brief. Migration 0607 publishes twelve, one per domain; a rite whose
    // brief is unpublished is one the front must not offer, so this is a 404
    // rather than a tracking row pointing at nothing.
    let challenge_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM challenge_templates \
         WHERE is_domain_rite = TRUE AND skill_domain = $1 AND status = 'published' \
         LIMIT 1",
    )
    .bind(&domain)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| {
        AppError::NotFound(format!(
            "No published Bonjour Skilluv template for domain: {domain}"
        ))
    })?;

    match rite.form {
        RiteForm::Fork => {
            start_fork_rite(&state, auth.user_id, &domain, challenge_id, q.starter, rite).await
        }
        RiteForm::Submission => {
            let row: OnboardingRow = sqlx::query_as(
                r#"
                INSERT INTO onboarding_bonjour_skilluv
                    (user_id, skill_domain, rite_form, challenge_id, status)
                VALUES ($1, $2, 'submission', $3, 'started')
                RETURNING user_id, skill_domain, rite_form, challenge_id, submission_id,
                          starter_slug, fork_full_name, fork_html_url, github_fork_id,
                          status, pr_number, pr_url, started_at, pr_opened_at, completed_at
                "#,
            )
            .bind(auth.user_id)
            .bind(&domain)
            .bind(challenge_id)
            .fetch_one(&state.db)
            .await?;

            Ok(Json(ApiResponse::new(StartBonjourResponse {
                already_started: false,
                onboarding: row_to_progress(&row),
                next_steps: Some(StartNextSteps {
                    clone_url: None,
                    expected_artifact: rite.expected_artifact.to_string(),
                    challenge_id: Some(challenge_id),
                    instructions_key: "onboarding.bonjour_skilluv.instructions".to_string(),
                }),
            })))
        }
    }
}

/// The `code` gesture: fork a starter on the caller's GitHub account.
///
/// The only path that needs a GitHub token, and the reason it is a function
/// rather than a branch left inline: what used to be the endpoint's first act
/// — and therefore every domain's first wall — is now one of twelve leaves.
async fn start_fork_rite(
    state: &AppState,
    user_id: Uuid,
    domain: &str,
    challenge_id: Uuid,
    starter: Option<String>,
    rite: &onboarding_rite::Rite,
) -> Result<Json<ApiResponse<StartBonjourResponse>>, AppError> {
    let access_token = gh::load_token(&state.db, &state.config.jwt_secret, user_id)
        .await?
        .ok_or_else(|| {
            AppError::Validation(
                "GitHub account not connected. Connect via /api/github/oauth first.".into(),
            )
        })?;

    let starter_slug = if let Some(explicit) = starter {
        validate_starter_slug(&explicit)?;
        explicit
    } else {
        // Look up user's primary (first) orientation
        let orientation_slug: Option<String> = sqlx::query_scalar(
            r#"
            SELECT o.slug
            FROM user_orientations uo
            JOIN orientations o ON o.id = uo.orientation_id
            WHERE uo.user_id = $1 AND uo.is_primary = TRUE AND uo.mode = 'active'
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&state.db)
        .await?;

        match orientation_slug {
            Some(slug) => starter_for_orientation(&slug).to_string(),
            None => DEFAULT_STARTER_SLUG.to_string(),
        }
    };

    let source_full_name = format!("{STARTER_ORG}/{starter_slug}");
    let fork = gh::fork_repo(&access_token, &source_full_name).await?;

    let row: OnboardingRow = sqlx::query_as(
        r#"
        INSERT INTO onboarding_bonjour_skilluv
            (user_id, skill_domain, rite_form, challenge_id,
             starter_slug, fork_full_name, fork_html_url, github_fork_id, status)
        VALUES ($1, $2, 'fork', $3, $4, $5, $6, $7, 'forked')
        RETURNING user_id, skill_domain, rite_form, challenge_id, submission_id,
                  starter_slug, fork_full_name, fork_html_url, github_fork_id,
                  status, pr_number, pr_url, started_at, pr_opened_at, completed_at
        "#,
    )
    .bind(user_id)
    .bind(domain)
    .bind(challenge_id)
    .bind(&starter_slug)
    .bind(&fork.full_name)
    .bind(&fork.html_url)
    .bind(fork.id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(StartBonjourResponse {
        already_started: false,
        onboarding: row_to_progress(&row),
        next_steps: Some(StartNextSteps {
            clone_url: Some(format!("git@github.com:{}.git", fork.full_name)),
            expected_artifact: rite.expected_artifact.to_string(),
            challenge_id: Some(challenge_id),
            instructions_key: "onboarding.bonjour_skilluv.instructions".to_string(),
        }),
    })))
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/onboarding/bonjour-skilluv/status
// ═══════════════════════════════════════════════════════════════════

/// Poll the current Bonjour Skilluv onboarding status. Front uses
/// this in the dashboard tile; WebSocket updates fire the same shape.
#[utoipa::path(
    get,
    path = "/api/onboarding/bonjour-skilluv/status",
    tag = "profile",
    responses(
        (status = 200, description = "Progress snapshot", body = ApiResponse<StatusBonjourResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn get_bonjour_skilluv_status(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<StatusBonjourResponse>>, AppError> {
    let row = load_row(&state.db, auth.user_id).await?;

    // The row's own domain once there is one — a caller who started their rite
    // under `design` must keep being told about `design` even if they later
    // change their declared domain.
    let domain: Option<String> = match &row {
        Some(r) => Some(r.skill_domain.clone()),
        None => {
            sqlx::query_scalar("SELECT skill_domain FROM users WHERE id = $1")
                .bind(auth.user_id)
                .fetch_one(&state.db)
                .await?
        }
    };

    let rite = match domain {
        Some(d) => describe_rite(&state.db, &d).await?,
        None => None,
    };

    Ok(Json(ApiResponse::new(StatusBonjourResponse {
        started: row.is_some(),
        onboarding: row.as_ref().map(row_to_progress),
        rite,
    })))
}

// ════════════════════════════════════════════════════════════════════
// GET /api/onboarding/rites
// ════════════════════════════════════════════════════════════════════

/// The twelve gestures, with the brief each one is against.
///
/// The signup flow shows the domains before an account has one, so it needs
/// the whole catalogue in a single call rather than the caller's own rite. A
/// domain whose `challenge_id` is `None` has no published brief and must not
/// be offered.
#[utoipa::path(
    get,
    path = "/api/onboarding/rites",
    tag = "profile",
    responses(
        (status = 200, description = "Every domain's first gesture", body = ApiResponse<RitesCatalogueResponse>),
    ),
    operation_id = "onboardingListRites",
)]
pub async fn list_rites(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<RitesCatalogueResponse>>, AppError> {
    // One query for the twelve briefs rather than twelve, because this is on
    // the signup path.
    let briefs: Vec<(String, Uuid, String)> = sqlx::query_as(
        "SELECT skill_domain, id, title FROM challenge_templates          WHERE is_domain_rite = TRUE AND status = 'published'",
    )
    .fetch_all(&state.db)
    .await?;

    let rites = onboarding_rite::RITES
        .iter()
        .map(|r| {
            let brief = briefs.iter().find(|(d, _, _)| d == r.domain);
            RiteDescriptor {
                domain: r.domain.to_string(),
                form: r.form,
                gesture: r.gesture.to_string(),
                expected_artifact: r.expected_artifact.to_string(),
                review_loop: r.review_loop.to_string(),
                requires_github: onboarding_rite::requires_github(r.domain),
                challenge_id: brief.map(|(_, id, _)| *id),
                challenge_title: brief.map(|(_, _, t)| t.clone()),
            }
        })
        .collect();

    Ok(Json(ApiResponse::new(RitesCatalogueResponse { rites })))
}

/// One domain's rite, with its published brief attached when there is one.
async fn describe_rite(
    db: &sqlx::PgPool,
    domain: &str,
) -> Result<Option<RiteDescriptor>, AppError> {
    let Some(rite) = onboarding_rite::for_domain(domain) else {
        return Ok(None);
    };
    let brief: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, title FROM challenge_templates          WHERE is_domain_rite = TRUE AND skill_domain = $1 AND status = 'published'          LIMIT 1",
    )
    .bind(domain)
    .fetch_optional(db)
    .await?;

    Ok(Some(RiteDescriptor {
        domain: rite.domain.to_string(),
        form: rite.form,
        gesture: rite.gesture.to_string(),
        expected_artifact: rite.expected_artifact.to_string(),
        review_loop: rite.review_loop.to_string(),
        requires_github: onboarding_rite::requires_github(rite.domain),
        challenge_id: brief.as_ref().map(|(id, _)| *id),
        challenge_title: brief.map(|(_, t)| t),
    }))
}

// ═══════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════

async fn load_row(db: &sqlx::PgPool, user_id: Uuid) -> Result<Option<OnboardingRow>, AppError> {
    let row: Option<OnboardingRow> = sqlx::query_as(
        r#"
        SELECT user_id, skill_domain, rite_form, challenge_id, submission_id,
               starter_slug, fork_full_name, fork_html_url, github_fork_id,
               status, pr_number, pr_url, started_at, pr_opened_at, completed_at
        FROM onboarding_bonjour_skilluv
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    Ok(row)
}

fn validate_starter_slug(slug: &str) -> Result<(), AppError> {
    if !slug.starts_with("starter-")
        || slug.len() < 10
        || slug.len() > 60
        || !slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(AppError::Validation(
            "starter must match pattern 'starter-[a-z0-9-]+' and be 10-60 chars".into(),
        ));
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Webhook handler — called from bounties.rs::handle_pull_request_event
// ═══════════════════════════════════════════════════════════════════

/// Handle a `pull_request` event on a fork we're tracking for Bonjour Skilluv.
///
/// Trigger conditions:
///   - action = "opened"
///   - The repo full_name matches an existing `onboarding_bonjour_skilluv.fork_full_name`
///   - The PR modifies `HELLO.md`
///
/// Actions taken:
///   1. Transition onboarding_bonjour_skilluv.status from `forked` to `pr_opened`,
///      set pr_number and pr_url, timestamp pr_opened_at
///   2. Fetch the HELLO.md content from the fork at the PR head sha
///   3. Insert a hello_wall_entries row so the user's Hello Wall page renders
///   4. Log the event for observability
///
/// Badge unlock is NOT triggered here — that's the proof engine's job (P17-P19),
/// which reads the transition and unlocks the "Bonjour Skilluv" badge based on
/// a badge_rules entry (to be seeded separately).
///
/// Idempotence: if the onboarding row already has status = 'pr_opened' or
/// 'completed', this handler is a no-op. Multiple webhook deliveries for the
/// same PR won't create duplicate hello_wall_entries (protected by UNIQUE
/// user_id on hello_wall_entries).
pub async fn handle_bonjour_skilluv_pr_event(
    state: &crate::AppState,
    payload: &Value,
) -> Result<(), AppError> {
    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    // Only react to the initial "opened" action. Later actions on the same PR
    // (synchronize, closed, reopened) don't change the onboarding status here.
    if action != "opened" {
        return Ok(());
    }

    let pr = payload.get("pull_request").cloned().unwrap_or(Value::Null);
    let pr_number = pr.get("number").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let pr_url = pr
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if pr_number == 0 || pr_url.is_empty() {
        return Ok(());
    }

    // Repo full_name = the fork where the PR was opened. For a Bonjour Skilluv
    // PR, base and head are on the SAME fork (user PRs their own fork's main
    // → showcase branch). So we look up by base.repo.full_name.
    let fork_full_name = pr
        .get("base")
        .and_then(|b| b.get("repo"))
        .and_then(|r| r.get("full_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if fork_full_name.is_empty() {
        return Ok(());
    }

    // Load the onboarding row by fork_full_name.
    let onboarding: Option<(Uuid, String, String, Option<Uuid>)> = sqlx::query_as(
        "SELECT user_id, status, skill_domain, challenge_id
         FROM onboarding_bonjour_skilluv WHERE fork_full_name = $1",
    )
    .bind(fork_full_name)
    .fetch_optional(&state.db)
    .await?;

    let Some((user_id, current_status, skill_domain, tracked_challenge)) = onboarding else {
        // Not a tracked Bonjour Skilluv fork — nothing to do.
        return Ok(());
    };

    // Idempotence: already at pr_opened or completed = skip.
    if matches!(current_status.as_str(), "pr_opened" | "completed") {
        return Ok(());
    }

    // Fetch the user's GitHub token to call the API for PR files and content.
    let access_token =
        crate::services::github::load_token(&state.db, &state.config.jwt_secret, user_id)
            .await?
            .ok_or_else(|| {
                AppError::Internal(
            "Bonjour Skilluv webhook: user has no GitHub connection, cannot verify PR files".into(),
        )
            })?;

    // List files changed in the PR. Skip if HELLO.md not in the diff.
    let files =
        crate::services::github::list_pr_files(&access_token, fork_full_name, pr_number).await?;
    let hello_touched = files
        .iter()
        .any(|f| f.filename == "HELLO.md" && matches!(f.status.as_str(), "added" | "modified"));
    if !hello_touched {
        // User opened a PR unrelated to HELLO.md — legitimate, but not a
        // completion trigger. We could store an intermediate state, but keep
        // it simple: no-op.
        tracing::info!(
            user_id = %user_id,
            pr_number,
            fork_full_name,
            "Bonjour Skilluv PR opened but HELLO.md not touched — skipping"
        );
        return Ok(());
    }

    // Fetch the current HELLO.md content on the PR's head branch. This lets us
    // snapshot what the user actually wrote for archival on the Hello Wall.
    let head_ref = pr
        .get("head")
        .and_then(|h| h.get("ref"))
        .and_then(|v| v.as_str())
        .unwrap_or("main");
    let hello_content = crate::services::github::fetch_file_content(
        &access_token,
        fork_full_name,
        "HELLO.md",
        head_ref,
    )
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(
            error = %e,
            user_id = %user_id,
            fork_full_name,
            "Bonjour Skilluv: failed to fetch HELLO.md content, using placeholder"
        );
        String::from("(content could not be fetched from GitHub)")
    });

    let hello_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(hello_content.as_bytes());
        hex::encode(hasher.finalize())
    };

    // Transaction: update onboarding row + insert hello_wall_entries row.
    let mut tx = state.db.begin().await?;

    sqlx::query(
        r#"
        UPDATE onboarding_bonjour_skilluv
        SET status = 'pr_opened',
            pr_number = $1,
            pr_url = $2,
            pr_opened_at = NOW()
        WHERE user_id = $3
        "#,
    )
    .bind(pr_number)
    .bind(&pr_url)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    // Insert Hello Wall entry. UNIQUE(user_id) prevents duplicates.
    // Extract the source starter repo from the fork_full_name.
    // Example: "amina/starter-fullstack-rust" → "starter-fullstack-rust"
    let source_starter = fork_full_name
        .split('/')
        .nth(1)
        .unwrap_or("starter-unknown")
        .to_string();

    let github_entry_url = format!(
        "https://github.com/skilluv-community/hello-wall/blob/main/entries/{}.md",
        // Very light sanitization: keep only ASCII alphanumerics + dash.
        // Real username sanitization happens application-side when the bot
        // mirrors to the repo; this is a placeholder URL until then.
        fork_full_name
            .split('/')
            .next()
            .unwrap_or("unknown")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect::<String>()
    );

    sqlx::query(
        r#"
        INSERT INTO hello_wall_entries
            (user_id, hello_markdown, hello_hash, github_entry_url,
             source_pr_url, source_starter_repo)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (user_id) DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(&hello_content)
    .bind(&hello_hash)
    .bind(&github_entry_url)
    .bind(&pr_url)
    .bind(&source_starter)
    .execute(&mut *tx)
    .await?;

    // The pull request is the artifact, so it becomes a deliverable and lands
    // in the same review queue as the other eleven rites (SKI-362).
    //
    // Until this, the code rite stopped at `pr_opened` and nothing moved it on
    // — while `badge_rules.bonjour_skilluv` fires on `completed_at IS NOT
    // NULL`. The founding badge was unreachable on the one path that shipped.
    // Opening a pull request is a real gesture but it is not a verdict: it
    // proves somebody pushed a branch, not that a person read what is on it,
    // and after SKI-361 no other rite is allowed to pass without being read.
    //
    // Rows written before migration 0609 carry no `challenge_id`; the domain's
    // published rite is the same answer, looked up.
    let challenge_id: Option<Uuid> = match tracked_challenge {
        Some(id) => Some(id),
        None => {
            sqlx::query_scalar(
                "SELECT id FROM challenge_templates
                 WHERE is_domain_rite AND skill_domain = $1 AND status = 'published'
                 LIMIT 1",
            )
            .bind(&skill_domain)
            .fetch_optional(&mut *tx)
            .await?
        }
    };

    // `hello_hash` is already the SHA-256 of the HELLO.md content, which is
    // what `deliverables` deduplicates on — so a redelivered webhook finds the
    // row rather than writing a second one.
    let deliverable_id: Option<Uuid> = sqlx::query_scalar(
        r#"
        INSERT INTO deliverables (
            challenge_id, user_id,
            artifact_type, artifact_url, artifact_hash, artifact_metadata,
            verifiable_by, verification_status,
            fragments_awarded, public, submitted_at, created_at
        )
        VALUES (
            $1, $2,
            'other', $3, $4, $5,
            'human_review', 'pending',
            0, TRUE, NOW(), NOW()
        )
        ON CONFLICT (user_id, artifact_hash) WHERE artifact_hash IS NOT NULL DO NOTHING
        RETURNING id
        "#,
    )
    .bind(challenge_id)
    .bind(user_id)
    .bind(&pr_url)
    .bind(&hello_hash)
    .bind(serde_json::json!({
        "source": "bonjour_skilluv_pull_request",
        "fork_full_name": fork_full_name,
        "pr_number": pr_number,
        "hello_markdown": hello_content,
    }))
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(id) = deliverable_id {
        sqlx::query("UPDATE onboarding_bonjour_skilluv SET deliverable_id = $1 WHERE user_id = $2")
            .bind(id)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;

        crate::services::ReviewQueueService::create_task_for_deliverable(
            &mut tx,
            id,
            &skill_domain,
            3,
            "any",
        )
        .await?;
    }

    tx.commit().await?;

    tracing::info!(
        user_id = %user_id,
        pr_number,
        fork_full_name,
        hello_hash = %hello_hash,
        deliverable_id = ?deliverable_id,
        "Bonjour Skilluv pull request detected — status pr_opened, Hello Wall entry created, queued for review"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_maps_to_expected_starter() {
        assert_eq!(
            starter_for_orientation("dev-fullstack"),
            "starter-fullstack-rust"
        );
        assert_eq!(
            starter_for_orientation("dev-frontend"),
            "starter-frontend-svelte"
        );
        assert_eq!(
            starter_for_orientation("mobile-android"),
            "starter-mobile-kotlin"
        );
        assert_eq!(
            starter_for_orientation("dev-embarque-iot"),
            "starter-iot-esp32"
        );
        assert_eq!(
            starter_for_orientation("game-programmer"),
            "starter-game-godot"
        );
        assert_eq!(
            starter_for_orientation("ml-engineer"),
            "starter-data-python"
        );
        assert_eq!(starter_for_orientation("devops-engineer"), "starter-devops");
        assert_eq!(
            starter_for_orientation("tech-writer"),
            "starter-frontend-htmx"
        );
        // Design roles
        assert_eq!(
            starter_for_orientation("illustrator"),
            "starter-frontend-svelte"
        );
        assert_eq!(
            starter_for_orientation("motion-designer"),
            "starter-frontend-svelte"
        );
        assert_eq!(
            starter_for_orientation("mobile-designer"),
            "starter-mobile-react-native"
        );
        assert_eq!(starter_for_orientation("3d-artist"), "starter-game-godot");
        // Security roles
        assert_eq!(
            starter_for_orientation("pentester-web"),
            "starter-fullstack-node"
        );
        assert_eq!(
            starter_for_orientation("security-engineer"),
            "starter-fullstack-node"
        );
        assert_eq!(
            starter_for_orientation("soc-analyst"),
            "starter-fullstack-node"
        );
        assert_eq!(
            starter_for_orientation("pentester-mobile"),
            "starter-mobile-react-native"
        );
        // Blockchain
        assert_eq!(
            starter_for_orientation("smart-contract-dev"),
            "starter-fullstack-node"
        );
        // Unknown orientation → default
        assert_eq!(
            starter_for_orientation("unknown-slug"),
            DEFAULT_STARTER_SLUG
        );
    }

    /// Regression guard : chaque slug d'orientation dans la DB doit avoir un
    /// mapping *explicite* dans `starter_for_orientation`, jamais tomber au
    /// `DEFAULT_STARTER_SLUG`. Le test lit la liste des 32 slugs (migrations
    /// 0002 + 0105 + 0106) et vérifie que chaque appel retourne un starter
    /// != du DEFAULT. Si on ajoute une orientation future, ce test échoue
    /// tant qu'on n'a pas ajouté un cas dédié dans le match.
    ///
    /// Note : la liste est hard-codée ici plutôt que lue de la DB pour que le
    /// test soit unitaire pur (pas de connexion Postgres requise en CI).
    #[test]
    fn every_db_orientation_maps_to_a_known_starter() {
        // Source : SELECT slug FROM orientations ORDER BY slug — snapshot au
        // 2026-07-22, 32 slugs (24 initial + 6 game + 1 IoT + 1 smart-contract).
        const ALL_ORIENTATION_SLUGS: &[&str] = &[
            "3d-artist",
            "cloud-architect",
            "data-analyst",
            "data-engineer",
            "dev-backend",
            "dev-embarque-iot",
            "dev-frontend",
            "dev-fullstack",
            "devops-engineer",
            "game-artist-2d",
            "game-artist-3d",
            "game-designer",
            "game-programmer",
            "game-sound-engineer",
            "illustrator",
            "ml-engineer",
            "mobile-android",
            "mobile-cross",
            "mobile-designer",
            "mobile-ios",
            "motion-designer",
            "open-source-maintainer",
            "pentester-mobile",
            "pentester-web",
            "prompt-engineer",
            "security-engineer",
            "smart-contract-dev",
            "soc-analyst",
            "sre",
            "systems-programmer",
            "tech-writer",
            "web-designer",
        ];

        let unmapped: Vec<_> = ALL_ORIENTATION_SLUGS
            .iter()
            .filter(|slug| explicit_starter_for_orientation(slug).is_none())
            .copied()
            .collect();
        assert!(
            unmapped.is_empty(),
            "Orientations sans mapping explicite (fallback DEFAULT) : {unmapped:?}. Ajoute un cas dédié dans explicit_starter_for_orientation()."
        );
    }

    #[test]
    fn validate_starter_slug_accepts_valid() {
        assert!(validate_starter_slug("starter-fullstack-rust").is_ok());
        assert!(validate_starter_slug("starter-iot-esp32").is_ok());
        assert!(validate_starter_slug("starter-devops").is_ok());
    }

    #[test]
    fn validate_starter_slug_rejects_invalid() {
        assert!(validate_starter_slug("").is_err());
        assert!(validate_starter_slug("no-prefix").is_err());
        assert!(validate_starter_slug("starter-").is_err()); // too short
        assert!(validate_starter_slug("starter-UPPER").is_err()); // uppercase
        assert!(validate_starter_slug("starter-under_score").is_err()); // underscore
        assert!(validate_starter_slug("starter-slash/hack").is_err()); // slash (attack surface)
        // Length overflow
        let too_long = format!("starter-{}", "x".repeat(60));
        assert!(validate_starter_slug(&too_long).is_err());
    }
}
