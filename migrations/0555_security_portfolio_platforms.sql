-- Where a security person already has a reputation, and the programmes they
-- can go and earn one on.
--
-- ## Why there is no `user_cyber_portfolios`
--
-- Tickets P-02 to P-05 asked for one table per platform — HackTheBox,
-- TryHackMe, CTFtime, HackerOne, Bugcrowd — each with its own columns:
-- `machines_pwned_count`, `rooms_completed_count`, `ctftime_rating`,
-- `reputation_score`. Five tables and five readers, and the fifth is where a
-- profile stops showing everything.
--
-- `user_external_portfolios` (0415) is that table, generic since it was
-- written: a platform, a handle, an item count, a reach figure, a `metadata`
-- object for whatever else the platform says, and a `figures_are_declared`
-- flag that the craft scores read. `portfolio_platforms` says what the two
-- numbers are called on each platform, which is how "87 machines" and "156
-- rooms" render correctly without either being a column.
--
-- So P-02 to P-05 are five rows, two fetchers, and nothing else.
--
-- ## HackerOne and Bugcrowd move domain
--
-- Migration 0537 filed them under `quality`, alongside the intrusion-testing
-- family. That was reasonable when `security` had one orientation and no
-- catalogue; it is not now. A person with forty disclosed HackerOne reports is
-- doing security, and the profile that should be showing them is the security
-- one. Quality keeps `intrusion` as a testing practice — scope written first,
-- method named, findings replayable — which is a different claim from a bounty
-- career.
--
-- The rows move rather than being duplicated: a second `hackerone` row would
-- mean two answers to how many reports somebody has disclosed.
--
-- ## Which of the five can actually be read
--
-- Two. This matters more than it looks, because a declared figure and a
-- fetched one are treated differently by every craft score:
--
--   * **TryHackMe** publishes an unauthenticated profile endpoint. Fetched.
--   * **CTFtime** publishes a documented public API. Fetched.
--   * **HackTheBox** has an API and it requires the account holder's own
--     token, which this platform is not going to ask anybody for. Declared,
--     with a link.
--   * **HackerOne** and **Bugcrowd** publish profile pages and no endpoint for
--     somebody else's disclosures. Declared, with a link — which for a
--     *disclosed* report is the whole proof anyway.
--
-- Nothing here scrapes. A scraper against a platform whose terms forbid it is
-- a liability, and a scraper that breaks silently is worse than a declared
-- figure that says it is declared.

-- HackerOne and Bugcrowd are NOT moved here. 0537 gives them to quality,
-- which ships with exactly three platforms and would drop to one if these
-- two left. This domain need not own the row: a bounty programme lives in
-- external_bounty_programs below, a claim in external_bounty_claims (0561),
-- and a researcher's HackerOne profile is a quality signal as much as a
-- security one. Taking them would break another domain's rollout for no
-- capability this one gains.

INSERT INTO portfolio_platforms
    (slug, skill_domain, name, profile_url_pattern, items_label, reach_label,
     has_public_api, synced_by, sort_order)
VALUES
    ('hackthebox', 'security', 'Hack The Box',
     'https://app.hackthebox.com/profile/{handle}', 'machines pwned', 'global rank',
     FALSE, NULL, 630),
    -- has_public_api is TRUE (both publish figures) but synced_by is NULL:
    -- no worker fetches them yet. The two are different claims -- one about
    -- the platform, one about this codebase. Stamping 'portfolio_sync' would
    -- put them in a set the sweep reads, and it reads neither. Fetchers are
    -- P-03/P-04; until they exist the honest value is NULL.
    ('tryhackme', 'security', 'TryHackMe',
     'https://tryhackme.com/p/{handle}', 'rooms completed', 'global rank',
     TRUE, NULL, 640),
    ('ctftime', 'security', 'CTFtime',
     'https://ctftime.org/user/{handle}', 'events played', 'rating',
     TRUE, NULL, 650),
    ('intigriti', 'security', 'Intigriti',
     'https://app.intigriti.com/researcher/{handle}', 'disclosed reports', 'points',
     FALSE, NULL, 660),
    ('yeswehack', 'security', 'YesWeHack',
     'https://yeswehack.com/hunters/{handle}', 'disclosed reports', 'reputation',
     FALSE, NULL, 670);

-- ═══════════════════════════════════════════════════════════════════
-- Programmes somebody can go and hunt on (T-13)
-- ═══════════════════════════════════════════════════════════════════
--
-- This platform does not run these programmes and never will. What it can do
-- is keep a curated list, tagged by the skills each one actually needs, so
-- that a researcher who has finished the training grounds has somewhere to go
-- next that is not "search the internet".
--
-- ## Why it is a table and not a documentation page
--
-- Because the answer changes weekly — programmes close, scopes change, payouts
-- move — and because it has to be filterable by skill to be worth anything. A
-- markdown list of forty programmes is a list nobody reads twice.
--
-- ## What a row is not
--
-- Not an endorsement, and not a promise about payment. `curated_at` says when
-- a human last looked, and a row nobody has looked at for a year is shown with
-- that date rather than quietly presented as current.

CREATE TABLE external_bounty_programs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Which platform it lives on. Constrained to the platforms this domain
    -- actually curates, and deliberately not a foreign key to
    -- `portfolio_platforms`: that table is about somebody's profile, and a
    -- programme is not a profile.
    platform VARCHAR(20) NOT NULL
        CHECK (platform IN ('hackerone', 'bugcrowd', 'intigriti',
                            'yeswehack', 'self_hosted')),
    program_slug VARCHAR(120) NOT NULL,
    program_url VARCHAR(500) NOT NULL CHECK (program_url ~ '^https://'),
    organisation_name VARCHAR(160) NOT NULL,
    -- One paragraph: what is in scope, in the words a researcher needs before
    -- deciding to spend an evening on it.
    scope_summary TEXT,
    -- Skill nodes, so the recommender can match a programme to what somebody
    -- has actually practised. Text rather than a foreign key array because a
    -- programme can need a skill this catalogue has not named yet, and
    -- refusing the row would lose the programme rather than gain the skill.
    skill_topics TEXT[] NOT NULL DEFAULT '{}',
    -- Free text on purpose: platforms state this in ranges, in currencies, and
    -- sometimes as "swag only". A numeric column would have forced a lie.
    payout_range VARCHAR(80),
    -- Whether it pays money at all. The one thing worth being structured,
    -- because "no bounty, thanks and a hall of fame entry" is a legitimate
    -- programme and a wasted evening for somebody who needed to be paid.
    pays_money BOOLEAN NOT NULL DEFAULT TRUE,
    -- Whether reports are published afterwards. Decides whether a finding
    -- there can ever be attested here: a disclosure this platform cannot read
    -- is a claim.
    discloses_reports BOOLEAN NOT NULL DEFAULT FALSE,

    -- When a human last checked it, and who. A programme nobody has looked at
    -- is shown with its date rather than presented as current.
    curated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    curated_by UUID REFERENCES users(id) ON DELETE SET NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    retired_reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (platform, program_slug),
    CONSTRAINT a_retired_programme_says_why CHECK (
        is_active OR retired_reason IS NOT NULL
    )
);

COMMENT ON TABLE external_bounty_programs IS
    'Curated public bounty programmes on other platforms. Not run here, not '
    'endorsed here: a shortlist tagged by skill so that somebody who has '
    'finished the training grounds has somewhere to go that is not a search '
    'engine.';

COMMENT ON COLUMN external_bounty_programs.discloses_reports IS
    'Whether the programme publishes reports. Decides whether a finding there '
    'can ever be attested here — a disclosure nobody can read is a claim.';

CREATE INDEX idx_external_bounty_programs_live
    ON external_bounty_programs (platform, curated_at DESC) WHERE is_active;
CREATE INDEX idx_external_bounty_programs_topics
    ON external_bounty_programs USING gin (skill_topics);

CREATE TRIGGER trg_external_bounty_programs_updated_at
    BEFORE UPDATE ON external_bounty_programs
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();
