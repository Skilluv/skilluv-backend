//! Which trade a piece of work belongs to, and what that unlocks.

mod common;
use common::TestApp;
use uuid::Uuid;

async fn a_project(app: &TestApp, slug: &str) -> Uuid {
    // Registered once even when two projects are needed: the endpoint refuses
    // a duplicate username, which is correct and not what these tests are
    // about.
    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE username = 'orient_owner'")
            .fetch_optional(&app.db)
            .await
            .unwrap();

    let owner = match existing {
        Some(id) => id,
        None => {
            app.register_user("orient_owner").await;
            sqlx::query_scalar("SELECT id FROM users WHERE username = 'orient_owner'")
                .fetch_one(&app.db)
                .await
                .unwrap()
        }
    };

    sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Projet', 'user', $2) RETURNING id",
    )
    .bind(slug)
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_renamed_trade_still_resolves() {
    let app = TestApp::spawn().await;

    // The partner catalogue was written against the old vocabulary. Rather
    // than rewrite the document, the lineage from migration 0173 is followed.
    let resolved: Option<String> = sqlx::query_scalar(
        "SELECT o.slug FROM orientations o
          WHERE o.id = resolve_orientation('dev-frontend')",
    )
    .fetch_optional(&app.db)
    .await
    .unwrap();

    assert_eq!(resolved.as_deref(), Some("web-frontend-developer"));
}

#[tokio::test]
async fn a_trade_that_never_moved_resolves_to_itself() {
    let app = TestApp::spawn().await;

    let resolved: Option<String> = sqlx::query_scalar(
        "SELECT o.slug FROM orientations o
          WHERE o.id = resolve_orientation('systems-programmer')",
    )
    .fetch_optional(&app.db)
    .await
    .unwrap();

    assert_eq!(resolved.as_deref(), Some("systems-programmer"));
}

#[tokio::test]
async fn a_slug_nobody_knows_resolves_to_nothing() {
    let app = TestApp::spawn().await;

    // Returning NULL rather than a guess: a wrong trade is worse than an
    // untyped slice, because it credits somebody with a speciality they
    // never worked in.
    let resolved: Option<Uuid> = sqlx::query_scalar("SELECT resolve_orientation('metier-invente')")
        .fetch_one(&app.db)
        .await
        .unwrap();

    assert!(resolved.is_none());
}

#[tokio::test]
async fn one_label_maps_to_one_trade_per_project() {
    let app = TestApp::spawn().await;
    let project = a_project(&app, "map-once").await;

    let orientation: Uuid =
        sqlx::query_scalar("SELECT resolve_orientation('web-frontend-developer')")
            .fetch_one(&app.db)
            .await
            .unwrap();

    sqlx::query(
        "INSERT INTO project_label_orientations (project_id, label, orientation_id)
         VALUES ($1, 'good first issue', $2)",
    )
    .bind(project)
    .bind(orientation)
    .execute(&app.db)
    .await
    .unwrap();

    // Two meanings for one label on one project is a contradiction the
    // ingestion would have to resolve by guessing.
    let refused = sqlx::query(
        "INSERT INTO project_label_orientations (project_id, label, orientation_id)
         VALUES ($1, 'good first issue', $2)",
    )
    .bind(project)
    .bind(orientation)
    .execute(&app.db)
    .await;
    assert!(refused.is_err());
}

#[tokio::test]
async fn the_same_label_can_mean_different_trades_on_different_projects() {
    let app = TestApp::spawn().await;
    let front = a_project(&app, "map-front").await;
    let kernel = a_project(&app, "map-kernel").await;

    // "good first issue" means frontend work on Excalidraw and kernel work on
    // a driver repository. That is the reason this is a table.
    for (project, slug) in [
        (front, "web-frontend-developer"),
        (kernel, "kernel-driver-developer"),
    ] {
        let orientation: Uuid = sqlx::query_scalar("SELECT resolve_orientation($1)")
            .bind(slug)
            .fetch_one(&app.db)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO project_label_orientations (project_id, label, orientation_id)
             VALUES ($1, 'good first issue', $2)",
        )
        .bind(project)
        .bind(orientation)
        .execute(&app.db)
        .await
        .expect("same label, different project");
    }

    let mapped: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM project_label_orientations WHERE label = 'good first issue'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(mapped, 2);
}

#[tokio::test]
async fn working_in_three_trades_is_now_counted_rather_than_judged() {
    let app = TestApp::spawn().await;
    let project = a_project(&app, "map-three").await;

    app.register_user("orient_polymath").await;
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'orient_polymath'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    // Migration 0177 had to grant this by hand, because nothing carried a
    // trade. It is a count now.
    for slug in [
        "web-frontend-developer",
        "kernel-driver-developer",
        "gpu-compute-developer",
    ] {
        let orientation: Uuid = sqlx::query_scalar("SELECT resolve_orientation($1)")
            .bind(slug)
            .fetch_one(&app.db)
            .await
            .unwrap();

        let slice: Uuid = sqlx::query_scalar(
            "INSERT INTO project_slices
                (project_id, title, description, primary_domain, slice_type,
                 difficulty, orientation_id)
             VALUES ($1, $2, 'x', 'code', 'github_issue', 3, $3)
             RETURNING id",
        )
        .bind(project)
        .bind(format!("tranche {slug}"))
        .bind(orientation)
        .fetch_one(&app.db)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO deliverables
                (user_id, slice_id, artifact_type, artifact_url, verifiable_by,
                 verification_status, verified_at)
             VALUES ($1, $2, 'pr_merged', 'https://example.test/pr', 'github_webhook',
                     'verified', NOW())",
        )
        .bind(user)
        .bind(slice)
        .execute(&app.db)
        .await
        .unwrap();
    }

    skilluv_backend::services::badge_engine::recompute_badges_for_user(&app.db, user)
        .await
        .expect("recompute");

    let holds: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM user_badges ub
             JOIN badge_rules r ON r.id = ub.rule_id
             WHERE ub.user_id = $1 AND r.slug = 'code-multi-domain'
               AND ub.revoked_at IS NULL)",
    )
    .bind(user)
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert!(holds, "three trades, verified, must be countable");
}

#[tokio::test]
async fn untyped_work_counts_towards_no_trade() {
    let app = TestApp::spawn().await;
    let project = a_project(&app, "map-untyped").await;

    app.register_user("orient_untyped").await;
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'orient_untyped'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    // Ten unlabelled slices are not ten trades, and not one either. An
    // unlabelled issue is honestly untyped.
    for n in 0..10 {
        let slice: Uuid = sqlx::query_scalar(
            "INSERT INTO project_slices
                (project_id, title, description, primary_domain, slice_type, difficulty)
             VALUES ($1, $2, 'x', 'code', 'github_issue', 3)
             RETURNING id",
        )
        .bind(project)
        .bind(format!("sans metier {n}"))
        .fetch_one(&app.db)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO deliverables
                (user_id, slice_id, artifact_type, artifact_url, verifiable_by,
                 verification_status, verified_at)
             VALUES ($1, $2, 'pr_merged', 'https://example.test/pr', 'github_webhook',
                     'verified', NOW())",
        )
        .bind(user)
        .bind(slice)
        .execute(&app.db)
        .await
        .unwrap();
    }

    skilluv_backend::services::badge_engine::recompute_badges_for_user(&app.db, user)
        .await
        .unwrap();

    let holds: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM user_badges ub
             JOIN badge_rules r ON r.id = ub.rule_id
             WHERE ub.user_id = $1 AND r.slug = 'code-multi-domain'
               AND ub.revoked_at IS NULL)",
    )
    .bind(user)
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert!(!holds, "untyped work must not be credited as a speciality");
}
