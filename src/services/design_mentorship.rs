//! Matching a design mentee with a design mentor.
//!
//! The scoring lives in [`crate::services::mentorship_matching`], because it
//! is the same five questions every domain asks. What is here is the one
//! thing that genuinely differs: **where a person's families come from**.
//!
//! ## Declared for a mentee, proven for a mentor
//!
//! A mentee's families are what they said interests them, in the onboarding
//! answers. That is the right source: somebody looking for a mentor is
//! looking towards where they want to go, not where they have already been.
//!
//! A mentor's families are the trades they have **been validated in** —
//! `reviewer_group` on the orientations behind their verified deliverables.
//! Also the right source, and deliberately different: a mentor who declared
//! an interest in motion and has never delivered any is not a motion mentor,
//! and an hour with them would teach that the expensive way.
//!
//! The code domain reads columns on `users` for both ends. That is not an
//! inconsistency to fix by making design match it — it is the older answer,
//! and this is the better one.
//!
//! ## The vocabulary
//!
//! For code it is programming languages. For design it is tools: somebody who
//! works in Blender and somebody who works in Figma can share a family and
//! still spend the hour translating. It is a bonus, never a requirement — a
//! good mentor with a different tool beats a mediocre one with the same.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::mentorship_matching::{self as matching, Match, Weights};

/// The design domain's thresholds.
///
/// Three mentees rather than code's five. Design mentorship is a critique
/// conversation over an artefact, which is slower and more attentive than
/// reading a diff; a designer carrying five is carrying them badly.
pub const WEIGHTS: Weights = Weights {
    min_score_gap: 500,
    max_timezone_gap_hours: 3,
    max_active_mentees: 3,
};

pub const DOMAIN: &str = "design";

#[derive(sqlx::FromRow)]
struct Mentee {
    craft_score: i32,
    timezone: Option<String>,
    /// The onboarding answers, from which the declared families and tool come.
    answers: serde_json::Value,
}

#[derive(sqlx::FromRow)]
struct Candidate {
    user_id: Uuid,
    username: String,
    headline: String,
    craft_score: i32,
    /// Reviewer families of the trades this person has validated work in.
    families: Vec<String>,
    /// Their declared tool, plus whatever they wrote as expertise on the
    /// mentor profile. Both, because somebody who filled in one rarely filled
    /// in the other.
    vocabulary: Vec<String>,
    timezone: Option<String>,
    active_mentees: i64,
}

/// Read a string array out of the onboarding answers.
fn answer_list(answers: &serde_json::Value, key: &str) -> Vec<String> {
    answers
        .get(key)
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_lowercase))
                .collect()
        })
        .unwrap_or_default()
}

fn answer_string(answers: &serde_json::Value, key: &str) -> Option<String> {
    answers
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::to_lowercase)
}

/// The reviewer families behind a set of declared trade slugs.
///
/// A mentee says "design-brand-identity"; a mentor is known by "brand". The
/// families are what makes them comparable, and mapping in SQL rather than in
/// a table here is what keeps this correct when a twenty-seventh trade
/// arrives.
async fn families_of_trades(db: &PgPool, slugs: &[String]) -> Result<Vec<String>, AppError> {
    if slugs.is_empty() {
        return Ok(Vec::new());
    }
    let families: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT reviewer_group FROM orientations
          WHERE slug = ANY($1) AND reviewer_group IS NOT NULL",
    )
    .bind(slugs)
    .fetch_all(db)
    .await?;
    Ok(families)
}

/// Mentors worth suggesting to this person, best first.
pub async fn matches_for(db: &PgPool, mentee_id: Uuid, limit: i64) -> Result<Vec<Match>, AppError> {
    // A mentee with no computed score reads as zero, which is the right
    // answer: the gap to a mentor is then the mentor's whole score, and
    // somebody with nothing proved has the most to learn.
    let mentee: Option<Mentee> = sqlx::query_as(
        r#"
        SELECT COALESCE(cs.score, 0) AS craft_score,
               u.timezone,
               COALESCE(p.answers, '{}'::jsonb) AS answers
          FROM users u
          LEFT JOIN craft_scores cs ON cs.user_id = u.id AND cs.skill_domain = $2
          LEFT JOIN user_domain_profiles p ON p.user_id = u.id AND p.domain = $2
         WHERE u.id = $1
        "#,
    )
    .bind(mentee_id)
    .bind(DOMAIN)
    .fetch_optional(db)
    .await?;

    let mentee = mentee.ok_or_else(|| AppError::NotFound("user not found".into()))?;

    let declared_trades = answer_list(&mentee.answers, "preferred_families");
    let mentee_families = families_of_trades(db, &declared_trades).await?;
    if mentee_families.is_empty() {
        return Err(AppError::Validation(
            "réponds d'abord au questionnaire design : sans famille, il n'y a rien sur quoi \
             faire correspondre"
                .into(),
        ));
    }

    let mentee_tool = answer_string(&mentee.answers, "main_tool");

    let candidates = sqlx::query_as::<_, Candidate>(
        r#"
        SELECT u.id AS user_id,
               u.username,
               m.headline,
               cs.score AS craft_score,
               -- Proven, not declared: the families of the trades this person
               -- has verified work in. A mentor who said they were interested
               -- in motion and never delivered any is not a motion mentor.
               ARRAY(
                   SELECT DISTINCT o.reviewer_group
                     FROM deliverables d
                     JOIN project_slices ps ON ps.id = d.slice_id
                     JOIN orientations o ON o.id = ps.orientation_id
                    WHERE d.user_id = u.id
                      AND d.verification_status = 'verified'
                      AND d.revoked_at IS NULL
                      AND o.primary_domain = $3
                      AND o.reviewer_group IS NOT NULL
               ) AS families,
               ARRAY(
                   SELECT DISTINCT lower(v)
                     FROM unnest(
                         m.expertise_areas
                         || COALESCE(ARRAY[p.answers ->> 'main_tool'], ARRAY[]::text[])
                     ) AS v
                    WHERE v IS NOT NULL
               ) AS vocabulary,
               u.timezone,
               (SELECT count(DISTINCT s.mentee_user_id)
                  FROM mentorship_sessions s
                 WHERE s.mentor_user_id = u.id
                   -- Booked and not yet over. `completed` is deliberately
                   -- absent: a session that happened is history, not a mentee
                   -- somebody is still carrying.
                   AND s.status IN ('paid', 'confirmed')
                   AND s.scheduled_at > NOW() - INTERVAL '60 days')
                   AS active_mentees
          FROM mentor_profiles m
          JOIN users u ON u.id = m.user_id
          LEFT JOIN user_domain_profiles p ON p.user_id = u.id AND p.domain = $3
          -- An inner join: a mentor with no computed design score has proved
          -- nothing here, and suggesting them would be suggesting somebody on
          -- the strength of having filled in a profile.
          JOIN craft_scores cs ON cs.user_id = u.id AND cs.skill_domain = $3
         WHERE m.active = TRUE
           AND u.is_banned = FALSE
           AND u.id <> $1
           AND cs.score >= $2
        "#,
    )
    .bind(mentee_id)
    .bind(mentee.craft_score + WEIGHTS.min_score_gap)
    .bind(DOMAIN)
    .fetch_all(db)
    .await?;

    let mut matches: Vec<Match> = candidates
        .into_iter()
        .map(|c| {
            let shared_families: Vec<String> = c
                .families
                .iter()
                .filter(|f| mentee_families.contains(f))
                .cloned()
                .collect();
            let shared_vocabulary: Vec<String> = match &mentee_tool {
                Some(tool) => c
                    .vocabulary
                    .iter()
                    .filter(|v| *v == tool)
                    .cloned()
                    .collect(),
                None => Vec::new(),
            };
            let gap = matching::timezone_gap(mentee.timezone.as_deref(), c.timezone.as_deref());
            let score_gap = c.craft_score - mentee.craft_score;

            let score = matching::score_candidate(
                shared_families.len(),
                shared_vocabulary.len(),
                score_gap,
                gap,
                c.active_mentees,
                WEIGHTS,
            );

            let because = matching::reasons(
                &shared_families,
                &shared_vocabulary,
                "tes outils",
                score_gap,
                gap,
                c.active_mentees,
                WEIGHTS,
            );

            Match {
                mentor_user_id: c.user_id,
                username: c.username,
                headline: c.headline,
                craft_score: c.craft_score,
                score,
                shared_families,
                shared_vocabulary,
                timezone_gap_hours: gap,
                active_mentees: c.active_mentees,
                because,
            }
        })
        .collect();

    matching::rank(&mut matches, limit);
    Ok(matches)
}

/// Whether somebody is stuck enough that a mentor is worth suggesting.
///
/// The backlog's trigger: three versions handed in with nothing validated, or
/// a challenge that has been through several rounds without converging.
///
/// Deliberately not a notification. Telling somebody "you seem to be
/// struggling" unprompted is a message that lands badly however it is worded;
/// this answers a question the client asks when it renders their dashboard,
/// and the suggestion appears beside their work rather than arriving at them.
pub async fn could_use_a_mentor(db: &PgPool, user_id: Uuid) -> Result<bool, AppError> {
    let (handed_in, validated): (i64, i64) = sqlx::query_as(
        r#"
        SELECT count(DISTINCT d.slice_id) FILTER (WHERE TRUE),
               count(DISTINCT d.slice_id) FILTER (WHERE s.status = 'validated')
          FROM slice_validation_decisions d
          JOIN project_slices s ON s.id = d.slice_id
         WHERE s.claimed_by_user_id = $1
           AND s.slice_type = 'design_artifact'
        "#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    Ok(handed_in >= 3 && validated == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_design_mentor_carries_fewer_people_than_a_code_one() {
        // A design mentorship is a critique conversation over an artefact,
        // which is slower and more attentive than reading a diff.
        assert_eq!(WEIGHTS.max_active_mentees, 3);
        const {
            assert!(
                WEIGHTS.max_active_mentees
                    < crate::services::code_mentorship::WEIGHTS.max_active_mentees
            )
        };
    }

    #[test]
    fn the_onboarding_answers_are_read_defensively() {
        // The answers are a JSONB object somebody's client wrote. A missing
        // key, a null, a number where a string was expected: none of them
        // should be a 500 on a dashboard.
        let empty = serde_json::json!({});
        assert!(answer_list(&empty, "preferred_families").is_empty());
        assert_eq!(answer_string(&empty, "main_tool"), None);

        let wrong_shapes = serde_json::json!({
            "preferred_families": "design-web",
            "main_tool": 42,
        });
        assert!(answer_list(&wrong_shapes, "preferred_families").is_empty());
        assert_eq!(answer_string(&wrong_shapes, "main_tool"), None);

        let mixed = serde_json::json!({
            "preferred_families": ["Design-Web", 7, null, "design-brand-identity"],
        });
        // Lowercased, and the entries that are not strings are dropped rather
        // than turned into something.
        assert_eq!(
            answer_list(&mixed, "preferred_families"),
            vec!["design-web", "design-brand-identity"]
        );
    }
}
