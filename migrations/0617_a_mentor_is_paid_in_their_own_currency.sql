-- A mentor sets the currency they are paid in.
--
-- ## What was wrong
--
-- `mentor_profiles` carried `hourly_rate_eur_cents` and
-- `monthly_subscription_eur_cents` and no currency at all, so every mentor on
-- the platform was priced in euros. `book_session` said as much in a comment:
--
--     // Sessions are priced in EUR today. When they are not, the currency
--     // comes from the row above and the rest of this needs no change.
--
-- That comment was right about the rest of the chain. `mentorship_sessions`
-- already carries a `currency`; `capture_session_funds` already parses it and
-- treats the two differently; `collect` already routes XOF plus a phone number
-- to Mobile Money; the wallet already pays out through Momo and FedaPay. The
-- only thing missing was the column that says what the mentor charges in.
--
-- ## Why it is not a display problem
--
-- Storing euros and converting at display time would leave a Beninese
-- mentor's income floating with the euro: they announce 15 000 XOF, the euro
-- moves, and they receive something else. What somebody is owed has to be
-- denominated in the money they are owed it in. Conversion belongs to search
-- and to an indicative "≈ 23 €", never to the amount due.
--
-- ## Minor units
--
-- The columns lose `_eur_` and become minor units *of the row's currency*.
-- EUR has 100 minor units to the unit; XOF has none — the minor unit is the
-- franc. `ledger::Currency` already makes exactly this distinction
-- (`Eur => minor / 100`, `Xof => minor`), and any reader that divides by 100
-- unconditionally will show a CFA price a hundred times too low.
--
-- ## Existing rows
--
-- Every mentor priced so far did so in euros, and the default says so. No
-- amount is rewritten: renaming a column does not move the number in it.

ALTER TABLE mentor_profiles
    RENAME COLUMN hourly_rate_eur_cents TO hourly_rate_cents;

ALTER TABLE mentor_profiles
    RENAME COLUMN monthly_subscription_eur_cents TO monthly_subscription_cents;

ALTER TABLE mentor_profiles
    ADD COLUMN currency CHAR(3) NOT NULL DEFAULT 'EUR'
        CHECK (currency IN ('EUR', 'XOF'));

COMMENT ON COLUMN mentor_profiles.currency IS
    'What this mentor charges and is paid in. Constrained to the two the '
    'ledger settles — adding a third means teaching `ledger::Currency` and '
    'the payout adapters about it first, so the CHECK is where that '
    'conversation starts rather than a place it can be skipped.';

COMMENT ON COLUMN mentor_profiles.hourly_rate_cents IS
    'Minor units of `currency`. 2500 with currency EUR is 25,00 €; 15000 with '
    'currency XOF is 15 000 F CFA, because the franc has no minor unit. Was '
    'hourly_rate_eur_cents until migration 0617.';

COMMENT ON COLUMN mentor_profiles.monthly_subscription_cents IS
    'Minor units of `currency`, same reading as hourly_rate_cents.';

-- The listing filter converts through this, so a query that names a ceiling
-- in one currency can still see mentors priced in another. The rate is the
-- ECB reference feed `services::fx` already maintains; a currency with no row
-- yet is shown rather than hidden, because dropping somebody from a list
-- because our feed is stale is worse than showing a price the reader can read
-- for themselves.
CREATE INDEX idx_mentor_profiles_currency_rate
    ON mentor_profiles (currency, hourly_rate_cents)
    WHERE active;
