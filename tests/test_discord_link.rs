//! Linking a Discord account, and what it sets in motion.
//!
//! The claim this suite holds is the one the whole feature rests on:
//! **`users.discord_user_id` gets written**. It has existed since migration
//! 0138 with a unique index, two of the bot's commands read it, and until this
//! work nothing had ever put a value in it. So `/skilluv me` answered "your
//! Discord account is not linked yet" to everybody, permanently, and every
//! role `discord-setup.py` creates stayed empty because the platform could not
//! tell which Discord member was which account.

mod common;
use common::TestApp;
use uuid::Uuid;

async fn user_id(app: &TestApp, username: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

/// Stand in for the OAuth callback: the profile Discord returns, stored the
/// way `services::oauth` stores it. The exchange itself is Discord's and is
/// not what can break here.
async fn link_discord(app: &TestApp, user: Uuid, snowflake: &str) -> Result<(), String> {
    let profile = skilluv_backend::services::oauth::OAuthProfile {
        provider: "discord",
        provider_user_id: snowflake.to_string(),
        email: None,
        email_verified: false,
        display_name: Some("Kofi".into()),
        avatar_url: None,
        username: Some("kofi".into()),
    };
    skilluv_backend::services::oauth::upsert_link(&app.db, user, &profile)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The two statements of one list, held together.
///
/// `VALID_PROVIDERS` in `services::oauth` and the CHECK of migration 0603 say
/// the same thing in two languages. Migration 0603 argues why that duplication
/// is right here — a provider is a Rust module, not a row — and this is the
/// other half of that argument: the duplication is only acceptable while
/// something fails when the two drift.
///
/// Which is exactly how this feature broke first. The adapter, the routes and
/// the column write all existed; every one of them ended at a CHECK that had
/// never heard of Discord.
#[tokio::test]
async fn the_provider_list_and_the_database_agree() {
    let app = TestApp::spawn().await;

    let def: String = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint
          WHERE conname = 'user_oauth_providers_provider_check'",
    )
    .fetch_one(&app.db)
    .await
    .expect("the constraint exists");

    for provider in skilluv_backend::services::oauth::VALID_PROVIDERS {
        assert!(
            def.contains(provider),
            "{provider} is in VALID_PROVIDERS but the database refuses it: {def}"
        );
    }
}

#[tokio::test]
async fn linking_writes_the_column_the_bot_reads() {
    let app = TestApp::spawn().await;
    app.register_user("disco_one").await;
    let uid = user_id(&app, "disco_one").await;

    let before: Option<String> =
        sqlx::query_scalar("SELECT discord_user_id FROM users WHERE id = $1")
            .bind(uid)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(before.is_none(), "nothing should have written it yet");

    link_discord(&app, uid, "1234567890123456789")
        .await
        .unwrap();

    let after: Option<String> =
        sqlx::query_scalar("SELECT discord_user_id FROM users WHERE id = $1")
            .bind(uid)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        after.as_deref(),
        Some("1234567890123456789"),
        "the bot matches interactions on this column"
    );
}

/// One Discord account belongs to at most one Skilluv account.
///
/// Migration 0138's unique index says so, and it is the right rule: roles are
/// derived from proof, so letting two accounts share a Discord identity would
/// let somebody wear a rank they did not earn. What is under test is that the
/// refusal is a message somebody can act on rather than a constraint violation
/// surfacing as a 500.
#[tokio::test]
async fn one_discord_account_cannot_dress_two_profiles() {
    let app = TestApp::spawn().await;
    app.register_user("disco_a").await;
    app.register_user("disco_b").await;
    let a = user_id(&app, "disco_a").await;
    let b = user_id(&app, "disco_b").await;

    link_discord(&app, a, "999888777666555444").await.unwrap();
    let refused = link_discord(&app, b, "999888777666555444").await;

    let err = refused.expect_err("the second link must be refused");
    assert!(
        err.to_lowercase().contains("already linked"),
        "the refusal has to say what to do about it: {err}"
    );

    // And the first link is untouched — a failed claim must not steal it.
    let still: Option<String> =
        sqlx::query_scalar("SELECT discord_user_id FROM users WHERE id = $1")
            .bind(a)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(still.as_deref(), Some("999888777666555444"));
}

#[tokio::test]
async fn linking_asks_the_bot_to_dress_them() {
    let app = TestApp::spawn().await;
    app.register_user("disco_queue").await;
    let uid = user_id(&app, "disco_queue").await;

    link_discord(&app, uid, "111222333444555666").await.unwrap();

    let (reason, pending): (String, bool) = sqlx::query_as(
        "SELECT reason, applied_at IS NULL FROM discord_role_sync_queue WHERE user_id = $1",
    )
    .bind(uid)
    .fetch_one(&app.db)
    .await
    .expect("a sync should have been queued");
    assert_eq!(reason, "linked");
    assert!(pending);
}

/// One pending row per person, whatever happens upstream.
///
/// A single validated deliverable can move a rank, grant a capability and fire
/// three hooks. Without the partial unique index of migration 0602 the bot
/// would compute the same answer four times and issue the same Discord writes
/// four times, against an API that rate-limits per guild.
#[tokio::test]
async fn repeated_requests_collapse_into_one_pending_row() {
    let app = TestApp::spawn().await;
    app.register_user("disco_collapse").await;
    let uid = user_id(&app, "disco_collapse").await;

    for reason in ["linked", "rank_changed", "capabilities_changed", "sweep"] {
        skilluv_backend::services::discord_roles::request_sync(&app.db, uid, reason)
            .await
            .unwrap();
    }

    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM discord_role_sync_queue WHERE user_id = $1 AND applied_at IS NULL",
    )
    .bind(uid)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(rows, 1, "four requests, one row of work");

    // The last reason wins, because it is the most recent thing that happened.
    let reason: String = sqlx::query_scalar(
        "SELECT reason FROM discord_role_sync_queue WHERE user_id = $1 AND applied_at IS NULL",
    )
    .bind(uid)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(reason, "sweep");
}

/// Unlinking has to strip, not freeze.
///
/// Somebody who unlinks while keeping `@Doyen` is an authority the platform no
/// longer backs, pointing at a person no longer connected to any account. The
/// order matters: the column is cleared *before* the sync is queued, so the
/// worker computes an empty desired set and takes everything back.
#[tokio::test]
async fn unlinking_clears_the_column_and_asks_for_the_roles_back() {
    let app = TestApp::spawn().await;
    app.register_user("disco_leaver").await;
    let uid = user_id(&app, "disco_leaver").await;

    link_discord(&app, uid, "777666555444333222").await.unwrap();
    // Pretend the bot did its pass, so the next queued row is unambiguous.
    sqlx::query("UPDATE discord_role_sync_queue SET applied_at = NOW() WHERE user_id = $1")
        .bind(uid)
        .execute(&app.db)
        .await
        .unwrap();

    skilluv_backend::services::oauth::unlink(&app.db, uid, "discord")
        .await
        .unwrap();

    let cleared: Option<String> =
        sqlx::query_scalar("SELECT discord_user_id FROM users WHERE id = $1")
            .bind(uid)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(cleared.is_none(), "the snowflake has to go");

    let reason: String = sqlx::query_scalar(
        "SELECT reason FROM discord_role_sync_queue WHERE user_id = $1 AND applied_at IS NULL",
    )
    .bind(uid)
    .fetch_one(&app.db)
    .await
    .expect("unlinking must queue the strip");
    assert_eq!(reason, "unlinked");
}

/// The standing the roles are computed from, read out of the database rather
/// than constructed in a test helper — this is the half `desired()` cannot
/// check on its own.
#[tokio::test]
async fn standing_is_empty_for_an_unlinked_account_and_read_for_a_linked_one() {
    let app = TestApp::spawn().await;
    app.register_user("disco_standing").await;
    let uid = user_id(&app, "disco_standing").await;

    let before = skilluv_backend::services::discord_roles::standing(&app.db, uid)
        .await
        .unwrap();
    assert!(!before.linked);
    assert!(before.domains.is_empty() && before.capabilities.is_empty());

    link_discord(&app, uid, "555444333222111000").await.unwrap();
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, 'mentor', 'test') ON CONFLICT DO NOTHING",
    )
    .bind(uid)
    .execute(&app.db)
    .await
    .unwrap();

    let after = skilluv_backend::services::discord_roles::standing(&app.db, uid)
        .await
        .unwrap();
    assert!(after.linked);
    assert!(
        after.capabilities.iter().any(|c| c == "mentor"),
        "{:?}",
        after.capabilities
    );

    // And that standing earns the role the declaration promises.
    let rules = skilluv_backend::services::discord_roles::rules().unwrap();
    let desired = skilluv_backend::services::discord_roles::desired(&rules, &after);
    assert!(
        desired.iter().any(|r| r == "Mentor"),
        "a mentor should wear @Mentor: {desired:?}"
    );
}
