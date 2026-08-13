-- Remove the old wallet ledger. The double-entry ledger is the only one now.
--
-- `talent_wallets.balance_eur` / `balance_xof` and `talent_transactions` were
-- the first attempt at recording what talents are owed: two mutable numbers
-- and an append log with a hash chain. Migration 0153 replaced them, and the
-- mentorship, bounty and withdrawal flows now go through it exclusively.
--
-- Keeping both would be the worst of the two worlds. Two places would answer
-- "how much do we owe this person", nothing would force them to agree, and
-- the day they disagreed there would be no way to know which was right. A
-- stale balance that looks authoritative is more dangerous than no balance.
--
-- What is lost, and why that is acceptable:
--
--   * The hash chain on `talent_transactions` proved rows had not been
--     edited. `ledger_entries` refuses UPDATE and DELETE outright, which is
--     the same guarantee enforced a level lower.
--   * Historical rows. There are none in production — the platform has not
--     launched — and inventing ledger entries to represent them would put
--     transactions in the books that never happened.
--
-- The wallet row itself stays: it holds the payout destinations (phone,
-- operator, Connect account, residency), which are not balances and have no
-- equivalent in the ledger.

DROP TABLE IF EXISTS talent_transactions;

ALTER TABLE talent_wallets
    DROP COLUMN IF EXISTS balance_eur,
    DROP COLUMN IF EXISTS balance_xof;

COMMENT ON TABLE talent_wallets IS
    'Payout destinations for one talent: residency, Mobile Money number and '
    'operator, Stripe Connect account and KYC state. Balances live in the '
    'ledger (migration 0153) and are read through ledger_user_balances.';

-- Withdrawals in a rolling window, for the daily and monthly limits.
--
-- These used to be counted from `talent_transactions`. Reading them from the
-- ledger keeps the limit and the money it limits in the same books — the
-- alternative being a cap computed from a table that no longer records
-- anything.
CREATE OR REPLACE FUNCTION ledger_withdrawn_within(
    p_user_id UUID,
    p_currency TEXT,
    p_hours INTEGER
)
RETURNS NUMERIC AS $$
    -- Withdrawals debit the user's available account, so they are positive
    -- entries there. Reversals credit it back and net out, which is right: a
    -- payout the provider refused should not consume someone's daily limit.
    SELECT COALESCE(SUM(e.amount), 0)
      FROM ledger_entries e
      JOIN ledger_accounts a ON a.id = e.account_id
      JOIN ledger_transactions t ON t.id = e.transaction_id
     WHERE a.owner_user_id = p_user_id
       AND a.state = 'available'
       AND a.currency = p_currency
       AND t.reason IN ('withdrawal', 'withdrawal_reversed')
       AND e.created_at >= NOW() - (p_hours || ' hours')::INTERVAL;
$$ LANGUAGE sql STABLE;

COMMENT ON FUNCTION ledger_withdrawn_within IS
    'Net amount withdrawn by a user in the last N hours, for rate limiting. '
    'Refused payouts net out, so they do not consume the limit.';
