//! Tests d'intégration P13.1 : talent_wallets + talent_transactions ledger.
//!
//! Vérifie :
//! - Init idempotent du wallet.
//! - Credit atomique + balance update.
//! - Debit refuse si insufficient.
//! - Chaîne de hash cohérente (verify_ledger_chain OK avant altération, KO après).
//! - Set residency_country.

use bigdecimal::BigDecimal;
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::talent_wallet::{
    self, Currency, LedgerEntry,
};

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p13_1_test_{}",
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
    let user_id = Uuid::new_v4();
    let short = &user_id.to_string()[..8];
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments)
         VALUES ($1, $2, $3, 'T', 'U', 'Test', 'dummy', TRUE, 0)",
    )
    .bind(user_id)
    .bind(format!("test-{user_id}@example.com"))
    .bind(format!("t{short}"))
    .execute(db)
    .await
    .expect("insert user");
    user_id
}

// ═══════════════════════════════════════════════════════════════════
// Init idempotent
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn wallet_init_is_idempotent() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;

    let a = talent_wallet::get_or_init_wallet(&db, user).await.expect("a");
    let b = talent_wallet::get_or_init_wallet(&db, user).await.expect("b");
    assert_eq!(a.user_id, b.user_id);
    assert_eq!(a.balance_eur, BigDecimal::from(0));
    assert_eq!(a.balance_xof, BigDecimal::from(0));
    assert_eq!(a.stripe_kyc_status, "not_started");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM talent_wallets WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db)
        .await
        .expect("c");
    assert_eq!(count, 1);

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Credit augmente la balance et écrit une tx
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn credit_increases_balance_and_logs_transaction() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;

    let amount = BigDecimal::from(50);
    let entry = LedgerEntry {
        user_id: user,
        delta: &amount,
        currency: Currency::Eur,
        reason: "bounty_payout",
        related_slice_id: None,
        related_provider_txn_id: None,
        notes: Some("test credit"),
    };
    let txn = talent_wallet::credit(&db, entry).await.expect("credit");
    assert_eq!(txn.delta, BigDecimal::from(50));
    assert_eq!(txn.currency, "EUR");
    assert_eq!(txn.reason, "bounty_payout");

    let w = talent_wallet::get_or_init_wallet(&db, user).await.expect("w");
    assert_eq!(w.balance_eur, BigDecimal::from(50));
    assert_eq!(w.balance_xof, BigDecimal::from(0));

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Debit refuse si insufficient
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn debit_refuses_if_insufficient_balance() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;

    // Init wallet à 0 puis débit 10 → refuse
    talent_wallet::get_or_init_wallet(&db, user).await.expect("init");
    let ten = BigDecimal::from(10);
    let entry = LedgerEntry {
        user_id: user,
        delta: &ten,
        currency: Currency::Eur,
        reason: "withdraw_stripe",
        related_slice_id: None,
        related_provider_txn_id: None,
        notes: None,
    };
    let res = talent_wallet::debit(&db, entry).await;
    assert!(res.is_err(), "débit doit être refusé sur balance 0");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Debit décremente la balance après credit
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn debit_decreases_balance_after_credit() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;

    let credit_amount = BigDecimal::from(100);
    talent_wallet::credit(
        &db,
        LedgerEntry {
            user_id: user,
            delta: &credit_amount,
            currency: Currency::Xof,
            reason: "bounty_payout",
            related_slice_id: None,
            related_provider_txn_id: None,
            notes: None,
        },
    )
    .await
    .expect("credit");

    let debit_amount = BigDecimal::from(30);
    talent_wallet::debit(
        &db,
        LedgerEntry {
            user_id: user,
            delta: &debit_amount,
            currency: Currency::Xof,
            reason: "withdraw_momo",
            related_slice_id: None,
            related_provider_txn_id: None,
            notes: None,
        },
    )
    .await
    .expect("debit");

    let w = talent_wallet::get_or_init_wallet(&db, user).await.expect("w");
    assert_eq!(w.balance_xof, BigDecimal::from(70));

    // 2 transactions dans le ledger, la 2e a un signe négatif
    let txs = talent_wallet::list_transactions(&db, user, 10).await.expect("l");
    assert_eq!(txs.len(), 2);
    let debit = txs.iter().find(|t| t.reason == "withdraw_momo").unwrap();
    assert!(debit.delta < BigDecimal::from(0), "debit stocke un delta négatif");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Ledger chain integrity
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn ledger_chain_verifies_before_and_after_tamper() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;

    for i in 1..=4 {
        let amt = BigDecimal::from(i * 10);
        talent_wallet::credit(
            &db,
            LedgerEntry {
                user_id: user,
                delta: &amt,
                currency: Currency::Eur,
                reason: "test_credit",
                related_slice_id: None,
                related_provider_txn_id: None,
                notes: None,
            },
        )
        .await
        .expect("c");
    }

    assert!(
        talent_wallet::verify_ledger_chain(&db, user).await.expect("v"),
        "ledger doit être cohérent apres 4 credits"
    );

    // Tamper : modifie le delta de la 2e transaction. La chaîne doit casser.
    sqlx::query(
        "UPDATE talent_transactions
         SET delta = delta + 1
         WHERE id = (
             SELECT id FROM talent_transactions
             WHERE user_id = $1 ORDER BY created_at ASC OFFSET 1 LIMIT 1
         )",
    )
    .bind(user)
    .execute(&db)
    .await
    .expect("tamper");

    assert!(
        !talent_wallet::verify_ledger_chain(&db, user).await.expect("v"),
        "ledger doit être invalide apres tamper"
    );

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// set_residency_country upsert + normalisation
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn set_residency_normalizes_to_upper() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;

    let w = talent_wallet::set_residency_country(&db, user, "ci").await.expect("s");
    assert_eq!(w.residency_country.as_deref(), Some("CI"));

    // Invalid → error
    let bad = talent_wallet::set_residency_country(&db, user, "XYZ").await;
    assert!(bad.is_err());

    db.close().await;
    cleanup_test_db(&name).await;
}
