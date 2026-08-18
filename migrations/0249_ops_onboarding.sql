-- The ops onboarding answers.
--
-- Six questions, asked once, kept. Columns rather than a JSONB blob for the
-- reason migration 0201 gave: every one of them is read by a query. The level
-- and the on-call experience filter what is offered, the trades pick the
-- guide, the cloud experience picks the terrain.
--
-- ## The question no other domain asks
--
-- On-call. It is the part of this trade that decides whether somebody can do
-- it at all — a person with a night job, a shared living space or an
-- unreliable connection cannot hold a pager, and that has nothing to do with
-- their skill. Asking it up front means never offering an on-call mission to
-- somebody for whom it would be a trap, and never assuming that someone who
-- has never done it is not ready for the rest of the domain.
--
-- ## Skipping is recorded
--
-- Somebody who skips is not somebody who answered nothing: the first means
-- "stop asking", the second means "ask again". Without the distinction the
-- wizard reappears forever for exactly the people who least wanted it.

ALTER TABLE users
    ADD COLUMN ops_onboarding_completed_at TIMESTAMPTZ,
    ADD COLUMN ops_onboarding_skipped_at TIMESTAMPTZ,
    ADD COLUMN ops_level VARCHAR(20),
    -- Trade slugs, not reviewer groups: the ops guides are per trade, and
    -- somebody picking eight has picked none.
    ADD COLUMN ops_trades TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN ops_cloud_experience TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN ops_weekly_hours VARCHAR(20),
    ADD COLUMN ops_objective VARCHAR(40),
    ADD COLUMN ops_oncall_experience VARCHAR(20),

    ADD CONSTRAINT ops_level_is_known CHECK (
        ops_level IS NULL OR ops_level IN (
            'beginner',   -- has not run anything in production
            'junior',     -- operates what others built
            'engineer',   -- builds and holds
            'senior',     -- others operate what they built
            'principal'   -- changed how other teams work
        )
    ),
    ADD CONSTRAINT ops_weekly_hours_is_known CHECK (
        ops_weekly_hours IS NULL OR ops_weekly_hours IN (
            'under_3', '3_to_10', 'over_10', 'fulltime'
        )
    ),
    ADD CONSTRAINT ops_objective_is_known CHECK (
        ops_objective IS NULL OR ops_objective IN (
            'learn',
            'build_portfolio',
            'find_paid_work',
            'become_mentor',
            'start_own_practice'
        )
    ),
    ADD CONSTRAINT ops_oncall_experience_is_known CHECK (
        ops_oncall_experience IS NULL OR ops_oncall_experience IN (
            'never', 'occasional', 'regular', 'always_on'
        )
    ),
    -- Two at most. Somebody claiming five trades has claimed a domain, and
    -- the playlist that follows would be everything, which is nothing.
    ADD CONSTRAINT ops_trades_are_at_most_two CHECK (
        cardinality(ops_trades) <= 2
    );

COMMENT ON COLUMN users.ops_oncall_experience IS
    'Whether this person has held a pager, and how often. Asked because '
    'on-call availability is a life constraint rather than a skill: never '
    'offering an on-call mission to somebody for whom it would be a trap '
    'requires knowing.';

COMMENT ON COLUMN users.ops_onboarding_skipped_at IS
    'Set when somebody chose not to answer. Distinct from never having been '
    'asked, so the wizard stops reappearing for the people who declined it.';

CREATE INDEX idx_users_ops_onboarding_pending
    ON users (created_at)
    WHERE ops_onboarding_completed_at IS NULL
      AND ops_onboarding_skipped_at IS NULL;
