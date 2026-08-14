-- A balance that does not get slower every year.
--
-- ─── The problem ──────────────────────────────────────────────────
--
-- `ledger_balance()` sums every entry an account has ever had. It is
-- indexed by account, so it is not a table scan — but it is O(entries in
-- that account), and entries only ever accumulate. A mentor with four years
-- of sessions recomputes four years of history every time they open their
-- wallet, and the wallet screen reads several of these.
--
-- Today that is microseconds. It is also the shape of a problem that
-- appears exactly when the platform starts working, which is the worst time
-- to discover it: the accounts that get slowest are the ones belonging to
-- the most active people.
--
-- ─── Why a snapshot is safe here and usually is not ───────────────
--
-- A cached total is normally a bug waiting to happen, because the thing it
-- caches can change underneath it. This ledger is the exception:
--
--   * `ledger_entries` is append-only. Migration 0153 installed a trigger
--     that refuses UPDATE, and this one refuses DELETE too — an entry is
--     never edited or removed, so a running total can only ever need adding
--     to.
--   * Every insert is balanced and inside a transaction, so a snapshot
--     updated by the same trigger commits or rolls back with it.
--
-- Under those two, the snapshot is not a cache that may be stale. It is the
-- same arithmetic, done once per entry instead of once per read.
--
-- ─── And it is checked anyway ─────────────────────────────────────
--
-- `ledger_verify_balances()` recomputes from entries and returns any
-- account where the two disagree. Trusting the invariant is right;
-- trusting it without ever checking is how a subtle trigger bug becomes a
-- year of wrong balances. The sweep runs it nightly.

CREATE TABLE ledger_account_balances (
    account_id UUID PRIMARY KEY REFERENCES ledger_accounts(id) ON DELETE CASCADE,
    balance NUMERIC(20, 4) NOT NULL DEFAULT 0,
    -- How many entries this total stands for. The cheap half of the
    -- verification: a count that disagrees points at the same problem a
    -- sum that disagrees does, without summing anything.
    entry_count BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE ledger_account_balances IS
    'Running total per account, maintained by trigger on insert. Safe only '
    'because ledger_entries is append-only and immutable; verified nightly '
    'by ledger_verify_balances().';

-- Seed from what exists. Empty in practice, and correct if it is not.
INSERT INTO ledger_account_balances (account_id, balance, entry_count)
SELECT a.id, COALESCE(SUM(e.amount), 0), COUNT(e.id)
  FROM ledger_accounts a
  LEFT JOIN ledger_entries e ON e.account_id = a.id
 GROUP BY a.id;

-- An account with no entries still needs a row, or its first entry has
-- nothing to add to.
CREATE OR REPLACE FUNCTION ledger_snapshot_new_account()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO ledger_account_balances (account_id) VALUES (NEW.id)
    ON CONFLICT (account_id) DO NOTHING;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_ledger_snapshot_new_account
    AFTER INSERT ON ledger_accounts
    FOR EACH ROW EXECUTE FUNCTION ledger_snapshot_new_account();

CREATE OR REPLACE FUNCTION ledger_snapshot_add_entry()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO ledger_account_balances (account_id, balance, entry_count, updated_at)
    VALUES (NEW.account_id, NEW.amount, 1, NOW())
    ON CONFLICT (account_id) DO UPDATE
        SET balance = ledger_account_balances.balance + EXCLUDED.balance,
            entry_count = ledger_account_balances.entry_count + 1,
            updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_ledger_snapshot_add_entry
    AFTER INSERT ON ledger_entries
    FOR EACH ROW EXECUTE FUNCTION ledger_snapshot_add_entry();

-- The other half of the invariant. 0153 refuses UPDATE on entries; without
-- this, a DELETE would silently invalidate every snapshot it touched.
CREATE OR REPLACE FUNCTION ledger_entries_no_delete()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION
        'ledger_entries is append-only; correct a mistake with a compensating entry';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_ledger_entries_no_delete
    BEFORE DELETE ON ledger_entries
    FOR EACH ROW EXECUTE FUNCTION ledger_entries_no_delete();

-- ─── Reads, now constant-time ─────────────────────────────────────

CREATE OR REPLACE FUNCTION ledger_balance(account_code TEXT)
RETURNS NUMERIC AS $$
    SELECT COALESCE(b.balance, 0)
      FROM ledger_accounts a
      LEFT JOIN ledger_account_balances b ON b.account_id = a.id
     WHERE a.code = account_code;
$$ LANGUAGE sql STABLE;

CREATE OR REPLACE FUNCTION ledger_user_balance(
    p_user_id UUID,
    p_state TEXT,
    p_currency TEXT
)
RETURNS NUMERIC AS $$
    -- Negated: claims are stored negative, and a person's balance is a
    -- positive quantity everywhere it is shown.
    SELECT -COALESCE(SUM(b.balance), 0)
      FROM ledger_accounts a
      JOIN ledger_account_balances b ON b.account_id = a.id
     WHERE a.owner_user_id = p_user_id
       AND a.state = p_state
       AND a.currency = p_currency;
$$ LANGUAGE sql STABLE;

CREATE OR REPLACE VIEW ledger_user_balances AS
SELECT a.owner_user_id AS user_id,
       a.currency,
       a.state,
       -COALESCE(SUM(b.balance), 0) AS balance
  FROM ledger_accounts a
  LEFT JOIN ledger_account_balances b ON b.account_id = a.id
 WHERE a.kind = 'user'
 GROUP BY a.owner_user_id, a.currency, a.state;

CREATE OR REPLACE VIEW ledger_provider_positions AS
SELECT a.code AS account_code,
       a.currency,
       COALESCE(b.balance, 0) AS balance
  FROM ledger_accounts a
  LEFT JOIN ledger_account_balances b ON b.account_id = a.id
 WHERE a.kind = 'psp';

-- ─── The check that keeps this honest ─────────────────────────────

CREATE OR REPLACE FUNCTION ledger_verify_balances()
RETURNS TABLE (
    account_code TEXT,
    snapshot NUMERIC,
    recomputed NUMERIC,
    drift NUMERIC
) AS $$
    SELECT a.code,
           COALESCE(b.balance, 0),
           COALESCE(SUM(e.amount), 0),
           COALESCE(b.balance, 0) - COALESCE(SUM(e.amount), 0)
      FROM ledger_accounts a
      LEFT JOIN ledger_account_balances b ON b.account_id = a.id
      LEFT JOIN ledger_entries e ON e.account_id = a.id
     GROUP BY a.code, b.balance
    HAVING COALESCE(b.balance, 0) <> COALESCE(SUM(e.amount), 0);
$$ LANGUAGE sql STABLE;

COMMENT ON FUNCTION ledger_verify_balances IS
    'Recomputes every account from its entries and returns those where the '
    'snapshot disagrees. Empty is the only acceptable result. Expensive by '
    'design — run nightly, never on a request.';
