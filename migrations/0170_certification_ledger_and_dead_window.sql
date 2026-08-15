-- Money the books never saw, and a promise for a flow that does not exist.
--
-- ─── The certification sale ───────────────────────────────────────
--
-- Migration 0156 gave `certification_purchase` a release window, and
-- nothing ever posts a certification sale to the ledger. The money goes to
-- Stripe, an attempt row is marked `paid`, and the double-entry books —
-- which exist so that every movement of real money is recorded — do not
-- know it happened. The platform's own revenue is understated by every
-- certification ever sold.
--
-- It is the simplest flow of the three: the platform is the seller, there
-- is no third party owed anything. Which is exactly why it was skipped,
-- and exactly why it should not have been: `platform:revenue` is the
-- account an accountant reads first.
--
-- ─── The window with nothing behind it ────────────────────────────
--
-- `talent_offer_booking` has a seven-day hold defined and no booking flow
-- to hold anything. `talent_offers` is deliberately a standing statement of
-- availability — migration 0147 says so in as many words: "no calendar, no
-- escrow, no booking state machine".
--
-- A release window for a subject nothing produces is not harmless. It reads
-- as a feature that exists, it appears in any report of what the platform
-- holds, and the next person to touch escrow will write code against it.
-- Deleted. If bookings arrive later they will bring their own row.

DELETE FROM release_windows WHERE subject_type = 'talent_offer_booking';

-- ─── The certification attempt, correctly named ───────────────────
--
-- `fulfilment` was written against a `pending_payment` status that this
-- table does not have; its states are `pending`, `paid`, `started`, …
-- Fixing the code rather than the table: the table is right, and inventing
-- a status to match a typo would be the wrong repair.

COMMENT ON COLUMN certification_attempts.status IS
    'pending -> paid -> started -> passed|failed. `pending` means awaiting '
    'payment; services::fulfilment moves it to `paid` when the money lands, '
    'from whichever road brought the news.';
