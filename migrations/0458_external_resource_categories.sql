-- `external_resources.category` stops being a CHECK.
--
-- ## The pattern, one more time
--
-- Migration 0220 wrote nine categories for the AI toolkit. 0426 restated all
-- nine and added eight for ops. This migration would have restated seventeen
-- and added five for quality, and the leadership one after it would have
-- restated twenty-two.
--
-- That is the exact sequence migrations 0400, 0404, 0406, 0408 and 0413 each
-- broke for a different column, and the reasoning has not changed: a CHECK
-- cannot be extended, only replaced, and the replacement that drops somebody
-- else's value fails at nobody's desk. Migration 0228 documents one that did.
--
-- ## Why now and not in the quality migration
--
-- Because two domains land in this branch. Doing it inside
-- `0459_quality_terrains` would have meant leadership either restating the
-- CHECK or depending on a table created for another domain's reasons. It is
-- its own change because it belongs to neither.

CREATE TABLE external_resource_categories (
    slug VARCHAR(20) PRIMARY KEY,
    -- NULL for the ones that are not specific to a domain. `community` and
    -- `learning` were shared between AI and ops from the start, and forcing
    -- a domain on them would mean picking one and filtering it back out.
    skill_domain VARCHAR(30) REFERENCES skill_domains(slug) ON UPDATE CASCADE,
    name VARCHAR(80) NOT NULL,
    description TEXT NOT NULL,
    sort_order SMALLINT NOT NULL DEFAULT 100
);

COMMENT ON TABLE external_resource_categories IS
    'What kind of thing a curated external resource is. A table rather than a '
    'CHECK restated once per domain — 0220, 0426 — because the replacement '
    'that drops a predecessor''s value fails silently and at nobody''s desk.';

INSERT INTO external_resource_categories
    (slug, skill_domain, name, description, sort_order)
VALUES
    -- Shared (migration 0220)
    ('community', NULL, 'Community',
     'A place where practitioners of the trade actually answer.', 10),
    ('learning', NULL, 'Learning',
     'A course, a book, a series. The access note says what it costs.', 20),
    ('hub', NULL, 'Hub',
     'A registry or catalogue somebody publishes to.', 30),

    -- AI (migration 0220)
    ('framework', 'ai', 'Framework', 'A training or inference framework.', 110),
    ('llm_tooling', 'ai', 'LLM tooling', 'Serving, evaluation, prompting, agents.', 120),
    ('mlops', 'ai', 'MLOps', 'Experiment tracking, registries, pipelines.', 130),
    ('data_stack', 'ai', 'Data stack', 'Labelling, versioning, feature stores.', 140),
    ('compute', 'ai', 'Compute', 'Where a model can actually be trained, and at what price.', 150),
    ('safety', 'ai', 'Safety', 'Evaluation of harms, red-teaming, guardrails.', 160),

    -- Ops (migration 0426)
    ('iac', 'ops', 'Infrastructure as code', 'Terraform, Pulumi, Ansible and their ecosystems.', 210),
    ('containers', 'ops', 'Containers', 'Building, running, and the registries in between.', 220),
    ('orchestration', 'ops', 'Orchestration', 'Kubernetes, Nomad, and what runs on top.', 230),
    ('cicd', 'ops', 'CI/CD', 'Pipelines, runners, release tooling.', 240),
    ('observability', 'ops', 'Observability', 'Metrics, logs, traces, and what joins them.', 250),
    ('database', 'ops', 'Databases', 'Engines, tuning tools, migration tooling.', 260),
    ('secrets', 'ops', 'Secrets', 'Stores, rotation, short-lived credentials.', 270),
    ('cloud_free_tier', 'ops', 'Cloud free tier',
     'What can actually be practised without a bill. Its own category because '
     'the answer changes every year and is what decides whether somebody can '
     'start at all.', 280),

    -- Quality
    ('test_runner', 'quality', 'Test runners and frameworks',
     'What actually executes the tests, per language and per level.', 310),
    ('test_tooling', 'quality', 'Test tooling',
     'Coverage, mutation, contract testing, reporting. Everything around the '
     'runner.', 320),
    ('security_scanner', 'quality', 'Security scanners',
     'DAST, SAST, dependency and container scanning. The access note says '
     'what the free edition still does.', 330),
    ('a11y_tooling', 'quality', 'Accessibility tooling',
     'Automated checkers, screen readers, contrast tools. The tools find '
     'about a third; the note says which third.', 340),
    ('research_tooling', 'quality', 'Research tooling',
     'Session recording, remote testing, participant recruitment. The '
     'category where the free tier is usually the binding constraint.', 350),
    ('practice_target', 'quality', 'Practice targets',
     'Systems built to be tested against, so that practising does not require '
     'anybody''s permission.', 360)
ON CONFLICT (slug) DO NOTHING;

-- Anything a database already holds that this list does not name. Nothing is
-- expected — the list came from the CHECK — and the insert is here so a
-- database with a hand-added category migrates instead of failing on the
-- foreign key below.
INSERT INTO external_resource_categories (slug, name, description)
SELECT DISTINCT r.category, r.category,
       'Carried over: added before the catalogue existed.'
  FROM external_resources r
 WHERE NOT EXISTS (
        SELECT 1 FROM external_resource_categories c WHERE c.slug = r.category)
ON CONFLICT (slug) DO NOTHING;

ALTER TABLE external_resources
    DROP CONSTRAINT IF EXISTS external_resources_category_check,
    ADD CONSTRAINT external_resources_category_fkey
        FOREIGN KEY (category) REFERENCES external_resource_categories(slug)
        ON UPDATE CASCADE;

COMMENT ON CONSTRAINT external_resources_category_fkey ON external_resources IS
    'Points at `external_resource_categories`. Replaces the CHECK that 0220 '
    'wrote and 0426 restated: a new category is now an INSERT, which cannot '
    'delete anybody else''s.';

CREATE INDEX idx_external_resource_categories_domain
    ON external_resource_categories (skill_domain, sort_order);
