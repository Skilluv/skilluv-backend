-- Paying without leaving the page, and confirming without the page at all.
--
-- ─── The failure this is built around ─────────────────────────────
--
-- The usual FedaPay integration bug, and it is an architecture bug rather
-- than an SDK one: the front end opens a payment, the payer confirms on
-- their phone, and then they refresh the page or close the tab. FedaPay has
-- the money. The backend never hears. The payment is real and the order
-- does not exist.
--
-- It is not specific to FedaPay. The Stripe flow here has the same shape:
-- fulfilment hangs off `checkout.session.completed`, and a webhook that is
-- lost, retried into a disabled endpoint, or never sent leaves a paid
-- customer with nothing. Nothing in the system ever asks.
--
-- Three changes, and none of them involve the browser:
--
--   1. **`merchant_reference`** — our own identifier, sent with the
--      transaction. It means we can ask "what happened to payment X"
--      without having stored the provider's id first, which is precisely
--      the state a lost response leaves us in.
--   2. **`fulfilled_at`** — a payment is not finished when the money
--      arrives, it is finished when the thing paid for exists. Separating
--      the two is what lets a sweep find "paid and not delivered".
--   3. **A methods catalogue** — which operator, in which country, over
--      which FedaPay mode, and whether it needs a redirect at all.
--
-- ─── Why the catalogue is data ────────────────────────────────────
--
-- FedaPay's operator list changes: Celtiis appeared in Benin, Mixx By Yas
-- is what Togocel is called now, and the set of operators that support
-- paying without leaving the page is a subset that grows. A `match` on
-- operator names would need a deployment every time.

ALTER TABLE payments
    -- Ours, sent to the provider and queryable back. `GET
    -- /v1/transactions/merchant/{reference}` is the call that makes a lost
    -- response recoverable.
    ADD COLUMN merchant_reference VARCHAR(80) UNIQUE,
    -- Which operator the payer chose, when the method is Mobile Money.
    ADD COLUMN operator VARCHAR(30),
    -- When the thing paid for was actually delivered. A payment that is
    -- `succeeded` with this NULL is money taken and nothing given, which is
    -- the state worth alerting on.
    ADD COLUMN fulfilled_at TIMESTAMPTZ,
    -- How many times a poller has asked the provider about this.
    ADD COLUMN check_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN last_checked_at TIMESTAMPTZ,
    -- How many credits a credit-pack purchase bought, frozen at checkout.
    -- Reading the pack table at delivery would let a price or size change
    -- between paying and receiving alter what someone gets.
    ADD COLUMN credits_purchased INTEGER CHECK (credits_purchased IS NULL OR credits_purchased > 0);

-- The poller's working set: succeeded but undelivered, and pending but old.
-- Both are money the payer has parted with and nothing to show for it.
CREATE INDEX idx_payments_unfulfilled
    ON payments (created_at)
    WHERE fulfilled_at IS NULL AND status IN ('pending', 'succeeded');

COMMENT ON COLUMN payments.fulfilled_at IS
    'When the thing paid for was delivered. NULL on a succeeded payment '
    'means the money arrived and the order does not exist — the exact state '
    'a closed browser tab used to leave behind.';

-- ─── Which operators exist, and how to reach them ─────────────────

CREATE TABLE payment_methods (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider VARCHAR(30) NOT NULL,
    country CHAR(2) NOT NULL CHECK (country = UPPER(country)),
    currency CHAR(3) NOT NULL CHECK (currency IN ('EUR', 'XOF')),

    -- What the payer recognises: `mtn`, `moov`, `celtiis`, `mixx`, …
    operator VARCHAR(30) NOT NULL,
    -- What the payer is shown. Operators rename themselves — Togocel is
    -- Mixx By Yas now — and a hardcoded label ages badly.
    label VARCHAR(60) NOT NULL,

    -- The provider's own name for this rail, e.g. FedaPay's `mtn_open`.
    -- The value that goes in `POST /v1/{mode}`.
    provider_mode VARCHAR(40) NOT NULL,

    -- TRUE when the payer can confirm without leaving our page: we create
    -- the transaction, ask for a token, and push the operator's prompt to
    -- their phone. FALSE means a redirect to the provider's page.
    supports_inline BOOLEAN NOT NULL DEFAULT FALSE,

    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order SMALLINT NOT NULL DEFAULT 100,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (provider, country, operator)
);

CREATE INDEX idx_payment_methods_lookup
    ON payment_methods (country, currency, sort_order)
    WHERE enabled = TRUE;

COMMENT ON TABLE payment_methods IS
    'Operators a payer can choose, per country, and how the provider names '
    'them. `supports_inline` is the subset that needs no redirect.';

-- FedaPay's collection coverage. `supports_inline` follows their list of
-- operators that accept a push prompt; the rest go through their page.
INSERT INTO payment_methods
    (provider, country, currency, operator, label, provider_mode, supports_inline, sort_order) VALUES

    -- Benin. MTN, Moov and Celtiis take the prompt; the other two do not.
    ('fedapay', 'BJ', 'XOF', 'mtn',     'MTN MoMo',    'mtn_open',    TRUE,  10),
    ('fedapay', 'BJ', 'XOF', 'moov',    'Moov Money',  'moov',        TRUE,  20),
    ('fedapay', 'BJ', 'XOF', 'celtiis', 'Celtiis Cash','celtiis',     TRUE,  30),
    ('fedapay', 'BJ', 'XOF', 'bmo',     'BMO',         'bmo',         FALSE, 40),
    ('fedapay', 'BJ', 'XOF', 'coris',   'Coris Money', 'coris',       FALSE, 50),

    -- Togo. Togocel is Mixx By Yas now, and the label follows.
    ('fedapay', 'TG', 'XOF', 'mixx',    'Mixx By Yas', 'togocel',     TRUE,  10),
    ('fedapay', 'TG', 'XOF', 'moov',    'Moov Money',  'moov_tg',     TRUE,  20),

    ('fedapay', 'CI', 'XOF', 'mtn',     'MTN MoMo',    'mtn_ci',      TRUE,  10),
    ('fedapay', 'NE', 'XOF', 'airtel',  'Airtel Money','airtel_ne',   TRUE,  10),
    ('fedapay', 'SN', 'XOF', 'free',    'Free Money',  'free_sn',     TRUE,  10),

    -- Cards, everywhere FedaPay takes them. Always a redirect: the card
    -- form is theirs, and it is theirs on purpose — holding card details
    -- ourselves would put this codebase in PCI scope.
    ('fedapay', 'BJ', 'XOF', 'card',    'Visa / Mastercard', 'card',  FALSE, 90),
    ('fedapay', 'CI', 'XOF', 'card',    'Visa / Mastercard', 'card',  FALSE, 90),
    ('fedapay', 'SN', 'XOF', 'card',    'Visa / Mastercard', 'card',  FALSE, 90),
    ('fedapay', 'TG', 'XOF', 'card',    'Visa / Mastercard', 'card',  FALSE, 90),
    ('fedapay', 'NE', 'XOF', 'card',    'Visa / Mastercard', 'card',  FALSE, 90),
    ('fedapay', 'BF', 'XOF', 'card',    'Visa / Mastercard', 'card',  FALSE, 90),
    ('fedapay', 'ML', 'XOF', 'card',    'Visa / Mastercard', 'card',  FALSE, 90),
    ('fedapay', 'GN', 'XOF', 'card',    'Visa / Mastercard', 'card',  FALSE, 90);

-- The sandbox rail. FedaPay removed the per-operator test servers, so a
-- single `momo_test` mode stands in for all of them: 64000001 and 66000001
-- succeed, anything else fails. Enabled only where a test key is in use,
-- which is why it is disabled by default rather than absent.
INSERT INTO payment_methods
    (provider, country, currency, operator, label, provider_mode, supports_inline, enabled, sort_order)
VALUES
    ('fedapay', 'BJ', 'XOF', 'momo_test', 'Mobile Money (sandbox)', 'momo_test', TRUE, FALSE, 200);
