-- Add FedaPay to the routing table.
--
-- This migration is the whole cost of adding a payout provider, alongside
-- one struct in `services::payout_adapters` and one line registering it.
-- Nothing else in the codebase changed, which is what the routing table and
-- the `PayoutProvider` trait were built for.
--
-- Why FedaPay and why now:
--
--   * Our direct Mobile Money adapters talk to one operator each, and each
--     needs its own commercial agreement per country. FedaPay reaches MTN,
--     Moov, Togocel, Orange and Wave across eight countries on a single
--     credential, which is the difference between opening Togo in an hour
--     and opening it in a quarter.
--   * Two countries in the seed had no coverage they could actually use:
--     Niger and Guinea were falling through to the XOF catch-all, which
--     points at an Orange integration nobody has exercised there.
--
-- FedaPay is a *fallback* in the countries where a direct operator route
-- already exists (priority 150, behind both), and the *primary* route
-- where none does. A direct integration is cheaper per transfer; an
-- aggregator that works beats a direct rail that has never been tried.

INSERT INTO payout_routes (country, currency, rail, provider, priority, notes) VALUES
    -- Fallback behind the direct operator routes.
    ('BJ', 'XOF', 'mobile_money', 'fedapay', 150, 'Benin — fallback behind MTN and Moov.'),
    ('CI', 'XOF', 'mobile_money', 'fedapay', 150, 'Cote d''Ivoire — fallback behind Orange and Wave.'),
    ('SN', 'XOF', 'mobile_money', 'fedapay', 150, 'Senegal — fallback behind Wave and Orange.'),
    ('TG', 'XOF', 'mobile_money', 'fedapay', 150, 'Togo — fallback behind MTN, and reaches Togocel.'),
    ('BF', 'XOF', 'mobile_money', 'fedapay', 150, 'Burkina Faso — fallback behind Orange.'),

    -- Primary where nothing else reaches.
    ('NE', 'XOF', 'mobile_money', 'fedapay', 100, 'Niger — no direct operator integration.'),
    ('GN', 'XOF', 'mobile_money', 'fedapay', 100, 'Guinea — no direct operator integration.');

-- The XOF catch-all pointed at an Orange integration that has never been
-- exercised outside the countries named above, so it promised coverage we
-- do not have. FedaPay takes it: a country we have not thought about is
-- better served by an aggregator than by a rail chosen at random.
UPDATE payout_routes
   SET provider = 'fedapay',
       notes = 'Catch-all for the XOF zone. FedaPay covers BJ, CI, SN, TG, BF, NE, GN and ML on one credential; '
               'a country outside that list still fails, but at the provider rather than in this table.'
 WHERE country IS NULL
   AND currency = 'XOF'
   AND rail = 'mobile_money'
   AND provider = 'orange';
