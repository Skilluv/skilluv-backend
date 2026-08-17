-- The code onboarding answers.
--
-- Seven questions, asked once, kept. What they are for is the recommendation
-- that follows them — a beginner in web with TypeScript and five hours a week
-- needs a different first month from a senior systems programmer in Rust, and
-- with thirty-three trades the platform cannot guess.
--
-- ## Why the answers are columns and not a JSONB blob
--
-- Every one of them is read by a query somewhere: the level filters the
-- recommendations, the families pick the guide, the languages pick the feed,
-- the objective decides what is offered first. A blob would mean each of
-- those queries reaching into JSON, and no constraint on what is in there.
--
-- ## Why it is skippable and why that is recorded
--
-- Somebody who skips is not somebody who answered nothing: the first means
-- "stop asking", the second means "ask again". `code_onboarding_skipped_at`
-- is what separates them, and without it the wizard would reappear forever
-- for exactly the people who least wanted it.

ALTER TABLE users
    ADD COLUMN code_onboarding_completed_at TIMESTAMPTZ,
    ADD COLUMN code_onboarding_skipped_at TIMESTAMPTZ,
    ADD COLUMN code_level VARCHAR(20),
    -- Reviewer groups, the same eight families the guides and the reviewer
    -- capabilities use. Three at most: somebody who picks eight has picked
    -- none.
    ADD COLUMN code_preferred_families TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN code_weekly_hours VARCHAR(20),
    ADD COLUMN code_objective VARCHAR(40),
    ADD COLUMN code_main_languages TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN code_challenge_preference VARCHAR(40),

    ADD CONSTRAINT code_level_is_known CHECK (
        code_level IS NULL OR code_level IN (
            'beginner',    -- 0-1 year
            'junior',      -- 1-3
            'mid',         -- 3-7
            'senior',      -- 7-15
            'staff'        -- 15+
        )
    ),
    ADD CONSTRAINT code_weekly_hours_is_known CHECK (
        code_weekly_hours IS NULL OR code_weekly_hours IN (
            'under_5', '5_to_15', '15_to_40', 'fulltime'
        )
    ),
    ADD CONSTRAINT code_objective_is_known CHECK (
        code_objective IS NULL OR code_objective IN (
            'learn',
            'build_portfolio',
            'find_paid_work',
            'contribute_upstream',
            'publish_library',
            'become_mentor',
            'ship_own_product'
        )
    ),
    ADD CONSTRAINT code_challenge_preference_is_known CHECK (
        code_challenge_preference IS NULL OR code_challenge_preference IN (
            'upstream_contributions',
            'solo_shipped_apps',
            'published_libraries',
            'long_team_projects',
            'short_hackathons'
        )
    ),
    -- Three families and three languages. Not a style rule: the
    -- recommendation is only as good as the narrowing, and somebody who
    -- selects everything has told us nothing while believing they answered.
    ADD CONSTRAINT code_families_are_a_choice
        CHECK (cardinality(code_preferred_families) <= 3),
    ADD CONSTRAINT code_languages_are_a_choice
        CHECK (cardinality(code_main_languages) <= 3);

COMMENT ON COLUMN users.code_onboarding_skipped_at IS
    'Skipped is not the same as unanswered: the first means stop asking. '
    'Without this the wizard would reappear forever for the people who least '
    'wanted it.';

CREATE INDEX idx_users_code_families
    ON users USING GIN (code_preferred_families)
    WHERE cardinality(code_preferred_families) > 0;

CREATE INDEX idx_users_code_languages
    ON users USING GIN (code_main_languages)
    WHERE cardinality(code_main_languages) > 0;
