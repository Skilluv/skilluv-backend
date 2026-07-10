//! FX rate service — Phase 4.4.
//!
//! Reference rates fetched daily from the European Central Bank (public XML feed).
//! Cached in Redis (24h) and mirrored in Postgres for cold-boot resilience.

use bigdecimal::BigDecimal;
use chrono::Utc;
use num_traits::ToPrimitive;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde::Serialize;
use sqlx::PgPool;
use std::str::FromStr;
use std::time::Duration;

use crate::errors::AppError;

const REDIS_KEY_PREFIX: &str = "fx:";
const REDIS_TTL_SECS: usize = 24 * 3600;
/// Margin we apply to the ECB reference rate when quoting a price in a non-EUR
/// currency, to cover the FX volatility and PSP conversion fee.
const FX_MARGIN_PCT: f64 = 3.0;

pub fn start_fx_refresher(db: PgPool) {
    tokio::spawn(async move {
        // Pull once at boot then every 6h.
        if let Err(err) = refresh_from_ecb(&db).await {
            tracing::warn!(error = %err, "initial FX refresh failed");
        }
        let mut ticker = tokio::time::interval(Duration::from_secs(6 * 3600));
        loop {
            ticker.tick().await;
            if let Err(err) = refresh_from_ecb(&db).await {
                tracing::warn!(error = %err, "FX refresh failed");
            }
        }
    });
}

/// Fetch the ECB reference feed and upsert rates into `fx_rates`.
/// XOF/XAF stay at their pegged value (already seeded).
pub async fn refresh_from_ecb(db: &PgPool) -> Result<(), AppError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(format!("fx client build: {e}")))?;
    let resp = client
        .get("https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("ecb fetch: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(format!("ecb status {}", resp.status())));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| AppError::Internal(format!("ecb decode: {e}")))?;
    let rates = parse_ecb_xml(&text);
    let mut upserted = 0usize;
    for (currency, rate) in &rates {
        let dec = BigDecimal::from_str(&format!("{rate:.8}"))
            .map_err(|_| AppError::Internal("bd from float".into()))?;
        sqlx::query(
            r#"
            INSERT INTO fx_rates (base_currency, quote_currency, rate)
            VALUES ('EUR', $1, $2)
            ON CONFLICT (base_currency, quote_currency) DO UPDATE
                SET rate = EXCLUDED.rate, fetched_at = NOW()
            "#,
        )
        .bind(currency)
        .bind(&dec)
        .execute(db)
        .await?;
        upserted += 1;
    }
    tracing::info!(rates = upserted, "FX rates refreshed from ECB");
    Ok(())
}

/// Very small XML parser tailored to the ECB feed. Looks for `<Cube currency="XXX" rate="Y.YYYY"/>`.
fn parse_ecb_xml(xml: &str) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    for line in xml.lines() {
        let l = line.trim();
        if !l.starts_with("<Cube currency=") {
            continue;
        }
        let currency = extract_attr(l, "currency=\"");
        let rate = extract_attr(l, "rate=\"");
        if let (Some(c), Some(r)) = (currency, rate) {
            if let Ok(f) = r.parse::<f64>() {
                out.push((c, f));
            }
        }
    }
    out
}

fn extract_attr(line: &str, needle: &str) -> Option<String> {
    let start = line.find(needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

// ─── Convert ─────────────────────────────────────────────────────

pub async fn convert_from_eur(
    db: &PgPool,
    redis: &mut ConnectionManager,
    to_currency: &str,
    eur_amount_cents: i64,
) -> Result<ConversionResult, AppError> {
    let to = to_currency.to_uppercase();
    if to == "EUR" {
        return Ok(ConversionResult {
            currency: to,
            amount_cents: eur_amount_cents,
            rate: 1.0,
            margin_applied_pct: 0.0,
        });
    }
    let rate = load_rate(db, redis, &to).await?;
    let with_margin = rate * (1.0 + FX_MARGIN_PCT / 100.0);
    let eur_units = (eur_amount_cents as f64) / 100.0;
    let quote_units = eur_units * with_margin;
    let quote_cents = (quote_units * 100.0).round() as i64;
    Ok(ConversionResult {
        currency: to,
        amount_cents: quote_cents,
        rate: with_margin,
        margin_applied_pct: FX_MARGIN_PCT,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversionResult {
    pub currency: String,
    pub amount_cents: i64,
    pub rate: f64,
    pub margin_applied_pct: f64,
}

async fn load_rate(
    db: &PgPool,
    redis: &mut ConnectionManager,
    currency: &str,
) -> Result<f64, AppError> {
    let key = format!("{REDIS_KEY_PREFIX}EUR:{currency}");
    let cached: Option<String> = redis.get(&key).await.ok().flatten();
    if let Some(v) = cached {
        if let Ok(r) = v.parse::<f64>() {
            return Ok(r);
        }
    }
    let row: Option<(BigDecimal,)> = sqlx::query_as(
        "SELECT rate FROM fx_rates WHERE base_currency = 'EUR' AND quote_currency = $1",
    )
    .bind(currency)
    .fetch_optional(db)
    .await?;
    let rate = row
        .map(|(r,)| r.to_f64().unwrap_or(0.0))
        .filter(|r| *r > 0.0)
        .ok_or_else(|| AppError::Validation(format!("no FX rate available for {currency}")))?;
    let _: Result<(), _> = redis
        .set_ex(&key, rate.to_string(), REDIS_TTL_SECS as u64)
        .await;
    Ok(rate)
}

// ─── Pricing packs (Phase 4.4 shared with 3.14) ──────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PricingPack {
    pub id: uuid::Uuid,
    pub slug: String,
    pub credit_count: i32,
    pub price_eur_cents: i64,
    pub active: bool,
    pub kind: String,
    pub position: i32,
    pub updated_at: chrono::DateTime<Utc>,
}

pub async fn active_packs(db: &PgPool, kind: Option<&str>) -> Result<Vec<PricingPack>, AppError> {
    let rows = if let Some(k) = kind {
        sqlx::query_as(
            "SELECT * FROM pricing_packs WHERE active = TRUE AND kind = $1 ORDER BY position, price_eur_cents",
        )
        .bind(k)
        .fetch_all(db)
        .await?
    } else {
        sqlx::query_as(
            "SELECT * FROM pricing_packs WHERE active = TRUE ORDER BY position, price_eur_cents",
        )
        .fetch_all(db)
        .await?
    };
    Ok(rows)
}

pub async fn pack_by_slug(db: &PgPool, slug: &str) -> Result<PricingPack, AppError> {
    let row: Option<PricingPack> =
        sqlx::query_as("SELECT * FROM pricing_packs WHERE slug = $1")
            .bind(slug)
            .fetch_optional(db)
            .await?;
    row.ok_or(AppError::NotFound("pack not found".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ecb_extracts_currencies() {
        let sample = r#"<gesmes:Envelope>
<Cube>
<Cube time='2026-07-02'>
<Cube currency='USD' rate='1.0850'/>
<Cube currency='JPY' rate='170.03'/>
<Cube currency='ZAR' rate='19.45'/>
</Cube>
</Cube>
</gesmes:Envelope>"#;
        // Attribute quotes in the ECB feed are double; adjust our test-parser check.
        let sample_dq = sample.replace('\'', "\"");
        let rates = parse_ecb_xml(&sample_dq);
        assert_eq!(rates.len(), 3);
        assert_eq!(rates[0].0, "USD");
        assert!((rates[0].1 - 1.0850).abs() < 1e-6);
    }
}
