-- The half of escrow that was never built.
--
-- ─── What existed and what did not ────────────────────────────────
--
-- Migration 0156 gave every flow a release window: seven days during which
-- the person who paid can say the thing they paid for did not happen.
-- `ledger.hold_dispute` freezes the money, `refund_from_dispute` gives it
-- back, `resolve_dispute_for_recipient` hands it over, and the catalogue
-- carries a `funds.disputed` notification.
--
-- None of it is reachable. `release::dispute` has no caller. There is no
-- endpoint through which a mentee can say the session did not happen, so
-- the window is seven days during which nothing can be done, and every hold
-- releases on schedule whatever happened.
--
-- That is worse than having no escrow: the release window is a promise to
-- the payer that they have recourse, and they have none.
--
-- ─── Who may dispute ──────────────────────────────────────────────
--
-- The payer, and only the payer. `pending_releases` records who is owed the
-- money and not who provided it, so there was no way to check. It does now.
--
-- ─── Why the recipient answers first ──────────────────────────────
--
-- A dispute that goes straight to an operator makes every disagreement our
-- problem and is how a marketplace ends up staffing a call centre. So the
-- recipient answers first: conceding refunds immediately and costs nobody
-- anything, and only a genuine disagreement reaches a human.

ALTER TABLE pending_releases
    -- Who paid. Nullable because a hold may be funded by an organisation
    -- rather than a person — a bounty pot, an enterprise budget — in which
    -- case `payer_enterprise_id` carries it instead.
    ADD COLUMN payer_id UUID REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN payer_enterprise_id UUID REFERENCES enterprises(id) ON DELETE SET NULL;

COMMENT ON COLUMN pending_releases.payer_id IS
    'Who funded this hold, and therefore who may dispute it. A hold with '
    'neither payer set cannot be disputed by anyone, which is a bug in the '
    'flow that created it rather than a state to design around.';

CREATE TABLE disputes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- The frozen hold. One dispute per hold: a second would be the same
    -- argument twice, and the money can only be in one place.
    pending_release_id UUID NOT NULL UNIQUE
        REFERENCES pending_releases(id) ON DELETE RESTRICT,

    raised_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    -- In the payer's own words. Required: "disputed" with no reason is
    -- something neither the recipient nor an operator can answer.
    reason TEXT NOT NULL CHECK (length(trim(reason)) >= 10),

    status VARCHAR(20) NOT NULL DEFAULT 'open'
        CHECK (status IN (
            'open',        -- waiting for the recipient to answer
            'contested',   -- the recipient disagrees; a human decides
            'refunded',    -- resolved for the payer; money returned
            'released',    -- resolved for the recipient; money handed over
            'withdrawn'    -- the payer changed their mind
        )),

    -- The recipient's side, when they contest rather than concede.
    recipient_response TEXT,
    responded_at TIMESTAMPTZ,

    -- Who decided, when a human had to. NULL when the recipient conceded or
    -- the payer withdrew, which is the outcome to aim for.
    resolved_by UUID REFERENCES users(id) ON DELETE SET NULL,
    resolution_note TEXT,
    resolved_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A resolved dispute has a resolution time, an open one does not. Every
    -- report of "how long do disputes take" reads these two columns.
    CONSTRAINT disputes_resolution_consistent CHECK (
        (status IN ('open', 'contested') AND resolved_at IS NULL)
        OR (status IN ('refunded', 'released', 'withdrawn') AND resolved_at IS NOT NULL)
    ),
    -- A decision an operator made must say why. The parties read it.
    CONSTRAINT disputes_human_decision_explained CHECK (
        resolved_by IS NULL OR resolution_note IS NOT NULL
    )
);

-- The operator queue: contested, oldest first.
CREATE INDEX idx_disputes_awaiting_decision
    ON disputes (created_at)
    WHERE status = 'contested';

-- "My disputes", from either side.
CREATE INDEX idx_disputes_raised_by ON disputes (raised_by, created_at DESC);

CREATE OR REPLACE FUNCTION touch_disputes_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_disputes_updated_at
    BEFORE UPDATE ON disputes
    FOR EACH ROW EXECUTE FUNCTION touch_disputes_updated_at();

-- ─── The notifications this makes possible ────────────────────────

INSERT INTO notification_kinds
    (kind, category, allows_in_app, allows_push, allows_email,
     default_in_app, default_push, default_email, transactional) VALUES

    -- To the recipient: someone is contesting money you were owed. All
    -- three channels, transactional, because there is a clock on the reply.
    ('dispute.opened',      'payments', TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, TRUE),
    -- To both parties, when it ends. Three kinds rather than one with the
    -- outcome as an argument: an outcome passed as a word is a word in one
    -- language, and it would land untranslated in the middle of a
    -- translated sentence.
    ('dispute.refunded',    'payments', TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, TRUE),
    ('dispute.released',    'payments', TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, TRUE),
    ('dispute.withdrawn',   'payments', TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, TRUE),
    -- To the operators, when the two sides disagree.
    ('dispute.needs_review','admin',    TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, TRUE);

UPDATE notification_kinds
   SET cta_path = CASE kind
       WHEN 'dispute.needs_review' THEN '/admin/disputes'
       ELSE '/wallet/disputes/{dispute_id}'
   END
 WHERE kind LIKE 'dispute.%';
