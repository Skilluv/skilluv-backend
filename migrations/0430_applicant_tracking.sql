-- The applicant tracker, and the product type that stops lying.
--
-- ## What was wrong
--
-- `enterprise_product_types` carried `subscription_pipeline` — "un accès
-- mensuel au suivi de candidatures" — with nothing behind it. No opening, no
-- candidate, no stage. A company could have been invoiced monthly for a
-- feature that did not exist, and the registry is exactly the place where
-- that mistake is invisible: it looks like a product because it is a row next
-- to forty-three real ones.
--
-- It is replaced here, not deleted and left absent: the thing it named is
-- built below, and the new type says what it actually is.
--
-- ## Why an ATS at all
--
-- A startup in Cotonou or Dakar hiring its first three developers has inbound
-- of its own — a form, a mailbox, a friend of a friend — and no budget for
-- Greenhouse. Without somewhere to put those people, the Skilluv shortlist is
-- a fifth tab they copy names out of. The tracker is what makes Skilluv the
-- place the whole hire happens rather than one source among several.
--
-- ## The part that needs saying out loud
--
-- This table holds personal data about people who never signed up to Skilluv.
-- That is new: everything else here is about somebody with an account who
-- agreed to be here.
--
-- So three rules, in the schema rather than in a policy document:
--
--   * **the enterprise is the controller, Skilluv is the processor.** The
--     rows belong to the company that entered them, and deleting their
--     subscription deletes them;
--   * **nothing is kept forever.** Every candidate carries the date after
--     which they are erased, defaulted from the subscription's retention and
--     enforced by a sweep. An ATS that quietly becomes a permanent CV
--     database is the thing candidates fear about applying;
--   * **a refusal carries a reason.** The same rule `mission_applications`
--     already holds: somebody who spent an hour on an application is owed a
--     sentence, and this platform does not sell the tooling that makes
--     silence easy.

-- ═══════════════════════════════════════════════════════════════════
-- The plans
-- ═══════════════════════════════════════════════════════════════════
--
-- Rows, like every other price on this platform. The free tier is real and
-- generous enough to hire with: a company that cannot afford the tool is
-- exactly the company this product exists for, and a free tier that only
-- fits a demo teaches them to keep using a spreadsheet.

CREATE TABLE ats_plans (
    slug VARCHAR(30) PRIMARY KEY,
    label VARCHAR(80) NOT NULL,
    -- NULL is unlimited, reserved for a negotiated tier.
    max_open_positions INTEGER CHECK (max_open_positions IS NULL OR max_open_positions > 0),
    max_candidates_per_opening INTEGER
        CHECK (max_candidates_per_opening IS NULL OR max_candidates_per_opening > 0),
    -- Search credits bundled in, so the tracker and the talent search are one
    -- purchase rather than two conversations.
    included_credits INTEGER NOT NULL DEFAULT 0 CHECK (included_credits >= 0),
    monthly_fee NUMERIC(10,2) NOT NULL CHECK (monthly_fee >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    -- How long a candidate's record survives after the opening closes, unless
    -- the candidate is a Skilluv user who can see and delete their own row.
    retention_days SMALLINT NOT NULL DEFAULT 180
        CHECK (retention_days BETWEEN 30 AND 730),
    sort_order SMALLINT NOT NULL DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT TRUE
);

INSERT INTO ats_plans
    (slug, label, max_open_positions, max_candidates_per_opening,
     included_credits, monthly_fee, retention_days, sort_order)
VALUES
    ('ats_free', 'Gratuit', 3, 100, 0, 0.00, 180, 1),
    ('ats_starter', 'Starter', 10, 500, 20, 49.00, 365, 2),
    ('ats_growth', 'Growth', 40, 2000, 100, 199.00, 365, 3),
    ('ats_scale', 'Scale', NULL, NULL, 400, 499.00, 730, 4)
ON CONFLICT (slug) DO NOTHING;

COMMENT ON TABLE ats_plans IS
    'What the tracker costs, as rows. The free tier is meant to be hired '
    'with: a company that cannot afford the tool is the company this product '
    'exists for.';

COMMENT ON COLUMN ats_plans.retention_days IS
    'How long a candidate record survives after its opening closes. Longer on '
    'paid tiers because a real pipeline is revisited, and capped at two years '
    'everywhere because an ATS that never forgets is a CV database nobody '
    'consented to.';

-- ═══════════════════════════════════════════════════════════════════
-- The subscription
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE ats_subscriptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    plan VARCHAR(30) NOT NULL REFERENCES ats_plans(slug) ON DELETE RESTRICT,
    -- The product row this was sold as, when it was sold rather than claimed
    -- on the free tier. NULL for `ats_free`, which nobody invoices.
    product_id UUID REFERENCES enterprise_products(id) ON DELETE SET NULL,

    -- `pending` is a plan chosen and not yet paid for. It is a real state
    -- rather than an absence: a company that picked Growth and abandoned the
    -- checkout has told us something, and reading it as "no tracker" would
    -- lose it. The free tier never passes through it — there is nothing to
    -- pay, so choosing it is having it.
    status VARCHAR(20) NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'active', 'past_due', 'cancelled')),
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    renews_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One tracker per company. Two would mean two pipelines for one hiring
    -- process, which is the problem the product solves.
    UNIQUE (enterprise_id),

    -- A cancellation with no date is a subscription nobody can tell the state
    -- of, and a date with no cancellation claims one that never happened.
    CONSTRAINT a_cancellation_says_when CHECK (
        (status = 'cancelled') = (cancelled_at IS NOT NULL)
    )
);

CREATE INDEX idx_ats_subscriptions_renewal
    ON ats_subscriptions (renews_at)
    WHERE status = 'active' AND renews_at IS NOT NULL;

CREATE TRIGGER trg_ats_subscriptions_updated_at
    BEFORE UPDATE ON ats_subscriptions
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- The openings
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE ats_openings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,

    title VARCHAR(200) NOT NULL CHECK (btrim(title) <> ''),
    description_md TEXT NOT NULL DEFAULT '',
    -- Which trade, so a Skilluv shortlist can be pushed into it without
    -- somebody retyping what they are looking for.
    orientation_id UUID REFERENCES orientations(id) ON DELETE SET NULL,
    positions_count SMALLINT NOT NULL DEFAULT 1 CHECK (positions_count > 0),
    remote_ok BOOLEAN NOT NULL DEFAULT TRUE,
    location VARCHAR(120),

    -- The salary range, said up front. Not required, and the absence is
    -- visible: a company that will not publish a range has told candidates
    -- something, and the tool should not hide it.
    salary_min NUMERIC(10,2) CHECK (salary_min IS NULL OR salary_min >= 0),
    salary_max NUMERIC(10,2) CHECK (salary_max IS NULL OR salary_max >= 0),
    salary_currency CHAR(3) CHECK (salary_currency IS NULL
                                   OR salary_currency IN ('EUR', 'XOF', 'USD')),

    status VARCHAR(20) NOT NULL DEFAULT 'open'
        CHECK (status IN ('draft', 'open', 'closed', 'cancelled')),
    opened_at TIMESTAMPTZ,
    closed_at TIMESTAMPTZ,

    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_salary_range_runs_upward CHECK (
        salary_min IS NULL OR salary_max IS NULL OR salary_max >= salary_min
    ),
    CONSTRAINT a_salary_figure_names_its_currency CHECK (
        (salary_min IS NULL AND salary_max IS NULL) OR salary_currency IS NOT NULL
    )
);

CREATE INDEX idx_ats_openings_enterprise
    ON ats_openings (enterprise_id, status, created_at DESC);

CREATE TRIGGER trg_ats_openings_updated_at
    BEFORE UPDATE ON ats_openings
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- The stages
-- ═══════════════════════════════════════════════════════════════════
--
-- Per opening rather than per company, because a company hiring a designer
-- and a database administrator does not run the same process, and a shared
-- stage list would force one of the two to lie about where somebody is.
--
-- Seeded with a default set when an opening is created, and editable after:
-- the point of an ATS somebody actually uses is that it bends to how they
-- already hire.

CREATE TABLE ats_stages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    opening_id UUID NOT NULL REFERENCES ats_openings(id) ON DELETE CASCADE,
    name VARCHAR(60) NOT NULL CHECK (btrim(name) <> ''),
    position SMALLINT NOT NULL CHECK (position >= 0),
    -- The two ends of the pipeline, marked so a report can say "hired" and
    -- "refused" without matching on a name somebody renamed.
    is_terminal_hired BOOLEAN NOT NULL DEFAULT FALSE,
    is_terminal_rejected BOOLEAN NOT NULL DEFAULT FALSE,

    UNIQUE (opening_id, position),

    CONSTRAINT a_stage_is_not_both_ends CHECK (
        NOT (is_terminal_hired AND is_terminal_rejected)
    )
);

CREATE INDEX idx_ats_stages_opening ON ats_stages (opening_id, position);

-- ═══════════════════════════════════════════════════════════════════
-- The candidates
-- ═══════════════════════════════════════════════════════════════════
--
-- Two kinds, and the difference matters more than it looks. A Skilluv talent
-- arrives with proofs a reader can open and a profile they control. Somebody
-- who applied through the company's own form arrives as a name, an address
-- and a file — data this platform is holding on behalf of somebody else, for
-- a person who never agreed to be here.
--
-- Both are candidates. Only one of them can be looked up, and only the other
-- one needs an expiry date defended in the schema.

CREATE TABLE ats_candidates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    opening_id UUID NOT NULL REFERENCES ats_openings(id) ON DELETE CASCADE,

    -- Set when the candidate is on Skilluv. Their proofs are read live from
    -- their profile rather than copied here: a snapshot of somebody's work
    -- goes stale, and a revoked attestation must stop counting everywhere.
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,

    -- Set when they are not. Kept minimal on purpose: a name to address them
    -- by and a way to reply.
    external_name VARCHAR(160),
    external_email VARCHAR(320),
    resume_url VARCHAR(500) CHECK (resume_url IS NULL OR resume_url ~ '^https://'),

    source VARCHAR(30) NOT NULL DEFAULT 'inbound' CHECK (source IN (
        -- They applied to the company directly.
        'inbound',
        -- Pushed across from a Skilluv shortlist or search.
        'skilluv_shortlist',
        -- Somebody in the company added them.
        'sourced',
        'referral'
    )),

    current_stage_id UUID REFERENCES ats_stages(id) ON DELETE SET NULL,
    -- Written when they reach a rejecting stage, and required there. The
    -- constraint lives on the history row below, where the transition is.
    rejected_at TIMESTAMPTZ,
    hired_at TIMESTAMPTZ,

    -- The date after which this row is erased. Defaulted from the plan when
    -- the candidate is created; a sweep does the deleting.
    erase_after DATE NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A candidate is a person on Skilluv or a person who is not, never
    -- neither: a row with no name and no account is a pipeline entry nobody
    -- can contact.
    CONSTRAINT a_candidate_is_somebody CHECK (
        user_id IS NOT NULL
        OR (external_name IS NOT NULL AND btrim(external_name) <> '')
    ),

    -- The same person twice in one opening is a data-entry mistake, not two
    -- candidates. Only enforceable for the ones we can identify.
    CONSTRAINT hired_and_rejected_are_exclusive CHECK (
        hired_at IS NULL OR rejected_at IS NULL
    )
);

CREATE UNIQUE INDEX uniq_ats_candidate_user_per_opening
    ON ats_candidates (opening_id, user_id)
    WHERE user_id IS NOT NULL;

CREATE UNIQUE INDEX uniq_ats_candidate_email_per_opening
    ON ats_candidates (opening_id, lower(external_email))
    WHERE external_email IS NOT NULL;

CREATE INDEX idx_ats_candidates_opening
    ON ats_candidates (opening_id, current_stage_id);

CREATE INDEX idx_ats_candidates_erasure ON ats_candidates (erase_after);

CREATE TRIGGER trg_ats_candidates_updated_at
    BEFORE UPDATE ON ats_candidates
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

COMMENT ON COLUMN ats_candidates.erase_after IS
    'When this record is deleted, whatever the company does. An ATS that '
    'never forgets is a CV database nobody consented to, and this platform '
    'is the processor rather than the owner of these rows.';

COMMENT ON COLUMN ats_candidates.user_id IS
    'Set when the candidate is on Skilluv. Their proofs are read live from '
    'their profile and never copied here: a snapshot goes stale, and a '
    'revoked attestation has to stop counting everywhere at once.';

-- ═══════════════════════════════════════════════════════════════════
-- Every move, with its reason
-- ═══════════════════════════════════════════════════════════════════
--
-- A pipeline whose history is a single `current_stage` column cannot answer
-- the question a hiring process is judged on: where do people fall out, and
-- how long did each of them wait. It also loses the reason.

CREATE TABLE ats_candidate_moves (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    candidate_id UUID NOT NULL REFERENCES ats_candidates(id) ON DELETE CASCADE,
    from_stage_id UUID REFERENCES ats_stages(id) ON DELETE SET NULL,
    to_stage_id UUID NOT NULL REFERENCES ats_stages(id) ON DELETE CASCADE,

    -- Required when the destination refuses somebody. Enforced in the
    -- service, which knows whether the stage is a rejecting one; the column
    -- is here so the reason cannot be lost once written.
    reason TEXT,
    moved_by UUID REFERENCES users(id) ON DELETE SET NULL,
    moved_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ats_moves_candidate
    ON ats_candidate_moves (candidate_id, moved_at);

COMMENT ON TABLE ats_candidate_moves IS
    'Every stage change, with who made it and why. A pipeline with only a '
    'current stage cannot say where people fall out or how long they waited, '
    'which is the only thing that improves a hiring process.';

-- ═══════════════════════════════════════════════════════════════════
-- The product type says what it is now
-- ═══════════════════════════════════════════════════════════════════
--
-- `subscription_pipeline` named a monthly access to candidate tracking that
-- did not exist. Anything already sold under it — nothing, at the time of
-- writing, and the UPDATE below is what makes that safe to assume — carries
-- over to the type that now has a tracker behind it.

INSERT INTO enterprise_product_types (slug, label, description, revenue_stream, recurring)
VALUES
    ('ats_subscription', 'Suivi de candidatures',
     'Un abonnement mensuel au suivi de candidatures : postes ouverts, '
     'pipeline, historique des décisions, et les crédits de recherche qui '
     'vont avec.',
     'other', TRUE)
ON CONFLICT (slug) DO NOTHING;

UPDATE enterprise_products
   SET product_type = 'ats_subscription'
 WHERE product_type = 'subscription_pipeline';

DELETE FROM enterprise_product_types WHERE slug = 'subscription_pipeline';
