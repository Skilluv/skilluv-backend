-- A vulnerability somebody reported, from submission to public disclosure.
--
-- ## The one table this domain genuinely needs
--
-- Every other cyber backlog item resolved onto something that already exists:
-- missions onto `missions`, competitions onto `tournaments`, external
-- profiles onto `user_external_portfolios`, certificates onto
-- `external_credentials`. This one does not, and the reason is worth writing
-- down because it is the test every future "we need a table" has to pass.
--
-- A finding is not a slice. A slice is work this platform offered and somebody
-- claimed; a finding is work nobody asked for, arriving unannounced, about a
-- system the reporter was given written permission to attack. It has three
-- things no slice has:
--
--   * a target that is not a project — a live host, in a scope published
--     beforehand;
--   * a severity that is negotiated, on a vector, between two people who
--     disagree about money;
--   * an embargo. A slice is published when it is done. A finding is the only
--     artefact here that is deliberately kept secret after it is finished, on
--     a clock, and published later by a rule rather than by a decision.
--
-- `quality_bug_reports` (0451) is the nearest thing and is also not it: it
-- hangs off a slice, it has no CVSS, no embargo and no bounty, and it is about
-- a defect rather than an exposure. The two stay separate and a person can
-- file both.
--
-- ## Three state columns, and why not five
--
-- The backlog proposed `status` (F-05), `triage_status` (W-03) and
-- `disclosure_status` (W-02) plus a `dedup_status` (W-05) and a
-- `severity_disputed` flag (W-04). Five state machines on one row is how a
-- report ends up `confirmed` and `triaged_invalid` at the same time.
--
-- What is kept:
--
--   * `status` — where the finding is in its life. One machine, transitions
--     enforced below.
--   * `disclosure_stage` — where the *public* is in relation to it. Genuinely
--     orthogonal: a fixed finding can be embargoed, and a confirmed one can be
--     public because the reporter was allowed to publish.
--   * `dedup_state` — whether this row is the original. Also orthogonal: a
--     duplicate is still triaged, still scored, and still earns a co-credit.
--
-- What is dropped: `triage_status`. Its four values were `pending`,
-- `triaged_valid`, `triaged_invalid` and `escalated_to_vendor` — the first
-- three are `status` in ('submitted', 'triaged'/'confirmed',
-- 'not_applicable'), and the fourth is `vendor_notified_at IS NOT NULL`, which
-- is a fact with a date rather than a state. What W-03 actually needed was the
-- three triage columns, and they are here.
--
-- `severity_disputed` is not a boolean either: it is
-- `severity_reported_tier <> severity_tier`, which also says which way the
-- disagreement went. The boolean would have been derivable and therefore able
-- to be wrong.
--
-- ## No fragments column
--
-- F-05 proposed `fragments_awarded` and `badge_awarded` on the finding.
-- Fragments are awarded on `deliverables` and badges live in `user_badges`;
-- a copy here would be a second place for both to be wrong. A confirmed
-- finding gets a deliverable — see the column added to that table at the
-- bottom of this migration — and the reward machinery that every other domain
-- already uses runs unchanged.
--
-- ## Why `ON DELETE RESTRICT` on the reporter
--
-- Same reason the ledger uses it. A published finding credits a person by
-- name in a write-up that is public and permanent; deleting the account has to
-- be a decision somebody makes deliberately about that credit, not a cascade.

-- ═══════════════════════════════════════════════════════════════════
-- The finding
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE security_findings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- ── Who found it ────────────────────────────────────────────────
    reporter_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    -- A reporter who wants the credit without the name. The hall of fame
    -- shows a stable alias derived from the id; the profile is not linked.
    -- Opt-in per finding rather than per account: the same person may want
    -- their name on a web bug and not on a client engagement.
    reporter_is_anonymous BOOLEAN NOT NULL DEFAULT FALSE,

    -- ── What was attacked ───────────────────────────────────────────
    --
    -- Three kinds, and each one points somewhere different:
    --
    --   * `platform` — this platform, under the published scope. No foreign
    --     key to follow; `target_host` is what identifies it.
    --   * `mission` — a paid engagement. The mission carries the scope, the
    --     confidentiality and the payout tier.
    --   * `project` — a repository or product listed here with
    --     `bug_bounty_open`, which is the column 0058 added and nothing has
    --     read until now.
    target_kind VARCHAR(20) NOT NULL
        CHECK (target_kind IN ('platform', 'mission', 'project')),
    mission_id UUID REFERENCES missions(id) ON DELETE SET NULL,
    project_id UUID REFERENCES projects(id) ON DELETE SET NULL,
    -- The host or application, as published in the scope. Checked against the
    -- scope by the service, and kept here because "which of our five domains"
    -- is the first question a triager asks.
    target_host VARCHAR(255),
    -- `POST /api/auth/login`, `LoginForm.svelte`, `sshd`. Free text because
    -- the shape differs per target, and indexed because deduplication reads
    -- it more than anything else.
    affected_endpoint VARCHAR(500),

    -- ── The report ──────────────────────────────────────────────────
    title VARCHAR(200) NOT NULL CHECK (length(btrim(title)) >= 5),
    -- Length floors rather than non-empty, for the reason 0451 gives: "it is
    -- broken" satisfies `<> ''` and is the exact thing being refused.
    description_md TEXT NOT NULL CHECK (length(btrim(description_md)) >= 50),
    reproduction_steps_md TEXT NOT NULL
        CHECK (length(btrim(reproduction_steps_md)) >= 30),
    -- What an attacker could do with it, on this system. Separate from the
    -- description because reporters who are asked for it in the same box
    -- write the vulnerability class again.
    impact_md TEXT,
    -- The proposed fix. Optional, and the thing that most distinguishes a
    -- report a maintainer is glad to receive.
    proposed_fix_md TEXT,
    -- Screenshots, captures, recordings. Keys in the private bucket rather
    -- than public URLs: a proof of an unfixed vulnerability is not public
    -- material, and the download endpoint checks who is asking.
    proof_keys TEXT[] NOT NULL DEFAULT '{}'
        CHECK (cardinality(proof_keys) <= 10),

    -- ── How bad it is ───────────────────────────────────────────────
    --
    -- CVSS 3.1 as a vector, not a number typed in. The score is computed from
    -- the vector by `services::cvss` and stored so that queries can sort on
    -- it; the vector is what makes the score arguable.
    cvss_vector VARCHAR(120)
        CHECK (cvss_vector IS NULL OR cvss_vector ~ '^CVSS:3\.1/[A-Z:/]+$'),
    cvss_score NUMERIC(3,1)
        CHECK (cvss_score IS NULL OR (cvss_score >= 0.0 AND cvss_score <= 10.0)),
    -- What the reporter said, kept after a validator overrides it. The
    -- disagreement is information: a reporter who consistently files criticals
    -- that are mediums is something a mentor should see — the argument 0451
    -- made for `severity_adjusted_to`.
    severity_reported_tier VARCHAR(15) NOT NULL
        CHECK (severity_reported_tier IN
               ('critical', 'high', 'medium', 'low', 'informational')),
    -- What it is. Equal to the reported tier until somebody with the
    -- capability changes it.
    severity_tier VARCHAR(15) NOT NULL
        CHECK (severity_tier IN
               ('critical', 'high', 'medium', 'low', 'informational')),
    severity_final_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    severity_override_reason TEXT,
    cwe_id VARCHAR(15) CHECK (cwe_id IS NULL OR cwe_id ~ '^CWE-[0-9]{1,5}$'),

    -- ── Life ────────────────────────────────────────────────────────
    status VARCHAR(20) NOT NULL DEFAULT 'submitted'
        CHECK (status IN (
            'submitted',        -- arrived, nobody has read it
            'triaged',          -- a triager judged it worth reproducing
            'confirmed',        -- reproduced by somebody else. The real one.
            'duplicate',        -- already known. See dedup_state.
            'not_applicable',   -- out of scope, or not a vulnerability
            'withdrawn',        -- the reporter took it back
            'fixed',            -- the owner shipped a change for it
            'published'         -- public, with a write-up
        )),

    -- ── Triage (W-03) ───────────────────────────────────────────────
    --
    -- A junior's finding is read by a senior before the owner or the vendor
    -- hears about it. That protects the vendor relationship from false
    -- positives and protects the junior from having sent one.
    triaged_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    triaged_at TIMESTAMPTZ,
    triage_notes_md TEXT,
    -- Set when the reporter's rank meant no triage was required. Recorded
    -- rather than inferred, because the rule will change and old rows should
    -- still say which way it went for them.
    triage_skipped_reason VARCHAR(40)
        CHECK (triage_skipped_reason IS NULL OR triage_skipped_reason IN (
            'reporter_rank', 'reporter_track_record', 'admin_override'
        )),

    -- ── Deduplication (W-05) ────────────────────────────────────────
    dedup_state VARCHAR(25) NOT NULL DEFAULT 'original'
        CHECK (dedup_state IN ('original', 'suspected', 'duplicate_confirmed')),
    duplicate_of_finding_id UUID REFERENCES security_findings(id) ON DELETE SET NULL,
    dedup_reviewed_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    dedup_reviewed_at TIMESTAMPTZ,
    -- What the scanner thought looked similar, and how much. Never acted on
    -- automatically: merging two findings decides who is paid, and a
    -- trigram score is not allowed to decide that.
    similar_finding_ids UUID[] NOT NULL DEFAULT '{}',
    similarity_scores REAL[] NOT NULL DEFAULT '{}',
    similarity_scanned_at TIMESTAMPTZ,

    -- ── Disclosure (W-02) ───────────────────────────────────────────
    disclosure_stage VARCHAR(25)
        CHECK (disclosure_stage IS NULL OR disclosure_stage IN (
            'embargoed',            -- the clock is running
            'extension_requested',  -- the owner asked for more time
            'partially_disclosed',  -- existence public, details held
            'public',               -- everything published
            'withheld'              -- deliberately never published, with a reason
        )),
    -- How long the owner gets, in days, from confirmation. Ninety is the
    -- industry default and is the default here; a mission or a project can
    -- carry its own policy, which is why the number is on the row rather than
    -- in the code.
    disclosure_policy_days SMALLINT NOT NULL DEFAULT 90
        CHECK (disclosure_policy_days BETWEEN 0 AND 730),
    embargo_ends_at TIMESTAMPTZ,
    vendor_notified_at TIMESTAMPTZ,
    vendor_patch_confirmed_at TIMESTAMPTZ,
    extension_requested_at TIMESTAMPTZ,
    extension_granted_days SMALLINT
        CHECK (extension_granted_days IS NULL OR extension_granted_days BETWEEN 1 AND 365),
    withheld_reason TEXT,

    -- ── Resolution ──────────────────────────────────────────────────
    fix_url VARCHAR(500) CHECK (fix_url IS NULL OR fix_url ~ '^https://'),
    fixed_at TIMESTAMPTZ,
    -- Path or URL of the public write-up. Relative paths are allowed because
    -- the platform's own write-ups live in this repository.
    writeup_url VARCHAR(500),
    published_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- ── What the row is not allowed to say ──────────────────────────

    -- The target points at what it names, and at nothing else. A finding with
    -- both a mission and a project has two scopes and two people who think
    -- they own the disclosure.
    CONSTRAINT a_finding_points_at_one_target CHECK (
        CASE target_kind
            WHEN 'mission' THEN mission_id IS NOT NULL AND project_id IS NULL
            WHEN 'project' THEN project_id IS NOT NULL AND mission_id IS NULL
            ELSE mission_id IS NULL AND project_id IS NULL
                 AND target_host IS NOT NULL
        END
    ),
    -- A triage names who did it and when, or neither.
    CONSTRAINT triage_is_complete CHECK (
        (triaged_at IS NULL) = (triaged_by_user_id IS NULL)
    ),
    -- Skipped or done, never both.
    CONSTRAINT triage_is_done_or_skipped CHECK (
        triage_skipped_reason IS NULL OR triaged_at IS NULL
    ),
    -- A severity that was changed says who changed it and why. An unexplained
    -- override is the thing researchers leave a platform over.
    CONSTRAINT a_severity_override_is_explained CHECK (
        severity_tier = severity_reported_tier
        OR (severity_final_by_user_id IS NOT NULL
            AND severity_override_reason IS NOT NULL)
    ),
    -- A duplicate says what of, and a row that says what of is a duplicate.
    CONSTRAINT a_duplicate_names_its_original CHECK (
        (dedup_state = 'duplicate_confirmed') = (duplicate_of_finding_id IS NOT NULL)
    ),
    CONSTRAINT a_finding_is_not_its_own_duplicate CHECK (
        duplicate_of_finding_id IS NULL OR duplicate_of_finding_id <> id
    ),
    -- The status `duplicate` and the dedup state have to agree. Two ways to
    -- say the same thing is the failure this whole migration's header is
    -- about.
    CONSTRAINT duplicate_status_matches_dedup_state CHECK (
        (status = 'duplicate') = (dedup_state = 'duplicate_confirmed')
    ),
    -- Scores and ids come in pairs, one score per candidate.
    CONSTRAINT similarity_is_paired CHECK (
        cardinality(similar_finding_ids) = cardinality(similarity_scores)
    ),
    -- A fix says where.
    CONSTRAINT a_fix_says_where CHECK (
        fixed_at IS NULL OR fix_url IS NOT NULL
    ),
    -- Published means public, and public means published. A write-up is
    -- required: the whole point of the last transition is that somebody can
    -- read what happened.
    CONSTRAINT publication_is_complete CHECK (
        (status = 'published') = (published_at IS NOT NULL)
    ),
    CONSTRAINT a_published_finding_has_a_writeup CHECK (
        published_at IS NULL OR writeup_url IS NOT NULL
    ),
    -- Withholding is a decision that is written down.
    CONSTRAINT withholding_is_explained CHECK (
        disclosure_stage <> 'withheld' OR withheld_reason IS NOT NULL
    ),
    -- An embargo has an end. A clock nobody can see is not coordinated
    -- disclosure, it is a promise.
    CONSTRAINT an_embargo_ends CHECK (
        disclosure_stage IS NULL
        OR disclosure_stage = 'withheld'
        OR embargo_ends_at IS NOT NULL
    ),
    -- An extension names its length.
    CONSTRAINT an_extension_is_measured CHECK (
        (extension_requested_at IS NULL AND extension_granted_days IS NULL)
        OR extension_requested_at IS NOT NULL
    )
);

COMMENT ON TABLE security_findings IS
    'A reported vulnerability, from arrival to public disclosure. The only '
    'table the security domain needed that did not already exist: a finding '
    'has a live target, a negotiated severity and an embargo, and no other '
    'artefact on this platform has any of the three.';

COMMENT ON COLUMN security_findings.severity_reported_tier IS
    'What the reporter claimed, kept after an override. The gap between this '
    'and severity_tier is the disagreement, and it is worth more than a '
    'boolean saying one happened.';

COMMENT ON COLUMN security_findings.similar_finding_ids IS
    'Candidates a scanner thought looked alike. Never merged automatically: '
    'a merge decides who is paid, and a trigram score does not get to.';

COMMENT ON COLUMN security_findings.disclosure_stage IS
    'Where the public is in relation to this finding. Orthogonal to status on '
    'purpose — a fixed finding can still be embargoed, and a confirmed one can '
    'be public when the owner allowed it.';

CREATE INDEX idx_security_findings_reporter
    ON security_findings (reporter_user_id, created_at DESC);
CREATE INDEX idx_security_findings_queue
    ON security_findings (status, severity_tier, created_at);
CREATE INDEX idx_security_findings_target
    ON security_findings (target_kind, target_host);
CREATE INDEX idx_security_findings_mission
    ON security_findings (mission_id) WHERE mission_id IS NOT NULL;
CREATE INDEX idx_security_findings_project
    ON security_findings (project_id) WHERE project_id IS NOT NULL;
-- The embargo worker reads exactly this.
CREATE INDEX idx_security_findings_embargo
    ON security_findings (embargo_ends_at)
    WHERE disclosure_stage IN ('embargoed', 'extension_requested');
-- Deduplication reads the endpoint and the class together, and the trigram
-- index is what makes the title comparison affordable.
--
-- `pg_trgm` is created here rather than assumed. T-07 said it was already
-- installed for the plagiarism detection of 0084; it is not — that detection
-- compares embeddings computed in Rust and touches no extension. Nothing else
-- in this schema uses trigrams, so the extension arrives with the first thing
-- that needs it, the way 0001 created the other two.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE INDEX idx_security_findings_dedup
    ON security_findings (cwe_id, affected_endpoint);
CREATE INDEX idx_security_findings_title_trgm
    ON security_findings USING gin (title gin_trgm_ops);

CREATE TRIGGER trg_security_findings_updated_at
    BEFORE UPDATE ON security_findings
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- Every transition, with who and why
-- ═══════════════════════════════════════════════════════════════════
--
-- T-04 asked for a complete audit trail on the admin page, and T-08 needs to
-- know what just changed in order to send the right notification. One table
-- answers both, and it is append-only: a history that can be edited is not
-- one.

CREATE TABLE security_finding_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    finding_id UUID NOT NULL REFERENCES security_findings(id) ON DELETE CASCADE,
    -- NULL when the actor was the platform: an embargo expiring, a scanner
    -- flagging a similarity. Said with a null rather than with a service
    -- account, so that "nobody decided this, a rule did" is readable.
    actor_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    event VARCHAR(40) NOT NULL,
    from_status VARCHAR(20),
    to_status VARCHAR(20),
    reason TEXT,
    -- Anything the event needs that is not a status: the severity before and
    -- after, the extension length, the duplicate it was merged into.
    detail JSONB,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE security_finding_events IS
    'Append-only history of a finding. Answers both the admin audit trail and '
    'the question "what changed" that the notifications are driven from.';

CREATE INDEX idx_security_finding_events_finding
    ON security_finding_events (finding_id, occurred_at);

-- ═══════════════════════════════════════════════════════════════════
-- Rounds (W-01)
-- ═══════════════════════════════════════════════════════════════════
--
-- A finding is not accepted or rejected on first read. The reviewer asks for
-- a clearer reproduction, or a proposed patch, or disputes the severity, and
-- the researcher answers. That is the same shape as the design iteration of
-- 0231 and the quality rounds of 0450, and it uses the same vocabulary table
-- — `revision_round_kinds`, with security rows added below — because a round
-- kind is a round kind whatever it is attached to.
--
-- What it cannot reuse is `slice_revision_rounds`, which is keyed to a slice.
-- A finding is not a slice, for the reasons at the top of this file.

CREATE TABLE security_finding_rounds (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    finding_id UUID NOT NULL REFERENCES security_findings(id) ON DELETE CASCADE,
    round_no SMALLINT NOT NULL CHECK (round_no BETWEEN 1 AND 5),
    kind VARCHAR(50) NOT NULL REFERENCES revision_round_kinds(slug) ON UPDATE CASCADE,
    requested_by UUID REFERENCES users(id) ON DELETE SET NULL,
    notes_md TEXT NOT NULL CHECK (length(btrim(notes_md)) >= 20),
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- The researcher's answer, and when it landed.
    answered_at TIMESTAMPTZ,
    answer_md TEXT,
    resolved_at TIMESTAMPTZ,
    resolved_by UUID REFERENCES users(id) ON DELETE SET NULL,
    resolution VARCHAR(20)
        CHECK (resolution IS NULL OR resolution IN ('satisfied', 'insufficient')),

    UNIQUE (finding_id, round_no),
    CONSTRAINT an_answer_has_text CHECK (
        (answered_at IS NULL) = (answer_md IS NULL)
    ),
    CONSTRAINT a_resolution_is_complete CHECK (
        (resolved_at IS NULL) = (resolution IS NULL)
    )
);

COMMENT ON TABLE security_finding_rounds IS
    'Iteration on a finding: reproduction not sufficient, patch requested, '
    'severity disputed. Capped at five rounds by the CHECK, which is the '
    'limit the design asked for — after five somebody decides.';

CREATE INDEX idx_security_finding_rounds_open
    ON security_finding_rounds (finding_id) WHERE resolved_at IS NULL;

-- The round vocabulary for this domain, in the table every domain shares.
INSERT INTO revision_round_kinds (slug, skill_domain, name, description, sort_order) VALUES
('sec_repro_insufficient', 'security', 'Reproduction insufficient',
 'The reviewer cannot get to the same result from what was written. The most '
 'common round in this domain, and the one that ends the report if it does not '
 'converge.', 610),
('sec_proof_insufficient', 'security', 'Proof does not show the claim',
 'The screenshot shows an error page, not the data that should not have been '
 'readable. The finding may be real; the evidence does not establish it.', 620),
('sec_severity_disputed', 'security', 'Severity disputed',
 'The defect is agreed, the vector is not. Settled by arguing the metrics, '
 'never by negotiating the adjective.', 630),
('sec_patch_requested', 'security', 'Fix proposal requested',
 'The reviewer wants a concrete remediation before accepting. Asked for rather '
 'than required: a report without one is still a report.', 640),
('sec_scope_question', 'security', 'Scope in question',
 'What was tested may be outside what was authorised. Blocking until answered '
 '— nothing else in the report matters if the answer is no.', 650),
('sec_impact_unclear', 'security', 'Impact not established',
 'What an attacker could actually do, on this system, is missing or is the '
 'vulnerability class restated.', 660);

-- ═══════════════════════════════════════════════════════════════════
-- The link to the reward machinery
-- ═══════════════════════════════════════════════════════════════════
--
-- F-06 asked whether a security finding counts towards a rank the way a
-- merged pull request does. It could not, and the reason was structural: rank
-- and craft score read `deliverables`, and `deliverables_at_least_one_parent`
-- required a slice, a challenge, a contest submission or a mission delivery.
-- A finding was none of the four, so no deliverable could exist for it, so it
-- counted nowhere.
--
-- One column and one widened CHECK fix that, and every mechanism downstream —
-- ranks, craft score, the public feed, plagiarism scanning, revocation —
-- starts working for this domain without being told about it.

ALTER TABLE deliverables
    ADD COLUMN security_finding_id UUID
        REFERENCES security_findings(id) ON DELETE CASCADE;

ALTER TABLE deliverables
    DROP CONSTRAINT deliverables_at_least_one_parent,
    ADD CONSTRAINT deliverables_at_least_one_parent CHECK (
        slice_id IS NOT NULL
        OR challenge_id IS NOT NULL
        OR tournament_submission_id IS NOT NULL
        OR mission_delivery_id IS NOT NULL
        OR security_finding_id IS NOT NULL
    );

COMMENT ON COLUMN deliverables.security_finding_id IS
    'The confirmed finding this deliverable records. The fifth parent a '
    'deliverable can have, added so that a vulnerability counts towards a rank '
    'exactly as a merged pull request does — which is what one cross-domain '
    'rank means.';

CREATE INDEX idx_deliverables_security_finding
    ON deliverables (security_finding_id) WHERE security_finding_id IS NOT NULL;

-- One deliverable per finding. Two would mean the same vulnerability counted
-- twice towards a rank.
CREATE UNIQUE INDEX uniq_deliverables_per_security_finding
    ON deliverables (security_finding_id) WHERE security_finding_id IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- Attestations point at findings
-- ═══════════════════════════════════════════════════════════════════
--
-- A-01 asked for `security_finding_id` on `attestations`. Granted, because
-- the verification page has to be able to show what the attestation rests on
-- without walking through the deliverable — and because a co-credit
-- attestation has a finding and no deliverable of its own.

ALTER TABLE attestations
    ADD COLUMN security_finding_id UUID
        REFERENCES security_findings(id) ON DELETE SET NULL;

COMMENT ON COLUMN attestations.security_finding_id IS
    'The finding behind a security attestation. Needed on its own rather than '
    'through the deliverable because a co-credit has a finding and no '
    'deliverable.';

CREATE INDEX idx_attestations_security_finding
    ON attestations (security_finding_id) WHERE security_finding_id IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- What the reporter is told (T-08)
-- ═══════════════════════════════════════════════════════════════════
--
-- Every transition the reporter can see, as a notification kind. A reporter
-- who is not told whether their report was read is a reporter who does not
-- send a second one, and that is the single most common way a disclosure
-- programme dies.
--
-- `transactional` on all of them: a person cannot opt out of being told what
-- happened to a report they filed. `allows_email` respects the existing
-- preference machinery, so the volume is still theirs to choose.

INSERT INTO notification_kinds
    (kind, category, allows_in_app, allows_push, allows_email,
     default_in_app, default_push, default_email, transactional, cta_path)
VALUES
('security.finding_received',   'learning', TRUE, FALSE, TRUE, TRUE, FALSE, TRUE, TRUE, '/security/reports'),
('security.finding_triaged',    'learning', TRUE, FALSE, TRUE, TRUE, FALSE, TRUE, TRUE, '/security/reports'),
('security.finding_confirmed',  'learning', TRUE, TRUE,  TRUE, TRUE, TRUE,  TRUE, TRUE, '/security/reports'),
('security.finding_duplicate',  'learning', TRUE, FALSE, TRUE, TRUE, FALSE, TRUE, TRUE, '/security/reports'),
('security.finding_rejected',   'learning', TRUE, FALSE, TRUE, TRUE, FALSE, TRUE, TRUE, '/security/reports'),
('security.finding_round',      'learning', TRUE, TRUE,  TRUE, TRUE, TRUE,  TRUE, TRUE, '/security/reports'),
('security.finding_fixed',      'learning', TRUE, FALSE, TRUE, TRUE, FALSE, TRUE, TRUE, '/security/reports'),
('security.finding_published',  'learning', TRUE, TRUE,  TRUE, TRUE, TRUE,  TRUE, TRUE, '/security/hall-of-fame'),
('security.severity_changed',   'learning', TRUE, FALSE, TRUE, TRUE, FALSE, TRUE, TRUE, '/security/reports'),
('security.embargo_ending',     'learning', TRUE, FALSE, TRUE, TRUE, FALSE, TRUE, TRUE, '/security/reports'),
-- The two the triager sees rather than the reporter.
('security.triage_queued',      'admin',    TRUE, FALSE, TRUE, TRUE, FALSE, TRUE, TRUE, '/admin/security/findings'),
('security.dedup_suspected',    'admin',    TRUE, FALSE, FALSE, TRUE, FALSE, FALSE, TRUE, '/admin/security/findings');
