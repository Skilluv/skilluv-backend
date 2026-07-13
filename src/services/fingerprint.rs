//! P14.4 — Fingerprinting utilisateur pour détection multi-account.
//!
//! Design :
//! - À chaque login, `record_fingerprint(user_id, ip, ua, canvas)` insère
//!   une ligne dans `user_fingerprints`.
//! - `detect_multi_accounts(window_hours=24, min_shared_features=2)` cherche
//!   des groupes de user_ids qui partagent au moins 2 des 3 signatures
//!   (ip/ua/canvas) dans la fenêtre. Si le groupe a > 3 members distincts,
//!   tous sont marqués `suspected_multi_account = TRUE`.
//! - Un cron/scheduler appelle ce job quotidiennement.

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Hash SHA-256 hex d'une chaîne (limite la fuite de PII en base).
pub fn hash_str(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    hex::encode(hasher.finalize())
}

/// Ajoute une entrée fingerprint pour un login.
pub async fn record_fingerprint(
    db: &PgPool,
    user_id: Uuid,
    ip: &str,
    user_agent: &str,
    canvas_fingerprint: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO user_fingerprints (user_id, ip_hash, ua_hash, canvas_hash)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(user_id)
    .bind(hash_str(ip))
    .bind(hash_str(user_agent))
    .bind(canvas_fingerprint.map(hash_str))
    .execute(db)
    .await?;
    Ok(())
}

/// Groupe suspect : ensemble de user_ids qui partagent au moins 2 features.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SuspectGroup {
    pub shared_ip: String,
    pub shared_ua: String,
    pub user_ids: Vec<Uuid>,
}

/// Détecte les groupes de user_ids qui partagent (ip_hash, ua_hash) — les 2
/// features les plus stables — dans la fenêtre glissante. Retourne les groupes
/// de taille > `min_group_size` (défaut 3).
///
/// Marque en même temps `users.suspected_multi_account = TRUE` pour chaque
/// user_id concerné.
pub async fn detect_multi_accounts(
    db: &PgPool,
    window_hours: i32,
    min_group_size: i32,
) -> Result<Vec<SuspectGroup>, AppError> {
    let rows: Vec<(String, String, Vec<Uuid>)> = sqlx::query_as(
        r#"
        SELECT ip_hash, ua_hash, ARRAY_AGG(DISTINCT user_id) AS user_ids
        FROM user_fingerprints
        WHERE created_at > NOW() - ($1::TEXT || ' hours')::INTERVAL
        GROUP BY ip_hash, ua_hash
        HAVING COUNT(DISTINCT user_id) >= $2
        ORDER BY COUNT(DISTINCT user_id) DESC
        "#,
    )
    .bind(window_hours.to_string())
    .bind(min_group_size)
    .fetch_all(db)
    .await?;

    let groups: Vec<SuspectGroup> = rows
        .into_iter()
        .map(|(ip, ua, ids)| SuspectGroup {
            shared_ip: ip,
            shared_ua: ua,
            user_ids: ids,
        })
        .collect();

    // Marque les users flaggés.
    for g in &groups {
        let reason = format!(
            "{} accounts share ip_hash + ua_hash within last {}h",
            g.user_ids.len(),
            window_hours
        );
        sqlx::query(
            r#"
            UPDATE users
            SET suspected_multi_account = TRUE,
                suspected_multi_account_at = NOW(),
                suspected_multi_account_reason = $1
            WHERE id = ANY($2)
              AND suspected_multi_account = FALSE
            "#,
        )
        .bind(&reason)
        .bind(&g.user_ids)
        .execute(db)
        .await?;
    }

    Ok(groups)
}

/// Purge les fingerprints anciens (> `keep_days`). Cron mensuel.
pub async fn purge_old_fingerprints(db: &PgPool, keep_days: i32) -> Result<u64, AppError> {
    let res = sqlx::query(
        "DELETE FROM user_fingerprints
         WHERE created_at < NOW() - ($1::TEXT || ' days')::INTERVAL",
    )
    .bind(keep_days.to_string())
    .execute(db)
    .await?;
    Ok(res.rows_affected())
}
