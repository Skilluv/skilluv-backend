-- Business model — the consultation line.
-- Migration 0239.
--
-- ## Two of these are the same product
--
-- An hour with one expert on a question, and a panel of seniors reviewing an
-- architecture document, are both "a company buys expert judgement on a
-- question it has written down". What differs is how many people answer and
-- whether the answer is a conversation or a written synthesis. Both are
-- columns.
--
-- ## The third is not, and it is the one to be careful with
--
-- A skill audit assesses the client's own employees. The person being
-- assessed is not the customer, did not ask for it, and will be managed
-- according to the result — possibly out of a job.
--
-- So the schema requires that each assessed person is told, and that what was
-- concluded about them is theirs to see. An assessment somebody is the
-- subject of and cannot read is a file kept on them, and Skilluv is not in
-- that business.

-- ═══════════════════════════════════════════════════════════════════
-- Consultations
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE consultations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,

    kind VARCHAR(30) NOT NULL CHECK (kind IN (
        -- One expert, one call, one question.
        'advisory',
        -- A panel reading a document and writing a synthesis.
        'architecture_review'
    )),

    topic VARCHAR(200) NOT NULL CHECK (btrim(topic) <> ''),
    -- The question, written down before anybody is booked. An advisory call
    -- with no stated question is an hour of both people working out what the
    -- hour is for.
    question_md TEXT NOT NULL CHECK (btrim(question_md) <> ''),
    skill_domain VARCHAR(30) NOT NULL,
    orientation_slug VARCHAR(80),

    -- Advisory.
    duration_minutes SMALLINT CHECK (duration_minutes IS NULL OR duration_minutes IN (30, 60, 120)),
    scheduled_at TIMESTAMPTZ,

    -- Architecture review.
    document_url VARCHAR(500) CHECK (document_url IS NULL OR document_url ~ '^https://'),
    review_deadline TIMESTAMPTZ,
    reviewers_wanted SMALLINT
        CHECK (reviewers_wanted IS NULL OR reviewers_wanted BETWEEN 2 AND 12),
    synthesis_md TEXT,
    synthesis_delivered_at TIMESTAMPTZ,

    fee NUMERIC(12,2) NOT NULL CHECK (fee > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    -- What Skilluv keeps. Higher on advisory, where the whole product is the
    -- introduction; lower on a review, where the experts do days of reading.
    commission_percent NUMERIC(5,2) NOT NULL
        CHECK (commission_percent >= 0 AND commission_percent <= 50),

    status VARCHAR(20) NOT NULL DEFAULT 'requested' CHECK (status IN (
        'requested', 'matching', 'scheduled', 'in_review', 'delivered', 'cancelled'
    )),
    cancelled_reason TEXT,

    rating SMALLINT CHECK (rating IS NULL OR rating BETWEEN 1 AND 5),
    rated_at TIMESTAMPTZ,

    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT an_advisory_is_a_call_of_a_stated_length CHECK (
        kind <> 'advisory' OR duration_minutes IS NOT NULL
    ),
    CONSTRAINT a_review_has_a_document_and_a_deadline CHECK (
        kind <> 'architecture_review'
        OR (document_url IS NOT NULL AND review_deadline IS NOT NULL
            AND reviewers_wanted IS NOT NULL)
    ),
    -- A review is not delivered until the synthesis exists. The synthesis is
    -- what the client bought; the comments are the working.
    CONSTRAINT a_delivered_review_has_its_synthesis CHECK (
        kind <> 'architecture_review'
        OR status <> 'delivered'
        OR (synthesis_md IS NOT NULL AND btrim(synthesis_md) <> ''
            AND synthesis_delivered_at IS NOT NULL)
    ),
    CONSTRAINT cancellation_carries_a_reason CHECK (
        status <> 'cancelled'
        OR (cancelled_reason IS NOT NULL AND btrim(cancelled_reason) <> '')
    )
);

COMMENT ON TABLE consultations IS
    'A company buying expert judgement on a written question. One expert on a '
    'call or a panel on a document: the same product with a different number '
    'of people answering.';

COMMENT ON COLUMN consultations.question_md IS
    'Written down before anybody is booked. An advisory call with no stated '
    'question is an hour of both people working out what the hour is for.';

CREATE INDEX idx_consultations_enterprise
    ON consultations (enterprise_id, created_at DESC);
CREATE INDEX idx_consultations_open
    ON consultations (kind, created_at DESC)
    WHERE status IN ('requested', 'matching');

CREATE OR REPLACE FUNCTION touch_consultation_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_consultations_updated_at
    BEFORE UPDATE ON consultations
    FOR EACH ROW EXECUTE FUNCTION touch_consultation_updated_at();

-- The experts on it. One row for an advisory, several for a review.
CREATE TABLE consultation_experts (
    consultation_id UUID NOT NULL REFERENCES consultations(id) ON DELETE CASCADE,
    expert_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    invited_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Their own answer. An expert's name attached to advice they did not
    -- agree to give is the platform speaking for them.
    accepted_at TIMESTAMPTZ,
    declined_at TIMESTAMPTZ,
    declined_reason TEXT,

    -- What they wrote. Required before they are paid: the fee buys the
    -- opinion, not the availability.
    comment_md TEXT,
    verdict VARCHAR(30) CHECK (verdict IS NULL OR verdict IN (
        'approve', 'approve_with_concerns', 'concerns', 'reject'
    )),
    submitted_at TIMESTAMPTZ,

    share NUMERIC(10,2) CHECK (share IS NULL OR share >= 0),
    paid_at TIMESTAMPTZ,

    PRIMARY KEY (consultation_id, expert_user_id),

    CONSTRAINT not_both_answers CHECK (accepted_at IS NULL OR declined_at IS NULL),
    CONSTRAINT a_submission_says_something CHECK (
        submitted_at IS NULL
        OR (comment_md IS NOT NULL AND btrim(comment_md) <> '')
    ),
    -- Nobody is paid for a slot they did not fill. The fee buys the opinion.
    CONSTRAINT nothing_is_paid_without_a_submission CHECK (
        paid_at IS NULL OR submitted_at IS NOT NULL
    )
);

COMMENT ON CONSTRAINT nothing_is_paid_without_a_submission ON consultation_experts IS
    'The fee buys the opinion, not the availability.';

CREATE INDEX idx_consultation_experts_user
    ON consultation_experts (expert_user_id, invited_at DESC);

-- ═══════════════════════════════════════════════════════════════════
-- Skill audits
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE enterprise_skill_audits (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,

    scope VARCHAR(200) NOT NULL CHECK (btrim(scope) <> ''),
    -- What the client says they will do with it. Not decoration: the people
    -- assessed are told this sentence, and it is the difference between a
    -- development plan and a redundancy list.
    stated_purpose TEXT NOT NULL CHECK (length(btrim(stated_purpose)) >= 20),

    employees_count SMALLINT NOT NULL CHECK (employees_count > 0),
    domains_assessed TEXT[] NOT NULL CHECK (cardinality(domains_assessed) > 0),
    orientations_assessed TEXT[] NOT NULL DEFAULT '{}',
    methodology TEXT[] NOT NULL DEFAULT '{challenges,code_review}'
        CHECK (cardinality(methodology) > 0),

    duration_weeks SMALLINT NOT NULL DEFAULT 3 CHECK (duration_weeks BETWEEN 1 AND 12),
    fee NUMERIC(12,2) NOT NULL CHECK (fee > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    matrix_url VARCHAR(500),
    recommendations_md TEXT,
    delivered_at TIMESTAMPTZ,

    status VARCHAR(20) NOT NULL DEFAULT 'briefing' CHECK (status IN (
        'briefing', 'assessing', 'delivered', 'cancelled'
    )),
    cancelled_reason TEXT,

    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_delivered_audit_has_a_matrix CHECK (
        status <> 'delivered'
        OR (matrix_url IS NOT NULL AND matrix_url ~ '^https://'
            AND delivered_at IS NOT NULL)
    ),
    CONSTRAINT cancellation_carries_a_reason CHECK (
        status <> 'cancelled'
        OR (cancelled_reason IS NOT NULL AND btrim(cancelled_reason) <> '')
    )
);

COMMENT ON COLUMN enterprise_skill_audits.stated_purpose IS
    'Shown to every person assessed. It is the difference between a '
    'development plan and a redundancy list, and they are entitled to know '
    'which one they are in.';

CREATE INDEX idx_skill_audits_enterprise
    ON enterprise_skill_audits (enterprise_id, created_at DESC);

CREATE TRIGGER trg_skill_audits_updated_at
    BEFORE UPDATE ON enterprise_skill_audits
    FOR EACH ROW EXECUTE FUNCTION touch_consultation_updated_at();

CREATE TABLE enterprise_employee_assessments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    audit_id UUID NOT NULL REFERENCES enterprise_skill_audits(id) ON DELETE CASCADE,

    -- The employee. An email rather than an account, because they work for
    -- the client and may never have heard of Skilluv.
    employee_email VARCHAR(255) NOT NULL CHECK (position('@' IN employee_email) > 1),
    employee_name VARCHAR(120),
    -- Set if they turn out to have a Skilluv account, so the assessment can
    -- reach them without going through their employer.
    matched_user_id UUID REFERENCES users(id) ON DELETE SET NULL,

    orientation_slug VARCHAR(80) NOT NULL,
    assessed_level VARCHAR(20) CHECK (assessed_level IS NULL OR assessed_level IN (
        'junior', 'mid', 'senior', 'principal'
    )),
    strengths TEXT[] NOT NULL DEFAULT '{}',
    gaps TEXT[] NOT NULL DEFAULT '{}',
    notes_md TEXT,

    -- The person was told this was happening, and when. Nothing is written
    -- about somebody who does not know they are being assessed.
    informed_at TIMESTAMPTZ,
    -- And they can read what was concluded. An assessment somebody is the
    -- subject of and cannot see is a file kept on them.
    shared_with_employee_at TIMESTAMPTZ,
    employee_response_md TEXT,

    assessed_by UUID REFERENCES users(id) ON DELETE SET NULL,
    assessed_at TIMESTAMPTZ,

    UNIQUE (audit_id, employee_email, orientation_slug),

    CONSTRAINT nobody_is_assessed_without_being_told CHECK (
        assessed_at IS NULL OR informed_at IS NOT NULL
    ),
    CONSTRAINT a_response_follows_a_sharing CHECK (
        employee_response_md IS NULL OR shared_with_employee_at IS NOT NULL
    )
);

COMMENT ON CONSTRAINT nobody_is_assessed_without_being_told
    ON enterprise_employee_assessments IS
    'Nothing is written about somebody who does not know they are being '
    'assessed. Their employer is the customer; they are not.';

COMMENT ON COLUMN enterprise_employee_assessments.employee_response_md IS
    'What the person said about the assessment of them. Kept alongside it, '
    'because a conclusion with no right of reply is a verdict.';

CREATE INDEX idx_assessments_audit ON enterprise_employee_assessments (audit_id);
CREATE INDEX idx_assessments_matched
    ON enterprise_employee_assessments (matched_user_id)
    WHERE matched_user_id IS NOT NULL;

-- An audit cannot be delivered while somebody in it has not been shown what
-- was concluded about them. Held in the database because the commercial
-- pressure is to deliver on the client's date.
CREATE OR REPLACE FUNCTION audit_delivers_only_when_everyone_has_seen_it()
RETURNS TRIGGER AS $$
DECLARE
    unseen INTEGER;
BEGIN
    IF NEW.status <> 'delivered' OR OLD.status = 'delivered' THEN
        RETURN NEW;
    END IF;

    SELECT count(*) INTO unseen
      FROM enterprise_employee_assessments
     WHERE audit_id = NEW.id
       AND assessed_at IS NOT NULL
       AND shared_with_employee_at IS NULL;

    IF unseen > 0 THEN
        RAISE EXCEPTION
            '% assessed people have not been shown what was written about them',
            unseen;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_audit_delivery_gate
    BEFORE UPDATE OF status ON enterprise_skill_audits
    FOR EACH ROW EXECUTE FUNCTION audit_delivers_only_when_everyone_has_seen_it();

-- ═══════════════════════════════════════════════════════════════════
-- The products these feed
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO enterprise_product_types
    (slug, label, description, revenue_stream, recurring)
VALUES
    ('advisory_call', 'Consultation ponctuelle',
     'Une heure avec un expert sur une question écrite à l''avance.',
     'consulting_fee', FALSE),
    ('architecture_review', 'Revue d''architecture',
     'Un document soumis à un panel de seniors, avec synthèse écrite.',
     'consulting_fee', FALSE),
    ('skill_audit', 'Audit de compétences',
     'Une évaluation des compétences réelles d''une équipe interne.',
     'consulting_fee', FALSE)
ON CONFLICT (slug) DO NOTHING;
