-- Portfolios stop belonging to code.
--
-- ## What was wrong
--
-- Migration 0197 built `user_code_portfolios` with fourteen platforms in a
-- CHECK — GitHub, crates.io, npm — and metric columns named after what those
-- platforms publish. It is a good table and the name is the only thing
-- limiting it: a musician's SoundCloud and a voice actor's Voice123 are the
-- same row, the same verification problem and the same staleness problem as a
-- GitHub account.
--
-- The backlog asked for `user_audio_portfolios`, plus a third table for voice
-- platforms. Three tables means three sync loops, three verification flows,
-- and three answers to "which external accounts has this person linked".
--
-- ## The platforms become rows, with a domain
--
-- Fifteenth restatement avoided. The catalogue also carries what each
-- platform's numbers *mean*, which the CHECK could not: `items` is
-- repositories on GitHub, tracks on Bandcamp and roles on Casting Call Club,
-- and a profile page that prints "42 items" is a page nobody reads twice.
--
-- ## The generic pair, and why the code columns stay
--
-- `items_count` and `reach_count` are what a cross-domain reader uses: how
-- much is there, and how far did it get. The code-specific columns stay
-- because they carry a distinction the pair loses — packages published is not
-- repositories owned, and downloads are not stars — and the code profile
-- reads them by name.
--
-- The rule is that the pair is a projection, always filled, never the only
-- copy of anything. `code_portfolio` sets both.
--
-- ## What is not built here
--
-- Scraping. The backlog proposed reading public SoundCloud and Bandcamp pages
-- for people who cannot use an API. Two reasons it is not here: both
-- platforms' terms forbid it, and a figure obtained that way is
-- indistinguishable — in this table — from one somebody typed. Declared
-- numbers are accepted and marked as declared; `verified_at` stays NULL until
-- something checked them, and the craft score reads the distinction.

CREATE TABLE portfolio_platforms (
    slug VARCHAR(30) PRIMARY KEY,
    -- The domain whose practitioners use it. NULL for the ones that cross —
    -- nobody's field owns GitHub.
    skill_domain VARCHAR(30) REFERENCES skill_domains(slug) ON UPDATE CASCADE,
    name VARCHAR(80) NOT NULL,
    profile_url_pattern TEXT,
    -- What `items_count` counts here, in the words a profile page prints.
    items_label VARCHAR(40),
    -- What `reach_count` counts here.
    reach_label VARCHAR(40),
    -- Whether anything can check a claim automatically. FALSE means every row
    -- for this platform is declared until a human looks.
    has_public_api BOOLEAN NOT NULL DEFAULT FALSE,
    sort_order SMALLINT NOT NULL DEFAULT 100
);

COMMENT ON TABLE portfolio_platforms IS
    'The external services somebody can link a profile from. Carries what each '
    'one''s numbers mean, which the CHECK it replaces could not: items are '
    'repositories on GitHub, tracks on Bandcamp and roles on Casting Call Club.';

INSERT INTO portfolio_platforms
    (slug, skill_domain, name, items_label, reach_label, has_public_api, sort_order) VALUES
    -- Code (migration 0197)
    ('github',        NULL,   'GitHub',        'dépôts', 'étoiles',        TRUE,  10),
    ('gitlab',        NULL,   'GitLab',        'dépôts', 'étoiles',        TRUE,  20),
    ('codeberg',      NULL,   'Codeberg',      'dépôts', 'étoiles',        TRUE,  30),
    ('sourcehut',     NULL,   'SourceHut',     'dépôts', 'étoiles',        FALSE, 40),
    ('crates_io',     'code', 'crates.io',     'paquets', 'téléchargements', TRUE,  50),
    ('npm',           'code', 'npm',           'paquets', 'téléchargements', TRUE,  60),
    ('pypi',          'code', 'PyPI',          'paquets', 'téléchargements', TRUE,  70),
    ('go_modules',    'code', 'Go modules',    'modules', NULL,            FALSE, 80),
    ('rubygems',      'code', 'RubyGems',      'gems',    'téléchargements', TRUE,  90),
    ('maven_central', 'code', 'Maven Central', 'artefacts', NULL,          FALSE, 100),
    ('nuget',         'code', 'NuGet',         'paquets', 'téléchargements', TRUE,  110),
    ('packagist',     'code', 'Packagist',     'paquets', 'téléchargements', TRUE,  120),
    ('hex',           'code', 'Hex',           'paquets', 'téléchargements', TRUE,  130),
    ('homebrew',      'code', 'Homebrew',      'formules', NULL,           FALSE, 140),
    -- Audio
    ('soundcloud',      'audio', 'SoundCloud',        'morceaux', 'écoutes',    FALSE, 210),
    ('bandcamp',        'audio', 'Bandcamp',          'sorties',  'écoutes',    FALSE, 220),
    ('freesound',       'audio', 'Freesound',         'sons',     'téléchargements', TRUE, 230),
    ('opengameart',     'audio', 'OpenGameArt',       'ressources', 'téléchargements', FALSE, 240),
    ('voice123',        'audio', 'Voice123',          'rôles',    NULL,         FALSE, 250),
    ('castingcallclub', 'audio', 'Casting Call Club', 'rôles',    NULL,         FALSE, 260),
    ('bandlab',         'audio', 'BandLab',           'morceaux', 'écoutes',    FALSE, 270);

-- ═══════════════════════════════════════════════════════════════════
-- The table loses the word `code`
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE user_code_portfolios RENAME TO user_external_portfolios;

ALTER TABLE user_external_portfolios
    DROP CONSTRAINT IF EXISTS user_code_portfolios_platform_check,
    ADD CONSTRAINT user_external_portfolios_platform_fkey
        FOREIGN KEY (platform) REFERENCES portfolio_platforms(slug) ON UPDATE CASCADE;

ALTER TABLE user_external_portfolios
    -- How much is there: repositories, tracks, sound effects, roles.
    ADD COLUMN items_count INTEGER CHECK (items_count IS NULL OR items_count >= 0),
    -- How far it got: stars, plays, downloads. NULL where the platform
    -- publishes nothing, which is not the same as zero.
    ADD COLUMN reach_count BIGINT CHECK (reach_count IS NULL OR reach_count >= 0),
    -- TRUE when the figures came from the person rather than from a fetch.
    -- The craft score reads this: a declared play count is worth counting and
    -- worth marking.
    ADD COLUMN figures_are_declared BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON TABLE user_external_portfolios IS
    'External profiles somebody has linked, in any domain. Renamed from '
    'user_code_portfolios: a SoundCloud account is the same row, the same '
    'verification problem and the same staleness problem as a GitHub one.';

COMMENT ON COLUMN user_external_portfolios.items_count IS
    'The domain-neutral count. What it counts is in portfolio_platforms.'
    'items_label. A projection of the code-specific columns where those apply '
    '— always filled, never the only copy.';

COMMENT ON COLUMN user_external_portfolios.figures_are_declared IS
    'TRUE when the numbers came from the person. Kept rather than refused: a '
    'musician on a platform with no API can still describe their work, and a '
    'reader is entitled to know which figures were checked.';

-- Backfill the pair for everything already linked.
UPDATE user_external_portfolios
   SET items_count = COALESCE(repos_count, packages_count),
       reach_count = COALESCE(downloads_total, stars_received::BIGINT)
 WHERE items_count IS NULL AND reach_count IS NULL;

-- Renaming the table leaves the index names behind, which is cosmetic until
-- somebody greps for the table and finds nothing.
ALTER INDEX idx_code_portfolios_user RENAME TO idx_external_portfolios_user;
ALTER INDEX idx_code_portfolios_stale RENAME TO idx_external_portfolios_stale;
ALTER INDEX idx_code_portfolios_verified RENAME TO idx_external_portfolios_verified;
