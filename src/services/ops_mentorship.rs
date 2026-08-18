//! Matching an ops mentee with an ops mentor.
//!
//! The matching itself is [`mentorship_matching`], shared with code and AI:
//! shared area, distance, timezone, load. This module adds the one thing
//! those domains have no equivalent for, and it is a refusal rather than a
//! weight.
//!
//! ## On-call cannot be taught by somebody who has never done it
//!
//! Half of what a junior needs here is not technical: what to do first at
//! three in the morning, when to escalate, how to write the message that goes
//! to customers while the system is still down. Somebody who has never held a
//! pager can teach Terraform perfectly well and cannot teach that.
//!
//! So a mentee heading for paid work — where on-call arrives whether or not
//! they went looking for it — is matched only against mentors who have
//! actually done it, and the reason is shown to them. The alternative is
//! matching on skill alone and letting both find out an hour into a paid
//! session.
//!
//! ## Why cloud experience is a bonus and not a filter
//!
//! A good SRE on GCP is a better mentor than a mediocre one on AWS. The
//! platforms differ in vocabulary and agree on almost everything that
//! matters, and filtering on them would empty the list in exactly the regions
//! where mentors are scarcest. That is why it is `tools_key` in the shared
//! rules rather than a condition here.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::mentorship_matching::{self, Match};

/// Who counts as having held a pager.
pub const HAS_HELD_A_PAGER: &[&str] = &["occasional", "regular", "always_on"];

/// The objectives that mean somebody will meet on-call whether or not they
/// asked for it.
pub const OBJECTIVES_THAT_MEET_ONCALL: &[&str] = &["find_paid_work", "start_own_practice"];

#[derive(Debug, Clone, Serialize)]
pub struct OpsMatch {
    #[serde(flatten)]
    pub base: Match,
    /// What the mentor answered about on-call. Shown rather than reduced to a
    /// flag: "regular" and "always_on" are different offers.
    pub mentor_oncall_experience: Option<String>,
}

/// Whether this person's stated objective means they will meet on-call.
pub fn needs_oncall_teaching(objective: Option<&str>) -> bool {
    objective.is_some_and(|o| OBJECTIVES_THAT_MEET_ONCALL.contains(&o))
}

/// Whether this answer means somebody has actually been woken up.
pub fn has_held_a_pager(experience: Option<&str>) -> bool {
    experience.is_some_and(|e| HAS_HELD_A_PAGER.contains(&e))
}

/// Ops mentors worth suggesting, best first.
pub async fn matches_for(
    db: &PgPool,
    mentee_id: Uuid,
    limit: i64,
) -> Result<Vec<OpsMatch>, AppError> {
    // Ask for more than the caller wants: the on-call filter below removes
    // candidates, and truncating first would answer with four when six were
    // available.
    let base = mentorship_matching::matches_for(
        db,
        mentorship_matching::OPS,
        mentee_id,
        (limit * 3).clamp(1, 50),
    )
    .await?;

    let objective: Option<String> = sqlx::query_scalar(
        "SELECT answers ->> 'objective'
           FROM user_domain_profiles
          WHERE user_id = $1 AND domain = 'ops'",
    )
    .bind(mentee_id)
    .fetch_optional(db)
    .await?
    .flatten();

    let needs_oncall = needs_oncall_teaching(objective.as_deref());

    let ids: Vec<Uuid> = base.iter().map(|m| m.mentor_user_id).collect();
    let experience: Vec<(Uuid, Option<String>)> = sqlx::query_as(
        "SELECT user_id, answers ->> 'oncall_experience'
           FROM user_domain_profiles
          WHERE domain = 'ops' AND user_id = ANY($1)",
    )
    .bind(&ids)
    .fetch_all(db)
    .await?;

    let mut matches: Vec<OpsMatch> = base
        .into_iter()
        .map(|mut m| {
            let mentor_oncall_experience = experience
                .iter()
                .find(|(id, _)| *id == m.mentor_user_id)
                .and_then(|(_, e)| e.clone());

            if needs_oncall && has_held_a_pager(mentor_oncall_experience.as_deref()) {
                m.because.push(
                    "A vraiment été d'astreinte — la moitié de ce qu'il y a à apprendre \
                     ici ne s'enseigne pas autrement."
                        .into(),
                );
            }

            OpsMatch {
                base: m,
                mentor_oncall_experience,
            }
        })
        // The refusal. Not a penalty: a mentor who has never been woken up
        // cannot teach being woken up, however good they are at the rest.
        .filter(|m| !needs_oncall || has_held_a_pager(m.mentor_oncall_experience.as_deref()))
        .collect();

    matches.truncate(limit.clamp(1, 50) as usize);
    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_somebody_who_has_been_woken_up_counts() {
        assert!(!has_held_a_pager(Some("never")));
        assert!(!has_held_a_pager(None), "no answer is not a yes");
        assert!(has_held_a_pager(Some("occasional")));
        assert!(has_held_a_pager(Some("always_on")));
    }

    #[test]
    fn the_filter_applies_to_the_objectives_that_meet_on_call() {
        // Somebody learning is not filtered: they can be taught Terraform by
        // anybody good, and nothing will page them.
        assert!(!needs_oncall_teaching(Some("learn")));
        assert!(!needs_oncall_teaching(Some("build_portfolio")));
        assert!(!needs_oncall_teaching(None));

        // Somebody heading for paid ops work will meet a rotation whether or
        // not they went looking for one.
        assert!(needs_oncall_teaching(Some("find_paid_work")));
        assert!(needs_oncall_teaching(Some("start_own_practice")));
    }
}
