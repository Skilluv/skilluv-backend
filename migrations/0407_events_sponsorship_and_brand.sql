-- Business model — the brand line: events, sponsors, campaigns, ambassadors.
-- Migration 0233.
--
-- ## One events table, not two
--
-- Migration 0093 created `events` for badge stamps: a slug, a window, and a
-- visual theme, enough to say "this person was at Hacktoberfest". The brand
-- line needs the rest of what an event is — a type, a place, a jury, a
-- livestream, sponsors — and the temptation is a second table called
-- `brand_events` sitting next to it.
--
-- It is the same object. A hackathon issues a stamp *and* sells sponsorship,
-- and two tables would mean two slugs, two participant lists, and an event
-- that exists twice with different dates. This migration grows the one that
-- is already there.
--
-- `is_active` becomes derived rather than stored: it is now a generated
-- column over `status`, so the older readers keep working and the two can no
-- longer disagree about whether an event is on.
--
-- ## Sponsorship as tiers with rows, not as columns
--
-- The four packages are rows in `event_sponsorship_packages`, with their
-- benefits as data. Bronze through Platinum are what we sell today; the
-- prices will move, a fifth tier will appear, and none of that should be a
-- migration. What a tier *grants* — credits to contact finalists — is issued
-- through the entitlement machinery from 0229 rather than a counter column
-- here, because there is already one place that answers "what does this
-- company have the right to do".

-- ═══════════════════════════════════════════════════════════════════
-- Events grow up
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE events
    ADD COLUMN event_type VARCHAR(30) NOT NULL DEFAULT 'community_meetup'
        CHECK (event_type IN (
            'hackathon',
            'game_jam',
            'game_fest',
            'design_awards',
            'cyber_ctf_championship',
            'audio_jam',
            'ai_challenge_annual',
            'coding_marathon',
            'community_meetup',
            'conference'
        )),
    -- Which trades it is for. Empty means everybody, which is a real answer
    -- for a meetup and a warning sign for a championship.
    ADD COLUMN domain_focus TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN location_type VARCHAR(10) NOT NULL DEFAULT 'online'
        CHECK (location_type IN ('online', 'onsite', 'hybrid')),
    -- Address, room, travel notes. Free-form because a venue in Cotonou and
    -- one in Lagos do not describe themselves the same way.
    ADD COLUMN location_details JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN max_participants INTEGER
        CHECK (max_participants IS NULL OR max_participants > 0),
    ADD COLUMN showcase_page_url VARCHAR(500),
    ADD COLUMN status VARCHAR(20) NOT NULL DEFAULT 'published'
        CHECK (status IN ('draft', 'published', 'live', 'finished', 'cancelled'));

-- An onsite event with no address is one nobody can attend.
ALTER TABLE events
    ADD CONSTRAINT onsite_events_say_where CHECK (
        location_type = 'online'
        OR status = 'draft'
        OR location_details <> '{}'::jsonb
    );

-- Carry the old flag over before it stops being a stored column.
UPDATE events SET status = 'draft' WHERE is_active = FALSE;

ALTER TABLE events DROP COLUMN is_active;

-- Derived, so the flag and the status cannot drift apart. Every reader that
-- filtered on `is_active` keeps working and now gets the right answer for a
-- finished or cancelled event too.
ALTER TABLE events
    ADD COLUMN is_active BOOLEAN
        GENERATED ALWAYS AS (status IN ('published', 'live')) STORED;

COMMENT ON COLUMN events.is_active IS
    'Derived from status. Kept so the badge-stamp readers from 0093 keep '
    'working; no longer settable, so it cannot disagree with the status.';

-- Dropping the column took its index with it.
CREATE INDEX idx_events_active_starts ON events (is_active, starts_at DESC);
CREATE INDEX idx_events_type_window ON events (event_type, starts_at DESC);

-- ── Participation gains a role ─────────────────────────────────────
--
-- A person can be a juror and a speaker at the same event, so the role is
-- part of the key rather than a column on a single row.

ALTER TABLE user_event_participation
    ADD COLUMN role VARCHAR(20) NOT NULL DEFAULT 'participant'
        CHECK (role IN ('participant', 'jury', 'organizer', 'speaker', 'sponsor_rep'));

ALTER TABLE user_event_participation DROP CONSTRAINT user_event_participation_pkey;
ALTER TABLE user_event_participation
    ADD PRIMARY KEY (event_id, user_id, role);

-- The seat limit, held in the database because two people registering at the
-- same moment against the last place would both pass a check in the service.
CREATE OR REPLACE FUNCTION event_has_room()
RETURNS TRIGGER AS $$
DECLARE
    ev RECORD;
    taken INTEGER;
BEGIN
    -- Only ordinary participants take a seat. A jury is invited, not
    -- admitted, and counting them would close registration early.
    IF NEW.role <> 'participant' THEN
        RETURN NEW;
    END IF;

    SELECT status, max_participants INTO ev
      FROM events WHERE id = NEW.event_id FOR UPDATE;

    IF ev.status NOT IN ('published', 'live') THEN
        RAISE EXCEPTION 'this event is %, and is not taking registrations', ev.status;
    END IF;

    IF ev.max_participants IS NULL THEN
        RETURN NEW;
    END IF;

    SELECT count(*) INTO taken
      FROM user_event_participation
     WHERE event_id = NEW.event_id AND role = 'participant';

    IF taken >= ev.max_participants THEN
        RAISE EXCEPTION 'this event already has its % participants', ev.max_participants;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_event_has_room
    BEFORE INSERT ON user_event_participation
    FOR EACH ROW EXECUTE FUNCTION event_has_room();

-- ═══════════════════════════════════════════════════════════════════
-- What a sponsor buys
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE event_sponsorship_packages (
    tier VARCHAR(20) PRIMARY KEY,
    label VARCHAR(120) NOT NULL,
    -- The published price. What a sponsorship is actually signed at lives on
    -- the sponsorship row, because a negotiated price is a fact about that
    -- deal and not a correction to the grid.
    list_fee NUMERIC(12,2) NOT NULL CHECK (list_fee >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    -- Where the logo goes, what slots come with it. Data rather than columns:
    -- the list of benefits changes every season, and none of those changes
    -- should be a migration.
    benefits JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- Credits to contact finalists, granted as an entitlement when the
    -- sponsorship is signed.
    talent_access_credits INTEGER NOT NULL DEFAULT 0
        CHECK (talent_access_credits >= 0),
    keynote_slot BOOLEAN NOT NULL DEFAULT FALSE,
    physical_stand BOOLEAN NOT NULL DEFAULT FALSE,
    named_challenge BOOLEAN NOT NULL DEFAULT FALSE,
    branded_content_included BOOLEAN NOT NULL DEFAULT FALSE,
    sort_order SMALLINT NOT NULL DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT TRUE
);

COMMENT ON TABLE event_sponsorship_packages IS
    'The published sponsorship grid, as rows. Prices move and a fifth tier '
    'will appear; neither should be a migration.';

INSERT INTO event_sponsorship_packages
    (tier, label, list_fee, benefits, talent_access_credits, keynote_slot,
     physical_stand, named_challenge, branded_content_included, sort_order)
VALUES
    -- The figures are the ones published in docs/business/PRICING.md, and
    -- deliberately not the ones a European agency would quote: the first
    -- sponsors are Beninese and Nigerian companies, and a grid they cannot
    -- afford is a grid with no sponsors on it.
    ('bronze', 'Bronze', 460.00,
     '{"logo": "liste des partenaires", "acces": "profils des participants"}'::jsonb,
     5, FALSE, FALSE, FALSE, FALSE, 1),
    ('silver', 'Argent', 1400.00,
     '{"logo": "en évidence", "epreuve": "co-conçue", "prise_de_parole": true}'::jsonb,
     20, TRUE, FALSE, TRUE, FALSE, 2),
    ('gold', 'Or', 3800.00,
     '{"logo": "principal", "nom": "sur l''événement", "finalistes": "accompagnement"}'::jsonb,
     50, TRUE, TRUE, TRUE, TRUE, 3),
    ('platinum', 'Platine', 12000.00,
     '{"titre": "événement présenté par le sponsor", "rapport": "impact sur mesure"}'::jsonb,
     100, TRUE, TRUE, TRUE, TRUE, 4),
    -- Everything that is not the grid. Kept as a tier rather than a NULL, so
    -- a negotiated deal still has a row saying what it was.
    ('custom', 'Sur mesure', 0.00, '{}'::jsonb, 0, FALSE, FALSE, FALSE, FALSE, 5)
ON CONFLICT (tier) DO NOTHING;

CREATE TABLE event_sponsorships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id UUID NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    package_tier VARCHAR(20) NOT NULL REFERENCES event_sponsorship_packages(tier),

    -- What was actually agreed. Defaults to the grid; a discount or a custom
    -- deal writes its own number here rather than editing the grid.
    agreed_fee NUMERIC(12,2) NOT NULL CHECK (agreed_fee >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    -- Anything on top of the tier, or in place of it for a custom deal.
    extra_benefits JSONB NOT NULL DEFAULT '{}'::jsonb,
    logo_placement TEXT[] NOT NULL DEFAULT '{}',
    named_challenge_slug VARCHAR(80),

    -- The stand, when there is one. Two booleans rather than a separate table
    -- because a stand has no life of its own: it exists because a sponsorship
    -- does, ends when it does, and belongs to exactly one.
    physical_stand BOOLEAN NOT NULL DEFAULT FALSE,
    virtual_stand_url VARCHAR(500),

    -- Which annual contract it counts against, when it is part of one.
    annual_contract_id UUID,

    status VARCHAR(20) NOT NULL DEFAULT 'proposed' CHECK (status IN (
        'proposed',
        'negotiating',
        'signed',
        -- Delivered and paid. What the revenue line is booked against.
        'honoured',
        'cancelled'
    )),
    declined_reason TEXT,
    signed_at TIMESTAMPTZ,
    honoured_at TIMESTAMPTZ,

    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One sponsorship per company per event. A company wanting more buys a
    -- higher tier; two rows would mean two logos for the same sponsor.
    UNIQUE (event_id, enterprise_id),

    CONSTRAINT cancellation_carries_a_reason CHECK (
        status <> 'cancelled'
        OR (declined_reason IS NOT NULL AND btrim(declined_reason) <> '')
    ),
    CONSTRAINT signing_is_dated CHECK (
        status NOT IN ('signed', 'honoured') OR signed_at IS NOT NULL
    ),
    CONSTRAINT a_virtual_stand_has_a_page CHECK (
        virtual_stand_url IS NULL OR virtual_stand_url ~ '^https://'
    )
);

COMMENT ON TABLE event_sponsorships IS
    'One company sponsoring one event. The agreed fee sits here rather than '
    'on the grid: a negotiated price is a fact about that deal, not a '
    'correction to what we publish.';

CREATE INDEX idx_sponsorships_event ON event_sponsorships (event_id, status);
CREATE INDEX idx_sponsorships_enterprise
    ON event_sponsorships (enterprise_id, created_at DESC);

CREATE OR REPLACE FUNCTION touch_brand_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_sponsorships_updated_at
    BEFORE UPDATE ON event_sponsorships
    FOR EACH ROW EXECUTE FUNCTION touch_brand_updated_at();

-- ── Leads a stand collects ─────────────────────────────────────────
--
-- A person who walked up to the stand and said they were interested. Not a
-- profile view and not a search result: an act by the person named, which is
-- what makes it lawful to hand their details to the sponsor.

CREATE TABLE sponsorship_leads (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sponsorship_id UUID NOT NULL REFERENCES event_sponsorships(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    -- What the person actually did. The sponsor gets the contact because of
    -- this, so it is recorded rather than assumed.
    interaction VARCHAR(20) NOT NULL CHECK (interaction IN (
        'stand_visit', 'demo_booked', 'question_asked', 'cv_shared'
    )),
    note TEXT,
    -- Consent is the whole basis for passing the details on. Without it the
    -- row is a visit log and nothing leaves Skilluv.
    contact_consent BOOLEAN NOT NULL DEFAULT FALSE,
    exported_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (sponsorship_id, user_id, interaction),

    CONSTRAINT nothing_leaves_without_consent CHECK (
        exported_at IS NULL OR contact_consent
    )
);

COMMENT ON CONSTRAINT nothing_leaves_without_consent ON sponsorship_leads IS
    'A lead is an act by the person named. Without their consent the row is '
    'a visit log, and nothing about them leaves Skilluv.';

CREATE INDEX idx_leads_sponsorship ON sponsorship_leads (sponsorship_id, created_at DESC);

-- ── Annual contracts ───────────────────────────────────────────────

CREATE TABLE annual_sponsorship_contracts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    year SMALLINT NOT NULL CHECK (year BETWEEN 2025 AND 2100),

    total_fee NUMERIC(12,2) NOT NULL CHECK (total_fee > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    -- How many events the contract covers. The discount is what is paid for
    -- committing to the number, so both are on the same row.
    max_events SMALLINT NOT NULL CHECK (max_events BETWEEN 2 AND 50),
    volume_discount_percent NUMERIC(4,2) NOT NULL DEFAULT 0
        CHECK (volume_discount_percent >= 0 AND volume_discount_percent <= 50),

    contract_url VARCHAR(500),
    signed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (enterprise_id, year),

    CONSTRAINT a_discount_needs_a_signature CHECK (
        volume_discount_percent = 0 OR signed_at IS NOT NULL
    )
);

COMMENT ON TABLE annual_sponsorship_contracts IS
    'A year bought at once. Which events it covers is read from the '
    'sponsorships pointing at it, not from an array kept in step by hand.';

-- Which events a contract covers is the set of sponsorships pointing at it.
-- An array column would have been a second copy of the same fact, and the
-- two would disagree the first time a sponsorship was cancelled.
ALTER TABLE event_sponsorships
    ADD CONSTRAINT event_sponsorships_annual_contract_fkey
        FOREIGN KEY (annual_contract_id)
        REFERENCES annual_sponsorship_contracts(id) ON DELETE SET NULL;

CREATE INDEX idx_sponsorships_annual
    ON event_sponsorships (annual_contract_id)
    WHERE annual_contract_id IS NOT NULL;

-- A contract cannot cover more events than it was sold for.
CREATE OR REPLACE FUNCTION annual_contract_has_room()
RETURNS TRIGGER AS $$
DECLARE
    allowed SMALLINT;
    used INTEGER;
BEGIN
    IF NEW.annual_contract_id IS NULL THEN
        RETURN NEW;
    END IF;

    SELECT max_events INTO allowed
      FROM annual_sponsorship_contracts
     WHERE id = NEW.annual_contract_id FOR UPDATE;

    SELECT count(*) INTO used
      FROM event_sponsorships
     WHERE annual_contract_id = NEW.annual_contract_id
       AND status <> 'cancelled'
       AND id <> NEW.id;

    IF used >= allowed THEN
        RAISE EXCEPTION
            'this contract covers % events and they are all used', allowed;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_annual_contract_has_room
    BEFORE INSERT OR UPDATE OF annual_contract_id ON event_sponsorships
    FOR EACH ROW EXECUTE FUNCTION annual_contract_has_room();

-- ═══════════════════════════════════════════════════════════════════
-- Livestreams, and the people who pay to watch
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE event_livestreams (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id UUID NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    platform VARCHAR(20) NOT NULL CHECK (platform IN (
        'youtube', 'twitch', 'linkedin', 'self_hosted'
    )),
    url VARCHAR(500) NOT NULL CHECK (url ~ '^https://'),
    -- Which sponsors appear in the stream itself. Separate from the
    -- sponsorship because a sponsor can buy a stream slot without sponsoring
    -- the event, and a sponsor of the event can decline the slot.
    sponsor_ids UUID[] NOT NULL DEFAULT '{}',
    -- Replays, backstage, the jury Q&A. What a subscription buys.
    premium_content_available BOOLEAN NOT NULL DEFAULT FALSE,
    replay_url VARCHAR(500),
    starts_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (event_id, platform)
);

CREATE INDEX idx_livestreams_event ON event_livestreams (event_id);

-- ── A person paying Skilluv for access ─────────────────────────────
--
-- The first thing on the platform an individual pays for, and it is worth
-- saying what it is not: not access to challenges, not a better rank, not
-- visibility. The rule is that talents do not pay to be seen. Watching a
-- replay in HD is not being seen, which is why this one is allowed to exist.

CREATE TABLE audience_plans (
    slug VARCHAR(40) PRIMARY KEY,
    label VARCHAR(120) NOT NULL,
    description TEXT NOT NULL,
    price NUMERIC(8,2) NOT NULL CHECK (price > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    period VARCHAR(10) NOT NULL CHECK (period IN ('monthly', 'yearly')),
    -- The revenue stream it feeds, so an accountant reads one table.
    revenue_stream VARCHAR(60) NOT NULL REFERENCES revenue_streams(slug),
    is_active BOOLEAN NOT NULL DEFAULT TRUE
);

COMMENT ON TABLE audience_plans IS
    'What an individual can pay Skilluv for. Deliberately short: talents do '
    'not pay to be seen, so nothing here sells visibility, ranking or access '
    'to work.';

CREATE TABLE audience_subscriptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    plan VARCHAR(40) NOT NULL REFERENCES audience_plans(slug),

    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- When the paid period runs out. Access is read from this rather than
    -- from a status column, so a lapsed subscription cannot be left "active"
    -- by a job that failed to run.
    expires_at TIMESTAMPTZ NOT NULL,
    cancelled_at TIMESTAMPTZ,
    auto_renew BOOLEAN NOT NULL DEFAULT TRUE,

    payment_provider VARCHAR(20),
    payment_reference VARCHAR(200),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_period_runs_forward CHECK (expires_at > started_at)
);

-- One live subscription per person per plan. Renewing extends the row it
-- already has; a second row would mean two charges for one access.
CREATE UNIQUE INDEX idx_audience_one_live_per_plan
    ON audience_subscriptions (user_id, plan)
    WHERE cancelled_at IS NULL;

CREATE INDEX idx_audience_expiring
    ON audience_subscriptions (expires_at)
    WHERE cancelled_at IS NULL AND auto_renew;

-- ═══════════════════════════════════════════════════════════════════
-- Sponsored content, and saying so
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE event_sponsored_content (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id UUID REFERENCES events(id) ON DELETE SET NULL,
    sponsor_enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,

    content_type VARCHAR(20) NOT NULL CHECK (content_type IN (
        'blog_post', 'video', 'newsletter', 'podcast', 'recap'
    )),
    title VARCHAR(200) NOT NULL CHECK (btrim(title) <> ''),
    content_url VARCHAR(500),
    fee NUMERIC(10,2) NOT NULL CHECK (fee >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    -- Not a flag somebody can forget. Sponsored content that does not say it
    -- is sponsored is the fastest way to lose an audience, so the wording is
    -- stored with the piece and the piece cannot go out without it.
    disclosure_text TEXT NOT NULL CHECK (length(btrim(disclosure_text)) >= 10),

    author_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'commissioned' CHECK (status IN (
        'commissioned', 'drafting', 'in_review', 'published', 'cancelled'
    )),
    published_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT published_content_has_a_url CHECK (
        status <> 'published'
        OR (content_url IS NOT NULL AND content_url ~ '^https://')
    )
);

COMMENT ON COLUMN event_sponsored_content.disclosure_text IS
    'Stored with the piece and required to exist. Sponsored content that '
    'does not say it is sponsored is the fastest way to lose an audience.';

CREATE INDEX idx_sponsored_content_sponsor
    ON event_sponsored_content (sponsor_enterprise_id, created_at DESC);

-- ═══════════════════════════════════════════════════════════════════
-- Product launch campaigns
-- ═══════════════════════════════════════════════════════════════════
--
-- A company launches something and asks the community to write about it,
-- film it, integrate it. Skilluv charges to run the campaign; the community
-- is paid per accepted piece. Two amounts, kept apart for the same reason as
-- the beta programmes: a client should see what goes to the people who did
-- the work.

CREATE TABLE product_launch_campaigns (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    product_name VARCHAR(200) NOT NULL CHECK (btrim(product_name) <> ''),
    brief_md TEXT NOT NULL CHECK (btrim(brief_md) <> ''),
    product_launch_date DATE NOT NULL,

    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,

    content_types_wanted TEXT[] NOT NULL
        CHECK (cardinality(content_types_wanted) > 0),

    -- The pot, and what one accepted piece is worth out of it.
    reward_pool NUMERIC(12,2) NOT NULL CHECK (reward_pool > 0),
    reward_per_piece NUMERIC(8,2) NOT NULL CHECK (reward_per_piece > 0),
    -- What Skilluv charges to run it.
    campaign_fee NUMERIC(10,2) NOT NULL CHECK (campaign_fee >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    status VARCHAR(20) NOT NULL DEFAULT 'briefing' CHECK (status IN (
        'briefing', 'open', 'reviewing', 'closed', 'cancelled'
    )),
    closed_reason TEXT,
    closed_at TIMESTAMPTZ,

    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_campaign_window_runs_forward CHECK (ends_at > starts_at),
    -- The pot has to buy at least one piece. A pool smaller than a single
    -- reward is a campaign nobody can be paid from, and the person who finds
    -- out is the one who already wrote the article.
    CONSTRAINT the_pool_buys_at_least_one_piece CHECK (reward_pool >= reward_per_piece),
    CONSTRAINT cancellation_carries_a_reason CHECK (
        status <> 'cancelled'
        OR (closed_reason IS NOT NULL AND btrim(closed_reason) <> '')
    )
);

COMMENT ON CONSTRAINT the_pool_buys_at_least_one_piece ON product_launch_campaigns IS
    'A pool smaller than one reward is a campaign nobody can be paid from, '
    'and the person who finds out is the one who already wrote the article.';

CREATE INDEX idx_launch_campaigns_open
    ON product_launch_campaigns (ends_at)
    WHERE status = 'open';
CREATE INDEX idx_launch_campaigns_enterprise
    ON product_launch_campaigns (enterprise_id, created_at DESC);

CREATE TRIGGER trg_launch_campaigns_updated_at
    BEFORE UPDATE ON product_launch_campaigns
    FOR EACH ROW EXECUTE FUNCTION touch_brand_updated_at();

CREATE TABLE launch_campaign_pieces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    campaign_id UUID NOT NULL REFERENCES product_launch_campaigns(id) ON DELETE CASCADE,
    author_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    content_type VARCHAR(30) NOT NULL,
    title VARCHAR(200) NOT NULL CHECK (btrim(title) <> ''),
    url VARCHAR(500) NOT NULL CHECK (url ~ '^https://'),
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Two gates, and the order matters. Skilluv checks the work is real and
    -- not spun from the press release; the sponsor then decides whether it
    -- serves them. Skipping the first would let a company reject honest
    -- criticism as "quality"; skipping the second would bill them for
    -- something they never wanted.
    quality_reviewed_by UUID REFERENCES users(id) ON DELETE SET NULL,
    quality_reviewed_at TIMESTAMPTZ,
    quality_notes TEXT,

    status VARCHAR(20) NOT NULL DEFAULT 'submitted' CHECK (status IN (
        'submitted',
        'quality_passed',
        'quality_failed',
        'accepted',
        'rejected'
    )),
    rejection_reason TEXT,
    decided_at TIMESTAMPTZ,
    reward_paid_at TIMESTAMPTZ,

    UNIQUE (campaign_id, url),

    CONSTRAINT rejection_carries_a_reason CHECK (
        status NOT IN ('quality_failed', 'rejected')
        OR (rejection_reason IS NOT NULL AND btrim(rejection_reason) <> '')
    ),
    CONSTRAINT the_sponsor_sees_nothing_unreviewed CHECK (
        status NOT IN ('accepted', 'rejected') OR quality_reviewed_at IS NOT NULL
    ),
    CONSTRAINT only_accepted_work_is_paid CHECK (
        reward_paid_at IS NULL OR status = 'accepted'
    )
);

COMMENT ON CONSTRAINT the_sponsor_sees_nothing_unreviewed ON launch_campaign_pieces IS
    'Skilluv checks the work is real before the sponsor decides whether it '
    'serves them. Without the first gate, honest criticism gets rejected as '
    '"quality"; without the second, a client is billed for what they never '
    'asked for.';

CREATE INDEX idx_launch_pieces_campaign
    ON launch_campaign_pieces (campaign_id, submitted_at DESC);
CREATE INDEX idx_launch_pieces_payable
    ON launch_campaign_pieces (campaign_id)
    WHERE status = 'accepted' AND reward_paid_at IS NULL;

-- ═══════════════════════════════════════════════════════════════════
-- Ambassadors
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE ambassador_programs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    name VARCHAR(200) NOT NULL CHECK (btrim(name) <> ''),
    brief_md TEXT NOT NULL CHECK (btrim(brief_md) <> ''),

    target_count SMALLINT NOT NULL CHECK (target_count BETWEEN 1 AND 200),
    monthly_stipend NUMERIC(8,2) NOT NULL CHECK (monthly_stipend > 0),
    expected_deliverables_per_month SMALLINT NOT NULL DEFAULT 1
        CHECK (expected_deliverables_per_month BETWEEN 1 AND 20),
    duration_months SMALLINT NOT NULL CHECK (duration_months BETWEEN 1 AND 36),

    swag_included BOOLEAN NOT NULL DEFAULT TRUE,
    preview_products_access BOOLEAN NOT NULL DEFAULT TRUE,

    -- What Skilluv charges to set it up, and to run it each month.
    activation_fee NUMERIC(10,2) NOT NULL CHECK (activation_fee >= 0),
    management_monthly_fee NUMERIC(8,2) NOT NULL DEFAULT 0
        CHECK (management_monthly_fee >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    -- The floor for being asked. An ambassador speaks for a company under
    -- their own name, and the rank is what says the name means something.
    minimum_rank VARCHAR(20) NOT NULL DEFAULT 'artisan',

    status VARCHAR(20) NOT NULL DEFAULT 'recruiting' CHECK (status IN (
        'recruiting', 'running', 'finished', 'cancelled'
    )),
    closed_reason TEXT,
    started_at TIMESTAMPTZ,
    ends_at TIMESTAMPTZ,

    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT cancellation_carries_a_reason CHECK (
        status <> 'cancelled'
        OR (closed_reason IS NOT NULL AND btrim(closed_reason) <> '')
    )
);

COMMENT ON TABLE ambassador_programs IS
    'People who speak for a company under their own name, paid a monthly '
    'stipend. The rank floor is what makes the name worth borrowing.';

CREATE INDEX idx_ambassador_programs_recruiting
    ON ambassador_programs (created_at DESC)
    WHERE status = 'recruiting';
CREATE INDEX idx_ambassador_programs_enterprise
    ON ambassador_programs (enterprise_id, created_at DESC);

CREATE TRIGGER trg_ambassador_programs_updated_at
    BEFORE UPDATE ON ambassador_programs
    FOR EACH ROW EXECUTE FUNCTION touch_brand_updated_at();

CREATE TABLE program_ambassadors (
    program_id UUID NOT NULL REFERENCES ambassador_programs(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    invited_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Their own answer. An ambassadorship lends somebody's name to a company,
    -- and nobody else can agree to that on their behalf.
    accepted_at TIMESTAMPTZ,
    declined_at TIMESTAMPTZ,
    onboarded_at TIMESTAMPTZ,
    left_at TIMESTAMPTZ,
    left_reason TEXT,

    status VARCHAR(20) NOT NULL DEFAULT 'invited' CHECK (status IN (
        'invited', 'active', 'paused', 'left'
    )),

    PRIMARY KEY (program_id, user_id),

    CONSTRAINT not_both_answers CHECK (accepted_at IS NULL OR declined_at IS NULL),
    CONSTRAINT nobody_is_active_without_agreeing CHECK (
        status <> 'active' OR accepted_at IS NOT NULL
    )
);

COMMENT ON CONSTRAINT nobody_is_active_without_agreeing ON program_ambassadors IS
    'An ambassadorship lends somebody name to a company. Nobody else can '
    'agree to that on their behalf.';

CREATE INDEX idx_program_ambassadors_user ON program_ambassadors (user_id, invited_at DESC);

-- What they delivered, and what was paid for it.
--
-- A count column on the row above would have been the same fact stored
-- twice, and the copy would be wrong the first time a piece was retracted.

CREATE TABLE ambassador_deliverables (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    program_id UUID NOT NULL,
    user_id UUID NOT NULL,

    -- The month it counts for, as its first day. Stipends are monthly, so
    -- what matters is which month a piece belongs to and not the hour.
    counts_for_month DATE NOT NULL,
    kind VARCHAR(30) NOT NULL,
    url VARCHAR(500) CHECK (url IS NULL OR url ~ '^https://'),
    note TEXT,
    accepted BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    FOREIGN KEY (program_id, user_id)
        REFERENCES program_ambassadors(program_id, user_id) ON DELETE CASCADE,

    CONSTRAINT a_month_is_its_first_day CHECK (
        EXTRACT(DAY FROM counts_for_month) = 1
    )
);

CREATE INDEX idx_ambassador_deliverables_month
    ON ambassador_deliverables (program_id, user_id, counts_for_month);

-- The monthly stipend, once paid, and once only.
CREATE TABLE ambassador_stipends (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    program_id UUID NOT NULL,
    user_id UUID NOT NULL,
    counts_for_month DATE NOT NULL,

    amount NUMERIC(8,2) NOT NULL CHECK (amount > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    -- How many pieces were counted. Recorded at payment because the stipend
    -- was decided on that number, and later retractions must not rewrite what
    -- was paid.
    deliverables_counted SMALLINT NOT NULL DEFAULT 0,
    paid_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    FOREIGN KEY (program_id, user_id)
        REFERENCES program_ambassadors(program_id, user_id) ON DELETE CASCADE,

    -- One stipend per person per month. A retry that paid twice would be
    -- found by an accountant, months later, if at all.
    UNIQUE (program_id, user_id, counts_for_month),

    CONSTRAINT a_month_is_its_first_day CHECK (
        EXTRACT(DAY FROM counts_for_month) = 1
    )
);

COMMENT ON TABLE ambassador_stipends IS
    'One row per person per month, enforced. A retry that paid twice would '
    'be found by an accountant months later, if at all.';

-- ═══════════════════════════════════════════════════════════════════
-- The revenue streams and products these feed
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO revenue_streams (slug, pillar, label, description, recurring) VALUES
    ('product_launch_campaign', 'brand', 'Campagne de lancement',
     'Ce que Skilluv facture pour organiser une campagne de contenu autour '
     'du lancement d''un produit.',
     FALSE),
    ('ambassador_program_fee', 'brand', 'Programme ambassadeurs',
     'Les frais d''activation et de gestion d''un programme d''ambassadeurs.',
     TRUE),
    ('audience_subscription', 'brand', 'Abonnement audience',
     'Un abonnement individuel aux rediffusions et aux contenus d''événement.',
     TRUE)
ON CONFLICT (slug) DO NOTHING;

INSERT INTO audience_plans
    (slug, label, description, price, currency, period, revenue_stream)
VALUES
    ('event_replays_annual', 'Rediffusions — accès annuel',
     'Accès aux rediffusions en haute définition, aux coulisses et aux '
     'questions au jury de tous les événements Skilluv.',
     10.00, 'EUR', 'yearly', 'audience_subscription')
ON CONFLICT (slug) DO NOTHING;

INSERT INTO enterprise_product_types
    (slug, label, description, revenue_stream, recurring)
VALUES
    ('annual_sponsorship', 'Sponsoring annuel',
     'Un contrat couvrant plusieurs événements sur une année.',
     'event_sponsorship', TRUE),
    ('product_launch_campaign', 'Campagne de lancement',
     'Une campagne de contenu communautaire autour d''un lancement produit.',
     'product_launch_campaign', FALSE),
    ('sponsored_content', 'Contenu sponsorisé',
     'Un article, une vidéo ou un épisode financé par une entreprise.',
     'media_sponsor_content', FALSE)
ON CONFLICT (slug) DO NOTHING;

UPDATE enterprise_product_types
   SET revenue_stream = 'ambassador_program_fee'
 WHERE slug = 'corporate_ambassador' AND revenue_stream IS DISTINCT FROM 'ambassador_program_fee';
