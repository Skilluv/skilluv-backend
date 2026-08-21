-- `code_package_stats` becomes `published_artifact_stats`.
--
-- ## Why the same table
--
-- A crate on crates.io and a model on HuggingFace raise the identical
-- question — is this used by anyone, and when did we last ask — and answer it
-- with the identical row: a registry, a name there, a download count that may
-- be absent, a fetch date, and the error from the last attempt if it failed.
--
-- The alternative was `user_ai_portfolios(platform, metadata_json)` as the
-- backlog proposed. A JSONB blob per platform means every reader parses a
-- different shape, nothing can be sorted or compared across platforms, and
-- the staleness rule has to be reimplemented per platform. The columns are
-- already right; only the name said `code`.
--
-- ## What is new
--
-- `likes_count`, because the AI hubs publish a popularity signal that is not
-- a download and not a dependent: HuggingFace likes, Kaggle votes. Folding it
-- into `dependents_count` would claim something false — a like is not a
-- project that depends on you.

ALTER TABLE code_package_stats RENAME TO published_artifact_stats;

ALTER INDEX uniq_code_package_stats RENAME TO uniq_published_artifact_stats;
ALTER INDEX idx_code_package_stats_staleness
    RENAME TO idx_published_artifact_stats_staleness;

ALTER TABLE published_artifact_stats
    ADD COLUMN likes_count INTEGER
        CHECK (likes_count IS NULL OR likes_count >= 0);

ALTER TABLE published_artifact_stats
    DROP CONSTRAINT IF EXISTS code_package_stats_registry_check;

ALTER TABLE published_artifact_stats
    ADD CONSTRAINT published_artifact_stats_registry_check
    CHECK (registry IN (
        -- Package registries (migration 0183)
        'crates_io', 'npm', 'pypi', 'go_modules', 'maven_central',
        'rubygems', 'nuget', 'packagist', 'hex_pm', 'homebrew',
        -- Model and dataset hubs. Models and datasets are separate namespaces
        -- on HuggingFace and answer on different endpoints, so they are
        -- separate registries here rather than one with a flag.
        'huggingface_models', 'huggingface_datasets', 'kaggle_datasets'
    ));

COMMENT ON TABLE published_artifact_stats IS
    'Usage figures for anything published to a public registry or hub — a '
    'crate, a package, a model, a dataset. Fetched on a schedule so a profile '
    'reads a row rather than calling five services per view.';

COMMENT ON COLUMN published_artifact_stats.likes_count IS
    'HuggingFace likes, Kaggle votes. Not a download and not a dependent: a '
    'like says somebody approved, which is a weaker claim than either, and '
    'folding it into dependents_count would overstate it.';
