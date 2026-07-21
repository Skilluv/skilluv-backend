//! Tests d'intégration P14.2 : RLS POC.
//!
//! Contexte : le rôle `skilluv` utilisé par les tests locaux a `rolsuper=true`
//! + `rolbypassrls=true` — donc RLS ne s'applique jamais à lui, y compris avec
//!   FORCE. En prod, le compte applicatif doit être créé sans ces attributs.
//!
//! Ces tests documentent le POC via des queries qui reproduisent la logique
//! de la policy USING sans dépendre de l'enforcement réel :
//!
//! - La policy `tenant_isolation_deliverables` existe.
//! - `set_tenant_context()` fixe le GUC `app.tenant_id`.
//! - L'expression `tenant_id IS NULL OR tenant_id = current_setting(...)`
//!   filtre correctement selon la valeur du GUC.
//!
//! En prod, il suffira de créer le user PG `skilluv_app` avec `NOSUPERUSER
//! NOBYPASSRLS`, d'appeler `set_tenant_context(...)` en début de chaque
//! request, et de `ALTER TABLE ... ENABLE ROW LEVEL SECURITY` sur les tables
//! concernées.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p14_2_test_{}",
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

async fn insert_tenant(db: &PgPool, slug: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO tenants (slug, name, contact_email)
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(slug)
    .bind(format!("Tenant {slug}"))
    .bind(format!("{slug}@example.com"))
    .fetch_one(db)
    .await
    .expect("tenant")
}

// ═══════════════════════════════════════════════════════════════════
// set_tenant_context existe et fixe le GUC dans la session
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn set_tenant_context_sets_the_guc() {
    let (db, name) = setup_test_db().await;
    let tenant = insert_tenant(&db, "corpSet").await;
    let mut conn = db.acquire().await.expect("conn");

    sqlx::query("SELECT set_tenant_context($1)")
        .bind(tenant)
        .execute(&mut *conn)
        .await
        .expect("set");
    let current: String = sqlx::query_scalar("SELECT current_setting('app.tenant_id', true)")
        .fetch_one(&mut *conn)
        .await
        .expect("get");
    assert_eq!(current, tenant.to_string());

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Policies existent + RLS peut être activée sans erreur
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn policies_are_installed_and_rls_can_be_enabled() {
    let (db, name) = setup_test_db().await;

    let policy_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pg_policies
         WHERE tablename IN ('deliverables', 'attestations')
           AND policyname LIKE 'tenant_isolation_%'",
    )
    .fetch_one(&db)
    .await
    .expect("count");
    assert_eq!(
        policy_count, 2,
        "2 policies POC attendues (deliverables + attestations)"
    );

    // ALTER + FORCE ne raise pas — même si RLS ne s'applique pas au superuser,
    // les commandes doivent réussir en préparation du déploiement prod.
    sqlx::query("ALTER TABLE deliverables ENABLE ROW LEVEL SECURITY")
        .execute(&db)
        .await
        .expect("enable");
    sqlx::query("ALTER TABLE deliverables FORCE ROW LEVEL SECURITY")
        .execute(&db)
        .await
        .expect("force");
    let (rls_on, rls_forced): (bool, bool) = sqlx::query_as(
        "SELECT relrowsecurity, relforcerowsecurity FROM pg_class WHERE relname = 'deliverables'",
    )
    .fetch_one(&db)
    .await
    .expect("cls");
    assert!(rls_on && rls_forced);

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// La policy USING isole correctement quand elle est appliquée
// (simulée via SELECT explicite avec la même condition)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn policy_filter_isolates_correctly() {
    let (db, name) = setup_test_db().await;
    let tenant_a = insert_tenant(&db, "corpP1").await;
    let tenant_b = insert_tenant(&db, "corpP2").await;

    let user_a = create_user(&db, Some(tenant_a)).await;
    let user_b = create_user(&db, Some(tenant_b)).await;
    let user_pub = create_user(&db, None).await;

    let ch_a = create_challenge(&db, Some(tenant_a)).await;
    let ch_b = create_challenge(&db, Some(tenant_b)).await;
    let ch_pub = create_challenge(&db, None).await;

    create_deliverable(&db, ch_a, user_a, Some(tenant_a)).await;
    create_deliverable(&db, ch_b, user_b, Some(tenant_b)).await;
    create_deliverable(&db, ch_pub, user_pub, None).await;

    let mut conn = db.acquire().await.expect("conn");

    // Sans set_tenant_context : la policy équivaut à voir uniquement les
    // rows tenant_id IS NULL.
    let public_visible: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM deliverables
        WHERE tenant_id IS NULL
           OR tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid
        "#,
    )
    .fetch_one(&mut *conn)
    .await
    .expect("visible");
    assert_eq!(
        public_visible, 1,
        "sans context, seule la row publique passe"
    );

    // Avec set_tenant_context(tenant_a) : voit tenant_a + public.
    sqlx::query("SELECT set_tenant_context($1)")
        .bind(tenant_a)
        .execute(&mut *conn)
        .await
        .expect("set");
    let both_visible: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM deliverables
        WHERE tenant_id IS NULL
           OR tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid
        "#,
    )
    .fetch_one(&mut *conn)
    .await
    .expect("v2");
    assert_eq!(both_visible, 2, "tenant_a + public");

    // Avec tenant_b : tenant_b + public, pas tenant_a.
    sqlx::query("SELECT set_tenant_context($1)")
        .bind(tenant_b)
        .execute(&mut *conn)
        .await
        .expect("set b");
    let b_visible: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM deliverables
        WHERE tenant_id IS NULL
           OR tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid
        "#,
    )
    .fetch_one(&mut *conn)
    .await
    .expect("v3");
    assert_eq!(b_visible, 2);

    db.close().await;
    cleanup_test_db(&name).await;
}

// ─── Helpers ─────────────────────────────────────────────────────

async fn create_user(db: &PgPool, tenant: Option<Uuid>) -> Uuid {
    let uid = Uuid::new_v4();
    let short = &uid.to_string()[..8];
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments, primary_tenant_id)
         VALUES ($1, $2, $3, 'T', 'U', 'Test', 'dummy', TRUE, 0, $4)",
    )
    .bind(uid)
    .bind(format!("t-{uid}@ex.io"))
    .bind(format!("t{short}"))
    .bind(tenant)
    .execute(db)
    .await
    .expect("u");
    uid
}

async fn create_challenge(db: &PgPool, tenant: Option<Uuid>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty,
             is_training, status, tenant_id)
         VALUES ('Anchor', 'D', 'I', 'code', 2, TRUE, 'published', $1)
         RETURNING id",
    )
    .bind(tenant)
    .fetch_one(db)
    .await
    .expect("ch")
}

async fn create_deliverable(
    db: &PgPool,
    challenge_id: Uuid,
    user_id: Uuid,
    tenant: Option<Uuid>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO deliverables
            (challenge_id, user_id, artifact_type, artifact_url,
             verifiable_by, verification_status, tenant_id)
         VALUES ($1, $2, 'other', $3, 'human_review', 'verified', $4)
         RETURNING id",
    )
    .bind(challenge_id)
    .bind(user_id)
    .bind(format!("skilluv:t:{}", Uuid::new_v4()))
    .bind(tenant)
    .fetch_one(db)
    .await
    .expect("del")
}
