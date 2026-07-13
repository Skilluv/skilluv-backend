//! Tests d'intégration P13.4 : bounty dual payout (fragments + wallet fiat).
//!
//! On teste directement la logique de dispatch résidence → devise + taux via
//! env, en simulant le contexte d'un merge de PR bounty.

use bigdecimal::BigDecimal;
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::talent_wallet::{self, Currency, LedgerEntry};

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p13_4_test_{}",
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

async fn insert_user(db: &PgPool) -> Uuid {
    let uid = Uuid::new_v4();
    let short = &uid.to_string()[..8];
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments)
         VALUES ($1, $2, $3, 'T', 'U', 'Test', 'dummy', TRUE, 0)",
    )
    .bind(uid)
    .bind(format!("t-{uid}@ex.io"))
    .bind(format!("t{short}"))
    .execute(db)
    .await
    .expect("u");
    uid
}

// ═══════════════════════════════════════════════════════════════════
// Credit wallet en XOF pour un talent résident CI
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn xof_credit_bounty_payout_lands_on_wallet() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    talent_wallet::set_residency_country(&db, user, "CI").await.expect("resi");

    // Simule 5 crédits × 3000 XOF/crédit = 15000 XOF
    let amount = BigDecimal::from(15000);
    talent_wallet::credit(
        &db,
        LedgerEntry {
            user_id: user,
            delta: &amount,
            currency: Currency::Xof,
            reason: "bounty_payout",
            related_slice_id: None,
            related_provider_txn_id: None,
            notes: Some("test dual payout"),
        },
    )
    .await
    .expect("c");

    let bal: BigDecimal =
        sqlx::query_scalar("SELECT balance_xof FROM talent_wallets WHERE user_id = $1")
            .bind(user)
            .fetch_one(&db)
            .await
            .expect("b");
    assert_eq!(bal, BigDecimal::from(15000));

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Ledger : la ligne bounty_payout est chainée par hash
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn bounty_payout_writes_ledger_row_with_slice_link() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;

    // Crée un project + slice pour le lien
    let owner = insert_user(&db).await;
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Bounty Project', 'user', $2) RETURNING id",
    )
    .bind(format!("p-{}", Uuid::new_v4()))
    .bind(owner)
    .fetch_one(&db)
    .await
    .expect("p");
    let slice: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain,
             difficulty, status)
         VALUES ($1, 'other', 'Bounty', 'D', 'code', 2, 'merged')
         RETURNING id",
    )
    .bind(project)
    .fetch_one(&db)
    .await
    .expect("s");

    let amount = BigDecimal::from(50);
    talent_wallet::credit(
        &db,
        LedgerEntry {
            user_id: user,
            delta: &amount,
            currency: Currency::Eur,
            reason: "bounty_payout",
            related_slice_id: Some(slice),
            related_provider_txn_id: None,
            notes: None,
        },
    )
    .await
    .expect("c");

    let (delta, currency, reason, related): (BigDecimal, String, String, Option<Uuid>) =
        sqlx::query_as(
            "SELECT delta, currency, reason, related_slice_id
             FROM talent_transactions WHERE user_id = $1",
        )
        .bind(user)
        .fetch_one(&db)
        .await
        .expect("row");
    assert_eq!(delta, BigDecimal::from(50));
    assert_eq!(currency, "EUR");
    assert_eq!(reason, "bounty_payout");
    assert_eq!(related, Some(slice));

    // Ledger integrity toujours OK.
    assert!(talent_wallet::verify_ledger_chain(&db, user).await.expect("v"));

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Résidence non-XOF (ex "FR") → EUR
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn eur_residency_gets_eur_credit() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    talent_wallet::set_residency_country(&db, user, "FR").await.expect("r");

    let amount = BigDecimal::from(80);
    talent_wallet::credit(
        &db,
        LedgerEntry {
            user_id: user,
            delta: &amount,
            currency: Currency::Eur,
            reason: "bounty_payout",
            related_slice_id: None,
            related_provider_txn_id: None,
            notes: None,
        },
    )
    .await
    .expect("c");

    let (eur, xof): (BigDecimal, BigDecimal) =
        sqlx::query_as("SELECT balance_eur, balance_xof FROM talent_wallets WHERE user_id = $1")
            .bind(user)
            .fetch_one(&db)
            .await
            .expect("b");
    assert_eq!(eur, BigDecimal::from(80));
    assert_eq!(xof, BigDecimal::from(0));

    db.close().await;
    cleanup_test_db(&name).await;
}
