//! The domain wizard: five questions whose answers shape what gets
//! recommended.
//!
//! ## Why the vocabulary is here and not in a CHECK
//!
//! Every answer is closed — a level is one of five values, not free text —
//! and refusing an unknown one matters, because a recommender reading
//! `"senior "` with a trailing space silently recommends nothing. But the
//! list changes as the wizard is reworded, and a CHECK would make each
//! rewording a migration. So the table stores an object and this module owns
//! the words.
//!
//! ## What this is not
//!
//! Not a claim about anybody. Declared level and declared framework are used
//! to sort what to show, never to credit: rank, badges and craft score read
//! proofs, and nothing here is one.
//!
//! ## HuggingFace
//!
//! The wizard collects a HuggingFace username, and does not import that
//! account's models. Importing them would put artefacts on a profile with no
//! verified deliverable behind them — a list somebody typed, which is exactly
//! what the charter refuses. The username is a link a reader can follow, and
//! a model counts here when it arrives as work that was reviewed.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn domain_profile_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/users/me/domain-profile/{domain}",
            get(get_profile).put(put_profile),
        )
        .route(
            "/users/me/domain-profile/{domain}/skip",
            axum::routing::post(skip_profile),
        )
        .route(
            "/users/me/domain-profile/{domain}/questions",
            get(list_questions),
        )
        .route("/domains/{domain}/mentors/for-me", get(mentor_matches))
}

use crate::validators::SKILL_DOMAINS as DOMAINS;

/// Domain-agnostic answers. Every domain asks these three.
const LEVELS: &[&str] = &[
    "debutant",
    "apprentissage",
    "practitioner",
    "senior",
    "researcher",
];
const WEEKLY_HOURS: &[&str] = &["lt3", "3_10", "gt10", "fulltime"];
const GOALS: &[&str] = &[
    "learning",
    "portfolio",
    "paid_missions",
    "academic_research",
    "startup",
];

/// AI only. What somebody can actually run decides which challenges are
/// worth showing them — recommending a seventy-billion-parameter fine-tune to
/// someone on free Colab wastes their week.
const COMPUTE: &[&str] = &[
    "none",         // free Colab or Kaggle, interrupted sessions
    "personal_gpu", // a card of their own
    "cloud_small",  // under 500 $ a month
    "cloud_large",  // over
    "enterprise",   // an employer's cluster
];
const FRAMEWORKS: &[&str] = &["pytorch", "jax", "tensorflow", "candle", "mlx", "other"];

/// Audio only. What the person wants to write *for*.
///
/// Not a skill domain, and the distinction matters: somebody scoring a podcast
/// practises audio, not podcasting. Recommending them a game jam because they
/// answered `podcast` would be the recommender reading a taxonomy it invented.
const AUDIO_DESTINATIONS: &[&str] = &["game", "motion", "podcast", "brand", "ui", "cross"];

/// Audio only. The stations somebody actually works in.
///
/// Asked because it is the single most useful thing to know when pairing a
/// beginner with a mentor: a session where one person cannot open the other's
/// project is an hour spent on file formats.
///
/// Plural for the same reason the AI frameworks are: a composer who writes in
/// Reaper and mixes in Ardour is one person, and forcing a choice would lose
/// half of what the matching needs.
const AUDIO_DAWS: &[&str] = &[
    "reaper",
    "ardour",
    "logic",
    "fl_studio",
    "ableton",
    "cubase",
    "pro_tools",
    "audacity",
    "other",
];

/// At most three. Somebody who selects everything has told us nothing while
/// believing they answered — the same cap the code wizard uses.
const MAX_SELECTIONS: usize = 3;

/// One question the wizard asks, and what it accepts as an answer.
///
/// A registry rather than a struct field per question. The wizard used to be a
/// typed body with an `Option<String>` per AI question and a comment saying
/// "AI only" above each; the second domain to need three questions of its own
/// would have made that struct a list of everybody's fields, each null for
/// everybody else — which is the shape migration 0306 removed from the `users`
/// table for exactly the same reason.
///
/// The wire format did not change: the body is still a flat object of the same
/// keys. What changed is that the keys a domain accepts are data, so the front
/// end can render the wizard from `GET .../questions` instead of shipping its
/// own copy of the list.
pub struct Question {
    pub key: &'static str,
    /// Several answers rather than one. Capped by [`MAX_SELECTIONS`].
    pub multi: bool,
    /// The closed vocabulary, or empty for free text — a username on somebody
    /// else's service, which we cannot enumerate.
    pub allowed: &'static [&'static str],
    /// Longest accepted free-text answer. Ignored when `allowed` is non-empty.
    pub max_len: usize,
}

const fn closed(key: &'static str, allowed: &'static [&'static str]) -> Question {
    Question {
        key,
        multi: false,
        allowed,
        max_len: 0,
    }
}

const fn closed_multi(key: &'static str, allowed: &'static [&'static str]) -> Question {
    Question {
        key,
        multi: true,
        allowed,
        max_len: 0,
    }
}

/// Several answers, checked for length rather than against a vocabulary.
///
/// For the questions whose vocabulary the platform does not own. `check_answer`
/// already reads an empty `allowed` as "any value within `max_len`"; this only
/// names the combination so a reader does not have to infer it.
const fn open_multi(key: &'static str, max_len: usize) -> Question {
    Question {
        key,
        multi: true,
        allowed: &[],
        max_len,
    }
}

const fn free_text(key: &'static str, max_len: usize) -> Question {
    Question {
        key,
        multi: false,
        allowed: &[],
        max_len,
    }
}

/// The code wizard's own ladder, kept rather than folded into the five above.
///
/// `staff` is a rank the design ladder has no word for, and inventing one so
/// that a single list would serve both loses the distinction the question
/// exists to draw. The same is true of the hours and the goals: "contribute
/// upstream" and "publish a library" are things a developer answers and a
/// designer does not.
///
/// So the three questions every domain asks are asked in each domain's own
/// words. Which is not the same as each domain asking different questions —
/// that is `questions_for`, below.
const CODE_LEVELS: &[&str] = &["beginner", "junior", "mid", "senior", "staff"];
const CODE_WEEKLY_HOURS: &[&str] = &["under_5", "5_to_15", "15_to_40", "fulltime"];
const CODE_GOALS: &[&str] = &[
    "learn",
    "build_portfolio",
    "find_paid_work",
    "contribute_upstream",
    "publish_library",
    "become_mentor",
    "ship_own_product",
];
const CODE_CHALLENGE_PREFERENCES: &[&str] = &[
    "upstream_contributions",
    "solo_shipped_apps",
    "published_libraries",
    "long_team_projects",
    "short_hackathons",
];

/// Asked in every domain, in the words that domain uses.
const COMMON_QUESTIONS: &[Question] = &[
    closed("level", LEVELS),
    closed("weekly_hours", WEEKLY_HOURS),
    closed("goal", GOALS),
];

const CODE_COMMON_QUESTIONS: &[Question] = &[
    closed("level", CODE_LEVELS),
    closed("weekly_hours", CODE_WEEKLY_HOURS),
    closed("goal", CODE_GOALS),
];

/// The three shared questions, in this domain's vocabulary.
fn common_questions_for(domain: &str) -> &'static [Question] {
    match domain {
        "code" => CODE_COMMON_QUESTIONS,
        _ => COMMON_QUESTIONS,
    }
}

const AI_QUESTIONS: &[Question] = &[
    closed("compute", COMPUTE),
    closed_multi("main_frameworks", FRAMEWORKS),
    free_text("huggingface_username", 60),
];

/// Whether somebody wants to be handed work or to compete for it. A designer
/// who only wants contests and is shown a queue of briefs reads the platform
/// as empty, and the reverse is worse: an invitation to compete lands badly on
/// somebody who came here to practise.
const CHALLENGE_PREFERENCES: &[&str] = &["individual", "contest", "both", "undecided"];

/// What they work in. A bonus in the matching, never a filter — a good mentor
/// in a neighbouring tool beats a mediocre one in the same.
const DESIGN_TOOLS: &[&str] = &[
    "figma",
    "adobe",
    "sketch",
    "blender",
    "after_effects",
    "other",
];

const DESIGN_QUESTIONS: &[Question] = &[
    closed("challenge_preference", CHALLENGE_PREFERENCES),
    closed("main_tool", DESIGN_TOOLS),
    // Recorded as an unconfirmed signal, never fetched. See
    // `claim_portfolio_signal`.
    free_text("portfolio_url", 500),
];

/// What a developer wants to spend their time on. Its own vocabulary for the
/// same reason the code ladder is: "upstream contributions" and "published
/// libraries" are answers a developer gives and a designer has no equivalent
/// for.
const CODE_QUESTIONS: &[Question] = &[
    closed("challenge_preference", CODE_CHALLENGE_PREFERENCES),
    // Open, where every other domain's tools question is closed, and on
    // purpose. A framework list and a DAW list are vocabularies the platform
    // owns; the set of things a developer works in is not one — it includes
    // Terraform and Elixir and whatever shipped last year. The sandbox keeps a
    // language catalogue, but it lists what a challenge can be *executed* in,
    // which is narrower than what somebody works in, and using it would refuse
    // a real answer.
    //
    // Safe to leave open because of what reads it: the matching treats tools
    // as a bonus and never a filter, and the first-issues query passes the
    // first one through as `?language=`. An unknown value costs an empty feed,
    // not a wrong recommendation. `MAX_SELECTIONS` still applies.
    open_multi("main_tools", 40),
    // Claimed here, proved only by the OAuth callback. See `claim_github`.
    free_text("github_username", 39),
];

/// Communication only. Which formats somebody actually works in.
///
/// Plural, like the AI frameworks and the audio stations: the person who
/// writes the documentation and then films the tutorial is this domain's
/// normal shape rather than its exception, and forcing a choice would lose
/// half of what the matching needs.
const COMMUNICATION_FORMATS: &[&str] = &[
    "documentation",
    "articles",
    "talks",
    "video",
    "livestream",
    "podcast",
    "translation",
    "research",
];

/// Communication and education both ask it: what the person communicates or
/// teaches *about*.
///
/// A domain slug rather than free text, because it is used to pick which
/// challenges to show — somebody who documents infrastructure should not be
/// handed a game-audio brief. `cross` is a real answer and the most common
/// one among people who have done this for a while.
const SUBJECT_DOMAINS: &[&str] = &[
    "code", "design", "game", "ai", "ops", "security", "audio", "cross",
];

/// Education only. Where the person teaches.
///
/// Asked because it is the single most useful thing to know when pairing two
/// educators: a bootcamp instructor and somebody running community workshops
/// share a craft and almost no constraints. Plural, because most people who
/// teach for a living do it in two of these.
const TEACHING_SETTINGS: &[&str] = &[
    "bootcamp",
    "school",
    "university",
    "in_company",
    "community",
    "self_paced",
    "one_to_one",
];

/// Education only. Who they teach.
///
/// A different question from `level`, which asks how experienced the *teacher*
/// is. Somebody twenty years into the trade may teach absolute beginners, and
/// conflating the two would recommend them a curriculum-design brief for
/// senior engineers.
const LEARNER_LEVELS: &[&str] = &["beginner", "junior", "mid", "senior", "mixed"];

const EDUCATION_QUESTIONS: &[Question] = &[
    closed_multi("main_settings", TEACHING_SETTINGS),
    closed("learner_level", LEARNER_LEVELS),
    closed("subject_domain", SUBJECT_DOMAINS),
];

/// Security only. Which certifications somebody says they hold.
///
/// A routing hint and never a claim. What a certification actually is on this
/// platform is a row in `external_credentials` with somebody's verification
/// against it; this answer is the wizard asking "what should we show you
/// first", and an OSCP holder shown the introduction to intercepting proxies
/// reads the platform as being for beginners.
///
/// `none` is a real answer and the most common one. It is listed so that
/// leaving the question blank and answering it honestly are distinguishable.
const SECURITY_CERTIFICATIONS: &[&str] = &[
    "none",
    "security_plus",
    "oscp_or_offsec",
    "ceh",
    "cissp_or_cism",
    "gcih_or_giac",
    "cloud_security",
    "other",
];

/// Security only. What somebody can actually run.
///
/// The single most useful thing to know before recommending anything in this
/// domain, and the analogue of the AI wizard's `compute` question. Somebody on
/// a locked-down work laptop cannot run a vulnerable virtual machine, and
/// pointing them at one wastes their week; they can work through hosted labs
/// all day.
const SECURITY_LAB_SETUPS: &[&str] = &[
    // A browser and nothing else. Hosted labs only.
    "browser_only",
    // Can install tools locally but not run virtual machines.
    "local_tools",
    // Can run virtual machines on their own machine.
    "local_vms",
    // Has a separate machine or a home lab.
    "home_lab",
    // Can spin up cloud instances and pay for them.
    "cloud",
];

const SECURITY_QUESTIONS: &[Question] = &[
    closed_multi("security_certifications", SECURITY_CERTIFICATIONS),
    closed("security_lab_setup", SECURITY_LAB_SETUPS),
    // Open, for the reason the code and quality ones are: what a security
    // person works in runs from Burp to Volatility to a spreadsheet of
    // controls, and a closed list would refuse real answers. Read as a bonus
    // in the mentor matching, never as a filter.
    open_multi("security_tools", 40),
];

const COMMUNICATION_QUESTIONS: &[Question] = &[
    closed_multi("main_formats", COMMUNICATION_FORMATS),
    closed("subject_domain", SUBJECT_DOMAINS),
    // Recorded as an unconfirmed signal, never fetched. A handle is a link a
    // reader can follow; an article counts here when it arrives as work that
    // was reviewed.
    free_text("dev_to_username", 60),
    free_text("blog_url", 500),
];

const AUDIO_QUESTIONS: &[Question] = &[
    closed("audio_destination", AUDIO_DESTINATIONS),
    closed_multi("main_daws", AUDIO_DAWS),
    free_text("soundcloud_username", 60),
    free_text("bandcamp_username", 60),
];

/// Quality only. Where somebody is arriving from.
///
/// Asked because this domain receives more career changers than any other on
/// the platform, and the first month of a developer moving into testing and of
/// somebody arriving from support have almost nothing in common. It steers
/// which guide is shown first and nothing else — it is a statement about a
/// path, never about a level.
const QUALITY_BACKGROUNDS: &[&str] = &[
    "developer_moving_across",
    "professional_tester",
    "support_or_operations",
    "career_change",
    "student",
    "other",
];

/// Quality only. Which domains somebody wants to put to the test.
///
/// The one wizard answer in this domain that has no equivalent anywhere else,
/// and the reason it exists: every other trade works *in* a domain, this one
/// works *on* one. Somebody who wants to test games and is shown a queue of
/// API test plans reads the platform as empty.
///
/// Closed against the same domain list every other guard reads, so a domain
/// opened later becomes an answer here without anybody editing this file.
/// Multi, capped by `MAX_SELECTIONS`: two is a specialisation, everything is
/// an absence of one.
const QUALITY_TARGET_DOMAINS: &[&str] = crate::validators::SKILL_DOMAINS;

const QUALITY_QUESTIONS: &[Question] = &[
    closed("quality_background", QUALITY_BACKGROUNDS),
    closed_multi("quality_target_domains", QUALITY_TARGET_DOMAINS),
    // Open, for the reason the code one is: the set of things a tester works
    // in runs from Playwright to a screen reader to a spreadsheet, and a
    // closed list would refuse real answers. Read as a bonus in the matching,
    // never as a filter.
    open_multi("quality_tools", 40),
];

/// Leadership only. How long somebody has been leading, as distinct from how
/// long they have been working.
///
/// Its own ladder rather than the shared one, because the shared vocabulary
/// (`debutant` … `researcher`) is about depth in a craft and this question is
/// about a different axis entirely: a principal engineer of fifteen years who
/// has never written a roadmap answers `aspiring` here, honestly, and the
/// shared list has no word that lets them.
const LEADERSHIP_LEVELS: &[&str] = &[
    "aspiring",    // no formal leading yet, and wants to
    "emerging",    // leads something small, informally
    "lead",        // a team or a track, formally
    "senior_lead", // several teams, or a function
    "executive",
];

/// Leadership only. What kind of leading somebody is here to do. Not the same
/// question as the trade — somebody can want `lead-tech` artefacts while
/// their situation only offers them community work.
const LEADERSHIP_CONTEXTS: &[&str] = &[
    "employed_team", // they lead people at work
    "open_source",   // they lead in a project they do not own
    "community",     // a group, a chapter, a server
    "own_venture",
    "none_yet", // practising before the situation exists
];

const LEADERSHIP_QUESTIONS: &[Question] = &[
    closed("leadership_level", LEADERSHIP_LEVELS),
    closed("leadership_context", LEADERSHIP_CONTEXTS),
    // Which domains they want to hold a direction for. Same list and same
    // reasoning as quality's: this trade works *on* a domain rather than in
    // one.
    closed_multi("leadership_target_domains", QUALITY_TARGET_DOMAINS),
    // Open, like code's and quality's. What a leader works in runs from Linear
    // to a shared document to a whiteboard, and a closed list would refuse
    // real answers.
    open_multi("leadership_tools", 40),
];

/// Which wizard a field belongs to, for the message when it arrives on
/// another one.
///
/// Derived from the same registry the validation reads, so a question added to
/// a domain is attributable to it without a second list to keep in step.
pub fn owning_domain(key: &str) -> Option<&'static str> {
    crate::validators::SKILL_DOMAINS
        .iter()
        .find(|domain| questions_for(domain).iter().any(|q| q.key == key))
        .copied()
}

/// What this domain asks, beyond the three everybody asks.
///
/// `preferred_families` is handled separately in every domain: its vocabulary
/// is a query against `orientations` rather than a constant.
pub fn questions_for(domain: &str) -> &'static [Question] {
    match domain {
        "ai" => AI_QUESTIONS,
        "audio" => AUDIO_QUESTIONS,
        "code" => CODE_QUESTIONS,
        "communication" => COMMUNICATION_QUESTIONS,
        "design" => DESIGN_QUESTIONS,
        "quality" => QUALITY_QUESTIONS,
        "leadership" => LEADERSHIP_QUESTIONS,
        "education" => EDUCATION_QUESTIONS,
        "security" => SECURITY_QUESTIONS,
        _ => &[],
    }
}

/// The families a mentee wants to be matched in, per domain: reviewer groups,
/// the same ones the guides and the review capabilities use.
///
/// Read by the mentor matching, which is why an unknown one is refused rather
/// than stored: it would match nobody, silently, and look like an empty
/// platform.
/// The values `preferred_families` accepts in this domain, in order.
///
/// One function because there are two callers: the wizard that validates the
/// answer and the endpoint that publishes the choices. They used to hold a
/// query each, and the queries disagreed — the endpoint offered design's
/// reviewer groups while the validator wanted design's trades, so a form built
/// from what the API published was refused by the API that published it. Both
/// were defensible alone, which is why nothing caught it until a test sent one
/// through the other.
pub async fn families_for(db: &sqlx::PgPool, domain: &str) -> Result<Vec<String>, AppError> {
    // Which vocabulary this domain's wizard speaks. Design asks which trades
    // interest you — `design-brand-identity` — where the others ask for the
    // reviewer family directly. The matcher reads the answer through the same
    // flag, so what the wizard accepts is what the matcher can use.
    let by_trade = crate::services::mentorship_matching::rules_for(domain)
        .is_some_and(|r| r.families_are_trade_slugs);

    // Archived either way: a trade nobody should be starting is one the
    // registration endpoint already refuses, and accepting it here would
    // store an answer that reads as a live choice.
    let known: Vec<String> = if by_trade {
        sqlx::query_scalar(
            "SELECT slug FROM orientations
              WHERE primary_domain = $1
                AND reviewer_group IS NOT NULL
                AND is_archived = FALSE
              ORDER BY slug",
        )
    } else {
        sqlx::query_scalar(
            "SELECT DISTINCT reviewer_group FROM orientations
              WHERE reviewer_group IS NOT NULL
                AND primary_domain = $1
                AND is_archived = FALSE",
        )
    }
    .bind(domain)
    .fetch_all(db)
    .await?;

    Ok(known)
}

/// The families a mentee wants to be matched in, per domain: reviewer groups,
/// the same ones the guides and the review capabilities use — or trades, where
/// the domain asks in trades. See [`families_for`].
///
/// Read by the mentor matching, which is why an unknown one is refused rather
/// than stored: it would match nobody, silently, and look like an empty
/// platform.
async fn check_families(
    db: &sqlx::PgPool,
    domain: &str,
    families: &[String],
) -> Result<(), AppError> {
    if families.len() > MAX_SELECTIONS {
        return Err(AppError::Validation(format!(
            "at most {MAX_SELECTIONS} families — picking everything says nothing"
        )));
    }
    let by_trade = crate::services::mentorship_matching::rules_for(domain)
        .is_some_and(|r| r.families_are_trade_slugs);
    let known = families_for(db, domain).await?;

    for family in families {
        if !known.contains(family) {
            // Only the offending value is named. The families are a list of
            // thirteen worth printing; the trades are twenty-six, and a
            // message that recites all of them buries the one thing the caller
            // needs to see — including, if it happens to be in the list, the
            // value they got right.
            return Err(if by_trade {
                AppError::Validation(format!(
                    "'{family}' is not a {domain} trade — GET /api/orientations?domain={domain} \
                     lists them"
                ))
            } else {
                AppError::Validation(format!(
                    "'{family}' is not a {domain} family — expected one of: {}",
                    known.join(", ")
                ))
            });
        }
    }
    Ok(())
}

/// The wizard's answers, as the flat object the wizard sends.
///
/// A map rather than a field per question. Which keys are accepted depends on
/// the domain and comes from [`questions_for`]; `GET .../questions` returns
/// the same list so a front end can render the form instead of hard-coding it.
///
/// `preferred_families` is accepted in every domain and validated against
/// `orientations` rather than against a constant.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct DomainProfileBody {
    #[schema(value_type = Object)]
    pub answers: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DomainProfileResponse {
    pub domain: String,
    #[schema(value_type = Object)]
    pub answers: serde_json::Value,
    /// When the wizard was answered. Absent means never.
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When somebody said stop asking. Different from having answered
    /// nothing: the first means "stop", the second means "ask again".
    pub skipped_at: Option<chrono::DateTime<chrono::Utc>>,
    /// What to do first, given what was just said.
    ///
    /// Present on the answer to the wizard and absent everywhere else. It is
    /// the reply to having answered, not a property of the profile: a read
    /// carrying one would invite a front end to show month-one advice to
    /// somebody in their sixth month.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<crate::services::onboarding_recommendation::Recommendation>,
}

/// Refuse an answer this question does not accept, naming what it does.
///
/// A closed question is checked against its vocabulary; a free-text one only
/// against its length. An unknown value is not stored as a curiosity: a
/// recommender that reads one has no branch for it and silently recommends
/// nothing, which looks like an empty platform rather than a bad answer.
fn check_answer(question: &Question, value: &str) -> Result<(), AppError> {
    if question.allowed.is_empty() {
        return crate::validators::check_max_len(value, question.key, question.max_len);
    }
    if !question.allowed.contains(&value) {
        return Err(AppError::Validation(format!(
            "{} must be one of: {}",
            question.key,
            question.allowed.join(", ")
        )));
    }
    Ok(())
}

/// Read a JSON array of strings, refusing anything else by name.
fn as_string_list(key: &str, value: &serde_json::Value) -> Result<Vec<String>, AppError> {
    let array = value
        .as_array()
        .ok_or_else(|| AppError::Validation(format!("'{key}' must be a list")))?;
    array
        .iter()
        .map(|v| {
            v.as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| AppError::Validation(format!("'{key}' must be a list of strings")))
        })
        .collect()
}

fn check_domain(domain: &str) -> Result<(), AppError> {
    if !DOMAINS.contains(&domain) {
        return Err(AppError::Validation(format!(
            "unknown domain '{domain}': one of {}",
            DOMAINS.join(", ")
        )));
    }
    Ok(())
}

/// The caller's answers for one domain. Empty object when never filled in.
#[utoipa::path(
    get, path = "/api/users/me/domain-profile/{domain}", tag = "profile",
    params(("domain" = String, Path, description = "Domain slug")),
    responses(
        (status = 200, description = "Answers", body = ApiResponse<DomainProfileResponse>),
        (status = 400, description = "Unknown domain", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "domainProfileGetProfile",
)]
pub async fn get_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(domain): Path<String>,
) -> Result<Json<ApiResponse<DomainProfileResponse>>, AppError> {
    check_domain(&domain)?;

    type Row = (
        serde_json::Value,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    );
    let row: Option<Row> = sqlx::query_as(
        "SELECT answers, completed_at, skipped_at
           FROM user_domain_profiles WHERE user_id = $1 AND domain = $2",
    )
    .bind(auth.user_id)
    .bind(&domain)
    .fetch_optional(&state.db)
    .await?;

    let (answers, completed_at, skipped_at) = row.unwrap_or((json!({}), None, None));

    Ok(Json(ApiResponse::new(DomainProfileResponse {
        domain,
        answers,
        completed_at,
        skipped_at,
        recommendation: None,
    })))
}

/// Turn the handles the wizard collected into linked portfolio rows.
///
/// Ticket O-01 asked the audio wizard to trigger the portfolio import when
/// somebody gives a SoundCloud or Bandcamp handle. Storing the handle and
/// doing nothing with it is the shape this codebase keeps removing: an answer
/// that reads as collected and is used by nothing.
///
/// The row is `figures_are_declared` with no counts, because a handle is not
/// an audience. What it gives a reader is a link they can follow, and what it
/// gives the person is one less form to fill in; the play counts come later,
/// from them, through `/api/audio/portfolios`.
///
/// Failures are logged and dropped. Somebody answering a wizard is not to be
/// shown an error because a convenience did not fire, and the endpoint that
/// does this properly is one click away.
async fn link_declared_handles(
    db: &sqlx::PgPool,
    user_id: uuid::Uuid,
    answers: &serde_json::Map<String, serde_json::Value>,
) {
    for (key, platform, base) in [
        (
            "soundcloud_username",
            "soundcloud",
            "https://soundcloud.com/",
        ),
        ("bandcamp_username", "bandcamp", "https://bandcamp.com/"),
        // Same treatment for the other wizards' handles. Each was stored and
        // read by nothing, which is the shape the paragraph above describes.
        // A username typed into a form is a claim; only the OAuth callback
        // makes it a proved account, so these rows stay unverified.
        ("github_username", "github", "https://github.com/"),
        (
            "huggingface_username",
            "huggingface",
            "https://huggingface.co/",
        ),
    ] {
        let Some(handle) = answers.get(key).and_then(|v| v.as_str()) else {
            continue;
        };
        let handle = handle.trim();
        // A handle with a slash in it is a pasted URL rather than a name, and
        // concatenating it would produce a link that goes nowhere.
        if handle.is_empty() || handle.contains('/') {
            continue;
        }

        let outcome = sqlx::query(
            r#"
            INSERT INTO user_external_portfolios
                (user_id, platform, handle, profile_url, figures_are_declared, sync_enabled)
            VALUES ($1, $2, $3, $4, TRUE, FALSE)
            ON CONFLICT (user_id, platform, handle) DO UPDATE
                SET profile_url = EXCLUDED.profile_url, updated_at = NOW()
            "#,
        )
        .bind(user_id)
        .bind(platform)
        .bind(handle)
        .bind(format!("{base}{handle}"))
        .execute(db)
        .await;

        if let Err(e) = outcome {
            tracing::warn!(user = %user_id, platform, error = %e, "handle not linked");
        }
    }
}

/// Record a declared portfolio, unconfirmed.
///
/// `external_signals` is where portfolios on platforms Skilluv does not own
/// live, and a row without `verified_at` is exactly what an unconfirmed one
/// is: visible to a moderator, invisible to a recruiter search. The backend
/// does not fetch the URL — fetching arbitrary user-supplied addresses is how
/// a server becomes somebody's proxy.
async fn claim_portfolio_signal(
    db: &sqlx::PgPool,
    user_id: uuid::Uuid,
    answers: &serde_json::Map<String, serde_json::Value>,
) {
    let Some(url) = answers.get("portfolio_url").and_then(|v| v.as_str()) else {
        return;
    };
    let url = url.trim();

    // The provider is read off the host rather than asked for: somebody
    // pasting a Behance link has already said which platform it is.
    let provider = if url.contains("behance.net") {
        "behance"
    } else if url.contains("dribbble.com") {
        "dribbble"
    } else if url.contains("artstation.com") {
        "artstation"
    } else if url.contains("vimeo.com") {
        "vimeo"
    } else {
        // A personal site is not one of the known platforms, and inventing a
        // provider for it would put it in a filter it does not belong in.
        return;
    };

    let result = sqlx::query(
        "INSERT INTO external_signals (user_id, provider, url, title)
         VALUES ($1, $2, $3, 'Portfolio')
         ON CONFLICT (user_id, url) DO NOTHING",
    )
    .bind(user_id)
    .bind(provider)
    .bind(url)
    .execute(db)
    .await;

    if let Err(err) = result {
        tracing::info!(%err, "portfolio url from the wizard not recorded");
    }
}

/// Save the wizard's answers.
///
/// Replaces the whole object rather than merging. A wizard sends every
/// question it asked, and merging would keep an answer the person has just
/// cleared — which is how somebody who lost access to a GPU keeps being shown
/// challenges they can no longer run.
#[utoipa::path(
    put, path = "/api/users/me/domain-profile/{domain}", tag = "profile",
    params(("domain" = String, Path, description = "Domain slug")),
    request_body = DomainProfileBody,
    responses(
        (status = 200, description = "Saved", body = ApiResponse<DomainProfileResponse>),
        (status = 400, description = "Unknown domain or answer", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn put_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(domain): Path<String>,
    Json(body): Json<DomainProfileBody>,
) -> Result<Json<ApiResponse<DomainProfileResponse>>, AppError> {
    check_domain(&domain)?;

    let asked = questions_for(&domain);

    // Only the answers actually given. A key present with a null value and an
    // absent key read the same to a recommender, and one of them is a lie
    // about having asked.
    let mut answers = serde_json::Map::new();

    for (key, value) in body.answers.iter() {
        if value.is_null() {
            continue;
        }

        // `preferred_families` is asked everywhere and checked against the
        // catalogue, so it is not in the per-domain list.
        if key == "preferred_families" {
            let families = as_string_list(key, value)?;
            check_families(&state.db, &domain, &families).await?;
            // An empty list is stored, and it is not the same as an absent
            // key. "No family in particular" is an answer somebody gave; never
            // opening the wizard is not, and the two have to be tellable apart
            // or the wizard reappears forever for the people who answered it
            // that way.
            answers.insert(key.clone(), json!(families));
            continue;
        }

        let question = common_questions_for(&domain)
            .iter()
            .chain(asked.iter())
            .find(|q| q.key == key)
            .ok_or_else(|| {
                // Naming the owner is the useful half. `compute` means
                // something for AI and nothing for design; `main_tool` is the
                // reverse. A caller who sent one to the wrong wizard has a
                // typo in the path, not in the field, and listing this
                // wizard's keys does not tell them that.
                if let Some(owner) = owning_domain(key) {
                    return AppError::Validation(format!(
                        "'{key}' belongs to the {owner} wizard, not the {domain} one"
                    ));
                }
                let known: Vec<&str> = common_questions_for(&domain)
                    .iter()
                    .chain(asked.iter())
                    .map(|q| q.key)
                    .chain(std::iter::once("preferred_families"))
                    .collect();
                AppError::Validation(format!(
                    "the {domain} wizard does not ask '{key}' — it asks: {}",
                    known.join(", ")
                ))
            })?;

        if question.multi {
            let values = as_string_list(key, value)?;
            if values.len() > MAX_SELECTIONS {
                return Err(AppError::Validation(format!(
                    "at most {MAX_SELECTIONS} answers to '{key}' — picking everything says nothing"
                )));
            }
            for v in &values {
                check_answer(question, v)?;
            }
            if !values.is_empty() {
                answers.insert(key.clone(), json!(values));
            }
        } else {
            let v = value
                .as_str()
                .ok_or_else(|| AppError::Validation(format!("'{key}' must be a string")))?;
            check_answer(question, v)?;
            answers.insert(key.clone(), json!(v));
        }
    }

    let answers = serde_json::Value::Object(answers);

    // No per-domain gate: the loop looks each handle up by key, and a key only
    // one wizard asks is absent from every other wizard's answers. Naming the
    // domain here as well would be a second list to keep in step with the
    // question registry, which is how the code wizard's GitHub handle went
    // missing in the first place.
    if let serde_json::Value::Object(map) = &answers {
        link_declared_handles(&state.db, auth.user_id, map).await;
        claim_portfolio_signal(&state.db, auth.user_id, map).await;
    }

    sqlx::query(
        r#"
        INSERT INTO user_domain_profiles (user_id, domain, answers, completed_at)
        VALUES ($1, $2, $3, NOW())
        ON CONFLICT (user_id, domain) DO UPDATE
            SET answers      = EXCLUDED.answers,
                completed_at = NOW(),
                -- Answering is un-skipping: somebody who said "stop asking"
                -- and then answered has changed their mind.
                skipped_at   = NULL
        "#,
    )
    .bind(auth.user_id)
    .bind(&domain)
    .bind(&answers)
    .execute(&state.db)
    .await?;

    let recommendation = crate::services::onboarding_recommendation::recommend(&domain, &answers);

    Ok(Json(ApiResponse::new(DomainProfileResponse {
        domain,
        answers,
        completed_at: Some(chrono::Utc::now()),
        skipped_at: None,
        recommendation: Some(recommendation),
    })))
}

/// Stop asking.
///
/// Recorded separately from "answered nothing". Without the distinction the
/// wizard reappears forever for exactly the people who least wanted it, and a
/// missing key cannot carry that difference — which is why 0235 keeps these
/// two as columns while the answers stay a blob.
#[utoipa::path(
    post, path = "/api/users/me/domain-profile/{domain}/skip", tag = "profile",
    params(("domain" = String, Path, description = "Domain slug")),
    responses(
        // No content, because there is no true content to send. The write
        // touches `skipped_at` and nothing else, so a body describing the
        // profile would have to either re-read the row or guess at it, and
        // guessing told somebody who had answered and then skipped that they
        // had never answered.
        (status = 204, description = "Recorded"),
        (status = 400, description = "Unknown domain", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn skip_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(domain): Path<String>,
) -> Result<axum::http::StatusCode, AppError> {
    check_domain(&domain)?;

    sqlx::query(
        r#"
        INSERT INTO user_domain_profiles (user_id, domain, skipped_at)
        VALUES ($1, $2, NOW())
        ON CONFLICT (user_id, domain) DO UPDATE SET skipped_at = NOW()
        "#,
    )
    .bind(auth.user_id)
    .bind(&domain)
    .execute(&state.db)
    .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize, ToSchema)]
pub struct QuestionSpec {
    pub key: String,
    /// `single`, `multi` or `text` — whether the answer is one value, several,
    /// or typed.
    ///
    /// Read it together with `allowed`: `multi` with an empty `allowed` and a
    /// `max_len` is several answers of any value, which is what `main_tools`
    /// is. It is `multi` and not `text` because the wire shape is a list, and
    /// a form that reads only this field has to send the shape the wizard
    /// accepts.
    pub answer: String,
    /// The accepted values. Empty where the question has no vocabulary, which
    /// is free text for a `single` question and any value for a `multi` one.
    pub allowed: Vec<String>,
    /// How many answers at most, for a multi-answer question.
    pub max_selections: Option<usize>,
    /// Longest accepted answer, where there is no vocabulary to check against.
    pub max_len: Option<usize>,
}

/// What this domain's wizard asks.
///
/// Exists so a front end renders the form from the platform rather than from
/// its own copy of the list — the copy that goes stale the first time a domain
/// adds a question and nobody tells the web team.
///
/// `preferred_families` is included with its live vocabulary, read from the
/// catalogue: the families of a domain change when an operator edits an
/// orientation, so a constant would be wrong within a release.
#[utoipa::path(
    get, path = "/api/users/me/domain-profile/{domain}/questions", tag = "profile",
    params(("domain" = String, Path, description = "Domain slug")),
    responses(
        (status = 200, description = "Questions", body = ApiResponse<Vec<QuestionSpec>>),
        (status = 400, description = "Unknown domain", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_questions(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(domain): Path<String>,
) -> Result<Json<ApiResponse<Vec<QuestionSpec>>>, AppError> {
    check_domain(&domain)?;

    let mut specs: Vec<QuestionSpec> = common_questions_for(&domain)
        .iter()
        .chain(questions_for(&domain).iter())
        .map(|q| QuestionSpec {
            key: q.key.to_string(),
            // `multi` first. A question can be several-of-anything —
            // `main_tools` is — and testing the vocabulary first called it
            // `text`, which renders as one input, sends a string, and is
            // refused by the validator that wanted a list.
            //
            // The three values still describe every case, because `allowed`
            // and `max_len` are sent alongside: `multi` with an empty
            // `allowed` and a `max_len` is "several, any value, this long".
            answer: if q.multi {
                "multi".into()
            } else if q.allowed.is_empty() {
                "text".into()
            } else {
                "single".into()
            },
            allowed: q.allowed.iter().map(|a| a.to_string()).collect(),
            max_selections: q.multi.then_some(MAX_SELECTIONS),
            max_len: q.allowed.is_empty().then_some(q.max_len),
        })
        .collect();

    // The same function the validator uses, not a second query that says
    // nearly the same thing. See `families_for`.
    let families = families_for(&state.db, &domain).await?;

    // Offered only where the domain has families to offer. A question with an
    // empty vocabulary is one nobody can answer, and showing it makes the
    // wizard look broken rather than short.
    if !families.is_empty() {
        specs.push(QuestionSpec {
            key: "preferred_families".into(),
            answer: "multi".into(),
            allowed: families,
            max_selections: Some(MAX_SELECTIONS),
            max_len: None,
        });
    }

    Ok(Json(ApiResponse::new(specs)))
}

// ═══════════════════════════════════════════════════════════════════
// GET /domains/{domain}/mentors/for-me
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct MentorMatchQuery {
    /// How many suggestions to return. Clamped to 50, because a list nobody
    /// reads to the end is a list that suggests nobody.
    #[serde(default)]
    pub limit: Option<i64>,
}

/// Mentors worth suggesting in one domain, with the reasoning attached.
///
/// One endpoint rather than one per domain. There were seven, each a
/// thirteen-line wrapper around the same `matches_for`, and they had already
/// drifted: some accepted a `limit` and some hardcoded ten, some answered a
/// bare array and some an envelope, and the two domains added last had no
/// endpoint at all — which is the failure mode a copy carries. What differs
/// between domains is the matching rules, and those live in one table of
/// constants the matcher reads.
///
/// The reasoning ships with each suggestion on purpose. A recommendation
/// nobody can argue with is one nobody can correct, and the first thing we
/// will get wrong here is who should be suggested to whom.
#[utoipa::path(
    get, path = "/api/domains/{domain}/mentors/for-me", tag = "mentorship",
    params(
        ("domain" = String, Path, description = "Which domain to look for a mentor in"),
        MentorMatchQuery,
    ),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No such domain, or one with no mentorship rules", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn mentor_matches(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(domain): Path<String>,
    axum::extract::Query(q): axum::extract::Query<MentorMatchQuery>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let rules = crate::services::mentorship_matching::rules_for(&domain).ok_or_else(|| {
        AppError::Validation(format!(
            "no mentorship rules for domain `{domain}` — how many mentees somebody              can carry and what their tools are called differ per domain, and              guessing them would match people badly rather than not at all"
        ))
    })?;

    let limit = q.limit.unwrap_or(10).clamp(1, 50);
    let mentors =
        crate::services::mentorship_matching::matches_for(&state.db, rules, auth.user_id, limit)
            .await?;
    let suggested =
        crate::services::mentorship_matching::could_use_a_mentor(&state.db, &domain, auth.user_id)
            .await?;

    Ok(Json(ApiResponse::new(json!({
        "mentors": mentors,
        "suggested": suggested,
    }))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(domain: &str, key: &str) -> &'static Question {
        common_questions_for(domain)
            .iter()
            .chain(questions_for(domain).iter())
            .find(|q| q.key == key)
            .expect("question exists")
    }

    #[test]
    fn an_answer_outside_the_vocabulary_is_refused() {
        let level = question("ai", "level");
        assert!(check_answer(level, "Senior").is_err());
        // A trailing space is a different string, and a recommender reading it
        // has no branch for it.
        assert!(check_answer(level, "senior ").is_err());
        assert!(check_answer(level, "senior").is_ok());
    }

    #[test]
    fn free_text_is_bounded_and_not_enumerated() {
        let handle = question("audio", "soundcloud_username");
        assert!(check_answer(handle, "someone").is_ok());
        assert!(check_answer(handle, &"x".repeat(200)).is_err());
    }

    #[test]
    fn a_domain_only_accepts_the_questions_it_asks() {
        // `compute` is an AI question. Sending it to the audio wizard used to
        // be silently stored, because the body had a field for it whatever the
        // path said.
        assert!(questions_for("ai").iter().any(|q| q.key == "compute"));
        assert!(!questions_for("audio").iter().any(|q| q.key == "compute"));
        assert!(questions_for("audio").iter().any(|q| q.key == "main_daws"));

        // Code asked nothing here until its own wizard was folded in. Now it
        // asks, and what matters is the same property: its questions are its
        // own and nobody else's.
        assert!(questions_for("code").iter().any(|q| q.key == "main_tools"));
        assert!(
            !questions_for("design")
                .iter()
                .any(|q| q.key == "main_tools")
        );
        assert!(!questions_for("code").iter().any(|q| q.key == "main_tool"));
    }

    #[test]
    fn the_domains_are_the_ones_the_schema_accepts() {
        // Asserted against the table by `test_skill_domains`; this only checks
        // the handler refuses what the database would.
        assert!(check_domain("ai").is_ok());
        assert!(check_domain("audio").is_ok());
        assert!(check_domain("marketing").is_err());
    }

    #[test]
    fn a_list_of_something_else_is_refused_by_name() {
        assert!(as_string_list("main_frameworks", &json!(["jax"])).is_ok());
        assert!(as_string_list("main_frameworks", &json!("jax")).is_err());
        assert!(as_string_list("main_frameworks", &json!([1, 2])).is_err());
    }
}
