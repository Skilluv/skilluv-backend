-- Phase 5.9 — White-label multi-tenancy.
--
-- Un tenant est un espace isolé (école, bootcamp) qui héberge ses talents,
-- ses challenges privés, sa charte visuelle. Le tenant par défaut = 'skilluv'
-- (existant) ; les nouveaux tenants sont des espaces additionnels.

CREATE TABLE tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(60) NOT NULL UNIQUE,
    name VARCHAR(200) NOT NULL,
    -- Sous-domaine qui identifie ce tenant (ex: acme-bootcamp.skilluv.com)
    subdomain VARCHAR(80) UNIQUE,
    custom_domain VARCHAR(200) UNIQUE,
    logo_url TEXT,
    primary_color VARCHAR(9) DEFAULT '#6C5CE7',
    secondary_color VARCHAR(9),
    plan VARCHAR(20) NOT NULL DEFAULT 'starter'
        CHECK (plan IN ('starter', 'pro', 'enterprise')),
    max_users INTEGER NOT NULL DEFAULT 100,
    contact_email VARCHAR(255) NOT NULL,
    settings JSONB NOT NULL DEFAULT '{}',
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tenants_subdomain ON tenants (subdomain) WHERE subdomain IS NOT NULL;

-- Seed du tenant racine
INSERT INTO tenants (id, slug, name, subdomain, contact_email, plan, max_users)
VALUES (
    '00000000-0000-0000-0000-000000000001',
    'skilluv',
    'Skilluv',
    'app',
    'ops@skilluv.com',
    'enterprise',
    1000000
)
ON CONFLICT (slug) DO NOTHING;

-- Rattachement users → tenants (un user peut appartenir à plusieurs tenants,
-- mais un tenant "primary" par user pour l'espace par défaut).
ALTER TABLE users ADD COLUMN IF NOT EXISTS primary_tenant_id UUID
    REFERENCES tenants(id) ON DELETE SET NULL;
UPDATE users SET primary_tenant_id = '00000000-0000-0000-0000-000000000001'
    WHERE primary_tenant_id IS NULL;

CREATE TABLE tenant_memberships (
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(20) NOT NULL DEFAULT 'member'
        CHECK (role IN ('member', 'instructor', 'admin', 'owner')),
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, user_id)
);

CREATE INDEX idx_tenant_memberships_user ON tenant_memberships (user_id);

-- Backfill : tous les users existants sont membres du tenant racine.
INSERT INTO tenant_memberships (tenant_id, user_id, role)
SELECT '00000000-0000-0000-0000-000000000001', id,
       CASE role WHEN 'admin' THEN 'admin' ELSE 'member' END
FROM users
ON CONFLICT DO NOTHING;

-- Challenges privés d'un tenant (visibles seulement pour ses membres)
ALTER TABLE challenges ADD COLUMN IF NOT EXISTS tenant_id UUID
    REFERENCES tenants(id) ON DELETE CASCADE;
-- NULL = challenge public (visible partout).
CREATE INDEX IF NOT EXISTS idx_challenges_tenant ON challenges (tenant_id)
    WHERE tenant_id IS NOT NULL;

-- Cohortes / groupes internes au tenant (ex: promo 2026)
CREATE TABLE tenant_cohorts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name VARCHAR(120) NOT NULL,
    starts_at TIMESTAMPTZ,
    ends_at TIMESTAMPTZ,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, name)
);

CREATE TABLE tenant_cohort_members (
    cohort_id UUID NOT NULL REFERENCES tenant_cohorts(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (cohort_id, user_id)
);
