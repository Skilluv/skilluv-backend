-- Double-entry ledger: the single source of truth for real money.
--
-- ─── Why ──────────────────────────────────────────────────────────
--
-- `talent_wallets.balance_eur` / `balance_xof` are two numbers mutated in
-- place. They record what someone has, never where it came from or where it
-- went, and nothing forces the platform's books to balance. Three
-- consequences, all of which have already happened in this codebase:
--
--   * A payout that half-succeeds leaves a balance nobody can reconcile.
--   * Money is either "in the wallet" or not — there is no way to hold it
--     while a session is contested, so a mentor could withdraw before the
--     student had the lesson, and a chargeback landed on us.
--   * `paid_at` and `payout_released_at` were stamped whether or not money
--     moved, because the row said nothing about the movement itself.
--
-- Double entry fixes the class, not the instances: every event writes at
-- least two rows that sum to zero per currency. Money cannot appear or
-- vanish, only move between named accounts. A balance stops being a column
-- to trust and becomes the sum of a history that can be replayed.
--
-- ─── Accounts ─────────────────────────────────────────────────────
--
-- Codes are stable, readable strings, e.g.
--
--   user:<uuid>:pending:EUR      earned, not yet released
--   user:<uuid>:available:EUR    withdrawable
--   user:<uuid>:disputed:EUR     frozen pending a human decision
--   platform:revenue:EUR         our commission, once earned
--   platform:fees:EUR            what the provider charged us
--   psp:stripe:settlement:EUR    money held at the provider, not by us
--   external:world:EUR           the counterparty outside the system
--
-- ─── Sign convention ──────────────────────────────────────────────
--
-- `amount` is positive for a debit and negative for a credit, in the
-- accounting sense. Two families of account, and the sign reads
-- differently on each:
--
--   Assets — `psp:*`. Positive balance = money we are holding there.
--   Claims — `user:*`, `platform:*`. Negative balance = what we owe, or
--            what we have earned. Stored negative; the read helpers below
--            flip it, so an API never shows a person a negative balance.
--
-- This is what lets one transaction answer both questions at once: where
-- the money physically sits, and whose it is. A student paying 100 EUR:
--
--   psp:stripe:settlement:EUR   +100   we now hold 100 at Stripe
--   user:<mentor>:pending:EUR    -85   85 of it is owed to the mentor
--   platform:revenue:EUR         -15   15 of it is ours
--                               ─────
--                                  0
--
-- Releasing the mentor's share moves it between two claim accounts, and
-- the assets do not move — which is exactly right, the money is still at
-- Stripe. Paying them out finally moves the asset:
--
--   user:<mentor>:available:EUR  +85   we owe them 85 less
--   psp:momo:settlement:EUR      -85   85 left our Mobile Money float
--
-- `external:world` is the counterparty for anything crossing the system
-- boundary without a claim behind it.
--
-- ─── What this migration does not do ──────────────────────────────
--
-- It does not touch `talent_wallets`. Backfilling historical balances into
-- entries would invent transactions that never happened. The columns stay
-- authoritative for existing flows until each is migrated deliberately, and
-- `ledger_balance()` below is what new code reads.

-- ─── Accounts ─────────────────────────────────────────────────────

CREATE TABLE ledger_accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- The readable identity. Unique so `ensure_account` can upsert on it and
    -- two concurrent requests cannot create the same account twice.
    code TEXT NOT NULL UNIQUE CHECK (length(code) BETWEEN 3 AND 200),
    kind VARCHAR(20) NOT NULL
        CHECK (kind IN ('user', 'platform', 'psp', 'external')),
    -- Set for `kind = 'user'`, so a person's accounts can be listed without
    -- parsing their code.
    owner_user_id UUID REFERENCES users(id) ON DELETE RESTRICT,
    -- Set for `kind = 'user'`. The three states money can be in.
    state VARCHAR(20)
        CHECK (state IS NULL OR state IN ('pending', 'available', 'disputed')),
    currency CHAR(3) NOT NULL CHECK (currency IN ('EUR', 'XOF')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A user account is meaningless without an owner and a state; a platform
    -- or external account has neither. Enforced rather than assumed: the
    -- balance helpers below select on these columns.
    CONSTRAINT ledger_accounts_user_shape CHECK (
        (kind = 'user' AND owner_user_id IS NOT NULL AND state IS NOT NULL)
        OR (kind <> 'user' AND owner_user_id IS NULL AND state IS NULL)
    )
);

CREATE INDEX idx_ledger_accounts_owner
    ON ledger_accounts (owner_user_id, currency, state)
    WHERE owner_user_id IS NOT NULL;

-- ─── Transactions ─────────────────────────────────────────────────

CREATE TABLE ledger_transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Business meaning: 'mentorship_capture', 'bounty_payout',
    -- 'withdrawal_momo', 'refund', 'dispute_hold', …
    reason VARCHAR(60) NOT NULL CHECK (length(reason) >= 3),
    -- Idempotency key. A provider webhook is delivered more than once by
    -- design; replaying it must not double an amount. Unique, so the second
    -- insert conflicts instead of succeeding quietly.
    idempotency_key TEXT UNIQUE,
    -- What this movement is about, for tracing back from money to product.
    subject_type VARCHAR(40),
    subject_id UUID,
    -- Provider-side identifier, for reconciliation against their statement.
    provider VARCHAR(30),
    provider_reference VARCHAR(160),
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ledger_transactions_subject
    ON ledger_transactions (subject_type, subject_id);
CREATE INDEX idx_ledger_transactions_provider
    ON ledger_transactions (provider, provider_reference)
    WHERE provider_reference IS NOT NULL;
CREATE INDEX idx_ledger_transactions_created
    ON ledger_transactions (created_at DESC);

-- ─── Entries ──────────────────────────────────────────────────────

CREATE TABLE ledger_entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    transaction_id UUID NOT NULL
        REFERENCES ledger_transactions(id) ON DELETE RESTRICT,
    account_id UUID NOT NULL
        REFERENCES ledger_accounts(id) ON DELETE RESTRICT,
    -- Signed: positive means money entered this account, negative that it
    -- left. Four decimals so XOF (no minor unit) and EUR (two) share one
    -- column without rounding surprises.
    amount NUMERIC(20, 4) NOT NULL CHECK (amount <> 0),
    currency CHAR(3) NOT NULL CHECK (currency IN ('EUR', 'XOF')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ledger_entries_account ON ledger_entries (account_id, created_at DESC);
CREATE INDEX idx_ledger_entries_transaction ON ledger_entries (transaction_id);

-- Entries are immutable. Correcting a mistake means writing the opposite
-- entry, which leaves both the error and the correction visible. An UPDATE
-- or DELETE would rewrite history and break every balance derived from it.
CREATE OR REPLACE FUNCTION ledger_entries_are_immutable()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION
        'ledger entries are append-only: post a reversing entry instead of % on %',
        TG_OP, TG_TABLE_NAME;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_ledger_entries_no_update
    BEFORE UPDATE OR DELETE ON ledger_entries
    FOR EACH ROW EXECUTE FUNCTION ledger_entries_are_immutable();

-- ─── The balance rule ─────────────────────────────────────────────
--
-- Every transaction sums to zero, per currency. Deferred to commit time:
-- the entries of one transaction are inserted as separate statements, so an
-- immediate check would fire on the first row, when the books are meant to
-- be unbalanced.

CREATE OR REPLACE FUNCTION ledger_transaction_must_balance()
RETURNS TRIGGER AS $$
DECLARE
    offending RECORD;
    entry_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO entry_count
      FROM ledger_entries WHERE transaction_id = NEW.transaction_id;

    -- A single-entry transaction is the mistake this table exists to
    -- prevent: money coming from nowhere.
    IF entry_count < 2 THEN
        RAISE EXCEPTION
            'ledger transaction % has % entr(y|ies): money must move between at least two accounts',
            NEW.transaction_id, entry_count;
    END IF;

    SELECT currency, SUM(amount) AS total INTO offending
      FROM ledger_entries
     WHERE transaction_id = NEW.transaction_id
     GROUP BY currency
    HAVING SUM(amount) <> 0
     LIMIT 1;

    IF FOUND THEN
        RAISE EXCEPTION
            'ledger transaction % does not balance in %: off by %',
            NEW.transaction_id, offending.currency, offending.total;
    END IF;

    -- An entry must sit in an account of its own currency, otherwise the
    -- per-currency sum above would balance across unrelated money.
    PERFORM 1
       FROM ledger_entries e
       JOIN ledger_accounts a ON a.id = e.account_id
      WHERE e.transaction_id = NEW.transaction_id
        AND a.currency <> e.currency;

    IF FOUND THEN
        RAISE EXCEPTION
            'ledger transaction % posts an entry into an account of another currency',
            NEW.transaction_id;
    END IF;

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE CONSTRAINT TRIGGER trg_ledger_balance
    AFTER INSERT ON ledger_entries
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION ledger_transaction_must_balance();

-- ─── Reading balances ─────────────────────────────────────────────

-- Raw signed balance of one account, by code. Returns 0 for an account that
-- does not exist yet: never having been used and holding nothing are the
-- same answer, and forcing callers to create an account before asking would
-- mean writing on a read.
--
-- Signed, so it reads as an asset. For a claim account use
-- `ledger_user_balance`, which flips it.
CREATE OR REPLACE FUNCTION ledger_balance(account_code TEXT)
RETURNS NUMERIC AS $$
    SELECT COALESCE(SUM(e.amount), 0)
      FROM ledger_entries e
      JOIN ledger_accounts a ON a.id = e.account_id
     WHERE a.code = account_code;
$$ LANGUAGE sql STABLE;

-- What a user holds in one state and currency, as a positive number.
--
-- Claims are stored negative (see the sign convention above), so this
-- negates. Nobody outside the ledger should have to know that.
CREATE OR REPLACE FUNCTION ledger_user_balance(
    p_user_id UUID,
    p_state TEXT,
    p_currency TEXT
)
RETURNS NUMERIC AS $$
    SELECT -COALESCE(SUM(e.amount), 0)
      FROM ledger_entries e
      JOIN ledger_accounts a ON a.id = e.account_id
     WHERE a.owner_user_id = p_user_id
       AND a.state = p_state
       AND a.currency = p_currency;
$$ LANGUAGE sql STABLE;

-- Operational view: everything a person holds, by state and currency.
CREATE VIEW ledger_user_balances AS
SELECT a.owner_user_id AS user_id,
       a.currency,
       a.state,
       -- Negated: claims are stored negative, and a person's balance is a
       -- positive quantity everywhere it is shown.
       -COALESCE(SUM(e.amount), 0) AS balance
  FROM ledger_accounts a
  LEFT JOIN ledger_entries e ON e.account_id = a.id
 WHERE a.kind = 'user'
 GROUP BY a.owner_user_id, a.currency, a.state;

COMMENT ON VIEW ledger_user_balances IS
    'Per-user balances derived from entries. Authoritative for ledger-backed '
    'flows; talent_wallets.balance_* remains authoritative for flows not yet '
    'migrated.';

-- Reconciliation view: what the books say we are holding at each provider.
-- Compared against the provider statement, a non-zero difference is drift
-- and wants a human before it compounds.
CREATE VIEW ledger_provider_positions AS
SELECT a.code AS account_code,
       a.currency,
       COALESCE(SUM(e.amount), 0) AS balance
  FROM ledger_accounts a
  LEFT JOIN ledger_entries e ON e.account_id = a.id
 WHERE a.kind = 'psp'
 GROUP BY a.code, a.currency;
