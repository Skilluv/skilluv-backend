//! Matching a code mentee with a code mentor.
//!
//! Generic mentorship matching asks "who is available and knows this area".
//! For code that is not enough: thirty-three trades and a dozen ecosystems
//! mean "knows this area" can be true and useless — a kernel engineer is not
//! the right person to review somebody's first React component, and both of
//! them would find that out an hour into a paid session.
//!
//! ## Where the scoring lives
//!
//! In `services::mentorship_matching`. The five questions below are the same
//! five every domain asks, and copied they would diverge silently — a wrong
//! ordering still looks like a list of plausible people. What stays here is
//! the one thing that genuinely differs: where a person's families and
//! vocabulary come from, which for code is columns on `users`.
//!
//! ## What the score is made of, and why each part is there
//!
//! * **Family.** The strongest signal, and the one that makes a session
//!   useful at all.
//! * **Language.** A bonus rather than a requirement: a good mentor in a
//!   neighbouring language beats a mediocre one in the same.
//! * **Distance.** A mentor should be well ahead, not adjacent — somebody a
//!   hundred points above you has nothing to teach yet — and not so far ahead
//!   that the conversation has no common ground.
//! * **Timezone.** Three hours is roughly the widest window where two people
//!   can find an hour that is not the middle of somebody's night.
//! * **Load.** A mentor already carrying five people is not available,
//!   whatever their calendar says.
//!
//! Every part is returned with the match, because a mentee who can see why
//! somebody was suggested can tell us it was wrong.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::mentorship_matching::{self as matching, Match, Weights};

/// The code domain's thresholds.
///
/// Five mentees rather than the default three: this is the most populated
/// domain, and a lower cap would leave most people unmatched.
pub const WEIGHTS: Weights = Weights {
    min_score_gap: 500,
    max_timezone_gap_hours: 3,
    max_active_mentees: 5,
};

/// What the person looking for a mentor said about themselves.
#[derive(sqlx::FromRow)]
struct Mentee {
    craft_score_code: i32,
    code_preferred_families: Vec<String>,
    code_main_languages: Vec<String>,
    timezone: Option<String>,
}

#[derive(sqlx::FromRow)]
struct Candidate {
    user_id: Uuid,
    username: String,
    headline: String,
    craft_score_code: i32,
    families: Vec<String>,
    languages: Vec<String>,
    timezone: Option<String>,
    active_mentees: i64,
}

/// Mentors worth suggesting to this person, best first.
pub async fn matches_for(db: &PgPool, mentee_id: Uuid, limit: i64) -> Result<Vec<Match>, AppError> {
    // A mentee with no computed score reads as zero here, which is the right
    // answer: the gap to a mentor is then the mentor's whole score, and
    // somebody with nothing proved has the most to learn.
    let mentee: Option<Mentee> = sqlx::query_as(
        "SELECT COALESCE(cs.score, 0) AS craft_score_code,
                u.code_preferred_families, u.code_main_languages, u.timezone
           FROM users u
           LEFT JOIN craft_scores cs ON cs.user_id = u.id AND cs.skill_domain = $2
          WHERE u.id = $1",
    )
    .bind(mentee_id)
    .bind(crate::services::craft_score::DOMAIN)
    .fetch_optional(db)
    .await?;
    let Mentee {
        craft_score_code: mentee_score,
        code_preferred_families: mentee_families,
        code_main_languages: mentee_languages,
        timezone: mentee_timezone,
    } = mentee.ok_or_else(|| AppError::NotFound("user not found".into()))?;

    if mentee_families.is_empty() {
        return Err(AppError::Validation(
            "answer the code onboarding first — without a family there is nothing to match on"
                .into(),
        ));
    }

    let candidates = sqlx::query_as::<_, Candidate>(
        r#"
        SELECT u.id AS user_id,
               u.username,
               m.headline,
               cs.score AS craft_score_code,
               u.code_preferred_families AS families,
               -- What the mentor works in: their declared languages, and the
               -- expertise they wrote on their mentor profile. Both, because
               -- somebody who filled in one rarely filled in the other.
               ARRAY(
                   SELECT DISTINCT lower(l)
                     FROM unnest(u.code_main_languages || m.expertise_areas) AS l
               ) AS languages,
               u.timezone,
               (SELECT count(DISTINCT s.mentee_user_id)
                  FROM mentorship_sessions s
                 WHERE s.mentor_user_id = u.id
                   -- Booked and not yet over. `completed` is deliberately
                   -- absent: a session that happened is history, not a
                   -- mentee somebody is still carrying.
                   AND s.status IN ('paid', 'confirmed')
                   AND s.scheduled_at > NOW() - INTERVAL '60 days')
                   AS active_mentees
          FROM mentor_profiles m
          JOIN users u ON u.id = m.user_id
          -- An inner join: a mentor with no computed score has proved
          -- nothing in this domain, and suggesting them would be suggesting
          -- somebody on the strength of having a mentor profile.
          JOIN craft_scores cs ON cs.user_id = u.id AND cs.skill_domain = $3
         WHERE m.active = TRUE
           AND u.is_banned = FALSE
           AND u.id <> $1
           AND cs.score >= $2
        "#,
    )
    .bind(mentee_id)
    .bind(mentee_score + WEIGHTS.min_score_gap)
    .bind(crate::services::craft_score::DOMAIN)
    .fetch_all(db)
    .await?;

    let mentee_languages: Vec<String> = mentee_languages.iter().map(|l| l.to_lowercase()).collect();

    let mut matches: Vec<Match> = candidates
        .into_iter()
        .map(|c| {
            let shared_families: Vec<String> = c
                .families
                .iter()
                .filter(|f| mentee_families.contains(f))
                .cloned()
                .collect();
            let shared_languages: Vec<String> = c
                .languages
                .iter()
                .filter(|l| mentee_languages.contains(l))
                .cloned()
                .collect();
            let gap = matching::timezone_gap(mentee_timezone.as_deref(), c.timezone.as_deref());
            let score_gap = c.craft_score_code - mentee_score;

            let score = matching::score_candidate(
                shared_families.len(),
                shared_languages.len(),
                score_gap,
                gap,
                c.active_mentees,
                WEIGHTS,
            );

            let because = matching::reasons(
                &shared_families,
                &shared_languages,
                "tes langages",
                score_gap,
                gap,
                c.active_mentees,
                WEIGHTS,
            );

            Match {
                mentor_user_id: c.user_id,
                username: c.username,
                headline: c.headline,
                craft_score: c.craft_score_code,
                score,
                shared_families,
                shared_vocabulary: shared_languages,
                timezone_gap_hours: gap,
                active_mentees: c.active_mentees,
                because,
            }
        })
        .collect();

    // A zero is a refusal, not a weak suggestion: showing a kernel engineer to
    // somebody learning React wastes an hour of both.
    matching::rank(&mut matches, limit);
    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::mentorship_matching::score_candidate;

    // The scoring itself is tested in `mentorship_matching`, where it lives.
    // What is worth pinning here is the choice this domain made.

    #[test]
    fn a_code_mentor_carries_more_people_than_the_default() {
        // The most populated domain: a lower cap would leave most people
        // unmatched.
        assert_eq!(WEIGHTS.max_active_mentees, 5);
        assert!(WEIGHTS.max_active_mentees > Weights::default_for_domain().max_active_mentees);
    }

    #[test]
    fn a_shared_language_helps_and_does_not_decide() {
        let same_language_one_family = score_candidate(1, 1, 1000, Some(0), 0, WEIGHTS);
        let two_families_no_language = score_candidate(2, 0, 1000, Some(0), 0, WEIGHTS);
        assert!(
            two_families_no_language > same_language_one_family,
            "a good mentor in a neighbouring language beats a mediocre one in the same"
        );
    }

    #[test]
    fn the_fifth_mentee_is_the_last_one() {
        assert!(score_candidate(2, 2, 2000, Some(0), 4, WEIGHTS) > 0);
        assert_eq!(score_candidate(2, 2, 2000, Some(0), 5, WEIGHTS), 0);
    }
}
