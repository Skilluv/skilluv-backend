-- Business model — the remaining products.
-- Migration 0241.
--
-- ## Four of the seven already have machinery
--
-- The newsletter is an `audience_plans` row: the table exists, it was built
-- for replay access, and a paid newsletter is the same object — an individual
-- paying Skilluv monthly for something that is not visibility.
--
-- Rank as a service is a scope on the existing metered API, not a second API.
-- One key, one quota, one usage table; a parallel one would mean two places
-- to enforce consent, and consent is the whole product.
--
-- Consulting is a third `consultations` kind. An advisory is an hour, a
-- review is a panel, an implementation is weeks — same object, same experts,
-- same commission machinery.
--
-- Media sponsorship is `event_sponsored_content` without the event. The
-- column was already nullable.
--
-- What is left needs tables: long placements, corporate learning seats, and
-- open calls for proposals.

-- ═══════════════════════════════════════════════════════════════════
-- The newsletter, as a plan
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO audience_plans
    (slug, label, description, price, currency, period, revenue_stream)
VALUES
    ('newsletter_premium', 'Lettre — édition complète',
     'Les données de rémunération, les tendances du marché tech africain et '
     'ce que nous voyons passer côté recrutement. L''édition gratuite reste '
     'gratuite.',
     8.00, 'EUR', 'monthly', 'newsletter_subscription')
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Consulting, as a third kind of consultation
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE consultations
    DROP CONSTRAINT consultations_kind_check;

ALTER TABLE consultations
    ADD CONSTRAINT consultations_kind_check CHECK (kind IN (
        'advisory',
        'architecture_review',
        -- Weeks of work helping a company build something internally: an
        -- apprenticeship programme, a skills framework, a proof-of-work
        -- practice. Same experts, same panel machinery, longer clock.
        'implementation'
    ));

ALTER TABLE consultations
    ADD COLUMN implementation_type VARCHAR(40) CHECK (
        implementation_type IS NULL OR implementation_type IN (
            'compagnonnage_setup',
            'apprenticeship_program_design',
            'tech_talent_strategy',
            'skills_framework_design',
            'proof_of_work_implementation'
        )
    ),
    ADD COLUMN duration_weeks SMALLINT
        CHECK (duration_weeks IS NULL OR duration_weeks BETWEEN 1 AND 52),
    ADD COLUMN deliverables JSONB NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE consultations
    ADD CONSTRAINT an_implementation_says_what_and_how_long CHECK (
        kind <> 'implementation'
        OR (implementation_type IS NOT NULL AND duration_weeks IS NOT NULL)
    );

COMMENT ON COLUMN consultations.implementation_type IS
    'What a company is being helped to build. An implementation is weeks of '
    'work, not an hour, and the type decides which experts belong on it.';

-- ═══════════════════════════════════════════════════════════════════
-- Media sponsorship, without an event
-- ═══════════════════════════════════════════════════════════════════
--
-- `event_sponsored_content` already had a nullable event. What it lacked was
-- the shapes that are not an event recap, and somewhere to record how many
-- people saw it — which is what a sponsor is actually buying.

ALTER TABLE event_sponsored_content
    DROP CONSTRAINT event_sponsored_content_content_type_check;

ALTER TABLE event_sponsored_content
    ADD CONSTRAINT event_sponsored_content_content_type_check CHECK (
        content_type IN (
            'blog_post', 'video', 'newsletter', 'podcast', 'recap',
            -- Not tied to an event.
            'youtube_video', 'podcast_episode', 'guide', 'case_study'
        )
    );

ALTER TABLE event_sponsored_content
    ADD COLUMN impressions INTEGER NOT NULL DEFAULT 0 CHECK (impressions >= 0),
    -- Where the sponsor's own claim about reach can be checked against ours.
    ADD COLUMN impressions_source VARCHAR(40);

COMMENT ON COLUMN event_sponsored_content.impressions IS
    'What the sponsor is actually buying. Reported with its source so their '
    'figure and ours can be compared rather than argued about.';

-- ═══════════════════════════════════════════════════════════════════
-- Long placements
-- ═══════════════════════════════════════════════════════════════════
--
-- A hire with two years of Skilluv attached: a fee up front, a monthly
-- monitoring charge, and a replacement if the person leaves inside the
-- guarantee.
--
-- The replacement is the part worth being careful with. It is a promise made
-- to a company about a person, and the person is not a component: the
-- guarantee replaces the *service*, and nothing here obliges anybody to stay.

CREATE TABLE long_term_placements (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    junior_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,

    duration_months SMALLINT NOT NULL DEFAULT 24
        CHECK (duration_months BETWEEN 6 AND 60),
    annual_salary_declared NUMERIC(14,2) NOT NULL
        CHECK (annual_salary_declared > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    upfront_fee NUMERIC(12,2) NOT NULL CHECK (upfront_fee >= 0),
    monthly_monitoring_fee NUMERIC(10,2) NOT NULL DEFAULT 0
        CHECK (monthly_monitoring_fee >= 0),
    -- Who accompanies them for the duration. The monitoring fee buys their
    -- time; a placement charging for monitoring with nobody assigned is
    -- charging for nothing.
    mentor_user_id UUID REFERENCES users(id) ON DELETE SET NULL,

    guarantee_months SMALLINT NOT NULL DEFAULT 12
        CHECK (guarantee_months BETWEEN 0 AND 24),

    started_on DATE,
    ended_on DATE,
    -- Why it ended. The person leaving is one of several reasons and is not
    -- assumed: a company that restructures has not been let down.
    ended_reason VARCHAR(30) CHECK (ended_reason IS NULL OR ended_reason IN (
        'completed', 'person_left', 'company_ended', 'mutual', 'dismissed'
    )),
    replacement_of UUID REFERENCES long_term_placements(id) ON DELETE SET NULL,

    status VARCHAR(20) NOT NULL DEFAULT 'proposed' CHECK (status IN (
        'proposed', 'active', 'completed', 'ended_early', 'declined'
    )),

    -- The person's own agreement, as everywhere a person is the subject.
    junior_accepted_at TIMESTAMPTZ,
    junior_declined_at TIMESTAMPTZ,

    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT not_both_answers CHECK (
        junior_accepted_at IS NULL OR junior_declined_at IS NULL
    ),
    CONSTRAINT nothing_starts_without_the_person CHECK (
        status <> 'active' OR junior_accepted_at IS NOT NULL
    ),
    CONSTRAINT monitoring_needs_somebody_doing_it CHECK (
        monthly_monitoring_fee = 0 OR mentor_user_id IS NOT NULL
    ),
    CONSTRAINT ending_says_why CHECK (
        status NOT IN ('completed', 'ended_early') OR ended_reason IS NOT NULL
    ),
    CONSTRAINT a_placement_runs_forward CHECK (
        ended_on IS NULL OR started_on IS NULL OR ended_on >= started_on
    ),
    CONSTRAINT nobody_mentors_themselves CHECK (
        mentor_user_id IS NULL OR mentor_user_id <> junior_user_id
    )
);

COMMENT ON TABLE long_term_placements IS
    'A hire with two years of Skilluv attached. The guarantee replaces the '
    'service, not the person: nothing here obliges anybody to stay.';

COMMENT ON CONSTRAINT monitoring_needs_somebody_doing_it ON long_term_placements IS
    'A placement charging a monitoring fee with nobody assigned is charging '
    'for nothing.';

CREATE INDEX idx_placements_enterprise
    ON long_term_placements (enterprise_id, created_at DESC);
CREATE INDEX idx_placements_person
    ON long_term_placements (junior_user_id, created_at DESC);
CREATE INDEX idx_placements_billable
    ON long_term_placements (started_on)
    WHERE status = 'active' AND monthly_monitoring_fee > 0;

CREATE OR REPLACE FUNCTION touch_additional_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_placements_updated_at
    BEFORE UPDATE ON long_term_placements
    FOR EACH ROW EXECUTE FUNCTION touch_additional_updated_at();

-- The monthly monitoring charge, one row per month. Charged for months that
-- were actually monitored, so a placement that ends in March is not billed
-- for April.
CREATE TABLE placement_monitoring_months (
    placement_id UUID NOT NULL REFERENCES long_term_placements(id) ON DELETE CASCADE,
    counts_for_month DATE NOT NULL,
    amount NUMERIC(10,2) NOT NULL CHECK (amount >= 0),
    billed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (placement_id, counts_for_month),

    CONSTRAINT a_month_is_its_first_day CHECK (EXTRACT(DAY FROM counts_for_month) = 1)
);

-- ═══════════════════════════════════════════════════════════════════
-- Corporate learning
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE corporate_learning_plans (
    slug VARCHAR(30) PRIMARY KEY,
    label VARCHAR(80) NOT NULL,
    monthly_fee_per_seat NUMERIC(8,2) NOT NULL CHECK (monthly_fee_per_seat > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    features TEXT[] NOT NULL CHECK (cardinality(features) > 0),
    sort_order SMALLINT NOT NULL DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT TRUE
);

INSERT INTO corporate_learning_plans
    (slug, label, monthly_fee_per_seat, features, sort_order)
VALUES
    ('essentials', 'Essentiel', 10.00,
     ARRAY['challenges_access', 'community'], 1),
    ('professional', 'Professionnel', 30.00,
     ARRAY['challenges_access', 'community', 'mentors_access', 'attestations'], 2),
    ('enterprise', 'Entreprise', 100.00,
     ARRAY['challenges_access', 'community', 'mentors_access', 'attestations',
           'custom_curriculums', 'private_cohorts'], 3)
ON CONFLICT (slug) DO NOTHING;

CREATE TABLE corporate_learning_subscriptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    plan VARCHAR(30) NOT NULL REFERENCES corporate_learning_plans(slug),

    seats SMALLINT NOT NULL CHECK (seats BETWEEN 1 AND 5000),
    -- Frozen at subscription, so a price change does not rewrite what a
    -- company is already paying.
    monthly_fee_per_seat NUMERIC(8,2) NOT NULL CHECK (monthly_fee_per_seat > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    current_period_end TIMESTAMPTZ NOT NULL,
    cancelled_at TIMESTAMPTZ,
    auto_renew BOOLEAN NOT NULL DEFAULT TRUE,
    stripe_subscription_id VARCHAR(80),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_period_runs_forward CHECK (current_period_end > started_at)
);

CREATE UNIQUE INDEX idx_one_live_learning_subscription
    ON corporate_learning_subscriptions (enterprise_id)
    WHERE cancelled_at IS NULL;

CREATE TABLE corporate_learning_seats (
    subscription_id UUID NOT NULL REFERENCES corporate_learning_subscriptions(id)
        ON DELETE CASCADE,
    employee_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    invited_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Their own act. A seat assigned and never taken is not a user, and
    -- counting it as one would let a company report engagement it does not
    -- have.
    activated_at TIMESTAMPTZ,
    released_at TIMESTAMPTZ,

    PRIMARY KEY (subscription_id, employee_user_id)
);

COMMENT ON COLUMN corporate_learning_seats.activated_at IS
    'Their own act. A seat assigned and never taken is not a user, and '
    'counting it as one would let a company report engagement it does not '
    'have.';

CREATE INDEX idx_learning_seats_user
    ON corporate_learning_seats (employee_user_id)
    WHERE released_at IS NULL;

-- A subscription cannot hand out more seats than it bought.
CREATE OR REPLACE FUNCTION learning_subscription_has_seats()
RETURNS TRIGGER AS $$
DECLARE
    bought SMALLINT;
    used INTEGER;
BEGIN
    SELECT seats INTO bought
      FROM corporate_learning_subscriptions
     WHERE id = NEW.subscription_id FOR UPDATE;

    SELECT count(*) INTO used
      FROM corporate_learning_seats
     WHERE subscription_id = NEW.subscription_id AND released_at IS NULL;

    IF used >= bought THEN
        RAISE EXCEPTION 'this subscription has % seats and they are all taken', bought;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_learning_seats_available
    BEFORE INSERT ON corporate_learning_seats
    FOR EACH ROW EXECUTE FUNCTION learning_subscription_has_seats();

-- ═══════════════════════════════════════════════════════════════════
-- Open calls for proposals
-- ═══════════════════════════════════════════════════════════════════
--
-- A company describes an outcome it wants and the community proposes how.
-- Different from a contest, where everybody does the work and one wins:
-- here nobody does the work until somebody is chosen, which is the honest
-- shape for anything larger than a weekend.

CREATE TABLE open_rfps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(120) NOT NULL UNIQUE,
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,

    title VARCHAR(200) NOT NULL CHECK (btrim(title) <> ''),
    context_md TEXT NOT NULL CHECK (length(btrim(context_md)) >= 100),
    desired_outcome_md TEXT NOT NULL CHECK (length(btrim(desired_outcome_md)) >= 50),

    -- A range, published. A call for proposals with no budget wastes the time
    -- of everybody whose answer would have been "not for that".
    budget_min NUMERIC(12,2) NOT NULL CHECK (budget_min > 0),
    budget_max NUMERIC(12,2) NOT NULL CHECK (budget_max > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    proposal_deadline TIMESTAMPTZ NOT NULL,
    -- When the company undertakes to have chosen. A call with no end is a
    -- pile of unpaid proposals nobody ever answers.
    selection_deadline TIMESTAMPTZ NOT NULL,

    visibility VARCHAR(20) NOT NULL DEFAULT 'public' CHECK (visibility IN (
        'public', 'invited_only'
    )),
    facilitation_fee NUMERIC(10,2) NOT NULL DEFAULT 0
        CHECK (facilitation_fee >= 0),

    winner_proposal_id UUID,
    outcome_engagement_id UUID REFERENCES team_engagements(id) ON DELETE SET NULL,

    status VARCHAR(20) NOT NULL DEFAULT 'open' CHECK (status IN (
        'open', 'reviewing', 'awarded', 'cancelled', 'expired'
    )),
    cancelled_reason TEXT,

    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_budget_range_runs_forward CHECK (budget_max >= budget_min),
    CONSTRAINT selection_follows_proposals CHECK (
        selection_deadline > proposal_deadline
    ),
    CONSTRAINT cancellation_carries_a_reason CHECK (
        status <> 'cancelled'
        OR (cancelled_reason IS NOT NULL AND btrim(cancelled_reason) <> '')
    )
);

COMMENT ON COLUMN open_rfps.budget_min IS
    'Published as a range. A call for proposals with no budget wastes the '
    'time of everybody whose answer would have been "not for that".';

COMMENT ON COLUMN open_rfps.selection_deadline IS
    'When the company undertakes to have chosen. A call with no end is a pile '
    'of unpaid proposals nobody ever answers.';

CREATE INDEX idx_rfps_open
    ON open_rfps (proposal_deadline)
    WHERE status = 'open' AND visibility = 'public';
CREATE INDEX idx_rfps_enterprise ON open_rfps (enterprise_id, created_at DESC);

CREATE TRIGGER trg_rfps_updated_at
    BEFORE UPDATE ON open_rfps
    FOR EACH ROW EXECUTE FUNCTION touch_additional_updated_at();

CREATE TABLE rfp_proposals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    rfp_id UUID NOT NULL REFERENCES open_rfps(id) ON DELETE CASCADE,

    proposer_user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    proposer_studio_id UUID REFERENCES studios(id) ON DELETE CASCADE,

    pitch_md TEXT NOT NULL CHECK (length(btrim(pitch_md)) >= 100),
    approach_md TEXT NOT NULL CHECK (length(btrim(approach_md)) >= 100),
    estimated_price NUMERIC(12,2) NOT NULL CHECK (estimated_price > 0),
    estimated_weeks SMALLINT NOT NULL CHECK (estimated_weeks BETWEEN 1 AND 104),
    credentials TEXT[] NOT NULL DEFAULT '{}',

    submitted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Every proposal gets an answer, and a refusal carries a reason. People
    -- wrote these for nothing; silence is the one thing that is not owed to
    -- them.
    decided_at TIMESTAMPTZ,
    selected BOOLEAN NOT NULL DEFAULT FALSE,
    decision_note TEXT,

    CONSTRAINT one_proposer CHECK (
        (proposer_user_id IS NOT NULL)::int + (proposer_studio_id IS NOT NULL)::int = 1
    ),
    CONSTRAINT a_decision_says_something CHECK (
        decided_at IS NULL
        OR selected
        OR (decision_note IS NOT NULL AND btrim(decision_note) <> '')
    )
);

COMMENT ON CONSTRAINT a_decision_says_something ON rfp_proposals IS
    'People wrote these for nothing. Silence is the one thing that is not '
    'owed to them.';

CREATE UNIQUE INDEX idx_one_proposal_per_person
    ON rfp_proposals (rfp_id, proposer_user_id)
    WHERE proposer_user_id IS NOT NULL;
CREATE UNIQUE INDEX idx_one_proposal_per_studio
    ON rfp_proposals (rfp_id, proposer_studio_id)
    WHERE proposer_studio_id IS NOT NULL;

CREATE INDEX idx_rfp_proposals_rfp ON rfp_proposals (rfp_id, submitted_at);

ALTER TABLE open_rfps
    ADD CONSTRAINT open_rfps_winner_fkey
        FOREIGN KEY (winner_proposal_id) REFERENCES rfp_proposals(id) ON DELETE SET NULL;

-- An award cannot happen while proposals are still unanswered. The company
-- has what it wants at that point, and the others are the ones left waiting.
CREATE OR REPLACE FUNCTION rfp_awards_only_when_everyone_has_an_answer()
RETURNS TRIGGER AS $$
DECLARE
    pending INTEGER;
BEGIN
    IF NEW.status <> 'awarded' OR OLD.status = 'awarded' THEN
        RETURN NEW;
    END IF;

    SELECT count(*) INTO pending
      FROM rfp_proposals
     WHERE rfp_id = NEW.id AND decided_at IS NULL;

    IF pending > 0 THEN
        RAISE EXCEPTION '% proposals have had no answer', pending;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_rfp_award_gate
    BEFORE UPDATE OF status ON open_rfps
    FOR EACH ROW EXECUTE FUNCTION rfp_awards_only_when_everyone_has_an_answer();

-- ═══════════════════════════════════════════════════════════════════
-- The revenue streams and products these feed
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO revenue_streams (slug, pillar, label, description, recurring) VALUES
    ('long_term_placement', 'talent', 'Placement longue durée',
     'Un recrutement accompagné sur deux ans : frais initiaux et suivi '
     'mensuel.',
     TRUE),
    ('corporate_learning', 'ecosystem', 'Abonnement formation entreprise',
     'Un accès Skilluv ouvert aux salariés d''une entreprise, au siège.',
     TRUE),
    ('rfp_facilitation', 'work', 'Appel à propositions',
     'Ce que Skilluv facture pour ouvrir un appel à propositions à la '
     'communauté et en tenir le calendrier.',
     FALSE)
ON CONFLICT (slug) DO NOTHING;

INSERT INTO enterprise_product_types
    (slug, label, description, revenue_stream, recurring)
VALUES
    ('long_term_placement', 'Placement longue durée',
     'Un recrutement accompagné sur la durée, avec garantie de remplacement.',
     'long_term_placement', TRUE),
    ('corporate_learning', 'Formation continue',
     'Des sièges Skilluv ouverts aux salariés.',
     'corporate_learning', TRUE),
    ('open_rfp', 'Appel à propositions',
     'Un besoin ouvert à la communauté, qui propose son approche.',
     'rfp_facilitation', FALSE),
    ('media_sponsorship', 'Contenu média sponsorisé',
     'Un article, une vidéo ou un épisode financé par une entreprise, hors '
     'événement.',
     'media_sponsor_content', FALSE)
ON CONFLICT (slug) DO NOTHING;

UPDATE enterprise_product_types
   SET revenue_stream = 'consulting_fee'
 WHERE slug = 'consulting_engagement'
   AND revenue_stream IS DISTINCT FROM 'consulting_fee';
