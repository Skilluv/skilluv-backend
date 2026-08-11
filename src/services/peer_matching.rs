//! SKI-41 (Post-MVP T2-02) — structured peer-to-peer coaching.
//!
//! ## The matching rule
//!
//! A candidate must share the caller's orientation and sit within one rank
//! of them. Those two are hard filters, not preferences: the whole premise
//! is *peers*, and a Doyen paired with an Apprenti is mentorship wearing a
//! different hat — which the platform already sells, formally and for
//! money, via `mentorship_sessions`.
//!
//! Timezone proximity and shared working languages are soft: they order
//! the candidates rather than excluding them. Excluding on timezone would
//! be actively harmful for the target audience — an orientation with four
//! enrolled people across three continents must still produce a match.
//!
//! ## Scoring
//!
//! Each signal contributes to a score in `[0, 100]`:
//!
//! | signal              | weight | rationale                                    |
//! |---------------------|--------|----------------------------------------------|
//! | rank distance       |     40 | same rank beats ±1                            |
//! | timezone distance   |     35 | a shared working hour is what makes it happen |
//! | language overlap    |     25 | you have to be able to talk                   |
//!
//! Timezone dominates language on purpose: two people who both speak
//! French but are twelve hours apart will never actually meet, whereas two
//! people in the same city with only English in common will.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ranks;

/// How many candidates `propose` returns. Three is the ticket's number and
/// a sensible one: enough to choose, few enough to actually decide.
pub const PROPOSAL_COUNT: i64 = 3;

/// Maximum rank gap tolerated. 1 keeps the pairing peer-to-peer.
const MAX_RANK_DISTANCE: i64 = 1;

const WEIGHT_RANK: f64 = 40.0;
const WEIGHT_TIMEZONE: f64 = 35.0;
const WEIGHT_LANGUAGE: f64 = 25.0;

/// Timezone gap beyond which the signal contributes nothing.
///
/// 8 hours is roughly the point where no shared working hour exists.
const TIMEZONE_HORIZON_HOURS: f64 = 8.0;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Enrollment {
    pub user_id: Uuid,
    pub orientation_id: Uuid,
    pub weekly_cadence: i16,
    pub active: bool,
    pub enrolled_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PeerMatch {
    pub id: Uuid,
    pub user_a: Uuid,
    pub user_b: Uuid,
    pub orientation_id: Uuid,
    pub weekly_cadence: i16,
    pub matched_at: chrono::DateTime<chrono::Utc>,
    pub active: bool,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    pub match_reason: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PeerSession {
    pub id: Uuid,
    pub match_id: Uuid,
    pub session_at: chrono::DateTime<chrono::Utc>,
    pub notes_a: Option<String>,
    pub notes_b: Option<String>,
    pub rating_a: Option<i16>,
    pub rating_b: Option<i16>,
    pub canceled: bool,
    pub canceled_by: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// A candidate peer, with the reasoning behind the score.
#[derive(Debug, Clone, Serialize)]
pub struct Proposal {
    pub user_id: Uuid,
    pub display_name: String,
    pub rank: String,
    pub timezone: Option<String>,
    pub working_languages: Vec<String>,
    pub weekly_cadence: i16,
    /// 0..=100, rounded to one decimal.
    pub score: f64,
    pub reason: ProposalReason,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProposalReason {
    pub rank_distance: i64,
    /// `None` when either side has not declared a timezone.
    pub timezone_distance_hours: Option<f64>,
    pub shared_languages: Vec<String>,
}

/// One row of the candidate pool, as read from the database.
#[derive(Debug, Clone, sqlx::FromRow)]
struct PoolRow {
    user_id: Uuid,
    display_name: String,
    rank: String,
    timezone: Option<String>,
    working_languages: Vec<String>,
    weekly_cadence: i16,
}

/// Parse a timezone into a UTC offset in hours.
///
/// Accepts the fixed-offset forms the profile actually stores (`UTC+2`,
/// `+02:00`, `-5`). Named IANA zones (`Europe/Paris`) return `None`:
/// resolving them needs a tz database, and a wrong offset would order
/// candidates worse than no offset at all — an unknown timezone simply
/// stops contributing to the score instead of contributing a lie.
fn offset_hours(tz: &str) -> Option<f64> {
    let raw = tz.trim();
    let raw = raw
        .strip_prefix("UTC")
        .or_else(|| raw.strip_prefix("utc"))
        .or_else(|| raw.strip_prefix("GMT"))
        .unwrap_or(raw)
        .trim();

    if raw.is_empty() {
        // Bare "UTC" is offset zero.
        return if tz.trim().eq_ignore_ascii_case("utc") || tz.trim().eq_ignore_ascii_case("gmt") {
            Some(0.0)
        } else {
            None
        };
    }

    let (sign, rest) = match raw.strip_prefix('-') {
        Some(r) => (-1.0, r),
        None => (1.0, raw.strip_prefix('+').unwrap_or(raw)),
    };

    let (hours, minutes) = match rest.split_once(':') {
        Some((h, m)) => (h, m.parse::<f64>().ok()?),
        None => (rest, 0.0),
    };
    let hours: f64 = hours.parse().ok()?;
    if !(0.0..=14.0).contains(&hours) || !(0.0..60.0).contains(&minutes) {
        return None;
    }
    Some(sign * (hours + minutes / 60.0))
}

/// Shortest distance between two UTC offsets, in hours.
///
/// The clock wraps: UTC+13 and UTC-11 are two hours apart, not twenty-four.
fn timezone_distance(a: f64, b: f64) -> f64 {
    let raw = (a - b).abs();
    raw.min(24.0 - raw)
}

/// Score a candidate against the caller. Returns `None` when a hard filter
/// rejects them.
fn score_candidate(
    me_rank: &str,
    me_tz: Option<f64>,
    me_langs: &[String],
    candidate: &PoolRow,
) -> Option<(f64, ProposalReason)> {
    let me_pos = ranks::rank_position(me_rank)? as i64;
    let their_pos = ranks::rank_position(&candidate.rank)? as i64;
    let rank_distance = (me_pos - their_pos).abs();
    if rank_distance > MAX_RANK_DISTANCE {
        return None;
    }

    // Rank: full weight at distance 0, half at distance 1.
    let rank_score = WEIGHT_RANK * (1.0 - rank_distance as f64 / (MAX_RANK_DISTANCE + 1) as f64);

    // Timezone: linear decay to the horizon. An undeclared timezone on
    // either side scores as half — neutral, so a user who never filled in
    // their profile is neither rewarded nor buried.
    let their_tz = candidate.timezone.as_deref().and_then(offset_hours);
    let (tz_score, timezone_distance_hours) = match (me_tz, their_tz) {
        (Some(a), Some(b)) => {
            let d = timezone_distance(a, b);
            let closeness = (1.0 - d / TIMEZONE_HORIZON_HOURS).max(0.0);
            (WEIGHT_TIMEZONE * closeness, Some(d))
        }
        _ => (WEIGHT_TIMEZONE * 0.5, None),
    };

    let shared_languages: Vec<String> = candidate
        .working_languages
        .iter()
        .filter(|l| me_langs.contains(l))
        .cloned()
        .collect();
    // Any shared language clears the practical bar; a second one adds
    // little, so this saturates rather than scaling with the count.
    let lang_score = if shared_languages.is_empty() {
        0.0
    } else {
        WEIGHT_LANGUAGE
    };

    let total = ((rank_score + tz_score + lang_score) * 10.0).round() / 10.0;
    Some((
        total,
        ProposalReason {
            rank_distance,
            timezone_distance_hours,
            shared_languages,
        },
    ))
}

/// Enroll (or re-enroll) a user for peer matching on an orientation.
pub async fn enroll(
    db: &PgPool,
    user_id: Uuid,
    orientation_id: Uuid,
    weekly_cadence: i16,
) -> Result<Enrollment, AppError> {
    if !(1..=5).contains(&weekly_cadence) {
        return Err(AppError::Validation(
            "weekly_cadence must be between 1 and 5".into(),
        ));
    }

    // Enrolling for an orientation you are not pursuing would put you in a
    // pool you cannot contribute to.
    let follows: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM user_orientations
              WHERE user_id = $1 AND orientation_id = $2 AND ended_at IS NULL
         )",
    )
    .bind(user_id)
    .bind(orientation_id)
    .fetch_one(db)
    .await?;
    if !follows {
        return Err(AppError::Validation(
            "add this orientation to your profile before enrolling for peer matching".into(),
        ));
    }

    let enrollment: Enrollment = sqlx::query_as(
        r#"
        INSERT INTO peer_matching_enrollments (user_id, orientation_id, weekly_cadence, active)
        VALUES ($1, $2, $3, TRUE)
        ON CONFLICT (user_id, orientation_id) DO UPDATE SET
            weekly_cadence = EXCLUDED.weekly_cadence,
            active         = TRUE,
            updated_at     = NOW()
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(orientation_id)
    .bind(weekly_cadence)
    .fetch_one(db)
    .await?;

    Ok(enrollment)
}

/// Pause matching for an orientation, keeping the chosen cadence.
pub async fn unenroll(db: &PgPool, user_id: Uuid, orientation_id: Uuid) -> Result<(), AppError> {
    let affected = sqlx::query(
        "UPDATE peer_matching_enrollments SET active = FALSE, updated_at = NOW()
          WHERE user_id = $1 AND orientation_id = $2 AND active = TRUE",
    )
    .bind(user_id)
    .bind(orientation_id)
    .execute(db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound("no active enrollment found".into()));
    }
    Ok(())
}

/// The caller's own matching inputs.
async fn load_self(
    db: &PgPool,
    user_id: Uuid,
    orientation_id: Uuid,
) -> Result<(String, Option<f64>, Vec<String>), AppError> {
    let row: Option<(Option<String>, Vec<String>)> = sqlx::query_as(
        "SELECT timezone, working_languages FROM user_orientations
          WHERE user_id = $1 AND orientation_id = $2 AND ended_at IS NULL",
    )
    .bind(user_id)
    .bind(orientation_id)
    .fetch_optional(db)
    .await?;
    let (tz, langs) = row.ok_or_else(|| {
        AppError::Validation("you are not currently following this orientation".into())
    })?;

    let rank: String = sqlx::query_scalar("SELECT rank FROM user_ranks WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(db)
        .await?
        .unwrap_or_else(|| ranks::RANK_APPRENTI.to_string());

    Ok((rank, tz.as_deref().and_then(offset_hours), langs))
}

/// Propose up to [`PROPOSAL_COUNT`] peers for an orientation.
///
/// Excludes the caller, anyone they already have a live match with, and
/// anyone outside the rank window. Ordered by score, best first.
pub async fn propose(
    db: &PgPool,
    user_id: Uuid,
    orientation_id: Uuid,
) -> Result<Vec<Proposal>, AppError> {
    let (me_rank, me_tz, me_langs) = load_self(db, user_id, orientation_id).await?;

    let pool: Vec<PoolRow> = sqlx::query_as(
        r#"
        SELECT e.user_id,
               COALESCE(NULLIF(u.display_name, ''), u.username) AS display_name,
               COALESCE(r.rank, 'apprenti')                     AS rank,
               uo.timezone,
               uo.working_languages,
               e.weekly_cadence
          FROM peer_matching_enrollments e
          JOIN users u  ON u.id = e.user_id
          JOIN user_orientations uo
                        ON uo.user_id = e.user_id
                       AND uo.orientation_id = e.orientation_id
                       AND uo.ended_at IS NULL
          LEFT JOIN user_ranks r ON r.user_id = e.user_id
         WHERE e.orientation_id = $1
           AND e.active = TRUE
           AND e.user_id <> $2
           AND u.is_banned = FALSE
           -- Already paired for this orientation: nothing to propose.
           AND NOT EXISTS (
               SELECT 1 FROM peer_matches m
                WHERE m.active = TRUE
                  AND m.orientation_id = $1
                  AND (   (m.user_a = $2 AND m.user_b = e.user_id)
                       OR (m.user_b = $2 AND m.user_a = e.user_id))
           )
           -- Blocks cut both ways.
           AND NOT EXISTS (
               SELECT 1 FROM user_blocks b
                WHERE (b.blocker_id = $2 AND b.blocked_id = e.user_id)
                   OR (b.blocker_id = e.user_id AND b.blocked_id = $2)
           )
        "#,
    )
    .bind(orientation_id)
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let mut scored: Vec<Proposal> = pool
        .into_iter()
        .filter_map(|c| {
            let (score, reason) = score_candidate(&me_rank, me_tz, &me_langs, &c)?;
            Some(Proposal {
                user_id: c.user_id,
                display_name: c.display_name,
                rank: c.rank,
                timezone: c.timezone,
                working_languages: c.working_languages,
                weekly_cadence: c.weekly_cadence,
                score,
                reason,
            })
        })
        .collect();

    // Descending score; user_id as a stable tiebreak so repeated calls
    // return the same order for the same data.
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.user_id.cmp(&b.user_id))
    });
    scored.truncate(PROPOSAL_COUNT as usize);
    Ok(scored)
}

/// Create a match between the caller and a proposed peer.
///
/// Re-runs the proposal query rather than trusting the client: a stale
/// proposal list must not be replayable into a pairing that the rules no
/// longer allow.
pub async fn create_match(
    db: &PgPool,
    user_id: Uuid,
    peer_id: Uuid,
    orientation_id: Uuid,
) -> Result<PeerMatch, AppError> {
    if user_id == peer_id {
        return Err(AppError::Validation("cannot match with yourself".into()));
    }

    let proposals = propose(db, user_id, orientation_id).await?;
    let chosen = proposals
        .into_iter()
        .find(|p| p.user_id == peer_id)
        .ok_or_else(|| {
            AppError::Validation(
                "this peer is no longer a valid match — request fresh proposals".into(),
            )
        })?;

    // Ordered pair, matching the CHECK in migration 0144.
    let (a, b) = if user_id < peer_id {
        (user_id, peer_id)
    } else {
        (peer_id, user_id)
    };

    let reason = serde_json::json!({
        "score": chosen.score,
        "rank_distance": chosen.reason.rank_distance,
        "timezone_distance_hours": chosen.reason.timezone_distance_hours,
        "shared_languages": chosen.reason.shared_languages,
    });

    let created: Result<PeerMatch, sqlx::Error> = sqlx::query_as(
        r#"
        INSERT INTO peer_matches
            (user_a, user_b, orientation_id, weekly_cadence, match_reason)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING *
        "#,
    )
    .bind(a)
    .bind(b)
    .bind(orientation_id)
    .bind(chosen.weekly_cadence)
    .bind(&reason)
    .fetch_one(db)
    .await;

    match created {
        Ok(m) => Ok(m),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(AppError::Conflict(
            "you already have a live match with this peer for this orientation".into(),
        )),
        Err(e) => Err(e.into()),
    }
}

/// Load a match the caller belongs to, or 404.
pub async fn get_match_for(
    db: &PgPool,
    match_id: Uuid,
    user_id: Uuid,
) -> Result<PeerMatch, AppError> {
    sqlx::query_as(
        "SELECT * FROM peer_matches
          WHERE id = $1 AND (user_a = $2 OR user_b = $2)",
    )
    .bind(match_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("peer match {match_id} not found")))
}

/// End a match. Either side may, unilaterally — requiring consent to stop
/// would make it harder to leave than to join.
pub async fn end_match(db: &PgPool, match_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
    let m = get_match_for(db, match_id, user_id).await?;
    if !m.active {
        return Err(AppError::Conflict("match already ended".into()));
    }
    sqlx::query("UPDATE peer_matches SET active = FALSE, ended_at = NOW() WHERE id = $1")
        .bind(match_id)
        .execute(db)
        .await?;
    Ok(())
}

/// Schedule a session on a live match.
pub async fn schedule_session(
    db: &PgPool,
    match_id: Uuid,
    user_id: Uuid,
    session_at: chrono::DateTime<chrono::Utc>,
) -> Result<PeerSession, AppError> {
    let m = get_match_for(db, match_id, user_id).await?;
    if !m.active {
        return Err(AppError::Conflict(
            "cannot schedule a session on an ended match".into(),
        ));
    }

    let session: PeerSession = sqlx::query_as(
        "INSERT INTO peer_sessions (match_id, session_at) VALUES ($1, $2) RETURNING *",
    )
    .bind(match_id)
    .bind(session_at)
    .fetch_one(db)
    .await?;
    Ok(session)
}

/// A participant's check-in on a session.
///
/// The caller's side is derived from the match, never from the request:
/// each participant can only ever write their own notes and rating.
pub async fn check_in(
    db: &PgPool,
    session_id: Uuid,
    user_id: Uuid,
    notes: Option<&str>,
    rating: Option<i16>,
) -> Result<PeerSession, AppError> {
    if let Some(r) = rating
        && !(1..=5).contains(&r)
    {
        return Err(AppError::Validation(
            "rating must be between 1 and 5".into(),
        ));
    }
    if let Some(n) = notes
        && n.chars().count() > 4000
    {
        return Err(AppError::Validation(
            "notes must be at most 4000 characters".into(),
        ));
    }

    let row: Option<(Uuid, Uuid, Uuid, bool)> = sqlx::query_as(
        "SELECT s.id, m.user_a, m.user_b, s.canceled
           FROM peer_sessions s JOIN peer_matches m ON m.id = s.match_id
          WHERE s.id = $1 AND (m.user_a = $2 OR m.user_b = $2)",
    )
    .bind(session_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    let (_, user_a, _, canceled) =
        row.ok_or_else(|| AppError::NotFound(format!("session {session_id} not found")))?;
    if canceled {
        return Err(AppError::Conflict(
            "cannot check in on a canceled session".into(),
        ));
    }

    let is_side_a = user_a == user_id;
    let session: PeerSession = sqlx::query_as(
        r#"
        UPDATE peer_sessions SET
            notes_a  = CASE WHEN $2 THEN COALESCE($3, notes_a)  ELSE notes_a  END,
            notes_b  = CASE WHEN $2 THEN notes_b                ELSE COALESCE($3, notes_b) END,
            rating_a = CASE WHEN $2 THEN COALESCE($4, rating_a) ELSE rating_a END,
            rating_b = CASE WHEN $2 THEN rating_b               ELSE COALESCE($4, rating_b) END
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(session_id)
    .bind(is_side_a)
    .bind(notes)
    .bind(rating)
    .fetch_one(db)
    .await?;

    Ok(session)
}

/// Cancel a session. Either participant may.
pub async fn cancel_session(
    db: &PgPool,
    session_id: Uuid,
    user_id: Uuid,
) -> Result<PeerSession, AppError> {
    let belongs: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM peer_sessions s JOIN peer_matches m ON m.id = s.match_id
              WHERE s.id = $1 AND (m.user_a = $2 OR m.user_b = $2)
         )",
    )
    .bind(session_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;
    if !belongs {
        return Err(AppError::NotFound(format!(
            "session {session_id} not found"
        )));
    }

    let session: PeerSession = sqlx::query_as(
        "UPDATE peer_sessions SET canceled = TRUE, canceled_by = $2
          WHERE id = $1 RETURNING *",
    )
    .bind(session_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;
    Ok(session)
}

#[cfg(test)]
mod unit {
    use super::*;

    fn pool_row(rank: &str, tz: Option<&str>, langs: &[&str]) -> PoolRow {
        PoolRow {
            user_id: Uuid::new_v4(),
            display_name: "Peer".into(),
            rank: rank.into(),
            timezone: tz.map(str::to_string),
            working_languages: langs.iter().map(|s| s.to_string()).collect(),
            weekly_cadence: 1,
        }
    }

    #[test]
    fn offsets_parse_the_forms_profiles_actually_store() {
        assert_eq!(offset_hours("UTC+2"), Some(2.0));
        assert_eq!(offset_hours("utc-5"), Some(-5.0));
        assert_eq!(offset_hours("+02:00"), Some(2.0));
        assert_eq!(offset_hours("-03:30"), Some(-3.5));
        assert_eq!(offset_hours("UTC"), Some(0.0));
        assert_eq!(offset_hours("0"), Some(0.0));
        // Named zones need a tz database; a guess would be worse than
        // abstaining.
        assert_eq!(offset_hours("Europe/Paris"), None);
        assert_eq!(offset_hours("nonsense"), None);
        assert_eq!(offset_hours("+99"), None);
    }

    #[test]
    fn timezone_distance_wraps_around_the_clock() {
        assert_eq!(timezone_distance(2.0, 2.0), 0.0);
        assert_eq!(timezone_distance(2.0, 5.0), 3.0);
        // The short way round: 13 and -11 are the same side of midnight.
        assert_eq!(timezone_distance(13.0, -11.0), 0.0);
        assert_eq!(timezone_distance(12.0, -11.0), 1.0);
    }

    #[test]
    fn rank_gap_beyond_one_is_rejected_outright() {
        let me = ranks::RANK_APPRENTI;
        let langs = vec!["fr".to_string()];
        assert!(
            score_candidate(
                me,
                Some(0.0),
                &langs,
                &pool_row(ranks::RANK_RANGER, Some("UTC"), &["fr"])
            )
            .is_some(),
            "one rank apart is still peers"
        );
        assert!(
            score_candidate(
                me,
                Some(0.0),
                &langs,
                &pool_row(ranks::RANK_ARTISAN, Some("UTC"), &["fr"])
            )
            .is_none(),
            "two ranks apart is mentorship, which is a different product"
        );
    }

    #[test]
    fn same_rank_same_timezone_shared_language_scores_full_marks() {
        let langs = vec!["fr".to_string(), "en".to_string()];
        let (score, reason) = score_candidate(
            ranks::RANK_RANGER,
            Some(1.0),
            &langs,
            &pool_row(ranks::RANK_RANGER, Some("UTC+1"), &["fr"]),
        )
        .expect("valid candidate");
        assert_eq!(score, 100.0);
        assert_eq!(reason.rank_distance, 0);
        assert_eq!(reason.timezone_distance_hours, Some(0.0));
        assert_eq!(reason.shared_languages, vec!["fr".to_string()]);
    }

    #[test]
    fn distant_timezone_and_no_shared_language_still_ranks_but_low() {
        let langs = vec!["fr".to_string()];
        let (score, reason) = score_candidate(
            ranks::RANK_RANGER,
            Some(0.0),
            &langs,
            &pool_row(ranks::RANK_RANGER, Some("UTC+10"), &["ja"]),
        )
        .expect("still a valid peer");
        // Rank 40 + timezone 0 (past the horizon) + language 0.
        assert_eq!(score, 40.0);
        assert!(reason.shared_languages.is_empty());
        assert!(
            score > 0.0,
            "a distant peer must remain proposable — a thin pool must still match"
        );
    }

    #[test]
    fn undeclared_timezone_is_neutral_not_penalised() {
        let langs = vec!["fr".to_string()];
        let (known, _) = score_candidate(
            ranks::RANK_RANGER,
            Some(0.0),
            &langs,
            &pool_row(ranks::RANK_RANGER, Some("UTC+10"), &["fr"]),
        )
        .unwrap();
        let (unknown, reason) = score_candidate(
            ranks::RANK_RANGER,
            Some(0.0),
            &langs,
            &pool_row(ranks::RANK_RANGER, None, &["fr"]),
        )
        .unwrap();
        assert!(
            unknown > known,
            "an unstated timezone must not score worse than a known-bad one"
        );
        assert_eq!(reason.timezone_distance_hours, None);
    }

    #[test]
    fn timezone_outweighs_language() {
        let langs = vec!["fr".to_string()];
        // Same timezone, no shared language.
        let (near_no_lang, _) = score_candidate(
            ranks::RANK_RANGER,
            Some(0.0),
            &langs,
            &pool_row(ranks::RANK_RANGER, Some("UTC"), &["ja"]),
        )
        .unwrap();
        // Far timezone, shared language.
        let (far_shared_lang, _) = score_candidate(
            ranks::RANK_RANGER,
            Some(0.0),
            &langs,
            &pool_row(ranks::RANK_RANGER, Some("UTC+12"), &["fr"]),
        )
        .unwrap();
        assert!(
            near_no_lang > far_shared_lang,
            "two people who can actually meet beat two who merely share a language"
        );
    }
}
