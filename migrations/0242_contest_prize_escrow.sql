-- Contests that pay, and the one rule that makes them not spec work.
--
-- ## The problem this exists to solve
--
-- A paid design contest is the most contested format in the trade, and for a
-- good reason: a brand publishes a brief, collects forty logos, pays for one,
-- and thirty-nine people worked for nothing. Everything that separates a
-- legitimate contest from that is a single question — was the money there
-- before the brief was?
--
-- So the rule is structural rather than editorial: **a contest with a cash
-- prize cannot leave `upcoming` until the money is escrowed.** Nobody can
-- enter it, because nobody can see it. That is a CHECK constraint below, not
-- a convention in a handler, because the one time it is bypassed is the time
-- it matters.
--
-- ## Why the money does not sit on a user account
--
-- At funding time nobody knows who wins. The ledger's user accounts are
-- claims on a named person, and there is no person yet. So the money is held
-- in an escrow account keyed to the contest, and moves to the podium's
-- `pending` accounts at finalisation — from where the existing release
-- window, disputes and withdrawals take over unchanged.
--
-- ## Why the platform takes nothing
--
-- `capture_for_recipient` splits an amount between a recipient and platform
-- revenue, because a paid mission is a sale we brokered. A contest prize is
-- not: the golden rule is that companies pay and talents do not, and a
-- commission skimmed off a prize is money taken from the winner. The escrowed
-- amount is what the podium receives, whole. If the platform is to be paid
-- for running the contest, the sponsor is invoiced on top, through the
-- machinery that already exists for that.
--
-- ## Currencies
--
-- The two the ledger knows. A prize denominated in something else would have
-- no account to sit in and no payout route to leave by.

-- ═══════════════════════════════════════════════════════════════════
-- An account that belongs to an outcome rather than to a person
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE ledger_accounts DROP CONSTRAINT IF EXISTS ledger_accounts_kind_check;

ALTER TABLE ledger_accounts
    ADD CONSTRAINT ledger_accounts_kind_check
    CHECK (kind IN ('user', 'platform', 'psp', 'external', 'escrow'));

COMMENT ON COLUMN ledger_accounts.kind IS
    'user: a claim on a named person. platform: a claim in our favour. '
    'psp: money held at a provider. external: the world outside. '
    'escrow: money owed to whoever an outcome designates — a contest podium '
    'that does not exist yet.';

-- `ledger_accounts_user_shape` already requires a non-user kind to carry no
-- owner and no state, which is exactly right for escrow: the whole point is
-- that neither is known yet.

-- ═══════════════════════════════════════════════════════════════════
-- What a contest promises, and whether it can keep it
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE tournaments
    -- The amount actually held. Not an intention: the column is only ever
    -- written together with the escrow that backs it.
    ADD COLUMN prize_cash_amount NUMERIC(12,2)
        CHECK (prize_cash_amount IS NULL OR prize_cash_amount > 0),
    ADD COLUMN prize_cash_currency CHAR(3)
        CHECK (prize_cash_currency IS NULL OR prize_cash_currency IN ('EUR', 'XOF')),
    ADD COLUMN prize_escrow_state VARCHAR(20) NOT NULL DEFAULT 'none'
        CHECK (prize_escrow_state IN (
            'none',      -- no cash prize
            'funded',    -- the money is held; the contest may open
            'awarded',   -- moved to the podium's pending accounts
            'refunded'   -- returned to the sponsor: cancelled, or no valid entry
        )),
    ADD COLUMN prize_funded_at TIMESTAMPTZ,
    -- Who put the money in. `sponsor_enterprise_id` says whose name is on the
    -- contest, which is not always who paid.
    ADD COLUMN prize_funded_by_enterprise_id UUID
        REFERENCES enterprises(id) ON DELETE RESTRICT;

COMMENT ON COLUMN tournaments.prize_cash_amount IS
    'The amount held in escrow for the podium, in full — the platform takes '
    'no share of a prize. NULL means the contest pays in fragments only.';

COMMENT ON COLUMN tournaments.prize_escrow_state IS
    'Where the cash is. A contest cannot open to entrants while a declared '
    'prize is unfunded: that is the difference between a contest and spec '
    'work.';

-- An amount needs a currency, and a currency needs an amount.
ALTER TABLE tournaments
    ADD CONSTRAINT tournaments_prize_amount_has_a_currency
    CHECK ((prize_cash_amount IS NULL) = (prize_cash_currency IS NULL));

-- `none` means no cash prize, and every other state means there is one.
ALTER TABLE tournaments
    ADD CONSTRAINT tournaments_escrow_state_matches_the_prize
    CHECK (
        (prize_escrow_state = 'none' AND prize_cash_amount IS NULL)
        OR (prize_escrow_state <> 'none' AND prize_cash_amount IS NOT NULL)
    );

-- Money held is money somebody put there, at a moment.
ALTER TABLE tournaments
    ADD CONSTRAINT tournaments_funded_escrow_names_its_funder
    CHECK (
        prize_escrow_state = 'none'
        OR (prize_funded_at IS NOT NULL AND prize_funded_by_enterprise_id IS NOT NULL)
    );

-- ═══════════════════════════════════════════════════════════════════
-- The rule
-- ═══════════════════════════════════════════════════════════════════
--
-- Half of it is already above, and it is the stronger half: the column that
-- announces the money is the column that records it. There is no way to write
-- an amount without writing the escrow state in the same breath, so a brief
-- can never advertise a prize nobody put up. `fund()` writes both or neither.
--
-- What is left is the other end. A contest whose escrow was returned still
-- carries the amount it once held, and nothing above stops it running with a
-- prize it can no longer pay. So: a contest people can enter — `registration`
-- or `active` — must be holding the money or have already paid it out.
--
-- `concluded` is allowed with any state because the refund happens after the
-- ranking, when nobody deserved the prize.

ALTER TABLE tournaments
    ADD CONSTRAINT tournaments_open_contest_still_holds_its_prize
    CHECK (
        prize_cash_amount IS NULL
        OR status NOT IN ('registration', 'active')
        OR prize_escrow_state IN ('funded', 'awarded')
    );

-- Sweeps read this: contests whose money is still held after they ended, and
-- which therefore owe somebody an award or a refund.
CREATE INDEX idx_tournaments_escrow_outstanding
    ON tournaments (prize_escrow_state, ends_at)
    WHERE prize_escrow_state = 'funded';
