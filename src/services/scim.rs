//! SCIM 2.0 provisioning service — token mgmt, user/group persistence,
//! deprovisioning semantics.
//!
//! Auth: each enterprise config carries at most one active bearer token,
//! stored as SHA-256. The cleartext is shown to the owner only once at
//! creation and cannot be recovered afterwards.
//!
//! Deprovisioning contract: PATCH `active=false` or DELETE on a user →
//!   - membership status flipped to `revoked`
//!   - every non-revoked session for that user is killed (login_method-agnostic)
//!
//! The user row itself is never deleted (audit / RGPD retention).

use base64::Engine;
use chrono::{DateTime, Utc};
use rand_core::RngCore;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

// Type aliases pour clippy::type_complexity (rangées sqlx::query_as).
type ScimRow130 = (
    Uuid,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<DateTime<Utc>>,
);
type ScimRow307 = (
    Uuid,
    Option<String>,
    String,
    String,
    String,
    String,
    String,
    String,
    DateTime<Utc>,
    DateTime<Utc>,
);
type ScimRow343 = (
    Uuid,
    Option<String>,
    String,
    String,
    String,
    String,
    String,
    String,
    DateTime<Utc>,
    DateTime<Utc>,
);
type ScimRow523 = (
    Uuid,
    Option<String>,
    String,
    Option<String>,
    DateTime<Utc>,
    DateTime<Utc>,
);
type ScimRow564 = (
    Uuid,
    Option<String>,
    String,
    Option<String>,
    DateTime<Utc>,
    DateTime<Utc>,
);

// ─── Token generation & verification ─────────────────────────────

/// Generate a fresh SCIM bearer token. The cleartext is returned once to the
/// caller (owner); only the SHA-256 hash is persisted.
pub fn generate_token() -> (String, Vec<u8>) {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("OS RNG");
    // A dedicated `scim_` prefix helps operators recognise these tokens in
    // logs / secret managers without leaking entropy.
    let cleartext = format!(
        "scim_{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    );
    let hash = hash_token(&cleartext);
    (cleartext, hash)
}

pub fn hash_token(token: &str) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    h.finalize().to_vec()
}

/// Constant-time comparison so a timing attacker can't distinguish a wrong
/// token from a near-miss.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

// Silence rand_core dep warning without pulling in an extra `rand`.
fn _rng_unused() {
    let mut _rng = rand_core::OsRng;
    let mut buf = [0u8; 4];
    _rng.fill_bytes(&mut buf);
}

// ─── Token DB ops ────────────────────────────────────────────────

/// Duration a rotated-out token remains valid alongside the new one so
/// operators can update their IdP without failed sync windows.
pub const TOKEN_ROTATION_GRACE_HOURS: i64 = 24;

pub async fn set_token(
    db: &PgPool,
    enterprise_id: Uuid,
    token_hash: &[u8],
) -> Result<(), AppError> {
    // Rotation: the previous current token becomes the graceful `previous`
    // slot (24h TTL). Only one previous slot exists ; consecutive rotations
    // within the grace window drop the older previous.
    let affected = sqlx::query(
        "UPDATE enterprise_sso_configs
         SET previous_scim_token_hash = scim_token_hash,
             previous_scim_token_expires_at = CASE
                 WHEN scim_token_hash IS NOT NULL
                 THEN NOW() + make_interval(hours => $3::INT)
                 ELSE NULL
             END,
             scim_token_hash = $1,
             scim_last_used_at = NULL,
             updated_at = NOW()
         WHERE enterprise_id = $2",
    )
    .bind(token_hash)
    .bind(enterprise_id)
    .bind(TOKEN_ROTATION_GRACE_HOURS as i32)
    .execute(db)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::Validation(
            "Enterprise SSO must be configured before enabling SCIM".into(),
        ));
    }
    Ok(())
}

pub async fn clear_token(db: &PgPool, enterprise_id: Uuid) -> Result<(), AppError> {
    // Explicit clear kills BOTH the current and the previous slot — the owner
    // wants SCIM off, not a grace period.
    sqlx::query(
        "UPDATE enterprise_sso_configs
         SET scim_token_hash = NULL,
             previous_scim_token_hash = NULL,
             previous_scim_token_expires_at = NULL,
             scim_last_used_at = NULL,
             updated_at = NOW()
         WHERE enterprise_id = $1",
    )
    .bind(enterprise_id)
    .execute(db)
    .await?;
    Ok(())
}

/// Resolve a bearer token to the enterprise it authenticates. Accepts either
/// the current token or the previous rotated-out token if it's still within
/// its grace window. Bumps `scim_last_used_at` for observability.
pub async fn resolve_token(db: &PgPool, cleartext: &str) -> Result<Uuid, AppError> {
    let hash = hash_token(cleartext);
    let candidates: Vec<ScimRow130> = sqlx::query_as(
        r#"
        SELECT enterprise_id,
               scim_token_hash,
               previous_scim_token_hash,
               previous_scim_token_expires_at
        FROM enterprise_sso_configs
        WHERE disabled_at IS NULL
          AND (scim_token_hash IS NOT NULL OR previous_scim_token_hash IS NOT NULL)
        "#,
    )
    .fetch_all(db)
    .await?;

    for (eid, current, previous, prev_expires) in candidates {
        let mut ok = false;
        if let Some(cur) = current
            && constant_time_eq(&cur, &hash)
        {
            ok = true;
        }
        if !ok
            && let (Some(prev), Some(expires)) = (previous, prev_expires)
            && expires > Utc::now()
            && constant_time_eq(&prev, &hash)
        {
            ok = true;
        }
        if ok {
            let _ = sqlx::query(
                "UPDATE enterprise_sso_configs SET scim_last_used_at = NOW() WHERE enterprise_id = $1",
            )
            .bind(eid)
            .execute(db)
            .await;
            return Ok(eid);
        }
    }
    Err(AppError::Unauthorized)
}

// ─── SCIM User persistence ───────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ScimUserView {
    pub id: Uuid,
    pub external_id: Option<String>,
    pub user_name: String,
    pub email: String,
    pub given_name: String,
    pub family_name: String,
    pub display_name: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct NewScimUser<'a> {
    pub enterprise_id: Uuid,
    pub external_id: Option<&'a str>,
    pub user_name: &'a str,
    pub email: &'a str,
    pub given_name: Option<&'a str>,
    pub family_name: Option<&'a str>,
    pub display_name: Option<&'a str>,
    pub default_role: &'a str,
    pub active: bool,
}

/// Provision a user via SCIM: creates/upserts the user, links `scim_external_id`,
/// attaches an active `enterprise_members` row (or activates the existing one).
///
/// Idempotency: if `external_id` is set and already exists in the DB, returns
/// AppError::Validation("already exists") so the caller can map it to 409.
pub async fn provision_user(db: &PgPool, new: NewScimUser<'_>) -> Result<Uuid, AppError> {
    let email_lower = new.email.trim().to_lowercase();
    let user_name_lower = new.user_name.trim().to_lowercase();

    // Enforce SCIM idempotency on externalId.
    if let Some(ext) = new.external_id {
        let existing: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM users WHERE scim_external_id = $1")
                .bind(ext)
                .fetch_optional(db)
                .await?;
        if existing.is_some() {
            return Err(AppError::Validation(
                "User with this externalId already exists".into(),
            ));
        }
    }

    let existing_by_email: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM users WHERE LOWER(email) = $1")
            .bind(&email_lower)
            .fetch_optional(db)
            .await?;

    let user_id = if let Some((uid,)) = existing_by_email {
        // Attach the SCIM external_id + refresh the name in place.
        sqlx::query(
            "UPDATE users
             SET scim_external_id = COALESCE($1, scim_external_id),
                 first_name = COALESCE($2, first_name),
                 last_name = COALESCE($3, last_name),
                 display_name = COALESCE($4, display_name),
                 updated_at = NOW()
             WHERE id = $5",
        )
        .bind(new.external_id)
        .bind(new.given_name)
        .bind(new.family_name)
        .bind(new.display_name)
        .bind(uid)
        .execute(db)
        .await?;
        uid
    } else {
        let placeholder_hash = "$argon2id$v=19$m=19456,t=2,p=1$scim-placeholder$scim-placeholder";
        let display = new
            .display_name
            .map(String::from)
            .unwrap_or_else(|| user_name_lower.clone());
        let inserted: (Uuid,) = sqlx::query_as(
            r#"
            INSERT INTO users
                (email, username, password_hash, first_name, last_name, display_name,
                 skill_domain, email_verified, role, scim_external_id)
            VALUES ($1, $2, $3, $4, $5, $6, NULL, TRUE, $7, $8)
            RETURNING id
            "#,
        )
        .bind(&email_lower)
        .bind(&user_name_lower)
        .bind(placeholder_hash)
        .bind(new.given_name.unwrap_or(""))
        .bind(new.family_name.unwrap_or(""))
        .bind(&display)
        .bind(new.default_role)
        .bind(new.external_id)
        .fetch_one(db)
        .await?;
        inserted.0
    };

    // Attach the enterprise membership at the requested activity state.
    let membership_status = if new.active { "active" } else { "revoked" };
    sqlx::query(
        r#"
        INSERT INTO enterprise_members (enterprise_id, user_id, role, status, accepted_at)
        VALUES ($1, $2, $3, $4, CASE WHEN $4 = 'active' THEN NOW() ELSE NULL END)
        ON CONFLICT (enterprise_id, user_id) DO UPDATE SET
            status = EXCLUDED.status,
            accepted_at = COALESCE(enterprise_members.accepted_at, EXCLUDED.accepted_at)
        "#,
    )
    .bind(new.enterprise_id)
    .bind(user_id)
    .bind(new.default_role)
    .bind(membership_status)
    .execute(db)
    .await?;

    if !new.active {
        revoke_all_sessions(db, user_id).await?;
    }

    Ok(user_id)
}

pub async fn get_user(
    db: &PgPool,
    enterprise_id: Uuid,
    user_id: Uuid,
) -> Result<Option<ScimUserView>, AppError> {
    let row: Option<ScimRow307> = sqlx::query_as(
        r#"
        SELECT u.id, u.scim_external_id, u.username, u.email,
               u.first_name, u.last_name, u.display_name,
               em.status,
               u.created_at, u.updated_at
        FROM users u
        JOIN enterprise_members em ON em.user_id = u.id
        WHERE u.id = $1 AND em.enterprise_id = $2
        "#,
    )
    .bind(user_id)
    .bind(enterprise_id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(map_user_view))
}

pub async fn list_users(
    db: &PgPool,
    enterprise_id: Uuid,
    filter_username: Option<&str>,
    start_index: i64,
    count: i64,
) -> Result<(Vec<ScimUserView>, i64), AppError> {
    let rows: Vec<ScimRow343> = sqlx::query_as(
        r#"
        SELECT u.id, u.scim_external_id, u.username, u.email,
               u.first_name, u.last_name, u.display_name,
               em.status,
               u.created_at, u.updated_at
        FROM users u
        JOIN enterprise_members em ON em.user_id = u.id
        WHERE em.enterprise_id = $1
          AND ($2::TEXT IS NULL OR LOWER(u.username) = LOWER($2))
        ORDER BY u.created_at
        OFFSET $3 LIMIT $4
        "#,
    )
    .bind(enterprise_id)
    .bind(filter_username)
    .bind(start_index.saturating_sub(1).max(0))
    .bind(count)
    .fetch_all(db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM users u
        JOIN enterprise_members em ON em.user_id = u.id
        WHERE em.enterprise_id = $1
          AND ($2::TEXT IS NULL OR LOWER(u.username) = LOWER($2))
        "#,
    )
    .bind(enterprise_id)
    .bind(filter_username)
    .fetch_one(db)
    .await?;

    Ok((rows.into_iter().map(map_user_view).collect(), total))
}

fn map_user_view(row: ScimRow307) -> ScimUserView {
    ScimUserView {
        id: row.0,
        external_id: row.1,
        user_name: row.2,
        email: row.3,
        given_name: row.4,
        family_name: row.5,
        display_name: row.6,
        active: row.7 == "active",
        created_at: row.8,
        updated_at: row.9,
    }
}

pub async fn set_user_active(
    db: &PgPool,
    enterprise_id: Uuid,
    user_id: Uuid,
    active: bool,
) -> Result<(), AppError> {
    let new_status = if active { "active" } else { "revoked" };
    let updated = sqlx::query(
        "UPDATE enterprise_members SET status = $1, accepted_at = COALESCE(accepted_at, NOW())
         WHERE enterprise_id = $2 AND user_id = $3",
    )
    .bind(new_status)
    .bind(enterprise_id)
    .bind(user_id)
    .execute(db)
    .await?
    .rows_affected();
    if updated == 0 {
        return Err(AppError::NotFound("member not found".into()));
    }
    if !active {
        revoke_all_sessions(db, user_id).await?;
    }
    Ok(())
}

pub async fn update_user_name(
    db: &PgPool,
    user_id: Uuid,
    given: Option<&str>,
    family: Option<&str>,
    display: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE users SET
            first_name = COALESCE($1, first_name),
            last_name = COALESCE($2, last_name),
            display_name = COALESCE($3, display_name),
            updated_at = NOW()
         WHERE id = $4",
    )
    .bind(given)
    .bind(family)
    .bind(display)
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(())
}

async fn revoke_all_sessions(db: &PgPool, user_id: Uuid) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE user_sessions SET revoked_at = NOW()
         WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(())
}

// ─── SCIM Groups persistence ─────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ScimGroupView {
    pub id: Uuid,
    pub external_id: Option<String>,
    pub display_name: String,
    pub mapped_role: Option<String>,
    pub members: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn create_group(
    db: &PgPool,
    enterprise_id: Uuid,
    external_id: Option<&str>,
    display_name: &str,
) -> Result<Uuid, AppError> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO scim_groups (enterprise_id, external_id, display_name)
         VALUES ($1, $2, $3)
         RETURNING id",
    )
    .bind(enterprise_id)
    .bind(external_id)
    .bind(display_name.trim())
    .fetch_one(db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("23505") => {
            AppError::Validation("Group with this displayName or externalId already exists".into())
        }
        other => AppError::Database(other),
    })?;
    Ok(row.0)
}

pub async fn get_group(
    db: &PgPool,
    enterprise_id: Uuid,
    group_id: Uuid,
) -> Result<Option<ScimGroupView>, AppError> {
    let row: Option<ScimRow523> = sqlx::query_as(
        "SELECT id, external_id, display_name, mapped_role, created_at, updated_at FROM scim_groups
         WHERE id = $1 AND enterprise_id = $2",
    )
    .bind(group_id)
    .bind(enterprise_id)
    .fetch_optional(db)
    .await?;
    let Some((id, external_id, display_name, mapped_role, created_at, updated_at)) = row else {
        return Ok(None);
    };
    let members: Vec<(Uuid,)> =
        sqlx::query_as("SELECT user_id FROM scim_group_members WHERE group_id = $1")
            .bind(id)
            .fetch_all(db)
            .await?;
    Ok(Some(ScimGroupView {
        id,
        external_id,
        display_name,
        mapped_role,
        members: members.into_iter().map(|(u,)| u).collect(),
        created_at,
        updated_at,
    }))
}

pub async fn list_groups(
    db: &PgPool,
    enterprise_id: Uuid,
    filter_display_name: Option<&str>,
    start_index: i64,
    count: i64,
) -> Result<(Vec<ScimGroupView>, i64), AppError> {
    let rows: Vec<ScimRow564> = sqlx::query_as(
        r#"
        SELECT id, external_id, display_name, mapped_role, created_at, updated_at
        FROM scim_groups
        WHERE enterprise_id = $1
          AND ($2::TEXT IS NULL OR LOWER(display_name) = LOWER($2))
        ORDER BY created_at
        OFFSET $3 LIMIT $4
        "#,
    )
    .bind(enterprise_id)
    .bind(filter_display_name)
    .bind(start_index.saturating_sub(1).max(0))
    .bind(count)
    .fetch_all(db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM scim_groups
        WHERE enterprise_id = $1
          AND ($2::TEXT IS NULL OR LOWER(display_name) = LOWER($2))
        "#,
    )
    .bind(enterprise_id)
    .bind(filter_display_name)
    .fetch_one(db)
    .await?;

    let mut views = Vec::with_capacity(rows.len());
    for (id, external_id, display_name, mapped_role, created_at, updated_at) in rows {
        let members: Vec<(Uuid,)> =
            sqlx::query_as("SELECT user_id FROM scim_group_members WHERE group_id = $1")
                .bind(id)
                .fetch_all(db)
                .await?;
        views.push(ScimGroupView {
            id,
            external_id,
            display_name,
            mapped_role,
            members: members.into_iter().map(|(u,)| u).collect(),
            created_at,
            updated_at,
        });
    }
    Ok((views, total))
}

pub async fn add_group_members(
    db: &PgPool,
    group_id: Uuid,
    user_ids: &[Uuid],
) -> Result<(), AppError> {
    for uid in user_ids {
        sqlx::query(
            "INSERT INTO scim_group_members (group_id, user_id) VALUES ($1, $2)
             ON CONFLICT DO NOTHING",
        )
        .bind(group_id)
        .bind(uid)
        .execute(db)
        .await?;
    }
    sqlx::query("UPDATE scim_groups SET updated_at = NOW() WHERE id = $1")
        .bind(group_id)
        .execute(db)
        .await?;
    recompute_roles_for_group(db, group_id, user_ids).await?;
    Ok(())
}

/// Ranks Skilluv roles by privilege — higher wins when a user is in several
/// role-mapping groups. `default_role` is used as the floor when the user is
/// in no role-mapping group.
fn role_rank(role: &str) -> u8 {
    match role {
        "enterprise" => 3,
        "recruiter" => 2,
        _ => 1,
    }
}

/// Recompute `enterprise_members.role` for the given users, based on the
/// intersection of their SCIM group memberships with the enterprise's
/// `mapped_role`-carrying groups. Falls back to `sso_config.default_role`.
///
/// Called after any add / remove / replace group members op, and after PUT of
/// a group's `mapped_role`.
pub async fn recompute_roles_for_group(
    db: &PgPool,
    group_id: Uuid,
    user_ids: &[Uuid],
) -> Result<(), AppError> {
    let enterprise_id: Option<(Uuid,)> =
        sqlx::query_as("SELECT enterprise_id FROM scim_groups WHERE id = $1")
            .bind(group_id)
            .fetch_optional(db)
            .await?;
    let Some((enterprise_id,)) = enterprise_id else {
        return Ok(());
    };
    for uid in user_ids {
        recompute_user_role(db, enterprise_id, *uid).await?;
    }
    Ok(())
}

pub async fn recompute_user_role(
    db: &PgPool,
    enterprise_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    let mapped_roles: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT g.mapped_role FROM scim_group_members gm
        JOIN scim_groups g ON g.id = gm.group_id
        WHERE gm.user_id = $1
          AND g.enterprise_id = $2
          AND g.mapped_role IS NOT NULL
        "#,
    )
    .bind(user_id)
    .bind(enterprise_id)
    .fetch_all(db)
    .await?;

    let default_role: Option<(String,)> =
        sqlx::query_as("SELECT default_role FROM enterprise_sso_configs WHERE enterprise_id = $1")
            .bind(enterprise_id)
            .fetch_optional(db)
            .await?;
    let fallback = default_role
        .map(|(r,)| r)
        .unwrap_or_else(|| "recruiter".to_string());

    let winning_role = mapped_roles
        .into_iter()
        .map(|(r,)| r)
        .max_by_key(|r| role_rank(r))
        .unwrap_or(fallback);

    sqlx::query(
        "UPDATE enterprise_members SET role = $1
         WHERE enterprise_id = $2 AND user_id = $3 AND status = 'active'",
    )
    .bind(&winning_role)
    .bind(enterprise_id)
    .bind(user_id)
    .execute(db)
    .await?;

    Ok(())
}

pub async fn set_group_mapped_role(
    db: &PgPool,
    enterprise_id: Uuid,
    group_id: Uuid,
    mapped_role: Option<&str>,
) -> Result<Vec<Uuid>, AppError> {
    if let Some(r) = mapped_role
        && !matches!(r, "recruiter" | "enterprise")
    {
        return Err(AppError::Validation(
            "mapped_role must be 'recruiter' or 'enterprise'".into(),
        ));
    }
    let affected = sqlx::query(
        "UPDATE scim_groups SET mapped_role = $1, updated_at = NOW()
         WHERE id = $2 AND enterprise_id = $3",
    )
    .bind(mapped_role)
    .bind(group_id)
    .bind(enterprise_id)
    .execute(db)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound("group not found".into()));
    }
    let members: Vec<(Uuid,)> =
        sqlx::query_as("SELECT user_id FROM scim_group_members WHERE group_id = $1")
            .bind(group_id)
            .fetch_all(db)
            .await?;
    let member_ids: Vec<Uuid> = members.into_iter().map(|(u,)| u).collect();
    for uid in &member_ids {
        recompute_user_role(db, enterprise_id, *uid).await?;
    }
    Ok(member_ids)
}

pub async fn remove_group_members(
    db: &PgPool,
    group_id: Uuid,
    user_ids: &[Uuid],
) -> Result<(), AppError> {
    for uid in user_ids {
        sqlx::query("DELETE FROM scim_group_members WHERE group_id = $1 AND user_id = $2")
            .bind(group_id)
            .bind(uid)
            .execute(db)
            .await?;
    }
    sqlx::query("UPDATE scim_groups SET updated_at = NOW() WHERE id = $1")
        .bind(group_id)
        .execute(db)
        .await?;
    recompute_roles_for_group(db, group_id, user_ids).await?;
    Ok(())
}

pub async fn replace_group_members(
    db: &PgPool,
    group_id: Uuid,
    user_ids: &[Uuid],
) -> Result<(), AppError> {
    // Capture existing members before the wipe so we can recompute *their*
    // roles too — otherwise a user removed from the group would keep the
    // previous mapped role forever.
    let existing: Vec<(Uuid,)> =
        sqlx::query_as("SELECT user_id FROM scim_group_members WHERE group_id = $1")
            .bind(group_id)
            .fetch_all(db)
            .await?;

    sqlx::query("DELETE FROM scim_group_members WHERE group_id = $1")
        .bind(group_id)
        .execute(db)
        .await?;
    add_group_members(db, group_id, user_ids).await?;

    // Recompute for anyone who was removed by the replace.
    let removed: Vec<Uuid> = existing
        .into_iter()
        .map(|(u,)| u)
        .filter(|u| !user_ids.contains(u))
        .collect();
    recompute_roles_for_group(db, group_id, &removed).await?;
    Ok(())
}

pub async fn delete_group(
    db: &PgPool,
    enterprise_id: Uuid,
    group_id: Uuid,
) -> Result<bool, AppError> {
    let deleted = sqlx::query("DELETE FROM scim_groups WHERE id = $1 AND enterprise_id = $2")
        .bind(group_id)
        .bind(enterprise_id)
        .execute(db)
        .await?
        .rows_affected();
    Ok(deleted > 0)
}

pub async fn update_group_display_name(
    db: &PgPool,
    enterprise_id: Uuid,
    group_id: Uuid,
    new_name: &str,
) -> Result<bool, AppError> {
    let affected = sqlx::query(
        "UPDATE scim_groups SET display_name = $1, updated_at = NOW()
         WHERE id = $2 AND enterprise_id = $3",
    )
    .bind(new_name)
    .bind(group_id)
    .bind(enterprise_id)
    .execute(db)
    .await?
    .rows_affected();
    Ok(affected > 0)
}
