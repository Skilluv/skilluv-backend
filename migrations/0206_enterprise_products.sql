-- What each enterprise actually bought.
--
-- ## What this answers that nothing else does
--
-- Every product keeps its own table: credits in `enterprise_credits`,
-- subscriptions in `enterprise_subscriptions`, bounties on the slices,
-- missions in `missions`. Each is right for its own flow and none of them can
-- answer "what does this company have with us" — which is the question
-- somebody asks before every renewal conversation, and the one an upsell is
-- decided from.
--
-- One row per engagement, pointing back at whatever holds the detail.
--
-- ## Why the product types are a table
--
-- Eighteen today, and each one maps to a revenue stream. Rows let that
-- mapping be a foreign key rather than a dictionary maintained in two
-- languages, and let the nineteenth product arrive without a migration —
-- the same reasoning as `revenue_streams` itself.

CREATE TABLE enterprise_product_types (
    slug VARCHAR(60) PRIMARY KEY,
    label VARCHAR(120) NOT NULL,
    description TEXT NOT NULL,
    -- How Skilluv earns from it. The join that turns "what do they have" into
    -- "what does it earn", without a mapping written twice.
    revenue_stream VARCHAR(60) REFERENCES revenue_streams(slug),
    -- Whether it renews on its own. Decides which engagements appear on a
    -- renewal list and which are simply over.
    recurring BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO enterprise_product_types (slug, label, description, revenue_stream, recurring) VALUES
    ('credits_pack', 'Pack de crédits',
     'Des crédits achetés d''avance, dépensés sur la recherche et les mises en relation.',
     'other', FALSE),
    ('subscription_pipeline', 'Abonnement pipeline',
     'Un accès mensuel au suivi de candidatures.',
     'other', TRUE),
    ('enterprise_program_annual', 'Programme entreprise annuel',
     'Un contrat annuel regroupant plusieurs produits à un tarif consolidé.',
     'enterprise_program_annual', TRUE),
    ('raas_campaign', 'Campagne de recrutement',
     'Une campagne de recrutement menée de bout en bout par Skilluv.',
     'raas_campaign_fee', FALSE),
    ('recruiting_contest', 'Concours de recrutement',
     'Un concours dont les finalistes deviennent un vivier de candidats.',
     'recruiting_contest_fee', FALSE),
    ('bounty', 'Prime',
     'Une prime posée sur une unité de travail précise.',
     'bounty', FALSE),
    ('studio_engagement', 'Mission Studios',
     'Une prestation menée par une équipe constituée par Skilluv.',
     'studio_margin', FALSE),
    ('outsourcing_project', 'Sous-traitance',
     'Du travail confié à Skilluv puis réparti entre contributeurs.',
     'outsourcing_margin', FALSE),
    ('sponsoring_event', 'Sponsoring d''événement',
     'Le sponsoring d''un hackathon, d''un marathon ou d''une remise de prix.',
     'event_sponsorship', FALSE),
    ('data_licensing', 'Licence de données',
     'Une licence sur des données agrégées et anonymisées.',
     'data_licensing', TRUE),
    ('talent_score_api', 'API de scores',
     'Un accès programmatique aux craft scores.',
     'talent_score_api', TRUE),
    ('white_label_platform', 'Plateforme en marque blanche',
     'La plateforme opérée sous la marque du client.',
     'white_label_platform', TRUE),
    ('certification_program', 'Programme de certification',
     'Un parcours de certification pour les équipes du client.',
     'certification_program', FALSE),
    ('academy_cohort', 'Cohorte Academy',
     'Une promotion formée pour le compte du client.',
     'academy_cohort_fee', FALSE),
    ('consulting_engagement', 'Mission de conseil',
     'Une mission de conseil menée directement par Skilluv.',
     'consulting_fee', FALSE),
    ('onboarding_service', 'Onboarding accompagné',
     'La mise en place accompagnée d''un nouveau produit chez le client.',
     'consulting_fee', FALSE),
    ('living_lab', 'Living lab',
     'Un terrain d''expérimentation partagé entre le client et la communauté.',
     'consulting_fee', TRUE),
    ('corporate_ambassador', 'Ambassadeur entreprise',
     'Un référent interne au client, formé et suivi par Skilluv.',
     'ambassador_commission', TRUE);

CREATE TABLE enterprise_products (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    product_type VARCHAR(60) NOT NULL
        REFERENCES enterprise_product_types(slug) ON DELETE RESTRICT,

    status VARCHAR(20) NOT NULL DEFAULT 'active' CHECK (status IN (
        -- Agreed, not started. A signature is not a delivery.
        'pending',
        'active',
        -- Ran its course.
        'completed',
        -- Stopped early. Carries a reason.
        'cancelled',
        -- A recurring product the client chose not to renew. Distinct from
        -- cancelled: one is a decision at the end, the other is a decision in
        -- the middle, and a renewal report that conflates them is useless.
        'lapsed'
    )),

    -- Where the detail lives. Free-form on purpose: each product already has
    -- a table and forcing eighteen nullable foreign keys onto this row would
    -- make it unreadable.
    source_table VARCHAR(60),
    source_id UUID,

    -- What it is worth, when there is a figure. Not a substitute for the
    -- ledger — that is what `platform_revenues` is — but a renewal
    -- conversation needs the contract value, which is not the same as what
    -- has been collected.
    contract_value NUMERIC(14,2) CHECK (contract_value IS NULL OR contract_value >= 0),
    currency CHAR(3) CHECK (currency IS NULL OR currency IN ('EUR', 'XOF', 'USD')),

    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- When a recurring product next needs a decision. NULL for one-off work.
    renews_at TIMESTAMPTZ,
    ended_at TIMESTAMPTZ,
    ended_reason TEXT,

    notes TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT metadata_is_an_object CHECK (jsonb_typeof(metadata) = 'object'),
    CONSTRAINT money_carries_its_currency CHECK ((contract_value IS NULL) = (currency IS NULL)),

    -- Stopping early without saying why leaves the next person guessing at
    -- exactly the moment they most need to know.
    CONSTRAINT cancellation_carries_a_reason CHECK (
        status <> 'cancelled'
        OR (ended_reason IS NOT NULL AND btrim(ended_reason) <> '')
    ),
    CONSTRAINT ended_products_have_an_end CHECK (
        status NOT IN ('completed', 'cancelled', 'lapsed') OR ended_at IS NOT NULL
    ),
    CONSTRAINT a_source_is_a_pair CHECK ((source_table IS NULL) = (source_id IS NULL))
);

COMMENT ON TABLE enterprise_products IS
    'One row per engagement. Every product has its own table and none of them '
    'can answer "what does this company have with us" — the question asked '
    'before every renewal.';

COMMENT ON COLUMN enterprise_products.contract_value IS
    'What was agreed, which is not what has been collected. The ledger holds '
    'the second; a renewal conversation needs the first.';

CREATE INDEX idx_enterprise_products_enterprise
    ON enterprise_products (enterprise_id, status, started_at DESC);

-- The renewal list: recurring engagements coming up, soonest first.
CREATE INDEX idx_enterprise_products_renewals
    ON enterprise_products (renews_at)
    WHERE status = 'active' AND renews_at IS NOT NULL;

CREATE INDEX idx_enterprise_products_type
    ON enterprise_products (product_type, status);

CREATE TRIGGER trg_enterprise_products_updated_at
    BEFORE UPDATE ON enterprise_products
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- A recurring product has a renewal date
-- ═══════════════════════════════════════════════════════════════════
--
-- Otherwise it silently never appears on a renewal list, which is the failure
-- mode where a subscription lapses because nobody was told to ask.

CREATE OR REPLACE FUNCTION recurring_product_has_a_renewal_date()
RETURNS TRIGGER AS $$
DECLARE
    is_recurring BOOLEAN;
BEGIN
    SELECT recurring INTO is_recurring
      FROM enterprise_product_types WHERE slug = NEW.product_type;

    IF is_recurring AND NEW.status = 'active' AND NEW.renews_at IS NULL THEN
        -- The product type fills the placeholder. Without the argument
        -- PostgreSQL refuses to compile the function body at CREATE time
        -- ("too few parameters for RAISE"), which made this migration — and
        -- therefore every one after it — impossible to apply.
        RAISE EXCEPTION 'a % renews — say when', NEW.product_type
            USING HINT = 'without a renewal date it never appears on a renewal list, '
                         'and it lapses because nobody was told to ask';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_recurring_product_has_a_renewal_date
    BEFORE INSERT OR UPDATE ON enterprise_products
    FOR EACH ROW EXECUTE FUNCTION recurring_product_has_a_renewal_date();
