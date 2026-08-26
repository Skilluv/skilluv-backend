-- Capture-the-flag, defensive labs and machine walkthroughs, on the
-- catalogue table that already holds every domain's practice work.
--
-- ## Why these are challenges and not slices
--
-- Tickets C-01, B-01 and T-12 all asked for new `project_slices.slice_type`
-- values: `ctf`, `blue_lab`, `cyber_machine_walkthrough`. That is the wrong
-- table, and the reason is structural rather than stylistic.
--
-- A slice is claimed. `claimed_by_user_id`, `claim_expires_at` and
-- `validated_at` are singular columns, because a slice is one piece of work
-- that one person or one team does once — a pull request against an upstream
-- issue. A capture-the-flag challenge is the opposite: the same challenge is
-- meant to be solved by everybody, independently, for years. Putting one in
-- `project_slices` means either the first person to claim it locks out the
-- other four hundred, or the claim columns stop meaning anything for that
-- slice type, and both of those are worse than they sound: `services::slices`
-- reads those columns everywhere.
--
-- `challenge_templates` is the table for work many people do: it carries
-- difficulty, reward, duration and an attempt history per person in
-- `challenge_submissions`. It is also where every domain that has arrived
-- since has seeded its practice catalogue — ai in 0219, communication in 0512,
-- education in 0528. Security had exactly one row in it.
--
-- The security *slices* — a real audit of a real repository, a hunt on a live
-- target — stay in `project_slices`, which is what 0550 is about. The line
-- between the two tables is: was the answer planted, or is nobody sure there
-- is one.
--
-- ## Six kinds, in two families
--
-- The line between the families is who checks the answer, and it follows from
-- who owns the target.
--
-- **Machine-checked — this platform owns the secret:**
--
--   * `ctf_flag` — a range we host, a flag we planted, verified by comparing
--     hashes. No human in the loop.
--   * `defensive_lab` — an artefact we hold, with questions whose answers we
--     know, hashed. The blue-team half of the same idea.
--
-- **Human-checked — somebody else owns the target:**
--
--   * `training_ground` — Juice Shop, WebGoat, DVWA, the PortSwigger labs.
--   * `machine_walkthrough` — a retired HackTheBox machine or a VulnHub image.
--   * `analysis_exercise` — a public forensic dataset: a capture, a log set, a
--     memory image published by somebody else under a licence that says link,
--     not rehost.
--   * `audit_exercise` — a codebase or document set to audit.
--
-- The second family submits a write-up and a reviewer reads it, and that is
-- not a fallback: on a target this platform does not control there is no
-- secret it can hold, and any "flag" it invented would be either guessable
-- from the scheme or a value the author had to have solved the challenge to
-- know. Seeding a hash the author guessed produces a challenge nobody can ever
-- pass, and nothing fails: the challenge is simply unanswerable for ever. So
-- the rule is that the platform machine-checks what it owns and reads what it
-- does not.
--
-- ## Why the answers are hashed and never stored
--
-- A flag or a lab answer is the solution to a challenge hundreds of people
-- will attempt. A plaintext column would mean one database dump, one
-- misconfigured admin endpoint or one over-broad SELECT in a log ends the
-- challenge for everybody. Hashes cost nothing and remove the class.
--
-- The same argument applies to what people submit: `security_flag_attempts`
-- keeps the hash of the attempt, not the attempt. An attempt log in plaintext
-- is a wordlist of near-miss guesses, which is a hint, and hints leak.

-- ═══════════════════════════════════════════════════════════════════
-- What a security challenge carries
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE challenge_templates
    -- NULL for an ordinary security challenge — a written exercise, a
    -- reading. The column says "this one is verified in a particular way",
    -- not "this one is about security".
    ADD COLUMN security_kind VARCHAR(25)
        CHECK (security_kind IS NULL OR security_kind IN (
            'ctf_flag', 'defensive_lab', 'machine_walkthrough',
            'training_ground', 'analysis_exercise', 'audit_exercise'
        )),

    -- ── Capture the flag ────────────────────────────────────────────
    -- SHA-256 of the expected flag, hex. Case-sensitive comparison: flags are.
    ADD COLUMN security_flag_hash CHAR(64),
    -- What the flag looks like, so somebody who has solved the challenge
    -- knows what to paste. `SKILLUV{lower_snake_case}`, `JUICESHOP:<key>`.
    -- The format is not a secret; the value is.
    ADD COLUMN security_flag_format VARCHAR(80),
    -- Where to attack. A hosted instance, or the platform's own staging.
    ADD COLUMN security_target_url VARCHAR(500),

    -- ── Difficulty, in the vocabulary this trade uses ───────────────
    -- `difficulty` is 1..5 across the platform and stays authoritative for
    -- rewards and recommendations. This is the word a security person expects
    -- to see, and it is not derivable: an "easy" box and a difficulty-2
    -- documentation task are not the same claim about half an hour.
    ADD COLUMN security_difficulty_tier VARCHAR(10)
        CHECK (security_difficulty_tier IS NULL OR security_difficulty_tier IN (
            'easy', 'medium', 'hard', 'insane'
        )),

    -- ── Defensive lab ───────────────────────────────────────────────
    -- Key in the artefact bucket, not a URL: the download endpoint signs it
    -- per request, so a link cannot be shared into a group chat and still work
    -- next week.
    ADD COLUMN security_lab_artifact_key VARCHAR(500),
    -- Said before the download starts. A five-hundred-megabyte memory image
    -- on a metered connection is a decision, not a click.
    ADD COLUMN security_lab_artifact_bytes BIGINT
        CHECK (security_lab_artifact_bytes IS NULL OR security_lab_artifact_bytes > 0),
    -- [{"id","kind","question","expected_answer_hash","choices","hint",
    --   "case_sensitive"}]. `kind` is 'text' or 'choice'. The hint is shown
    -- only for a question that was answered wrongly, which is what makes a
    -- failed attempt teach something.
    ADD COLUMN security_lab_questions JSONB
        CHECK (security_lab_questions IS NULL OR (
            jsonb_typeof(security_lab_questions) = 'array'
            AND jsonb_array_length(security_lab_questions) > 0
        )),
    -- How much of it has to be right. Per challenge rather than global: a
    -- five-question lab and a twenty-question one do not have the same pass
    -- mark, and one number in the code would have been wrong for both.
    ADD COLUMN security_lab_pass_percent SMALLINT
        CHECK (security_lab_pass_percent IS NULL
               OR security_lab_pass_percent BETWEEN 50 AND 100),
    -- Attempts before a cooling-off period. Three, because a lab with
    -- unlimited attempts is a multiple-choice quiz you brute-force.
    ADD COLUMN security_lab_max_attempts SMALLINT
        CHECK (security_lab_max_attempts IS NULL
               OR security_lab_max_attempts BETWEEN 1 AND 10),

    -- ── Somebody else's target ──────────────────────────────────────
    -- Where the machine or the application actually lives. This platform
    -- curates and reviews; it does not rehost, and the licences of the
    -- material are the reason.
    ADD COLUMN security_external_source VARCHAR(30)
        CHECK (security_external_source IS NULL OR security_external_source IN (
            'hackthebox_retired', 'vulnhub', 'owasp_project',
            'tryhackme', 'own_instance', 'public_dataset'
        )),
    ADD COLUMN security_external_url VARCHAR(500),
    -- The reference walkthrough, written or vetted by a senior here, that a
    -- submission is read against. Published after a submission is reviewed,
    -- never before.
    ADD COLUMN security_official_walkthrough_url VARCHAR(500),

    -- ── The write-up ────────────────────────────────────────────────
    -- TRUE when a captured flag or a set of answers is not enough on its own.
    ADD COLUMN security_writeup_required BOOLEAN NOT NULL DEFAULT FALSE,
    -- Whose material this is, and under what licence. Required for anything
    -- built on a third party's dataset or application: B-04 lists sources
    -- under CC-BY, and attribution that lives in a migration comment is
    -- attribution nobody sees.
    ADD COLUMN security_attribution_md TEXT;

-- Only the security domain uses any of it.
ALTER TABLE challenge_templates
    ADD CONSTRAINT security_kind_belongs_to_security CHECK (
        security_kind IS NULL OR skill_domain = 'security'
    ),
    -- Each kind carries what it needs to be attemptable at all. A flag
    -- challenge with no hash cannot be solved and cannot fail either: it is
    -- published, claimable and permanently unanswerable.
    ADD CONSTRAINT a_flag_challenge_has_a_flag CHECK (
        security_kind <> 'ctf_flag'
        OR (security_flag_hash IS NOT NULL
            AND security_flag_format IS NOT NULL
            AND security_target_url IS NOT NULL)
    ),
    ADD CONSTRAINT a_lab_has_an_artefact_and_questions CHECK (
        security_kind <> 'defensive_lab'
        OR (security_lab_artifact_key IS NOT NULL
            AND security_lab_questions IS NOT NULL
            AND security_lab_pass_percent IS NOT NULL
            AND security_lab_max_attempts IS NOT NULL)
    ),
    -- The human-checked family names the target it does not own, and says
    -- out loud that a write-up is what is being submitted. Without the second
    -- half a challenge would be published with no verification at all.
    ADD CONSTRAINT an_external_target_says_where CHECK (
        security_kind NOT IN ('machine_walkthrough', 'training_ground',
                              'analysis_exercise')
        OR (security_external_source IS NOT NULL
            AND security_external_url IS NOT NULL
            AND security_writeup_required)
    ),
    ADD CONSTRAINT an_audit_exercise_is_read_by_somebody CHECK (
        security_kind <> 'audit_exercise' OR security_writeup_required
    ),
    -- The flag machinery and the lab machinery are mutually exclusive: a
    -- challenge graded two ways has two answers to whether it was passed.
    ADD CONSTRAINT a_challenge_is_graded_one_way CHECK (
        security_flag_hash IS NULL OR security_lab_questions IS NULL
    ),
    -- A difficulty tier is a security vocabulary. Nothing else may set it.
    ADD CONSTRAINT a_difficulty_tier_belongs_to_security CHECK (
        security_difficulty_tier IS NULL OR skill_domain = 'security'
    );

COMMENT ON COLUMN challenge_templates.security_flag_hash IS
    'SHA-256 of the expected flag. Never the flag: one dump or one wide SELECT '
    'in a log would end the challenge for everybody who has not solved it yet.';

COMMENT ON COLUMN challenge_templates.security_lab_questions IS
    'Questions with hashed expected answers and per-question hints. The hint '
    'is returned only for a question that was got wrong, which is what makes a '
    'failed attempt worth something.';

COMMENT ON COLUMN challenge_templates.security_difficulty_tier IS
    'easy/medium/hard/insane, the words this trade actually uses. Does not '
    'replace `difficulty` 1..5, which still drives rewards — an "easy" box and '
    'a difficulty-2 writing task are not the same claim.';

COMMENT ON COLUMN challenge_templates.security_external_url IS
    'Where the machine or application lives. This platform curates and '
    'reviews; it does not rehost somebody else''s licensed material.';

CREATE INDEX idx_challenge_templates_security_kind
    ON challenge_templates (security_kind, security_difficulty_tier)
    WHERE security_kind IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- Flag attempts
-- ═══════════════════════════════════════════════════════════════════
--
-- Every submission, right or wrong, hashed. Three jobs:
--
--   * the hourly cap that stops a flag being brute-forced;
--   * the audit trail behind "this account submitted four hundred guesses";
--   * first blood, which is `min(attempted_at) WHERE correct` and has to be
--     decided on a row that cannot be back-dated.
--
-- Successful attempts also produce a `challenge_submissions` row with status
-- `success`, which is what the catalogue, the attempt count and the
-- leaderboard read. Two rows for one event, on purpose: this table is
-- high-volume telemetry that can be pruned, and that one is the record of the
-- solve that never can.

CREATE TABLE security_flag_attempts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    challenge_id UUID NOT NULL REFERENCES challenge_templates(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- SHA-256 of what was submitted. Not the submission: a log of near-miss
    -- guesses is a hint, and hints leak.
    submitted_hash CHAR(64) NOT NULL,
    correct BOOLEAN NOT NULL,
    -- Which attempt this was for this person on this challenge. Read by the
    -- rate limit and by the profile, so it is counted rather than derived on
    -- every listing.
    attempt_no INTEGER NOT NULL CHECK (attempt_no >= 1),
    attempted_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE security_flag_attempts IS
    'Every flag submission, hashed. Carries the brute-force cap, the audit '
    'trail, and the first-solve timestamp — which has to sit on a row nobody '
    'can back-date.';

CREATE INDEX idx_security_flag_attempts_cap
    ON security_flag_attempts (challenge_id, user_id, attempted_at DESC);
-- First blood, and the solve count per challenge.
CREATE INDEX idx_security_flag_attempts_solves
    ON security_flag_attempts (challenge_id, attempted_at)
    WHERE correct;
-- One correct attempt per person per challenge. A second would double-count a
-- solve on every scoreboard that reads this table.
CREATE UNIQUE INDEX uniq_security_flag_solve
    ON security_flag_attempts (challenge_id, user_id)
    WHERE correct;

-- ═══════════════════════════════════════════════════════════════════
-- What a graded lab attempt leaves behind
-- ═══════════════════════════════════════════════════════════════════
--
-- The attempt itself is a `challenge_submissions` row, which already carries
-- `attempt_number`, `status` and `fragments_earned`. What it has nowhere to
-- put is the grade: which questions were right, which were wrong, and the
-- percentage — which is what the response has to contain for a retry to be
-- worth anything.
--
-- The answers themselves are deliberately not stored. They are the solution
-- to a lab hundreds of people will attempt, and a table of submitted answers
-- is that solution with a confidence score attached.

ALTER TABLE challenge_submissions
    ADD COLUMN security_grade JSONB
        CHECK (security_grade IS NULL OR jsonb_typeof(security_grade) = 'object');

COMMENT ON COLUMN challenge_submissions.security_grade IS
    'The grade of a defensive lab attempt: {"correct":[...],"wrong":[...],'
    '"score_percent":n}. The answers are not stored — a table of submitted '
    'answers is the solution with a confidence score attached.';
