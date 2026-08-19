-- Teams Skilluv sells, and the work they are sold for.
--
-- ## Eight tickets, four tables
--
-- The backlog describes Studios, ad-hoc outsourcing, long retainers,
-- discovery phases, group sprints and fractional placements as six tables.
-- Five of them are the same sentence: *an enterprise buys assembled people
-- for a bounded period, at a margin*. They differ in three fields — who
-- assembled them, how long, and how it is priced.
--
-- Six tables would have meant six status machines, six member lists and six
-- places to get the escrow cascade wrong. One engagement table with a `kind`
-- says the same thing, and the milestones that carry the money are written
-- once.
--
-- ## What is genuinely separate
--
-- **A studio** is a standing team: it exists between engagements, has an
-- identity, a specialisation and a track record. That is the differentiator
-- against ad-hoc assembly and it is why the margin is higher — so it gets its
-- own table rather than being an engagement with a name.
--
-- **Beta testing** is not a team at all: a hundred people each paid a small
-- fixed reward for an opinion. Modelling it as an engagement would mean a
-- hundred members on one row and a delivery nobody delivers.

CREATE TABLE studios (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(80) NOT NULL UNIQUE,
    name VARCHAR(120) NOT NULL,
    -- What this studio is for. Narrow on purpose: a studio that does
    -- everything is a pool, and a pool has no track record to sell.
    specialization TEXT NOT NULL CHECK (btrim(specialization) <> ''),
    domains TEXT[] NOT NULL DEFAULT '{}',
    -- What the whole team costs for a day. The unit clients actually buy.
    day_rate NUMERIC(12,2) NOT NULL CHECK (day_rate > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    max_members SMALLINT NOT NULL DEFAULT 15 CHECK (max_members BETWEEN 2 AND 30),
    -- Somebody answers for the studio. A team with no lead is a group of
    -- people who each think somebody else is handling it.
    lead_user_id UUID REFERENCES users(id) ON DELETE SET NULL,

    status VARCHAR(20) NOT NULL DEFAULT 'forming' CHECK (status IN (
        -- Recruiting its members. Not sellable.
        'forming',
        'active',
        -- Between engagements by choice, still a team.
        'paused',
        'disbanded'
    )),
    disbanded_reason TEXT,

    formed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT disbanding_carries_a_reason CHECK (
        status <> 'disbanded'
        OR (disbanded_reason IS NOT NULL AND btrim(disbanded_reason) <> '')
    ),
    -- A studio that is sellable has somebody answering for it.
    CONSTRAINT an_active_studio_has_a_lead CHECK (
        status <> 'active' OR lead_user_id IS NOT NULL
    )
);

COMMENT ON TABLE studios IS
    'A standing team: it exists between engagements, which is the whole '
    'difference from ad-hoc assembly and the reason its margin is higher.';

CREATE INDEX idx_studios_sellable
    ON studios (status, day_rate)
    WHERE status = 'active';
CREATE INDEX idx_studios_domains ON studios USING GIN (domains);

CREATE TRIGGER trg_studios_updated_at
    BEFORE UPDATE ON studios
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

CREATE TABLE studio_members (
    studio_id UUID NOT NULL REFERENCES studios(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_in_studio VARCHAR(120) NOT NULL CHECK (btrim(role_in_studio) <> ''),
    -- What share of an engagement's talent pot this person takes. The shares
    -- of a studio's live members must total a hundred — enforced below,
    -- because a studio whose shares sum to ninety is a studio where somebody
    -- silently loses a tenth of their pay.
    revenue_share_percent NUMERIC(5,2) NOT NULL
        CHECK (revenue_share_percent > 0 AND revenue_share_percent <= 100),
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    left_at TIMESTAMPTZ,
    PRIMARY KEY (studio_id, user_id)
);

COMMENT ON COLUMN studio_members.revenue_share_percent IS
    'Of the engagement''s talent pot. Live members must total 100: a studio '
    'summing to 90 is one where somebody silently loses a tenth of their pay.';

CREATE INDEX idx_studio_members_live
    ON studio_members (studio_id)
    WHERE left_at IS NULL;
CREATE INDEX idx_studio_members_user ON studio_members (user_id);

-- ═══════════════════════════════════════════════════════════════════
-- The engagement
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE team_engagements (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    -- Set when a standing team took it. NULL means Skilluv assembled people
    -- for this one piece of work, which is the cheaper margin.
    studio_id UUID REFERENCES studios(id) ON DELETE SET NULL,

    kind VARCHAR(20) NOT NULL CHECK (kind IN (
        -- A team assembled for one project.
        'outsourcing',
        -- Timeboxed exploration where the client has a question rather than
        -- a brief. The deliverable is a recommendation.
        'discovery',
        -- Short, intense, a fixed cohort. Bought by the sprint.
        'sprint',
        -- One person, part of their week, for months.
        'fractional'
    )),

    title VARCHAR(200) NOT NULL CHECK (btrim(title) <> ''),
    brief_md TEXT NOT NULL CHECK (btrim(brief_md) <> ''),
    domains_required TEXT[] NOT NULL DEFAULT '{}',
    orientations_required TEXT[] NOT NULL DEFAULT '{}',

    team_size_min SMALLINT NOT NULL DEFAULT 1 CHECK (team_size_min > 0),
    team_size_max SMALLINT NOT NULL CHECK (team_size_max > 0),
    duration_weeks SMALLINT CHECK (duration_weeks IS NULL OR duration_weeks > 0),
    -- `fractional` only: how much of somebody's week.
    days_per_week NUMERIC(3,1)
        CHECK (days_per_week IS NULL OR (days_per_week >= 0.5 AND days_per_week <= 4)),

    pricing_model VARCHAR(20) NOT NULL CHECK (pricing_model IN (
        'fixed_price',
        'retainer_monthly',
        'day_rate'
    )),
    budget NUMERIC(14,2) CHECK (budget IS NULL OR budget > 0),
    monthly_retainer NUMERIC(14,2) CHECK (monthly_retainer IS NULL OR monthly_retainer > 0),
    day_rate NUMERIC(12,2) CHECK (day_rate IS NULL OR day_rate > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    -- What Skilluv keeps. Higher for a studio: the client is buying an
    -- assembled team with a track record and management included, not a
    -- list of people who happened to be free.
    margin_percent NUMERIC(5,2) NOT NULL
        CHECK (margin_percent >= 0 AND margin_percent <= 40),

    nda_required BOOLEAN NOT NULL DEFAULT TRUE,
    ip_terms VARCHAR(40) NOT NULL DEFAULT 'full_ownership_client' CHECK (ip_terms IN (
        'full_ownership_client',
        'open_source_output',
        'retain_reusable_components',
        'dual_license'
    )),
    upstream_license_spdx VARCHAR(60) REFERENCES software_licenses(spdx_id),

    status VARCHAR(30) NOT NULL DEFAULT 'briefing' CHECK (status IN (
        'briefing',
        -- Skilluv is putting the team together.
        'assembling',
        'proposed',
        'in_progress',
        'delivered',
        'closed',
        'cancelled'
    )),
    -- Whoever at Skilluv answers for delivery. Part of what the margin pays
    -- for, and the difference from a freelance marketplace.
    project_lead_user_id UUID REFERENCES users(id) ON DELETE SET NULL,

    starts_at TIMESTAMPTZ,
    ends_at TIMESTAMPTZ,
    delivered_at TIMESTAMPTZ,
    closed_at TIMESTAMPTZ,
    closed_reason TEXT,

    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT team_size_runs_forward CHECK (team_size_max >= team_size_min),

    -- Each pricing model needs its own figure and only its own. A retainer
    -- with a budget and no monthly amount is an engagement nobody can invoice.
    CONSTRAINT price_matches_the_model CHECK (
        CASE pricing_model
            WHEN 'fixed_price' THEN budget IS NOT NULL
            WHEN 'retainer_monthly' THEN monthly_retainer IS NOT NULL
            WHEN 'day_rate' THEN day_rate IS NOT NULL
        END
    ),

    -- A fractional placement is one person, part of their week. Written as a
    -- constraint because "team of one at four days" and "team of four" are
    -- priced and staffed completely differently.
    CONSTRAINT fractional_is_one_person CHECK (
        kind <> 'fractional'
        OR (team_size_max = 1 AND days_per_week IS NOT NULL)
    ),
    -- Discovery is bounded by definition: it exists to stop an open-ended
    -- exploration becoming an open-ended bill.
    CONSTRAINT discovery_is_timeboxed CHECK (
        kind <> 'discovery'
        OR (duration_weeks IS NOT NULL AND duration_weeks BETWEEN 2 AND 6)
    ),
    CONSTRAINT a_sprint_is_short CHECK (
        kind <> 'sprint'
        OR (duration_weeks IS NOT NULL AND duration_weeks BETWEEN 1 AND 12)
    ),

    CONSTRAINT cancellation_carries_a_reason CHECK (
        status <> 'cancelled'
        OR (closed_reason IS NOT NULL AND btrim(closed_reason) <> '')
    )
);

COMMENT ON TABLE team_engagements IS
    'An enterprise buying assembled people for a bounded period. Five of the '
    'backlog''s six tables were this sentence with different durations and '
    'pricing; six status machines would have meant six ways to get escrow '
    'wrong.';

COMMENT ON COLUMN team_engagements.margin_percent IS
    'Higher for a studio: the client buys an assembled team with a track '
    'record and management included, not a list of people who were free.';

CREATE INDEX idx_engagements_enterprise
    ON team_engagements (enterprise_id, status, created_at DESC);
CREATE INDEX idx_engagements_studio
    ON team_engagements (studio_id, status)
    WHERE studio_id IS NOT NULL;
CREATE INDEX idx_engagements_open
    ON team_engagements (kind, status)
    WHERE status IN ('briefing', 'assembling', 'proposed', 'in_progress');

CREATE TRIGGER trg_engagements_updated_at
    BEFORE UPDATE ON team_engagements
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- The licence check that already governs missions, applied here too: an
-- engagement is where somebody would most expect to promise ownership they
-- cannot deliver, because the work is larger and the contract is longer.
CREATE OR REPLACE FUNCTION engagement_ip_terms_match_the_license()
RETURNS TRIGGER AS $$
DECLARE
    l RECORD;
BEGIN
    IF NEW.upstream_license_spdx IS NULL THEN
        RETURN NEW;
    END IF;

    SELECT * INTO l FROM software_licenses WHERE spdx_id = NEW.upstream_license_spdx;

    IF NEW.ip_terms IN ('full_ownership_client', 'retain_reusable_components')
       AND NOT l.allows_client_ownership THEN
        RAISE EXCEPTION
            'a % engagement cannot promise client ownership: %',
            NEW.upstream_license_spdx, l.caveat;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_engagement_ip_terms_match_the_license
    BEFORE INSERT OR UPDATE ON team_engagements
    FOR EACH ROW EXECUTE FUNCTION engagement_ip_terms_match_the_license();

CREATE TABLE engagement_members (
    engagement_id UUID NOT NULL REFERENCES team_engagements(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_on_engagement VARCHAR(120) NOT NULL CHECK (btrim(role_on_engagement) <> ''),
    -- Of the talent pot. Copied from the studio's shares when a studio takes
    -- the work, set by hand when Skilluv assembles.
    share_percent NUMERIC(5,2) NOT NULL
        CHECK (share_percent > 0 AND share_percent <= 100),
    -- Nobody is put on paid work without agreeing to it.
    accepted_at TIMESTAMPTZ,
    declined_at TIMESTAMPTZ,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    left_at TIMESTAMPTZ,

    PRIMARY KEY (engagement_id, user_id),

    CONSTRAINT not_both_answers CHECK (accepted_at IS NULL OR declined_at IS NULL)
);

COMMENT ON TABLE engagement_members IS
    'Who is on it, and for what share. Nobody is put on paid work without '
    'agreeing: `accepted_at` is the agreement, and an engagement cannot start '
    'without everybody having given one.';

CREATE INDEX idx_engagement_members_user ON engagement_members (user_id);

-- ═══════════════════════════════════════════════════════════════════
-- Milestones — the money, and the quality gate
-- ═══════════════════════════════════════════════════════════════════
--
-- One table serving two tickets. The escrow cascade (03-02) needs somewhere
-- to say "this much is released now"; the quality guarantee (03-04) needs
-- somewhere to say "this was reviewed before the client saw it". Both are
-- properties of the same checkpoint, and two tables would mean a milestone
-- that was paid without being reviewed.

CREATE TABLE engagement_milestones (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    engagement_id UUID NOT NULL REFERENCES team_engagements(id) ON DELETE CASCADE,
    sequence SMALLINT NOT NULL CHECK (sequence > 0),
    title VARCHAR(200) NOT NULL CHECK (btrim(title) <> ''),
    -- What "done" means for this checkpoint. Agreed before the work, because
    -- a milestone defined afterwards is a milestone argued about.
    acceptance_criteria TEXT NOT NULL CHECK (btrim(acceptance_criteria) <> ''),
    due_on DATE,

    -- The share of the engagement's value released here. The shares must
    -- total a hundred — a set that sums to eighty leaves a fifth of the
    -- contract with nowhere to be paid from.
    value_percent NUMERIC(5,2) NOT NULL
        CHECK (value_percent > 0 AND value_percent <= 100),

    status VARCHAR(30) NOT NULL DEFAULT 'pending' CHECK (status IN (
        'pending',
        'in_progress',
        -- Handed to Skilluv's review, not to the client. This is the step
        -- that distinguishes the platform from a freelance marketplace, and
        -- it is why the margin is what it is.
        'in_review',
        -- Passed review, waiting on the client.
        'submitted',
        'accepted',
        'rejected'
    )),

    -- The quality gate. Who reviewed it, and what they said.
    reviewed_by UUID REFERENCES users(id) ON DELETE SET NULL,
    reviewed_at TIMESTAMPTZ,
    review_notes TEXT,

    accepted_at TIMESTAMPTZ,
    accepted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    rejected_at TIMESTAMPTZ,
    rejection_reason TEXT,
    -- Set when the talent pot for this milestone has actually been released.
    released_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (engagement_id, sequence),

    CONSTRAINT rejection_carries_a_reason CHECK (
        status <> 'rejected'
        OR (rejection_reason IS NOT NULL AND btrim(rejection_reason) <> '')
    ),
    -- Nothing reaches the client without having been reviewed. The guarantee
    -- is the product; a milestone that skipped it is a milestone the margin
    -- was charged for and not delivered.
    CONSTRAINT nothing_reaches_the_client_unreviewed CHECK (
        status NOT IN ('submitted', 'accepted') OR reviewed_at IS NOT NULL
    ),
    CONSTRAINT nothing_is_released_unaccepted CHECK (
        released_at IS NULL OR accepted_at IS NOT NULL
    )
);

COMMENT ON TABLE engagement_milestones IS
    'One checkpoint: what is due, what it is worth, and who reviewed it. The '
    'money and the quality gate are properties of the same moment, and two '
    'tables would mean a milestone paid without being reviewed.';

COMMENT ON CONSTRAINT nothing_reaches_the_client_unreviewed ON engagement_milestones IS
    'The guarantee is the product. A milestone that skipped review is one the '
    'margin was charged for and not delivered.';

CREATE INDEX idx_milestones_engagement ON engagement_milestones (engagement_id, sequence);
CREATE INDEX idx_milestones_awaiting_review
    ON engagement_milestones (created_at)
    WHERE status = 'in_review';
CREATE INDEX idx_milestones_payable
    ON engagement_milestones (accepted_at)
    WHERE status = 'accepted' AND released_at IS NULL;

CREATE TRIGGER trg_milestones_updated_at
    BEFORE UPDATE ON engagement_milestones
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- Beta testing
-- ═══════════════════════════════════════════════════════════════════
--
-- Not a team. A hundred people each paid a small fixed reward for an
-- opinion, with Skilluv charging separately for running it — recruiting the
-- right testers, structuring the feedback, and writing the report that makes
-- a hundred opinions usable.

CREATE TABLE beta_programs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    product_name VARCHAR(200) NOT NULL CHECK (btrim(product_name) <> ''),
    brief_md TEXT NOT NULL CHECK (btrim(brief_md) <> ''),

    test_type VARCHAR(30) NOT NULL CHECK (test_type IN (
        'usability', 'game_playtest', 'security', 'performance', 'accessibility'
    )),
    -- What kind of tester is wanted. A usability test wants people who are
    -- not experts; a security test wants the opposite, and recruiting the
    -- wrong ones produces a report that says nothing.
    target_domains TEXT[] NOT NULL DEFAULT '{}',
    target_orientations TEXT[] NOT NULL DEFAULT '{}',

    testers_wanted SMALLINT NOT NULL CHECK (testers_wanted BETWEEN 5 AND 500),
    duration_weeks SMALLINT NOT NULL CHECK (duration_weeks BETWEEN 1 AND 12),
    -- Per tester, paid on accepted feedback. Fixed rather than hourly:
    -- testing is bounded work and an hourly rate would invite padding.
    tester_reward NUMERIC(10,2) NOT NULL CHECK (tester_reward > 0),
    -- What Skilluv charges for running it. Separate from the rewards, and
    -- visible as such: a client should see what goes to the testers and what
    -- goes to the platform.
    program_fee NUMERIC(12,2) NOT NULL CHECK (program_fee >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    status VARCHAR(20) NOT NULL DEFAULT 'recruiting' CHECK (status IN (
        'recruiting', 'running', 'reporting', 'closed', 'cancelled'
    )),
    closed_reason TEXT,
    started_at TIMESTAMPTZ,
    ends_at TIMESTAMPTZ,
    closed_at TIMESTAMPTZ,

    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT cancellation_carries_a_reason CHECK (
        status <> 'cancelled'
        OR (closed_reason IS NOT NULL AND btrim(closed_reason) <> '')
    )
);

COMMENT ON TABLE beta_programs IS
    'A hundred people paid a small fixed reward for an opinion. The program '
    'fee is separate from the rewards and visible as such: a client should '
    'see what goes to the testers and what goes to the platform.';

CREATE INDEX idx_beta_recruiting
    ON beta_programs (test_type, created_at DESC)
    WHERE status = 'recruiting';
CREATE INDEX idx_beta_enterprise ON beta_programs (enterprise_id, created_at DESC);

CREATE TRIGGER trg_beta_programs_updated_at
    BEFORE UPDATE ON beta_programs
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

CREATE TABLE beta_testers (
    program_id UUID NOT NULL REFERENCES beta_programs(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    status VARCHAR(20) NOT NULL DEFAULT 'joined' CHECK (status IN (
        'joined',
        'submitted',
        -- The feedback was usable. What the reward is paid on.
        'accepted',
        -- It was not. Carries a reason, because somebody spent hours on it.
        'rejected',
        'withdrew'
    )),
    feedback_md TEXT,
    submitted_at TIMESTAMPTZ,
    reviewed_at TIMESTAMPTZ,
    rejection_reason TEXT,
    reward_paid_at TIMESTAMPTZ,

    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (program_id, user_id),

    CONSTRAINT submitted_feedback_says_something CHECK (
        status NOT IN ('submitted', 'accepted')
        OR (feedback_md IS NOT NULL AND btrim(feedback_md) <> '')
    ),
    CONSTRAINT rejection_carries_a_reason CHECK (
        status <> 'rejected'
        OR (rejection_reason IS NOT NULL AND btrim(rejection_reason) <> '')
    ),
    CONSTRAINT only_accepted_feedback_is_paid CHECK (
        reward_paid_at IS NULL OR status = 'accepted'
    )
);

COMMENT ON TABLE beta_testers IS
    'One row per tester. Rejection carries a reason: somebody spent hours on '
    'the feedback being refused.';

CREATE INDEX idx_beta_testers_user ON beta_testers (user_id, joined_at DESC);
CREATE INDEX idx_beta_testers_payable
    ON beta_testers (program_id)
    WHERE status = 'accepted' AND reward_paid_at IS NULL;

-- A programme stops taking testers once it has the number it asked for.
-- In the database, because two people joining at the same moment against the
-- last place would both pass a service check.
CREATE OR REPLACE FUNCTION beta_program_has_room()
RETURNS TRIGGER AS $$
DECLARE
    program RECORD;
    joined INTEGER;
BEGIN
    SELECT status, testers_wanted INTO program
      FROM beta_programs WHERE id = NEW.program_id FOR UPDATE;

    IF program.status <> 'recruiting' THEN
        RAISE EXCEPTION 'this programme is %, not recruiting', program.status;
    END IF;

    SELECT count(*) INTO joined
      FROM beta_testers
     WHERE program_id = NEW.program_id AND status <> 'withdrew';

    IF joined >= program.testers_wanted THEN
        RAISE EXCEPTION 'this programme already has its % testers',
            program.testers_wanted;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_beta_program_has_room
    BEFORE INSERT ON beta_testers
    FOR EACH ROW EXECUTE FUNCTION beta_program_has_room();

-- ═══════════════════════════════════════════════════════════════════
-- The revenue streams these feed
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO revenue_streams (slug, pillar, label, description, recurring) VALUES
    ('beta_program_fee', 'work', 'Programme de test',
     'Ce que Skilluv facture pour organiser un test : recruter les bons '
     'testeurs, structurer les retours, et écrire le rapport qui rend cent '
     'avis exploitables.',
     FALSE)
ON CONFLICT (slug) DO NOTHING;

INSERT INTO enterprise_product_types
    (slug, label, description, revenue_stream, recurring)
VALUES
    ('discovery_phase', 'Phase de cadrage',
     'Une exploration bornée quand le client a une question plutôt qu''un brief.',
     'consulting_fee', FALSE),
    ('group_sprint', 'Sprint groupé',
     'Une cohorte réunie pour une durée courte et intense.',
     'outsourcing_margin', FALSE),
    ('fractional_placement', 'Placement fractionné',
     'Une personne, une partie de sa semaine, sur plusieurs mois.',
     'outsourcing_margin', TRUE),
    ('beta_program', 'Programme de test',
     'Une cohorte de testeurs rémunérés, avec rapport.',
     'beta_program_fee', FALSE)
ON CONFLICT (slug) DO NOTHING;
