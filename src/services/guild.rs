//! Guilds service — Phase 2 Sprint 4.
//!
//! Persistent MMO/F1-style groups. Solo membership, 7-day cooldown on leave,
//! 10% of fragments → GP, divisions for matchmaking, three invitation flows.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

// ─── Constants ────────────────────────────────────────────────────

pub const FOUNDING_FRAGMENTS_COST: i32 = 200;
pub const MIN_FOUNDER_FRAGMENTS: i32 = 500; // "artisan" threshold
pub const REQUIRED_COFOUNDER_COUNT: usize = 3;
pub const LEAVE_COOLDOWN_DAYS: i64 = 7;
pub const GP_PERCENT_FROM_FRAGMENTS: i32 = 10; // 10% of fragments → GP
pub const INVITE_TOKEN_TTL_DAYS: i64 = 7;
pub const INVITE_DIRECT_TTL_DAYS: i64 = 14;
pub const WAR_DECISION_WINDOW_HOURS: i64 = 48;
pub const WAR_DEFAULT_DURATION_DAYS: i64 = 7;

// ─── Domain types ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Guild {
    pub id: Uuid,
    pub slug: String,
    pub tag: String,
    pub name: String,
    pub description: Option<String>,
    pub motto: Option<String>,
    pub logo_url: Option<String>,
    pub banner_url: Option<String>,
    pub color_primary: Option<String>,
    pub color_secondary: Option<String>,
    pub founder_id: Uuid,
    pub membership_mode: String,
    pub max_members: i32,
    pub level: i32,
    pub gp_total: i64,
    pub gp_season: i64,
    pub division: String,
    pub forum_category_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub disbanded_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct GuildMember {
    pub guild_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub joined_at: DateTime<Utc>,
    pub gp_contributed: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct GuildInvitation {
    pub id: Uuid,
    pub guild_id: Uuid,
    pub inviter_id: Uuid,
    pub invited_user_id: Option<Uuid>,
    pub token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct GuildApplication {
    pub id: Uuid,
    pub guild_id: Uuid,
    pub applicant_id: Uuid,
    pub message: String,
    pub status: String,
    pub decided_by_id: Option<Uuid>,
    pub decided_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct GuildWar {
    pub id: Uuid,
    pub challenger_guild_id: Uuid,
    pub defender_guild_id: Uuid,
    pub stake_gp: i64,
    pub status: String,
    pub challenger_score: i32,
    pub defender_score: i32,
    pub winner_guild_id: Option<Uuid>,
    pub proposed_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub concluded_at: Option<DateTime<Utc>>,
}

// ─── Creation (with co-founders, atomic) ─────────────────────────

pub struct CreateGuildInput {
    pub founder_id: Uuid,
    pub slug: String,
    pub tag: String,
    pub name: String,
    pub description: Option<String>,
    pub motto: Option<String>,
    pub membership_mode: String,
    pub cofounder_ids: Vec<Uuid>,
}

pub struct GuildCreated {
    pub guild: Guild,
    pub forum_category_id: Uuid,
    pub cofounders_added: Vec<Uuid>,
}

pub async fn create_guild(db: &PgPool, input: CreateGuildInput) -> Result<GuildCreated, AppError> {
    // ─── Pre-flight validation ───────────────────────────────────
    if input.cofounder_ids.len() != REQUIRED_COFOUNDER_COUNT {
        return Err(AppError::Validation(format!(
            "Exactly {REQUIRED_COFOUNDER_COUNT} co-founders are required to mint a guild"
        )));
    }
    if input.cofounder_ids.iter().any(|id| id == &input.founder_id) {
        return Err(AppError::Validation(
            "The founder cannot be listed as their own co-founder".into(),
        ));
    }
    let unique_cofounders: std::collections::HashSet<&Uuid> = input.cofounder_ids.iter().collect();
    if unique_cofounders.len() != REQUIRED_COFOUNDER_COUNT {
        return Err(AppError::Validation("Co-founders must be distinct".into()));
    }
    if !matches!(
        input.membership_mode.as_str(),
        "open" | "application" | "invite_only"
    ) {
        return Err(AppError::Validation(
            "membership_mode must be open | application | invite_only".into(),
        ));
    }
    let slug = input.slug.trim().to_lowercase();
    if slug.len() < 3
        || slug.len() > 50
        || !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(AppError::Validation(
            "slug must be 3-50 alphanumeric/dash characters".into(),
        ));
    }
    let tag = input.tag.trim().to_uppercase();
    if tag.len() < 3 || tag.len() > 5 || !tag.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(AppError::Validation(
            "tag must be 3-5 alphanumeric uppercase characters".into(),
        ));
    }
    let name = input.name.trim();
    if name.len() < 3 || name.len() > 60 {
        return Err(AppError::Validation("name must be 3-60 characters".into()));
    }

    // ─── Run in a single transaction ─────────────────────────────
    let mut tx = db.begin().await?;

    // Founder must be artisan+ AND not already in a guild AND not on cooldown
    let founder_check: Option<(i32, String)> = sqlx::query_as(
        "SELECT total_fragments, title FROM users WHERE id = $1 AND is_banned = FALSE",
    )
    .bind(input.founder_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (founder_fragments, _founder_title) =
        founder_check.ok_or(AppError::NotFound("founder not found".into()))?;
    if founder_fragments < MIN_FOUNDER_FRAGMENTS {
        return Err(AppError::Validation(format!(
            "Guild creation requires {MIN_FOUNDER_FRAGMENTS}+ fragments (artisan tier or above)"
        )));
    }
    if founder_fragments < MIN_FOUNDER_FRAGMENTS + FOUNDING_FRAGMENTS_COST {
        return Err(AppError::Validation(format!(
            "Need {} fragments to cover the {FOUNDING_FRAGMENTS_COST}-fragment founding cost",
            MIN_FOUNDER_FRAGMENTS + FOUNDING_FRAGMENTS_COST
        )));
    }

    ensure_can_join(&mut tx, input.founder_id).await?;
    for cof in &input.cofounder_ids {
        ensure_can_join(&mut tx, *cof).await?;
        let cof_ok: Option<(bool,)> = sqlx::query_as("SELECT is_banned FROM users WHERE id = $1")
            .bind(cof)
            .fetch_optional(&mut *tx)
            .await?;
        match cof_ok {
            Some((true,)) => {
                return Err(AppError::Validation(
                    "A co-founder is banned and cannot be enrolled".into(),
                ));
            }
            None => {
                return Err(AppError::Validation(
                    "A co-founder user_id does not exist".into(),
                ));
            }
            _ => {}
        }
    }

    // Charge the founding cost
    let updated = sqlx::query(
        "UPDATE users SET total_fragments = total_fragments - $1 WHERE id = $2 AND total_fragments >= $1",
    )
    .bind(FOUNDING_FRAGMENTS_COST)
    .bind(input.founder_id)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::Validation(
            "Insufficient fragments at charge time".into(),
        ));
    }

    // Insert guild
    let guild: Guild = sqlx::query_as(
        r#"
        INSERT INTO guilds (slug, tag, name, description, motto, founder_id, membership_mode)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING *
        "#,
    )
    .bind(&slug)
    .bind(&tag)
    .bind(name)
    .bind(input.description.as_deref().map(str::trim))
    .bind(input.motto.as_deref().map(str::trim))
    .bind(input.founder_id)
    .bind(&input.membership_mode)
    .fetch_one(&mut *tx)
    .await?;

    // Create the private forum category bound to the guild
    let category_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO forum_categories (slug, name, description, position, locked, guild_id)
        VALUES ($1, $2, $3, $4, TRUE, $5)
        RETURNING id
        "#,
    )
    .bind(format!("guild-{}", slug))
    .bind(format!("[{tag}] {name}"))
    .bind(format!("Private forum for guild [{tag}] {name}"))
    .bind(1000) // private guild categories sorted at the end
    .bind(guild.id)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query("UPDATE guilds SET forum_category_id = $1 WHERE id = $2")
        .bind(category_id)
        .bind(guild.id)
        .execute(&mut *tx)
        .await?;

    // Insert founder + co-founders
    sqlx::query("INSERT INTO guild_members (guild_id, user_id, role) VALUES ($1, $2, 'founder')")
        .bind(guild.id)
        .bind(input.founder_id)
        .execute(&mut *tx)
        .await?;
    let mut cofounders_added = Vec::new();
    for cof in &input.cofounder_ids {
        let res = sqlx::query(
            "INSERT INTO guild_members (guild_id, user_id, role) VALUES ($1, $2, 'member')",
        )
        .bind(guild.id)
        .bind(cof)
        .execute(&mut *tx)
        .await;
        match res {
            Ok(_) => cofounders_added.push(*cof),
            Err(err) => {
                tracing::warn!(error = %err, %cof, "co-founder insert failed");
                return Err(AppError::Validation(
                    "A co-founder is already in another guild".into(),
                ));
            }
        }
    }

    tx.commit().await?;

    Ok(GuildCreated {
        guild,
        forum_category_id: category_id,
        cofounders_added,
    })
}

// ─── Membership helpers ───────────────────────────────────────────

async fn ensure_can_join(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
) -> Result<(), AppError> {
    let already: Option<(Uuid,)> =
        sqlx::query_as("SELECT guild_id FROM guild_members WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&mut **tx)
            .await?;
    if already.is_some() {
        return Err(AppError::Validation(
            "User is already in a guild (one-guild rule)".into(),
        ));
    }
    let cooldown: Option<(DateTime<Utc>,)> =
        sqlx::query_as("SELECT available_at FROM user_guild_cooldown WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&mut **tx)
            .await?;
    if let Some((available_at,)) = cooldown {
        if available_at > Utc::now() {
            return Err(AppError::Validation(format!(
                "User is in guild cooldown until {}",
                available_at.to_rfc3339()
            )));
        }
    }
    Ok(())
}

pub async fn current_guild_id(db: &PgPool, user_id: Uuid) -> Result<Option<Uuid>, AppError> {
    let row: Option<(Uuid,)> =
        sqlx::query_as("SELECT guild_id FROM guild_members WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    Ok(row.map(|(id,)| id))
}

pub async fn join_guild(db: &PgPool, guild_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
    let mut tx = db.begin().await?;
    ensure_can_join(&mut tx, user_id).await?;

    // Cap on members + active guild check
    let guild: Guild = sqlx::query_as("SELECT * FROM guilds WHERE id = $1")
        .bind(guild_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("guild not found".into()))?;
    if guild.disbanded_at.is_some() {
        return Err(AppError::Validation("Guild has been disbanded".into()));
    }
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM guild_members WHERE guild_id = $1")
        .bind(guild_id)
        .fetch_one(&mut *tx)
        .await?;
    if count.0 >= guild.max_members as i64 {
        return Err(AppError::Validation("Guild is full".into()));
    }

    sqlx::query("INSERT INTO guild_members (guild_id, user_id, role) VALUES ($1, $2, 'recruit')")
        .bind(guild_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn leave_guild(db: &PgPool, user_id: Uuid) -> Result<Uuid, AppError> {
    let mut tx = db.begin().await?;
    let row: Option<(Uuid, String)> =
        sqlx::query_as("SELECT guild_id, role FROM guild_members WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?;
    let (guild_id, role) = row.ok_or(AppError::NotFound("user is not in a guild".into()))?;

    // Founder cannot just leave : must transfer ownership first
    if role == "founder" {
        let other_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM guild_members WHERE guild_id = $1 AND user_id <> $2",
        )
        .bind(guild_id)
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;
        if other_count.0 > 0 {
            return Err(AppError::Validation(
                "Founder must transfer ownership before leaving".into(),
            ));
        }
        // Last member ⇒ disband the guild
        sqlx::query("UPDATE guilds SET disbanded_at = NOW() WHERE id = $1")
            .bind(guild_id)
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query("DELETE FROM guild_members WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    let available_at = Utc::now() + ChronoDuration::days(LEAVE_COOLDOWN_DAYS);
    sqlx::query(
        r#"
        INSERT INTO user_guild_cooldown (user_id, available_at)
        VALUES ($1, $2)
        ON CONFLICT (user_id) DO UPDATE SET available_at = EXCLUDED.available_at
        "#,
    )
    .bind(user_id)
    .bind(available_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(guild_id)
}

pub async fn list_members(db: &PgPool, guild_id: Uuid) -> Result<Vec<GuildMember>, AppError> {
    let rows =
        sqlx::query_as("SELECT * FROM guild_members WHERE guild_id = $1 ORDER BY role, joined_at")
            .bind(guild_id)
            .fetch_all(db)
            .await?;
    Ok(rows)
}

pub async fn role_of(
    db: &PgPool,
    guild_id: Uuid,
    user_id: Uuid,
) -> Result<Option<String>, AppError> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT role FROM guild_members WHERE guild_id = $1 AND user_id = $2")
            .bind(guild_id)
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    Ok(row.map(|(r,)| r))
}

pub fn is_officer(role: &str) -> bool {
    matches!(role, "founder" | "officer")
}

pub async fn promote(
    db: &PgPool,
    guild_id: Uuid,
    actor_id: Uuid,
    target_id: Uuid,
    new_role: &str,
) -> Result<(), AppError> {
    if !matches!(new_role, "officer" | "member" | "recruit") {
        return Err(AppError::Validation(
            "new_role must be officer | member | recruit".into(),
        ));
    }
    let actor_role = role_of(db, guild_id, actor_id)
        .await?
        .ok_or(AppError::Forbidden)?;
    if !is_officer(&actor_role) {
        return Err(AppError::Forbidden);
    }
    sqlx::query(
        "UPDATE guild_members SET role = $1 WHERE guild_id = $2 AND user_id = $3 AND role <> 'founder'",
    )
    .bind(new_role)
    .bind(guild_id)
    .bind(target_id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn kick_member(
    db: &PgPool,
    guild_id: Uuid,
    actor_id: Uuid,
    target_id: Uuid,
) -> Result<(), AppError> {
    let actor_role = role_of(db, guild_id, actor_id)
        .await?
        .ok_or(AppError::Forbidden)?;
    if !is_officer(&actor_role) {
        return Err(AppError::Forbidden);
    }
    let target_role = role_of(db, guild_id, target_id).await?;
    if target_role.as_deref() == Some("founder") {
        return Err(AppError::Validation("Cannot kick the founder".into()));
    }
    sqlx::query("DELETE FROM guild_members WHERE guild_id = $1 AND user_id = $2")
        .bind(guild_id)
        .bind(target_id)
        .execute(db)
        .await?;
    let available_at = Utc::now() + ChronoDuration::days(LEAVE_COOLDOWN_DAYS);
    sqlx::query(
        r#"
        INSERT INTO user_guild_cooldown (user_id, available_at)
        VALUES ($1, $2)
        ON CONFLICT (user_id) DO UPDATE SET available_at = EXCLUDED.available_at
        "#,
    )
    .bind(target_id)
    .bind(available_at)
    .execute(db)
    .await?;
    Ok(())
}

// ─── GP integration ───────────────────────────────────────────────

/// P10.5 — Push a collective GP bonus to a specific guild, independent of individual members.
///
/// Utilisé quand une team liée à une guilde submit un team challenge : en plus
/// du 10% par membre distribué via `award_gp_for_fragments`, on abonde la guilde
/// avec 10% du total collectif — la victoire team = coup de force pour la guilde.
pub async fn award_bonus_gp_for_team(
    db: &PgPool,
    guild_id: Uuid,
    total_fragments: i32,
) -> Result<i64, AppError> {
    if total_fragments <= 0 {
        return Ok(0);
    }
    let gp_to_add = (total_fragments as i64) * (GP_PERCENT_FROM_FRAGMENTS as i64) / 100;
    if gp_to_add <= 0 {
        return Ok(0);
    }
    sqlx::query(
        "UPDATE guilds
         SET gp_total = gp_total + $1,
             gp_season = gp_season + $1,
             updated_at = NOW()
         WHERE id = $2 AND disbanded_at IS NULL",
    )
    .bind(gp_to_add)
    .bind(guild_id)
    .execute(db)
    .await?;
    Ok(gp_to_add)
}

/// Push 10% of `fragments_earned` to the user's guild (if any). Returns the GP added.
pub async fn award_gp_for_fragments(
    db: &PgPool,
    user_id: Uuid,
    fragments_earned: i32,
) -> Result<i64, AppError> {
    if fragments_earned <= 0 {
        return Ok(0);
    }
    let gp_to_add = (fragments_earned as i64) * (GP_PERCENT_FROM_FRAGMENTS as i64) / 100;
    if gp_to_add <= 0 {
        return Ok(0);
    }
    let row: Option<(Uuid,)> =
        sqlx::query_as("SELECT guild_id FROM guild_members WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    let Some((guild_id,)) = row else {
        return Ok(0);
    };
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE guilds SET gp_total = gp_total + $1, gp_season = gp_season + $1, updated_at = NOW() WHERE id = $2 AND disbanded_at IS NULL")
        .bind(gp_to_add)
        .bind(guild_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE guild_members SET gp_contributed = gp_contributed + $1 WHERE guild_id = $2 AND user_id = $3",
    )
    .bind(gp_to_add)
    .bind(guild_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(gp_to_add)
}

// ─── P10.6 : skill matrix par guilde ─────────────────────────────

/// Ligne d'agrégat par (guilde, domaine) : combien de membres pratiquent ce
/// domaine, leur niveau moyen, et les 3 skills les plus pratiqués dedans.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct GuildDomainRow {
    pub domain: String,
    pub member_count: i64,
    pub avg_level: Option<f64>,
    pub top_skills: Vec<String>,
}

/// Skill matrix d'une guilde : agrégat par domaine.
///
/// Computed à la volée (pas de matview) — le volume est petit (30 members × ~10 skills
/// prouvés/membre × 7 domaines = ~2100 rows max). Cache Redis peut être ajouté
/// plus tard si mesure de perf le justifie.
pub async fn guild_skill_matrix(
    db: &PgPool,
    guild_id: Uuid,
) -> Result<Vec<GuildDomainRow>, AppError> {
    let rows: Vec<GuildDomainRow> = sqlx::query_as(
        r#"
        WITH member_skills AS (
            SELECT us.user_id, us.skill_id, us.proficiency_level,
                   sn.slug, sn.domain
            FROM guild_members gm
            JOIN user_skills us ON us.user_id = gm.user_id
            JOIN skill_nodes sn ON sn.id = us.skill_id
            WHERE gm.guild_id = $1
              AND us.proven_count > 0
        ),
        per_domain AS (
            SELECT domain,
                   COUNT(DISTINCT user_id) AS member_count,
                   AVG(proficiency_level)::DOUBLE PRECISION AS avg_level
            FROM member_skills
            GROUP BY domain
        ),
        top_slugs AS (
            SELECT domain, slug,
                   ROW_NUMBER() OVER (
                       PARTITION BY domain
                       ORDER BY COUNT(*) DESC, MAX(proficiency_level) DESC
                   ) AS rn
            FROM member_skills
            GROUP BY domain, slug
        )
        SELECT pd.domain,
               pd.member_count,
               pd.avg_level,
               COALESCE(ARRAY_AGG(ts.slug ORDER BY ts.rn)
                        FILTER (WHERE ts.rn <= 3), ARRAY[]::TEXT[]) AS top_skills
        FROM per_domain pd
        LEFT JOIN top_slugs ts ON ts.domain = pd.domain AND ts.rn <= 3
        GROUP BY pd.domain, pd.member_count, pd.avg_level
        ORDER BY pd.member_count DESC, pd.domain
        "#,
    )
    .bind(guild_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

// ─── Leaderboards ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct GuildLeaderboardEntry {
    pub id: Uuid,
    pub slug: String,
    pub tag: String,
    pub name: String,
    pub level: i32,
    pub gp_total: i64,
    pub gp_season: i64,
    pub division: String,
    pub member_count: i64,
}

pub async fn leaderboard(
    db: &PgPool,
    season_only: bool,
    division_filter: Option<&str>,
    limit: i64,
) -> Result<Vec<GuildLeaderboardEntry>, AppError> {
    let order_col = if season_only { "gp_season" } else { "gp_total" };
    let sql = format!(
        r#"
        SELECT g.id, g.slug, g.tag, g.name, g.level, g.gp_total, g.gp_season, g.division,
               (SELECT COUNT(*) FROM guild_members WHERE guild_id = g.id)::BIGINT AS member_count
        FROM guilds g
        WHERE g.disbanded_at IS NULL
          AND ($1::text IS NULL OR g.division = $1)
        ORDER BY g.{order_col} DESC
        LIMIT $2
        "#
    );
    let rows = sqlx::query_as(&sql)
        .bind(division_filter)
        .bind(limit.clamp(1, 200))
        .fetch_all(db)
        .await?;
    Ok(rows)
}

// ─── Invitations ──────────────────────────────────────────────────

pub async fn invite_direct(
    db: &PgPool,
    inviter_id: Uuid,
    guild_id: Uuid,
    invited_user_id: Uuid,
) -> Result<GuildInvitation, AppError> {
    let role = role_of(db, guild_id, inviter_id)
        .await?
        .ok_or(AppError::Forbidden)?;
    if !is_officer(&role) {
        return Err(AppError::Forbidden);
    }
    if invited_user_id == inviter_id {
        return Err(AppError::Validation("Cannot invite yourself".into()));
    }
    let expires_at = Utc::now() + ChronoDuration::days(INVITE_DIRECT_TTL_DAYS);
    let invite: GuildInvitation = sqlx::query_as(
        r#"
        INSERT INTO guild_invitations (guild_id, inviter_id, invited_user_id, expires_at)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(guild_id)
    .bind(inviter_id)
    .bind(invited_user_id)
    .bind(expires_at)
    .fetch_one(db)
    .await?;
    Ok(invite)
}

pub async fn create_shareable_token(
    db: &PgPool,
    inviter_id: Uuid,
    guild_id: Uuid,
) -> Result<GuildInvitation, AppError> {
    let role = role_of(db, guild_id, inviter_id)
        .await?
        .ok_or(AppError::Forbidden)?;
    if !is_officer(&role) {
        return Err(AppError::Forbidden);
    }
    // Strong-enough token: two UUIDv4 hex-encoded back-to-back (256 bits of entropy).
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let expires_at = Utc::now() + ChronoDuration::days(INVITE_TOKEN_TTL_DAYS);
    let invite: GuildInvitation = sqlx::query_as(
        r#"
        INSERT INTO guild_invitations (guild_id, inviter_id, token, expires_at)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(guild_id)
    .bind(inviter_id)
    .bind(&token)
    .bind(expires_at)
    .fetch_one(db)
    .await?;
    Ok(invite)
}

pub async fn accept_direct_invitation(
    db: &PgPool,
    invitation_id: Uuid,
    user_id: Uuid,
) -> Result<Uuid, AppError> {
    let mut tx = db.begin().await?;
    let row: Option<GuildInvitation> = sqlx::query_as(
        "SELECT * FROM guild_invitations WHERE id = $1 AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(invitation_id)
    .fetch_optional(&mut *tx)
    .await?;
    let invite = row.ok_or(AppError::NotFound(
        "invitation not found or already used".into(),
    ))?;
    if invite.expires_at < Utc::now() {
        return Err(AppError::Validation("Invitation expired".into()));
    }
    if invite.invited_user_id != Some(user_id) {
        return Err(AppError::Forbidden);
    }
    sqlx::query("UPDATE guild_invitations SET accepted_at = NOW() WHERE id = $1")
        .bind(invitation_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    // Now join (separate tx — keeps locking minimal). If it fails, the invitation stays marked
    // accepted ; the user can be added manually if needed (rare).
    join_guild(db, invite.guild_id, user_id).await?;
    Ok(invite.guild_id)
}

pub async fn join_by_token(db: &PgPool, token: &str, user_id: Uuid) -> Result<Uuid, AppError> {
    let row: Option<GuildInvitation> = sqlx::query_as(
        "SELECT * FROM guild_invitations WHERE token = $1 AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(token)
    .fetch_optional(db)
    .await?;
    let invite = row.ok_or(AppError::NotFound("invalid or used token".into()))?;
    if invite.expires_at < Utc::now() {
        return Err(AppError::Validation("Token expired".into()));
    }
    join_guild(db, invite.guild_id, user_id).await?;
    // Token-link invites can be used multiple times within their TTL ; do not mark as accepted.
    Ok(invite.guild_id)
}

// ─── Applications ─────────────────────────────────────────────────

pub async fn apply_to_guild(
    db: &PgPool,
    guild_id: Uuid,
    applicant_id: Uuid,
    message: &str,
) -> Result<GuildApplication, AppError> {
    let guild: Guild =
        sqlx::query_as("SELECT * FROM guilds WHERE id = $1 AND disbanded_at IS NULL")
            .bind(guild_id)
            .fetch_optional(db)
            .await?
            .ok_or(AppError::NotFound("guild not found".into()))?;
    if guild.membership_mode == "invite_only" {
        return Err(AppError::Validation(
            "Guild does not accept applications".into(),
        ));
    }
    if guild.membership_mode == "open" {
        // For 'open' guilds, fold "apply" into immediate join
        join_guild(db, guild_id, applicant_id).await?;
        let app: GuildApplication = sqlx::query_as(
            r#"
            INSERT INTO guild_applications (guild_id, applicant_id, message, status, decided_at)
            VALUES ($1, $2, $3, 'accepted', NOW())
            RETURNING *
            "#,
        )
        .bind(guild_id)
        .bind(applicant_id)
        .bind(message.trim())
        .fetch_one(db)
        .await?;
        return Ok(app);
    }
    let app: GuildApplication = sqlx::query_as(
        r#"
        INSERT INTO guild_applications (guild_id, applicant_id, message, status)
        VALUES ($1, $2, $3, 'pending')
        RETURNING *
        "#,
    )
    .bind(guild_id)
    .bind(applicant_id)
    .bind(message.trim())
    .fetch_one(db)
    .await?;
    Ok(app)
}

pub async fn decide_application(
    db: &PgPool,
    application_id: Uuid,
    decider_id: Uuid,
    accept: bool,
) -> Result<GuildApplication, AppError> {
    let app: Option<GuildApplication> =
        sqlx::query_as("SELECT * FROM guild_applications WHERE id = $1 AND status = 'pending'")
            .bind(application_id)
            .fetch_optional(db)
            .await?;
    let app = app.ok_or(AppError::NotFound("application not found".into()))?;
    let role = role_of(db, app.guild_id, decider_id)
        .await?
        .ok_or(AppError::Forbidden)?;
    if !is_officer(&role) {
        return Err(AppError::Forbidden);
    }
    let new_status = if accept { "accepted" } else { "rejected" };
    let updated: GuildApplication = sqlx::query_as(
        r#"
        UPDATE guild_applications
        SET status = $1, decided_by_id = $2, decided_at = NOW()
        WHERE id = $3
        RETURNING *
        "#,
    )
    .bind(new_status)
    .bind(decider_id)
    .bind(application_id)
    .fetch_one(db)
    .await?;
    if accept {
        join_guild(db, app.guild_id, app.applicant_id).await?;
    }
    Ok(updated)
}

// ─── Guild Wars ───────────────────────────────────────────────────

/// Returns the (min_pct, max_pct) stake range for a given division, expressed as integers /100.
pub fn stake_pct_range_for_division(division: &str) -> (i32, i32) {
    match division {
        "bronze" => (1, 2),
        "silver" => (2, 3),
        "gold" => (3, 5),
        "platinum" => (4, 7),
        "legende" => (5, 10),
        _ => (1, 2),
    }
}

/// Resolve the allowed stake amount based on both guilds' divisions and gp_total.
/// The challenger picks a stake_gp value ; we validate it falls in the legal band.
pub fn validate_stake_amount(
    challenger_division: &str,
    challenger_gp: i64,
    defender_gp: i64,
    proposed_stake_gp: i64,
) -> Result<(), AppError> {
    let (min_pct, max_pct) = stake_pct_range_for_division(challenger_division);
    let smaller_gp = challenger_gp.min(defender_gp).max(0);
    let min_stake = ((smaller_gp * min_pct as i64) / 100).max(1);
    let max_stake = ((smaller_gp * max_pct as i64) / 100).max(min_stake);
    if proposed_stake_gp < min_stake || proposed_stake_gp > max_stake {
        return Err(AppError::Validation(format!(
            "stake_gp must be between {min_stake} and {max_stake} for division {challenger_division}"
        )));
    }
    Ok(())
}

pub async fn propose_war(
    db: &PgPool,
    challenger_id: Uuid,
    challenger_guild_id: Uuid,
    defender_guild_id: Uuid,
    stake_gp: i64,
) -> Result<GuildWar, AppError> {
    if challenger_guild_id == defender_guild_id {
        return Err(AppError::Validation("Cannot war yourself".into()));
    }
    let role = role_of(db, challenger_guild_id, challenger_id)
        .await?
        .ok_or(AppError::Forbidden)?;
    if !is_officer(&role) {
        return Err(AppError::Forbidden);
    }

    let challenger: Guild =
        sqlx::query_as("SELECT * FROM guilds WHERE id = $1 AND disbanded_at IS NULL")
            .bind(challenger_guild_id)
            .fetch_optional(db)
            .await?
            .ok_or(AppError::NotFound("challenger guild not found".into()))?;
    let defender: Guild =
        sqlx::query_as("SELECT * FROM guilds WHERE id = $1 AND disbanded_at IS NULL")
            .bind(defender_guild_id)
            .fetch_optional(db)
            .await?
            .ok_or(AppError::NotFound("defender guild not found".into()))?;

    validate_stake_amount(
        &challenger.division,
        challenger.gp_total,
        defender.gp_total,
        stake_gp,
    )?;

    // Refuse if an active war already exists between these two
    let existing: Option<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT id FROM guild_wars
        WHERE status IN ('proposed', 'accepted')
          AND ((challenger_guild_id = $1 AND defender_guild_id = $2)
            OR (challenger_guild_id = $2 AND defender_guild_id = $1))
        "#,
    )
    .bind(challenger_guild_id)
    .bind(defender_guild_id)
    .fetch_optional(db)
    .await?;
    if existing.is_some() {
        return Err(AppError::Validation(
            "A war is already proposed or in progress between these guilds".into(),
        ));
    }

    let war: GuildWar = sqlx::query_as(
        r#"
        INSERT INTO guild_wars (challenger_guild_id, defender_guild_id, stake_gp)
        VALUES ($1, $2, $3)
        RETURNING *
        "#,
    )
    .bind(challenger_guild_id)
    .bind(defender_guild_id)
    .bind(stake_gp)
    .fetch_one(db)
    .await?;
    Ok(war)
}

pub async fn respond_to_war(
    db: &PgPool,
    war_id: Uuid,
    decider_id: Uuid,
    accept: bool,
) -> Result<GuildWar, AppError> {
    let war: Option<GuildWar> =
        sqlx::query_as("SELECT * FROM guild_wars WHERE id = $1 AND status = 'proposed'")
            .bind(war_id)
            .fetch_optional(db)
            .await?;
    let war = war.ok_or(AppError::NotFound(
        "war not found or already decided".into(),
    ))?;
    let role = role_of(db, war.defender_guild_id, decider_id)
        .await?
        .ok_or(AppError::Forbidden)?;
    if !is_officer(&role) {
        return Err(AppError::Forbidden);
    }
    if war.proposed_at + ChronoDuration::hours(WAR_DECISION_WINDOW_HOURS) < Utc::now() {
        // The 48h window has lapsed → auto-reject
        let _ = sqlx::query(
            "UPDATE guild_wars SET status = 'rejected', decided_at = NOW() WHERE id = $1",
        )
        .bind(war_id)
        .execute(db)
        .await;
        return Err(AppError::Validation("Decision window expired".into()));
    }

    let new_status = if accept { "accepted" } else { "rejected" };
    let mut tx = db.begin().await?;
    let updated: GuildWar = if accept {
        let ends_at = Utc::now() + ChronoDuration::days(WAR_DEFAULT_DURATION_DAYS);
        // Lock the stake from both guilds' GP
        sqlx::query("UPDATE guilds SET gp_total = gp_total - $1 WHERE id = $2 AND gp_total >= $1")
            .bind(war.stake_gp)
            .bind(war.challenger_guild_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE guilds SET gp_total = gp_total - $1 WHERE id = $2 AND gp_total >= $1")
            .bind(war.stake_gp)
            .bind(war.defender_guild_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query_as(
            "UPDATE guild_wars SET status = 'accepted', decided_at = NOW(), ends_at = $1 WHERE id = $2 RETURNING *",
        )
        .bind(ends_at)
        .bind(war_id)
        .fetch_one(&mut *tx)
        .await?
    } else {
        sqlx::query_as(
            "UPDATE guild_wars SET status = $1, decided_at = NOW() WHERE id = $2 RETURNING *",
        )
        .bind(new_status)
        .bind(war_id)
        .fetch_one(&mut *tx)
        .await?
    };
    tx.commit().await?;
    Ok(updated)
}

pub async fn conclude_war(
    db: &PgPool,
    war_id: Uuid,
    winner_guild_id: Uuid,
) -> Result<GuildWar, AppError> {
    let war: Option<GuildWar> =
        sqlx::query_as("SELECT * FROM guild_wars WHERE id = $1 AND status = 'accepted'")
            .bind(war_id)
            .fetch_optional(db)
            .await?;
    let war = war.ok_or(AppError::NotFound("war not active".into()))?;
    if winner_guild_id != war.challenger_guild_id && winner_guild_id != war.defender_guild_id {
        return Err(AppError::Validation(
            "winner must be one of the two warring guilds".into(),
        ));
    }
    let mut tx = db.begin().await?;
    // Winner gets both stakes ; loser is already debited from accept.
    let pot = war.stake_gp * 2;
    sqlx::query(
        "UPDATE guilds SET gp_total = gp_total + $1, gp_season = gp_season + $1 WHERE id = $2",
    )
    .bind(pot)
    .bind(winner_guild_id)
    .execute(&mut *tx)
    .await?;
    let updated: GuildWar = sqlx::query_as(
        r#"
        UPDATE guild_wars
        SET status = 'concluded', winner_guild_id = $1, concluded_at = NOW()
        WHERE id = $2
        RETURNING *
        "#,
    )
    .bind(winner_guild_id)
    .bind(war_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(updated)
}

// ─── Lookups ──────────────────────────────────────────────────────

pub async fn by_slug(db: &PgPool, slug: &str) -> Result<Guild, AppError> {
    sqlx::query_as("SELECT * FROM guilds WHERE slug = $1 AND disbanded_at IS NULL")
        .bind(slug)
        .fetch_optional(db)
        .await?
        .ok_or(AppError::NotFound("guild not found".into()))
}

pub async fn by_id(db: &PgPool, id: Uuid) -> Result<Guild, AppError> {
    sqlx::query_as("SELECT * FROM guilds WHERE id = $1 AND disbanded_at IS NULL")
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or(AppError::NotFound("guild not found".into()))
}

// ─── Moderation ───────────────────────────────────────────────────

pub async fn admin_dissolve(db: &PgPool, guild_id: Uuid) -> Result<(), AppError> {
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE guilds SET disbanded_at = NOW() WHERE id = $1 AND disbanded_at IS NULL")
        .bind(guild_id)
        .execute(&mut *tx)
        .await?;
    // Remove all members ; this triggers cooldown via the leave path is too heavy here.
    // Just clear membership and set a short global cooldown for everyone.
    let cooldown_until = Utc::now() + ChronoDuration::days(LEAVE_COOLDOWN_DAYS);
    let members: Vec<(Uuid,)> =
        sqlx::query_as("SELECT user_id FROM guild_members WHERE guild_id = $1")
            .bind(guild_id)
            .fetch_all(&mut *tx)
            .await?;
    sqlx::query("DELETE FROM guild_members WHERE guild_id = $1")
        .bind(guild_id)
        .execute(&mut *tx)
        .await?;
    for (uid,) in &members {
        sqlx::query(
            r#"
            INSERT INTO user_guild_cooldown (user_id, available_at)
            VALUES ($1, $2)
            ON CONFLICT (user_id) DO UPDATE SET available_at = EXCLUDED.available_at
            "#,
        )
        .bind(uid)
        .bind(cooldown_until)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stake_band_bronze() {
        assert_eq!(stake_pct_range_for_division("bronze"), (1, 2));
    }

    #[test]
    fn stake_band_legende() {
        assert_eq!(stake_pct_range_for_division("legende"), (5, 10));
    }

    #[test]
    fn validate_stake_inside_band() {
        // bronze: 1-2% of min(100k, 200k) = 1000-2000
        assert!(validate_stake_amount("bronze", 100_000, 200_000, 1500).is_ok());
    }

    #[test]
    fn validate_stake_too_low() {
        assert!(validate_stake_amount("bronze", 100_000, 200_000, 500).is_err());
    }

    #[test]
    fn validate_stake_too_high() {
        assert!(validate_stake_amount("bronze", 100_000, 200_000, 5000).is_err());
    }

    #[test]
    fn is_officer_matrix() {
        assert!(is_officer("founder"));
        assert!(is_officer("officer"));
        assert!(!is_officer("member"));
        assert!(!is_officer("recruit"));
    }
}
