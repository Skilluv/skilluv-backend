-- Enterprise accounts
CREATE TABLE enterprises (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id UUID NOT NULL REFERENCES users(id),
    company_name VARCHAR(200) NOT NULL,
    slug VARCHAR(200) NOT NULL UNIQUE,
    description TEXT,
    website VARCHAR(500),
    logo_url VARCHAR(500),
    industry VARCHAR(100),
    company_size VARCHAR(30) NOT NULL CHECK (company_size IN ('1-10', '11-50', '51-200', '201-500', '501-1000', '1000+')),
    verified BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX idx_enterprises_owner ON enterprises (owner_id);
CREATE UNIQUE INDEX idx_enterprises_slug ON enterprises (slug);

-- Enterprise team members (owner + recruiters)
CREATE TABLE enterprise_members (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(20) NOT NULL DEFAULT 'recruiter' CHECK (role IN ('owner', 'recruiter')),
    invited_by UUID REFERENCES users(id),
    invited_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    accepted_at TIMESTAMPTZ,
    status VARCHAR(20) NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'active', 'revoked')),
    UNIQUE (enterprise_id, user_id)
);

CREATE INDEX idx_enterprise_members_enterprise ON enterprise_members (enterprise_id);
CREATE INDEX idx_enterprise_members_user ON enterprise_members (user_id);
