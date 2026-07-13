//! P13.1 — Talent wallet + transactions ledger.
//!
//! Modélise le portefeuille réel d'un talent (fiat, pas fragments) avec :
//! - Crédit atomique via SQL update guardé par balance CHECK.
//! - Débit atomique via SQL update guardé par balance >= amount.
//! - Ledger chaîne de hash : chaque `talent_transactions.ledger_hash` inclut
//!   le hash de la ligne précédente du même user, rejouable pour audit.
//!
//! Le canal payout (Stripe vs Mobile Money) est déterminé par
//! `talent_wallets.residency_country` — logique en P13.2/P13.3.

use bigdecimal::BigDecimal;
use chrono::{DateTime, Timelike, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Devises supportées. XOF = Franc CFA UEMOA (Afrique de l'Ouest).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Currency {
    Eur,
    Xof,
}

impl Currency {
    pub fn as_str(&self) -> &'static str {
        match self {
            Currency::Eur => "EUR",
            Currency::Xof => "XOF",
        }
    }

    pub fn from_str(s: &str) -> Result<Currency, AppError> {
        match s {
            "EUR" | "eur" => Ok(Currency::Eur),
            "XOF" | "xof" => Ok(Currency::Xof),
            _ => Err(AppError::Validation(format!(
                "unsupported currency '{s}' (only EUR and XOF)"
            ))),
        }
    }
}

/// Wallet complet d'un talent, incluant les infos de payout provider.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TalentWallet {
    pub user_id: Uuid,
    pub balance_eur: BigDecimal,
    pub balance_xof: BigDecimal,
    pub residency_country: Option<String>,
    pub stripe_account_id: Option<String>,
    pub stripe_kyc_status: String,
    pub momo_phone: Option<String>,
    pub momo_phone_verified: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Une ligne du ledger.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TalentTransaction {
    pub id: Uuid,
    pub user_id: Uuid,
    pub delta: BigDecimal,
    pub currency: String,
    pub reason: String,
    pub related_slice_id: Option<Uuid>,
    pub related_provider_txn_id: Option<String>,
    pub notes: Option<String>,
    pub prev_ledger_hash: Option<Vec<u8>>,
    pub ledger_hash: Vec<u8>,
    pub created_at: DateTime<Utc>,
}

/// Paramètres d'un crédit/débit.
#[derive(Debug, Clone)]
pub struct LedgerEntry<'a> {
    pub user_id: Uuid,
    pub delta: &'a BigDecimal,
    pub currency: Currency,
    pub reason: &'a str,
    pub related_slice_id: Option<Uuid>,
    pub related_provider_txn_id: Option<&'a str>,
    pub notes: Option<&'a str>,
}

/// Récupère ou initialise le wallet d'un user. Idempotent.
pub async fn get_or_init_wallet(
    db: &PgPool,
    user_id: Uuid,
) -> Result<TalentWallet, AppError> {
    let wallet = sqlx::query_as::<_, TalentWallet>(
        r#"
        INSERT INTO talent_wallets (user_id)
        VALUES ($1)
        ON CONFLICT (user_id) DO UPDATE SET user_id = talent_wallets.user_id
        RETURNING *
        "#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;
    Ok(wallet)
}

/// Met à jour le pays de résidence (utile pour choisir Stripe vs Momo).
pub async fn set_residency_country(
    db: &PgPool,
    user_id: Uuid,
    country_iso2: &str,
) -> Result<TalentWallet, AppError> {
    if country_iso2.len() != 2 {
        return Err(AppError::Validation(
            "country must be ISO 3166-1 alpha-2 (2 chars)".into(),
        ));
    }
    let wallet = sqlx::query_as::<_, TalentWallet>(
        r#"
        INSERT INTO talent_wallets (user_id, residency_country)
        VALUES ($1, $2)
        ON CONFLICT (user_id) DO UPDATE SET
            residency_country = EXCLUDED.residency_country,
            updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(country_iso2.to_uppercase())
    .fetch_one(db)
    .await?;
    Ok(wallet)
}

/// Récupère le hash de la ligne précédente du user (None si première tx).
async fn latest_ledger_hash(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
) -> Result<Option<Vec<u8>>, AppError> {
    let row: Option<(Vec<u8>,)> = sqlx::query_as(
        "SELECT ledger_hash FROM talent_transactions
         WHERE user_id = $1
         ORDER BY created_at DESC, id DESC
         LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|(h,)| h))
}

/// Compute le hash SHA-256 d'une transaction pour chaînage.
fn compute_ledger_hash(
    tx_id: Uuid,
    prev_hash: Option<&[u8]>,
    user_id: Uuid,
    delta: &BigDecimal,
    currency: Currency,
    reason: &str,
    related_slice_id: Option<Uuid>,
    created_at: DateTime<Utc>,
) -> Vec<u8> {
    let mut hasher = Sha256::new();
    if let Some(h) = prev_hash {
        hasher.update(h);
    }
    hasher.update(tx_id.as_bytes());
    hasher.update(user_id.as_bytes());
    hasher.update(delta.to_string().as_bytes());
    hasher.update(currency.as_str().as_bytes());
    hasher.update(reason.as_bytes());
    if let Some(sid) = related_slice_id {
        hasher.update(sid.as_bytes());
    }
    hasher.update(created_at.to_rfc3339().as_bytes());
    hasher.finalize().to_vec()
}

/// Crédit atomique : delta doit être > 0. Met à jour balance + insert tx.
///
/// Retourne la ligne de transaction créée (avec ledger_hash).
pub async fn credit(
    db: &PgPool,
    entry: LedgerEntry<'_>,
) -> Result<TalentTransaction, AppError> {
    if entry.delta <= &BigDecimal::from(0) {
        return Err(AppError::Validation("credit delta must be > 0".into()));
    }
    apply_ledger_entry(db, entry, true).await
}

/// Débit atomique : delta positif (montant à retirer). Refuse si balance < amount.
pub async fn debit(
    db: &PgPool,
    entry: LedgerEntry<'_>,
) -> Result<TalentTransaction, AppError> {
    if entry.delta <= &BigDecimal::from(0) {
        return Err(AppError::Validation("debit amount must be > 0".into()));
    }
    apply_ledger_entry(db, entry, false).await
}

/// Impl commune credit/debit — le signe du delta stocké est ± selon le sens.
async fn apply_ledger_entry(
    db: &PgPool,
    entry: LedgerEntry<'_>,
    is_credit: bool,
) -> Result<TalentTransaction, AppError> {
    let column = match entry.currency {
        Currency::Eur => "balance_eur",
        Currency::Xof => "balance_xof",
    };

    let mut tx = db.begin().await?;

    // Init wallet si nécessaire (evite les erreurs FK sur l'INSERT).
    sqlx::query(
        "INSERT INTO talent_wallets (user_id) VALUES ($1) ON CONFLICT DO NOTHING",
    )
    .bind(entry.user_id)
    .execute(&mut *tx)
    .await?;

    // Update balance, guardé par CHECK constraint.
    let sql = if is_credit {
        format!(
            "UPDATE talent_wallets SET {column} = {column} + $1, updated_at = NOW()
             WHERE user_id = $2 RETURNING {column}",
        )
    } else {
        format!(
            "UPDATE talent_wallets SET {column} = {column} - $1, updated_at = NOW()
             WHERE user_id = $2 AND {column} >= $1 RETURNING {column}",
        )
    };

    let updated: Option<(BigDecimal,)> = sqlx::query_as(&sql)
        .bind(entry.delta)
        .bind(entry.user_id)
        .fetch_optional(&mut *tx)
        .await?;

    if updated.is_none() {
        // Débit refusé (balance insuffisante) — le CHECK aurait aussi refusé un
        // credit qui overflow, mais concrètement seul le débit tombe ici.
        return Err(AppError::Validation(
            "insufficient balance for this debit".into(),
        ));
    }

    // Compute ledger hash.
    let tx_id = Uuid::new_v4();
    // PG TIMESTAMPTZ stores microseconds — on tronque en Rust pour que le
    // re-hash au verify (qui lit depuis PG) matche celui du write.
    let now = {
        let n = Utc::now();
        // Sub-nanos to sub-micros
        let micros = n.timestamp_subsec_micros();
        n.with_timezone(&Utc)
            .with_nanosecond(micros * 1000)
            .unwrap_or(n)
    };
    let prev_hash = latest_ledger_hash(&mut tx, entry.user_id).await?;
    let signed_delta = if is_credit {
        entry.delta.clone()
    } else {
        -entry.delta.clone()
    };
    let ledger_hash = compute_ledger_hash(
        tx_id,
        prev_hash.as_deref(),
        entry.user_id,
        &signed_delta,
        entry.currency,
        entry.reason,
        entry.related_slice_id,
        now,
    );

    let row: TalentTransaction = sqlx::query_as::<_, TalentTransaction>(
        r#"
        INSERT INTO talent_transactions
            (id, user_id, delta, currency, reason, related_slice_id,
             related_provider_txn_id, notes, prev_ledger_hash, ledger_hash, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        RETURNING *
        "#,
    )
    .bind(tx_id)
    .bind(entry.user_id)
    .bind(&signed_delta)
    .bind(entry.currency.as_str())
    .bind(entry.reason)
    .bind(entry.related_slice_id)
    .bind(entry.related_provider_txn_id)
    .bind(entry.notes)
    .bind(&prev_hash)
    .bind(&ledger_hash)
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(row)
}

/// Liste les transactions d'un user, ordre chronologique DESC.
pub async fn list_transactions(
    db: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<TalentTransaction>, AppError> {
    let rows = sqlx::query_as::<_, TalentTransaction>(
        "SELECT * FROM talent_transactions
         WHERE user_id = $1
         ORDER BY created_at DESC, id DESC
         LIMIT $2",
    )
    .bind(user_id)
    .bind(limit.clamp(1, 500))
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// P13.5 — Somme des débits (montants sortants) sur une fenêtre glissante.
///
/// Utilisé pour enforcer les limites journalières / mensuelles avant d'accepter
/// un withdraw. `delta < 0` = débit ; on retourne la somme en valeur absolue.
pub async fn debits_within(
    db: &PgPool,
    user_id: Uuid,
    currency: Currency,
    hours: i32,
) -> Result<BigDecimal, AppError> {
    let sum: Option<BigDecimal> = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(-delta), 0)::NUMERIC(14,2)
        FROM talent_transactions
        WHERE user_id = $1 AND currency = $2 AND delta < 0
          AND created_at > NOW() - ($3::TEXT || ' hours')::INTERVAL
        "#,
    )
    .bind(user_id)
    .bind(currency.as_str())
    .bind(hours.to_string())
    .fetch_one(db)
    .await?;
    Ok(sum.unwrap_or_else(|| BigDecimal::from(0)))
}

/// P13.5 — Export CSV statement (obligation fiscale + auto-consultation user).
///
/// Colonnes : `id,created_at,reason,delta,currency,related_slice_id,related_provider_txn_id`.
pub async fn statement_csv(db: &PgPool, user_id: Uuid) -> Result<String, AppError> {
    let rows = sqlx::query_as::<_, TalentTransaction>(
        "SELECT * FROM talent_transactions
         WHERE user_id = $1
         ORDER BY created_at ASC, id ASC",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let mut out = String::from(
        "id,created_at,reason,delta,currency,related_slice_id,related_provider_txn_id\n",
    );
    for r in rows {
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            r.id,
            r.created_at.to_rfc3339(),
            r.reason,
            r.delta,
            r.currency,
            r.related_slice_id
                .map(|u| u.to_string())
                .unwrap_or_default(),
            r.related_provider_txn_id.unwrap_or_default(),
        ));
    }
    Ok(out)
}

/// Vérifie l'intégrité de la chaîne de hash pour un user.
///
/// Retourne `Ok(true)` si la chaîne est cohérente, `Ok(false)` si une
/// transaction a été modifiée (chaîne rompue). Usage : audit périodique + test.
pub async fn verify_ledger_chain(
    db: &PgPool,
    user_id: Uuid,
) -> Result<bool, AppError> {
    let rows: Vec<TalentTransaction> = sqlx::query_as::<_, TalentTransaction>(
        "SELECT * FROM talent_transactions
         WHERE user_id = $1
         ORDER BY created_at ASC, id ASC",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let mut expected_prev: Option<Vec<u8>> = None;
    for row in &rows {
        let currency = Currency::from_str(&row.currency)?;
        let computed = compute_ledger_hash(
            row.id,
            expected_prev.as_deref(),
            row.user_id,
            &row.delta,
            currency,
            &row.reason,
            row.related_slice_id,
            row.created_at,
        );
        if computed != row.ledger_hash {
            return Ok(false);
        }
        if row.prev_ledger_hash != expected_prev {
            return Ok(false);
        }
        expected_prev = Some(row.ledger_hash.clone());
    }
    Ok(true)
}
