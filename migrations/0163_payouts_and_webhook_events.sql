-- The lifecycle of a payout, and the events that resolve it.
--
-- ─── What was missing ─────────────────────────────────────────────
--
-- `services::payout::send` records the movement in the ledger, asks the
-- provider, and reverses the record if the provider refuses on the spot.
-- That covers exactly one failure mode: an immediate no.
--
-- It is not the common one. Mobile Money and FedaPay both answer `pending`
-- and settle minutes or hours later, over a callback. Stripe answers
-- `completed` for a transfer between balances and can still fail the payout
-- to the recipient's bank days afterwards. In every one of those cases the
-- ledger says the money left, the recipient's balance is down, and nothing
-- in the system is listening for the answer.
--
-- There was no webhook endpoint for payments at all — the only ones that
-- exist are GitHub, Linear, Brevo and Stripe Connect `account.updated`, and
-- that last one only touches KYC status.
--
-- ─── Two tables, two jobs ─────────────────────────────────────────
--
-- `payouts` is the thing with a lifecycle: one row per attempt to send
-- money out, from `pending` to `sent`, `failed` or `reversed`. It is what
-- the reconciliation sweep walks and what an operator looks at.
--
-- `payment_webhook_events` is the append-only log of what providers told
-- us. Kept raw and forever: when the books and a provider's statement
-- disagree, the argument is settled by what they actually sent, not by our
-- interpretation of it. It is also the deduplication key — every provider
-- delivers the same event more than once, by design.

-- ─── Payouts ──────────────────────────────────────────────────────

CREATE TABLE payouts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,

    -- The movement this payout recorded. RESTRICT, not CASCADE: deleting a
    -- ledger transaction under a payout would erase the only evidence of
    -- where the money went.
    ledger_transaction_id UUID
        REFERENCES ledger_transactions(id) ON DELETE RESTRICT,

    provider VARCHAR(30) NOT NULL,
    -- The provider's own identifier. Absent until they answer, which is why
    -- it is nullable and why the reconciliation sweep treats a pending
    -- payout without one as its worst case.
    provider_reference VARCHAR(160),
    rail VARCHAR(20) NOT NULL CHECK (rail IN ('bank_account', 'mobile_money')),

    amount NUMERIC(20, 4) NOT NULL CHECK (amount > 0),
    currency CHAR(3) NOT NULL CHECK (currency IN ('EUR', 'XOF')),

    -- Where it went, kept for support and disputes. Masked at write time:
    -- a full phone number in a table an operator browses is a privacy
    -- problem waiting to be a data-protection one.
    destination_masked VARCHAR(40),

    status VARCHAR(20) NOT NULL DEFAULT 'pending'
        CHECK (status IN (
            'pending',   -- accepted by the provider, not yet settled
            'sent',      -- the provider confirmed the recipient was paid
            'failed',    -- the provider refused; the ledger movement was reversed
            'reversed'   -- we reversed it ourselves after the fact
        )),

    -- Same key the ledger transaction carries, so a replay of the whole
    -- request finds this row rather than creating a second one.
    idempotency_key TEXT UNIQUE,

    -- Non-null on failure. The provider's own words, because "payout
    -- failed" is not something a person can act on and "the number is not
    -- registered on MTN Benin" is.
    failure_reason TEXT,

    -- Stamped when a webhook or the sweep resolved it, and when the sweep
    -- last looked. Distinct: a payout can be checked many times before it
    -- settles, and knowing when we last asked is what makes the sweep
    -- cheap to run often.
    settled_at TIMESTAMPTZ,
    last_checked_at TIMESTAMPTZ,
    -- How many times the sweep has asked the provider. Bounded escalation:
    -- past a threshold it stops asking and asks a human instead.
    check_count INTEGER NOT NULL DEFAULT 0,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A settled payout has a settlement time, and an unsettled one does
    -- not. Enforced rather than assumed: every report of "when did this
    -- arrive" reads this column.
    CONSTRAINT payouts_settled_consistently CHECK (
        (status IN ('pending') AND settled_at IS NULL)
        OR (status IN ('sent', 'failed', 'reversed') AND settled_at IS NOT NULL)
    ),
    -- A failure without a reason is a support ticket nobody can answer.
    CONSTRAINT payouts_failure_explained CHECK (
        status <> 'failed' OR failure_reason IS NOT NULL
    )
);

-- The sweep's query: unsettled payouts, oldest first.
CREATE INDEX idx_payouts_unsettled
    ON payouts (created_at)
    WHERE status = 'pending';

-- The webhook's query: find the payout a provider is talking about.
CREATE UNIQUE INDEX idx_payouts_provider_reference
    ON payouts (provider, provider_reference)
    WHERE provider_reference IS NOT NULL;

CREATE INDEX idx_payouts_user ON payouts (user_id, created_at DESC);

COMMENT ON TABLE payouts IS
    'One row per attempt to send money out. Written by services::payout::send, '
    'resolved by a provider webhook or by services::reconciliation.';

-- ─── Webhook events ───────────────────────────────────────────────

CREATE TABLE payment_webhook_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider VARCHAR(30) NOT NULL,

    -- The provider's event id. Every provider redelivers, so this is the
    -- deduplication key and it is unique per provider. A provider that
    -- sends no id gets a hash of the body, computed at the edge.
    provider_event_id VARCHAR(200) NOT NULL,

    -- What we understood it to mean, after normalisation. NULL where we
    -- understood nothing — stored anyway, because an event we cannot read
    -- today is exactly what we will want to read when something is wrong.
    kind VARCHAR(40),

    -- Verbatim. The argument-settler.
    payload JSONB NOT NULL,

    -- FALSE means the signature did not check out. Such an event is stored
    -- and never applied: dropping it silently would hide an attack, and
    -- applying it would be the attack succeeding.
    signature_verified BOOLEAN NOT NULL DEFAULT FALSE,

    -- NULL until applied. Set even when the event turned out to be a no-op,
    -- so the sweep can tell "not handled yet" from "handled, nothing to do".
    processed_at TIMESTAMPTZ,
    -- Why applying it failed, if it did. Such an event is retried.
    processing_error TEXT,

    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (provider, provider_event_id)
);

CREATE INDEX idx_payment_webhook_events_unprocessed
    ON payment_webhook_events (received_at)
    WHERE processed_at IS NULL;

COMMENT ON TABLE payment_webhook_events IS
    'Append-only log of what payment providers told us, raw. Deduplicated by '
    '(provider, provider_event_id). Never edited, never deleted.';

-- ─── Timestamps ───────────────────────────────────────────────────

CREATE OR REPLACE FUNCTION touch_payouts_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_payouts_updated_at
    BEFORE UPDATE ON payouts
    FOR EACH ROW EXECUTE FUNCTION touch_payouts_updated_at();

-- A webhook event is evidence. Editing one is falsifying evidence, so the
-- only column that may change is the record of what we did with it.
CREATE OR REPLACE FUNCTION payment_webhook_events_are_evidence()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.provider IS DISTINCT FROM OLD.provider
       OR NEW.provider_event_id IS DISTINCT FROM OLD.provider_event_id
       OR NEW.payload IS DISTINCT FROM OLD.payload
       OR NEW.received_at IS DISTINCT FROM OLD.received_at
       OR NEW.signature_verified IS DISTINCT FROM OLD.signature_verified
    THEN
        RAISE EXCEPTION
            'payment_webhook_events is append-only; only processed_at, kind and processing_error may change';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_payment_webhook_events_immutable
    BEFORE UPDATE ON payment_webhook_events
    FOR EACH ROW EXECUTE FUNCTION payment_webhook_events_are_evidence();

CREATE OR REPLACE FUNCTION payment_webhook_events_no_delete()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'payment_webhook_events is append-only; events are never deleted';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_payment_webhook_events_no_delete
    BEFORE DELETE ON payment_webhook_events
    FOR EACH ROW EXECUTE FUNCTION payment_webhook_events_no_delete();
