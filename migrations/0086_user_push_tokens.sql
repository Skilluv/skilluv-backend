-- Phase P15.1 — Tokens push mobiles (FCM Android + APNS iOS).
-- Migration 0086.
--
-- Rationale :
--   Web Push (VAPID) est déjà en place (services/push_sender.rs, Phase 4.12)
--   pour les browsers. Pour les apps mobiles natives, il faut :
--   - FCM (Firebase Cloud Messaging) pour Android.
--   - APNS (Apple Push Notification Service) pour iOS.
--
--   Chaque device envoie son token au boot ; on le stocke ici pour pouvoir
--   pusher les notifications côté serveur (NotificationService::send).
--
-- Design :
--   - `device_id` : identifiant stable du device côté client (ex: UUID
--     généré au premier boot et persisté localement). Permet de retirer
--     un token spécifique quand l'app détecte une régénération.
--   - UNIQUE (user_id, device_id) : un device par user.
--   - Rétention : sur 404/InvalidRegistration au push, on supprime le token.
--   - `last_seen_at` : refreshé au register + à chaque push envoyé avec succès.

CREATE TABLE user_push_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    platform VARCHAR(10) NOT NULL CHECK (platform IN ('fcm', 'apns')),
    token TEXT NOT NULL,
    device_id VARCHAR(128) NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, device_id)
);

-- Recherche : "tokens actifs d'un user" (push).
CREATE INDEX idx_user_push_tokens_user
    ON user_push_tokens (user_id, last_seen_at DESC);

-- Cleanup : purger les tokens inactifs > 90j (cron).
CREATE INDEX idx_user_push_tokens_stale
    ON user_push_tokens (last_seen_at);
