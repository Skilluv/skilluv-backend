-- Enterprise B2B SSO — OIDC configuration per enterprise.
--
-- Un enterprise peut configurer un IdP OIDC (Okta, Azure AD, Google Workspace,
-- Auth0, Keycloak, …). Ses recruteurs se connectent via
-- GET /api/enterprise/sso/{slug}/start → IdP → callback → session Skilluv.
--
-- Le client_secret IdP est chiffré at-rest en AES-256-GCM avec SSO_ENCRYPTION_KEY
-- (env var, 32 bytes base64). Refus au boot en prod si absente.

CREATE TABLE enterprise_sso_configs (
    enterprise_id UUID PRIMARY KEY REFERENCES enterprises(id) ON DELETE CASCADE,

    -- OIDC issuer URL (utilisé pour discovery: {issuer}/.well-known/openid-configuration)
    issuer TEXT NOT NULL,
    client_id TEXT NOT NULL,

    -- AES-256-GCM ciphertext + 12-byte nonce
    client_secret_encrypted BYTEA NOT NULL,
    client_secret_nonce BYTEA NOT NULL,

    -- Domaines email autorisés pour la découverte SSO ("acme.com").
    -- Un email @acme.com peut initier un login SSO pour cette entreprise.
    email_domains TEXT[] NOT NULL DEFAULT '{}',

    -- Si TRUE, POST /auth/login est refusé pour tout email dont le domaine matche
    -- (retourne SSO_REQUIRED avec l'URL de start).
    enforce_sso BOOLEAN NOT NULL DEFAULT FALSE,

    -- Just-in-time provisioning: crée un user + membership au premier login SSO.
    auto_provision BOOLEAN NOT NULL DEFAULT TRUE,

    -- Rôle assigné aux users auto-provisionnés (recruiter par défaut).
    default_role VARCHAR(20) NOT NULL DEFAULT 'recruiter'
        CHECK (default_role IN ('recruiter', 'enterprise')),

    disabled_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index GIN pour matcher un email domain rapidement dans la découverte.
CREATE INDEX idx_enterprise_sso_email_domains ON enterprise_sso_configs
    USING GIN (email_domains);

-- Trace des logins SSO pour l'audit / debugging.
ALTER TABLE user_sessions
    ADD COLUMN IF NOT EXISTS login_method VARCHAR(20) NOT NULL DEFAULT 'password'
        CHECK (login_method IN ('password', 'oauth', 'sso', 'magic_link', 'webauthn'));

CREATE INDEX IF NOT EXISTS idx_user_sessions_login_method ON user_sessions (login_method);
