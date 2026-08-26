-- The two domains this branch opened had nobody who could run them.
--
-- 0404 wrote a `domain_curator:{domain}` capability for the nine domains that
-- existed, 0500 wrote communication's and 0517 wrote education's. 0450 and
-- 0460 opened quality and leadership and wrote neither.
--
-- It is not a cosmetic gap. `routes::practice` guards terrain adoption on
-- exactly this capability, and migration 0533 gave both domains six terrains
-- each — so the endpoint that turns a researched shortlist into a real
-- terrain was, for those two domains and only those two, guarded on a
-- capability the catalogue has no row for. Ungrantable, and therefore dead.
--
-- Found by `scripts/check-migrations.sh`, which counts these. The count it
-- expected had gone stale in the other direction — communication and
-- education had pushed it from nine to eleven — so the number was wrong for
-- one reason while hiding that it was also wrong for another. The check is
-- derived now rather than fixed, so opening a domain without its curator
-- fails rather than moving a constant.

INSERT INTO capability_catalog (capability, family, scope, description) VALUES
    ('domain_curator:quality', 'domain_curator', 'quality',
     'Runs the quality domain: its challenges, its defect hunts, its terrains, '
     'its featurings.'),
    ('domain_curator:leadership', 'domain_curator', 'leadership',
     'Runs the leadership domain: its challenges, its contests, its terrains, '
     'its featurings.')
ON CONFLICT (capability) DO NOTHING;
