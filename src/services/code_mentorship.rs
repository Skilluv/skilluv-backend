//! Matching a code mentee with a code mentor.
//!
//! Generic mentorship matching asks "who is available and knows this area".
//! For code that is not enough: thirty-three trades and a dozen ecosystems
//! mean "knows this area" can be true and useless — a kernel engineer is not
//! the right person to review somebody's first React component, and both of
//! them would find that out an hour into a paid session.
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

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// How far ahead a mentor should be. Below this the conversation is between
/// peers, which is valuable and is not mentorship.
pub const MIN_SCORE_GAP: i32 = 500;

/// Beyond three hours, "let us find an hour" means somebody's midnight.
pub const MAX_TIMEZONE_GAP_HOURS: i32 = 3;

/// More than five active mentees and the sixth gets what is left over.
/// Higher than the other domains' because this is the most populated one, and
/// a lower cap would leave most people unmatched.
pub const MAX_ACTIVE_MENTEES: i64 = 5;

#[derive(Debug, Clone, Serialize)]
pub struct Match {
    pub mentor_user_id: Uuid,
    pub username: String,
    pub headline: String,
    pub craft_score_code: i32,
    pub score: i32,
    /// Which families you have in common.
    pub shared_families: Vec<String>,
    pub shared_languages: Vec<String>,
    pub timezone_gap_hours: Option<i32>,
    pub active_mentees: i64,
    /// The reasoning, in sentences. A mentee who can read why somebody was
    /// suggested can tell us it was wrong.
    pub because: Vec<String>,
}

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

/// Hours east of UTC, from an offset string like `+02:00` or `-05:00`.
///
/// Deliberately narrow: this reads what the onboarding stores, and anything
/// else — an IANA name, a city — answers `None` rather than a guess. A wrong
/// offset would suggest mentors in the wrong half of the planet, which is
/// worse than suggesting nobody.
pub fn utc_offset_hours(timezone: &str) -> Option<i32> {
    let trimmed = timezone.trim();
    let (sign, rest) = match trimmed.strip_prefix('+') {
        Some(rest) => (1, rest),
        None => (-1, trimmed.strip_prefix('-')?),
    };
    let hours: i32 = rest.split(':').next()?.parse().ok()?;
    (0..=14).contains(&hours).then_some(sign * hours)
}

/// How far apart two offsets are, in hours.
pub fn timezone_gap(a: Option<&str>, b: Option<&str>) -> Option<i32> {
    let a = utc_offset_hours(a?)?;
    let b = utc_offset_hours(b?)?;
    Some((a - b).abs())
}

/// Score one candidate against one mentee.
///
/// Pure and tested: this is where a mistake is silent, because a wrong
/// ordering still looks like a list of plausible people.
#[allow(clippy::too_many_arguments)]
pub fn score_candidate(
    shared_families: usize,
    shared_languages: usize,
    score_gap: i32,
    timezone_gap: Option<i32>,
    active_mentees: i64,
) -> i32 {
    // Nothing in common in the family is not a match at any price.
    if shared_families == 0 {
        return 0;
    }
    // Somebody adjacent to you has nothing to teach yet.
    if score_gap < MIN_SCORE_GAP {
        return 0;
    }
    // Already carrying as many as they can.
    if active_mentees >= MAX_ACTIVE_MENTEES {
        return 0;
    }

    let mut score = 100 * shared_families as i32;
    score += 40 * shared_languages as i32;

    // The gap helps, with diminishing returns: three thousand points ahead is
    // not six times better than five hundred, and past a point the
    // conversation loses its common ground.
    score += ((score_gap as f64 / 100.0).sqrt() * 30.0).round() as i32;

    match timezone_gap {
        Some(gap) if gap <= MAX_TIMEZONE_GAP_HOURS => score += 60 - 15 * gap,
        // Further than that, still possible and much harder to schedule.
        Some(_) => score -= 40,
        // Unknown. Neither rewarded nor punished — most people have not
        // filled it in, and punishing them would hide good mentors.
        None => {}
    }

    // Room left. A mentor with nobody has more attention to give.
    score += 10 * (MAX_ACTIVE_MENTEES - active_mentees) as i32;

    score.max(0)
}

/// Mentors worth suggesting to this person, best first.
pub async fn matches_for(db: &PgPool, mentee_id: Uuid, limit: i64) -> Result<Vec<Match>, AppError> {
    let mentee: Option<Mentee> = sqlx::query_as(
        "SELECT craft_score_code, code_preferred_families, code_main_languages, timezone
           FROM users WHERE id = $1",
    )
    .bind(mentee_id)
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
               u.craft_score_code,
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
         WHERE m.active = TRUE
           AND u.is_banned = FALSE
           AND u.id <> $1
           AND u.craft_score_code >= $2
        "#,
    )
    .bind(mentee_id)
    .bind(mentee_score + MIN_SCORE_GAP)
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
            let gap = timezone_gap(mentee_timezone.as_deref(), c.timezone.as_deref());
            let score_gap = c.craft_score_code - mentee_score;

            let score = score_candidate(
                shared_families.len(),
                shared_languages.len(),
                score_gap,
                gap,
                c.active_mentees,
            );

            let mut because = Vec::new();
            if !shared_families.is_empty() {
                because.push(format!(
                    "Travaille dans {} — la même famille que toi.",
                    shared_families.join(", ")
                ));
            }
            if !shared_languages.is_empty() {
                because.push(format!(
                    "Partage tes langages : {}.",
                    shared_languages.join(", ")
                ));
            }
            because.push(format!(
                "{score_gap} points de craft score d'avance : assez pour t'apprendre quelque \
                 chose, pas assez pour parler une autre langue."
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
                craft_score_code: c.craft_score_code,
                score,
                shared_families,
                shared_languages,
                timezone_gap_hours: gap,
                active_mentees: c.active_mentees,
                because,
            }
        })
        // A zero is a refusal, not a weak suggestion: showing a kernel
        // engineer to somebody learning React wastes an hour of both.
        .filter(|m| m.score > 0)
        .collect();

    matches.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.active_mentees.cmp(&b.active_mentees))
    });
    matches.truncate(limit.clamp(1, 50) as usize);
    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_offset_is_read_and_anything_else_is_not_guessed() {
        assert_eq!(utc_offset_hours("+01:00"), Some(1));
        assert_eq!(utc_offset_hours("-05:00"), Some(-5));
        assert_eq!(utc_offset_hours("+00:00"), Some(0));
        // A wrong offset suggests mentors on the wrong half of the planet,
        // which is worse than suggesting nobody.
        assert_eq!(utc_offset_hours("Africa/Porto-Novo"), None);
        assert_eq!(utc_offset_hours("CET"), None);
        assert_eq!(utc_offset_hours("+99:00"), None);
    }

    #[test]
    fn a_gap_needs_both_ends() {
        assert_eq!(timezone_gap(Some("+01:00"), Some("-02:00")), Some(3));
        assert_eq!(timezone_gap(Some("+01:00"), None), None);
        assert_eq!(timezone_gap(None, None), None);
    }

    #[test]
    fn no_shared_family_is_not_a_match_at_any_price() {
        assert_eq!(score_candidate(0, 3, 5000, Some(0), 0), 0);
    }

    #[test]
    fn somebody_adjacent_has_nothing_to_teach_yet() {
        assert_eq!(score_candidate(1, 1, 100, Some(0), 0), 0);
        assert!(score_candidate(1, 1, MIN_SCORE_GAP, Some(0), 0) > 0);
    }

    #[test]
    fn a_full_mentor_is_not_available_whatever_their_calendar_says() {
        assert_eq!(score_candidate(2, 2, 2000, Some(0), MAX_ACTIVE_MENTEES), 0);
        assert!(score_candidate(2, 2, 2000, Some(0), MAX_ACTIVE_MENTEES - 1) > 0);
    }

    #[test]
    fn a_shared_language_helps_and_does_not_decide() {
        let same_language_one_family = score_candidate(1, 1, 1000, Some(0), 0);
        let two_families_no_language = score_candidate(2, 0, 1000, Some(0), 0);
        assert!(
            two_families_no_language > same_language_one_family,
            "a good mentor in a neighbouring language beats a mediocre one in the same"
        );
    }

    #[test]
    fn the_gap_has_diminishing_returns() {
        let modest = score_candidate(1, 0, 500, None, 0);
        let large = score_candidate(1, 0, 5000, None, 0);
        let enormous = score_candidate(1, 0, 50_000, None, 0);
        assert!(modest < large && large < enormous);
        // Ten times the gap is not ten times the score.
        assert!(large - modest < (modest - 100) * 10);
    }

    #[test]
    fn an_unknown_timezone_is_neither_rewarded_nor_punished() {
        let unknown = score_candidate(1, 1, 1000, None, 0);
        let close = score_candidate(1, 1, 1000, Some(1), 0);
        let far = score_candidate(1, 1, 1000, Some(9), 0);
        assert!(close > unknown, "a known close timezone should win");
        assert!(unknown > far, "a known far timezone should lose");
    }
}
