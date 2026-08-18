-- Business model — the data line.
-- Migration 0235.
--
-- ## The one thing that has to be right
--
-- Every product in this section sells something about people who are not the
-- customer. A score API for recruiters, a report for a development bank, a
-- licence for a research lab, an aggregated identity for a credit check —
-- all of them describe somebody who joined Skilluv to find work, not to be
-- an entry in a dataset.
--
-- So consent is not a boolean and not a setting. It is one row per person per
-- purpose, dated, revocable, with the revocation kept rather than deleted.
-- Somebody happy to appear in a public score API has not thereby agreed to be
-- sold to a bank, and a single flag would have made those the same decision.
--
-- The default is no. Every table below reads the consent rows rather than
-- keeping its own copy of who is in scope: an array of covered user ids would
-- be a second copy of a decision people can change, and the copy would be
-- wrong the first time somebody changed their mind.
--
-- ## One white-label table, not two
--
-- A government instance is a white-label deployment whose partner happens to
-- be a ministry and whose attestations carry official recognition. Two tables
-- would have meant two deployment paths, two branding configs and two places
-- to get the feature list wrong.

-- ═══════════════════════════════════════════════════════════════════
-- Consent
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE data_purposes (
    slug VARCHAR(40) PRIMARY KEY,
    label VARCHAR(120) NOT NULL,
    -- What the person is actually agreeing to, in words they can read
    -- without a lawyer. Stored rather than hard-coded in the front end so
    -- that what was agreed to can be shown back exactly as it was worded.
    description TEXT NOT NULL,
    -- Whether Skilluv earns from it. Decides whether a royalty is owed, and
    -- makes the distinction visible on the consent screen: people should
    -- know which of these makes money.
    commercial BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE data_purposes IS
    'What somebody can be asked to agree to, one row each. Separate purposes '
    'because agreeing to appear in a public score API is not agreeing to be '
    'sold to a bank.';

INSERT INTO data_purposes (slug, label, description, commercial) VALUES
    ('public_score_api',
     'Score public via l''API',
     'Votre score d''artisanat, votre rang et le nombre de vos attestations '
     'peuvent être lus par des outils tiers (ATS, plateformes) via notre API '
     'publique. Ni votre adresse, ni vos coordonnées, ni le détail de vos '
     'projets privés.',
     TRUE),
    ('research_licensing',
     'Recherche académique',
     'Vos données, agrégées et anonymisées, peuvent entrer dans des jeux de '
     'données cédés à des laboratoires et des institutions publiques. Aucune '
     'ligne ne vous nomme.',
     FALSE),
    ('commercial_licensing',
     'Licence commerciale',
     'Vos données, agrégées et anonymisées, peuvent entrer dans des jeux de '
     'données vendus à des entreprises. Une part des revenus vous revient.',
     TRUE),
    ('identity_aggregation',
     'Profil unifié',
     'Skilluv peut agréger votre activité publique sur d''autres plateformes '
     '(GitHub, publications, paquets) en un profil unique, et le montrer aux '
     'partenaires que vous aurez autorisés.',
     TRUE)
ON CONFLICT (slug) DO NOTHING;

CREATE TABLE talent_data_consent (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    purpose VARCHAR(40) NOT NULL REFERENCES data_purposes(slug) ON DELETE RESTRICT,

    granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Kept rather than deleted. A revoked consent is evidence that consent
    -- once existed for the period a dataset was built in, and deleting the
    -- row would make that unprovable in exactly the audit where it matters.
    revoked_at TIMESTAMPTZ,

    -- The share of a licence fee that comes back to this person. Per person
    -- because it is negotiable — a dataset built mostly from one cohort can
    -- pay that cohort more.
    revenue_share_percent NUMERIC(5,2) NOT NULL DEFAULT 1.00
        CHECK (revenue_share_percent >= 0 AND revenue_share_percent <= 10),

    -- The exact wording agreed to, copied at the moment of agreement. The
    -- description in `data_purposes` will be improved over time, and consent
    -- given to the old wording was not given to the new one.
    wording_agreed TEXT NOT NULL,

    PRIMARY KEY (user_id, purpose)
);

COMMENT ON COLUMN talent_data_consent.wording_agreed IS
    'The exact text agreed to, copied at that moment. The purpose '
    'description will be improved over time, and consent to the old wording '
    'is not consent to the new.';

COMMENT ON COLUMN talent_data_consent.revoked_at IS
    'Kept rather than deleted. A revoked consent proves consent existed for '
    'the period a dataset was built in, which is exactly the audit where it '
    'matters.';

-- The read path every product in this section uses.
CREATE INDEX idx_consent_live
    ON talent_data_consent (purpose, user_id)
    WHERE revoked_at IS NULL;

CREATE OR REPLACE FUNCTION has_data_consent(target_user UUID, target_purpose TEXT)
RETURNS BOOLEAN AS $$
    SELECT EXISTS (
        SELECT 1 FROM talent_data_consent
         WHERE user_id = target_user
           AND purpose = target_purpose
           AND revoked_at IS NULL
    );
$$ LANGUAGE SQL STABLE;

COMMENT ON FUNCTION has_data_consent IS
    'The single answer to "may we". Written once so no product invents its '
    'own version and gets it slightly wrong.';

-- ═══════════════════════════════════════════════════════════════════
-- The metered API
-- ═══════════════════════════════════════════════════════════════════
--
-- `api_keys` from migration 0018 is per-person with a flat rate limit. The
-- data line sells to companies with quotas and a bill, so the key grows an
-- owner and a plan rather than being replaced: the existing keys keep
-- working, on the free plan, which is what they already were.

CREATE TABLE api_plans (
    slug VARCHAR(30) PRIMARY KEY,
    label VARCHAR(80) NOT NULL,
    -- Requests per calendar month. NULL is unmetered, reserved for the
    -- negotiated tier.
    monthly_quota INTEGER CHECK (monthly_quota IS NULL OR monthly_quota > 0),
    -- A ceiling per day as well, so one runaway script cannot spend a
    -- month's quota before anybody notices.
    daily_ceiling INTEGER CHECK (daily_ceiling IS NULL OR daily_ceiling > 0),
    monthly_fee NUMERIC(10,2) NOT NULL CHECK (monthly_fee >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    -- Whether the caller has to name Skilluv where the data appears. The
    -- price of the free tier.
    attribution_required BOOLEAN NOT NULL DEFAULT FALSE,
    sla BOOLEAN NOT NULL DEFAULT FALSE,
    sort_order SMALLINT NOT NULL DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT TRUE
);

INSERT INTO api_plans
    (slug, label, monthly_quota, daily_ceiling, monthly_fee, attribution_required,
     sla, sort_order)
VALUES
    ('free', 'Gratuit', 3000, 100, 0.00, TRUE, FALSE, 1),
    ('startup', 'Startup', 10000, 1000, 100.00, FALSE, FALSE, 2),
    ('business', 'Business', 100000, 10000, 500.00, FALSE, FALSE, 3),
    ('enterprise', 'Entreprise', NULL, NULL, 0.00, FALSE, TRUE, 4)
ON CONFLICT (slug) DO NOTHING;

COMMENT ON TABLE api_plans IS
    'What API access costs. The enterprise tier has no quota and no listed '
    'price: it is negotiated, and a number here would be a guess printed as '
    'a fact.';

ALTER TABLE api_keys
    -- Whose key it is when a company holds it. The person who created it is
    -- still `user_id`, because somebody has to be answerable for a key.
    ADD COLUMN enterprise_id UUID REFERENCES enterprises(id) ON DELETE CASCADE,
    ADD COLUMN plan VARCHAR(30) NOT NULL DEFAULT 'free' REFERENCES api_plans(slug),
    -- What this key is allowed to read. Separate from `permissions` because
    -- that column is about the caller's own profile, and these are about
    -- other people's.
    ADD COLUMN data_scopes TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN revoked_at TIMESTAMPTZ,
    ADD COLUMN revoked_reason TEXT;

CREATE INDEX idx_api_keys_enterprise
    ON api_keys (enterprise_id)
    WHERE enterprise_id IS NOT NULL AND revoked_at IS NULL;

-- Usage, one row per key per day.
--
-- Per day rather than per request: a row per call would be the largest table
-- in the database within a year and would answer no question the daily count
-- does not. The billing period is a month, and the ceiling is a day.
CREATE TABLE api_usage_daily (
    api_key_id UUID NOT NULL REFERENCES api_keys(id) ON DELETE CASCADE,
    used_on DATE NOT NULL,
    requests INTEGER NOT NULL DEFAULT 0 CHECK (requests >= 0),
    -- Calls refused for being over quota. Counted separately so a client
    -- asking "why did it stop working" gets an answer rather than a shrug.
    throttled INTEGER NOT NULL DEFAULT 0 CHECK (throttled >= 0),

    PRIMARY KEY (api_key_id, used_on)
);

CREATE INDEX idx_api_usage_month ON api_usage_daily (used_on);

-- ═══════════════════════════════════════════════════════════════════
-- Reports sold to institutions
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE talent_intelligence_reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    client_type VARCHAR(30) NOT NULL CHECK (client_type IN (
        'government', 'development_bank', 'foundation', 'university',
        'consulting_firm', 'enterprise'
    )),
    client_org VARCHAR(200) NOT NULL CHECK (btrim(client_org) <> ''),
    -- The company, when the client is one of ours. NULL for a ministry or a
    -- foundation, which is why the name is a text column as well.
    enterprise_id UUID REFERENCES enterprises(id) ON DELETE SET NULL,

    title VARCHAR(200) NOT NULL CHECK (btrim(title) <> ''),
    scope_md TEXT NOT NULL CHECK (btrim(scope_md) <> ''),
    delivery_formats TEXT[] NOT NULL DEFAULT '{pdf}'
        CHECK (cardinality(delivery_formats) > 0),

    fee NUMERIC(12,2) NOT NULL CHECK (fee >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    -- How few people a published figure may rest on. A "skills gap" chart
    -- drawn from four people in one town names those four whatever the
    -- header says.
    minimum_cohort_size INTEGER NOT NULL DEFAULT 30
        CHECK (minimum_cohort_size >= 20),

    author_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'requested' CHECK (status IN (
        'requested', 'scoping', 'in_production', 'delivered', 'cancelled'
    )),
    cancelled_reason TEXT,
    delivered_at TIMESTAMPTZ,
    document_url VARCHAR(500),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_delivered_report_has_a_document CHECK (
        status <> 'delivered'
        OR (document_url IS NOT NULL AND document_url ~ '^https://')
    ),
    CONSTRAINT cancellation_carries_a_reason CHECK (
        status <> 'cancelled'
        OR (cancelled_reason IS NOT NULL AND btrim(cancelled_reason) <> '')
    )
);

COMMENT ON COLUMN talent_intelligence_reports.minimum_cohort_size IS
    'How few people a published figure may rest on. A skills-gap chart drawn '
    'from four people in one town names those four, whatever the header '
    'says.';

-- Not `idx_reports_status`: migration 0013 took that name for the moderation
-- reports, and index names are database-wide even when the tables are not.
CREATE INDEX idx_intelligence_reports_status
    ON talent_intelligence_reports (status, created_at DESC);

CREATE OR REPLACE FUNCTION touch_data_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_reports_updated_at
    BEFORE UPDATE ON talent_intelligence_reports
    FOR EACH ROW EXECUTE FUNCTION touch_data_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- Licensing, and what comes back to the people in the data
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE data_licensing_contracts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    licensee_org VARCHAR(200) NOT NULL CHECK (btrim(licensee_org) <> ''),
    licensee_type VARCHAR(30) NOT NULL CHECK (licensee_type IN (
        'research_lab', 'university', 'government', 'development_bank',
        'enterprise', 'ngo'
    )),

    -- Which consent this contract runs on. A commercial licensee cannot be
    -- served from research consent, and the foreign key is what stops
    -- somebody deciding otherwise in a hurry.
    purpose VARCHAR(40) NOT NULL REFERENCES data_purposes(slug) ON DELETE RESTRICT,
    contract_purpose_md TEXT NOT NULL CHECK (btrim(contract_purpose_md) <> ''),
    data_scope JSONB NOT NULL DEFAULT '{}'::jsonb,

    starts_on DATE NOT NULL,
    ends_on DATE,

    total_fee NUMERIC(12,2) NOT NULL CHECK (total_fee > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    -- The share of the fee that goes back to the people in the dataset,
    -- divided between them. Not a gift: it is why they said yes.
    talents_share_percent NUMERIC(5,2) NOT NULL DEFAULT 1.00
        CHECK (talents_share_percent >= 0 AND talents_share_percent <= 20),

    contract_url VARCHAR(500),
    signed_at TIMESTAMPTZ,
    status VARCHAR(20) NOT NULL DEFAULT 'negotiating' CHECK (status IN (
        'negotiating', 'signed', 'active', 'expired', 'terminated'
    )),
    terminated_reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_contract_runs_forward CHECK (ends_on IS NULL OR ends_on > starts_on),
    CONSTRAINT an_active_contract_is_signed CHECK (
        status NOT IN ('signed', 'active') OR signed_at IS NOT NULL
    ),
    -- A commercial licence has to pay the people in it. Zero is defensible
    -- for a public research dataset and is not for a sale.
    CONSTRAINT a_commercial_licence_pays_the_people_in_it CHECK (
        purpose <> 'commercial_licensing' OR talents_share_percent > 0
    ),
    CONSTRAINT termination_carries_a_reason CHECK (
        status <> 'terminated'
        OR (terminated_reason IS NOT NULL AND btrim(terminated_reason) <> '')
    )
);

COMMENT ON TABLE data_licensing_contracts IS
    'Who has a licence, for what, and under which consent. Who is covered is '
    'read from the consent rows, never copied here: an array of user ids '
    'would be a stale copy of a decision people can change.';

COMMENT ON CONSTRAINT a_commercial_licence_pays_the_people_in_it
    ON data_licensing_contracts IS
    'Zero is defensible for a public research dataset. It is not defensible '
    'for a sale.';

CREATE INDEX idx_licensing_active
    ON data_licensing_contracts (purpose, starts_on)
    WHERE status = 'active';

CREATE TRIGGER trg_licensing_updated_at
    BEFORE UPDATE ON data_licensing_contracts
    FOR EACH ROW EXECUTE FUNCTION touch_data_updated_at();

-- What each person is owed for a period.
CREATE TABLE talent_data_royalties (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    contract_id UUID NOT NULL REFERENCES data_licensing_contracts(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    period_start DATE NOT NULL,
    period_end DATE NOT NULL,
    amount NUMERIC(10,2) NOT NULL CHECK (amount >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    -- How many people shared the pot that period. Recorded because the share
    -- was worked out from it, and somebody joining later must not change what
    -- was already paid.
    cohort_size INTEGER NOT NULL CHECK (cohort_size > 0),

    paid_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One royalty per person per contract per period. A retry that paid
    -- twice would be found by an accountant months later, if at all.
    UNIQUE (contract_id, user_id, period_start),

    CONSTRAINT a_period_runs_forward CHECK (period_end > period_start)
);

CREATE INDEX idx_royalties_unpaid
    ON talent_data_royalties (contract_id)
    WHERE paid_at IS NULL;
CREATE INDEX idx_royalties_user ON talent_data_royalties (user_id, period_start DESC);

-- ═══════════════════════════════════════════════════════════════════
-- The Data Room
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE data_room_subscriptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    tier VARCHAR(20) NOT NULL CHECK (tier IN ('basic', 'pro', 'enterprise')),

    monthly_fee NUMERIC(10,2) NOT NULL CHECK (monthly_fee > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    api_access BOOLEAN NOT NULL DEFAULT FALSE,
    custom_reports_included SMALLINT NOT NULL DEFAULT 0
        CHECK (custom_reports_included >= 0),

    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Access is read from this, not from the status, so a lapsed
    -- subscription cannot be left open by a billing job that failed to run.
    expires_at TIMESTAMPTZ NOT NULL,
    cancelled_at TIMESTAMPTZ,
    auto_renew BOOLEAN NOT NULL DEFAULT TRUE,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_subscription_runs_forward CHECK (expires_at > started_at)
);

-- One live subscription per company. A second would be a second bill for the
-- same access.
CREATE UNIQUE INDEX idx_data_room_one_live
    ON data_room_subscriptions (enterprise_id)
    WHERE cancelled_at IS NULL;

COMMENT ON TABLE data_room_subscriptions IS
    'Market insight sold by the month. Every figure it serves is aggregated '
    'over a floor cohort — the Data Room is the product most likely to be '
    'asked for a number small enough to name somebody.';

-- ═══════════════════════════════════════════════════════════════════
-- White-label, including the government instances
-- ═══════════════════════════════════════════════════════════════════
--
-- A government instance is a white-label deployment whose partner is a
-- ministry and whose attestations carry official recognition. One table, and
-- the recognition is a column.

CREATE TABLE white_label_deployments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    partner_org VARCHAR(200) NOT NULL CHECK (btrim(partner_org) <> ''),
    partner_type VARCHAR(30) NOT NULL CHECK (partner_type IN (
        'university', 'bootcamp', 'coding_school', 'corporate_academy',
        -- A ministry. Same deployment, different weight.
        'government'
    )),
    country CHAR(2),

    deployment_host VARCHAR(200) NOT NULL UNIQUE
        CHECK (deployment_host ~ '^[a-z0-9.-]+\.[a-z]{2,}$'),
    branding JSONB NOT NULL DEFAULT '{}'::jsonb,
    features_enabled TEXT[] NOT NULL DEFAULT '{attestations,portfolio}'
        CHECK (cardinality(features_enabled) > 0),

    -- What a government instance officially recognises. Empty for everybody
    -- else, and the constraint below keeps it that way: an attestation
    -- claiming state recognition it does not have is the worst thing this
    -- platform could ship.
    official_recognition_scope TEXT[] NOT NULL DEFAULT '{}',

    setup_fee NUMERIC(12,2) NOT NULL DEFAULT 0 CHECK (setup_fee >= 0),
    monthly_fee NUMERIC(10,2) NOT NULL DEFAULT 0 CHECK (monthly_fee >= 0),
    annual_fee NUMERIC(12,2) CHECK (annual_fee IS NULL OR annual_fee >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    users_limit INTEGER CHECK (users_limit IS NULL OR users_limit > 0),

    contract_url VARCHAR(500),
    signed_at TIMESTAMPTZ,
    launched_on DATE,
    contract_ends_on DATE,

    status VARCHAR(20) NOT NULL DEFAULT 'provisioning' CHECK (status IN (
        'provisioning', 'live', 'suspended', 'ended'
    )),
    ended_reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT only_a_government_recognises_officially CHECK (
        partner_type = 'government' OR cardinality(official_recognition_scope) = 0
    ),
    -- Official recognition rests on a signed contract with a state. Without
    -- one it is a claim, and the people carrying the attestation are the ones
    -- who find out it was worthless.
    CONSTRAINT recognition_rests_on_a_signed_contract CHECK (
        cardinality(official_recognition_scope) = 0
        OR (signed_at IS NOT NULL AND contract_url IS NOT NULL)
    ),
    CONSTRAINT a_live_deployment_is_signed CHECK (
        status <> 'live' OR signed_at IS NOT NULL
    ),
    CONSTRAINT ending_carries_a_reason CHECK (
        status <> 'ended'
        OR (ended_reason IS NOT NULL AND btrim(ended_reason) <> '')
    )
);

COMMENT ON CONSTRAINT recognition_rests_on_a_signed_contract
    ON white_label_deployments IS
    'Official recognition rests on a signed contract with a state. Without '
    'one it is a claim, and the people carrying the attestation are the ones '
    'who find out it was worthless.';

CREATE INDEX idx_white_label_live
    ON white_label_deployments (partner_type)
    WHERE status = 'live';

CREATE TRIGGER trg_white_label_updated_at
    BEFORE UPDATE ON white_label_deployments
    FOR EACH ROW EXECUTE FUNCTION touch_data_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- The unified profile
-- ═══════════════════════════════════════════════════════════════════
--
-- Activity gathered from the places somebody already publishes: their merged
-- pull requests here, their verified external signals, their attestations.
-- Recomputed on a schedule, never on read, so a bank's query cannot cost a
-- hundred joins.
--
-- Nothing leaves without `identity_aggregation` consent, and the partners
-- allowed to see it are named one by one rather than by a single flag: "a
-- bank" and "any bank" are not the same permission.

CREATE TABLE unified_identity_scores (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,

    aggregate_score INTEGER NOT NULL DEFAULT 0 CHECK (aggregate_score >= 0),
    -- Which sources actually contributed. A score built from one platform
    -- and a score built from six are different claims, and a reader should
    -- be able to tell them apart.
    platforms_covered TEXT[] NOT NULL DEFAULT '{}',
    -- The parts, so a figure can be argued with rather than only believed.
    breakdown JSONB NOT NULL DEFAULT '{}'::jsonb,

    last_computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_score_names_its_sources CHECK (
        aggregate_score = 0 OR cardinality(platforms_covered) > 0
    )
);

COMMENT ON TABLE unified_identity_scores IS
    'Recomputed on a schedule, never on read. Nothing leaves without '
    'identity_aggregation consent, and the partners allowed to see it are '
    'named one by one: "a bank" and "any bank" are not the same permission.';

CREATE TABLE identity_licensing_partners (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    partner_slug VARCHAR(60) NOT NULL,
    allowed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ,

    PRIMARY KEY (user_id, partner_slug)
);

CREATE INDEX idx_identity_partners_live
    ON identity_licensing_partners (partner_slug)
    WHERE revoked_at IS NULL;

-- ═══════════════════════════════════════════════════════════════════
-- The revenue streams and products these feed
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO revenue_streams (slug, pillar, label, description, recurring) VALUES
    ('data_room_subscription', 'data', 'Abonnement Data Room',
     'Un accès mensuel aux tendances du marché du travail tech.',
     TRUE),
    ('intelligence_report', 'data', 'Rapport sur mesure',
     'Un rapport commandé par une institution : état des lieux, écart de '
     'compétences, pipeline.',
     FALSE)
ON CONFLICT (slug) DO NOTHING;

INSERT INTO enterprise_product_types
    (slug, label, description, revenue_stream, recurring)
VALUES
    ('data_room', 'Data Room',
     'Un abonnement aux tendances du marché : salaires, disponibilités, '
     'compétences qui montent.',
     'data_room_subscription', TRUE),
    ('intelligence_report', 'Rapport sur mesure',
     'Un rapport commandé sur un périmètre donné.',
     'intelligence_report', FALSE),
    ('api_access', 'Accès API',
     'Un accès programmatique aux scores et aux attestations publiques.',
     'talent_score_api', TRUE)
ON CONFLICT (slug) DO NOTHING;
