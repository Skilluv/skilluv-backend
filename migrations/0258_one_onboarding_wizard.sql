-- Two onboarding wizards, one of them writing to columns on `users`.
--
-- ## What was there
--
-- `user_domain_profiles` stores wizard answers as an object keyed by
-- `(user_id, domain)`, with a completed and a skipped timestamp. Design and AI
-- use it.
--
-- Code does not. The code wizard writes eight columns on `users`:
-- `code_level`, `code_preferred_families`, `code_weekly_hours`,
-- `code_objective`, `code_main_languages`, `code_challenge_preference`,
-- `code_onboarding_completed_at`, `code_onboarding_skipped_at`.
--
-- Seven domains at eight columns each is fifty-six columns on the users table,
-- a migration for every question reworded, and — already true today — two
-- pieces of code that read "what did this person say in the wizard" from two
-- different places and disagree about the answer.
--
-- ## What this does
--
-- Moves the code answers into `user_domain_profiles` and drops the columns.
-- The answers keep their own vocabulary: a code level is `beginner..staff` and
-- a design level is `debutant..researcher`, and flattening them into one list
-- would mean inventing a word for a rank neither wizard asks about.
--
-- `code_main_languages` becomes `main_tools`. It is the same question — what
-- do you work in — and the design wizard already calls it that.

-- ═══════════════════════════════════════════════════════════════════
-- Two timestamps the generic table never had
-- ═══════════════════════════════════════════════════════════════════

-- The code wizard could tell "answered", "skipped" and "not yet" apart. The
-- generic one could not: it had `answers` and nothing else, so a person who
-- declined and a person who had not got round to it looked identical, and the
-- prompt would come back forever for the ones who least wanted it.
--
-- `updated_at` is not a substitute. It moves when anything is written, and it
-- says nothing about a skip, which writes no answer at all.
ALTER TABLE user_domain_profiles
    ADD COLUMN IF NOT EXISTS completed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS skipped_at TIMESTAMPTZ;

COMMENT ON COLUMN user_domain_profiles.completed_at IS
    'When the wizard was last answered. Null with a `skipped_at` means '
    'declined; null with neither means a row written by something other than '
    'the wizard.';

-- ═══════════════════════════════════════════════════════════════════
-- Move what is there
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO user_domain_profiles (user_id, domain, answers, completed_at, skipped_at)
SELECT u.id,
       'code',
       -- `strip_nulls` so an unanswered question is absent rather than
       -- present-and-null. A reader checking `answers ? 'level'` must get the
       -- same answer as one checking whether it is null.
       jsonb_strip_nulls(jsonb_build_object(
           'level', u.code_level,
           'weekly_hours', u.code_weekly_hours,
           -- `goal` here, `code_objective` there: the generic wizard already
           -- calls this question `goal`, and two names for one question is
           -- how a reader ends up checking the wrong key.
           'goal', u.code_objective,
           'challenge_preference', u.code_challenge_preference,
           'preferred_families', to_jsonb(u.code_preferred_families),
           'main_tools', to_jsonb(u.code_main_languages)
       )),
       u.code_onboarding_completed_at,
       u.code_onboarding_skipped_at
  FROM users u
 WHERE u.code_onboarding_completed_at IS NOT NULL
    OR u.code_onboarding_skipped_at IS NOT NULL
-- Somebody who has already filled in the new wizard keeps that answer: it is
-- the more recent statement of the same thing.
ON CONFLICT (user_id, domain) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Then take the old shape away
-- ═══════════════════════════════════════════════════════════════════

-- Dropped rather than deprecated. A column left behind is a column something
-- keeps writing to, and the whole point of the move is that there is one
-- place to read.
ALTER TABLE users
    DROP COLUMN IF EXISTS code_level,
    DROP COLUMN IF EXISTS code_preferred_families,
    DROP COLUMN IF EXISTS code_weekly_hours,
    DROP COLUMN IF EXISTS code_objective,
    DROP COLUMN IF EXISTS code_main_languages,
    DROP COLUMN IF EXISTS code_challenge_preference,
    DROP COLUMN IF EXISTS code_onboarding_completed_at,
    DROP COLUMN IF EXISTS code_onboarding_skipped_at;

COMMENT ON TABLE user_domain_profiles IS
    'What somebody said in the onboarding wizard, per domain. One row per '
    '(person, domain); the vocabulary of each answer belongs to its domain '
    'and lives in `routes::domain_profile`, not in a CHECK — rewording a '
    'question must not be a migration. Nothing here is a claim about '
    'anybody: rank, badges and craft score read proofs, and a declared level '
    'is not one.';

-- ═══════════════════════════════════════════════════════════════════
-- Reading one answer out of the object
-- ═══════════════════════════════════════════════════════════════════

-- A list answer, as a text array, or NULL when the key is absent or holds
-- something that is not a list.
--
-- A function rather than the same four lines of `jsonb_array_elements_text`
-- in every query that reads the wizard. The queries that had it inline
-- disagreed about what an absent key meant, which is exactly the kind of
-- disagreement that produces an empty recommendation nobody can explain.
CREATE OR REPLACE FUNCTION wizard_list(answers JSONB, key TEXT)
RETURNS TEXT[]
LANGUAGE sql
IMMUTABLE
PARALLEL SAFE
AS $$
    SELECT CASE
        WHEN jsonb_typeof(answers -> key) = 'array'
            THEN ARRAY(SELECT jsonb_array_elements_text(answers -> key))
        ELSE NULL
    END
$$;

COMMENT ON FUNCTION wizard_list(JSONB, TEXT) IS
    'One list answer out of `user_domain_profiles.answers`. NULL when the key '
    'is absent or holds something that is not a list — callers COALESCE to an '
    'empty array, which is the answer "none in particular".';
