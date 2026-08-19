-- The seven domain columns that were still holding a free string.
--
-- Migration 0400 made the domains rows so that a domain would be declared in
-- one place. Seven tables written on parallel branches kept a bare
-- VARCHAR — `academy_cohorts`, `consultations`, `external_resources`,
-- `featured_talents`, `marketplace_items`, `mentoring_programs` and
-- `tournament_series` — and a typo in any of them is a row that exists,
-- passes every insert, and is invisible to every listing that joins the
-- catalogue. That is worse than a rejected write: nobody finds out.
--
-- `tests/test_skill_domains.rs::every_domain_column_references_the_table`
-- fails on the next one, which is why it is checked in the schema rather
-- than in review.
--
-- ON UPDATE CASCADE throughout, matching the rest: a domain slug that gets
-- renamed is renamed everywhere it is used.
--
-- Existing rows first. Only `tournament_series.skill_domain` is nullable, so
-- that one loses a slug nobody declared — NULL says "unset", which is true.
-- The other six are NOT NULL, and a row whose domain does not exist is
-- unreachable from every listing that joins the catalogue; it moves to `code`
-- rather than being deleted, because the row still holds work somebody did.

UPDATE tournament_series SET skill_domain = NULL
 WHERE skill_domain IS NOT NULL
   AND skill_domain NOT IN (SELECT slug FROM skill_domains);

UPDATE academy_cohorts SET skill_domain = 'code'
 WHERE skill_domain NOT IN (SELECT slug FROM skill_domains);

UPDATE consultations SET skill_domain = 'code'
 WHERE skill_domain NOT IN (SELECT slug FROM skill_domains);

UPDATE external_resources SET domain = 'code'
 WHERE domain NOT IN (SELECT slug FROM skill_domains);

UPDATE featured_talents SET skill_domain = 'code'
 WHERE skill_domain NOT IN (SELECT slug FROM skill_domains);

UPDATE marketplace_items SET skill_domain = 'code'
 WHERE skill_domain NOT IN (SELECT slug FROM skill_domains);

UPDATE mentoring_programs SET skill_domain = 'code'
 WHERE skill_domain NOT IN (SELECT slug FROM skill_domains);

ALTER TABLE academy_cohorts
    ADD CONSTRAINT academy_cohorts_skill_domain_fkey
    FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE consultations
    ADD CONSTRAINT consultations_skill_domain_fkey
    FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE external_resources
    ADD CONSTRAINT external_resources_domain_fkey
    FOREIGN KEY (domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE featured_talents
    ADD CONSTRAINT featured_talents_skill_domain_fkey
    FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE marketplace_items
    ADD CONSTRAINT marketplace_items_skill_domain_fkey
    FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE mentoring_programs
    ADD CONSTRAINT mentoring_programs_skill_domain_fkey
    FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE tournament_series
    ADD CONSTRAINT tournament_series_skill_domain_fkey
    FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;
