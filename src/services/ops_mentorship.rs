//! Matching an ops mentee with an ops mentor.
//!
//! The shape follows `code_mentorship` — shared area, distance, timezone,
//! load — with one dimension that domain has no equivalent for, and it is
//! the reason this is a separate module rather than a parameter.
//!
//! ## On-call cannot be taught by somebody who has never done it
//!
//! Half of what a junior needs here is not technical: what to do first at
//! three in the morning, when to escalate, how to write the message that goes
//! to customers while the system is still down. Somebody who has never held
//! a pager can teach Terraform perfectly well and cannot teach that.
//!
//! So a mentee whose objective involves on-call is matched only against
//! mentors who have actually done it, and the reason is shown to them. The
//! alternative — matching on skill alone and letting both discover it an hour
//! into a paid session — is the failure this module exists to prevent.
//!
//! ## Why cloud experience is a bonus and not a filter
//!
//! A good SRE on GCP is a better mentor than a mediocre one on AWS. The
//! platforms differ in their vocabulary and agree on almost everything that
//! matters, and filtering on them would empty the list in exactly the regions
//! where mentors are scarcest.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::code_mentorship::{MAX_TIMEZONE_GAP_HOURS, timezone_gap};

/// How far ahead a mentor should be. Below this the conversation is between
/// peers, which is valuable and is not mentorship.
pub const MIN_SCORE_GAP: i32 = 400;

/// Lower than the code domain's five: an ops mentee often arrives with a
/// system that is on fire, and a mentor carrying four of those is carrying
/// four emergencies.
pub const MAX_ACTIVE_MENTEES: i64 = 4;

/// Who counts as having held a pager.
pub const HAS_HELD_A_PAGER: &[&str] = &["occasional", "regular", "always_on"];

#[derive(Debug, Clone, Serialize)]
pub struct Match {
    pub mentor_user_id: Uuid,
    pub username: String,
    pub headline: String,
    pub craft_score_ops: i32,
    pub score: i32,
    pub shared_trades: Vec<String>,
    pub shared_platforms: Vec<String>,
    pub mentor_oncall_experience: Option<String>,
    pub timezone_gap_hours: Option<i32>,
    pub active_mentees: i64,
    pub because: Vec<String>,
}

#[derive(sqlx::FromRow)]
struct Mentee {
    craft_score_ops: i32,
    ops_trades: Vec<String>,
    ops_cloud_experience: Vec<String>,
    ops_objective: Option<String>,
    timezone: Option<String>,
}

#[derive(sqlx::FromRow)]
struct Candidate {
    user_id: Uuid,
    username: String,
    headline: String,
    craft_score_ops: i32,
    trades: Vec<String>,
    platforms: Vec<String>,
    oncall_experience: Option<String>,
    timezone: Option<String>,
    active_mentees: i64,
}

/// Score one candidate. Pure and tested: a wrong ordering still looks like a
/// list of plausible people, which is how a bug here survives.
pub fn score_candidate(
    shared_trades: usize,
    shared_platforms: usize,
    score_gap: i32,
    timezone_gap: Option<i32>,
    active_mentees: i64,
    needs_oncall: bool,
    mentor_held_a_pager: bool,
) -> i32 {
    if shared_trades == 0 {
        return 0;
    }
    if score_gap < MIN_SCORE_GAP {
        return 0;
    }
    if active_mentees >= MAX_ACTIVE_MENTEES {
        return 0;
    }
    // The one hard refusal this domain adds. Not a penalty: a mentor who has
    // never been woken up cannot teach being woken up, however good they are.
    if needs_oncall && !mentor_held_a_pager {
        return 0;
    }

    let mut score = 100 * shared_trades as i32;
    score += 30 * shared_platforms as i32;
    score += ((score_gap as f64 / 100.0).sqrt() * 30.0).round() as i32;

    match timezone_gap {
        Some(gap) if gap <= MAX_TIMEZONE_GAP_HOURS => score += 60 - 15 * gap,
        Some(_) => score -= 40,
        None => {}
    }

    score += 10 * (MAX_ACTIVE_MENTEES - active_mentees) as i32;
    score.max(0)
}

/// Mentors worth suggesting to this person, best first.
pub async fn matches_for(db: &PgPool, mentee_id: Uuid, limit: i64) -> Result<Vec<Match>, AppError> {
    let mentee: Option<Mentee> = sqlx::query_as(
        "SELECT COALESCE(cs.score, 0) AS craft_score_ops,
                u.ops_trades, u.ops_cloud_experience, u.ops_objective, u.timezone
           FROM users u
           LEFT JOIN craft_scores cs ON cs.user_id = u.id AND cs.skill_domain = 'ops'
          WHERE u.id = $1",
    )
    .bind(mentee_id)
    .fetch_optional(db)
    .await?;

    let mentee = mentee.ok_or_else(|| AppError::NotFound("user not found".into()))?;

    if mentee.ops_trades.is_empty() {
        return Err(AppError::Validation(
            "answer the ops onboarding first — without a trade there is nothing to \
             match on"
                .into(),
        ));
    }

    // Somebody who wants paid work in this domain will meet on-call, whether
    // or not they went looking for it, so they are matched as if they need
    // that half of the teaching.
    let needs_oncall = matches!(
        mentee.ops_objective.as_deref(),
        Some("find_paid_work") | Some("start_own_practice")
    );

    let candidates = sqlx::query_as::<_, Candidate>(
        r#"
        SELECT u.id AS user_id,
               u.username,
               m.headline,
               cs.score AS craft_score_ops,
               u.ops_trades AS trades,
               ARRAY(
                   SELECT DISTINCT lower(p)
                     FROM unnest(u.ops_cloud_experience || m.expertise_areas) AS p
               ) AS platforms,
               u.ops_oncall_experience AS oncall_experience,
               u.timezone,
               (SELECT count(DISTINCT s.mentee_user_id)
                  FROM mentorship_sessions s
                 WHERE s.mentor_user_id = u.id
                   AND s.status IN ('paid', 'confirmed')
                   AND s.scheduled_at > NOW() - INTERVAL '60 days')
                   AS active_mentees
          FROM mentor_profiles m
          JOIN users u ON u.id = m.user_id
          -- Inner join: a mentor with no ops score has proved nothing in this
          -- domain, and suggesting them would be suggesting somebody on the
          -- strength of having a mentor profile.
          JOIN craft_scores cs ON cs.user_id = u.id AND cs.skill_domain = 'ops'
         WHERE m.active = TRUE
           AND u.is_banned = FALSE
           AND u.id <> $1
           AND cs.score >= $2
        "#,
    )
    .bind(mentee_id)
    .bind(mentee.craft_score_ops + MIN_SCORE_GAP)
    .fetch_all(db)
    .await?;

    let mentee_platforms: Vec<String> = mentee
        .ops_cloud_experience
        .iter()
        .map(|p| p.to_lowercase())
        .collect();

    let mut matches: Vec<Match> = candidates
        .into_iter()
        .map(|c| {
            let shared_trades: Vec<String> = c
                .trades
                .iter()
                .filter(|t| mentee.ops_trades.contains(t))
                .cloned()
                .collect();
            let shared_platforms: Vec<String> = c
                .platforms
                .iter()
                .filter(|p| mentee_platforms.contains(p))
                .cloned()
                .collect();

            let gap = timezone_gap(mentee.timezone.as_deref(), c.timezone.as_deref());
            let score_gap = c.craft_score_ops - mentee.craft_score_ops;
            let held_a_pager = c
                .oncall_experience
                .as_deref()
                .is_some_and(|e| HAS_HELD_A_PAGER.contains(&e));

            let score = score_candidate(
                shared_trades.len(),
                shared_platforms.len(),
                score_gap,
                gap,
                c.active_mentees,
                needs_oncall,
                held_a_pager,
            );

            let mut because = Vec::new();
            if !shared_trades.is_empty() {
                because.push(format!(
                    "Exerce le même métier que toi : {}.",
                    shared_trades.join(", ")
                ));
            }
            if !shared_platforms.is_empty() {
                because.push(format!(
                    "Connaît les mêmes plateformes : {}.",
                    shared_platforms.join(", ")
                ));
            }
            if needs_oncall && held_a_pager {
                because.push(
                    "A vraiment été d'astreinte — la moitié de ce qu'il y a à apprendre \
                     ici ne s'enseigne pas autrement."
                        .into(),
                );
            }
            because.push(format!(
                "{score_gap} points d'avance : assez pour t'apprendre quelque chose, pas \
                 assez pour parler une autre langue."
            ));
            match gap {
                Some(0) => because.push("Même fuseau horaire.".into()),
                Some(g) if g <= MAX_TIMEZONE_GAP_HOURS => {
                    because.push(format!("{g} h de décalage : trouvable."))
                }
                Some(g) => because.push(format!("{g} h de décalage : à organiser.")),
                None => {}
            }
            if c.active_mentees == 0 {
                because.push("N'accompagne personne en ce moment.".into());
            }

            Match {
                mentor_user_id: c.user_id,
                username: c.username,
                headline: c.headline,
                craft_score_ops: c.craft_score_ops,
                score,
                shared_trades,
                shared_platforms,
                mentor_oncall_experience: c.oncall_experience,
                timezone_gap_hours: gap,
                active_mentees: c.active_mentees,
                because,
            }
        })
        // A zero is a refusal, not a weak suggestion.
        .filter(|m| m.score > 0)
        .collect();

    matches.sort_by(|a, b| b.score.cmp(&a.score).then(a.username.cmp(&b.username)));
    matches.truncate(limit.clamp(1, 50) as usize);
    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_different_trade_is_not_a_match_at_any_price() {
        assert_eq!(score_candidate(0, 3, 5000, Some(0), 0, false, true), 0);
    }

    #[test]
    fn somebody_adjacent_has_nothing_to_teach_yet() {
        assert_eq!(
            score_candidate(1, 1, MIN_SCORE_GAP - 1, Some(0), 0, false, true),
            0
        );
        assert!(score_candidate(1, 1, MIN_SCORE_GAP, Some(0), 0, false, true) > 0);
    }

    #[test]
    fn a_mentor_who_never_held_a_pager_cannot_teach_on_call() {
        // The one refusal this domain adds. Everything else about this
        // candidate is ideal.
        assert_eq!(score_candidate(2, 2, 3000, Some(0), 0, true, false), 0);
        assert!(score_candidate(2, 2, 3000, Some(0), 0, true, true) > 0);

        // And it is a refusal only when the mentee needs that teaching: a
        // mentor who has never been on call is still an excellent Terraform
        // mentor.
        assert!(score_candidate(2, 2, 3000, Some(0), 0, false, false) > 0);
    }

    #[test]
    fn a_full_mentor_is_not_available_whatever_the_calendar_says() {
        assert_eq!(
            score_candidate(2, 2, 3000, Some(0), MAX_ACTIVE_MENTEES, false, true),
            0
        );
    }

    #[test]
    fn the_platform_bonus_never_outweighs_the_trade() {
        // Somebody in your trade on a different cloud beats somebody in a
        // different trade on yours — the second is already zero, so this
        // checks the ordering among real matches.
        let same_trade_other_cloud = score_candidate(1, 0, 1000, Some(0), 0, false, true);
        let same_trade_same_cloud = score_candidate(1, 3, 1000, Some(0), 0, false, true);
        assert!(same_trade_same_cloud > same_trade_other_cloud);
        assert!(same_trade_other_cloud > 0);
    }
}
