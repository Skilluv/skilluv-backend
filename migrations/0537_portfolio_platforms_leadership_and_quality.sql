-- Portfolio platforms for leadership and quality, and a column that stops
-- promising a sweep that does not exist.
--
-- ## Two problems, one table
--
-- **Neither domain had a single row.** Tickets leadership/P-01, P-02 and
-- quality/P-01, P-02 ask for external portfolio imports, and
-- `portfolio_platforms` held nothing for either — so a member of the two
-- domains this branch opened could not declare an external portfolio at all,
-- while communication had ten platforms and education six.
--
-- **`has_public_api` was read as two different things.** The sweep in
-- `services::portfolio_sync` selects on it, and the fetcher only knows five
-- slugs. Sixteen rows — `github`, `crates_io`, `npm`, `pypi`, `docker_hub`,
-- `huggingface` and the rest — were picked up every cycle, fell through to
-- the catch-all arm, logged "nothing knows how to fetch it" and wrote back
-- empty statistics. The column states a fact about the platform, which is
-- worth keeping: it says what could be built. What it never said is whether
-- anybody built it.
--
-- `sync_implemented` says that, and the sweep filters on it instead. A test
-- checks the two lists against the `match` in the fetcher, so they cannot
-- drift the way one list and one match already had.
--
-- ## Why no `user_leadership_portfolios` or `user_qa_portfolios`
--
-- Both tickets propose a table of their own. `user_external_portfolios`
-- already holds what they describe, including the part that matters: a figure
-- somebody typed is stored with `figures_are_declared = TRUE`, left
-- unverified, and counted at a discount. Two more tables would have meant two
-- more places for that distinction to be forgotten, and a third domain would
-- have wanted a third.

-- ═══════════════════════════════════════════════════════════════════
-- What the sweep can actually read
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE portfolio_platforms
    ADD COLUMN sync_implemented BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN portfolio_platforms.has_public_api IS
    'Whether the platform exposes figures that could be fetched. A fact about '
    'the platform, and a shortlist of what is worth building next.';

COMMENT ON COLUMN portfolio_platforms.sync_implemented IS
    'Whether a fetcher exists in services::portfolio_sync. The sweep selects '
    'on this. Kept apart from has_public_api because the two were conflated, '
    'and sixteen platforms were swept every cycle so that nothing could read '
    'them.';

UPDATE portfolio_platforms
   SET sync_implemented = TRUE
 WHERE slug IN ('dev_to', 'hashnode', 'personal_blog', 'youtube', 'weblate');

-- ═══════════════════════════════════════════════════════════════════
-- The four rows every domain shares, in the language of the rest
-- ═══════════════════════════════════════════════════════════════════
--
-- Seeded in French before the repository settled on English, and left that
-- way while ten domains' worth of rows were written in English around them.

UPDATE portfolio_platforms SET items_label = 'repositories', reach_label = 'stars'
 WHERE skill_domain IS NULL AND slug IN ('github', 'gitlab', 'codeberg', 'sourcehut');

-- ═══════════════════════════════════════════════════════════════════
-- Leadership
-- ═══════════════════════════════════════════════════════════════════
--
-- P-01 asks for LinkedIn recommendations and says itself that the API is
-- closed for endorsements, proposing manual entry. Manual entry is what this
-- table already supports, so the row is the whole implementation: declared,
-- unverified, discounted, and honest about which of the three it is.
--
-- P-02 asks for GitHub review counts and RFC issues. That is not a second
-- platform — it is one account seen through a different lens, and adding a
-- `github` row per domain would ask somebody to declare the same account
-- twice. The account is declared once against the shared `github` row; what
-- is missing is a fetcher that counts reviews rather than repositories, and
-- `sync_implemented = FALSE` now says so instead of the sweep pretending
-- otherwise every fifteen minutes.

INSERT INTO portfolio_platforms
    (slug, skill_domain, name, profile_url_pattern, items_label, reach_label,
     has_public_api, sync_implemented, sort_order)
VALUES
    ('linkedin', 'leadership', 'LinkedIn',
     'https://www.linkedin.com/in/{handle}', 'recommendations', '',
     FALSE, FALSE, 10),
    ('notion_public', 'leadership', 'Notion (public)',
     'https://{handle}.notion.site', 'documents', '',
     FALSE, FALSE, 20),
    ('speakerdeck', 'leadership', 'Speaker Deck',
     'https://speakerdeck.com/{handle}', 'decks', 'views',
     TRUE, FALSE, 30);

-- ═══════════════════════════════════════════════════════════════════
-- Quality
-- ═══════════════════════════════════════════════════════════════════
--
-- P-02 asks for publicly disclosed reports on HackerOne and Bugcrowd. Both
-- publish a profile page and neither offers an open endpoint for somebody
-- else's disclosures, so these are declared with a link a reader can follow —
-- which for a disclosed report is the whole proof anyway.
--
-- P-01's GitHub reviewer statistics are the same account and the same missing
-- fetcher as leadership's P-02.

INSERT INTO portfolio_platforms
    (slug, skill_domain, name, profile_url_pattern, items_label, reach_label,
     has_public_api, sync_implemented, sort_order)
VALUES
    ('hackerone', 'quality', 'HackerOne',
     'https://hackerone.com/{handle}', 'disclosed reports', 'reputation',
     FALSE, FALSE, 10),
    ('bugcrowd', 'quality', 'Bugcrowd',
     'https://bugcrowd.com/{handle}', 'disclosed reports', 'points',
     FALSE, FALSE, 20),
    ('testing_blog', 'quality', 'Testing blog or newsletter',
     '{handle}', 'posts', '',
     TRUE, FALSE, 30);
