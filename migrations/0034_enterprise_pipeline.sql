-- Phase 3.5 — recruiter pipeline (kanban-style tracking + private notes).

CREATE TABLE enterprise_pipeline_entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    talent_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    stage VARCHAR(30) NOT NULL DEFAULT 'to_contact'
        CHECK (stage IN ('to_contact', 'contacted', 'interviewing', 'offer_sent', 'hired', 'rejected', 'dropped')),
    position INTEGER NOT NULL DEFAULT 0,  -- ordering within the stage column
    notes TEXT,  -- private to the enterprise, never visible to the talent
    salary_proposed_eur INTEGER,
    last_action_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (enterprise_id, talent_id)
);

CREATE INDEX idx_pipeline_enterprise_stage ON enterprise_pipeline_entries (enterprise_id, stage, position);
CREATE INDEX idx_pipeline_talent ON enterprise_pipeline_entries (talent_id);

-- History of stage transitions (audit trail for hires + funnel analysis)
CREATE TABLE enterprise_pipeline_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    entry_id UUID NOT NULL REFERENCES enterprise_pipeline_entries(id) ON DELETE CASCADE,
    from_stage VARCHAR(30),
    to_stage VARCHAR(30) NOT NULL,
    actor_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    note TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_pipeline_history_entry ON enterprise_pipeline_history (entry_id, created_at DESC);
