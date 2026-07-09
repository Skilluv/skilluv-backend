-- Enterprise bookmarks (simple talent saves)
CREATE TABLE enterprise_bookmarks (
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    talent_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (enterprise_id, talent_id)
);

CREATE INDEX idx_enterprise_bookmarks_enterprise ON enterprise_bookmarks (enterprise_id, created_at DESC);

-- Named talent lists
CREATE TABLE talent_lists (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    name VARCHAR(200) NOT NULL,
    description TEXT,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_talent_lists_enterprise ON talent_lists (enterprise_id);

-- Members of a named talent list
CREATE TABLE talent_list_members (
    list_id UUID NOT NULL REFERENCES talent_lists(id) ON DELETE CASCADE,
    talent_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    added_by UUID NOT NULL REFERENCES users(id),
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (list_id, talent_id)
);

CREATE INDEX idx_talent_list_members_list ON talent_list_members (list_id);
