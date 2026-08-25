-- Where a published artefact lives, as rows — and the seven places
-- communication publishes.
--
-- ## The nineteenth list of this shape
--
-- `published_artifact_stats.registry` was written by 0181 with ten package
-- registries, restated by 0214 to add the model hubs, and restated again by
-- 0432 to add the infrastructure ones. Seventeen values in a CHECK, three
-- rewrites, and the warning migrations 0228 and 0305 wrote is still true:
-- every addition is an opportunity to silently delete somebody else's.
--
-- Communication would make it a fourth rewrite. Same answer as 0400, 0404,
-- 0406, 0408, 0413, 0415 and 0416: a table.
--
-- ## The table carries what the CHECK could not, and it fixes a live bug
--
-- `craft_score::measure` sums downloads across *every* row of
-- `published_artifact_stats` attached to a person's deliverables, and calls
-- the result `library_downloads` in the **code** craft score. The AI profile
-- avoids that by listing its three hubs by hand; the code one lists nothing,
-- so a HuggingFace model already pays into the code score today. `/code-profile`
-- has the same hole in its published-packages list.
--
-- Neither is visible: the number is plausible, only ever compared to itself,
-- and the person it flatters has no reason to report it. With a domain on the
-- registry, both queries can say what they mean, and they are changed in the
-- same commit as this migration.
--
-- ## Seven new rows, and what each one will actually answer
--
-- The point of `has_public_api` is that it decides whether a figure can exist
-- at all. Written down here rather than inferred from whether the fetcher has
-- a branch, so that a platform we simply have not implemented yet reads
-- differently from one that publishes nothing.
--
--   * `dev_to` — public REST, gives reactions and comments.
--   * `hashnode` — public GraphQL, gives reactions and views.
--   * `youtube` — Data API v3, gives views, likes and comments. Needs a key,
--     which is why the column below says the fetch is conditional.
--   * `medium` — nothing machine-readable since 2019. Recognised, never
--     fetched, and the row says so.
--   * `speakerdeck` — slide decks. No API; the row exists so a talk can name
--     where its slides are without the URL being unrecognised.
--   * `arxiv` — the Atom API, which gives the version and the date but no
--     readership figure. That is the truth about arXiv, and writing zero
--     downloads for a paper everybody reads would be worse than writing
--     nothing.
--   * `zenodo` — REST, and the one research host that publishes both views
--     and downloads.
--
-- ## Two columns, because a view is not a download
--
-- `downloads_total` means somebody installed something. A video has views and
-- a post has reactions, and putting either in a downloads column would make
-- the code craft score's `library_downloads` term count video views the day
-- somebody publishes a tutorial — which is the exact bug this migration is
-- also fixing.

CREATE TABLE publication_registries (
    slug VARCHAR(30) PRIMARY KEY,
    -- The domain whose practitioners publish there. NULL for the ones that
    -- cross: nobody's field owns a personal blog.
    skill_domain VARCHAR(30) REFERENCES skill_domains(slug) ON UPDATE CASCADE,
    name VARCHAR(80) NOT NULL,
    -- What kind of thing is published there, in the words a profile prints.
    kind VARCHAR(20) NOT NULL CHECK (kind IN (
        'package',      -- a library somebody installs
        'model_hub',    -- weights or a dataset
        'infra',        -- a module, a chart, an image
        'article',      -- a written piece
        'video',        -- a video or a recording
        'slides',       -- a deck
        'paper'         -- a preprint or an archived record
    )),
    -- Whether anything can be fetched at all. FALSE means every figure for
    -- this platform is absent rather than zero.
    has_public_api BOOLEAN NOT NULL DEFAULT FALSE,
    -- TRUE when the API exists but needs a credential the deployment may not
    -- have. Distinguishes "we cannot" from "this one is not configured here",
    -- which are different answers to why a figure is missing.
    api_needs_credential BOOLEAN NOT NULL DEFAULT FALSE,
    sort_order SMALLINT NOT NULL DEFAULT 100,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE publication_registries IS
    'Every place a published artefact can live. A table rather than the CHECK '
    'that 0181, 0214 and 0432 each rewrote, and it carries the domain — '
    'without which the code craft score counts a HuggingFace model as a '
    'library download.';

INSERT INTO publication_registries
    (slug, skill_domain, name, kind, has_public_api, api_needs_credential, sort_order) VALUES
    -- Migration 0181
    ('crates_io',            'code', 'crates.io',        'package',   TRUE,  FALSE, 10),
    ('npm',                  'code', 'npm',              'package',   TRUE,  FALSE, 20),
    ('pypi',                 'code', 'PyPI',             'package',   TRUE,  FALSE, 30),
    ('go_modules',           'code', 'Go modules',       'package',   FALSE, FALSE, 40),
    ('maven_central',        'code', 'Maven Central',    'package',   FALSE, FALSE, 50),
    ('rubygems',             'code', 'RubyGems',         'package',   FALSE, FALSE, 60),
    ('nuget',                'code', 'NuGet',            'package',   FALSE, FALSE, 70),
    ('packagist',            'code', 'Packagist',        'package',   FALSE, FALSE, 80),
    ('hex_pm',               'code', 'Hex',              'package',   FALSE, FALSE, 90),
    ('homebrew',             'code', 'Homebrew',         'package',   FALSE, FALSE, 100),
    -- Migration 0214
    ('huggingface_models',   'ai',   'HuggingFace (models)',  'model_hub', TRUE,  FALSE, 110),
    ('huggingface_datasets', 'ai',   'HuggingFace (datasets)',  'model_hub', TRUE,  FALSE, 120),
    ('kaggle_datasets',      'ai',   'Kaggle',                 'model_hub', FALSE, TRUE,  130),
    -- Migration 0432
    ('terraform_registry',   'ops',  'Terraform Registry', 'infra', TRUE,  FALSE, 140),
    ('ansible_galaxy',       'ops',  'Ansible Galaxy',     'infra', TRUE,  FALSE, 150),
    ('artifacthub',          'ops',  'ArtifactHub',        'infra', TRUE,  FALSE, 160),
    ('docker_hub',           'ops',  'Docker Hub',         'infra', TRUE,  FALSE, 170),
    -- Communication
    ('dev_to',      'communication', 'DEV',          'article', TRUE,  FALSE, 210),
    ('hashnode',    'communication', 'Hashnode',     'article', TRUE,  FALSE, 220),
    ('medium',      'communication', 'Medium',       'article', FALSE, FALSE, 230),
    ('youtube',     'communication', 'YouTube',      'video',   TRUE,  TRUE,  240),
    ('speakerdeck', 'communication', 'Speaker Deck', 'slides',  FALSE, FALSE, 250),
    ('arxiv',       'communication', 'arXiv',        'paper',   TRUE,  FALSE, 260),
    ('zenodo',      'communication', 'Zenodo',       'paper',   TRUE,  FALSE, 270);

ALTER TABLE published_artifact_stats
    DROP CONSTRAINT IF EXISTS published_artifact_stats_registry_check,
    ADD CONSTRAINT published_artifact_stats_registry_fkey
        FOREIGN KEY (registry) REFERENCES publication_registries(slug) ON UPDATE CASCADE;

COMMENT ON CONSTRAINT published_artifact_stats_registry_fkey ON published_artifact_stats IS
    'Points at `publication_registries`. Replaces the CHECK that 0181, 0214 '
    'and 0432 each had to restate in full.';

-- ═══════════════════════════════════════════════════════════════════
-- What a published article, video or paper reports
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE published_artifact_stats
    -- How many people saw it. Never folded into `downloads_total`: that
    -- column means somebody installed something, and the code craft score
    -- sums it.
    ADD COLUMN views_count BIGINT CHECK (views_count IS NULL OR views_count >= 0),
    -- Reactions, claps, comments — whatever the platform counts as a
    -- deliberate gesture. One column rather than one per platform, because no
    -- reader compares a clap to a reaction.
    ADD COLUMN engagement_count INTEGER
        CHECK (engagement_count IS NULL OR engagement_count >= 0),
    -- When the platform says it went out. Distinct from `created_at`, which
    -- is when we first heard of it.
    ADD COLUMN published_at TIMESTAMPTZ;

COMMENT ON COLUMN published_artifact_stats.views_count IS
    'Readers or viewers, where the platform publishes the figure. NULL where '
    'it does not — writing zero would claim nobody read something we cannot '
    'measure.';

COMMENT ON COLUMN published_artifact_stats.engagement_count IS
    'Deliberate gestures: reactions, claps, comments. Platform-neutral on '
    'purpose — nobody compares a clap to a reaction.';
