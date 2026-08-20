//! Handing a mission in, being told to try again, and saying afterwards how it
//! went.
//!
//! ## Why rounds exist here at all
//!
//! Migration 0192 built a mission that goes `in_progress → delivered →
//! closed`. That is a code mission: one pull request, merged or not.
//!
//! A design mission is not shaped like that. A brand identity is handed in,
//! the client says the mark does not survive one colour, it is handed in
//! again. Two or three rounds is the normal case — the same thing the
//! challenge loop already models. Without somewhere to record them, "not yet"
//! could only be expressed by cancelling the mission or by arguing over
//! e-mail, and both lose the trail an arbitration would need.
//!
//! ## The mission status still never goes backwards
//!
//! A delivery is submitted while the mission is `in_progress`. A request for
//! changes leaves it `in_progress`. The mission reaches `delivered` only when
//! a delivery is accepted. The rounds live on the delivery.
//!
//! ## Ratings are blind
//!
//! A rating one side can read before writing their own is not a rating, it is
//! a negotiation. Both are written blind and revealed together — or after
//! [`RATING_REVEAL_DAYS`], so a client who never rates cannot suppress a
//! designer's rating for ever by staying silent.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// How long a rating stays hidden when only one side has written it.
///
/// Fourteen days. Long enough that a slow but honest client still gets the
/// blind exchange, short enough that a designer's rating is not held hostage
/// by silence.
pub const RATING_REVEAL_DAYS: i64 = 14;

/// The shortest useful reason for asking for changes.
///
/// Twenty characters. The same floor the brief refusals use, and for the same
/// reason: "not quite" costs a round and teaches nothing.
pub const MIN_CHANGE_REASON: usize = 20;

/// The most rounds a mission can run before something is wrong that a schema
/// cannot fix. Matches the CHECK on the table.
pub const MAX_ROUNDS: i16 = 20;

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Delivery {
    pub id: Uuid,
    pub mission_id: Uuid,
    pub round: i16,
    pub delivered_by: Uuid,
    pub artifact_url: String,
    pub notes_md: Option<String>,
    pub delivered_at: chrono::DateTime<chrono::Utc>,
    pub decision: Option<String>,
    pub decision_reason: Option<String>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    /// True when this round is past what the brief said it included. Not a
    /// refusal — a fact an arbitration can read.
    pub beyond_agreed_rounds: bool,
}

/// Hand in a round.
///
/// Only the person the mission is assigned to, only while it is in progress,
/// and only when the previous round has been answered — otherwise a designer
/// could bury a request for changes under a new delivery.
pub async fn deliver(
    db: &PgPool,
    mission_id: Uuid,
    user_id: Uuid,
    artifact_url: &str,
    notes_md: Option<&str>,
) -> Result<Delivery, AppError> {
    let url = artifact_url.trim();
    if url.chars().count() < 4 || url.chars().count() > 2048 {
        return Err(AppError::Validation(
            "the delivery URL must be between 4 and 2048 characters".into(),
        ));
    }
    if !(url.starts_with("https://") || url.starts_with("s3://")) {
        return Err(AppError::Validation(
            "the delivery URL must be an https link or a stored object".into(),
        ));
    }

    let mission: Option<(String, Option<Uuid>, Option<i16>)> = sqlx::query_as(
        "SELECT status, assigned_user_id, included_rounds FROM missions WHERE id = $1",
    )
    .bind(mission_id)
    .fetch_optional(db)
    .await?;

    let (status, assigned, included_rounds) =
        mission.ok_or_else(|| AppError::NotFound("mission not found".into()))?;

    if assigned != Some(user_id) {
        return Err(AppError::Forbidden);
    }
    if status != "in_progress" {
        return Err(AppError::Conflict(format!(
            "a mission is delivered while it is in progress, not while it is {status}"
        )));
    }

    let waiting: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM mission_deliveries
                         WHERE mission_id = $1 AND decision IS NULL)",
    )
    .bind(mission_id)
    .fetch_one(db)
    .await?;
    if waiting {
        return Err(AppError::Conflict(
            "a round is already waiting for an answer".into(),
        ));
    }

    let previous: i16 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(round), 0::SMALLINT) FROM mission_deliveries WHERE mission_id = $1",
    )
    .bind(mission_id)
    .fetch_one(db)
    .await?;

    let round = previous + 1;
    if round > MAX_ROUNDS {
        return Err(AppError::Conflict(
            "this mission has run more rounds than a schema can help with; it needs an \
             arbitration, not a delivery"
                .into(),
        ));
    }

    // Marked, never refused: the platform is not party to the contract, and a
    // designer past the agreed rounds needs the fact recorded, not a locked
    // door.
    let beyond = included_rounds.is_some_and(|included| round > included);

    let delivery = sqlx::query_as::<_, Delivery>(
        r#"
        INSERT INTO mission_deliveries
            (mission_id, round, delivered_by, artifact_url, notes_md, beyond_agreed_rounds)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, mission_id, round, delivered_by, artifact_url, notes_md, delivered_at,
                  decision, decision_reason, decided_at, beyond_agreed_rounds
        "#,
    )
    .bind(mission_id)
    .bind(round)
    .bind(user_id)
    .bind(url)
    .bind(notes_md.map(str::trim).filter(|s| !s.is_empty()))
    .bind(beyond)
    .fetch_one(db)
    .await?;

    Ok(delivery)
}

/// Accept a round. The mission becomes `delivered`.
pub async fn accept(db: &PgPool, mission_id: Uuid, client_id: Uuid) -> Result<Delivery, AppError> {
    let mut tx = db.begin().await?;
    let delivery_id = waiting_round(&mut tx, mission_id, client_id).await?;

    let delivery = sqlx::query_as::<_, Delivery>(
        r#"
        UPDATE mission_deliveries
           SET decision = 'accepted', decided_by = $2, decided_at = NOW()
         WHERE id = $1 AND decision IS NULL
     RETURNING id, mission_id, round, delivered_by, artifact_url, notes_md, delivered_at,
               decision, decision_reason, decided_at, beyond_agreed_rounds
        "#,
    )
    .bind(delivery_id)
    .bind(client_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::Conflict("this round was answered by somebody else".into()))?;

    // The mission moves forward only here. Accepting is the one thing that
    // ends the round loop.
    sqlx::query(
        "UPDATE missions SET status = 'delivered', updated_at = NOW()
          WHERE id = $1 AND status = 'in_progress'",
    )
    .bind(mission_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // After the commit, and logged rather than raised: the acceptance is a
    // fact once written, and failing it because an attestation could not be
    // issued would leave the mission delivered and the client staring at an
    // error. The attestation is re-issuable; a half-accepted delivery is not.
    if let Err(err) = attest_acceptance(db, mission_id, &delivery).await {
        tracing::warn!(%err, mission = %mission_id, "mission attestation not issued");
    }

    // And the instalment this round earned, where the mission is paid that
    // way. Does nothing for the other four models: they release on closure,
    // which is the same money at a different moment.
    if let Err(err) =
        crate::services::mission_milestones::release_for_round(db, mission_id, delivery.round).await
    {
        tracing::warn!(%err, mission = %mission_id, "milestone instalment not released");
    }

    Ok(delivery)
}

/// Issue the attestation an accepted mission earns.
///
/// The rounds are counted from the deliveries, because that is what a reader
/// of the attestation is being told: three rounds is not a worse mission than
/// one, it is a mission where somebody was told what was wrong and came back.
async fn attest_acceptance(
    db: &PgPool,
    mission_id: Uuid,
    delivery: &Delivery,
) -> Result<(), AppError> {
    let mission: Option<(String, String)> = sqlx::query_as(
        "SELECT skill_domain, title FROM missions WHERE id = $1",
    )
    .bind(mission_id)
    .fetch_optional(db)
    .await?;

    let Some((domain, title)) = mission else {
        return Ok(());
    };

    // The artefact first. Migration 0233 rules that an artefact basis must
    // link a deliverable, and it is right to: an attestation whose evidence
    // is a sentence is an attestation nobody can check.
    let deliverable_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO deliverables
            (mission_delivery_id, user_id, artifact_type, artifact_url,
             verifiable_by, verification_status, verified_at, public)
        VALUES ($1, $2, 'design_artifact', $3,
                -- The client looked at it and said yes. That is a human
                -- review, and it is the only verification a paid mission has.
                'human_review', 'verified', NOW(),
                -- Public: that the work happened is public, what it paid is
                -- not, and the amount is nowhere on this row.
                TRUE)
        ON CONFLICT (mission_delivery_id) WHERE mission_delivery_id IS NOT NULL
            DO UPDATE SET artifact_url = EXCLUDED.artifact_url
        RETURNING id
        "#,
    )
    .bind(delivery.id)
    .bind(delivery.delivered_by)
    .bind(&delivery.artifact_url)
    .fetch_one(db)
    .await?;

    crate::services::design_attestations::mission_delivered(
        db,
        delivery.delivered_by,
        &domain,
        &title,
        // The artefact, not the mission page. Evidence is the thing somebody
        // opens to check the claim, and a mission page says only that a
        // mission existed.
        &delivery.artifact_url,
        delivery.round,
        deliverable_id,
    )
    .await?;

    Ok(())
}

/// Ask for another round, saying what is wrong.
///
/// The mission stays `in_progress`: nothing regresses, because it never
/// advanced.
pub async fn request_changes(
    db: &PgPool,
    mission_id: Uuid,
    client_id: Uuid,
    reason: &str,
) -> Result<Delivery, AppError> {
    let reason = reason.trim();
    if reason.chars().count() < MIN_CHANGE_REASON {
        return Err(AppError::Validation(format!(
            "dis ce qui ne va pas en {MIN_CHANGE_REASON} caractères au moins : « pas tout à \
             fait » coûte un tour et n'apprend rien"
        )));
    }
    crate::validators::check_max_len(reason, "reason", 4000)?;

    let mut tx = db.begin().await?;
    let delivery_id = waiting_round(&mut tx, mission_id, client_id).await?;

    let delivery = sqlx::query_as::<_, Delivery>(
        r#"
        UPDATE mission_deliveries
           SET decision = 'changes_requested', decision_reason = $3,
               decided_by = $2, decided_at = NOW()
         WHERE id = $1 AND decision IS NULL
     RETURNING id, mission_id, round, delivered_by, artifact_url, notes_md, delivered_at,
               decision, decision_reason, decided_at, beyond_agreed_rounds
        "#,
    )
    .bind(delivery_id)
    .bind(client_id)
    .bind(reason)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::Conflict("this round was answered by somebody else".into()))?;

    tx.commit().await?;
    Ok(delivery)
}

/// The round waiting for an answer, if the caller is entitled to answer it.
///
/// Entitlement is membership of the enterprise that published the mission —
/// not the person who happened to click publish, who may have left.
async fn waiting_round(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    mission_id: Uuid,
    client_id: Uuid,
) -> Result<Uuid, AppError> {
    let entitled: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM missions m
               JOIN enterprise_members em ON em.enterprise_id = m.enterprise_id
              WHERE m.id = $1 AND em.user_id = $2
         )",
    )
    .bind(mission_id)
    .bind(client_id)
    .fetch_one(&mut **tx)
    .await?;
    if !entitled {
        return Err(AppError::Forbidden);
    }

    sqlx::query_scalar(
        "SELECT id FROM mission_deliveries
          WHERE mission_id = $1 AND decision IS NULL
          ORDER BY round DESC LIMIT 1
            FOR UPDATE",
    )
    .bind(mission_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::Conflict("no round is waiting for an answer".into()))
}

/// Every round of a mission, oldest first — the trail an arbitration reads.
pub async fn rounds_of(db: &PgPool, mission_id: Uuid) -> Result<Vec<Delivery>, AppError> {
    let rows = sqlx::query_as::<_, Delivery>(
        r#"
        SELECT id, mission_id, round, delivered_by, artifact_url, notes_md, delivered_at,
               decision, decision_reason, decided_at, beyond_agreed_rounds
          FROM mission_deliveries
         WHERE mission_id = $1
         ORDER BY round ASC
        "#,
    )
    .bind(mission_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

// ═══════════════════════════════════════════════════════════════════
// Ratings
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Rating {
    pub mission_id: Uuid,
    pub direction: String,
    pub rater_id: Uuid,
    pub rated_id: Uuid,
    pub rating: i16,
    pub comment_md: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Rate the other side.
///
/// Only once the mission is delivered or closed: rating work that is still
/// being argued about is a lever, not an opinion.
pub async fn rate(
    db: &PgPool,
    mission_id: Uuid,
    rater_id: Uuid,
    rating: i16,
    comment_md: Option<&str>,
) -> Result<Rating, AppError> {
    if !(1..=5).contains(&rating) {
        return Err(AppError::Validation("a rating goes from 1 to 5".into()));
    }
    if let Some(comment) = comment_md {
        crate::validators::check_max_len(comment, "comment_md", 4000)?;
    }

    let mission: Option<(String, Option<Uuid>, Uuid)> = sqlx::query_as(
        "SELECT status, assigned_user_id, enterprise_id FROM missions WHERE id = $1",
    )
    .bind(mission_id)
    .fetch_optional(db)
    .await?;
    let (status, assigned, enterprise_id) =
        mission.ok_or_else(|| AppError::NotFound("mission not found".into()))?;

    if !matches!(status.as_str(), "delivered" | "closed") {
        return Err(AppError::Conflict(
            "a mission is rated once it has been delivered — rating work still being argued \
             about is a lever, not an opinion"
                .into(),
        ));
    }

    let talent = assigned.ok_or_else(|| {
        AppError::Conflict("this mission was never assigned, so there is nobody to rate".into())
    })?;

    // Which side is speaking, and who they are rating. A client rates the
    // person who did the work; the talent rates whoever holds the mission for
    // the enterprise, which is not necessarily who clicked publish.
    let (direction, rated_id) = if rater_id == talent {
        let counterpart: Option<Uuid> = sqlx::query_scalar(
            // The longest-standing active member, by invitation date —
            // `enterprise_members` records when somebody was invited, not a
            // generic `created_at`. Owner first, because the owner is the
            // person a talent is really rating.
            "SELECT user_id FROM enterprise_members
              WHERE enterprise_id = $1 AND status = 'active'
              ORDER BY (role = 'owner') DESC, invited_at ASC
              LIMIT 1",
        )
        .bind(enterprise_id)
        .fetch_optional(db)
        .await?;
        let counterpart = counterpart.ok_or_else(|| {
            AppError::Conflict("this enterprise has no member left to rate".into())
        })?;
        ("talent_to_client", counterpart)
    } else {
        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM enterprise_members
                             WHERE enterprise_id = $1 AND user_id = $2)",
        )
        .bind(enterprise_id)
        .bind(rater_id)
        .fetch_one(db)
        .await?;
        if !member {
            return Err(AppError::Forbidden);
        }
        ("client_to_talent", talent)
    };

    let row = sqlx::query_as::<_, Rating>(
        r#"
        INSERT INTO mission_ratings
            (mission_id, direction, rater_id, rated_id, rating, comment_md)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING mission_id, direction, rater_id, rated_id, rating, comment_md, created_at
        "#,
    )
    .bind(mission_id)
    .bind(direction)
    .bind(rater_id)
    .bind(rated_id)
    .bind(rating)
    .bind(comment_md.map(str::trim).filter(|s| !s.is_empty()))
    .fetch_one(db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
            AppError::Conflict("you have already rated this mission".into())
        }
        other => AppError::from(other),
    })?;

    Ok(row)
}

/// Whether the ratings on a mission are readable yet.
///
/// True once both are in, or once the first is [`RATING_REVEAL_DAYS`] old. A
/// rating one side can read before writing their own is a negotiation; a
/// rating a silent client can suppress for ever is worse.
pub fn revealed(
    count: i64,
    first_written_at: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    if count >= 2 {
        return true;
    }
    match first_written_at {
        Some(first) => (now - first).num_days() >= RATING_REVEAL_DAYS,
        None => false,
    }
}

/// The ratings on a mission, if they are readable yet.
///
/// Returns an empty list rather than a refusal while they are still hidden:
/// "nobody has said anything yet" and "it is not your turn to read" look the
/// same to a client, and the second is not a distinction worth leaking.
pub async fn ratings_of(db: &PgPool, mission_id: Uuid) -> Result<Vec<Rating>, AppError> {
    let rows = sqlx::query_as::<_, Rating>(
        r#"
        SELECT mission_id, direction, rater_id, rated_id, rating, comment_md, created_at
          FROM mission_ratings
         WHERE mission_id = $1
         ORDER BY created_at ASC
        "#,
    )
    .bind(mission_id)
    .fetch_all(db)
    .await?;

    let first = rows.first().map(|r| r.created_at);
    if revealed(rows.len() as i64, first, chrono::Utc::now()) {
        Ok(rows)
    } else {
        Ok(Vec::new())
    }
}

/// What somebody's received ratings average to, and how many there are.
///
/// Only the revealed ones count. A rating still in its blind window is not
/// yet a fact about anybody.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct Standing {
    pub received: i64,
    /// `None` until there is at least one revealed rating. Deliberately not
    /// zero: an unrated person is not a badly rated one.
    pub average: Option<f64>,
}

pub async fn standing_of(db: &PgPool, user_id: Uuid) -> Result<Standing, AppError> {
    let row: Option<(i64, Option<f64>)> = sqlx::query_as(
        r#"
        WITH visible AS (
            SELECT r.rating
              FROM mission_ratings r
             WHERE r.rated_id = $1
               AND (
                   -- both sides are in
                   (SELECT count(*) FROM mission_ratings o WHERE o.mission_id = r.mission_id) >= 2
                   -- or the blind window has passed
                   OR r.created_at < NOW() - make_interval(days => $2::INT)
               )
        )
        SELECT count(*)::BIGINT, avg(rating)::DOUBLE PRECISION FROM visible
        "#,
    )
    .bind(user_id)
    .bind(RATING_REVEAL_DAYS as i32)
    .fetch_optional(db)
    .await?;

    let (received, average) = row.unwrap_or((0, None));
    Ok(Standing {
        received,
        average: if received > 0 { average } else { None },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both ends derived from one instant. An earlier version called
    /// `Utc::now()` twice — once for `now`, once inside the helper — so the
    /// fourteen-day case came out a few microseconds short of fourteen days
    /// and `num_days` truncated it to thirteen.
    fn window(days_ago: i64) -> (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) {
        let now = chrono::Utc::now();
        (now - chrono::Duration::days(days_ago), now)
    }

    #[test]
    fn one_rating_stays_hidden_until_the_window_passes() {
        // A rating the other side can read before writing their own is a
        // negotiation, not a rating.
        let (first, now) = window(1);
        assert!(!revealed(1, Some(first), now));

        let (first, now) = window(RATING_REVEAL_DAYS - 1);
        assert!(!revealed(1, Some(first), now));

        // But a silent client must not be able to suppress it for ever.
        let (first, now) = window(RATING_REVEAL_DAYS);
        assert!(revealed(1, Some(first), now));
    }

    #[test]
    fn both_sides_reveal_each_other_immediately() {
        let (first, now) = window(0);
        assert!(revealed(2, Some(first), now));
    }

    #[test]
    fn nothing_written_is_nothing_to_reveal() {
        assert!(!revealed(0, None, chrono::Utc::now()));
    }
}
