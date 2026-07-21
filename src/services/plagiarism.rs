//! P14.3 — Détection de plagiat cross-user via cosine similarity sur embeddings.
//!
//! Design :
//! - Chaque deliverable a un embedding (FLOAT4[]) stocké dans
//!   `deliverable_embeddings`.
//! - `scan_deliverable` calcule la cosine similarity contre les autres
//!   deliverables du même tenant sur les 30 derniers jours, et met à jour
//!   `deliverables.plagiarism_score` + `plagiarism_similar_to`.
//! - Seuil warning : `>= 0.90` = flag steward. `>= 0.95` = quasi-copie certaine.
//!
//! L'embedding lui-même est produit par un modèle NLP (via `grpc_ai_url`).
//! En P14.3 on expose le stockage + le scan ; la génération de l'embedding
//! côté client (endpoint /admin/fraud/embed) est ajoutée en P14.5.

use bigdecimal::BigDecimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Cosine similarity entre deux vecteurs de même dimension.
/// Retourne une valeur dans [-1.0, 1.0]. Renvoie 0.0 si dimensions
/// incompatibles ou norme nulle.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Stocke l'embedding d'un deliverable. Idempotent (upsert).
pub async fn store_embedding(
    db: &PgPool,
    deliverable_id: Uuid,
    tenant_id: Option<Uuid>,
    embedding: &[f32],
) -> Result<(), AppError> {
    if embedding.is_empty() {
        return Err(AppError::Validation("empty embedding".into()));
    }
    sqlx::query(
        r#"
        INSERT INTO deliverable_embeddings (deliverable_id, embedding, tenant_id)
        VALUES ($1, $2, $3)
        ON CONFLICT (deliverable_id) DO UPDATE SET
            embedding = EXCLUDED.embedding,
            tenant_id = EXCLUDED.tenant_id,
            created_at = NOW()
        "#,
    )
    .bind(deliverable_id)
    .bind(embedding)
    .bind(tenant_id)
    .execute(db)
    .await?;
    Ok(())
}

/// Résultat du scan : le deliverable le plus similaire trouvé + son score.
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub best_match_id: Option<Uuid>,
    pub best_score: f32,
    pub compared_count: usize,
}

/// Compare l'embedding d'un deliverable contre tous les autres deliverables
/// du même tenant sur les `window_days` derniers jours.
///
/// Met à jour `deliverables.plagiarism_score` + `plagiarism_similar_to` avec
/// le meilleur match, si un match ≥ `threshold` est trouvé. Sinon, marque
/// juste `plagiarism_scanned_at`.
pub async fn scan_deliverable(
    db: &PgPool,
    deliverable_id: Uuid,
    threshold: f32,
    window_days: i32,
) -> Result<ScanResult, AppError> {
    // 1. Charger l'embedding target.
    let target: Option<(Vec<f32>, Option<Uuid>)> = sqlx::query_as(
        "SELECT embedding, tenant_id
         FROM deliverable_embeddings
         WHERE deliverable_id = $1",
    )
    .bind(deliverable_id)
    .fetch_optional(db)
    .await?;
    let (target_emb, target_tenant) = target
        .ok_or_else(|| AppError::NotFound("No embedding stored for this deliverable".into()))?;

    // 2. Charger les candidats du même tenant.
    let candidates: Vec<(Uuid, Vec<f32>)> = sqlx::query_as(
        r#"
        SELECT deliverable_id, embedding
        FROM deliverable_embeddings
        WHERE deliverable_id <> $1
          AND (tenant_id IS NOT DISTINCT FROM $2)
          AND created_at > NOW() - ($3::TEXT || ' days')::INTERVAL
        "#,
    )
    .bind(deliverable_id)
    .bind(target_tenant)
    .bind(window_days.to_string())
    .fetch_all(db)
    .await?;

    // 3. Compute similarités.
    let mut best_id: Option<Uuid> = None;
    let mut best_score: f32 = -1.0;
    for (candidate_id, candidate_emb) in &candidates {
        let sim = cosine_similarity(&target_emb, candidate_emb);
        if sim > best_score {
            best_score = sim;
            best_id = Some(*candidate_id);
        }
    }

    // 4. Update le deliverable target.
    let (score_bd, similar_to) = if best_score >= threshold && best_id.is_some() {
        let score = BigDecimal::try_from(best_score as f64).ok();
        (score, best_id)
    } else {
        (None, None)
    };
    sqlx::query(
        r#"
        UPDATE deliverables
        SET plagiarism_score = $1,
            plagiarism_similar_to = $2,
            plagiarism_scanned_at = NOW()
        WHERE id = $3
        "#,
    )
    .bind(&score_bd)
    .bind(similar_to)
    .bind(deliverable_id)
    .execute(db)
    .await?;

    Ok(ScanResult {
        best_match_id: best_id,
        best_score: if best_score < 0.0 { 0.0 } else { best_score },
        compared_count: candidates.len(),
    })
}

/// Deliverables flagged (score >= threshold), triés par score DESC. Pour le
/// fraud dashboard admin (P14.5).
pub async fn list_flagged(
    db: &PgPool,
    threshold: BigDecimal,
    limit: i64,
) -> Result<Vec<(Uuid, BigDecimal, Option<Uuid>)>, AppError> {
    let rows: Vec<(Uuid, BigDecimal, Option<Uuid>)> = sqlx::query_as(
        r#"
        SELECT id, plagiarism_score, plagiarism_similar_to
        FROM deliverables
        WHERE plagiarism_score IS NOT NULL AND plagiarism_score >= $1
        ORDER BY plagiarism_score DESC
        LIMIT $2
        "#,
    )
    .bind(threshold)
    .bind(limit.clamp(1, 200))
    .fetch_all(db)
    .await?;
    Ok(rows)
}
