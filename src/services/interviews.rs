//! Interview scheduling.
//!
//! One table for every interview Skilluv arranges — off a contest shortlist,
//! off a recruitment campaign, off a trial period. Three tables would have
//! meant three notification paths and three places to forget the time zone.
//!
//! ## Who chooses
//!
//! The company offers slots; the person picks one. Not the other way round,
//! and not "the platform assigns the earliest": somebody job-hunting while
//! employed cannot take a slot at eleven on a Tuesday, and a system that
//! books it for them has quietly filtered out everybody with a job.
//!
//! Times are stored as given, in UTC. A slot rewritten into somebody's local
//! time is a slot argued about later.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const SOURCES: &[&str] = &[
    "enterprise_contest",
    "recruitment_campaign",
    "recruitment_trial",
];

pub const PLATFORMS: &[&str] = &["zoom", "meet", "teams", "phone", "in_person"];

/// A window offered to somebody.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slot {
    pub start: chrono::DateTime<chrono::Utc>,
    pub end: chrono::DateTime<chrono::Utc>,
}

/// Whether a set of proposed slots is usable.
///
/// Each has to run forward and last at least a quarter of an hour. A
/// zero-length slot is a typo that would otherwise be confirmed, and the
/// person who finds out is the one who cleared their afternoon.
pub fn slots_are_usable(slots: &[Slot]) -> Result<(), String> {
    if slots.is_empty() {
        return Err("offer at least one slot".into());
    }
    if slots.len() > 20 {
        return Err("twenty slots is already more choice than help".into());
    }
    for slot in slots {
        if slot.end <= slot.start {
            return Err("a slot has to end after it starts".into());
        }
        if (slot.end - slot.start) < chrono::Duration::minutes(15) {
            return Err("a slot shorter than fifteen minutes is not an interview".into());
        }
    }
    Ok(())
}

/// Whether a chosen slot is one of the offered ones.
///
/// Compared on both ends. Accepting a start that matches and an end that does
/// not would let a client book an hour where they offered twenty minutes.
pub fn slot_was_offered(chosen: &Slot, offered: &[Slot]) -> bool {
    offered
        .iter()
        .any(|s| s.start == chosen.start && s.end == chosen.end)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Interview {
    pub id: Uuid,
    pub source_type: String,
    pub source_id: Uuid,
    pub talent_user_id: Uuid,
    pub enterprise_id: Uuid,
    pub proposed_slots: serde_json::Value,
    pub confirmed_slot: Option<serde_json::Value>,
    pub platform: Option<String>,
    pub meeting_url: Option<String>,
    pub location: Option<String>,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const INTERVIEW_SELECT: &str = r#"
    SELECT id, source_type, source_id, talent_user_id, enterprise_id,
           proposed_slots, confirmed_slot, platform, meeting_url, location,
           status, created_at
      FROM interview_scheduling
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct ProposalInput {
    pub source_type: String,
    pub source_id: Uuid,
    pub talent_user_id: Uuid,
    pub slots: Vec<Slot>,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub meeting_url: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
}

/// The company offers times.
pub async fn propose(
    db: &PgPool,
    enterprise_id: Uuid,
    input: ProposalInput,
) -> Result<Interview, AppError> {
    if !SOURCES.contains(&input.source_type.as_str()) {
        return Err(AppError::Validation(format!(
            "source_type must be one of: {}",
            SOURCES.join(", ")
        )));
    }
    if let Some(platform) = &input.platform
        && !PLATFORMS.contains(&platform.as_str())
    {
        return Err(AppError::Validation(format!(
            "platform must be one of: {}",
            PLATFORMS.join(", ")
        )));
    }

    slots_are_usable(&input.slots).map_err(AppError::Validation)?;

    let slots = serde_json::to_value(&input.slots)
        .map_err(|e| AppError::Internal(format!("could not encode the slots: {e}")))?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO interview_scheduling
            (source_type, source_id, talent_user_id, enterprise_id, proposed_slots,
             platform, meeting_url, location)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
         RETURNING id",
    )
    .bind(&input.source_type)
    .bind(input.source_id)
    .bind(input.talent_user_id)
    .bind(enterprise_id)
    .bind(&slots)
    .bind(input.platform.as_deref())
    .bind(input.meeting_url.as_deref())
    .bind(input.location.as_deref())
    .fetch_one(db)
    .await?;

    by_id(db, id).await
}

pub async fn by_id(db: &PgPool, id: Uuid) -> Result<Interview, AppError> {
    let sql = format!("{INTERVIEW_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Interview>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("interview not found".into()))
}

/// Everything somebody has been offered or has booked.
pub async fn for_talent(db: &PgPool, user_id: Uuid) -> Result<Vec<Interview>, AppError> {
    let sql = format!(
        "{INTERVIEW_SELECT} WHERE talent_user_id = $1
            AND status IN ('proposed', 'confirmed')
          ORDER BY created_at DESC"
    );
    let rows = sqlx::query_as::<_, Interview>(sqlx::AssertSqlSafe(sql))
        .bind(user_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

pub async fn for_source(
    db: &PgPool,
    source_type: &str,
    source_id: Uuid,
) -> Result<Vec<Interview>, AppError> {
    let sql = format!(
        "{INTERVIEW_SELECT} WHERE source_type = $1 AND source_id = $2
          ORDER BY created_at DESC"
    );
    let rows = sqlx::query_as::<_, Interview>(sqlx::AssertSqlSafe(sql))
        .bind(source_type)
        .bind(source_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// The person picks one.
pub async fn confirm(
    db: &PgPool,
    interview_id: Uuid,
    user_id: Uuid,
    chosen: Slot,
) -> Result<Interview, AppError> {
    let interview = by_id(db, interview_id).await?;
    if interview.talent_user_id != user_id {
        return Err(AppError::NotFound("interview not found".into()));
    }
    if interview.status != "proposed" {
        return Err(AppError::Validation(format!(
            "this interview is {} and is not waiting on an answer",
            interview.status
        )));
    }

    let offered: Vec<Slot> = serde_json::from_value(interview.proposed_slots.clone())
        .map_err(|e| AppError::Internal(format!("stored slots are unreadable: {e}")))?;

    if !slot_was_offered(&chosen, &offered) {
        return Err(AppError::Validation(
            "that time was not offered. Picking one outside the list would book the \
             company for an hour it never said it was free."
                .into(),
        ));
    }

    let value = serde_json::to_value(&chosen)
        .map_err(|e| AppError::Internal(format!("could not encode the slot: {e}")))?;

    sqlx::query(
        "UPDATE interview_scheduling
            SET status = 'confirmed', confirmed_slot = $2 WHERE id = $1",
    )
    .bind(interview_id)
    .bind(&value)
    .execute(db)
    .await
    .map_err(|e| {
        if e.to_string().contains("a_remote_interview_has_a_link") {
            AppError::Validation(
                "a remote interview needs a link before it can be confirmed".into(),
            )
        } else {
            AppError::from(e)
        }
    })?;

    by_id(db, interview_id).await
}

/// The person says no.
///
/// Declining is a first-class answer, not a silence. Somebody who has taken
/// another job should be able to say so once rather than be chased.
pub async fn decline(
    db: &PgPool,
    interview_id: Uuid,
    user_id: Uuid,
    reason: Option<&str>,
) -> Result<(), AppError> {
    let done = sqlx::query(
        "UPDATE interview_scheduling
            SET status = 'declined', declined_reason = $3
          WHERE id = $1 AND talent_user_id = $2 AND status = 'proposed'",
    )
    .bind(interview_id)
    .bind(user_id)
    .bind(reason.map(str::trim).filter(|r| !r.is_empty()))
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "you have no open invitation to this interview".into(),
        ));
    }
    Ok(())
}

/// It happened.
///
/// Marks the contest submission too, when that is where it came from: the
/// success fee rests on an interview having taken place, and the two facts
/// must not be able to disagree.
pub async fn complete(db: &PgPool, interview_id: Uuid) -> Result<(), AppError> {
    let interview = by_id(db, interview_id).await?;
    if interview.status != "confirmed" {
        return Err(AppError::Validation(format!(
            "this interview is {} — only a confirmed one can be marked done",
            interview.status
        )));
    }

    let mut tx = db.begin().await?;
    sqlx::query("UPDATE interview_scheduling SET status = 'completed' WHERE id = $1")
        .bind(interview_id)
        .execute(&mut *tx)
        .await?;

    if interview.source_type == "enterprise_contest" {
        sqlx::query(
            "UPDATE contest_submissions SET interview_completed = TRUE
              WHERE contest_id = $1 AND talent_user_id = $2",
        )
        .bind(interview.source_id)
        .bind(interview.talent_user_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u32) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp(1_800_000_000 + (hour as i64) * 3600, 0).unwrap()
    }

    fn slot(from: u32, to: u32) -> Slot {
        Slot {
            start: at(from),
            end: at(to),
        }
    }

    #[test]
    fn a_backwards_slot_is_refused() {
        assert!(slots_are_usable(&[slot(10, 9)]).is_err());
    }

    #[test]
    fn a_zero_length_slot_is_refused() {
        // A typo that would otherwise be confirmed, and the person who finds
        // out is the one who cleared their afternoon.
        let same = Slot {
            start: at(10),
            end: at(10),
        };
        assert!(slots_are_usable(&[same]).is_err());
    }

    #[test]
    fn offering_nothing_is_not_offering() {
        assert!(slots_are_usable(&[]).is_err());
    }

    #[test]
    fn a_normal_set_of_slots_passes() {
        assert!(slots_are_usable(&[slot(9, 10), slot(14, 15), slot(16, 17)]).is_ok());
    }

    #[test]
    fn a_slot_matches_only_on_both_ends() {
        let offered = vec![slot(9, 10), slot(14, 15)];
        assert!(slot_was_offered(&slot(14, 15), &offered));
        // Same start, longer end: accepting this would book an hour where
        // twenty minutes were offered.
        assert!(!slot_was_offered(&slot(14, 16), &offered));
        assert!(!slot_was_offered(&slot(11, 12), &offered));
    }

    #[test]
    fn every_source_and_platform_is_a_known_one() {
        assert_eq!(SOURCES.len(), 3);
        assert_eq!(PLATFORMS.len(), 5);
        assert!(PLATFORMS.contains(&"in_person"));
    }
}
