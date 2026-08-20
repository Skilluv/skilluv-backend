//! Erasing a person without erasing what other people proved.
//!
//! ## The two failures this sits between
//!
//! Delete everything, and a contest where the second place vanished leaves
//! first and third unexplained — and the winner's own attestation cites a
//! ranking that no longer adds up. Somebody else's proof was collateral.
//!
//! Delete nothing, and the request was refused.
//!
//! What has to go is the **personal data**, not every trace that a participant
//! existed. So: a tombstone. The `users` row survives with nothing personal in
//! it, everything pointing at it still points somewhere, and everything it
//! said about the person is gone.
//!
//! ## What is deleted outright
//!
//! Rows that are wholly about one person and that nobody else's record leans
//! on. Notifications nobody else reads, e-mail preferences, tokens to
//! third-party tools, the answers to a wizard, the portfolios they declared.
//!
//! ## What survives, and why
//!
//! Contest entries and their rankings; validated deliverables; attestations.
//! All of them are now attached to a tombstone, which is the honest state: a
//! contest had four entrants and one of them has since left.
//!
//! ## What this does not do
//!
//! It does not reach the object storage. Files uploaded by the person are
//! removed by the storage lifecycle, not synchronously here — a request that
//! could half-fail against a remote service is a request that leaves an
//! account half-erased, and the half that matters is the database.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Tables holding rows that are wholly about one person.
///
/// Listed rather than discovered, so that adding one is a decision somebody
/// takes rather than a cascade nobody read. Each entry is a table and the
/// column naming the person.
const PURELY_PERSONAL: &[(&str, &str)] = &[
    // Preferences and settings.
    ("notification_preferences", "user_id"),
    ("public_feed_preferences", "user_id"),
    ("user_privacy", "user_id"),
    ("user_activity", "user_id"),
    ("notifications", "user_id"),
    ("notification_outbox", "user_id"),
    // What somebody said about themselves. Declarations, never proofs.
    ("user_domain_profiles", "user_id"),
    ("user_languages", "user_id"),
    ("user_skills", "user_id"),
    // Accounts on other services. The tokens above all — leaving one behind
    // would leave Skilluv able to read somebody's Figma after they left.
    ("design_cloud_connections", "user_id"),
    ("user_code_portfolios", "user_id"),
    ("external_signals", "user_id"),
    // Devices and sessions.
    ("user_push_tokens", "user_id"),
    ("push_subscriptions", "user_id"),
    ("webauthn_credentials", "user_id"),
    // Uploads that never became a deliverable.
    ("design_upload_sessions", "user_id"),
];

/// Erase an account, leaving a tombstone.
///
/// Idempotent: erasing an already-erased account changes nothing and reports
/// so. A retried request must not produce a second tombstone or a second
/// audit line.
pub async fn erase(db: &PgPool, user_id: Uuid) -> Result<bool, AppError> {
    let already: Option<Option<chrono::DateTime<chrono::Utc>>> =
        sqlx::query_scalar("SELECT deleted_at FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await?;

    match already {
        None => return Err(AppError::NotFound("no such account".into())),
        Some(Some(_)) => return Ok(false),
        Some(None) => {}
    }

    // Which of them this deployment actually has, asked once and before the
    // transaction opens.
    //
    // The first version tried each DELETE and logged the failures. That does
    // not work: a statement that fails inside a transaction aborts it, and
    // every statement after the first miss was refused — so the tombstone was
    // never written and the caller saw "current transaction is aborted"
    // instead of a clear error. Checking first is the only way to be
    // tolerant of a missing table *and* atomic about the rest.
    let names: Vec<String> = PURELY_PERSONAL
        .iter()
        .map(|(table, _)| (*table).to_string())
        .collect();
    let present: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables
          WHERE table_schema = 'public' AND table_name = ANY($1)",
    )
    .bind(&names)
    .fetch_all(db)
    .await?;

    let mut tx = db.begin().await?;

    for (table, column) in PURELY_PERSONAL {
        if !present.iter().any(|name| name == table) {
            continue;
        }
        // The table list is a constant in this file, not user input. Written
        // out rather than parameterised because an identifier cannot be a
        // bind parameter in any dialect.
        let sql = format!("DELETE FROM {table} WHERE {column} = $1");
        // Fatal from here. Half an erasure is worse than none: the caller is
        // told it worked, and the rows nobody deleted stay.
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
    }

    // The tombstone. Eight hex characters is enough to be unique in practice
    // and short enough to read as what it is.
    let mark = Uuid::new_v4().simple().to_string()[..8].to_string();

    sqlx::query(
        r#"
        UPDATE users
           SET username = 'supprime-' || $2,
               display_name = 'Compte supprimé',
               -- Reserved by RFC 2606, so a stray mailer cannot deliver
               -- anywhere at all.
               email = 'supprime-' || $2 || '@invalid',
               -- Nothing hashes to this. The login path refuses an erased
               -- account before it gets here; this is what makes a path that
               -- forgot to refuse fail anyway.
               password_hash = 'erased',
               -- `title` is left alone: it is the rank — apprenti, artisan,
               -- maitre, legende — not an intitulé somebody typed. It says
               -- nothing about who they were.
               bio = NULL,
               avatar_url = NULL,
               country_iso2 = NULL,
               timezone = NULL,
               discord_user_id = NULL,
               -- Everything else somebody typed about themselves. Listed one
               -- by one rather than swept, so that adding a personal column
               -- to `users` without adding it here is a diff a reviewer can
               -- see.
               -- Blanked rather than nulled: both are NOT NULL, and an
               -- empty string is as absent as the column allows.
               first_name = '',
               last_name = '',
               city = NULL,
               linkedin = NULL,
               twitter = NULL,
               website = NULL,
               profile_readme_sync_url = NULL,
               profile_active = FALSE,
               profile_hidden = TRUE,
               available_for_hire = FALSE,
               totp_enabled = FALSE,
               totp_secret = NULL,
               deleted_at = NOW()
         WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(user_id)
    .bind(&mark)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
}

/// Whether an account has been erased.
///
/// Read by the login path. A tombstone that could still be authenticated
/// against would make the whole thing decorative.
pub async fn is_erased(db: &PgPool, user_id: Uuid) -> Result<bool, AppError> {
    let erased: Option<bool> =
        sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    Ok(erased.unwrap_or(false))
}
