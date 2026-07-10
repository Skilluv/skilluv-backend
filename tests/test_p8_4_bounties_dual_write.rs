//! Tests d'intégration P8.4 : dual-write oss_bounties + project_slices.
//!
//! Le service SQL est testé directement (l'endpoint HTTP demande le stack auth
//! complet). On simule la logique de création : INSERT bounty + match project
//! par github_repo_owner/name + INSERT slice + UPDATE bounty.slice_id.

use bigdecimal::BigDecimal;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::str::FromStr;
use uuid::Uuid;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p8_4_test_{}",
        Uuid::new_v4().to_string().replace('-', "")
    );
    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await
        .expect("admin");
    sqlx::query(&format!("CREATE DATABASE \"{db_name}\""))
        .execute(&admin_pool)
        .await
        .expect("create");
    admin_pool.close().await;

    let db_url = format!("postgres://skilluv:skilluv_secret@localhost:5433/{db_name}");
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("connect");
    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("migrations");
    (db, db_name)
}

async fn cleanup_test_db(db_name: &str) {
    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await
        .expect("admin");
    let _ = sqlx::query(&format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{db_name}'"
    ))
    .execute(&admin_pool)
    .await;
    let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS \"{db_name}\""))
        .execute(&admin_pool)
        .await;
    admin_pool.close().await;
}

async fn insert_test_user(db: &PgPool, user_id: Uuid) {
    let short = &user_id.to_string()[..8];
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments)
         VALUES ($1, $2, $3, $4, $5, $6, $7, TRUE, 0)",
    )
    .bind(user_id)
    .bind(format!("test-{user_id}@example.com"))
    .bind(format!("t{short}"))
    .bind("T")
    .bind("U")
    .bind("Test User")
    .bind("dummy")
    .execute(db)
    .await
    .expect("insert user");
}

async fn insert_test_enterprise(db: &PgPool, owner: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO enterprises (company_name, slug, company_size, owner_id)
         VALUES ('Test Enterprise', $1, '1-10', $2)
         RETURNING id",
    )
    .bind(format!("test-ent-{}", Uuid::new_v4()))
    .bind(owner)
    .fetch_one(db)
    .await
    .expect("insert enterprise")
}

async fn insert_project_with_repo(
    db: &PgPool,
    owner: Uuid,
    repo_owner: &str,
    repo_name: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO projects
            (slug, name, owner_type, owner_id, github_repo_owner, github_repo_name)
         VALUES ($1, 'Test Project', 'user', $2, $3, $4)
         RETURNING id",
    )
    .bind(format!("proj-{}", Uuid::new_v4()))
    .bind(owner)
    .bind(repo_owner)
    .bind(repo_name)
    .fetch_one(db)
    .await
    .expect("insert project")
}

/// Simule la logique de dual-write de create_bounty (extraite du service pour
/// pouvoir la tester sans le stack HTTP + auth complet).
async fn simulate_dual_write_bounty(
    db: &PgPool,
    enterprise_id: Uuid,
    posted_by: Uuid,
    repo_owner: &str,
    repo_name: &str,
    issue_number: i32,
    title: &str,
    reward: BigDecimal,
) -> (Uuid, Option<Uuid>) {
    let mut tx = db.begin().await.expect("begin tx");

    let bounty_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO oss_bounties
            (enterprise_id, posted_by_user_id, repo_owner, repo_name, issue_number, issue_url,
             title, description, reward_credits, fragments_bonus, required_skills, difficulty, tags)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 100, ARRAY[]::text[], 3, ARRAY[]::text[])
        RETURNING id
        "#,
    )
    .bind(enterprise_id)
    .bind(posted_by)
    .bind(repo_owner)
    .bind(repo_name)
    .bind(issue_number)
    .bind(format!("http://gh/{repo_owner}/{repo_name}/issues/{issue_number}"))
    .bind(title)
    .bind("Test description")
    .bind(&reward)
    .fetch_one(&mut *tx)
    .await
    .expect("insert bounty");

    let matched_project_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM projects
         WHERE github_repo_owner = $1 AND github_repo_name = $2
         LIMIT 1",
    )
    .bind(repo_owner)
    .bind(repo_name)
    .fetch_optional(&mut *tx)
    .await
    .expect("match project");

    let slice_id = if let Some(project_id) = matched_project_id {
        let slice_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO project_slices
                (project_id, slice_type, external_ref,
                 title, description,
                 primary_domain, difficulty, fragments_reward, credits_reward,
                 status, created_by_user_id, ingested_from)
            VALUES ($1, 'github_issue', $2,
                    $3, $4,
                    'code', 3, 100, $5,
                    'open', $6, 'legacy_bounty')
            RETURNING id
            "#,
        )
        .bind(project_id)
        .bind(issue_number.to_string())
        .bind(title)
        .bind("Test description")
        .bind(&reward)
        .bind(posted_by)
        .fetch_one(&mut *tx)
        .await
        .expect("insert slice");

        sqlx::query("UPDATE oss_bounties SET slice_id = $1 WHERE id = $2")
            .bind(slice_id)
            .bind(bounty_id)
            .execute(&mut *tx)
            .await
            .expect("link");

        Some(slice_id)
    } else {
        None
    };

    tx.commit().await.expect("commit");
    (bounty_id, slice_id)
}

// ═══════════════════════════════════════════════════════════════════
// Cas 1 : project matche → dual-write réussi
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn bounty_with_matching_project_creates_linked_slice() {
    let (db, db_name) = setup_test_db().await;
    let owner = Uuid::new_v4();
    insert_test_user(&db, owner).await;
    let enterprise_id = insert_test_enterprise(&db, owner).await;
    let project_id = insert_project_with_repo(&db, owner, "acme", "widgets").await;

    let (bounty_id, slice_id) = simulate_dual_write_bounty(
        &db,
        enterprise_id,
        owner,
        "acme",
        "widgets",
        42,
        "Fix bug",
        BigDecimal::from_str("50.00").unwrap(),
    )
    .await;

    assert!(slice_id.is_some(), "matching project → slice created");
    let slice_id = slice_id.unwrap();

    // Vérifier link bidirectionnel
    let linked: Uuid = sqlx::query_scalar(
        "SELECT slice_id FROM oss_bounties WHERE id = $1",
    )
    .bind(bounty_id)
    .fetch_one(&db)
    .await
    .expect("fetch");
    assert_eq!(linked, slice_id);

    // Slice pointe vers le bon project
    let slice_project: Uuid = sqlx::query_scalar(
        "SELECT project_id FROM project_slices WHERE id = $1",
    )
    .bind(slice_id)
    .fetch_one(&db)
    .await
    .expect("fetch");
    assert_eq!(slice_project, project_id);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Cas 2 : pas de project → bounty seule, pas de slice
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn bounty_without_matching_project_is_orphan() {
    let (db, db_name) = setup_test_db().await;
    let owner = Uuid::new_v4();
    insert_test_user(&db, owner).await;
    let enterprise_id = insert_test_enterprise(&db, owner).await;
    // Pas de project créé — le repo unknown/unknown ne matche rien

    let (bounty_id, slice_id) = simulate_dual_write_bounty(
        &db,
        enterprise_id,
        owner,
        "unknown-owner",
        "unknown-repo",
        7,
        "Orphan",
        BigDecimal::from_str("10.00").unwrap(),
    )
    .await;

    assert!(slice_id.is_none(), "no project → no slice");

    let linked: Option<Uuid> = sqlx::query_scalar(
        "SELECT slice_id FROM oss_bounties WHERE id = $1",
    )
    .bind(bounty_id)
    .fetch_one(&db)
    .await
    .expect("fetch");
    assert!(linked.is_none());

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Cas 3 : slice héritée d'une bounty a bien les métadonnées attendues
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn dual_write_slice_has_expected_metadata() {
    let (db, db_name) = setup_test_db().await;
    let owner = Uuid::new_v4();
    insert_test_user(&db, owner).await;
    let enterprise_id = insert_test_enterprise(&db, owner).await;
    let _project_id = insert_project_with_repo(&db, owner, "orgz", "repoz").await;

    let (_bounty_id, slice_id) = simulate_dual_write_bounty(
        &db,
        enterprise_id,
        owner,
        "orgz",
        "repoz",
        99,
        "Titled slice",
        BigDecimal::from_str("42.00").unwrap(),
    )
    .await;

    let slice_id = slice_id.unwrap();

    let (
        slice_type,
        external_ref,
        title,
        primary_domain,
        difficulty,
        credits_reward,
        status,
        ingested_from,
    ): (String, Option<String>, String, String, i16, BigDecimal, String, String) =
        sqlx::query_as(
            "SELECT slice_type, external_ref, title, primary_domain, difficulty,
                    credits_reward, status, ingested_from
             FROM project_slices WHERE id = $1",
        )
        .bind(slice_id)
        .fetch_one(&db)
        .await
        .expect("fetch");

    assert_eq!(slice_type, "github_issue");
    assert_eq!(external_ref, Some("99".to_string()));
    assert_eq!(title, "Titled slice");
    assert_eq!(primary_domain, "code");
    assert_eq!(difficulty, 3);
    assert_eq!(credits_reward, BigDecimal::from_str("42.00").unwrap());
    assert_eq!(status, "open");
    assert_eq!(ingested_from, "legacy_bounty");

    db.close().await;
    cleanup_test_db(&db_name).await;
}
