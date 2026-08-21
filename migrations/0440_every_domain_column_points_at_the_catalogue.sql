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


-- ═══════════════════════════════════════════════════════════════════
-- The two Discord routing tables
-- ═══════════════════════════════════════════════════════════════════
--
-- 0257 wrote these with a bare VARCHAR and said why: `skill_domains` existed
-- on some developer machines and not in the canonical chain, so referencing
-- it migrated locally and failed on a fresh database. That was true on the
-- branch it was written on. It is not true here — 0400 puts the table in the
-- chain, three hundred migrations before this one — and the cost of no key is
-- concrete for a routing table: a typo'd domain does not fail, it routes the
-- announcement nowhere and nobody finds out.
--
-- `discord_channels.skill_domain` used the empty string for the domain-blind
-- room, so that the primary key would not have to reason about NULL equality.
-- A sentinel cannot point at a catalogue, so it becomes NULL and the
-- uniqueness moves to an index over COALESCE — same guarantee, one less value
-- that means something only by convention.

UPDATE discord_channels SET skill_domain = NULL WHERE skill_domain = '';

UPDATE discord_channels SET skill_domain = NULL
 WHERE skill_domain IS NOT NULL
   AND skill_domain NOT IN (SELECT slug FROM skill_domains);

UPDATE discord_notifications_queue SET skill_domain = NULL
 WHERE skill_domain IS NOT NULL
   AND skill_domain NOT IN (SELECT slug FROM skill_domains);

ALTER TABLE discord_channels
    DROP CONSTRAINT IF EXISTS discord_channels_pkey;

ALTER TABLE discord_channels
    ALTER COLUMN skill_domain DROP DEFAULT,
    ALTER COLUMN skill_domain DROP NOT NULL;

CREATE UNIQUE INDEX idx_discord_channels_purpose_domain
    ON discord_channels (purpose, COALESCE(skill_domain, ''));

ALTER TABLE discord_channels
    ADD CONSTRAINT discord_channels_skill_domain_fkey
    FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;

ALTER TABLE discord_notifications_queue
    ADD CONSTRAINT discord_notifications_queue_skill_domain_fkey
    FOREIGN KEY (skill_domain) REFERENCES skill_domains(slug) ON UPDATE CASCADE;
