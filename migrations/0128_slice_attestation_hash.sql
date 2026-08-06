-- SKI-90 (P26 v2 F-02) — immutable proof hash on validated slices.
--
-- When a validator approves a PR (`services/slice_validation::approve`),
-- the service computes a SHA-256 over the tuple
--   (slice_id, submitted_pr_url, validated_at, validated_by_user_id, secret)
-- and stores it as 64-char lowercase hex. The `secret` component is
-- `JWT_SECRET` so an attacker who reads the DB cannot forge a matching
-- hash for a different slice_id (they'd need the running server secret).
--
-- Purpose: give the attestation issued to the challenger a stable,
-- verifiable identifier the frontend can display and third parties can
-- check via a future `/attestations/{hash}` endpoint. The row remains
-- the source of truth; the hash is derived, not primary.

ALTER TABLE project_slices
    ADD COLUMN IF NOT EXISTS attestation_hash CHAR(64)
    CHECK (attestation_hash IS NULL OR attestation_hash ~ '^[0-9a-f]{64}$');

-- Lookup by hash (future verifier endpoint).
CREATE UNIQUE INDEX IF NOT EXISTS uq_slice_attestation_hash
    ON project_slices (attestation_hash)
    WHERE attestation_hash IS NOT NULL;
