//! What design mentorship needs that the shared matcher does not provide.
//!
//! The matching itself is [`mentorship_matching`], with
//! [`mentorship_matching::DESIGN`] as its rules. This module held a second
//! copy of that query, and the two differed in a way worth keeping: it read a
//! mentor's families from their **verified deliverables** rather than from
//! what they told the wizard interested them. That is the better answer, and
//! it is not design-specific — a mentor in any domain who declared an
//! interest and never delivered is not a mentor in that family — so it moved
//! into the shared matcher and applies to all five.
//!
//! What is left here is the one question no other domain asks.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Whether somebody is handing work in and getting none of it validated.
///
/// Three attempts with nothing accepted is not a skill gap that another
/// attempt fixes. It is the point where a designer needs somebody to look at
/// the work with them, and the point where, left alone, they stop.
///
/// Design rather than every domain because the failure looks different here:
/// a rejected pull request comes back with a diff and a comment, and a
/// rejected artefact comes back with a paragraph that is easy to read as a
/// verdict on taste.
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
    use crate::services::mentorship_matching::{CODE, DESIGN};

    #[test]
    fn a_design_mentor_carries_fewer_people_than_a_code_one() {
        // A design mentorship is a critique conversation over an artefact,
        // which is slower and more attentive than reading a diff.
        let design = DESIGN.max_active_mentees;
        let code = CODE.max_active_mentees;
        assert_eq!(design, 3);
        assert!(design < code, "{design} should be under {code}");
    }

    #[test]
    fn the_design_wizard_stores_trades_where_the_mentor_side_stores_families() {
        // `design-brand-identity` matches `brand` nowhere without the
        // resolution step, and the failure is silent: an empty list of
        // mentors reads as "nobody available".
        let design = DESIGN.families_are_trade_slugs;
        let code = CODE.families_are_trade_slugs;
        assert!(design, "the design wizard asks for trades");
        assert!(!code, "the code wizard already asks for families");
    }
}
