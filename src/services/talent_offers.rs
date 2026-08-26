//! SKI-45 (Post-MVP T3-02) — reverse marketplace.
//!
//! See migration 0147 for how an offer differs from a mentor profile.
//!
//! Two gates live here rather than in the schema, because both depend on
//! state that changes independently of the offer row:
//!
//!   * **Rank** — publishing requires Artisan or above. Rank is derived, so
//!     a snapshot in the row would go stale; it is checked on write and
//!     re-checked on read, which means an offer from someone since demoted
//!     stops being listed without any cleanup job.
//!   * **Payout readiness** — a priced offer requires a verified Stripe
//!     Connect account. Advertising a price the platform could not pay out
//!     is a promise we cannot keep.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ranks;

pub const OFFER_TYPES: &[&str] = &[
    "pair_programming",
    "code_review",
    "whiteboard",
    "mock_interview",
    "career_advice",
];

/// Minimum rank required to publish. Artisan means 11 verified
/// deliverables and an attestation — enough of a track record that the
/// offer means something.
pub const MIN_RANK: &str = ranks::RANK_ARTISAN;

/// Cap on live offers per talent, so the browse view stays a marketplace
/// rather than one person's catalogue.
pub const MAX_OFFERS_PER_USER: i64 = 5;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TalentOffer {
    pub id: Uuid,
    pub user_id: Uuid,
    pub offer_type: String,
    pub skill_id: Option<Uuid>,
    pub availability_hours: i16,
    pub price_cents_per_hour: Option<i64>,
    pub description: String,
    pub active: bool,
    /// Set when moderation took the offer off the marketplace. Distinct
    /// from `active`, which is the author's own pause switch — see
    /// migration 0443.
    pub moderation_held_at: Option<chrono::DateTime<chrono::Utc>>,
    pub moderation_reason: Option<String>,
    pub moderated_by: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl TalentOffer {
    /// Listed publicly only when both the author and the platform agree.
    pub fn is_listed(&self) -> bool {
        self.active && self.moderation_held_at.is_none()
    }
}

pub struct CreateOfferParams<'a> {
    pub offer_type: &'a str,
    pub skill_id: Option<Uuid>,
    pub availability_hours: i16,
    pub price_cents_per_hour: Option<i64>,
    pub description: &'a str,
}

fn validate_shape(params: &CreateOfferParams<'_>) -> Result<(), AppError> {
    if !OFFER_TYPES.contains(&params.offer_type) {
        return Err(AppError::Validation(format!(
            "offer_type must be one of: {}",
            OFFER_TYPES.join(", ")
        )));
    }
    if !(1..=20).contains(&params.availability_hours) {
        return Err(AppError::Validation(
            "availability_hours must be between 1 and 20".into(),
        ));
    }
    if let Some(p) = params.price_cents_per_hour
        && p <= 0
    {
        return Err(AppError::Validation(
            "price_cents_per_hour must be positive, or null for a free offer".into(),
        ));
    }
    if params.description.chars().count() > 2000 {
        return Err(AppError::Validation(
            "description must be at most 2000 characters".into(),
        ));
    }
    Ok(())
}

/// Assert the user may publish offers at all.
///
/// Reads the effective rank (SKI-46), so someone serving a vouching
/// penalty loses publishing rights for its duration — which is the point
/// of the penalty.
pub async fn assert_can_publish(db: &PgPool, user_id: Uuid) -> Result<String, AppError> {
    let rank = ranks::effective_rank(db, user_id).await?;
    let min = ranks::rank_position(MIN_RANK).unwrap_or(usize::MAX);
    let mine = ranks::rank_position(&rank).unwrap_or(0);
    if mine < min {
        return Err(AppError::Forbidden);
    }
    Ok(rank)
}

/// Assert the user can actually be paid before letting them ask to be.
async fn assert_payout_ready(db: &PgPool, user_id: Uuid) -> Result<(), AppError> {
    let status: Option<String> =
        sqlx::query_scalar("SELECT stripe_kyc_status FROM talent_wallets WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    if status.as_deref() == Some("verified") {
        return Ok(());
    }
    Err(AppError::Validation(
        "finish your Stripe payout setup before publishing a paid offer, \
         or publish it as free (price_cents_per_hour: null)"
            .into(),
    ))
}

pub async fn create(
    db: &PgPool,
    user_id: Uuid,
    params: CreateOfferParams<'_>,
) -> Result<TalentOffer, AppError> {
    validate_shape(&params)?;
    assert_can_publish(db, user_id).await?;

    if params.price_cents_per_hour.is_some() {
        assert_payout_ready(db, user_id).await?;
    }

    if let Some(skill_id) = params.skill_id {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM skill_nodes WHERE id = $1)")
                .bind(skill_id)
                .fetch_one(db)
                .await?;
        if !exists {
            return Err(AppError::NotFound(format!("skill {skill_id} not found")));
        }
    }

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM talent_offers WHERE user_id = $1 AND active = TRUE",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;
    if count >= MAX_OFFERS_PER_USER {
        return Err(AppError::Validation(format!(
            "at most {MAX_OFFERS_PER_USER} live offers — pause one first"
        )));
    }

    let inserted: Result<TalentOffer, sqlx::Error> = sqlx::query_as(
        r#"
        INSERT INTO talent_offers
            (user_id, offer_type, skill_id, availability_hours,
             price_cents_per_hour, description)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(params.offer_type)
    .bind(params.skill_id)
    .bind(params.availability_hours)
    .bind(params.price_cents_per_hour)
    .bind(params.description.trim())
    .fetch_one(db)
    .await;

    match inserted {
        Ok(o) => Ok(o),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(AppError::Conflict(
            "you already have an offer of this type for this skill".into(),
        )),
        Err(e) => Err(e.into()),
    }
}

/// One row of the public browse listing.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct OfferListing {
    pub id: Uuid,
    pub user_id: Uuid,
    pub display_name: String,
    pub username: String,
    pub rank: String,
    pub offer_type: String,
    pub skill_id: Option<Uuid>,
    pub skill_slug: Option<String>,
    pub availability_hours: i16,
    pub price_cents_per_hour: Option<i64>,
    pub description: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// The two rank sets the browse query tests against.
///
/// Returns `(eligible, eligible_if_penalised)`:
///
/// * `eligible` — raw ranks that clear [`MIN_RANK`] on their own;
/// * `eligible_if_penalised` — raw ranks that still clear it *after* the
///   one-step vouching penalty, i.e. one notch higher.
///
/// Derived from `ranks::rank_order()` so the ladder is defined in exactly
/// one place.
fn eligible_rank_sets() -> (Vec<String>, Vec<String>) {
    let order = ranks::rank_order();
    let min = ranks::rank_position(MIN_RANK).unwrap_or(0);
    let eligible: Vec<String> = order[min..].iter().map(|r| r.to_string()).collect();
    // A penalised author is demoted one step, so they need to start one
    // step above the bar. `saturating_add` keeps an out-of-range MIN_RANK
    // from panicking on the slice.
    let penalised_min = (min + 1).min(order.len());
    let eligible_if_penalised: Vec<String> = order[penalised_min..]
        .iter()
        .map(|r| r.to_string())
        .collect();
    (eligible, eligible_if_penalised)
}

pub struct BrowseFilter<'a> {
    pub offer_type: Option<&'a str>,
    pub skill_slug: Option<&'a str>,
    /// Only free offers.
    pub free_only: bool,
    pub limit: i64,
    pub offset: i64,
}

/// Public browse.
///
/// Filters out offers whose author no longer meets the rank bar, applying
/// the same penalty layer as `assert_can_publish`. Doing it in SQL keeps
/// the pagination honest: filtering after the LIMIT would return short
/// pages.
pub async fn browse(db: &PgPool, filter: BrowseFilter<'_>) -> Result<Vec<OfferListing>, AppError> {
    if let Some(t) = filter.offer_type
        && !OFFER_TYPES.contains(&t)
    {
        return Err(AppError::Validation(format!(
            "offer_type must be one of: {}",
            OFFER_TYPES.join(", ")
        )));
    }

    // The rank ladder stays in Rust. Two precomputed sets turn the
    // eligibility rule into a plain membership test in SQL, instead of
    // re-encoding the ladder as a CASE expression that would have to be
    // kept in sync with `ranks` by hand.
    let (eligible, eligible_if_penalised) = eligible_rank_sets();

    let rows: Vec<OfferListing> = sqlx::query_as(
        r#"
        SELECT o.id,
               o.user_id,
               COALESCE(NULLIF(u.display_name, ''), u.username) AS display_name,
               u.username,
               COALESCE(r.rank, 'apprenti')                     AS rank,
               o.offer_type,
               o.skill_id,
               sn.slug                                          AS skill_slug,
               o.availability_hours,
               o.price_cents_per_hour,
               o.description,
               o.created_at
          FROM talent_offers o
          JOIN users u      ON u.id = o.user_id
          LEFT JOIN user_ranks r  ON r.user_id = o.user_id
          LEFT JOIN skill_nodes sn ON sn.id = o.skill_id
         WHERE o.active = TRUE
           AND o.moderation_held_at IS NULL
           AND u.is_banned = FALSE
           AND u.profile_hidden = FALSE
           -- Effective rank: a live vouching penalty drops the author one
           -- step, so a penalised author needs a raw rank one notch higher
           -- to still clear the bar.
           AND COALESCE(r.rank, 'apprenti') = ANY(
                 CASE WHEN r.penalty_until IS NOT NULL AND r.penalty_until > NOW()
                      THEN $2::TEXT[]
                      ELSE $1::TEXT[]
                 END
               )
           AND ($3::TEXT IS NULL OR o.offer_type = $3)
           AND ($4::TEXT IS NULL OR sn.slug = $4)
           AND (NOT $5::BOOLEAN OR o.price_cents_per_hour IS NULL)
         ORDER BY o.created_at DESC
         LIMIT $6 OFFSET $7
        "#,
    )
    .bind(&eligible)
    .bind(&eligible_if_penalised)
    .bind(filter.offer_type)
    .bind(filter.skill_slug)
    .bind(filter.free_only)
    .bind(filter.limit)
    .bind(filter.offset)
    .fetch_all(db)
    .await?;

    Ok(rows)
}

/// Update an offer the caller owns.
pub async fn update(
    db: &PgPool,
    offer_id: Uuid,
    user_id: Uuid,
    availability_hours: Option<i16>,
    price_cents_per_hour: Option<Option<i64>>,
    description: Option<&str>,
    active: Option<bool>,
) -> Result<TalentOffer, AppError> {
    if let Some(h) = availability_hours
        && !(1..=20).contains(&h)
    {
        return Err(AppError::Validation(
            "availability_hours must be between 1 and 20".into(),
        ));
    }
    if let Some(d) = description
        && d.chars().count() > 2000
    {
        return Err(AppError::Validation(
            "description must be at most 2000 characters".into(),
        ));
    }

    // Switching a free offer to paid re-runs the payout check: the wallet
    // may have been set up before, or not at all.
    if let Some(Some(price)) = price_cents_per_hour {
        if price <= 0 {
            return Err(AppError::Validation(
                "price_cents_per_hour must be positive, or null for a free offer".into(),
            ));
        }
        assert_payout_ready(db, user_id).await?;
    }

    // Re-activating requires still meeting the rank bar.
    if active == Some(true) {
        assert_can_publish(db, user_id).await?;

        // A moderation hold is not the author's to lift. The browse query
        // already filters on it, so this check changes no listing — it
        // replaces a request that appears to succeed while showing nothing
        // with an answer that says what actually happened.
        let held: Option<Option<chrono::DateTime<chrono::Utc>>> = sqlx::query_scalar(
            "SELECT moderation_held_at FROM talent_offers WHERE id = $1 AND user_id = $2",
        )
        .bind(offer_id)
        .bind(user_id)
        .fetch_optional(db)
        .await?;
        if let Some(Some(_)) = held {
            return Err(AppError::Forbidden);
        }
    }

    let updated: Option<TalentOffer> = sqlx::query_as(
        r#"
        UPDATE talent_offers SET
            availability_hours   = COALESCE($3, availability_hours),
            price_cents_per_hour = CASE WHEN $4::BOOLEAN THEN $5 ELSE price_cents_per_hour END,
            description          = COALESCE($6, description),
            active               = COALESCE($7, active),
            updated_at           = NOW()
        WHERE id = $1 AND user_id = $2
        RETURNING *
        "#,
    )
    .bind(offer_id)
    .bind(user_id)
    .bind(availability_hours)
    // Distinguishes "leave the price alone" from "set it to null (free)".
    .bind(price_cents_per_hour.is_some())
    .bind(price_cents_per_hour.flatten())
    .bind(description.map(str::trim))
    .bind(active)
    .fetch_optional(db)
    .await?;

    updated.ok_or_else(|| AppError::NotFound(format!("offer {offer_id} not found")))
}

/// One row of the admin listing.
///
/// Same projection as [`OfferListing`] plus what moderation needs and the
/// public browse must never carry: the author's visibility state, and the
/// hold itself. Reusing the public struct would have meant either leaking
/// those fields publicly or maintaining two nearly identical queries.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AdminOfferListing {
    pub id: Uuid,
    pub user_id: Uuid,
    pub display_name: String,
    pub username: String,
    pub rank: String,
    pub offer_type: String,
    pub skill_id: Option<Uuid>,
    pub skill_slug: Option<String>,
    pub availability_hours: i16,
    pub price_cents_per_hour: Option<i64>,
    pub description: String,
    pub active: bool,
    pub moderation_held_at: Option<chrono::DateTime<chrono::Utc>>,
    pub moderation_reason: Option<String>,
    pub moderated_by: Option<Uuid>,
    /// Why the offer is not in the public browse, when it is not. `None`
    /// means it is listed. One of `paused_by_author`, `moderation_hold`,
    /// `author_hidden`, `author_banned`, `rank_below_bar`.
    pub hidden_reason: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub struct AdminBrowseFilter<'a> {
    pub offer_type: Option<&'a str>,
    pub skill_slug: Option<&'a str>,
    pub user_id: Option<Uuid>,
    /// Include offers the author paused or moderation held.
    pub include_inactive: bool,
    /// Only offers currently under a moderation hold.
    pub held_only: bool,
    pub limit: i64,
    pub offset: i64,
}

/// SKI-296 — the admin listing.
///
/// The public browse could not serve moderation: it filters on `active`,
/// on the author being visible, and on the author still clearing the rank
/// bar. An offer worth investigating is very often one that fails at least
/// one of those, so the surface that shows only compliant offers is
/// exactly the wrong one to moderate from.
///
/// `hidden_reason` is computed here rather than left for the caller to
/// infer, because inferring it means re-implementing the browse
/// eligibility rule in the admin panel, and the two would drift.
pub async fn admin_browse(
    db: &PgPool,
    filter: AdminBrowseFilter<'_>,
) -> Result<(Vec<AdminOfferListing>, i64), AppError> {
    if let Some(t) = filter.offer_type
        && !OFFER_TYPES.contains(&t)
    {
        return Err(AppError::Validation(format!(
            "offer_type must be one of: {}",
            OFFER_TYPES.join(", ")
        )));
    }

    let (eligible, eligible_if_penalised) = eligible_rank_sets();

    let rows: Vec<AdminOfferListing> = sqlx::query_as(
        r#"
        SELECT o.id,
               o.user_id,
               COALESCE(NULLIF(u.display_name, ''), u.username) AS display_name,
               u.username,
               COALESCE(r.rank, 'apprenti')                     AS rank,
               o.offer_type,
               o.skill_id,
               sn.slug                                          AS skill_slug,
               o.availability_hours,
               o.price_cents_per_hour,
               o.description,
               o.active,
               o.moderation_held_at,
               o.moderation_reason,
               o.moderated_by,
               CASE
                   WHEN o.moderation_held_at IS NOT NULL THEN 'moderation_hold'::TEXT
                   WHEN NOT o.active                     THEN 'paused_by_author'::TEXT
                   WHEN u.is_banned                      THEN 'author_banned'::TEXT
                   WHEN u.profile_hidden                 THEN 'author_hidden'::TEXT
                   WHEN COALESCE(r.rank, 'apprenti') <> ALL(
                            CASE WHEN r.penalty_until IS NOT NULL AND r.penalty_until > NOW()
                                 THEN $1::TEXT[]
                                 ELSE $2::TEXT[]
                            END
                        )                                THEN 'rank_below_bar'::TEXT
                   ELSE NULL
               END                                              AS hidden_reason,
               o.created_at,
               o.updated_at
          FROM talent_offers o
          JOIN users u             ON u.id = o.user_id
          LEFT JOIN user_ranks r   ON r.user_id = o.user_id
          LEFT JOIN skill_nodes sn ON sn.id = o.skill_id
         WHERE ($3::BOOLEAN OR (o.active = TRUE AND o.moderation_held_at IS NULL))
           AND (NOT $4::BOOLEAN OR o.moderation_held_at IS NOT NULL)
           AND ($5::TEXT IS NULL OR o.offer_type = $5)
           AND ($6::TEXT IS NULL OR sn.slug = $6)
           AND ($7::UUID IS NULL OR o.user_id = $7)
         ORDER BY o.created_at DESC
         LIMIT $8 OFFSET $9
        "#,
    )
    .bind(&eligible_if_penalised)
    .bind(&eligible)
    .bind(filter.include_inactive)
    .bind(filter.held_only)
    .bind(filter.offer_type)
    .bind(filter.skill_slug)
    .bind(filter.user_id)
    .bind(filter.limit)
    .bind(filter.offset)
    .fetch_all(db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
          FROM talent_offers o
          JOIN users u             ON u.id = o.user_id
          LEFT JOIN skill_nodes sn ON sn.id = o.skill_id
         WHERE ($1::BOOLEAN OR (o.active = TRUE AND o.moderation_held_at IS NULL))
           AND (NOT $2::BOOLEAN OR o.moderation_held_at IS NOT NULL)
           AND ($3::TEXT IS NULL OR o.offer_type = $3)
           AND ($4::TEXT IS NULL OR sn.slug = $4)
           AND ($5::UUID IS NULL OR o.user_id = $5)
        "#,
    )
    .bind(filter.include_inactive)
    .bind(filter.held_only)
    .bind(filter.offer_type)
    .bind(filter.skill_slug)
    .bind(filter.user_id)
    .fetch_one(db)
    .await?;

    Ok((rows, total))
}

/// Shortest motive a hold will accept.
pub const MIN_HOLD_REASON_CHARS: usize = 8;

/// Put an offer under a moderation hold.
///
/// The row is kept: an offer under dispute has to stay readable while the
/// dispute is instructed, and deleting it would destroy the evidence for
/// the decision at the moment the decision is made.
pub async fn moderation_hold(
    db: &PgPool,
    offer_id: Uuid,
    moderator_id: Uuid,
    reason: &str,
) -> Result<TalentOffer, AppError> {
    let reason = reason.trim();
    if reason.chars().count() < MIN_HOLD_REASON_CHARS {
        return Err(AppError::Validation(format!(
            "reason must be at least {MIN_HOLD_REASON_CHARS} characters — \
             an offer pulled without a recorded motive cannot be appealed"
        )));
    }

    let existing: Option<TalentOffer> = sqlx::query_as("SELECT * FROM talent_offers WHERE id = $1")
        .bind(offer_id)
        .fetch_optional(db)
        .await?;
    let existing =
        existing.ok_or_else(|| AppError::NotFound(format!("offer {offer_id} not found")))?;
    if existing.moderation_held_at.is_some() {
        return Err(AppError::Conflict("offer is already held".into()));
    }

    let updated: TalentOffer = sqlx::query_as(
        r#"
        UPDATE talent_offers
           SET moderation_held_at = NOW(),
               moderation_reason  = $2,
               moderated_by       = $3,
               updated_at         = NOW()
         WHERE id = $1
         RETURNING *
        "#,
    )
    .bind(offer_id)
    .bind(reason)
    .bind(moderator_id)
    .fetch_one(db)
    .await?;
    Ok(updated)
}

/// Lift a hold. Admin-only, and never reachable by the author — that
/// asymmetry is the whole point of the column.
pub async fn moderation_release(db: &PgPool, offer_id: Uuid) -> Result<TalentOffer, AppError> {
    let updated: Option<TalentOffer> = sqlx::query_as(
        r#"
        UPDATE talent_offers
           SET moderation_held_at = NULL,
               moderation_reason  = NULL,
               moderated_by       = NULL,
               updated_at         = NOW()
         WHERE id = $1 AND moderation_held_at IS NOT NULL
         RETURNING *
        "#,
    )
    .bind(offer_id)
    .fetch_optional(db)
    .await?;
    updated.ok_or_else(|| AppError::NotFound(format!("held offer {offer_id} not found")))
}

#[cfg(test)]
mod unit {
    use super::*;

    fn params(
        price: Option<i64>,
        hours: i16,
        offer_type: &'static str,
    ) -> CreateOfferParams<'static> {
        CreateOfferParams {
            offer_type,
            skill_id: None,
            availability_hours: hours,
            price_cents_per_hour: price,
            description: "",
        }
    }

    #[test]
    fn offer_type_must_be_known() {
        assert!(validate_shape(&params(None, 2, "pair_programming")).is_ok());
        assert!(validate_shape(&params(None, 2, "therapy")).is_err());
    }

    #[test]
    fn availability_is_capped_at_a_side_activity() {
        assert!(validate_shape(&params(None, 1, "code_review")).is_ok());
        assert!(validate_shape(&params(None, 20, "code_review")).is_ok());
        assert!(validate_shape(&params(None, 0, "code_review")).is_err());
        assert!(
            validate_shape(&params(None, 21, "code_review")).is_err(),
            "past 20h/week this is employment, not an offer"
        );
    }

    #[test]
    fn price_is_either_absent_or_positive() {
        assert!(validate_shape(&params(None, 2, "whiteboard")).is_ok());
        assert!(validate_shape(&params(Some(5000), 2, "whiteboard")).is_ok());
        assert!(validate_shape(&params(Some(0), 2, "whiteboard")).is_err());
        assert!(validate_shape(&params(Some(-1), 2, "whiteboard")).is_err());
    }

    #[test]
    fn min_rank_is_a_real_rank_above_the_entry_level() {
        let min = ranks::rank_position(MIN_RANK).expect("MIN_RANK is on the ladder");
        assert!(min > 0, "publishing must require more than signing up");
    }

    #[test]
    fn a_hold_outranks_the_authors_own_switch() {
        let base = TalentOffer {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
            offer_type: "code_review".into(),
            skill_id: None,
            availability_hours: 2,
            price_cents_per_hour: None,
            description: String::new(),
            active: true,
            moderation_held_at: None,
            moderation_reason: None,
            moderated_by: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        assert!(base.is_listed());

        let paused = TalentOffer {
            active: false,
            ..base.clone()
        };
        assert!(!paused.is_listed());

        // Held while the author still wants it listed: the platform wins.
        // That asymmetry is the entire reason the column exists rather
        // than reusing `active`.
        let held = TalentOffer {
            moderation_held_at: Some(chrono::Utc::now()),
            moderation_reason: Some("misleading rate".into()),
            ..base
        };
        assert!(held.active && !held.is_listed());
    }

    #[test]
    fn penalised_authors_need_one_rank_more() {
        let (eligible, penalised) = eligible_rank_sets();
        assert_eq!(
            eligible,
            vec![
                ranks::RANK_ARTISAN.to_string(),
                ranks::RANK_MAITRE.to_string(),
                ranks::RANK_DOYEN.to_string(),
            ]
        );
        // An Artisan serving a penalty drops to Ranger and loses the right
        // to publish; a Maître drops to Artisan and keeps it.
        assert_eq!(
            penalised,
            vec![
                ranks::RANK_MAITRE.to_string(),
                ranks::RANK_DOYEN.to_string(),
            ]
        );
        assert!(!penalised.contains(&ranks::RANK_ARTISAN.to_string()));
    }
}
