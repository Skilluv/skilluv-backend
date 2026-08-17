-- Business model — the finance line.
-- Migration 0236.
--
-- ## What makes this section different
--
-- Every other line sells work, attention or data. This one sells money, and
-- money is regulated. Advancing a contributor's invoice, introducing them to
-- a lender, placing them with an insurer, financing a cohort against future
-- salary — each of those is a regulated activity somewhere, and in several
-- places at once for a platform operating between Benin and the EU.
--
-- The backlog says so in a note at the top and then describes the tables. The
-- honest version puts the note in the schema: a partnership cannot be active
-- without a stated regulatory basis, and nothing can be referred through an
-- inactive one. The code is complete and the switch is a document, which is
-- the correct order — the reverse produces a product that ships and then asks
-- whether it was allowed to.
--
-- ## One partnerships table
--
-- A bank we introduce people to and an insurer we introduce people to are the
-- same object: a third party, a commission, a registration that permits the
-- introduction, and a country list. Three tables would have been three places
-- to forget the registration.
--
-- ## The fund is not here
--
-- Ticket 07-07 describes a venture arm as a separate legal entity with its
-- own general partner, limited partners and regulator. A table in Skilluv's
-- database for a fund that legally is not Skilluv would be a fiction, and the
-- first person to read it would believe it. It stays in the strategy
-- documents until the entity exists.

-- ═══════════════════════════════════════════════════════════════════
-- Partnerships
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE financial_partnerships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    partner_org VARCHAR(200) NOT NULL CHECK (btrim(partner_org) <> ''),

    kind VARCHAR(30) NOT NULL CHECK (kind IN (
        'loan',
        -- Professional indemnity: mistakes in delivered work.
        'insurance_professional',
        -- Income protection when a project collapses.
        'insurance_income',
        'insurance_cyber',
        'insurance_health'
    )),

    -- Where the partner is allowed to operate, as ISO country codes. Not a
    -- marketing field: an introduction made outside the partner's licence is
    -- the introduction that gets both of us fined.
    countries CHAR(2)[] NOT NULL CHECK (cardinality(countries) > 0),

    commission_percent NUMERIC(5,2) NOT NULL
        CHECK (commission_percent >= 0 AND commission_percent <= 10),

    -- What permits Skilluv to make the introduction at all: an intermediary
    -- registration number, a licence reference, a written exemption. Free
    -- text because every regulator words it differently, and required
    -- because none of them accept its absence.
    regulatory_basis TEXT,
    -- Where that registration can be checked by somebody who does not take
    -- our word for it.
    registry_url VARCHAR(500),
    contract_url VARCHAR(500),

    -- Who may be introduced. A rank floor because the partner is pricing on
    -- Skilluv's assessment, and an assessment of somebody with no history is
    -- not an assessment.
    min_rank VARCHAR(20),

    status VARCHAR(20) NOT NULL DEFAULT 'draft' CHECK (status IN (
        'draft',
        -- Signed and permitted. The only status that can take referrals.
        'active',
        'suspended',
        'ended'
    )),
    ended_reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- The gate. A live partnership states what permits it, and points at a
    -- signed contract. Without both, no introduction can be made through it.
    CONSTRAINT an_active_partnership_states_its_permission CHECK (
        status <> 'active'
        OR (regulatory_basis IS NOT NULL AND btrim(regulatory_basis) <> ''
            AND contract_url IS NOT NULL)
    ),
    CONSTRAINT ending_carries_a_reason CHECK (
        status <> 'ended'
        OR (ended_reason IS NOT NULL AND btrim(ended_reason) <> '')
    )
);

COMMENT ON TABLE financial_partnerships IS
    'A third party we introduce contributors to for a commission. Banks and '
    'insurers are the same object; three tables would have been three places '
    'to forget the registration.';

COMMENT ON CONSTRAINT an_active_partnership_states_its_permission
    ON financial_partnerships IS
    'Introducing somebody to a lender or an insurer is a regulated act. The '
    'code is complete and the switch is a document — the reverse produces a '
    'product that ships and then asks whether it was allowed to.';

CREATE INDEX idx_partnerships_active
    ON financial_partnerships (kind)
    WHERE status = 'active';

CREATE OR REPLACE FUNCTION touch_finance_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_partnerships_updated_at
    BEFORE UPDATE ON financial_partnerships
    FOR EACH ROW EXECUTE FUNCTION touch_finance_updated_at();

-- ── Referrals ──────────────────────────────────────────────────────
--
-- A loan application and an insurance subscription are the same shape: we
-- introduced this person, the partner decided, this is what Skilluv earns.
-- What differs is whether the commission is once or monthly, which is a
-- column.

CREATE TABLE partnership_referrals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    partnership_id UUID NOT NULL REFERENCES financial_partnerships(id) ON DELETE RESTRICT,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    -- The person's own request, in their words. Nobody is referred to a
    -- lender by an algorithm noticing they might need money.
    purpose TEXT NOT NULL CHECK (btrim(purpose) <> ''),
    amount_requested NUMERIC(12,2) CHECK (amount_requested IS NULL OR amount_requested > 0),
    coverage_requested NUMERIC(12,2)
        CHECK (coverage_requested IS NULL OR coverage_requested > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    -- What Skilluv passed on. Recorded because the partner priced on it, and
    -- because the person is entitled to know what was said about them.
    shared_snapshot JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- Their own agreement to that being shared, which is what makes the
    -- introduction lawful rather than a data transfer.
    consented_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    decision VARCHAR(20) NOT NULL DEFAULT 'pending' CHECK (decision IN (
        'pending', 'approved', 'rejected', 'withdrawn'
    )),
    decision_note TEXT,
    decided_at TIMESTAMPTZ,

    approved_amount NUMERIC(12,2) CHECK (approved_amount IS NULL OR approved_amount > 0),
    monthly_premium NUMERIC(10,2) CHECK (monthly_premium IS NULL OR monthly_premium > 0),

    commission_amount NUMERIC(10,2) CHECK (commission_amount IS NULL OR commission_amount >= 0),
    commission_booked_at TIMESTAMPTZ,

    started_on DATE,
    ends_on DATE,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_decision_is_dated CHECK (
        decision = 'pending' OR decided_at IS NOT NULL
    ),
    -- Nothing is earned on a refusal. Booking a commission against a
    -- rejected application is how an introduction business starts referring
    -- people it knows will be turned down.
    CONSTRAINT nothing_is_earned_on_a_refusal CHECK (
        commission_amount IS NULL OR decision = 'approved'
    ),
    CONSTRAINT an_approval_says_what_was_granted CHECK (
        decision <> 'approved'
        OR approved_amount IS NOT NULL
        OR monthly_premium IS NOT NULL
    )
);

COMMENT ON CONSTRAINT nothing_is_earned_on_a_refusal ON partnership_referrals IS
    'Booking a commission against a rejected application is how an '
    'introduction business starts referring people it knows will be turned '
    'down.';

CREATE INDEX idx_referrals_user ON partnership_referrals (user_id, created_at DESC);
CREATE INDEX idx_referrals_pending
    ON partnership_referrals (partnership_id, created_at)
    WHERE decision = 'pending';

CREATE TRIGGER trg_referrals_updated_at
    BEFORE UPDATE ON partnership_referrals
    FOR EACH ROW EXECUTE FUNCTION touch_finance_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- Advance pay
-- ═══════════════════════════════════════════════════════════════════
--
-- Skilluv fronts part of what a contributor is owed on work already
-- delivered, and takes it back when the client pays. Not a loan: the money
-- exists, it is in escrow, and the advance is against a specific invoice.
--
-- That distinction is why this one can exist without a banking partner, and
-- why the schema will not let it drift into a loan: an advance points at one
-- invoice, cannot exceed it, and repays from it.

CREATE TABLE advance_pay_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,

    -- The invoice it is advanced against. One, named, and required — an
    -- advance against "upcoming work" is a loan wearing a different word.
    invoice_id UUID NOT NULL REFERENCES mission_invoices(id) ON DELETE RESTRICT,

    expected_payment NUMERIC(12,2) NOT NULL CHECK (expected_payment > 0),
    advance_percent NUMERIC(5,2) NOT NULL
        CHECK (advance_percent >= 30 AND advance_percent <= 90),
    advance_amount NUMERIC(12,2) NOT NULL CHECK (advance_amount > 0),
    fee_percent NUMERIC(5,2) NOT NULL DEFAULT 4.00
        CHECK (fee_percent >= 0 AND fee_percent <= 8),
    fee_amount NUMERIC(10,2) NOT NULL CHECK (fee_amount >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    status VARCHAR(20) NOT NULL DEFAULT 'requested' CHECK (status IN (
        'requested',
        'approved',
        'refused',
        'disbursed',
        -- The client paid and the advance came back out of it.
        'repaid',
        -- The client never paid. Skilluv carries it; the contributor does
        -- not, which is the entire point of the fee.
        'written_off'
    )),
    refusal_reason TEXT,

    approved_at TIMESTAMPTZ,
    disbursed_at TIMESTAMPTZ,
    repaid_at TIMESTAMPTZ,
    written_off_at TIMESTAMPTZ,
    written_off_reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One live advance per invoice. Two would advance more than the invoice
    -- is worth and there would be nothing to repay the second from.
    CONSTRAINT an_advance_fits_its_invoice CHECK (advance_amount <= expected_payment),
    CONSTRAINT a_refusal_carries_a_reason CHECK (
        status <> 'refused'
        OR (refusal_reason IS NOT NULL AND btrim(refusal_reason) <> '')
    ),
    CONSTRAINT a_write_off_carries_a_reason CHECK (
        status <> 'written_off'
        OR (written_off_reason IS NOT NULL AND btrim(written_off_reason) <> '')
    ),
    CONSTRAINT disbursement_is_dated CHECK (
        status NOT IN ('disbursed', 'repaid') OR disbursed_at IS NOT NULL
    )
);

COMMENT ON TABLE advance_pay_requests IS
    'Money already owed, paid early. Not a loan: it points at one invoice, '
    'cannot exceed it, and repays from it. That is what keeps it outside '
    'credit regulation, so the schema enforces it rather than trusting it.';

CREATE UNIQUE INDEX idx_one_live_advance_per_invoice
    ON advance_pay_requests (invoice_id)
    WHERE status IN ('requested', 'approved', 'disbursed');

CREATE INDEX idx_advances_user ON advance_pay_requests (user_id, created_at DESC);
CREATE INDEX idx_advances_outstanding
    ON advance_pay_requests (disbursed_at)
    WHERE status = 'disbursed';

CREATE TRIGGER trg_advances_updated_at
    BEFORE UPDATE ON advance_pay_requests
    FOR EACH ROW EXECUTE FUNCTION touch_finance_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- The payment guarantee
-- ═══════════════════════════════════════════════════════════════════
--
-- A contributor pays a small monthly fee; if a client disputes and refuses to
-- pay for work Skilluv reviewed and accepted, Skilluv pays anyway and chases
-- the client itself.
--
-- Both caps matter. Per mission, so one large engagement cannot exhaust the
-- scheme; per year, so the scheme cannot be arbitraged by somebody taking
-- work they expect to be disputed.

CREATE TABLE payment_guarantee_subscriptions (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    tier VARCHAR(20) NOT NULL CHECK (tier IN ('basic', 'premium')),

    monthly_fee NUMERIC(8,2) NOT NULL CHECK (monthly_fee > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    max_per_mission NUMERIC(12,2) NOT NULL CHECK (max_per_mission > 0),
    annual_cap NUMERIC(12,2) NOT NULL CHECK (annual_cap > 0),

    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    cancelled_at TIMESTAMPTZ,
    auto_renew BOOLEAN NOT NULL DEFAULT TRUE,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_subscription_runs_forward CHECK (expires_at > started_at),
    CONSTRAINT the_annual_cap_covers_at_least_one_mission CHECK (
        annual_cap >= max_per_mission
    )
);

COMMENT ON CONSTRAINT the_annual_cap_covers_at_least_one_mission
    ON payment_guarantee_subscriptions IS
    'An annual cap below the per-mission limit sells a guarantee that cannot '
    'be claimed once in full.';

-- What was actually paid out under the guarantee, and whether it came back.
CREATE TABLE payment_guarantee_claims (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    invoice_id UUID REFERENCES mission_invoices(id) ON DELETE SET NULL,

    amount NUMERIC(12,2) NOT NULL CHECK (amount > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    -- The year the claim counts against, for the annual cap. Stored because
    -- a claim opened in December and paid in January belongs to December.
    counts_for_year SMALLINT NOT NULL CHECK (counts_for_year BETWEEN 2025 AND 2100),

    reason TEXT NOT NULL CHECK (btrim(reason) <> ''),
    status VARCHAR(20) NOT NULL DEFAULT 'opened' CHECK (status IN (
        'opened', 'paid', 'refused',
        -- Skilluv chased the client and got it back.
        'recovered'
    )),
    refusal_reason TEXT,
    paid_at TIMESTAMPTZ,
    recovered_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_refusal_carries_a_reason CHECK (
        status <> 'refused'
        OR (refusal_reason IS NOT NULL AND btrim(refusal_reason) <> '')
    )
);

CREATE INDEX idx_guarantee_claims_year
    ON payment_guarantee_claims (user_id, counts_for_year);

-- ═══════════════════════════════════════════════════════════════════
-- Growth financing
-- ═══════════════════════════════════════════════════════════════════
--
-- A company funds a cohort's training; Skilluv runs it and places the best
-- of them with that company. The backlog calls it a reversed income share
-- agreement, and the reversal is the important half: the trainee owes
-- nothing, ever, whatever happens.
--
-- An income share agreement that a trainee can owe is a debt taken on by
-- somebody with no income, which is the arrangement that has ruined the
-- reputation of every bootcamp that tried it. The schema has no column for a
-- trainee obligation, and that absence is deliberate.

CREATE TABLE growth_financing_programs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    name VARCHAR(200) NOT NULL CHECK (btrim(name) <> ''),
    brief_md TEXT NOT NULL CHECK (btrim(brief_md) <> ''),

    cohort_size SMALLINT NOT NULL CHECK (cohort_size BETWEEN 5 AND 200),
    duration_months SMALLINT NOT NULL CHECK (duration_months BETWEEN 1 AND 24),
    total_investment NUMERIC(12,2) NOT NULL CHECK (total_investment > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    -- What the company expects out of it. A floor rather than a promise:
    -- Skilluv does not guarantee that people will accept a job.
    hires_expected_min SMALLINT NOT NULL CHECK (hires_expected_min >= 0),
    orchestration_fee NUMERIC(10,2) NOT NULL DEFAULT 0
        CHECK (orchestration_fee >= 0),

    -- What happens to the trainees who are not hired: nothing. They keep the
    -- training and owe nobody. Stated as a column so the answer is in the
    -- data rather than in a conversation.
    unplaced_owe_nothing BOOLEAN NOT NULL DEFAULT TRUE
        CHECK (unplaced_owe_nothing),

    status VARCHAR(20) NOT NULL DEFAULT 'briefing' CHECK (status IN (
        'briefing', 'recruiting', 'running', 'placing', 'closed', 'cancelled'
    )),
    closed_reason TEXT,
    started_at TIMESTAMPTZ,
    closed_at TIMESTAMPTZ,

    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT the_expected_hires_fit_the_cohort CHECK (
        hires_expected_min <= cohort_size
    ),
    CONSTRAINT cancellation_carries_a_reason CHECK (
        status <> 'cancelled'
        OR (closed_reason IS NOT NULL AND btrim(closed_reason) <> '')
    )
);

COMMENT ON COLUMN growth_financing_programs.unplaced_owe_nothing IS
    'True, always, and checked. An income share agreement a trainee can owe '
    'is a debt taken on by somebody with no income — the arrangement that '
    'ruined the reputation of every bootcamp that tried it.';

CREATE INDEX idx_growth_programs_enterprise
    ON growth_financing_programs (enterprise_id, created_at DESC);

CREATE TRIGGER trg_growth_programs_updated_at
    BEFORE UPDATE ON growth_financing_programs
    FOR EACH ROW EXECUTE FUNCTION touch_finance_updated_at();

CREATE TABLE growth_financing_trainees (
    program_id UUID NOT NULL REFERENCES growth_financing_programs(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    left_at TIMESTAMPTZ,

    -- Offered a job by the funding company, and their own answer to it.
    offered_at TIMESTAMPTZ,
    accepted_at TIMESTAMPTZ,
    declined_at TIMESTAMPTZ,

    status VARCHAR(20) NOT NULL DEFAULT 'training' CHECK (status IN (
        'training', 'completed', 'hired', 'left'
    )),

    PRIMARY KEY (program_id, user_id),

    CONSTRAINT not_both_answers CHECK (accepted_at IS NULL OR declined_at IS NULL),
    -- Declining the job is a normal outcome and costs nothing. The company
    -- funded the training; it did not buy the person.
    CONSTRAINT nobody_is_hired_without_accepting CHECK (
        status <> 'hired' OR accepted_at IS NOT NULL
    )
);

COMMENT ON CONSTRAINT nobody_is_hired_without_accepting ON growth_financing_trainees IS
    'Declining is a normal outcome and costs nothing. The company funded the '
    'training; it did not buy the person.';

-- ═══════════════════════════════════════════════════════════════════
-- The revenue streams these feed
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO revenue_streams (slug, pillar, label, description, recurring) VALUES
    ('payment_guarantee_fee', 'finance', 'Garantie de paiement',
     'L''abonnement mensuel d''un contributeur à la garantie de paiement.',
     TRUE)
ON CONFLICT (slug) DO NOTHING;

INSERT INTO enterprise_product_types
    (slug, label, description, revenue_stream, recurring)
VALUES
    ('growth_financing', 'Financement de promotion',
     'Une promotion formée aux frais d''une entreprise, qui recrute ensuite '
     'parmi elle.',
     'growth_financing_isa', FALSE)
ON CONFLICT (slug) DO NOTHING;
