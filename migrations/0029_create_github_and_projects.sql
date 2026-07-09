-- Sprint 5 Phase 2 — GitHub OAuth + projects + curated OSS list.

-- GitHub OAuth connection per Skilluv user. Token encrypted at-rest.
CREATE TABLE github_connections (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    github_user_id BIGINT NOT NULL UNIQUE,
    github_login VARCHAR(80) NOT NULL,
    access_token_encrypted BYTEA NOT NULL,
    access_token_nonce BYTEA NOT NULL,
    scopes TEXT,
    last_synced_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_github_connections_login ON github_connections (github_login);

-- Public repos synced from GitHub. Minimal schema — we don't host code.
CREATE TABLE github_repos (
    id BIGINT PRIMARY KEY,  -- GitHub repo ID
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    full_name VARCHAR(140) NOT NULL,
    name VARCHAR(100) NOT NULL,
    description TEXT,
    html_url VARCHAR(500) NOT NULL,
    homepage VARCHAR(500),
    language VARCHAR(50),
    stargazers_count INTEGER NOT NULL DEFAULT 0,
    forks_count INTEGER NOT NULL DEFAULT 0,
    open_issues_count INTEGER NOT NULL DEFAULT 0,
    archived BOOLEAN NOT NULL DEFAULT FALSE,
    fork BOOLEAN NOT NULL DEFAULT FALSE,
    pushed_at TIMESTAMPTZ,
    created_at_github TIMESTAMPTZ,
    synced_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_github_repos_user ON github_repos (user_id, stargazers_count DESC);

-- Projects: Skilluv-native entities. Can belong to a user or to a guild.
-- Curated by admin flag = part of the "OSS-friendly" curated list shown to talents looking for contribs.
CREATE TABLE projects (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(80) NOT NULL UNIQUE,
    name VARCHAR(120) NOT NULL CHECK (length(name) BETWEEN 2 AND 120),
    description TEXT,
    repo_url VARCHAR(500),
    demo_url VARCHAR(500),
    tech_stack TEXT[] NOT NULL DEFAULT '{}',
    is_oss BOOLEAN NOT NULL DEFAULT TRUE,
    looking_for_contributors BOOLEAN NOT NULL DEFAULT FALSE,
    owner_type VARCHAR(20) NOT NULL CHECK (owner_type IN ('user', 'guild')),
    owner_id UUID NOT NULL,  -- references users(id) or guilds(id) ; enforced in app code
    curated_by_admin BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    archived_at TIMESTAMPTZ
);

CREATE INDEX idx_projects_owner ON projects (owner_type, owner_id, created_at DESC);
CREATE INDEX idx_projects_looking ON projects (looking_for_contributors) WHERE archived_at IS NULL AND looking_for_contributors = TRUE;
CREATE INDEX idx_projects_curated ON projects (curated_by_admin) WHERE archived_at IS NULL AND curated_by_admin = TRUE;

CREATE TABLE project_contributors (
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(20) NOT NULL DEFAULT 'contributor' CHECK (role IN ('maintainer', 'contributor')),
    commits_count INTEGER NOT NULL DEFAULT 0,
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (project_id, user_id)
);

CREATE INDEX idx_project_contributors_user ON project_contributors (user_id);
