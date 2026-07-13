-- Phase P14.3 — Anti-plagiat via embeddings + cosine similarity.
-- Migration 0084.
--
-- Rationale :
--   Aujourd'hui `deliverables.artifact_hash` (SHA-256) empêche un même user
--   de re-soumettre le même contenu. Mais deux users différents avec le même
--   code (copié-collé) passent. Pour scaler la modération, on ajoute :
--   - `deliverable_embeddings(deliverable_id, embedding FLOAT4[N])` : vecteur
--     dense produit par un modèle embeddings (calculé via `grpc_ai_url` en
--     prod, ou fourni côté test).
--   - `deliverables.plagiarism_score NUMERIC(4,3)` : score max de similarité
--     cosinus contre les 30 derniers jours du même tenant. Setté par le job
--     `POST /api/admin/fraud/scan-deliverable/{id}` (P14.5).
--   - `deliverables.plagiarism_similar_to UUID` : lien vers la deliverable la
--     plus similaire (audit).
--
-- On garde les embeddings en FLOAT4[] plutôt que VECTOR (pgvector) pour éviter
-- une dépendance extension. La cosine similarity est calculée en Rust côté
-- service (petit volume : ~30 derniers jours × ~100 rows/tenant).

ALTER TABLE deliverables
    ADD COLUMN IF NOT EXISTS plagiarism_score NUMERIC(4,3)
        CHECK (plagiarism_score IS NULL OR (plagiarism_score >= 0.0 AND plagiarism_score <= 1.0)),
    ADD COLUMN IF NOT EXISTS plagiarism_similar_to UUID
        REFERENCES deliverables(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS plagiarism_scanned_at TIMESTAMPTZ;

-- Dashboard fraud : deliverables flagged > threshold
CREATE INDEX IF NOT EXISTS idx_deliverables_plagiarism_flagged
    ON deliverables (plagiarism_score DESC)
    WHERE plagiarism_score IS NOT NULL AND plagiarism_score >= 0.9;

CREATE TABLE IF NOT EXISTS deliverable_embeddings (
    deliverable_id UUID PRIMARY KEY REFERENCES deliverables(id) ON DELETE CASCADE,
    embedding REAL[] NOT NULL,
    tenant_id UUID REFERENCES tenants(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Requête typique : "récupérer tous les embeddings du même tenant sur les
-- 30 derniers jours" pour comparer.
CREATE INDEX IF NOT EXISTS idx_deliverable_embeddings_tenant_time
    ON deliverable_embeddings (tenant_id, created_at DESC);
