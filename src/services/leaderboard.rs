use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

const WEEKLY_TTL: u64 = 8 * 24 * 60 * 60; // 8 days
const MONTHLY_TTL: u64 = 35 * 24 * 60 * 60; // 35 days

const VALID_DOMAINS: &[&str] = &["global", "code", "design", "game", "security"];
const VALID_PERIODS: &[&str] = &["alltime", "weekly", "monthly"];

pub struct LeaderboardService;

impl LeaderboardService {
    pub fn validate_domain(domain: &str) -> Result<(), AppError> {
        if !VALID_DOMAINS.contains(&domain) {
            return Err(AppError::Validation(format!(
                "domain must be one of: {}",
                VALID_DOMAINS.join(", ")
            )));
        }
        Ok(())
    }

    pub fn validate_period(period: &str) -> Result<(), AppError> {
        if !VALID_PERIODS.contains(&period) {
            return Err(AppError::Validation(format!(
                "period must be one of: {}",
                VALID_PERIODS.join(", ")
            )));
        }
        Ok(())
    }

    fn leaderboard_key(domain: &str, period: &str) -> String {
        match period {
            "alltime" => format!("leaderboard:{domain}:alltime"),
            "weekly" => {
                let suffix = chrono::Utc::now().format("%G-W%V");
                format!("leaderboard:{domain}:weekly:{suffix}")
            }
            "monthly" => {
                let suffix = chrono::Utc::now().format("%Y-%m");
                format!("leaderboard:{domain}:monthly:{suffix}")
            }
            _ => format!("leaderboard:{domain}:{period}"),
        }
    }

    /// Update a user's score across all relevant leaderboards.
    /// Called after fragments are earned from a challenge submission.
    pub async fn update_score(
        redis: &mut ConnectionManager,
        db: &PgPool,
        user_id: Uuid,
        new_total_fragments: i32,
        skill_domain: &str,
        fragments_just_earned: i32,
    ) -> Result<(), AppError> {
        let user_id_str = user_id.to_string();

        // 1. Global alltime — absolute score (ZADD replaces)
        let key = Self::leaderboard_key("global", "alltime");
        let () = redis
            .zadd(&key, &user_id_str, new_total_fragments as f64)
            .await?;

        // 2. Domain alltime — sum of weighted_proven_count in that domain.
        // P8.7 : skill_fragments retiré, source unique = user_skills + skill_nodes.
        let domain_total: Option<i64> = sqlx::query_scalar(
            r#"
            SELECT COALESCE(SUM(us.weighted_proven_count)::BIGINT, 0)
            FROM user_skills us
            JOIN skill_nodes sn ON sn.id = us.skill_id
            WHERE us.user_id = $1 AND sn.domain = $2
            "#,
        )
        .bind(user_id)
        .bind(skill_domain)
        .fetch_one(db)
        .await?;

        let key = Self::leaderboard_key(skill_domain, "alltime");
        let () = redis
            .zadd(&key, &user_id_str, domain_total.unwrap_or(0) as f64)
            .await?;

        // 3. Weekly and monthly — ZINCRBY for incremental updates
        let earned = fragments_just_earned as f64;
        for period in &["weekly", "monthly"] {
            let ttl = if *period == "weekly" {
                WEEKLY_TTL
            } else {
                MONTHLY_TTL
            };

            // Global weekly/monthly
            let key = Self::leaderboard_key("global", period);
            let _: f64 = redis.zincr(&key, &user_id_str, earned).await?;
            Self::ensure_ttl(redis, &key, ttl).await?;

            // Domain weekly/monthly
            let key = Self::leaderboard_key(skill_domain, period);
            let _: f64 = redis.zincr(&key, &user_id_str, earned).await?;
            Self::ensure_ttl(redis, &key, ttl).await?;
        }

        Ok(())
    }

    /// Set TTL only if the key doesn't already have one (first write).
    async fn ensure_ttl(
        redis: &mut ConnectionManager,
        key: &str,
        ttl_secs: u64,
    ) -> Result<(), AppError> {
        let current_ttl: i64 = redis::cmd("TTL").arg(key).query_async(redis).await?;
        // TTL returns -1 if no expiry set
        if current_ttl == -1 {
            let () = redis.expire(key, ttl_secs as i64).await?;
        }
        Ok(())
    }

    /// Get a page of leaderboard entries (user_id, score), ordered by score descending.
    pub async fn get_page(
        redis: &mut ConnectionManager,
        domain: &str,
        period: &str,
        offset: isize,
        count: isize,
    ) -> Result<Vec<(String, f64)>, AppError> {
        let key = Self::leaderboard_key(domain, period);
        let entries: Vec<(String, f64)> = redis
            .zrevrange_withscores(&key, offset, offset + count - 1)
            .await?;
        Ok(entries)
    }

    /// Get a user's rank (1-based). Returns None if user not in leaderboard.
    pub async fn get_rank(
        redis: &mut ConnectionManager,
        domain: &str,
        period: &str,
        user_id: Uuid,
    ) -> Result<Option<i64>, AppError> {
        let key = Self::leaderboard_key(domain, period);
        let rank: Option<i64> = redis.zrevrank(&key, user_id.to_string()).await?;
        Ok(rank.map(|r| r + 1)) // Convert 0-based to 1-based
    }

    /// Get a user's score. Returns None if user not in leaderboard.
    pub async fn get_score(
        redis: &mut ConnectionManager,
        domain: &str,
        period: &str,
        user_id: Uuid,
    ) -> Result<Option<f64>, AppError> {
        let key = Self::leaderboard_key(domain, period);
        let score: Option<f64> = redis.zscore(&key, user_id.to_string()).await?;
        Ok(score)
    }

    /// Get total number of participants in a leaderboard.
    pub async fn get_total(
        redis: &mut ConnectionManager,
        domain: &str,
        period: &str,
    ) -> Result<i64, AppError> {
        let key = Self::leaderboard_key(domain, period);
        let count: i64 = redis.zcard(&key).await?;
        Ok(count)
    }

    /// Remove a user from all leaderboards (for account deletion or ban).
    pub async fn remove_user(redis: &mut ConnectionManager, user_id: Uuid) -> Result<(), AppError> {
        let user_id_str = user_id.to_string();
        let domains = &["global", "code", "design", "game", "security"];

        for domain in domains {
            // Alltime
            let key = Self::leaderboard_key(domain, "alltime");
            let _: i64 = redis.zrem(&key, &user_id_str).await?;

            // Current weekly
            let key = Self::leaderboard_key(domain, "weekly");
            let _: i64 = redis.zrem(&key, &user_id_str).await?;

            // Current monthly
            let key = Self::leaderboard_key(domain, "monthly");
            let _: i64 = redis.zrem(&key, &user_id_str).await?;
        }

        Ok(())
    }

    /// Seed all alltime leaderboards from database.
    /// Called at startup for initial population.
    pub async fn seed_from_db(redis: &mut ConnectionManager, db: &PgPool) -> Result<(), AppError> {
        // Global alltime: all active, non-banned users with fragments > 0
        let users: Vec<(Uuid, i32)> = sqlx::query_as(
            "SELECT id, total_fragments FROM users WHERE profile_active = TRUE AND is_banned = FALSE AND total_fragments > 0",
        )
        .fetch_all(db)
        .await?;

        for (user_id, total) in &users {
            let key = Self::leaderboard_key("global", "alltime");
            let () = redis.zadd(&key, user_id.to_string(), *total as f64).await?;
        }

        // Per-domain alltime: aggregate from user_skills + skill_nodes.
        // P8.7 : skill_fragments legacy retiré, source unique.
        let domain_scores: Vec<(Uuid, String, i64)> = sqlx::query_as(
            r#"
            SELECT us.user_id, sn.domain,
                   SUM(us.weighted_proven_count)::BIGINT AS total
            FROM user_skills us
            JOIN skill_nodes sn ON sn.id = us.skill_id
            JOIN users u ON u.id = us.user_id
            WHERE u.profile_active = TRUE AND u.is_banned = FALSE
            GROUP BY us.user_id, sn.domain
            HAVING SUM(us.weighted_proven_count) > 0
            "#,
        )
        .fetch_all(db)
        .await?;

        for (user_id, domain, total) in &domain_scores {
            let key = Self::leaderboard_key(domain, "alltime");
            let () = redis.zadd(&key, user_id.to_string(), *total as f64).await?;
        }

        tracing::info!(
            global_users = users.len(),
            domain_entries = domain_scores.len(),
            "Leaderboards seeded from database"
        );

        Ok(())
    }
}
