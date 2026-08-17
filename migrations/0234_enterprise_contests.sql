-- Business model — contests a company pays for.
-- Migration 0234.
--
-- ## Six shapes, one table
--
-- The backlog names six: a recruiting contest, an award-scale challenge, a
-- product-led hackathon, a corporate internal hackathon, a migration contest,
-- and an innovation sprint. Written as six tables they would have been six
-- status machines, six submission lists and six places to get the shortlist
-- wrong.
--
-- They differ in one thing only: what happens to the winner.
--
--   * recruiting   — an interview, and possibly a hire;
--   * award        — a prize out of a pool;
--   * product_led  — their work goes into the company's stack;
--   * corporate    — outside people work alongside the company's own;
--   * migration    — a proof of concept, then the real migration.
--
-- The last three end in an engagement, which already exists as a table. So
-- the outcome is a foreign key, not a fourth copy of the same columns.
--
-- ## The innovation sprint is not here
--
-- Five days, a fixed team composition, a facilitator and a fixed fee is not a
-- contest — nobody competes. It is `team_engagements` with `kind = 'sprint'`,
-- which already bounds the duration and carries a project lead. A separate
-- table would have been a second sprint with its own status machine, and the
-- first question anybody asked would be which one to book.
--
-- ## `competitions` does not exist
--
-- Two tickets ask to extend `competitions.format`. The table is called
-- `tournaments`, and it is the community ladder: seasons, guild wars,
-- divisions, prize pools in fragments. Adding an enterprise recruiting
-- contest to it would put a company's hiring funnel on the same leaderboard
-- as a guild war. They are different objects and stay apart.

-- ═══════════════════════════════════════════════════════════════════
-- The contests
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE enterprise_contests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(80) NOT NULL UNIQUE,
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,

    kind VARCHAR(20) NOT NULL CHECK (kind IN (
        -- The prize is an interview. Different from a prize in money, and
        -- the difference is stated to the entrants before they enter.
        'recruiting',
        -- A grand challenge with a real prize pool and a campaign around it.
        'award',
        -- The winning work goes into the company's product, and the team is
        -- paid to finish it.
        'product_led',
        -- The company's own employees, with outside people mixed in.
        'corporate_internal',
        -- A proof of concept for a migration, then the migration.
        'migration'
    )),

    title VARCHAR(200) NOT NULL CHECK (btrim(title) <> ''),
    brief_md TEXT NOT NULL CHECK (btrim(brief_md) <> ''),
    orientation_target VARCHAR(80),
    domain_target VARCHAR(30),
    difficulty_tier SMALLINT CHECK (difficulty_tier IS NULL OR difficulty_tier BETWEEN 1 AND 5),

    visibility VARCHAR(30) NOT NULL DEFAULT 'public' CHECK (visibility IN (
        'public',
        -- Only the people invited can see it or enter. Used when a company
        -- is hiring quietly, which is a real need and not a loophole.
        'invitation_only'
    )),

    opens_at TIMESTAMPTZ,
    submissions_deadline TIMESTAMPTZ NOT NULL,
    -- How many go through. Named a shortlist rather than a winner count
    -- because for a recruiting contest everybody on it gets an interview.
    shortlist_size SMALLINT NOT NULL DEFAULT 3
        CHECK (shortlist_size BETWEEN 1 AND 20),

    -- ── What it costs, by kind ─────────────────────────────────────
    --
    -- Nullable and constrained per kind rather than split across five
    -- tables. A recruiting contest has no prize pool; an award has no
    -- per-candidate contact fee. The constraints below say which is which.

    -- Recruiting.
    mode VARCHAR(20) CHECK (mode IS NULL OR mode IN ('self_serve', 'managed')),
    setup_fee NUMERIC(10,2) CHECK (setup_fee IS NULL OR setup_fee >= 0),
    per_candidate_contact_fee NUMERIC(8,2)
        CHECK (per_candidate_contact_fee IS NULL OR per_candidate_contact_fee >= 0),
    managed_campaign_fee NUMERIC(10,2)
        CHECK (managed_campaign_fee IS NULL OR managed_campaign_fee >= 0),
    success_fee_percent NUMERIC(5,2)
        CHECK (success_fee_percent IS NULL
               OR (success_fee_percent > 0 AND success_fee_percent <= 30)),

    -- Award.
    prize_first NUMERIC(12,2) CHECK (prize_first IS NULL OR prize_first > 0),
    prize_pool_total NUMERIC(12,2)
        CHECK (prize_pool_total IS NULL OR prize_pool_total > 0),
    marketing_budget NUMERIC(12,2)
        CHECK (marketing_budget IS NULL OR marketing_budget >= 0),
    jury_composition JSONB NOT NULL DEFAULT '[]'::jsonb,

    -- Corporate internal.
    internal_employees_count SMALLINT
        CHECK (internal_employees_count IS NULL OR internal_employees_count > 0),
    external_talents_count SMALLINT
        CHECK (external_talents_count IS NULL OR external_talents_count > 0),
    external_talents_specialization TEXT[] NOT NULL DEFAULT '{}',
    per_external_talent_fee NUMERIC(8,2)
        CHECK (per_external_talent_fee IS NULL OR per_external_talent_fee > 0),

    -- Migration.
    current_stack_md TEXT,
    target_stack_md TEXT,

    -- What Skilluv charges to run it, whatever the kind. Every contest has
    -- one, and it is the line the revenue ledger reads.
    orchestration_fee NUMERIC(12,2) NOT NULL DEFAULT 0
        CHECK (orchestration_fee >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    -- ── What it turned into ────────────────────────────────────────
    --
    -- One pointer instead of a table per outcome. A product-led hackathon, a
    -- migration contest and a corporate hackathon all end in paid work, and
    -- the work is an engagement.
    outcome_engagement_id UUID REFERENCES team_engagements(id) ON DELETE SET NULL,

    status VARCHAR(30) NOT NULL DEFAULT 'draft' CHECK (status IN (
        'draft',
        'published',
        'submissions_open',
        'judging',
        'shortlist_ready',
        -- Recruiting only: the shortlist is talking to the company.
        'interviews_ongoing',
        'concluded',
        'cancelled'
    )),
    cancelled_reason TEXT,
    concluded_at TIMESTAMPTZ,

    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_window_runs_forward CHECK (
        opens_at IS NULL OR submissions_deadline > opens_at
    ),
    CONSTRAINT cancellation_carries_a_reason CHECK (
        status <> 'cancelled'
        OR (cancelled_reason IS NOT NULL AND btrim(cancelled_reason) <> '')
    ),

    -- A recruiting contest has to say which way it is run: self-serve, where
    -- the company does its own sourcing and pays per contact, or managed,
    -- where Skilluv runs it for a fee. The two are billed differently and a
    -- contest that says neither cannot be invoiced at all.
    CONSTRAINT a_recruiting_contest_says_how_it_is_run CHECK (
        kind <> 'recruiting' OR mode IS NOT NULL
    ),
    -- The prize for a recruiting contest is an interview. Attaching a cash
    -- prize would make it a different product wearing the same name, and the
    -- entrants would have been told the wrong thing.
    CONSTRAINT a_recruiting_contest_pays_in_interviews CHECK (
        kind <> 'recruiting' OR (prize_first IS NULL AND prize_pool_total IS NULL)
    ),
    -- An award challenge without a prize is a call for free work at scale.
    CONSTRAINT an_award_has_a_prize CHECK (
        kind <> 'award'
        OR (prize_first IS NOT NULL AND prize_pool_total IS NOT NULL
            AND prize_pool_total >= prize_first)
    ),
    CONSTRAINT a_migration_contest_names_both_stacks CHECK (
        kind <> 'migration'
        OR (current_stack_md IS NOT NULL AND btrim(current_stack_md) <> ''
            AND target_stack_md IS NOT NULL AND btrim(target_stack_md) <> '')
    ),
    -- The point of a corporate hackathon is the mix. Zero outsiders is the
    -- company running its own event and not needing us.
    CONSTRAINT a_corporate_hackathon_mixes_people CHECK (
        kind <> 'corporate_internal'
        OR (internal_employees_count IS NOT NULL AND external_talents_count IS NOT NULL
            AND per_external_talent_fee IS NOT NULL)
    )
);

COMMENT ON TABLE enterprise_contests IS
    'A contest a company pays for. Six shapes in the backlog, one table: '
    'they differ only in what happens to the winner, and for three of them '
    'that is an engagement, which already has a table.';

COMMENT ON COLUMN enterprise_contests.outcome_engagement_id IS
    'What it turned into. A product-led hackathon, a migration contest and a '
    'corporate hackathon all end in paid work, and paid work is an '
    'engagement.';

COMMENT ON CONSTRAINT a_recruiting_contest_pays_in_interviews ON enterprise_contests IS
    'The prize is an interview, and the entrants were told so. A cash prize '
    'bolted on would make it a different product wearing the same name.';

CREATE INDEX idx_contests_open
    ON enterprise_contests (submissions_deadline)
    WHERE status = 'submissions_open' AND visibility = 'public';
CREATE INDEX idx_contests_enterprise
    ON enterprise_contests (enterprise_id, created_at DESC);
CREATE INDEX idx_contests_kind ON enterprise_contests (kind, status);

CREATE OR REPLACE FUNCTION touch_contests_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_contests_updated_at
    BEFORE UPDATE ON enterprise_contests
    FOR EACH ROW EXECUTE FUNCTION touch_contests_updated_at();

-- ── Invitations ────────────────────────────────────────────────────

CREATE TABLE contest_invitations (
    contest_id UUID NOT NULL REFERENCES enterprise_contests(id) ON DELETE CASCADE,
    talent_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    invited_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Their own answer, as everywhere else a person's name is involved.
    accepted_at TIMESTAMPTZ,
    declined_at TIMESTAMPTZ,

    PRIMARY KEY (contest_id, talent_user_id),

    CONSTRAINT not_both_answers CHECK (accepted_at IS NULL OR declined_at IS NULL)
);

CREATE INDEX idx_contest_invitations_user
    ON contest_invitations (talent_user_id, invited_at DESC);

-- ── Submissions ────────────────────────────────────────────────────

CREATE TABLE contest_submissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    contest_id UUID NOT NULL REFERENCES enterprise_contests(id) ON DELETE CASCADE,
    talent_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    deliverable_url VARCHAR(500) NOT NULL CHECK (deliverable_url ~ '^https://'),
    notes_md TEXT,
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Judging. The rank is set for everyone judged, not only the shortlist:
    -- an entrant who came fourth out of forty is owed that number, and
    -- keeping it only for the top three makes the other thirty-seven
    -- indistinguishable from people who did not enter.
    final_rank INTEGER CHECK (final_rank IS NULL OR final_rank > 0),
    shortlisted BOOLEAN NOT NULL DEFAULT FALSE,
    judge_notes TEXT,
    judged_at TIMESTAMPTZ,

    -- Recruiting outcomes.
    interview_completed BOOLEAN NOT NULL DEFAULT FALSE,
    hired BOOLEAN NOT NULL DEFAULT FALSE,

    UNIQUE (contest_id, talent_user_id),
    -- One rank per contest. Two people ranked second is a judging error, and
    -- silently allowing it makes the shortlist arbitrary.
    UNIQUE (contest_id, final_rank),

    CONSTRAINT a_shortlist_is_judged CHECK (NOT shortlisted OR judged_at IS NOT NULL),
    CONSTRAINT only_the_shortlist_is_interviewed CHECK (
        NOT interview_completed OR shortlisted
    ),
    CONSTRAINT nobody_is_hired_without_an_interview CHECK (
        NOT hired OR interview_completed
    )
);

COMMENT ON CONSTRAINT nobody_is_hired_without_an_interview ON contest_submissions IS
    'The prize was an interview. A hire recorded without one means the '
    'interview happened off the platform, and the success fee would rest on '
    'nothing anybody can point at.';

CREATE INDEX idx_contest_submissions_contest
    ON contest_submissions (contest_id, final_rank NULLS LAST);
CREATE INDEX idx_contest_submissions_user
    ON contest_submissions (talent_user_id, submitted_at DESC);

-- ═══════════════════════════════════════════════════════════════════
-- Interview scheduling
-- ═══════════════════════════════════════════════════════════════════
--
-- Deliberately polymorphic. A contest shortlist, a recruitment campaign and
-- a trial period all end in the same conversation, and three scheduling
-- tables would mean three notification paths and three places to forget the
-- time zone.

CREATE TABLE interview_scheduling (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_type VARCHAR(30) NOT NULL CHECK (source_type IN (
        'enterprise_contest', 'recruitment_campaign', 'recruitment_trial'
    )),
    source_id UUID NOT NULL,

    talent_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,

    -- Offered by the company, chosen by the person. Stored as given, in UTC,
    -- because a slot rewritten into somebody's local time is a slot argued
    -- about later.
    proposed_slots JSONB NOT NULL DEFAULT '[]'::jsonb,
    confirmed_slot JSONB,

    platform VARCHAR(20) CHECK (platform IS NULL OR platform IN (
        'zoom', 'meet', 'teams', 'phone', 'in_person'
    )),
    meeting_url VARCHAR(500),
    location TEXT,

    status VARCHAR(20) NOT NULL DEFAULT 'proposed' CHECK (status IN (
        'proposed', 'confirmed', 'completed', 'declined', 'cancelled'
    )),
    declined_reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A confirmed interview with no slot is a meeting nobody can attend.
    CONSTRAINT a_confirmed_interview_has_a_time CHECK (
        status NOT IN ('confirmed', 'completed') OR confirmed_slot IS NOT NULL
    ),
    -- Proposing nothing is not proposing.
    CONSTRAINT a_proposal_offers_something CHECK (
        status <> 'proposed' OR jsonb_array_length(proposed_slots) > 0
    ),
    CONSTRAINT a_remote_interview_has_a_link CHECK (
        status <> 'confirmed'
        OR platform IN ('phone', 'in_person')
        OR platform IS NULL
        OR meeting_url IS NOT NULL
    )
);

COMMENT ON TABLE interview_scheduling IS
    'One table for every interview Skilluv arranges. Three would have meant '
    'three notification paths and three places to forget the time zone.';

CREATE INDEX idx_interviews_source ON interview_scheduling (source_type, source_id);
CREATE INDEX idx_interviews_talent
    ON interview_scheduling (talent_user_id, created_at DESC);
CREATE INDEX idx_interviews_pending
    ON interview_scheduling (created_at)
    WHERE status = 'proposed';

CREATE TRIGGER trg_interviews_updated_at
    BEFORE UPDATE ON interview_scheduling
    FOR EACH ROW EXECUTE FUNCTION touch_contests_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- Success fees follow the hire, whatever produced it
-- ═══════════════════════════════════════════════════════════════════
--
-- The guarantee, the pro-rated refund and the departure tracking are already
-- built in migration 0226. A contest hire is the same fee out of a different
-- door, so it gets a column rather than a table.

ALTER TABLE recruitment_success_fees
    ADD COLUMN contest_id UUID REFERENCES enterprise_contests(id) ON DELETE SET NULL;

ALTER TABLE recruitment_success_fees
    ADD CONSTRAINT a_fee_names_what_produced_it CHECK (
        campaign_id IS NOT NULL OR contest_id IS NOT NULL
    );

COMMENT ON CONSTRAINT a_fee_names_what_produced_it ON recruitment_success_fees IS
    'Every fee points at the campaign or the contest that produced the hire. '
    'One with neither cannot be defended when the client asks what they are '
    'paying for.';

CREATE INDEX idx_success_fees_contest
    ON recruitment_success_fees (contest_id)
    WHERE contest_id IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- Attestations for the people who did not win
-- ═══════════════════════════════════════════════════════════════════
--
-- Two new bases rather than two new attestation types. `artefact` already
-- means "rests on something, and `basis` says what" — a shortlist place is
-- exactly that. A new type would have needed its own generator, its own
-- verification page and its own row in every switch that reads the four.
--
-- The point of the finalist one is that it is worth having when the answer
-- was no: it says a company with a real vacancy put this person in its last
-- three, which is a harder thing to claim than a certificate.

ALTER TABLE attestations
    DROP CONSTRAINT IF EXISTS attestations_basis_check;

ALTER TABLE attestations
    ADD CONSTRAINT attestations_basis_check
    CHECK (basis IS NULL OR basis IN (
        -- Code (migration 0178)
        'code_pr_merged_upstream',
        'code_project_shipped',
        'code_library_published',
        'code_rfc_accepted',
        'code_standard_contribution',
        'code_devtool_adopted',
        'featured_coder',
        -- AI (migration 0213)
        'ai_model_shipped',
        'ai_dataset_published',
        'ai_agent_system_deployed',
        'ai_paper_published',
        'ai_benchmark_result',
        'ai_safety_finding_validated',
        'featured_ai_researcher',
        -- Contests (this migration). Worth having when the answer was no.
        'contest_finalist',
        'contest_hired'
    ));

-- The contest bases rest on a contest, not on a deliverable, so they are
-- deliberately outside `attestations_artifact_basis_links_a_deliverable`.
ALTER TABLE attestations
    ADD COLUMN contest_id UUID REFERENCES enterprise_contests(id) ON DELETE SET NULL;

ALTER TABLE attestations
    ADD CONSTRAINT a_contest_attestation_names_its_contest CHECK (
        basis IS NULL
        OR basis NOT IN ('contest_finalist', 'contest_hired')
        OR contest_id IS NOT NULL
    );

-- One finalist attestation per person per contest. A second would double
-- every count that reads them.
CREATE UNIQUE INDEX uniq_attestations_per_contest
    ON attestations (user_id, contest_id, basis)
    WHERE contest_id IS NOT NULL AND revoked_at IS NULL;

-- ═══════════════════════════════════════════════════════════════════
-- The revenue streams and products these feed
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO revenue_streams (slug, pillar, label, description, recurring) VALUES
    ('contest_orchestration_fee', 'brand', 'Orchestration de concours',
     'Ce que Skilluv facture pour organiser un concours d''entreprise : le '
     'brief, le jury, le jugement et la remise.',
     FALSE)
ON CONFLICT (slug) DO NOTHING;

INSERT INTO enterprise_product_types
    (slug, label, description, revenue_stream, recurring)
VALUES
    ('innovation_award', 'Grand défi',
     'Un concours à grande échelle, avec cagnotte et campagne.',
     'contest_orchestration_fee', FALSE),
    ('product_led_hackathon', 'Hackathon produit',
     'Un hackathon dont le prix est l''intégration du travail gagnant.',
     'contest_orchestration_fee', FALSE),
    ('corporate_hackathon', 'Hackathon interne',
     'Un hackathon d''entreprise avec des profils Skilluv mêlés aux équipes.',
     'contest_orchestration_fee', FALSE),
    ('migration_contest', 'Concours de migration',
     'Un concours d''approche, puis la migration confiée à l''équipe gagnante.',
     'contest_orchestration_fee', FALSE)
ON CONFLICT (slug) DO NOTHING;
