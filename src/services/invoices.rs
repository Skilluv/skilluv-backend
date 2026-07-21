//! Invoice generation (Phase 3.10).
//!
//! Sequential numbering per year (`SKL-YYYY-00001`). Records billing address snapshot.
//! PDF rendering is deferred to Phase 4 ; for now we expose a clean print-ready HTML
//! that browsers convert via Ctrl/Cmd+P → PDF.

use chrono::{Datelike, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Invoice {
    pub id: Uuid,
    pub invoice_number: String,
    pub enterprise_id: Uuid,
    pub amount_ht_cents: i64,
    pub amount_tva_cents: i64,
    pub amount_ttc_cents: i64,
    pub tva_rate: bigdecimal::BigDecimal,
    pub currency: String,
    pub billing_country: Option<String>,
    pub billing_company_name: Option<String>,
    pub billing_address: Option<String>,
    pub billing_vat_number: Option<String>,
    pub description: Option<String>,
    pub stripe_payment_intent_id: Option<String>,
    pub stripe_session_id: Option<String>,
    pub related_transaction_id: Option<Uuid>,
    pub issued_at: chrono::DateTime<Utc>,
}

pub struct CreateInvoiceInput<'a> {
    pub enterprise_id: Uuid,
    pub amount_ht_cents: i64,
    pub amount_tva_cents: i64,
    pub amount_ttc_cents: i64,
    pub tva_rate_pct: f64,
    pub currency: &'a str,
    pub billing_country: Option<&'a str>,
    pub billing_company_name: Option<&'a str>,
    pub billing_address: Option<&'a str>,
    pub billing_vat_number: Option<&'a str>,
    pub description: Option<&'a str>,
    pub stripe_payment_intent_id: Option<&'a str>,
    pub stripe_session_id: Option<&'a str>,
    pub related_transaction_id: Option<Uuid>,
}

pub async fn create(db: &PgPool, input: CreateInvoiceInput<'_>) -> Result<Invoice, AppError> {
    let now = Utc::now();
    let year = now.year();
    let next: (i32,) = sqlx::query_as("SELECT claim_invoice_number($1)")
        .bind(year)
        .fetch_one(db)
        .await?;
    let number = format!("SKL-{year}-{:05}", next.0);
    let tva_rate = crate::services::credits::dec_from_f64(input.tva_rate_pct);
    let inv: Invoice = sqlx::query_as(
        r#"
        INSERT INTO invoices
            (invoice_number, enterprise_id, amount_ht_cents, amount_tva_cents, amount_ttc_cents,
             tva_rate, currency, billing_country, billing_company_name, billing_address,
             billing_vat_number, description,
             stripe_payment_intent_id, stripe_session_id, related_transaction_id)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
        RETURNING *
        "#,
    )
    .bind(&number)
    .bind(input.enterprise_id)
    .bind(input.amount_ht_cents)
    .bind(input.amount_tva_cents)
    .bind(input.amount_ttc_cents)
    .bind(&tva_rate)
    .bind(input.currency)
    .bind(input.billing_country)
    .bind(input.billing_company_name)
    .bind(input.billing_address)
    .bind(input.billing_vat_number)
    .bind(input.description)
    .bind(input.stripe_payment_intent_id)
    .bind(input.stripe_session_id)
    .bind(input.related_transaction_id)
    .fetch_one(db)
    .await?;
    Ok(inv)
}

pub async fn list_for_enterprise(
    db: &PgPool,
    enterprise_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<Invoice>, AppError> {
    let rows = sqlx::query_as(
        "SELECT * FROM invoices WHERE enterprise_id = $1 ORDER BY issued_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(enterprise_id)
    .bind(limit.clamp(1, 200))
    .bind(offset.max(0))
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn by_id_for_enterprise(
    db: &PgPool,
    invoice_id: Uuid,
    enterprise_id: Uuid,
) -> Result<Invoice, AppError> {
    let row: Option<Invoice> =
        sqlx::query_as("SELECT * FROM invoices WHERE id = $1 AND enterprise_id = $2")
            .bind(invoice_id)
            .bind(enterprise_id)
            .fetch_optional(db)
            .await?;
    row.ok_or(AppError::NotFound("invoice not found".into()))
}

pub fn render_html(inv: &Invoice, enterprise_name: &str) -> String {
    fn esc(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
    fn money(cents: i64) -> String {
        format!("{:.2}", (cents as f64) / 100.0)
    }
    format!(
        r#"<!doctype html>
<html lang="fr">
<head>
<meta charset="utf-8">
<title>Facture {number}</title>
<meta name="robots" content="noindex">
<style>
  body {{ font-family: -apple-system, system-ui, Helvetica, Arial, sans-serif; max-width: 760px; margin: 30px auto; padding: 0 32px; color: #1a1a2e; }}
  header {{ display: flex; justify-content: space-between; align-items: flex-start; border-bottom: 2px solid #6c5ce7; padding-bottom: 16px; }}
  h1 {{ color: #6c5ce7; margin: 0; }}
  table {{ width: 100%; border-collapse: collapse; margin: 24px 0; }}
  th, td {{ text-align: left; padding: 10px; border-bottom: 1px solid #eee; }}
  th {{ background: #f4f4f9; }}
  .totals {{ margin-top: 18px; }}
  .totals td {{ border: 0; }}
  .totals .label {{ text-align: right; padding-right: 16px; color: #666; }}
  .totals .ttc {{ font-size: 18px; font-weight: bold; color: #6c5ce7; }}
  footer {{ margin-top: 40px; font-size: 11px; color: #888; border-top: 1px solid #eee; padding-top: 10px; }}
</style>
</head>
<body>
<header>
  <div>
    <h1>Skilluv</h1>
    <p style="margin:4px 0 0;color:#666;">Plateforme talents tech</p>
  </div>
  <div style="text-align:right;">
    <p style="margin:0;font-size:20px;font-weight:bold;">Facture {number}</p>
    <p style="margin:4px 0 0;color:#666;">Émise le {date}</p>
  </div>
</header>

<section style="display:flex;gap:40px;margin:24px 0;">
  <div style="flex:1;">
    <h3 style="margin:0 0 8px;color:#888;text-transform:uppercase;font-size:11px;letter-spacing:1px;">Facturé à</h3>
    <p style="margin:0;font-weight:bold;">{billing_name}</p>
    <p style="margin:0;white-space:pre-line;">{billing_addr}</p>
    {vat_line}
  </div>
  <div style="flex:1;">
    <h3 style="margin:0 0 8px;color:#888;text-transform:uppercase;font-size:11px;letter-spacing:1px;">Émetteur</h3>
    <p style="margin:0;font-weight:bold;">Skilluv</p>
    <p style="margin:0;">{seller_address}</p>
    <p style="margin:0;">SIRET : {siret}</p>
    <p style="margin:0;">TVA : {seller_vat}</p>
  </div>
</section>

<table>
  <thead>
    <tr><th>Description</th><th style="text-align:right;">Montant HT</th></tr>
  </thead>
  <tbody>
    <tr>
      <td>{desc}</td>
      <td style="text-align:right;">{ht_amount} {currency}</td>
    </tr>
  </tbody>
</table>

<table class="totals">
  <tr><td class="label">Total HT</td><td style="text-align:right;">{ht_amount} {currency}</td></tr>
  <tr><td class="label">TVA ({tva_rate}%)</td><td style="text-align:right;">{tva_amount} {currency}</td></tr>
  <tr><td class="label ttc">Total TTC</td><td class="ttc" style="text-align:right;">{ttc_amount} {currency}</td></tr>
</table>

<footer>
  Numéro de facture séquentiel. Conformément à la réglementation française, cette facture est archivée 10 ans.<br>
  Référence paiement Stripe : {pi}
</footer>
</body>
</html>"#,
        number = esc(&inv.invoice_number),
        date = inv.issued_at.format("%d/%m/%Y").to_string(),
        billing_name = esc(inv
            .billing_company_name
            .as_deref()
            .unwrap_or(enterprise_name)),
        billing_addr = esc(inv.billing_address.as_deref().unwrap_or("")),
        vat_line = inv
            .billing_vat_number
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|v| format!("<p style=\"margin:4px 0 0;\">TVA : {}</p>", esc(v)))
            .unwrap_or_default(),
        seller_address =
            std::env::var("SKILLUV_BILLING_ADDRESS").unwrap_or_else(|_| "Adresse Skilluv".into()),
        siret = std::env::var("SKILLUV_SIRET").unwrap_or_else(|_| "TODO".into()),
        seller_vat = std::env::var("SKILLUV_VAT_NUMBER").unwrap_or_else(|_| "TODO".into()),
        desc = esc(inv.description.as_deref().unwrap_or("Crédits Skilluv")),
        ht_amount = money(inv.amount_ht_cents),
        tva_amount = money(inv.amount_tva_cents),
        ttc_amount = money(inv.amount_ttc_cents),
        tva_rate = inv.tva_rate,
        currency = esc(&inv.currency),
        pi = esc(inv.stripe_payment_intent_id.as_deref().unwrap_or("—")),
    )
}
