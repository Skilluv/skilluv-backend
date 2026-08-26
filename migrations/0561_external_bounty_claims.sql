-- A bounty somebody earned on another platform, claimed here and checked.
--
-- ## Why this is not `external_credentials`
--
-- That table holds certifications: an issuer, a level, an expiry, a page a
-- reviewer opens. A disclosed bounty report has none of those and one thing
-- they do not — a severity, which is the whole point of it. Filing bounties
-- there would put "holds an OSCP" and "was paid for a finding on HackerOne" in
-- one list under one heading, and the profile that reads it says
-- "certifications".
--
-- ## Why this is not `security_findings`
--
-- Because this platform did not see it. A finding here was reported here,
-- reproduced here and has an embargo this platform is keeping; a bounty
-- elsewhere is a link to somebody else's published report. Putting them in one
-- table would mean every count of "findings confirmed" either included work
-- nobody here verified, or needed a filter that somebody would forget.
--
-- The craft score reflects that: an external bounty is worth 40 and a finding
-- confirmed here is worth 60, and the reason is that one of them we checked.
--
-- ## What "verified" means here, and what it cannot mean
--
-- A reviewer opens the public disclosure and checks three things: that it
-- exists, that it names this person, and that its severity is what was claimed.
-- That is all anybody can check from outside, and it is stated rather than
-- implied — the platform is not claiming to have reproduced somebody else's
-- vulnerability on somebody else's system.
--
-- A claim whose report is not public cannot be verified at all, and is refused
-- rather than left pending: most bounty reports are never disclosed, and a
-- queue full of unverifiable claims is a queue nobody works.

CREATE TABLE external_bounty_claims (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- The curated programme, where the claim is against one this platform
    -- lists. Null for a programme nobody has curated yet, which is most of
    -- them: refusing the claim would be refusing the work.
    program_id UUID REFERENCES external_bounty_programs(id) ON DELETE SET NULL,
    platform VARCHAR(20) NOT NULL
        CHECK (platform IN ('hackerone', 'bugcrowd', 'intigriti',
                            'yeswehack', 'self_hosted')),
    organisation_name VARCHAR(160) NOT NULL,

    -- The public disclosure. Required, and the reason this table can exist at
    -- all: without it there is nothing a reviewer could look at.
    report_url VARCHAR(500) NOT NULL CHECK (report_url ~ '^https://'),
    -- What the platform there rated it. Their scale, not ours, which is why it
    -- is recorded as claimed and can be adjusted by whoever checks.
    claimed_severity VARCHAR(15) NOT NULL
        CHECK (claimed_severity IN ('critical', 'high', 'medium', 'low',
                                    'informational')),
    severity VARCHAR(15)
        CHECK (severity IS NULL OR severity IN ('critical', 'high', 'medium',
                                                'low', 'informational')),
    cwe_id VARCHAR(15) CHECK (cwe_id IS NULL OR cwe_id ~ '^CWE-[0-9]{1,5}$'),
    -- Two sentences from the person: what it was. Not the report, which lives
    -- at the URL.
    summary_md TEXT NOT NULL CHECK (length(btrim(summary_md)) >= 40),
    disclosed_on DATE,

    -- Review. Three states and no more: waiting, accepted, refused.
    verified_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    verified_at TIMESTAMPTZ,
    refused_at TIMESTAMPTZ,
    refused_reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One claim per person per report. Two would double-count one finding.
    UNIQUE (user_id, report_url),
    CONSTRAINT a_claim_is_not_both_accepted_and_refused CHECK (
        verified_at IS NULL OR refused_at IS NULL
    ),
    CONSTRAINT a_verification_names_its_reviewer CHECK (
        (verified_at IS NULL) = (verified_by_user_id IS NULL)
    ),
    CONSTRAINT a_refusal_says_why CHECK (
        (refused_at IS NULL) = (refused_reason IS NULL)
    ),
    -- An accepted claim carries the severity the reviewer settled on, which
    -- may not be the one that was claimed.
    CONSTRAINT an_accepted_claim_has_a_settled_severity CHECK (
        verified_at IS NULL OR severity IS NOT NULL
    )
);

COMMENT ON TABLE external_bounty_claims IS
    'A bounty report disclosed on another platform, claimed here and checked '
    'against its public page. Kept apart from security_findings because this '
    'platform did not reproduce it, and from external_credentials because it is '
    'not a certification.';

COMMENT ON COLUMN external_bounty_claims.severity IS
    'What the reviewer settled on, which need not be what the other platform '
    'rated it. Their scales differ from ours and from each other.';

CREATE INDEX idx_external_bounty_claims_user
    ON external_bounty_claims (user_id, created_at DESC);
-- The review queue reads exactly this.
CREATE INDEX idx_external_bounty_claims_pending
    ON external_bounty_claims (created_at)
    WHERE verified_at IS NULL AND refused_at IS NULL;

CREATE TRIGGER trg_external_bounty_claims_updated_at
    BEFORE UPDATE ON external_bounty_claims
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();
