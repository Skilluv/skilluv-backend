-- Business model — the ecosystem line.
-- Migration 0237.
--
-- ## Four certifications, one table
--
-- The backlog names four things somebody pays Skilluv to be called: a
-- certified reviewer, a certified enterprise partner, a certified external
-- studio, and a cyber-competence certificate for a client's team. Written as
-- four tables they would have been four renewal jobs, four expiry checks and
-- four places to forget that a certification lapses.
--
-- They are one object: a subject, a level, a fee, an audit, an issue date and
-- an expiry. What differs is who the subject is, which is a column.
--
-- ## Selling a label is selling trust that is not ours to spend
--
-- A "Skilluv Certified Partner" badge tells a contributor that this company
-- pays fairly and respects what was agreed. If the badge is bought rather
-- than earned, the person it misleads is the contributor — who has no way to
-- know, and who took the job because of it.
--
-- So a certification cannot be issued without a recorded audit and a score,
-- and the fee is booked at issue rather than at order. Paying does not
-- certify; passing does.

-- ═══════════════════════════════════════════════════════════════════
-- Certifications
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE certification_programs (
    slug VARCHAR(40) PRIMARY KEY,
    label VARCHAR(120) NOT NULL,
    description TEXT NOT NULL,
    -- Who is being certified. Decides which of the subject columns is filled
    -- and which audit questions apply.
    subject_kind VARCHAR(20) NOT NULL CHECK (subject_kind IN (
        'person', 'enterprise', 'external_org'
    )),
    -- What it costs a year, as published. A negotiated figure goes on the
    -- certification, never here.
    annual_fee NUMERIC(10,2) NOT NULL CHECK (annual_fee >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    -- How long it lasts before it has to be re-earned. Every certification
    -- expires: one that does not is a statement about the past sold as a
    -- statement about the present.
    valid_months SMALLINT NOT NULL DEFAULT 12 CHECK (valid_months BETWEEN 3 AND 36),
    -- The score an audit has to reach. Out of 100.
    pass_mark NUMERIC(5,2) NOT NULL DEFAULT 70.00
        CHECK (pass_mark > 0 AND pass_mark <= 100),
    revenue_stream VARCHAR(60) NOT NULL REFERENCES revenue_streams(slug),
    is_active BOOLEAN NOT NULL DEFAULT TRUE
);

COMMENT ON COLUMN certification_programs.valid_months IS
    'Every certification expires. One that does not is a statement about the '
    'past sold as a statement about the present.';

INSERT INTO certification_programs
    (slug, label, description, subject_kind, annual_fee, valid_months, pass_mark,
     revenue_stream)
VALUES
    ('certified_reviewer',
     'Relecteur certifié',
     'Un contributeur formé et évalué pour relire le travail des autres sur '
     'les missions payantes. La certification est annuelle et se repasse.',
     'person', 500.00, 12, 75.00, 'certification_program'),
    ('enterprise_partner_bronze',
     'Partenaire certifié — Bronze',
     'Une entreprise dont la relation avec les contributeurs Skilluv a été '
     'auditée : paiements, délais, respect de ce qui a été convenu.',
     'enterprise', 5000.00, 12, 70.00, 'certification_program'),
    ('enterprise_partner_silver',
     'Partenaire certifié — Argent',
     'Audit approfondi, avec entretiens auprès des contributeurs ayant '
     'travaillé pour l''entreprise.',
     'enterprise', 10000.00, 12, 80.00, 'certification_program'),
    ('enterprise_partner_gold',
     'Partenaire certifié — Or',
     'Audit complet et mise en avant publique. Le niveau qui engage le plus '
     'la crédibilité de Skilluv, donc celui qui exige le plus.',
     'enterprise', 15000.00, 12, 90.00, 'certification_program'),
    ('external_studio',
     'Studio certifié',
     'Un studio extérieur formé à la méthode Skilluv — relectures, '
     'attestations — et audité sur son application.',
     'external_org', 5000.00, 12, 75.00, 'certification_program'),
    ('team_security_competence',
     'Compétence sécurité d''équipe',
     'Une évaluation des compétences sécurité d''une équipe cliente, '
     'opposable auprès des assureurs partenaires.',
     'enterprise', 8000.00, 12, 70.00, 'certification_program')
ON CONFLICT (slug) DO NOTHING;

CREATE TABLE certifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    program VARCHAR(40) NOT NULL REFERENCES certification_programs(slug) ON DELETE RESTRICT,

    -- Exactly one of these, matching the programme's subject kind.
    subject_user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    subject_enterprise_id UUID REFERENCES enterprises(id) ON DELETE CASCADE,
    subject_org_name VARCHAR(200),
    subject_org_url VARCHAR(500),

    -- What was actually charged. Defaults from the programme; a negotiated
    -- figure lives here so the published price stays what we publish.
    fee NUMERIC(10,2) NOT NULL CHECK (fee >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    -- Which trades or domains it covers. A reviewer certified in backend is
    -- not thereby certified in security, and a certification that does not
    -- say what it covers covers nothing.
    scope TEXT[] NOT NULL DEFAULT '{}',

    -- The audit. Required before anything is issued: paying does not
    -- certify, passing does.
    audit_score NUMERIC(5,2) CHECK (audit_score IS NULL OR (audit_score >= 0 AND audit_score <= 100)),
    audit_notes TEXT,
    audit_by UUID REFERENCES users(id) ON DELETE SET NULL,
    audited_at TIMESTAMPTZ,

    issued_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    renewals SMALLINT NOT NULL DEFAULT 0 CHECK (renewals >= 0),

    revoked_at TIMESTAMPTZ,
    revoked_reason TEXT,

    status VARCHAR(20) NOT NULL DEFAULT 'requested' CHECK (status IN (
        'requested', 'auditing', 'issued', 'expired', 'failed', 'revoked'
    )),
    failure_reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One subject, named once. Two would make it unclear who is certified.
    CONSTRAINT one_subject CHECK (
        (subject_user_id IS NOT NULL)::int
        + (subject_enterprise_id IS NOT NULL)::int
        + (subject_org_name IS NOT NULL)::int = 1
    ),
    -- The gate. A badge that was bought rather than earned misleads the
    -- contributor who took the job because of it.
    CONSTRAINT nothing_is_issued_without_an_audit CHECK (
        status <> 'issued'
        OR (audited_at IS NOT NULL AND audit_score IS NOT NULL
            AND issued_at IS NOT NULL AND expires_at IS NOT NULL)
    ),
    CONSTRAINT a_failure_carries_a_reason CHECK (
        status <> 'failed'
        OR (failure_reason IS NOT NULL AND btrim(failure_reason) <> '')
    ),
    CONSTRAINT a_revocation_carries_a_reason CHECK (
        status <> 'revoked'
        OR (revoked_reason IS NOT NULL AND btrim(revoked_reason) <> '')
    ),
    CONSTRAINT a_certification_runs_forward CHECK (
        expires_at IS NULL OR issued_at IS NULL OR expires_at > issued_at
    )
);

COMMENT ON TABLE certifications IS
    'One row per thing somebody pays to be called. Four tables would have '
    'been four renewal jobs and four places to forget that a certification '
    'lapses.';

COMMENT ON CONSTRAINT nothing_is_issued_without_an_audit ON certifications IS
    'Paying does not certify; passing does. A bought badge misleads the '
    'contributor who took the job because of it, and they have no way to '
    'know.';

-- One live certification per subject per programme. Two would let somebody
-- hold a lapsed one and a current one and show whichever suits.
CREATE UNIQUE INDEX idx_one_live_certification_per_person
    ON certifications (program, subject_user_id)
    WHERE subject_user_id IS NOT NULL AND status IN ('requested', 'auditing', 'issued');
CREATE UNIQUE INDEX idx_one_live_certification_per_enterprise
    ON certifications (program, subject_enterprise_id)
    WHERE subject_enterprise_id IS NOT NULL AND status IN ('requested', 'auditing', 'issued');

CREATE INDEX idx_certifications_expiring
    ON certifications (expires_at)
    WHERE status = 'issued';

CREATE OR REPLACE FUNCTION touch_ecosystem_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_certifications_updated_at
    BEFORE UPDATE ON certifications
    FOR EACH ROW EXECUTE FUNCTION touch_ecosystem_updated_at();

-- What an enterprise audit actually looked at, so the score can be argued
-- with rather than only believed.
CREATE TABLE certification_audit_findings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    certification_id UUID NOT NULL REFERENCES certifications(id) ON DELETE CASCADE,

    criterion VARCHAR(60) NOT NULL,
    score NUMERIC(5,2) NOT NULL CHECK (score >= 0 AND score <= 100),
    weight NUMERIC(5,2) NOT NULL DEFAULT 1.00 CHECK (weight > 0),
    -- What the score rests on. A criterion scored without evidence is an
    -- opinion with a number on it.
    evidence TEXT NOT NULL CHECK (btrim(evidence) <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (certification_id, criterion)
);

COMMENT ON COLUMN certification_audit_findings.evidence IS
    'Required. A criterion scored without evidence is an opinion with a '
    'number on it.';

-- ═══════════════════════════════════════════════════════════════════
-- The creators marketplace
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE marketplace_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(120) NOT NULL UNIQUE,
    creator_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,

    item_type VARCHAR(40) NOT NULL,
    skill_domain VARCHAR(30) NOT NULL,
    title VARCHAR(200) NOT NULL CHECK (btrim(title) <> ''),
    description_md TEXT NOT NULL CHECK (btrim(description_md) <> ''),

    thumbnail_url VARCHAR(500) NOT NULL CHECK (thumbnail_url ~ '^https://'),
    preview_urls TEXT[] NOT NULL DEFAULT '{}',
    -- Where the buyer's files are. Never handed out directly: a purchase
    -- issues a token and the token expires.
    file_keys TEXT[] NOT NULL CHECK (cardinality(file_keys) > 0),

    license_type VARCHAR(30) NOT NULL CHECK (license_type IN (
        'personal_use', 'commercial', 'extended_commercial'
    )),
    -- What the buyer is allowed to do, in words, alongside the machine
    -- label. A licence nobody can read is a licence nobody follows.
    license_summary TEXT NOT NULL CHECK (btrim(license_summary) <> ''),

    price NUMERIC(10,2) NOT NULL CHECK (price > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    downloads_count INTEGER NOT NULL DEFAULT 0 CHECK (downloads_count >= 0),
    rating_sum INTEGER NOT NULL DEFAULT 0 CHECK (rating_sum >= 0),
    rating_count INTEGER NOT NULL DEFAULT 0 CHECK (rating_count >= 0),
    -- Derived, so the average and its parts cannot disagree.
    rating_avg NUMERIC(3,2)
        GENERATED ALWAYS AS (
            CASE WHEN rating_count = 0 THEN NULL
                 ELSE round(rating_sum::NUMERIC / rating_count, 2)
            END
        ) STORED,

    status VARCHAR(20) NOT NULL DEFAULT 'draft' CHECK (status IN (
        'draft', 'in_review', 'published', 'delisted'
    )),
    delisted_reason TEXT,
    published_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT delisting_carries_a_reason CHECK (
        status <> 'delisted'
        OR (delisted_reason IS NOT NULL AND btrim(delisted_reason) <> '')
    ),
    CONSTRAINT a_published_item_is_dated CHECK (
        status <> 'published' OR published_at IS NOT NULL
    )
);

COMMENT ON COLUMN marketplace_items.rating_avg IS
    'Generated from the sum and the count, so the average and its parts '
    'cannot disagree.';

CREATE INDEX idx_marketplace_published
    ON marketplace_items (skill_domain, published_at DESC)
    WHERE status = 'published';
CREATE INDEX idx_marketplace_creator
    ON marketplace_items (creator_user_id, created_at DESC);

CREATE TRIGGER trg_marketplace_items_updated_at
    BEFORE UPDATE ON marketplace_items
    FOR EACH ROW EXECUTE FUNCTION touch_ecosystem_updated_at();

CREATE TABLE marketplace_purchases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    item_id UUID NOT NULL REFERENCES marketplace_items(id) ON DELETE RESTRICT,

    buyer_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    buyer_enterprise_id UUID REFERENCES enterprises(id) ON DELETE SET NULL,

    -- What was actually charged, frozen. A creator raising their price must
    -- not change what somebody already paid, and a report read next year has
    -- to show the price of the day.
    amount_paid NUMERIC(10,2) NOT NULL CHECK (amount_paid >= 0),
    commission_percent NUMERIC(5,2) NOT NULL
        CHECK (commission_percent >= 0 AND commission_percent <= 30),
    commission_amount NUMERIC(10,2) NOT NULL CHECK (commission_amount >= 0),
    creator_payout NUMERIC(10,2) NOT NULL CHECK (creator_payout >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    -- How the files are fetched. Expires, because a permanent link posted
    -- once is the whole catalogue given away.
    download_token VARCHAR(64) NOT NULL UNIQUE,
    token_expires_at TIMESTAMPTZ NOT NULL,
    downloads_used SMALLINT NOT NULL DEFAULT 0 CHECK (downloads_used >= 0),

    -- One rating per purchase, and only from somebody who bought it.
    rating SMALLINT CHECK (rating IS NULL OR rating BETWEEN 1 AND 5),
    review TEXT,
    rated_at TIMESTAMPTZ,

    refunded_at TIMESTAMPTZ,
    refund_reason TEXT,

    purchased_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT one_buyer CHECK (
        (buyer_user_id IS NOT NULL)::int + (buyer_enterprise_id IS NOT NULL)::int = 1
    ),
    -- The parts add back to what was paid. Anything else is money that went
    -- somewhere nobody can name.
    CONSTRAINT the_split_adds_up CHECK (
        commission_amount + creator_payout = amount_paid
    ),
    CONSTRAINT a_refund_carries_a_reason CHECK (
        refunded_at IS NULL OR (refund_reason IS NOT NULL AND btrim(refund_reason) <> '')
    )
);

COMMENT ON CONSTRAINT the_split_adds_up ON marketplace_purchases IS
    'The commission and the payout add back to what was paid. Anything else '
    'is money that went somewhere nobody can name.';

CREATE INDEX idx_purchases_item ON marketplace_purchases (item_id, purchased_at DESC);
CREATE INDEX idx_purchases_buyer
    ON marketplace_purchases (buyer_user_id, purchased_at DESC)
    WHERE buyer_user_id IS NOT NULL;

-- The rating on a purchase keeps the item's counters in step. Kept in a
-- trigger rather than in the service because two paths write ratings — the
-- buyer's own, and a moderator removing one — and only one of them would
-- have remembered.
CREATE OR REPLACE FUNCTION marketplace_rating_rolls_up()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'UPDATE' AND OLD.rating IS NOT DISTINCT FROM NEW.rating THEN
        RETURN NEW;
    END IF;

    IF OLD.rating IS NOT NULL THEN
        UPDATE marketplace_items
           SET rating_sum = rating_sum - OLD.rating,
               rating_count = rating_count - 1
         WHERE id = NEW.item_id;
    END IF;

    IF NEW.rating IS NOT NULL THEN
        UPDATE marketplace_items
           SET rating_sum = rating_sum + NEW.rating,
               rating_count = rating_count + 1
         WHERE id = NEW.item_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_marketplace_rating_rolls_up
    AFTER UPDATE OF rating ON marketplace_purchases
    FOR EACH ROW EXECUTE FUNCTION marketplace_rating_rolls_up();

-- ═══════════════════════════════════════════════════════════════════
-- Academy cohorts
-- ═══════════════════════════════════════════════════════════════════
--
-- A company pays for a cohort to be trained and commits to hiring from it.
-- Close to the growth financing of migration 0236 and deliberately separate:
-- there the company funds and hopes, here it commits to a share up front and
-- pays a fee per hire. The obligations differ, so the tables do.

CREATE TABLE academy_cohorts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sponsoring_enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,

    name VARCHAR(200) NOT NULL CHECK (btrim(name) <> ''),
    brief_md TEXT NOT NULL CHECK (btrim(brief_md) <> ''),
    skill_domain VARCHAR(30) NOT NULL,
    orientations_target TEXT[] NOT NULL DEFAULT '{}',

    cohort_size SMALLINT NOT NULL CHECK (cohort_size BETWEEN 5 AND 200),
    duration_weeks SMALLINT NOT NULL CHECK (duration_weeks BETWEEN 2 AND 52),

    sponsorship_fee NUMERIC(12,2) NOT NULL CHECK (sponsorship_fee > 0),
    success_fee_per_hire NUMERIC(10,2) NOT NULL DEFAULT 0
        CHECK (success_fee_per_hire >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    -- What share of the cohort the company undertakes to interview. An
    -- undertaking to interview, not to hire: nobody can promise that a
    -- specific person will be wanted, and a promise to hire would be a
    -- promise made about somebody else's judgement.
    interview_top_percent NUMERIC(5,2) NOT NULL DEFAULT 20.00
        CHECK (interview_top_percent > 0 AND interview_top_percent <= 100),

    starts_on DATE,
    graduates_on DATE,

    status VARCHAR(20) NOT NULL DEFAULT 'recruiting' CHECK (status IN (
        'recruiting', 'running', 'placing', 'closed', 'cancelled'
    )),
    closed_reason TEXT,

    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_cohort_runs_forward CHECK (
        graduates_on IS NULL OR starts_on IS NULL OR graduates_on > starts_on
    ),
    CONSTRAINT cancellation_carries_a_reason CHECK (
        status <> 'cancelled'
        OR (closed_reason IS NOT NULL AND btrim(closed_reason) <> '')
    )
);

COMMENT ON COLUMN academy_cohorts.interview_top_percent IS
    'An undertaking to interview, not to hire. Nobody can promise a specific '
    'person will be wanted, and promising it would be a promise about '
    'somebody else''s judgement.';

CREATE INDEX idx_academy_cohorts_enterprise
    ON academy_cohorts (sponsoring_enterprise_id, created_at DESC);
CREATE INDEX idx_academy_cohorts_open
    ON academy_cohorts (created_at DESC)
    WHERE status = 'recruiting';

CREATE TRIGGER trg_academy_cohorts_updated_at
    BEFORE UPDATE ON academy_cohorts
    FOR EACH ROW EXECUTE FUNCTION touch_ecosystem_updated_at();

CREATE TABLE academy_cohort_members (
    cohort_id UUID NOT NULL REFERENCES academy_cohorts(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    graduated_at TIMESTAMPTZ,
    left_at TIMESTAMPTZ,
    -- Where they finished in the cohort. What the interview undertaking is
    -- measured against.
    final_rank SMALLINT CHECK (final_rank IS NULL OR final_rank > 0),

    interviewed_at TIMESTAMPTZ,
    hired_at TIMESTAMPTZ,
    -- The fee charged for this hire, frozen at the moment it happened.
    success_fee_charged NUMERIC(10,2)
        CHECK (success_fee_charged IS NULL OR success_fee_charged >= 0),

    status VARCHAR(20) NOT NULL DEFAULT 'training' CHECK (status IN (
        'training', 'graduated', 'interviewing', 'hired', 'left'
    )),

    PRIMARY KEY (cohort_id, user_id),
    UNIQUE (cohort_id, final_rank),

    -- Nobody is hired out of a cohort without an interview, and nobody is
    -- interviewed before they finish. The sponsor bought a trained cohort,
    -- not first refusal on people mid-course.
    CONSTRAINT hiring_follows_an_interview CHECK (
        hired_at IS NULL OR interviewed_at IS NOT NULL
    ),
    CONSTRAINT interviewing_follows_graduation CHECK (
        interviewed_at IS NULL OR graduated_at IS NOT NULL
    ),
    CONSTRAINT a_fee_follows_a_hire CHECK (
        success_fee_charged IS NULL OR hired_at IS NOT NULL
    )
);

COMMENT ON CONSTRAINT interviewing_follows_graduation ON academy_cohort_members IS
    'The sponsor bought a trained cohort, not first refusal on people '
    'halfway through the course.';

-- ═══════════════════════════════════════════════════════════════════
-- The revenue streams and products these feed
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO enterprise_product_types
    (slug, label, description, revenue_stream, recurring)
VALUES
    ('partner_certification', 'Label partenaire',
     'Un label annuel adossé à un audit de la relation avec les '
     'contributeurs.',
     'certification_program', TRUE),
    ('team_security_certification', 'Certification sécurité d''équipe',
     'Une évaluation des compétences sécurité d''une équipe, opposable '
     'auprès des assureurs.',
     'certification_program', TRUE)
ON CONFLICT (slug) DO NOTHING;
