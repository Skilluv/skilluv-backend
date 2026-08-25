-- One separator in `award_categories.slug`.
--
-- Thirty rows are kebab-case — `best-docs-contribution`, `rookie-educator` —
-- and sixteen are snake_case, seeded by 0425, 0452 and 0467. A slug goes in a
-- URL, and a column that uses two separators is a column no consumer can
-- guess at: it is the difference between `/awards/best-docs-contribution` and
-- `/awards/quality_bug_of_the_year` on the same page.
--
-- ## Why the wording is left alone
--
-- Only the separator changes. The shapes in this table — `best-*`, `rookie-*`,
-- `{domain}-*-of-the-year` — are three ways of framing an award rather than
-- three conventions in conflict, and rewriting them would rename things
-- people have already read.
--
-- ## What is safe about this
--
-- `award_nominees` and `award_votes` reference `award_categories.id`, not the
-- slug, so nothing points at the old strings. Nothing in `src/` or `tests/`
-- names them either — the only Rust that reached for these rows did it with
-- `slug LIKE '%ai%'`, which is a substring match this branch already removed
-- for catching `best-blockchain-project` and `best-trainer`.

UPDATE award_categories SET slug = replace(slug, '_', '-')
 WHERE slug LIKE '%\_%';

-- ═══════════════════════════════════════════════════════════════════
-- And the five names still in French
-- ═══════════════════════════════════════════════════════════════════
--
-- Seeded by 0425 before the repository settled on English, and sitting beside
-- quality's and leadership's English ones in the same ceremony.

UPDATE award_categories SET name = 'Runbook of the year'
 WHERE slug = 'ops-runbook-of-the-year';
UPDATE award_categories SET name = 'Post-mortem of the year'
 WHERE slug = 'ops-postmortem-of-the-year';
UPDATE award_categories SET name = 'Module of the year'
 WHERE slug = 'ops-module-of-the-year';
UPDATE award_categories SET name = 'Saving of the year'
 WHERE slug = 'ops-saving-of-the-year';
UPDATE award_categories SET name = 'The quiet year'
 WHERE slug = 'ops-quiet-year';

-- The descriptions that go with them, for the same reason.
UPDATE award_categories
   SET description = 'The runbook somebody else followed at three in the morning '
                     'and did not have to think.'
 WHERE slug = 'ops-runbook-of-the-year';
UPDATE award_categories
   SET description = 'The write-up that named a cause nobody wanted named, without '
                     'naming a person.'
 WHERE slug = 'ops-postmortem-of-the-year';
UPDATE award_categories
   SET description = 'The module other people deployed without asking its author '
                     'anything.'
 WHERE slug = 'ops-module-of-the-year';
UPDATE award_categories
   SET description = 'The bill that went down, with the measurement that shows it '
                     'and the service that did not get worse.'
 WHERE slug = 'ops-saving-of-the-year';
UPDATE award_categories
   SET description = 'The system nobody had to think about. The hardest one to '
                     'notice, which is why it is on the evening.'
 WHERE slug = 'ops-quiet-year';
