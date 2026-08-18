-- Business model — onboarding as a service, living labs, team proposals.
-- Migration 0240.
--
-- ## Three products that keep a relationship going
--
-- The rest of the business model sells a transaction: a hire, a contest, a
-- report. These three sell continuity — somebody accompanied through their
-- first months, a community kept engaged with a product, a team that keeps
-- working together and goes looking for the next thing itself.
--
-- ## The direction of the last one
--
-- Migration 0230 reversed recruitment: a person posts what they want and
-- companies pitch. This reverses delivery: a team posts a solution to a
-- problem they have identified, and companies decide whether they have that
-- problem. It is a different object — it ends in an engagement, not a hire —
-- and it is the only place on the platform where the offer originates with
-- the people doing the work.

-- ═══════════════════════════════════════════════════════════════════
-- Onboarding a new hire
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE hire_onboarding_engagements (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,

    -- The person being onboarded. Their own account, always: the whole
    -- product is three months of somebody's attention on them, and it cannot
    -- be delivered to an email address.
    junior_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    -- Their own agreement. Being accompanied is done with somebody, not to
    -- them, and their employer buying it does not make it consented to.
    junior_accepted_at TIMESTAMPTZ,
    junior_declined_at TIMESTAMPTZ,

    mentor_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,

    duration_months SMALLINT NOT NULL DEFAULT 3 CHECK (duration_months BETWEEN 1 AND 12),
    curriculum JSONB NOT NULL DEFAULT '{}'::jsonb,
    fee NUMERIC(10,2) NOT NULL CHECK (fee > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    -- The mentor's share of the fee. The rest is what Skilluv keeps for
    -- designing the run and holding the check-ins.
    mentor_share_percent NUMERIC(5,2) NOT NULL DEFAULT 60.00
        CHECK (mentor_share_percent >= 0 AND mentor_share_percent <= 100),

    started_on DATE,
    completed_at TIMESTAMPTZ,

    -- Whether the person is still there at three and six months. The reason
    -- the product exists, and also a fact about somebody that their employer
    -- and Skilluv both benefit from knowing — so it is recorded only for
    -- somebody who agreed to the engagement in the first place.
    retention_3m BOOLEAN,
    retention_6m BOOLEAN,
    retention_checked_at TIMESTAMPTZ,

    status VARCHAR(20) NOT NULL DEFAULT 'proposed' CHECK (status IN (
        'proposed', 'active', 'completed', 'ended_early', 'declined'
    )),
    ended_reason TEXT,

    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT not_both_answers CHECK (
        junior_accepted_at IS NULL OR junior_declined_at IS NULL
    ),
    -- Nothing starts before the person says yes.
    CONSTRAINT nothing_starts_without_the_junior CHECK (
        status <> 'active' OR junior_accepted_at IS NOT NULL
    ),
    -- And nothing is recorded about how long they stayed if they never
    -- agreed to be accompanied.
    CONSTRAINT retention_follows_an_agreement CHECK (
        (retention_3m IS NULL AND retention_6m IS NULL)
        OR junior_accepted_at IS NOT NULL
    ),
    CONSTRAINT the_mentor_is_not_the_junior CHECK (mentor_user_id <> junior_user_id),
    CONSTRAINT ending_early_carries_a_reason CHECK (
        status <> 'ended_early'
        OR (ended_reason IS NOT NULL AND btrim(ended_reason) <> '')
    )
);

COMMENT ON TABLE hire_onboarding_engagements IS
    'Three months of somebody accompanying a new hire. Bought by the '
    'employer, agreed to by the person: being accompanied is done with '
    'somebody, not to them.';

COMMENT ON COLUMN hire_onboarding_engagements.retention_3m IS
    'Whether they are still there. The reason the product exists, and a fact '
    'about somebody their employer benefits from knowing — recorded only for '
    'an engagement they agreed to.';

CREATE INDEX idx_onboarding_enterprise
    ON hire_onboarding_engagements (enterprise_id, created_at DESC);
CREATE INDEX idx_onboarding_junior
    ON hire_onboarding_engagements (junior_user_id, created_at DESC);
CREATE INDEX idx_onboarding_due_check
    ON hire_onboarding_engagements (started_on)
    WHERE status = 'active';

CREATE OR REPLACE FUNCTION touch_continuous_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_onboarding_updated_at
    BEFORE UPDATE ON hire_onboarding_engagements
    FOR EACH ROW EXECUTE FUNCTION touch_continuous_updated_at();

-- The monthly conversation. Written down because "we checked in" without a
-- record is what every abandoned onboarding programme says about itself.
CREATE TABLE onboarding_check_ins (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    engagement_id UUID NOT NULL REFERENCES hire_onboarding_engagements(id)
        ON DELETE CASCADE,

    month_number SMALLINT NOT NULL CHECK (month_number BETWEEN 1 AND 12),
    held_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- The mentor's note, and the junior's. Both, because an onboarding
    -- assessed only by the person paid to deliver it assesses itself.
    mentor_notes_md TEXT,
    junior_notes_md TEXT,
    going_well BOOLEAN,

    UNIQUE (engagement_id, month_number)
);

COMMENT ON TABLE onboarding_check_ins IS
    'Both sides write. An onboarding assessed only by the person paid to '
    'deliver it assesses itself.';

-- ═══════════════════════════════════════════════════════════════════
-- Living labs
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE living_lab_engagements (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,

    product_name VARCHAR(200) NOT NULL CHECK (btrim(product_name) <> ''),
    scope_md TEXT NOT NULL CHECK (btrim(scope_md) <> ''),
    community_target SMALLINT NOT NULL CHECK (community_target BETWEEN 10 AND 2000),

    activity_types TEXT[] NOT NULL CHECK (cardinality(activity_types) > 0),

    monthly_fee NUMERIC(10,2) NOT NULL CHECK (monthly_fee > 0),
    -- What goes to the people doing the testing, each month. Separate from
    -- the fee and visible as such, as everywhere else on this platform: a
    -- client should see what reaches the community and what reaches us.
    monthly_reward_pool NUMERIC(10,2) NOT NULL CHECK (monthly_reward_pool >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    started_on DATE,
    ends_on DATE,
    status VARCHAR(20) NOT NULL DEFAULT 'setup' CHECK (status IN (
        'setup', 'recruiting', 'running', 'paused', 'ended'
    )),
    ended_reason TEXT,

    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_lab_runs_forward CHECK (
        ends_on IS NULL OR started_on IS NULL OR ends_on > started_on
    ),
    -- A lab whose reward pool is zero is a company asking a hundred people
    -- to work on its product for the pleasure of it, with Skilluv taking a
    -- monthly fee for arranging that.
    CONSTRAINT a_lab_pays_the_people_in_it CHECK (monthly_reward_pool > 0),
    CONSTRAINT ending_carries_a_reason CHECK (
        status <> 'ended'
        OR (ended_reason IS NOT NULL AND btrim(ended_reason) <> '')
    )
);

COMMENT ON CONSTRAINT a_lab_pays_the_people_in_it ON living_lab_engagements IS
    'A zero pool is a company asking a hundred people to work on its product '
    'for the pleasure of it, with Skilluv taking a monthly fee for arranging '
    'that.';

CREATE INDEX idx_labs_open
    ON living_lab_engagements (created_at DESC)
    WHERE status = 'recruiting';
CREATE INDEX idx_labs_enterprise
    ON living_lab_engagements (enterprise_id, created_at DESC);

CREATE TRIGGER trg_labs_updated_at
    BEFORE UPDATE ON living_lab_engagements
    FOR EACH ROW EXECUTE FUNCTION touch_continuous_updated_at();

CREATE TABLE living_lab_members (
    lab_id UUID NOT NULL REFERENCES living_lab_engagements(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    left_at TIMESTAMPTZ,
    -- Whether they agreed to see the client's unreleased product. A lab
    -- usually means an NDA, and joining one is a commitment beyond turning up.
    nda_accepted_at TIMESTAMPTZ,

    PRIMARY KEY (lab_id, user_id)
);

CREATE INDEX idx_lab_members_user ON living_lab_members (user_id, joined_at DESC);

CREATE TABLE living_lab_contributions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    lab_id UUID NOT NULL,
    user_id UUID NOT NULL,

    activity_type VARCHAR(40) NOT NULL,
    summary_md TEXT NOT NULL CHECK (btrim(summary_md) <> ''),
    -- The month it counts for, as its first day: the pool is monthly, so a
    -- contribution belongs to a month and not to an hour.
    counts_for_month DATE NOT NULL,

    accepted BOOLEAN,
    rejection_reason TEXT,
    reward NUMERIC(8,2) CHECK (reward IS NULL OR reward >= 0),
    paid_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    FOREIGN KEY (lab_id, user_id) REFERENCES living_lab_members(lab_id, user_id)
        ON DELETE CASCADE,

    CONSTRAINT a_month_is_its_first_day CHECK (EXTRACT(DAY FROM counts_for_month) = 1),
    CONSTRAINT a_rejection_carries_a_reason CHECK (
        accepted IS DISTINCT FROM FALSE
        OR (rejection_reason IS NOT NULL AND btrim(rejection_reason) <> '')
    ),
    CONSTRAINT only_accepted_work_is_paid CHECK (
        paid_at IS NULL OR accepted IS TRUE
    )
);

CREATE INDEX idx_lab_contributions_month
    ON living_lab_contributions (lab_id, counts_for_month);
CREATE INDEX idx_lab_contributions_payable
    ON living_lab_contributions (lab_id, counts_for_month)
    WHERE accepted IS TRUE AND paid_at IS NULL;

-- ═══════════════════════════════════════════════════════════════════
-- Team proposals
-- ═══════════════════════════════════════════════════════════════════
--
-- A team says: here is a problem we think you have, here is what we would do
-- about it, here is what it would cost. Companies say whether they have it.
--
-- The only place on the platform where the offer originates with the people
-- doing the work. Migration 0230 reversed recruitment; this reverses
-- delivery, and it ends in an engagement rather than a hire.

CREATE TABLE team_proposals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(120) NOT NULL UNIQUE,
    initiator_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- The standing team behind it, when there is one.
    studio_id UUID REFERENCES studios(id) ON DELETE SET NULL,

    title VARCHAR(200) NOT NULL CHECK (btrim(title) <> ''),
    -- The problem first, in the client's terms. A proposal that opens with
    -- the solution is a team describing what it wants to build.
    problem_md TEXT NOT NULL CHECK (length(btrim(problem_md)) >= 100),
    approach_md TEXT NOT NULL CHECK (length(btrim(approach_md)) >= 100),
    -- What this team has already done that makes the claim credible.
    evidence JSONB NOT NULL DEFAULT '[]'::jsonb,

    budget_estimate NUMERIC(12,2) CHECK (budget_estimate IS NULL OR budget_estimate > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    available_from DATE,
    available_until DATE,

    target_industries TEXT[] NOT NULL DEFAULT '{}',
    -- Named companies, or nobody in particular. A proposal aimed at everybody
    -- is public; one aimed at three companies is visible only to them.
    target_enterprise_ids UUID[] NOT NULL DEFAULT '{}',

    facilitation_percent NUMERIC(5,2) NOT NULL DEFAULT 10.00
        CHECK (facilitation_percent >= 0 AND facilitation_percent <= 20),

    status VARCHAR(20) NOT NULL DEFAULT 'draft' CHECK (status IN (
        'draft', 'published', 'in_discussion', 'signed', 'withdrawn', 'expired'
    )),
    withdrawn_reason TEXT,
    -- What it became.
    outcome_engagement_id UUID REFERENCES team_engagements(id) ON DELETE SET NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT availability_runs_forward CHECK (
        available_until IS NULL OR available_from IS NULL
        OR available_until > available_from
    ),
    CONSTRAINT withdrawal_carries_a_reason CHECK (
        status <> 'withdrawn'
        OR (withdrawn_reason IS NOT NULL AND btrim(withdrawn_reason) <> '')
    )
);

COMMENT ON COLUMN team_proposals.problem_md IS
    'The problem first, in the client''s terms. A proposal that opens with '
    'the solution is a team describing what it wants to build.';

CREATE INDEX idx_proposals_public
    ON team_proposals (created_at DESC)
    WHERE status = 'published' AND cardinality(target_enterprise_ids) = 0;
CREATE INDEX idx_proposals_initiator
    ON team_proposals (initiator_user_id, created_at DESC);

CREATE TRIGGER trg_proposals_updated_at
    BEFORE UPDATE ON team_proposals
    FOR EACH ROW EXECUTE FUNCTION touch_continuous_updated_at();

CREATE TABLE team_proposal_members (
    proposal_id UUID NOT NULL REFERENCES team_proposals(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_on_proposal VARCHAR(120) NOT NULL CHECK (btrim(role_on_proposal) <> ''),
    -- Their own agreement to being named. A proposal listing people who did
    -- not agree is a team assembled on paper, and the client finds out at the
    -- kickoff.
    accepted_at TIMESTAMPTZ,
    declined_at TIMESTAMPTZ,

    PRIMARY KEY (proposal_id, user_id),

    CONSTRAINT not_both_answers CHECK (accepted_at IS NULL OR declined_at IS NULL)
);

COMMENT ON TABLE team_proposal_members IS
    'A proposal listing people who did not agree is a team assembled on '
    'paper, and the client finds out at the kickoff.';

CREATE TABLE proposal_enterprise_interests (
    proposal_id UUID NOT NULL REFERENCES team_proposals(id) ON DELETE CASCADE,
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,

    interested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    note_md TEXT,
    meeting_at TIMESTAMPTZ,
    signed_at TIMESTAMPTZ,
    contract_value NUMERIC(12,2) CHECK (contract_value IS NULL OR contract_value > 0),
    facilitation_fee NUMERIC(10,2)
        CHECK (facilitation_fee IS NULL OR facilitation_fee >= 0),

    PRIMARY KEY (proposal_id, enterprise_id),

    CONSTRAINT a_signature_names_a_value CHECK (
        signed_at IS NULL OR contract_value IS NOT NULL
    ),
    CONSTRAINT a_fee_follows_a_signature CHECK (
        facilitation_fee IS NULL OR signed_at IS NOT NULL
    )
);

CREATE INDEX idx_proposal_interests_enterprise
    ON proposal_enterprise_interests (enterprise_id, interested_at DESC);

-- ═══════════════════════════════════════════════════════════════════
-- The revenue streams and products these feed
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO revenue_streams (slug, pillar, label, description, recurring) VALUES
    ('onboarding_service', 'work', 'Accompagnement à la prise de poste',
     'Trois mois d''accompagnement structuré d''une personne nouvellement '
     'recrutée, facturés à l''employeur.',
     FALSE),
    ('living_lab_subscription', 'work', 'Living lab',
     'Un abonnement mensuel pour ouvrir un produit à la communauté.',
     TRUE),
    ('proposal_facilitation', 'work', 'Mise en relation sur proposition',
     'La part Skilluv sur un contrat signé à partir d''une proposition '
     'formulée par une équipe.',
     FALSE)
ON CONFLICT (slug) DO NOTHING;

UPDATE enterprise_product_types
   SET revenue_stream = 'onboarding_service'
 WHERE slug = 'onboarding_service'
   AND revenue_stream IS DISTINCT FROM 'onboarding_service';

UPDATE enterprise_product_types
   SET revenue_stream = 'living_lab_subscription', recurring = TRUE
 WHERE slug = 'living_lab'
   AND revenue_stream IS DISTINCT FROM 'living_lab_subscription';

INSERT INTO enterprise_product_types
    (slug, label, description, revenue_stream, recurring)
VALUES
    ('team_proposal', 'Proposition d''équipe',
     'Un contrat né d''une proposition formulée par une équipe Skilluv.',
     'proposal_facilitation', FALSE)
ON CONFLICT (slug) DO NOTHING;
