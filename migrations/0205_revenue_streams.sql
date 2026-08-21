-- Where the money comes from, as a catalogue.
--
-- ## What the backlog asked for and why this differs
--
-- The ticket proposes widening a CHECK on `platform_revenues.revenue_type` to
-- twenty-two values. Two things about the real schema make that the wrong
-- shape:
--
--   * the column is called `source`, not `revenue_type`, and it is
--     `VARCHAR(20)` — `marketplace_creators_commission` is thirty-three
--     characters and would have been silently truncated or refused;
--   * twenty-two values that will keep growing is the case this codebase
--     already answers with a table. Orientations, badge rules, mission types,
--     licences and craft-score weights are all rows for the same reason: the
--     twenty-third arrives without a deployment.
--
-- ## What a row carries that a CHECK cannot
--
-- Which of the seven business pillars it belongs to, whether it is recurring,
-- and a sentence saying what it actually is. A finance dashboard grouping
-- revenue by pillar is then a join rather than a twenty-two-branch mapping
-- maintained in two languages.

CREATE TABLE revenue_streams (
    slug VARCHAR(60) PRIMARY KEY,
    -- The seven pillars of the business model.
    pillar VARCHAR(20) NOT NULL CHECK (pillar IN (
        'talent',        -- recruitment, search, credits
        'work',          -- bounties, missions, studios, outsourcing
        'brand',         -- sponsorship, events, campaigns
        'data',          -- scores, licensing, reports, white-label
        'finance',       -- advances, loans, insurance, fund
        'ecosystem',     -- creator marketplace, certifications, academy
        'consultation',  -- advisory, audits, onboarding-as-a-service
        'platform'       -- what the platform charges for being the platform
    )),
    label VARCHAR(120) NOT NULL,
    description TEXT NOT NULL,
    -- Whether it repeats on its own. Decides which figure a dashboard shows
    -- as run-rate and which as one-off, and getting that wrong is how a
    -- business overstates its recurring revenue to itself.
    recurring BOOLEAN NOT NULL DEFAULT FALSE,
    -- False until something actually books revenue under it. Every row below
    -- starts false except the four that already exist: a catalogue of
    -- twenty-two live streams when four are live is a lie told to oneself
    -- first.
    is_live BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE revenue_streams IS
    'The catalogue of ways Skilluv earns. Rows rather than a CHECK: the '
    'twenty-third arrives without a deployment, and a row can say which '
    'pillar it belongs to and whether it recurs.';

COMMENT ON COLUMN revenue_streams.is_live IS
    'False until something books revenue under it. A catalogue of twenty-two '
    'live streams when four are live is a lie told to oneself first.';

INSERT INTO revenue_streams (slug, pillar, label, description, recurring, is_live) VALUES
    -- The four that exist today.
    ('bounty', 'work', 'Commission sur prime',
     'La part Skilluv sur une prime versée par une entreprise à un contributeur.',
     FALSE, TRUE),
    ('mentor_session', 'ecosystem', 'Commission sur session de mentorat',
     'La part Skilluv sur une session payante entre un mentor et un mentoré.',
     FALSE, TRUE),
    ('api_metered', 'data', 'API à l''usage',
     'Facturation à l''appel ou au palier sur l''API publique.',
     TRUE, TRUE),
    ('sponsored_challenge', 'brand', 'Challenge sponsorisé',
     'Prix forfaitaire pour qu''une entreprise sponsorise un challenge.',
     FALSE, TRUE),

    -- Work.
    ('mission_marketplace', 'work', 'Commission marketplace missions',
     'La part Skilluv sur une mission payée, gelée à la sélection du prestataire.',
     FALSE, FALSE),
    ('studio_margin', 'work', 'Marge Skilluv Studios',
     'La marge sur une prestation menée par une équipe constituée par Skilluv.',
     FALSE, FALSE),
    ('outsourcing_margin', 'work', 'Marge sous-traitance',
     'La marge sur du travail confié à Skilluv puis réparti entre contributeurs.',
     FALSE, FALSE),

    -- Talent.
    ('recruitment_success_fee', 'talent', 'Honoraires au recrutement',
     'Un pourcentage du salaire annuel, facturé une fois le recrutement confirmé.',
     FALSE, FALSE),
    ('recruiting_contest_fee', 'talent', 'Concours de recrutement',
     'Frais de mise en place et de campagne pour un concours de recrutement.',
     FALSE, FALSE),
    ('raas_campaign_fee', 'talent', 'Recrutement comme service',
     'Campagne de recrutement menée de bout en bout par Skilluv.',
     FALSE, FALSE),
    ('enterprise_program_annual', 'talent', 'Programme entreprise annuel',
     'Un contrat annuel regroupant recherche, sponsoring et accès aux données.',
     TRUE, FALSE),

    -- Brand and events.
    ('event_sponsorship', 'brand', 'Sponsoring d''événement',
     'Le sponsoring d''un hackathon, d''un marathon ou d''une remise de prix.',
     FALSE, FALSE),
    ('media_sponsor_content', 'brand', 'Contenu sponsorisé',
     'Un contenu éditorial financé par une entreprise, signalé comme tel.',
     FALSE, FALSE),
    ('newsletter_subscription', 'brand', 'Abonnement newsletter',
     'Un abonnement payant à la lettre d''information.',
     TRUE, FALSE),

    -- Data.
    ('talent_score_api', 'data', 'API de scores',
     'Accès aux craft scores par abonnement ou à l''appel.',
     TRUE, FALSE),
    ('data_licensing', 'data', 'Licence de données',
     'Licence sur des données agrégées et anonymisées.',
     TRUE, FALSE),
    ('white_label_platform', 'data', 'Plateforme en marque blanche',
     'La plateforme opérée sous la marque d''un tiers.',
     TRUE, FALSE),

    -- Finance.
    ('factoring_take', 'finance', 'Avance sur revenus',
     'La commission d''une avance versée à un contributeur avant l''échéance.',
     FALSE, FALSE),
    ('insurance_commission', 'finance', 'Commission d''apporteur (assurance)',
     'La commission d''intermédiation sur une assurance souscrite via Skilluv.',
     FALSE, FALSE),
    ('growth_financing_isa', 'finance', 'Financement de formation',
     'Un remboursement indexé sur les revenus futurs, plafonné.',
     TRUE, FALSE),
    ('fund_carry', 'finance', 'Intéressement du fonds',
     'La part de plus-value revenant à Skilluv sur le fonds. Horizon long.',
     FALSE, FALSE),

    -- Ecosystem.
    ('marketplace_creators_commission', 'ecosystem', 'Commission créateurs',
     'La part Skilluv sur une vente réalisée par un créateur de contenu.',
     FALSE, FALSE),
    ('certification_program', 'ecosystem', 'Programme de certification',
     'Les frais d''un parcours de certification.',
     FALSE, FALSE),
    ('academy_cohort_fee', 'ecosystem', 'Cohorte Academy entreprise',
     'Une promotion formée pour le compte d''une entreprise.',
     FALSE, FALSE),
    ('training_corporate', 'ecosystem', 'Abonnement formation entreprise',
     'Un accès continu aux parcours pour les équipes d''une entreprise.',
     TRUE, FALSE),
    ('ambassador_commission', 'ecosystem', 'Commission ambassadeur',
     'La commission versée puis reprise sur une affaire apportée.',
     FALSE, FALSE),

    -- Consultation.
    ('consulting_fee', 'consultation', 'Conseil',
     'Une mission de conseil menée directement par Skilluv.',
     FALSE, FALSE),

    -- Platform.
    ('other', 'platform', 'Autre',
     'Ce qui n''entre pas encore dans une catégorie. À typer dès que possible : '
     'une ligne qui reste ici six mois est une catégorie manquante.',
     FALSE, TRUE);

-- ═══════════════════════════════════════════════════════════════════
-- Pointing the ledger at the catalogue
-- ═══════════════════════════════════════════════════════════════════
--
-- The column is widened before the foreign key, because two of the new slugs
-- do not fit in twenty characters and the constraint would fail on its own
-- seed data.

ALTER TABLE platform_revenues
    DROP CONSTRAINT platform_revenues_source_check;

ALTER TABLE platform_revenues
    ALTER COLUMN source TYPE VARCHAR(60);

ALTER TABLE platform_revenues
    ADD CONSTRAINT platform_revenues_source_fkey
        FOREIGN KEY (source) REFERENCES revenue_streams(slug);

COMMENT ON COLUMN platform_revenues.source IS
    'Which revenue stream this line belongs to. A foreign key rather than a '
    'CHECK, so adding the twenty-third is an INSERT.';

-- A stream that has booked revenue is live, whatever the catalogue said.
-- Maintained by a trigger rather than by whoever wired the flow, because that
-- person has no reason to remember a catalogue flag.
CREATE OR REPLACE FUNCTION revenue_stream_goes_live()
RETURNS TRIGGER AS $$
BEGIN
    UPDATE revenue_streams
       SET is_live = TRUE
     WHERE slug = NEW.source AND is_live = FALSE;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_revenue_stream_goes_live
    AFTER INSERT ON platform_revenues
    FOR EACH ROW EXECUTE FUNCTION revenue_stream_goes_live();
