-- The talent posts, and companies come to them.
--
-- ## Why this is worth building rather than being a gimmick
--
-- The ordinary direction has the company describe a role and the person
-- explain why they fit. It rewards whoever writes the best application, which
-- is a skill and not the one being hired for.
--
-- Reversed, the person states what they want — role, pay, where, from when —
-- and companies argue why they should be the ones. It puts the burden of
-- persuasion on the side with the budget, which is where it belongs, and it
-- only works on a platform where the person's work is already checkable.
-- Anywhere else the company would be pitching to a CV.
--
-- ## Who may post
--
-- Not everybody. A posting is a claim on the attention of every company on
-- the platform, and the argument for reversing the direction is that the
-- person's work speaks for itself — which requires that some of it exists.
-- The rank threshold is the platform's existing answer to "has this person
-- done enough to be taken at their word", so it is the one used here rather
-- than a second, separate bar.

CREATE TABLE reverse_recruitment_postings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- One at a time. Two postings from one person would be two answers to
    -- "what are you looking for", and a company would not know which is
    -- current.
    talent_user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,

    title VARCHAR(200) NOT NULL CHECK (btrim(title) <> ''),
    -- What they want to do, in their words rather than from a list.
    desired_role VARCHAR(200) NOT NULL CHECK (btrim(desired_role) <> ''),
    desired_domain VARCHAR(30) NOT NULL,
    desired_orientations TEXT[] NOT NULL DEFAULT '{}',
    -- {"min": 1200000, "max": 1800000, "currency": "XOF", "period": "year"}.
    -- Optional, and its absence is a position rather than an omission: some
    -- people would rather hear an offer first.
    desired_salary_range JSONB,
    remote_only BOOLEAN NOT NULL DEFAULT FALSE,
    preferred_countries TEXT[] NOT NULL DEFAULT '{}',
    available_from DATE NOT NULL,
    -- What they are not looking for. The section that saves the most wasted
    -- pitches, and the one people forget to write.
    not_looking_for TEXT,

    -- The spam control, and the reason the whole thing stays usable. Without
    -- a ceiling, a posting from a strong profile becomes an inbox nobody
    -- opens, and the feature dies of its own success.
    max_pitches_per_month SMALLINT NOT NULL DEFAULT 10
        CHECK (max_pitches_per_month BETWEEN 1 AND 50),

    status VARCHAR(20) NOT NULL DEFAULT 'active' CHECK (status IN (
        'active',
        -- Still listed, not taking pitches. For somebody in the middle of a
        -- conversation who does not want to start ten more.
        'paused',
        'closed'
    )),
    closed_reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT salary_range_is_an_object
        CHECK (desired_salary_range IS NULL
               OR jsonb_typeof(desired_salary_range) = 'object')
);

COMMENT ON TABLE reverse_recruitment_postings IS
    'What somebody is looking for, stated by them. Puts the burden of '
    'persuasion on the side with the budget, which only works where the '
    'person''s work is already checkable.';

COMMENT ON COLUMN reverse_recruitment_postings.max_pitches_per_month IS
    'The reason this stays usable. Without a ceiling, a strong profile''s '
    'posting becomes an inbox nobody opens and the feature dies of its own '
    'success.';

CREATE INDEX idx_reverse_postings_open
    ON reverse_recruitment_postings (desired_domain, available_from)
    WHERE status = 'active';
CREATE INDEX idx_reverse_postings_orientations
    ON reverse_recruitment_postings USING GIN (desired_orientations);

CREATE TRIGGER trg_reverse_postings_updated_at
    BEFORE UPDATE ON reverse_recruitment_postings
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

CREATE TABLE reverse_recruitment_pitches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    posting_id UUID NOT NULL REFERENCES reverse_recruitment_postings(id) ON DELETE CASCADE,
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    sent_by UUID REFERENCES users(id) ON DELETE SET NULL,

    -- The argument. Required and long enough to have taken effort: the whole
    -- premise is that the company does the persuading, and a two-line pitch
    -- is the company asking the person to do it instead.
    pitch_md TEXT NOT NULL CHECK (length(btrim(pitch_md)) >= 200),
    -- What they are offering. Optional, and its absence is visible to the
    -- person reading — a pitch with no figure against a posting that named
    -- one is a choice the reader can weigh.
    offered_salary NUMERIC(14,2) CHECK (offered_salary IS NULL OR offered_salary > 0),
    currency CHAR(3) CHECK (currency IS NULL OR currency IN ('EUR', 'XOF', 'USD')),
    -- What it costs the company to send. Higher than an ordinary contact:
    -- the opportunity is rarer and the ceiling makes each one scarce.
    credits_spent SMALLINT NOT NULL DEFAULT 4 CHECK (credits_spent > 0),

    status VARCHAR(20) NOT NULL DEFAULT 'sent' CHECK (status IN (
        'sent',
        'read',
        'interested',
        'declined',
        'hired'
    )),
    read_at TIMESTAMPTZ,
    responded_at TIMESTAMPTZ,
    -- Optional, and asked for rather than required: somebody declining ten
    -- pitches should not have to justify each one.
    decline_reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One pitch per company per posting. A second one is a follow-up, which
    -- belongs in a conversation rather than in a new pitch that costs credits
    -- and reopens the counter.
    UNIQUE (posting_id, enterprise_id),

    CONSTRAINT money_carries_its_currency CHECK ((offered_salary IS NULL) = (currency IS NULL))
);

COMMENT ON TABLE reverse_recruitment_pitches IS
    'A company arguing for itself. The minimum length is deliberate: a '
    'two-line pitch is the company asking the person to do the persuading '
    'after all.';

CREATE INDEX idx_reverse_pitches_posting
    ON reverse_recruitment_pitches (posting_id, created_at DESC);
CREATE INDEX idx_reverse_pitches_enterprise
    ON reverse_recruitment_pitches (enterprise_id, created_at DESC);

-- ═══════════════════════════════════════════════════════════════════
-- The ceiling, enforced where it cannot be gone round
-- ═══════════════════════════════════════════════════════════════════
--
-- In the database rather than the service, because the count has to be taken
-- and the row written without a gap between them. Two companies pitching at
-- the same moment against the last remaining slot would both pass a service
-- check and both insert.

CREATE OR REPLACE FUNCTION pitch_respects_the_ceiling()
RETURNS TRIGGER AS $$
DECLARE
    posting RECORD;
    this_month INTEGER;
BEGIN
    SELECT status, max_pitches_per_month INTO posting
      FROM reverse_recruitment_postings
     WHERE id = NEW.posting_id
       FOR UPDATE;

    IF posting.status <> 'active' THEN
        RAISE EXCEPTION 'this posting is %, not taking pitches', posting.status;
    END IF;

    SELECT count(*) INTO this_month
      FROM reverse_recruitment_pitches
     WHERE posting_id = NEW.posting_id
       AND created_at > date_trunc('month', NOW());

    IF this_month >= posting.max_pitches_per_month THEN
        RAISE EXCEPTION 'this posting has taken its % pitches this month',
            posting.max_pitches_per_month
            USING HINT = 'the ceiling is what keeps the inbox readable — try next month';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_pitch_respects_the_ceiling
    BEFORE INSERT ON reverse_recruitment_pitches
    FOR EACH ROW EXECUTE FUNCTION pitch_respects_the_ceiling();

-- ═══════════════════════════════════════════════════════════════════
-- What this has to tell people
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO notification_kinds
    (kind, category, allows_in_app, allows_push, allows_email,
     default_in_app, default_push, default_email, transactional) VALUES
    -- To the talent: a company argued for itself. Not transactional — there
    -- is no clock — but on every channel, because the whole point is that
    -- these are rare and worth reading.
    ('reverse_recruitment.pitch_received', 'enterprise',
     TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, FALSE),
    -- To the company, when the answer comes. Transactional: they spent
    -- credits and are owed the outcome.
    ('reverse_recruitment.pitch_answered', 'enterprise',
     TRUE, TRUE, TRUE, TRUE, FALSE, TRUE, TRUE);

UPDATE notification_kinds
   SET cta_path = CASE kind
       WHEN 'reverse_recruitment.pitch_received' THEN '/me/pitches'
       ELSE '/enterprise/reverse-recruitment'
   END
 WHERE kind LIKE 'reverse_recruitment.%';
