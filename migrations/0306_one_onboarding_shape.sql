-- One shape for the onboarding answers, and it is the table.
--
-- ## The disagreement, settled
--
-- Migration 0201 put the code answers in eight `users.code_*` columns.
-- Migration 0224 put the AI ones in a table keyed by domain, and said the
-- disagreement was open because settling it meant rewriting the code
-- recommendation and mentorship services. That is what this does.
--
-- 0201's argument was real: every answer is read by a query somewhere, and a
-- blob means those queries reaching into JSON with no constraint on what they
-- find. Here is why it loses anyway.
--
--   * **Seven domains.** Eight columns each is fifty-six on `users`, the one
--     table every query in the codebase already touches, each NULL for
--     everybody outside its domain.
--   * **Every option is a migration.** Adding "staff" to the levels, or a
--     framework to the AI list, means altering a CHECK on `users`.
--   * **The recommender needs a branch per domain** to know which column to
--     read, which is the shape the orientation refactor spent 0173 removing.
--
-- And the constraint argument is one this codebase has already answered twice,
-- in its own words. `tournaments.rules` (0189): "Validated in the service
-- rather than here: the required keys differ per kind, and a CHECK spanning
-- five shapes is unreadable and unchangeable." `missions` carries the same
-- note. Onboarding answers are that exact shape — same question, seven
-- vocabularies — and the answer does not change because the table is `users`.
--
-- ## What stays a column
--
-- `completed_at` and `skipped_at`. They are not answers, they are the state of
-- the wizard, they are identical in every domain, and 0201 is right that
-- skipping and not answering are different things: without the distinction the
-- wizard reappears forever for exactly the people who least wanted it. A JSONB
-- key would have made that difference depend on a spelling.
--
-- ## Nothing is lost
--
-- The copy across is exact and happens before the columns go. A row already
-- created by the AI wizard keeps its answers: the update merges rather than
-- replaces, so somebody who did both wizards ends with both.

ALTER TABLE user_domain_profiles
    ADD COLUMN completed_at TIMESTAMPTZ,
    ADD COLUMN skipped_at TIMESTAMPTZ;

COMMENT ON COLUMN user_domain_profiles.completed_at IS
    'When the wizard was answered. A column rather than a JSONB key because '
    'every domain has it and a query reads it to decide whether to ask again.';

COMMENT ON COLUMN user_domain_profiles.skipped_at IS
    'When somebody said stop asking. Different from having answered nothing: '
    'the first means "stop", the second means "ask again".';

-- Read by the recommenders: "everybody in this family", "everybody who works
-- in Rust". Without it those become a sequential scan of the table.
CREATE INDEX idx_user_domain_profiles_answers
    ON user_domain_profiles USING gin (answers jsonb_path_ops);

-- ═══════════════════════════════════════════════════════════════════
-- The code answers move
-- ═══════════════════════════════════════════════════════════════════
--
-- Only the keys that were actually answered. A NULL column becomes an absent
-- key, not a null one: a key present with a null value and an absent key read
-- the same to a recommender, and one of them is a lie about having asked.

INSERT INTO user_domain_profiles (user_id, domain, answers, completed_at, skipped_at)
SELECT u.id, 'code',
       COALESCE(jsonb_object_agg(kv.key, kv.value)
                FILTER (WHERE kv.value IS NOT NULL), '{}'::JSONB),
       u.code_onboarding_completed_at,
       u.code_onboarding_skipped_at
  FROM users u
  LEFT JOIN LATERAL (
        VALUES
          ('level',                 to_jsonb(u.code_level)),
          ('weekly_hours',          to_jsonb(u.code_weekly_hours)),
          ('objective',             to_jsonb(u.code_objective)),
          ('challenge_preference',  to_jsonb(u.code_challenge_preference)),
          ('preferred_families',
           CASE WHEN cardinality(u.code_preferred_families) > 0
                THEN to_jsonb(u.code_preferred_families) END),
          ('main_languages',
           CASE WHEN cardinality(u.code_main_languages) > 0
                THEN to_jsonb(u.code_main_languages) END)
       ) AS kv(key, value) ON TRUE
 WHERE u.code_onboarding_completed_at IS NOT NULL
    OR u.code_onboarding_skipped_at IS NOT NULL
 GROUP BY u.id, u.code_onboarding_completed_at, u.code_onboarding_skipped_at
ON CONFLICT (user_id, domain) DO UPDATE
    SET answers      = user_domain_profiles.answers || EXCLUDED.answers,
        completed_at = COALESCE(EXCLUDED.completed_at, user_domain_profiles.completed_at),
        skipped_at   = COALESCE(EXCLUDED.skipped_at, user_domain_profiles.skipped_at);

ALTER TABLE users
    DROP COLUMN code_onboarding_completed_at,
    DROP COLUMN code_onboarding_skipped_at,
    DROP COLUMN code_level,
    DROP COLUMN code_preferred_families,
    DROP COLUMN code_weekly_hours,
    DROP COLUMN code_objective,
    DROP COLUMN code_main_languages,
    DROP COLUMN code_challenge_preference;
