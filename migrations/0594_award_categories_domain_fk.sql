-- The award-category domain points at the catalogue, like every domain column.
--
-- 0590 added award_categories.skill_domain but not the foreign key to
-- skill_domains that every other domain column carries (0400). The CI invariant
-- "no unchecked domain column" caught it: a domain column with no key does not
-- fail on a typo, it routes to nothing and nobody finds out. Nullable stays
-- nullable — a cross-cutting category has no domain — and an FK permits NULL.

ALTER TABLE award_categories
    ADD CONSTRAINT award_categories_skill_domain_fkey
    FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;
