-- What a published library actually gets used.
--
-- ## Why the numbers are stored rather than fetched
--
-- A profile page that called crates.io, npm and PyPI on every view would be
-- slow, would break when a registry is down, and would get us rate-limited
-- by all three. The sync fetches on a schedule and the page reads a row.
--
-- ## Why `fetched_at` is not decoration
--
-- A download count with no date is a number nobody can situate. Stale is
-- fine and common — a weekly sync means up to seven days old — but it has to
-- be visible, so a reader knows whether "12 000 downloads" was true today or
-- last month.

CREATE TABLE code_package_stats (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slice_id UUID NOT NULL REFERENCES project_slices(id) ON DELETE CASCADE,

    -- Which registry, and what the package is called there. Both, because
    -- `serde` on crates.io and `serde` on npm are different things.
    registry VARCHAR(30) NOT NULL
        CHECK (registry IN (
            'crates_io', 'npm', 'pypi', 'go_modules', 'maven_central',
            'rubygems', 'nuget', 'packagist', 'hex_pm', 'homebrew'
        )),
    package_name VARCHAR(200) NOT NULL
        CHECK (length(btrim(package_name)) > 0),
    latest_version VARCHAR(60),

    -- NULL means the registry does not publish it, which is different from
    -- zero. Go modules and Homebrew report no download count at all, and
    -- writing 0 there would claim nobody uses a package we simply cannot
    -- measure.
    downloads_total BIGINT CHECK (downloads_total IS NULL OR downloads_total >= 0),
    downloads_recent BIGINT CHECK (downloads_recent IS NULL OR downloads_recent >= 0),
    dependents_count INTEGER CHECK (dependents_count IS NULL OR dependents_count >= 0),

    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Set when the last attempt failed. Kept alongside the previous numbers
    -- rather than replacing them: an old figure with a visible date beats no
    -- figure at all, and beats a silent zero.
    last_error TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- One row per package per slice. A slice publishing to two registries has
-- two rows, which is true rather than convenient.
CREATE UNIQUE INDEX uniq_code_package_stats
    ON code_package_stats (slice_id, registry, package_name);

CREATE INDEX idx_code_package_stats_staleness
    ON code_package_stats (fetched_at);

COMMENT ON TABLE code_package_stats IS
    'Usage figures for published packages, fetched on a schedule. A profile '
    'reads a row rather than calling three registries per view.';

COMMENT ON COLUMN code_package_stats.downloads_total IS
    'NULL when the registry does not publish one. Different from zero: Go '
    'and Homebrew report nothing, and a zero there would claim nobody uses '
    'what we merely cannot measure.';
