-- What somebody tells us about themselves, per domain.
--
-- ## Why not `users.ai_*`
--
-- The backlog asked for AI columns on `users`. Six domains asking the same
-- favour is thirty columns on the one table every query in the codebase
-- touches, each NULL for everybody outside its domain, and each needing a
-- migration to add a possible answer.
--
-- One row per person per domain instead. The generic part — the table — is
-- written once; the domain-specific part is the answers, and those are
-- validated in the handler where the vocabulary lives, not in a CHECK that
-- would need a migration every time an option is reworded.
--
-- ## The disagreement that used to be here
--
-- Migration 0201 made the opposite choice for the code domain: eight
-- `users.code_*` columns. Both shapes coexisted for a while and this comment
-- recorded why. Migration 0235 settles it — the code answers moved here and
-- the columns are gone — and states the reasoning.

CREATE TABLE user_domain_profiles (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    domain VARCHAR(30) NOT NULL
        CHECK (domain IN (
            'code', 'design', 'game', 'security', 'soft_skills', 'ai', 'ops'
        )),
    -- An object, always. A JSONB column accepting a bare string or a list
    -- would let one caller store `"pytorch"` where every reader expects
    -- `{"main_framework": "pytorch"}`.
    answers JSONB NOT NULL DEFAULT '{}'::JSONB
        CHECK (jsonb_typeof(answers) = 'object'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, domain)
);

COMMENT ON TABLE user_domain_profiles IS
    'Self-reported answers per domain — level, available compute, hours a '
    'week. Used to recommend, never to credit: nothing here reaches a rank, '
    'a badge or a score.';

COMMENT ON COLUMN user_domain_profiles.answers IS
    'The domain wizard''s answers. Validated in the handler rather than by a '
    'CHECK, so rewording an option is a deployment and not a migration.';

CREATE INDEX idx_user_domain_profiles_domain
    ON user_domain_profiles (domain);

CREATE OR REPLACE FUNCTION touch_user_domain_profiles_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_user_domain_profiles_updated_at
    BEFORE UPDATE ON user_domain_profiles
    FOR EACH ROW EXECUTE FUNCTION touch_user_domain_profiles_updated_at();
