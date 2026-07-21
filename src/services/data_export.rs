//! GDPR data export (Phase 1.8).
//!
//! Endpoint triggers a background task that aggregates ALL data we hold about a user,
//! zips it, uploads to MinIO at `data-exports/{user_id}/{timestamp}.zip`, generates a
//! presigned URL valid 7 days, and emails the user.

use std::io::{Cursor, Write};
use std::sync::Arc;

use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::errors::AppError;
use crate::services::{EmailService, StorageService};

pub const EXPORT_KEY_PREFIX: &str = "data-exports/";

#[derive(Debug, Clone, Serialize)]
pub struct ExportArtifact {
    pub key: String,
    pub size_bytes: u64,
    pub url: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Run the full export for a single user. Designed to be spawned in a tokio task.
pub async fn generate_export(
    db: PgPool,
    storage: Arc<StorageService>,
    email: Arc<EmailService>,
    user_id: Uuid,
) -> Result<ExportArtifact, AppError> {
    let started = std::time::Instant::now();
    tracing::info!(%user_id, "data export started");

    let user_row = fetch_user(&db, user_id).await?;
    let to_email = user_row
        .get("email")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let display_name = user_row
        .get("display_name")
        .and_then(Value::as_str)
        .unwrap_or("user")
        .to_string();

    let mut buf = Cursor::new(Vec::<u8>::with_capacity(64 * 1024));
    {
        let mut zip = ZipWriter::new(&mut buf);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        write_json(&mut zip, "user.json", &user_row, options)?;
        write_json(
            &mut zip,
            "preferences.json",
            &fetch_table(&db, "user_email_preferences", user_id, "user_id").await?,
            options,
        )?;
        write_json(
            &mut zip,
            "privacy.json",
            &fetch_table(&db, "user_privacy", user_id, "user_id").await?,
            options,
        )?;
        write_json(
            &mut zip,
            "activity.json",
            &fetch_table(&db, "user_activity", user_id, "user_id").await?,
            options,
        )?;
        // P8.7 : skill_fragments retiré, user_skills seul dans l'export.
        write_json(
            &mut zip,
            "user_skills.json",
            &fetch_table(&db, "user_skills", user_id, "user_id").await?,
            options,
        )?;
        write_json(
            &mut zip,
            "badges.json",
            &fetch_table(&db, "user_badges", user_id, "user_id").await?,
            options,
        )?;
        write_json(
            &mut zip,
            "submissions.json",
            &fetch_table(&db, "challenge_submissions", user_id, "user_id").await?,
            options,
        )?;
        write_json(
            &mut zip,
            "notifications.json",
            &fetch_table(&db, "notifications", user_id, "user_id").await?,
            options,
        )?;
        write_json(
            &mut zip,
            "interest_requests_sent.json",
            &fetch_table(&db, "interest_requests", user_id, "talent_id").await?,
            options,
        )?;
        write_json(
            &mut zip,
            "conversations.json",
            &fetch_conversations(&db, user_id).await?,
            options,
        )?;
        write_json(
            &mut zip,
            "messages.json",
            &fetch_messages(&db, user_id).await?,
            options,
        )?;
        write_json(
            &mut zip,
            "email_log.json",
            &fetch_table(&db, "email_log", user_id, "user_id").await?,
            options,
        )?;

        // P9.1 : le code des submissions vit désormais dans
        // `deliverables.artifact_metadata.code_content`. On extrait les .txt à
        // partir de là pour les challenge_submissions liées.
        let subs: Vec<(String, Option<serde_json::Value>)> = sqlx::query_as(
            r#"
            SELECT COALESCE(d.challenge_id::text, ''), d.artifact_metadata
            FROM deliverables d
            WHERE d.user_id = $1
              AND d.artifact_metadata ? 'code_content'
            ORDER BY d.submitted_at
            "#,
        )
        .bind(user_id)
        .fetch_all(&db)
        .await?;
        for (cidx, (challenge_id, meta)) in subs.iter().enumerate() {
            let code = meta
                .as_ref()
                .and_then(|m| m.get("code_content"))
                .and_then(|c| c.as_str());
            if let Some(code) = code {
                let path = format!("deliverables/{cidx:03}_{challenge_id}.txt");
                zip.start_file(&path, options).map_err(zip_err)?;
                zip.write_all(code.as_bytes())
                    .map_err(|e| AppError::Internal(format!("zip write: {e}")))?;
            }
        }

        zip.finish().map_err(zip_err)?;
    }

    let bytes = buf.into_inner();
    let size_bytes = bytes.len() as u64;
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    let key = format!("{EXPORT_KEY_PREFIX}{user_id}/{timestamp}.zip");
    storage
        .upload_generic(&key, &bytes, "application/zip")
        .await?;

    // Presigned URL valid 7 days
    let expires_seconds: u32 = 7 * 24 * 3600;
    let url = storage.presigned_get_url(&key, expires_seconds).await?;
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_seconds as i64);

    let elapsed_ms = started.elapsed().as_millis();
    tracing::info!(%user_id, size_bytes, elapsed_ms, "data export generated");

    // Email user with the link (best-effort — failure not fatal for the export itself)
    let html = format!(
        r#"
        <div style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto;">
            <h2 style="color:#1a1a2e">Ton export de données Skilluv est prêt</h2>
            <p>Bonjour {display_name},</p>
            <p>L'archive complète de tes données est téléchargeable au lien ci-dessous. Le lien expire dans 7 jours.</p>
            <p style="text-align:center; margin: 30px 0;">
                <a href="{url}" style="background:#6c5ce7;color:white;padding:14px 28px;text-decoration:none;border-radius:8px;font-weight:bold;">
                    Télécharger mon archive ({mb:.1} MB)
                </a>
            </p>
            <p style="color:#666;font-size:12px;">Si tu n'as pas demandé cet export, signale-le immédiatement à security@skilluv.com.</p>
        </div>
        "#,
        url = url,
        mb = (size_bytes as f64) / 1024.0 / 1024.0,
        display_name = display_name,
    );
    if !to_email.is_empty()
        && let Err(err) = email
            .send_with_log(
                &db,
                crate::services::email::SendWithLogParams {
                    user_id,
                    to_email: &to_email,
                    to_name: &display_name,
                    subject: "Skilluv — Ton export de données est prêt",
                    html: &html,
                    kind: "data_export",
                },
            )
            .await
    {
        tracing::warn!(%user_id, error = %err, "data export email send failed");
    }

    Ok(ExportArtifact {
        key,
        size_bytes,
        url,
        expires_at,
    })
}

fn write_json<W: Write + std::io::Seek>(
    zip: &mut ZipWriter<W>,
    name: &str,
    value: &Value,
    options: SimpleFileOptions,
) -> Result<(), AppError> {
    zip.start_file(name, options).map_err(zip_err)?;
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| AppError::Internal(format!("json serialize: {e}")))?;
    zip.write_all(&bytes)
        .map_err(|e| AppError::Internal(format!("zip write: {e}")))?;
    Ok(())
}

fn zip_err(e: zip::result::ZipError) -> AppError {
    AppError::Internal(format!("zip: {e}"))
}

async fn fetch_user(db: &PgPool, user_id: Uuid) -> Result<Value, AppError> {
    let row: Option<sqlx::postgres::PgRow> = sqlx::query("SELECT * FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    match row {
        Some(r) => Ok(pg_row_to_json(&r, &["password_hash", "totp_secret"])),
        None => Err(AppError::NotFound("user not found".into())),
    }
}

async fn fetch_table(
    db: &PgPool,
    table: &str,
    user_id: Uuid,
    user_col: &str,
) -> Result<Value, AppError> {
    let sql = format!("SELECT * FROM {table} WHERE {user_col} = $1");
    let rows = sqlx::query(&sql).bind(user_id).fetch_all(db).await?;
    let arr: Vec<Value> = rows.iter().map(|r| pg_row_to_json(r, &[])).collect();
    Ok(Value::Array(arr))
}

async fn fetch_conversations(db: &PgPool, user_id: Uuid) -> Result<Value, AppError> {
    let rows = sqlx::query("SELECT * FROM conversations WHERE talent_id = $1")
        .bind(user_id)
        .fetch_all(db)
        .await?;
    let arr: Vec<Value> = rows.iter().map(|r| pg_row_to_json(r, &[])).collect();
    Ok(Value::Array(arr))
}

async fn fetch_messages(db: &PgPool, user_id: Uuid) -> Result<Value, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT m.*
        FROM messages m
        JOIN conversations c ON c.id = m.conversation_id
        WHERE c.talent_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    let arr: Vec<Value> = rows.iter().map(|r| pg_row_to_json(r, &[])).collect();
    Ok(Value::Array(arr))
}

/// Convert a PgRow into a JSON object using column metadata.
/// Sensitive columns listed in `skip_cols` are omitted.
fn pg_row_to_json(row: &sqlx::postgres::PgRow, skip_cols: &[&str]) -> Value {
    use sqlx::Column;
    use sqlx::Row;
    use sqlx::TypeInfo;

    let mut obj = serde_json::Map::new();
    for col in row.columns() {
        let name = col.name();
        if skip_cols.contains(&name) {
            continue;
        }
        let value = column_to_json(row, name, col.type_info().name());
        obj.insert(name.to_string(), value);
    }
    Value::Object(obj)
}

fn column_to_json(row: &sqlx::postgres::PgRow, name: &str, type_name: &str) -> Value {
    use sqlx::Row;
    match type_name {
        "TEXT" | "VARCHAR" | "CHAR" | "BPCHAR" | "CITEXT" => row
            .try_get::<Option<String>, _>(name)
            .ok()
            .flatten()
            .map(Value::String)
            .unwrap_or(Value::Null),
        "INT2" | "INT4" => row
            .try_get::<Option<i32>, _>(name)
            .ok()
            .flatten()
            .map(|v| json!(v))
            .unwrap_or(Value::Null),
        "INT8" => row
            .try_get::<Option<i64>, _>(name)
            .ok()
            .flatten()
            .map(|v| json!(v))
            .unwrap_or(Value::Null),
        "FLOAT4" | "FLOAT8" | "NUMERIC" => row
            .try_get::<Option<f64>, _>(name)
            .ok()
            .flatten()
            .map(|v| json!(v))
            .unwrap_or(Value::Null),
        "BOOL" => row
            .try_get::<Option<bool>, _>(name)
            .ok()
            .flatten()
            .map(|v| json!(v))
            .unwrap_or(Value::Null),
        "UUID" => row
            .try_get::<Option<Uuid>, _>(name)
            .ok()
            .flatten()
            .map(|v| Value::String(v.to_string()))
            .unwrap_or(Value::Null),
        "TIMESTAMPTZ" | "TIMESTAMP" => row
            .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(name)
            .ok()
            .flatten()
            .map(|v| Value::String(v.to_rfc3339()))
            .unwrap_or(Value::Null),
        "DATE" => row
            .try_get::<Option<chrono::NaiveDate>, _>(name)
            .ok()
            .flatten()
            .map(|v| Value::String(v.to_string()))
            .unwrap_or(Value::Null),
        "JSON" | "JSONB" => row
            .try_get::<Option<Value>, _>(name)
            .ok()
            .flatten()
            .unwrap_or(Value::Null),
        _ => {
            // Fallback: try as text, otherwise null
            row.try_get::<Option<String>, _>(name)
                .ok()
                .flatten()
                .map(Value::String)
                .unwrap_or(Value::Null)
        }
    }
}
