-- Red-team findings, in a shape that can be checked and disclosed.
--
-- ## Why a table and not a document
--
-- A safety finding written as prose cannot be compared, counted or refuted,
-- and — the part that matters most — cannot be held to a disclosure process.
-- Somebody publishing a working jailbreak the day they find it does harm no
-- matter how good the write-up is. Rows can carry a state machine; a markdown
-- file cannot.
--
-- ## What makes a finding admissible
--
-- A named target with its version, a success rate over a stated number of
-- attempts, and a proposed mitigation. The three are required by constraint
-- rather than by convention:
--
--   * a target without a version is untestable six months later, because the
--     provider has redeployed and nobody can tell what was tried;
--   * one screenshot is an anecdote — the review grid asks for a rate over N
--     attempts, and this is where that stops being advice;
--   * reporting without proposing leaves the whole problem with the reader.
--
-- ## Disclosure
--
-- Five states, and the transitions are what the policy actually is. Ninety
-- days is the default embargo, the same window the industry converged on; it
-- is a column and not a constant because a provider who fixes in a week
-- should not wait twelve, and one who asks for longer is sometimes right.
--
-- `withheld` exists because publishing everything is not always the
-- responsible choice, and a platform that has no state for that quietly
-- pushes people into publishing. It requires a written reason: withholding
-- with no stated ground is indistinguishable from burying a finding.

CREATE TABLE ai_safety_reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slice_id UUID NOT NULL REFERENCES project_slices(id) ON DELETE CASCADE,

    -- What was attacked, exactly. "GPT-4" is not a target; a model name with
    -- the version or snapshot date is.
    target_model VARCHAR(120) NOT NULL
        CHECK (length(btrim(target_model)) > 0),
    target_version VARCHAR(60) NOT NULL
        CHECK (length(btrim(target_version)) > 0),

    attack_type VARCHAR(40) NOT NULL
        CHECK (attack_type IN (
            'prompt_injection',      -- instructions smuggled in as data
            'jailbreak',             -- refusal training bypassed
            'data_extraction',       -- training data or system prompt recovered
            'tool_misuse',           -- an agent made to use its tools wrongly
            'bias',                  -- systematically different treatment
            'hallucination',         -- confident, checkable, false
            'adversarial_input',     -- perturbation the model is not robust to
            'other'
        )),

    -- How to obtain it again. Free text because the honest answer varies —
    -- a conversation, a file, a perturbation recipe — required because a
    -- finding nobody can reproduce is not a finding.
    reproduction_md TEXT NOT NULL
        CHECK (length(btrim(reproduction_md)) >= 40),
    -- What the model did. Kept verbatim: paraphrasing an unsafe output is how
    -- a finding stops being verifiable.
    observed_output TEXT NOT NULL
        CHECK (length(btrim(observed_output)) > 0),

    -- A rate, not an anecdote. Seven successes out of ten is a different
    -- claim from seven out of a thousand, and both get written up the same
    -- way when only the successes are recorded.
    attempts INTEGER NOT NULL CHECK (attempts > 0),
    successes INTEGER NOT NULL CHECK (successes >= 0),

    severity_tier VARCHAR(10) NOT NULL
        CHECK (severity_tier IN ('low', 'medium', 'high', 'critical')),
    -- Why that tier. A severity with no reasoning is a number somebody chose.
    severity_rationale_md TEXT NOT NULL
        CHECK (length(btrim(severity_rationale_md)) >= 20),

    mitigation_proposed_md TEXT NOT NULL
        CHECK (length(btrim(mitigation_proposed_md)) >= 20),

    disclosure_status VARCHAR(20) NOT NULL DEFAULT 'private'
        CHECK (disclosure_status IN (
            'private',          -- known to the author and the reviewers only
            'vendor_notified',  -- sent to whoever can fix it
            'embargoed',        -- notified, and a publication date agreed
            'published',        -- out
            'withheld'          -- deliberately not published, with a reason
        )),
    vendor_notified_at TIMESTAMPTZ,
    embargo_until TIMESTAMPTZ,
    published_at TIMESTAMPTZ,
    withheld_reason_md TEXT,

    -- Set when a reviewer ran the reproduction and saw the same thing.
    reproduced_at TIMESTAMPTZ,
    reproduced_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT ai_safety_successes_within_attempts
        CHECK (successes <= attempts),

    -- Every state past `private` rests on the vendor having been told. An
    -- embargo nobody was notified of is a delay with no counterpart.
    CONSTRAINT ai_safety_notified_before_embargo
        CHECK (
            disclosure_status NOT IN ('vendor_notified', 'embargoed', 'published')
            OR vendor_notified_at IS NOT NULL
        ),
    CONSTRAINT ai_safety_embargo_has_a_date
        CHECK (disclosure_status <> 'embargoed' OR embargo_until IS NOT NULL),
    CONSTRAINT ai_safety_published_has_a_date
        CHECK (disclosure_status <> 'published' OR published_at IS NOT NULL),
    CONSTRAINT ai_safety_withheld_says_why
        CHECK (
            disclosure_status <> 'withheld'
            OR (withheld_reason_md IS NOT NULL
                AND length(btrim(withheld_reason_md)) >= 20)
        ),

    -- Reproduction is an event with an author. Half of it says nothing.
    CONSTRAINT ai_safety_reproduction_is_complete
        CHECK (
            (reproduced_at IS NULL AND reproduced_by_user_id IS NULL)
            OR (reproduced_at IS NOT NULL AND reproduced_by_user_id IS NOT NULL)
        )
);

COMMENT ON TABLE ai_safety_reports IS
    'Red-team findings with the target, the reproduction, a success rate and '
    'a proposed mitigation — and the disclosure state, which is the part a '
    'markdown file could not carry.';

COMMENT ON COLUMN ai_safety_reports.attempts IS
    'Denominator of the success rate. Seven out of ten and seven out of a '
    'thousand read identically when only the successes are written down.';

COMMENT ON COLUMN ai_safety_reports.embargo_until IS
    'Agreed publication date. Ninety days from notification is the default '
    'the industry settled on; it is a column because a provider who fixes in '
    'a week should not wait twelve.';

CREATE INDEX idx_ai_safety_reports_slice
    ON ai_safety_reports (slice_id);

-- The queue an operator works from: what is notified and waiting, ordered by
-- when it comes out.
CREATE INDEX idx_ai_safety_reports_embargo
    ON ai_safety_reports (embargo_until)
    WHERE disclosure_status = 'embargoed';

CREATE INDEX idx_ai_safety_reports_severity
    ON ai_safety_reports (severity_tier, created_at DESC);

CREATE OR REPLACE FUNCTION touch_ai_safety_reports_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_ai_safety_reports_updated_at
    BEFORE UPDATE ON ai_safety_reports
    FOR EACH ROW EXECUTE FUNCTION touch_ai_safety_reports_updated_at();
