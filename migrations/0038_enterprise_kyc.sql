-- Phase 4.5 — KYC enterprise (levels: none / basic / full).

CREATE TABLE enterprise_kyc (
    enterprise_id UUID PRIMARY KEY REFERENCES enterprises(id) ON DELETE CASCADE,
    level VARCHAR(10) NOT NULL DEFAULT 'none' CHECK (level IN ('none', 'basic', 'full')),
    status VARCHAR(20) NOT NULL DEFAULT 'not_started'
        CHECK (status IN ('not_started', 'pending', 'approved', 'rejected')),
    monthly_spend_eur_cents BIGINT NOT NULL DEFAULT 0,
    reviewed_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    reviewed_at TIMESTAMPTZ,
    rejection_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Documents uploaded for a KYC review. `storage_key` points to MinIO.
CREATE TABLE enterprise_kyc_documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    kind VARCHAR(40) NOT NULL,
    -- 'kbis' | 'articles_of_association' | 'siege_proof' | 'director_id' | 'bank_rib' | 'other'
    storage_key VARCHAR(500) NOT NULL,
    content_type VARCHAR(80) NOT NULL,
    size_bytes BIGINT NOT NULL,
    uploaded_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    uploaded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_kyc_docs_enterprise ON enterprise_kyc_documents (enterprise_id, uploaded_at DESC)
    WHERE deleted_at IS NULL;
