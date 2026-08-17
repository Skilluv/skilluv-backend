-- The seven domains, in the two places that still enumerated four.
--
-- ## What was wrong
--
-- Migration 0002 created `users.skill_domain` with the four domains that
-- existed in 2024 — code, design, game, security. Migration 0049 made it
-- nullable and rewrote the CHECK, keeping the same four. Since then 0056 and
-- 0088 gave `skill_nodes` and `orientations` seven, 0207 widened
-- `challenge_templates` to seven, and this branch seeds a design domain that
-- assumes them.
--
-- So the platform could grant somebody `challenge_validator:ai`, seed them
-- forty-one AI challenges and rank them on an AI craft score — and still
-- refuse `ai` when they tried to say that is what they do. The onboarding
-- gate in `require_completed_profile` reads this column, which means an AI
-- practitioner could not finish signing up at all.
--
-- 0207 saw this and deliberately left it, on the grounds that it carried a
-- product question. It does not: the question of which domains exist was
-- answered by 0056, and every other table has been following that answer for
-- a year. This is the last two tables catching up.
--
-- ## Why `sponsored_challenge_requests` goes with it
--
-- Same list, same origin (0033), same consequence at a different door: an
-- enterprise cannot sponsor an AI or an ops challenge. Leaving one of the two
-- narrow would recreate exactly the split this migration exists to end.
--
-- ## Where the list lives now
--
-- `crate::validators::SKILL_DOMAINS`, which is asserted against this file at
-- test time. There were six copies in Rust and three were stale; the code
-- side of this migration deletes five of them.

ALTER TABLE users DROP CONSTRAINT IF EXISTS users_skill_domain_check;

ALTER TABLE users
    ADD CONSTRAINT users_skill_domain_check
    CHECK (skill_domain IS NULL OR skill_domain IN (
        'code', 'design', 'game', 'security', 'ops', 'ai', 'soft_skills'
    ));

COMMENT ON COLUMN users.skill_domain IS
    'The domain this account primarily practises in. NULL until onboarding '
    'completes. Kept beside `user_orientations` rather than replaced by it: '
    'orientations say which trades somebody claims, this says which ladder '
    'they are ranked on, and a profile has many of the first and one of the '
    'second.';

ALTER TABLE sponsored_challenge_requests
    DROP CONSTRAINT IF EXISTS sponsored_challenge_requests_skill_domain_check;

ALTER TABLE sponsored_challenge_requests
    ADD CONSTRAINT sponsored_challenge_requests_skill_domain_check
    CHECK (skill_domain IN (
        'code', 'design', 'game', 'security', 'ops', 'ai', 'soft_skills'
    ));
