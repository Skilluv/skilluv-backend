-- Phase P14.4 — Fingerprinting login pour detection multi-account.
-- Migration 0085.
--
-- Rationale :
--   Sans anti-multi-account, un utilisateur peut créer 10 comptes, résoudre
--   un challenge sur le premier, puis 9 fois de plus pour farm des fragments
--   ou attester d'une "compétence" de manière artificielle. Le fingerprinting
--   au moment du login capture 3 signatures :
--   - `ip` : hash de l'IP source (SHA-256 hex pour éviter stocker en clair).
--   - `ua_hash` : hash du User-Agent.
--   - `canvas_hash` : hash canvas fingerprint envoyé par le client (JS).
--
--   Le job `detect_multi_accounts` (P14.5) flag `users.suspected_multi_account`
--   quand N > 3 comptes partagent 2 des 3 signatures dans une même journée.
--
-- Design :
--   - Une ligne par login (append-only), rétention 90 jours via job de purge.
--   - Pas de FK stricte user_id → users pour préserver l'historique même en
--     cas de compte supprimé (on garde les fingerprints pour analyse).

CREATE TABLE user_fingerprints (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,
    ip_hash VARCHAR(64) NOT NULL,   -- SHA-256 hex de l'IP source.
    ua_hash VARCHAR(64) NOT NULL,   -- SHA-256 hex du User-Agent.
    canvas_hash VARCHAR(64),        -- Fingerprint canvas (optionnel, JS-generated).
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Requête typique : "quels user_ids partagent le même ip+ua sur les 24 derniers h ?"
CREATE INDEX idx_user_fingerprints_ip_ua
    ON user_fingerprints (ip_hash, ua_hash, created_at DESC);

-- Job de purge : cron delete created_at < NOW() - 90d.
CREATE INDEX idx_user_fingerprints_created
    ON user_fingerprints (created_at);

-- Recherche : "toutes les traces d'un user".
CREATE INDEX idx_user_fingerprints_user
    ON user_fingerprints (user_id, created_at DESC);

-- ═══════════════════════════════════════════════════════════════════
-- Flag sur users pour cas suspects
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS suspected_multi_account BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS suspected_multi_account_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS suspected_multi_account_reason TEXT;

-- Dashboard admin : liste des users suspects.
CREATE INDEX IF NOT EXISTS idx_users_suspected_multi
    ON users (suspected_multi_account_at DESC)
    WHERE suspected_multi_account = TRUE;
