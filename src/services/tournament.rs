//! Seasons + tournaments service — Phase 2 Sprint 6.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const VALID_KINDS: &[&str] = &[
    "individual",
    "guild_war",
    "hackathon",
    "marathon",       // Format 1 Grande Epreuve : saisons impaires, cooperatif
    "defi_solitaire", // Format 3 : background permanent, defi ultime solo
    // Code contests (migration 0189). A code hackathon is a `hackathon` with
    // `skill_domain = 'code'`, not a fourth kind: the difference is subject,
    // not machinery.
    "code_golf",
    "tdd_contest",
    // AI contests, on the same machinery and distinguished by `skill_domain`.
    "benchmark_rush", // 48h to move a public benchmark, ladder-scored
    // Head to head on one task, decided by the room. Renamed from
    // `prompt_battle` in migration 0235: two designers on one logo is the
    // same event as two engineers on one prompt, and `skill_domain` is where
    // the difference belongs.
    "duel",
    // One written brief, N answers, a jury ranks them. Not a hackathon:
    // nobody builds against a clock.
    "brief_contest",
];
pub const VALID_FORMATS: &[&str] = &["swiss", "bracket", "ladder"];
pub const VALID_PARTICIPANT_TYPES: &[&str] = &["user", "guild"];

/// Kinds where the smallest number wins.
///
/// Only code golf, and that is the whole point of it. Anything ranked by
/// character count and sorted descending crowns the worst entry.
pub const LOWER_IS_BETTER_KINDS: &[&str] = &["code_golf"];

/// The end of the scale a kind is won at.
pub fn scoring_direction_for(kind: &str) -> &'static str {
    if LOWER_IS_BETTER_KINDS.contains(&kind) {
        "lower_is_better"
    } else {
        "higher_is_better"
    }
}

// ─── Seasons ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Season {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub status: String,
    pub closed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSeasonInput {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

pub async fn create_season(db: &PgPool, input: CreateSeasonInput) -> Result<Season, AppError> {
    if input.ends_at <= input.starts_at {
        return Err(AppError::Validation(
            "ends_at must be after starts_at".into(),
        ));
    }
    let row: Season = sqlx::query_as(
        r#"
        INSERT INTO seasons (slug, name, description, starts_at, ends_at)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING *
        "#,
    )
    .bind(input.slug.trim().to_lowercase())
    .bind(input.name.trim())
    .bind(input.description.as_deref().map(str::trim))
    .bind(input.starts_at)
    .bind(input.ends_at)
    .fetch_one(db)
    .await?;
    Ok(row)
}

pub async fn list_seasons(db: &PgPool) -> Result<Vec<Season>, AppError> {
    let rows = sqlx::query_as("SELECT * FROM seasons ORDER BY starts_at DESC LIMIT 50")
        .fetch_all(db)
        .await?;
    Ok(rows)
}

pub async fn current_season(db: &PgPool) -> Result<Option<Season>, AppError> {
    let row = sqlx::query_as(
        "SELECT * FROM seasons WHERE status = 'active' ORDER BY starts_at DESC LIMIT 1",
    )
    .fetch_optional(db)
    .await?;
    Ok(row)
}

pub async fn set_season_status(
    db: &PgPool,
    season_id: Uuid,
    status: &str,
) -> Result<Season, AppError> {
    if !matches!(status, "upcoming" | "active" | "ended") {
        return Err(AppError::Validation("invalid season status".into()));
    }
    let row: Season = sqlx::query_as(
        "UPDATE seasons SET status = $1, closed_at = CASE WHEN $1 = 'ended' THEN NOW() ELSE NULL END WHERE id = $2 RETURNING *",
    )
    .bind(status)
    .bind(season_id)
    .fetch_one(db)
    .await?;
    Ok(row)
}

/// End-of-season housekeeping: reset gp_season for every guild + recompute division ladder.
/// Top 20% bumped one rank, bottom 20% dropped one rank.
pub async fn close_season(db: &PgPool, season_id: Uuid) -> Result<SeasonCloseReport, AppError> {
    let mut tx = db.begin().await?;
    let season: Season = sqlx::query_as("SELECT * FROM seasons WHERE id = $1 FOR UPDATE")
        .bind(season_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("season not found".into()))?;
    if season.status == "ended" {
        return Err(AppError::Validation("season already ended".into()));
    }

    // Snapshot ranks per division before reset
    let snapshot: Vec<(Uuid, String, i64)> = sqlx::query_as(
        "SELECT id, division, gp_season FROM guilds WHERE disbanded_at IS NULL ORDER BY division, gp_season DESC",
    )
    .fetch_all(&mut *tx)
    .await?;

    let mut promotions = 0i64;
    let mut relegations = 0i64;

    let divisions = ["bronze", "silver", "gold", "platinum", "legende"];
    let div_index = |d: &str| divisions.iter().position(|x| *x == d).unwrap_or(0);

    // Bucket by division
    let mut by_div: std::collections::HashMap<String, Vec<(Uuid, i64)>> =
        std::collections::HashMap::new();
    for (id, div, gp) in snapshot {
        by_div.entry(div).or_default().push((id, gp));
    }

    for (div, mut entries) in by_div {
        // Already sorted desc thanks to the ORDER BY, but be defensive.
        entries.sort_by_key(|e| std::cmp::Reverse(e.1));
        let n = entries.len();
        if n < 5 {
            // Too small to ladder fairly — skip
            continue;
        }
        let top_n = ((n as f64) * 0.20).ceil() as usize;
        let bot_n = ((n as f64) * 0.20).ceil() as usize;
        let idx = div_index(&div);
        // Top X% promoted (if not already at the highest division)
        if idx < divisions.len() - 1 {
            for (id, _) in entries.iter().take(top_n) {
                let new_div = divisions[idx + 1];
                sqlx::query("UPDATE guilds SET division = $1 WHERE id = $2")
                    .bind(new_div)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                promotions += 1;
            }
        }
        // Bottom X% relegated (if not already at the lowest)
        if idx > 0 {
            for (id, _) in entries.iter().rev().take(bot_n) {
                let new_div = divisions[idx - 1];
                sqlx::query("UPDATE guilds SET division = $1 WHERE id = $2")
                    .bind(new_div)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                relegations += 1;
            }
        }
    }

    // Reset season GP for all active guilds
    let updated = sqlx::query("UPDATE guilds SET gp_season = 0 WHERE disbanded_at IS NULL")
        .execute(&mut *tx)
        .await?
        .rows_affected();

    // Mark season ended
    sqlx::query("UPDATE seasons SET status = 'ended', closed_at = NOW() WHERE id = $1")
        .bind(season_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(SeasonCloseReport {
        season_id,
        guilds_reset: updated as i64,
        promotions,
        relegations,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct SeasonCloseReport {
    pub season_id: Uuid,
    pub guilds_reset: i64,
    pub promotions: i64,
    pub relegations: i64,
}

// ─── Tournaments ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Tournament {
    pub id: Uuid,
    pub season_id: Option<Uuid>,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub kind: String,
    pub format: String,
    pub prize_pool_fragments: i32,
    pub prize_pool_gp: i32,
    pub sponsor_enterprise_id: Option<Uuid>,
    pub sponsor_logo_url: Option<String>,
    pub sponsor_blurb: Option<String>,
    pub registration_opens_at: Option<DateTime<Utc>>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub status: String,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// NULL means the contest is open to every domain.
    pub skill_domain: Option<String>,
    /// What the contest asks for. Shape depends on `kind` — see
    /// `contest::validate_rules`.
    pub rules: serde_json::Value,
    pub scoring_direction: String,
    /// While the contest is open, entrants read only their own entry. See
    /// migration 0244 for why this narrows *when* rather than *whether*.
    pub blind_until_close: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTournamentInput {
    pub season_id: Option<Uuid>,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub kind: String,
    pub format: Option<String>,
    pub prize_pool_fragments: Option<i32>,
    pub prize_pool_gp: Option<i32>,
    pub sponsor_enterprise_id: Option<Uuid>,
    pub sponsor_logo_url: Option<String>,
    pub sponsor_blurb: Option<String>,
    pub registration_opens_at: Option<DateTime<Utc>>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    #[serde(default)]
    pub skill_domain: Option<String>,
    #[serde(default)]
    pub rules: Option<serde_json::Value>,
}

pub async fn create_tournament(
    db: &PgPool,
    creator_id: Uuid,
    input: CreateTournamentInput,
) -> Result<Tournament, AppError> {
    if !VALID_KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::Validation(format!(
            "kind must be one of: {}",
            VALID_KINDS.join(", ")
        )));
    }
    let format = input.format.unwrap_or_else(|| "ladder".into());
    if !VALID_FORMATS.contains(&format.as_str()) {
        return Err(AppError::Validation(format!(
            "format must be one of: {}",
            VALID_FORMATS.join(", ")
        )));
    }
    if input.ends_at <= input.starts_at {
        return Err(AppError::Validation(
            "ends_at must be after starts_at".into(),
        ));
    }
    let slug = input.slug.trim().to_lowercase();
    if slug.len() < 2
        || slug.len() > 60
        || !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(AppError::Validation(
            "slug must be 2-60 lowercase alphanumeric/dash".into(),
        ));
    }
    if input.kind != "hackathon" && input.sponsor_enterprise_id.is_some() {
        return Err(AppError::Validation(
            "Only hackathon tournaments may have a sponsor".into(),
        ));
    }
    let rules = input.rules.unwrap_or_else(|| serde_json::json!({}));
    crate::services::contest::validate_rules(&input.kind, &rules)?;
    // Not a caller's choice: code golf is won at the bottom of the scale
    // whatever anybody types, and letting an admin invert it once would
    // silently crown the longest solution.
    let scoring_direction = scoring_direction_for(&input.kind);

    let t: Tournament = sqlx::query_as(
        r#"
        INSERT INTO tournaments
            (season_id, slug, name, description, kind, format,
             prize_pool_fragments, prize_pool_gp,
             sponsor_enterprise_id, sponsor_logo_url, sponsor_blurb,
             registration_opens_at, starts_at, ends_at, created_by,
             skill_domain, rules, scoring_direction)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)
        RETURNING *
        "#,
    )
    .bind(input.season_id)
    .bind(&slug)
    .bind(input.name.trim())
    .bind(input.description.as_deref().map(str::trim))
    .bind(&input.kind)
    .bind(&format)
    .bind(input.prize_pool_fragments.unwrap_or(0))
    .bind(input.prize_pool_gp.unwrap_or(0))
    .bind(input.sponsor_enterprise_id)
    .bind(input.sponsor_logo_url.as_deref())
    .bind(input.sponsor_blurb.as_deref())
    .bind(input.registration_opens_at)
    .bind(input.starts_at)
    .bind(input.ends_at)
    .bind(creator_id)
    .bind(input.skill_domain.as_deref())
    .bind(&rules)
    .bind(scoring_direction)
    .fetch_one(db)
    .await?;

    // Announced at creation rather than on a later status change: a contest
    // is created open, and a room that hears about it the day it closes has
    // been told nothing useful. A cross-domain contest routes to the
    // domain-blind room, which is what `skill_domain = NULL` resolves to.
    crate::services::discord_announce::contest_opened(
        db,
        t.skill_domain.as_deref().unwrap_or_default(),
        &t.slug,
        &t.name,
        t.ends_at,
        // The cash prize lives on the row but not on this struct, so it is
        // read back rather than guessed at. A contest funded after creation
        // simply announces without a figure, which is honest: at the moment
        // of the post there was none.
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT prize_cash_amount::TEXT || ' ' || prize_cash_currency
               FROM tournaments WHERE id = $1",
        )
        .bind(t.id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .flatten(),
    )
    .await;

    Ok(t)
}

/// What a caller may narrow the list by.
///
/// A struct rather than four positional arguments because every one of them
/// is optional and three of them are strings: `list_tournaments(db, None,
/// Some("design"), None, ...)` is a bug waiting for the day two of the
/// filters are transposed.
#[derive(Debug, Default, Clone)]
pub struct TournamentFilter<'a> {
    pub status: Option<&'a str>,
    /// `code_golf`, `brief_contest`, `duel`… — see `contest::VALID_KINDS`.
    pub kind: Option<&'a str>,
    /// A domain-scoped contest, plus the ones open to every domain: asking
    /// for design contests and being shown none of the cross-domain ones
    /// would hide exactly the events that want the widest field.
    pub skill_domain: Option<&'a str>,
    /// Only what somebody can still enter or watch.
    pub upcoming_or_active_only: bool,
    pub limit: i64,
}

/// List tournaments, narrowed in SQL.
///
/// The filters used to be three hand-written query strings and a `match` that
/// bound their parameters in the right order. Adding a fourth would have meant
/// eight strings, so the predicates are now expressed once and disabled by a
/// NULL bind. PostgreSQL still uses the indexes: `$n IS NULL OR col = $n`
/// short-circuits per row and every column here is either indexed or tiny.
///
/// Without `kind` and `skill_domain` the design contest page asked for two
/// hundred rows and filtered them in the browser, which stops working at the
/// two hundred and first tournament — silently, by dropping the oldest.
pub async fn list_tournaments(
    db: &PgPool,
    filter: TournamentFilter<'_>,
) -> Result<Vec<Tournament>, AppError> {
    let rows = sqlx::query_as(
        r#"
        SELECT * FROM tournaments
         WHERE ($1::text IS NULL OR status = $1)
           AND ($2::text IS NULL OR kind = $2)
           AND ($3::text IS NULL OR skill_domain = $3 OR skill_domain IS NULL)
           AND (NOT $4 OR status IN ('upcoming', 'registration', 'active'))
         ORDER BY CASE WHEN $4 THEN starts_at END ASC,
                  starts_at DESC
         LIMIT $5
        "#,
    )
    .bind(filter.status)
    .bind(filter.kind)
    .bind(filter.skill_domain)
    .bind(filter.upcoming_or_active_only)
    .bind(filter.limit.clamp(1, 100))
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn by_slug(db: &PgPool, slug: &str) -> Result<Tournament, AppError> {
    sqlx::query_as("SELECT * FROM tournaments WHERE slug = $1")
        .bind(slug)
        .fetch_optional(db)
        .await?
        .ok_or(AppError::NotFound("tournament not found".into()))
}

pub async fn set_status(
    db: &PgPool,
    tournament_id: Uuid,
    new_status: &str,
) -> Result<Tournament, AppError> {
    if !matches!(
        new_status,
        "upcoming" | "registration" | "active" | "concluded" | "cancelled"
    ) {
        return Err(AppError::Validation("invalid tournament status".into()));
    }
    sqlx::query_as(
        "UPDATE tournaments SET status = $1, updated_at = NOW() WHERE id = $2 RETURNING *",
    )
    .bind(new_status)
    .bind(tournament_id)
    .fetch_one(db)
    .await
    .map_err(AppError::from)
}

// ─── Participants ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TournamentParticipant {
    pub tournament_id: Uuid,
    pub participant_type: String,
    pub participant_id: Uuid,
    pub score: i32,
    pub rank: Option<i32>,
    pub prize_fragments_awarded: i32,
    pub prize_gp_awarded: i32,
    pub registered_at: DateTime<Utc>,
}

/// A leaderboard line, with whoever is on it.
///
/// Separate from [`TournamentParticipant`] because the identity is joined at
/// read time and is not part of the row: a participant is a foreign key, and
/// putting a display name on the stored shape would invite writing one.
///
/// Both name fields are optional, and that is the deleted-account case rather
/// than an oversight. A user row can go; the participation stays, because
/// removing it would rewrite a podium that other people were ranked against.
/// Such a line comes back nameless, and a reader shows it as withdrawn.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LeaderboardEntry {
    pub tournament_id: Uuid,
    pub participant_type: String,
    pub participant_id: Uuid,
    pub score: i32,
    pub rank: Option<i32>,
    pub prize_fragments_awarded: i32,
    pub prize_gp_awarded: i32,
    pub registered_at: DateTime<Utc>,
    /// The handle a reader can build a link from: the account's username, or
    /// the guild's slug. NULL when the account is gone.
    pub username: Option<String>,
    /// What to print: the person's display name, or the guild's name.
    pub display_name: Option<String>,
    /// The avatar, or the guild's logo.
    pub avatar_url: Option<String>,
}

pub async fn register_individual(
    db: &PgPool,
    tournament_id: Uuid,
    user_id: Uuid,
) -> Result<TournamentParticipant, AppError> {
    let t = sqlx::query_as::<_, Tournament>("SELECT * FROM tournaments WHERE id = $1")
        .bind(tournament_id)
        .fetch_optional(db)
        .await?
        .ok_or(AppError::NotFound("tournament not found".into()))?;
    // Stated the other way round on purpose. It used to allow `individual`
    // and `hackathon` and refuse everything else, so `marathon` and
    // `defi_solitaire` — added in migration 0114 — could be created and never
    // joined: the endpoint answered "not open to individual registration"
    // about tournaments that take nothing but individuals.
    //
    // Guild wars take guilds. Everything else takes people, including the two
    // AI kinds added here, and a new kind is now open by default rather than
    // silently closed.
    if t.kind == "guild_war" {
        return Err(AppError::Validation(
            "this tournament takes guilds, not individual registrations".into(),
        ));
    }
    if !matches!(t.status.as_str(), "registration" | "upcoming") {
        return Err(AppError::Validation(
            "registration is closed for this tournament".into(),
        ));
    }
    let row: TournamentParticipant = sqlx::query_as(
        r#"
        INSERT INTO tournament_participants (tournament_id, participant_type, participant_id)
        VALUES ($1, 'user', $2)
        ON CONFLICT (tournament_id, participant_type, participant_id) DO UPDATE SET registered_at = tournament_participants.registered_at
        RETURNING *
        "#,
    )
    .bind(tournament_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;
    Ok(row)
}

pub async fn register_guild(
    db: &PgPool,
    tournament_id: Uuid,
    requester_id: Uuid,
    guild_id: Uuid,
) -> Result<TournamentParticipant, AppError> {
    let t = sqlx::query_as::<_, Tournament>("SELECT * FROM tournaments WHERE id = $1")
        .bind(tournament_id)
        .fetch_optional(db)
        .await?
        .ok_or(AppError::NotFound("tournament not found".into()))?;
    if t.kind != "guild_war" {
        return Err(AppError::Validation(
            "this tournament is not a guild_war".into(),
        ));
    }
    if !matches!(t.status.as_str(), "registration" | "upcoming") {
        return Err(AppError::Validation(
            "registration is closed for this tournament".into(),
        ));
    }
    // Requester must be an officer of the guild
    let role: Option<(String,)> =
        sqlx::query_as("SELECT role FROM guild_members WHERE guild_id = $1 AND user_id = $2")
            .bind(guild_id)
            .bind(requester_id)
            .fetch_optional(db)
            .await?;
    let is_officer = matches!(
        role.map(|(r,)| r).as_deref(),
        Some("founder") | Some("officer")
    );
    if !is_officer {
        return Err(AppError::Forbidden);
    }
    let row: TournamentParticipant = sqlx::query_as(
        r#"
        INSERT INTO tournament_participants (tournament_id, participant_type, participant_id)
        VALUES ($1, 'guild', $2)
        ON CONFLICT (tournament_id, participant_type, participant_id) DO UPDATE SET registered_at = tournament_participants.registered_at
        RETURNING *
        "#,
    )
    .bind(tournament_id)
    .bind(guild_id)
    .fetch_one(db)
    .await?;
    Ok(row)
}

/// The ranking, with names on it.
///
/// A podium that can only print UUIDs is not a podium, so the identity is
/// joined here rather than left to N follow-up requests by the caller. Both
/// joins are LEFT, and a participant whose account is gone comes back with
/// null names instead of vanishing from the ranking — removing the line would
/// change the standing of everybody below it.
pub async fn leaderboard_of(
    db: &PgPool,
    tournament_id: Uuid,
) -> Result<Vec<LeaderboardEntry>, AppError> {
    // A participant who has not scored yet sorts last in both directions.
    // Without this, `lower_is_better` would put every unscored entry on top,
    // and zero would win a code golf.
    let rows = sqlx::query_as(
        r#"
        SELECT p.tournament_id,
               p.participant_type,
               p.participant_id,
               p.score,
               p.rank,
               p.prize_fragments_awarded,
               p.prize_gp_awarded,
               p.registered_at,
               COALESCE(u.username, g.slug) AS username,
               COALESCE(u.display_name, g.name) AS display_name,
               COALESCE(u.avatar_url, g.logo_url) AS avatar_url
          FROM tournament_participants p
          JOIN tournaments t ON t.id = p.tournament_id
          LEFT JOIN users u
                 ON p.participant_type = 'user' AND u.id = p.participant_id
          LEFT JOIN guilds g
                 ON p.participant_type = 'guild' AND g.id = p.participant_id
         WHERE p.tournament_id = $1
        ORDER BY p.rank NULLS LAST,
                 (p.score = 0) ASC,
                 CASE WHEN t.scoring_direction = 'lower_is_better'
                      THEN p.score
                 END ASC NULLS LAST,
                 CASE WHEN t.scoring_direction = 'higher_is_better'
                      THEN p.score
                 END DESC NULLS LAST,
                 p.registered_at ASC
        "#,
    )
    .bind(tournament_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn set_participant_score(
    db: &PgPool,
    tournament_id: Uuid,
    participant_type: &str,
    participant_id: Uuid,
    score: i32,
) -> Result<(), AppError> {
    if !VALID_PARTICIPANT_TYPES.contains(&participant_type) {
        return Err(AppError::Validation("invalid participant_type".into()));
    }
    sqlx::query(
        r#"
        UPDATE tournament_participants
        SET score = $1
        WHERE tournament_id = $2 AND participant_type = $3 AND participant_id = $4
        "#,
    )
    .bind(score)
    .bind(tournament_id)
    .bind(participant_type)
    .bind(participant_id)
    .execute(db)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct ConclusionReport {
    pub tournament_id: Uuid,
    pub participants_ranked: i64,
    pub fragments_distributed: i64,
    pub gp_distributed: i64,
}

/// Rank participants by current score (desc) and distribute the prize pool to the top 3
/// with a 50/30/20 split (rounded down; remainder stays in the platform).
pub async fn conclude_tournament(
    db: &PgPool,
    tournament_id: Uuid,
) -> Result<ConclusionReport, AppError> {
    let t = sqlx::query_as::<_, Tournament>("SELECT * FROM tournaments WHERE id = $1")
        .bind(tournament_id)
        .fetch_optional(db)
        .await?
        .ok_or(AppError::NotFound("tournament not found".into()))?;
    if t.status == "concluded" {
        return Err(AppError::Validation("tournament already concluded".into()));
    }

    let mut tx = db.begin().await?;

    // 1. Assign ranks, at the end of the scale this kind is won at. An
    //    unscored participant sorts last either way — zero characters is not
    //    a winning code golf entry, it is an absent one.
    let participants: Vec<(String, Uuid, i32)> = sqlx::query_as(
        r#"
        SELECT participant_type, participant_id, score
          FROM tournament_participants
         WHERE tournament_id = $1
         ORDER BY (score = 0) ASC,
                  CASE WHEN $2 = 'lower_is_better' THEN score END ASC NULLS LAST,
                  CASE WHEN $2 = 'higher_is_better' THEN score END DESC NULLS LAST,
                  registered_at ASC
        "#,
    )
    .bind(tournament_id)
    .bind(&t.scoring_direction)
    .fetch_all(&mut *tx)
    .await?;

    let total = participants.len() as i64;
    for (idx, (ptype, pid, _score)) in participants.iter().enumerate() {
        let rank = (idx as i32) + 1;
        sqlx::query(
            "UPDATE tournament_participants SET rank = $1 WHERE tournament_id = $2 AND participant_type = $3 AND participant_id = $4",
        )
        .bind(rank)
        .bind(tournament_id)
        .bind(ptype)
        .bind(pid)
        .execute(&mut *tx)
        .await?;
    }

    // 2. Distribute prize pool 50/30/20 to top 3
    let split = [50, 30, 20];
    let mut fragments_paid = 0i64;
    let mut gp_paid = 0i64;
    for (i, (ptype, pid, _)) in participants.iter().take(3).enumerate() {
        let frags = (t.prize_pool_fragments as i64) * (split[i] as i64) / 100;
        let gp = (t.prize_pool_gp as i64) * (split[i] as i64) / 100;
        if frags > 0 || gp > 0 {
            sqlx::query(
                r#"
                UPDATE tournament_participants
                SET prize_fragments_awarded = $1, prize_gp_awarded = $2
                WHERE tournament_id = $3 AND participant_type = $4 AND participant_id = $5
                "#,
            )
            .bind(frags as i32)
            .bind(gp as i32)
            .bind(tournament_id)
            .bind(ptype)
            .bind(pid)
            .execute(&mut *tx)
            .await?;
        }
        // Pay out:
        if ptype == "user" && frags > 0 {
            sqlx::query("UPDATE users SET total_fragments = total_fragments + $1 WHERE id = $2")
                .bind(frags as i32)
                .bind(pid)
                .execute(&mut *tx)
                .await?;
            fragments_paid += frags;
        }
        if ptype == "guild" && gp > 0 {
            sqlx::query("UPDATE guilds SET gp_total = gp_total + $1, gp_season = gp_season + $1 WHERE id = $2")
                .bind(gp)
                .bind(pid)
                .execute(&mut *tx)
                .await?;
            gp_paid += gp;
        }
    }

    // 3. Mark concluded
    sqlx::query("UPDATE tournaments SET status = 'concluded', updated_at = NOW() WHERE id = $1")
        .bind(tournament_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    announce_result(db, &t, &participants).await;

    Ok(ConclusionReport {
        tournament_id,
        participants_ranked: total,
        fragments_distributed: fragments_paid,
        gp_distributed: gp_paid,
    })
}

/// Tell everybody who took part that the ranking is out.
///
/// Two notifications, not one, and not one per outcome: the ranking is the
/// same event for everybody, and the podium is a second, rarer event that
/// only three people get. `tournament.podium` already existed and already
/// carries the place, so no `contest_winner` kind is invented for the same
/// moment — that would mean two celebrations for one result.
///
/// Guild entries are skipped: `Recipient` addresses people, and notifying a
/// guild means notifying its members, which is `guild.*`'s business.
///
/// Everything here is after the commit and every failure is logged rather
/// than raised. The ranking is final at this point; a mail server being down
/// must not make an organiser conclude the contest twice.
async fn announce_result(db: &PgPool, t: &Tournament, participants: &[(String, Uuid, i32)]) {
    // The room hears about the winner. One post, not one per participant: a
    // ranking is news, a full leaderboard pasted into a chat channel is not.
    if let Some((_, winner_id, _)) = participants
        .iter()
        .find(|(ptype, _, _)| ptype == "user")
    {
        let username: Option<String> =
            sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
                .bind(winner_id)
                .fetch_optional(db)
                .await
                .unwrap_or_default();
        if let Some(username) = username {
            crate::services::discord_announce::contest_won(
                db,
                t.skill_domain.as_deref().unwrap_or_default(),
                &t.slug,
                &t.name,
                &username,
            )
            .await;
        }
    }

    for (index, (ptype, pid, _)) in participants.iter().enumerate() {
        if ptype != "user" {
            continue;
        }
        if let Err(e) = crate::services::notify::send(
            crate::services::notify::Ctx::db_only(db),
            crate::services::notify::Recipient::User(*pid),
            "contest.concluded",
        )
        .arg("contest", t.name.clone())
        .payload(serde_json::json!({
            "tournament_id": t.id,
            "tournament_slug": t.slug,
            "rank": index + 1,
        }))
        .execute()
        .await
        {
            tracing::warn!(tournament_id = %t.id, error = %e, "result notification not delivered");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_enums() {
        assert!(VALID_KINDS.contains(&"individual"));
        assert!(VALID_KINDS.contains(&"guild_war"));
        assert!(VALID_KINDS.contains(&"hackathon"));
        // Grande Epreuve Formats 1 + 3 (migration 0114).
        assert!(VALID_KINDS.contains(&"marathon"));
        assert!(VALID_KINDS.contains(&"defi_solitaire"));
        assert!(VALID_FORMATS.contains(&"swiss"));
        assert!(VALID_FORMATS.contains(&"bracket"));
        assert!(VALID_FORMATS.contains(&"ladder"));
        assert!(VALID_PARTICIPANT_TYPES.contains(&"user"));
        assert!(VALID_PARTICIPANT_TYPES.contains(&"guild"));
    }
}
