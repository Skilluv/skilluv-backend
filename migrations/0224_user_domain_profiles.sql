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
-- ## An open disagreement, stated rather than hidden
--
-- Migration 0201 made the opposite choice for the code domain: eight
-- `users.code_*` columns, on the grounds that every answer is read by a query
-- and a blob would mean each of those queries reaching into JSON with no
-- constraint on what it finds.
--
-- That argument is real and it is answered — a GIN index queries JSONB, and
-- the vocabulary is enforced in the handler — but it is not answered *here*,
-- because settling it means rewriting the code recommendation and mentorship
-- services while that branch is still moving. Two shapes coexist for now and
-- this comment is the record of why.
--
-- Whoever resolves it: the cost of keeping both is one more shape to learn.
-- The cost of the columns is eight per domain on `users`, forever, and a
-- recommender with a branch per domain deciding which column to read.
--
-- ## What is deliberately not in here
--
-- The trades somebody claims. `user_orientations` has held those since 0089,
-- with a cap and a history, and a second copy in a JSONB blob is how the two
-- start disagreeing. The wizard writes there.
--
-- ## Declared, not proven
--
-- Everything here is self-reported, and it is used to recommend rather than
-- to credit: nothing in this table reaches a rank, a badge or a craft score.
-- Someone writing "researcher" gets shown harder challenges, not a title.

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
