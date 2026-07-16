# Audit log — append-only, rétention 7 ans, export S3

**Statut au 2026-07-15** : migration 0099 livrée (REVOKE UPDATE/DELETE + rôle `audit_admin`). Cron export S3 non implémenté (stub documenté ci-dessous).

## Ce qui est en place

- **Tables**:
  - `admin_audit_log` (0014) — actions admin (bans, revokes, KYC decisions, etc.).
  - `audit_log` (0024) — audit générique (actor_type peut être system/enterprise/user).
- **Append-only** : REVOKE UPDATE, DELETE sur les 2 tables. Effectif en prod avec rôle NOSUPERUSER.
- **Rôle read-only `audit_admin`** : SELECT-only sur audit tables. À utiliser par SOC / SRE / DPO.
- **Env vars** :
  - `SKILLUV_AUDIT_RETENTION_DAYS` (default 2555 = 7 ans) — non utilisé au MVP (cron pas actif).

## Ce qui manque pour prod

### 1. Rôle app NOSUPERUSER dédié

En dev, `skilluv` est superuser → REVOKE symbolique. En prod, créer :

```sql
CREATE ROLE skilluv_app NOSUPERUSER NOBYPASSRLS LOGIN
    PASSWORD 'strong_secret';
GRANT CONNECT ON DATABASE skilluv TO skilluv_app;
GRANT USAGE ON SCHEMA public TO skilluv_app;
GRANT SELECT, INSERT ON admin_audit_log, audit_log TO skilluv_app;
-- Note: UPDATE/DELETE non granted → append-only enforced
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO skilluv_app;
-- Puis REVOKE UPDATE, DELETE sur les 2 audit tables (déjà fait par 0099).
```

`DATABASE_URL` prod pointe sur `skilluv_app`. Migrations continuent sur `skilluv` (superuser).

### 2. Password audit_admin

En prod : `ALTER ROLE audit_admin WITH PASSWORD 'xxx'`. Rotation via secrets manager.

### 3. Cron export S3 (à implémenter en BE-E.2)

Objectif : rétention 7 ans en DB, puis export S3 avec Object Lock (immutable) avant DELETE.

Design proposé :

```rust
// services/audit_exporter.rs (non implémenté)
pub async fn export_and_prune(db: &PgPool, s3: &S3Client) -> Result<Report> {
    let retention_days: i32 = std::env::var("SKILLUV_AUDIT_RETENTION_DAYS")
        .ok().and_then(|s| s.parse().ok()).unwrap_or(2555);

    // 1. Fetch rows older than retention.
    let old_rows: Vec<AuditRow> = sqlx::query_as(
        "SELECT * FROM audit_log WHERE created_at < NOW() - MAKE_INTERVAL(days => $1)"
    ).bind(retention_days).fetch_all(db).await?;

    if old_rows.is_empty() { return Ok(Report::empty()); }

    // 2. Serialize en NDJSON compressé gzip.
    let payload = compress_ndjson(&old_rows)?;

    // 3. Upload S3 avec Object Lock (mode COMPLIANCE, retain 10 ans).
    let key = format!("skilluv/audit-log/{}.ndjson.gz", chrono::Utc::now().format("%Y-%m-%d"));
    s3.put_object()
        .bucket(&bucket)
        .key(&key)
        .body(payload.into())
        .object_lock_mode(ObjectLockMode::Compliance)
        .object_lock_retain_until_date(chrono::Utc::now() + chrono::Duration::days(3650))
        .send().await?;

    // 4. DELETE côté DB (safe : rows en S3 en Object Lock).
    let deleted = sqlx::query(
        "DELETE FROM audit_log WHERE created_at < NOW() - MAKE_INTERVAL(days => $1)"
    ).bind(retention_days).execute(db).await?.rows_affected();

    Ok(Report { archived: old_rows.len(), deleted, s3_key: key })
}
```

**Dépendances** :
- Crate `aws-sdk-s3` avec Object Lock support.
- Bucket S3 (ou compatible) avec Object Lock activé au moment de création (impossible à activer après).
- KMS key pour SSE-KMS (optionnel mais recommandé).

**Wire** : tokio task périodique dans `main.rs`, similaire à `start_proof_sweep_task` (P19.3). Intervalle quotidien.

**Effort** : ~2 jours dev + config infra AWS/Cloudflare R2. À faire en BE-E.2 quand cash/temps permettent.

### 4. Alerting anomalies

À ajouter (post-MVP) :
- Alert Grafana si `admin_audit_log` INSERT rate > baseline (spike = compromission possible).
- Alert si retention window > 7 ans + cron désactivé (accumulation illimitée).

## Résumé responsabilités

| Rôle | Peut faire |
|---|---|
| `skilluv` (superuser dev) | tout (bypass REVOKE) |
| `skilluv_app` (NOSUPERUSER prod) | INSERT + SELECT sur audit — pas UPDATE ni DELETE |
| `audit_admin` (read-only) | SELECT sur audit_log + admin_audit_log |
| Migrations | tourner sous `skilluv` (superuser) pour ATTACH/DETACH policies |
| Cron export S3 | tourner sous rôle dédié `audit_exporter` avec DELETE sur audit tables + PUT S3 |

## Conformité

- **GDPR Art. 30** (registre des traitements) — les logs admin_audit_log documentent chaque action sur les données personnelles.
- **SOC 2 CC7.2** (log de sécurité) — append-only + rétention 7 ans + accès restreint.
- **ISO 27001 A.12.4** (event logging) — même contract.
