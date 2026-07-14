//! P17.4 — Rank system Apprenti → Doyen.
//!
//! Le rank est **dérivé** des preuves : deliverables verified + attestations
//! reçues. Progression unidirectionnelle (on ne rétrograde jamais).
//!
//! Seuils (spec UX BMAD `badge-system-design.md`) :
//!   apprenti : inscription
//!   ranger   : 4 deliverables verified
//!   artisan  : 11 deliverables + 1 attestation
//!   maitre   : 26 deliverables + 3 attestations
//!   doyen    : 50 deliverables + 5 attestations + users.role = 'mentor'
//!
//! Note : la contrainte mentor pour doyen est un stub — la vraie logique
//! "capabilities validées" arrivera en P18 (capabilities/personas).

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const RANK_APPRENTI: &str = "apprenti";
pub const RANK_RANGER: &str = "ranger";
pub const RANK_ARTISAN: &str = "artisan";
pub const RANK_MAITRE: &str = "maitre";
pub const RANK_DOYEN: &str = "doyen";

const ORDER: &[&str] = &[
    RANK_APPRENTI, RANK_RANGER, RANK_ARTISAN, RANK_MAITRE, RANK_DOYEN,
];

/// Retourne (rank_courant, rank_calculé, promoted?).
pub async fn recompute_rank_for_user(
    db: &PgPool,
    user_id: Uuid,
) -> Result<(String, String, bool), AppError> {
    let deliverables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deliverables
         WHERE user_id = $1 AND verification_status = 'verified'",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let attestations: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM attestations
         WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let is_mentor: bool = sqlx::query_scalar(
        "SELECT COALESCE(role = 'mentor', FALSE) FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?
    .unwrap_or(false);

    let computed = compute_rank(deliverables, attestations, is_mentor);

    let current: String = sqlx::query_scalar(
        "SELECT rank FROM user_ranks WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?
    .unwrap_or_else(|| RANK_APPRENTI.to_string());

    // Unidirectionnel : on ne descend jamais.
    if rank_index(&computed) > rank_index(&current) {
        let mut tx = db.begin().await?;
        sqlx::query(
            "INSERT INTO user_ranks (user_id, rank, previous_rank, achieved_at)
             VALUES ($1, $2, $3, NOW())
             ON CONFLICT (user_id) DO UPDATE SET
                 rank = EXCLUDED.rank,
                 previous_rank = user_ranks.rank,
                 achieved_at = NOW()",
        )
        .bind(user_id)
        .bind(&computed)
        .bind(&current)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO user_rank_history (user_id, from_rank, to_rank, reason)
             VALUES ($1, $2, $3, 'auto-promotion via badge_engine::recompute')",
        )
        .bind(user_id)
        .bind(&current)
        .bind(&computed)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok((current, computed, true));
    }

    Ok((current, computed, false))
}

fn compute_rank(deliverables: i64, attestations: i64, is_mentor: bool) -> String {
    if deliverables >= 50 && attestations >= 5 && is_mentor {
        RANK_DOYEN.into()
    } else if deliverables >= 26 && attestations >= 3 {
        RANK_MAITRE.into()
    } else if deliverables >= 11 && attestations >= 1 {
        RANK_ARTISAN.into()
    } else if deliverables >= 4 {
        RANK_RANGER.into()
    } else {
        RANK_APPRENTI.into()
    }
}

fn rank_index(rank: &str) -> usize {
    ORDER.iter().position(|r| *r == rank).unwrap_or(0)
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn thresholds_are_progressive() {
        assert_eq!(compute_rank(0, 0, false), RANK_APPRENTI);
        assert_eq!(compute_rank(3, 0, false), RANK_APPRENTI);
        assert_eq!(compute_rank(4, 0, false), RANK_RANGER);
        assert_eq!(compute_rank(11, 0, false), RANK_RANGER, "attestation manquante");
        assert_eq!(compute_rank(11, 1, false), RANK_ARTISAN);
        assert_eq!(compute_rank(26, 3, false), RANK_MAITRE);
        assert_eq!(compute_rank(50, 5, false), RANK_MAITRE, "mentor manquant");
        assert_eq!(compute_rank(50, 5, true), RANK_DOYEN);
    }
}
