//! SKI-289 — revoking a guild invitation.
//!
//! Two things the front end needs and did not have:
//!   * a route under the guild (`DELETE /guilds/{id}/invitations/{inv}`),
//!     which also refuses an invitation belonging to another guild;
//!   * idempotence, so a double-click or a retried request is not surfaced
//!     to the user as an error.
//!
//! Authorization is checked before invitation state, so a non-officer
//! cannot tell an already-revoked invitation from a pending one.

mod common;
use common::TestApp;
use uuid::Uuid;

async fn make_user(app: &TestApp, username: &str) -> Uuid {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "INSERT INTO users (username, email, password_hash, display_name, first_name, last_name)
         VALUES ('{username}', '{username}@test.dev', 'x', '{username}', 'F', 'L')
         RETURNING id"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn make_guild(app: &TestApp, slug: &str, tag: &str, founder: Uuid) -> Uuid {
    let guild: Uuid = sqlx::query_scalar(
        "INSERT INTO guilds (slug, tag, name, founder_id) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(slug)
    .bind(tag)
    .bind(format!("Guild {slug}"))
    .bind(founder)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query("INSERT INTO guild_members (guild_id, user_id, role) VALUES ($1, $2, 'founder')")
        .bind(guild)
        .bind(founder)
        .execute(&app.db)
        .await
        .unwrap();

    guild
}

async fn make_invitation(app: &TestApp, guild: Uuid, inviter: Uuid, invited: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO guild_invitations (guild_id, inviter_id, invited_user_id, expires_at)
         VALUES ($1, $2, $3, NOW() + INTERVAL '7 days')
         RETURNING id",
    )
    .bind(guild)
    .bind(inviter)
    .bind(invited)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn revoked_at(app: &TestApp, invitation: Uuid) -> Option<chrono::DateTime<chrono::Utc>> {
    sqlx::query_scalar("SELECT revoked_at FROM guild_invitations WHERE id = $1")
        .bind(invitation)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

/// Registers `username`, logs them in, and makes them the founder of a
/// fresh guild. Returns `(guild_id, user_id)`.
async fn logged_in_founder(app: &TestApp, username: &str, slug: &str, tag: &str) -> (Uuid, Uuid) {
    app.register_user(username).await;
    app.login(username).await;
    let uid: Uuid = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT id FROM users WHERE username = '{username}'"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap();
    let guild = make_guild(app, slug, tag, uid).await;
    (guild, uid)
}

#[tokio::test]
async fn officer_revokes_an_invitation_under_its_guild() {
    let app = TestApp::spawn().await;
    let (guild, founder) = logged_in_founder(&app, "guild_boss", "revoke-ok", "RVOK").await;
    let target = make_user(&app, "invitee_ok").await;
    let invitation = make_invitation(&app, guild, founder, target).await;

    let resp = app
        .delete(&format!("/api/guilds/{guild}/invitations/{invitation}"))
        .await;
    assert_eq!(resp.status().as_u16(), 200);
    assert!(revoked_at(&app, invitation).await.is_some());
}

#[tokio::test]
async fn revoking_twice_succeeds_and_keeps_the_first_timestamp() {
    let app = TestApp::spawn().await;
    let (guild, founder) = logged_in_founder(&app, "guild_boss2", "revoke-idem", "RIDE").await;
    let target = make_user(&app, "invitee_idem").await;
    let invitation = make_invitation(&app, guild, founder, target).await;

    let first = app
        .delete(&format!("/api/guilds/{guild}/invitations/{invitation}"))
        .await;
    assert_eq!(first.status().as_u16(), 200);
    let stamp = revoked_at(&app, invitation).await.expect("revoked");

    let second = app
        .delete(&format!("/api/guilds/{guild}/invitations/{invitation}"))
        .await;
    assert_eq!(
        second.status().as_u16(),
        200,
        "a repeated revoke is the caller's intent already satisfied, not an error"
    );
    assert_eq!(
        revoked_at(&app, invitation).await,
        Some(stamp),
        "the second call must not rewrite when the revocation happened"
    );
}

#[tokio::test]
async fn an_invitation_from_another_guild_is_not_found() {
    let app = TestApp::spawn().await;
    let (guild, _founder) = logged_in_founder(&app, "guild_boss3", "revoke-mine", "RMIN").await;

    let stranger = make_user(&app, "other_founder").await;
    let other_guild = make_guild(&app, "revoke-theirs", "RTHE", stranger).await;
    let target = make_user(&app, "invitee_other").await;
    let invitation = make_invitation(&app, other_guild, stranger, target).await;

    let resp = app
        .delete(&format!("/api/guilds/{guild}/invitations/{invitation}"))
        .await;
    assert_eq!(resp.status().as_u16(), 404);
    assert!(
        revoked_at(&app, invitation).await.is_none(),
        "the other guild's invitation must be untouched"
    );
}

#[tokio::test]
async fn an_accepted_invitation_cannot_be_revoked() {
    let app = TestApp::spawn().await;
    let (guild, founder) = logged_in_founder(&app, "guild_boss4", "revoke-acc", "RACC").await;
    let target = make_user(&app, "invitee_accepted").await;
    let invitation = make_invitation(&app, guild, founder, target).await;

    sqlx::query("UPDATE guild_invitations SET accepted_at = NOW() WHERE id = $1")
        .bind(invitation)
        .execute(&app.db)
        .await
        .unwrap();

    let resp = app
        .delete(&format!("/api/guilds/{guild}/invitations/{invitation}"))
        .await;
    assert_eq!(resp.status().as_u16(), 400);
    assert!(revoked_at(&app, invitation).await.is_none());
}

#[tokio::test]
async fn a_non_member_cannot_revoke() {
    let app = TestApp::spawn().await;
    let founder = make_user(&app, "guild_boss5").await;
    let guild = make_guild(&app, "revoke-403", "R403", founder).await;
    let target = make_user(&app, "invitee_403").await;
    let invitation = make_invitation(&app, guild, founder, target).await;

    app.register_user("outsider").await;
    app.login("outsider").await;

    let resp = app
        .delete(&format!("/api/guilds/{guild}/invitations/{invitation}"))
        .await;
    assert_eq!(resp.status().as_u16(), 403);
    assert!(revoked_at(&app, invitation).await.is_none());
}

#[tokio::test]
async fn a_non_member_gets_403_on_an_already_revoked_invitation() {
    let app = TestApp::spawn().await;
    let founder = make_user(&app, "guild_boss6").await;
    let guild = make_guild(&app, "revoke-probe", "RPRB", founder).await;
    let target = make_user(&app, "invitee_probe").await;
    let invitation = make_invitation(&app, guild, founder, target).await;

    sqlx::query("UPDATE guild_invitations SET revoked_at = NOW() WHERE id = $1")
        .bind(invitation)
        .execute(&app.db)
        .await
        .unwrap();

    app.register_user("prober").await;
    app.login("prober").await;

    let resp = app
        .delete(&format!("/api/guilds/{guild}/invitations/{invitation}"))
        .await;
    assert_eq!(
        resp.status().as_u16(),
        403,
        "state must not leak to callers who have no business reading it"
    );
}

#[tokio::test]
async fn the_flat_route_still_works() {
    let app = TestApp::spawn().await;
    let (guild, founder) = logged_in_founder(&app, "guild_boss7", "revoke-flat", "RFLT").await;
    let target = make_user(&app, "invitee_flat").await;
    let invitation = make_invitation(&app, guild, founder, target).await;

    let resp = app
        .delete(&format!("/api/guild-invitations/{invitation}"))
        .await;
    assert_eq!(resp.status().as_u16(), 200);
    assert!(revoked_at(&app, invitation).await.is_some());
}
