-- Paid missions.
--
-- ## Why this is `missions` and not `code_missions`
--
-- The backlog asks for a code marketplace and says to reuse whatever the
-- cyber and design ones have. Neither exists yet, so building `code_missions`
-- now would guarantee three tables differing only in the enum of their type
-- column — and the third one would inherit whichever mistakes the first two
-- had already been shipped with.
--
-- One table with a domain, and the types as rows. A design marketplace is
-- then twelve INSERTs, not a migration and a second half of the codebase.
--
-- ## Why the type is a table
--
-- Twelve types today, and the thirteenth arrives the first time somebody
-- asks for work this list has no word for. A CHECK constraint would make
-- that a deployment; a row makes it an afternoon.

CREATE TABLE mission_types (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(60) NOT NULL UNIQUE,
    skill_domain VARCHAR(30) NOT NULL,
    name VARCHAR(120) NOT NULL,
    description TEXT NOT NULL,
    sort_order SMALLINT NOT NULL DEFAULT 100,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE mission_types IS
    'What kinds of paid work exist, per domain. A row rather than an enum: '
    'the thirteenth kind arrives the first time somebody asks for work this '
    'list has no word for.';

CREATE INDEX idx_mission_types_domain
    ON mission_types (skill_domain, sort_order)
    WHERE is_active = TRUE;

CREATE TABLE missions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(120) NOT NULL UNIQUE,
    -- Enterprises pay, talents do not. That is the platform's rule, and this
    -- foreign key is where it stops being a slogan.
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    mission_type_id UUID NOT NULL REFERENCES mission_types(id) ON DELETE RESTRICT,
    skill_domain VARCHAR(30) NOT NULL,

    title VARCHAR(200) NOT NULL,
    description TEXT NOT NULL,
    -- What "done" means, agreed before anybody starts. A mission without it
    -- ends in an argument about scope, every time.
    acceptance_criteria TEXT NOT NULL CHECK (btrim(acceptance_criteria) <> ''),

    target_languages TEXT[] NOT NULL DEFAULT '{}',
    target_frameworks TEXT[] NOT NULL DEFAULT '{}',
    -- Which trade this is, when it is one. Lets a mission appear in the same
    -- filters as everything else somebody's trade governs.
    orientation_id UUID REFERENCES orientations(id) ON DELETE SET NULL,

    deliverable_format VARCHAR(30) NOT NULL CHECK (deliverable_format IN (
        'github_pr', 'repository_handover', 'library_published', 'consulting_report'
    )),

    nda_required BOOLEAN NOT NULL DEFAULT FALSE,
    -- Who owns what at the end. Stated up front rather than negotiated after
    -- the work exists, when the person who did it has no leverage left.
    ip_terms VARCHAR(40) NOT NULL DEFAULT 'full_ownership_client' CHECK (ip_terms IN (
        -- The client owns the delivered code. The usual arrangement.
        'full_ownership_client',
        -- The deliverable is released under an open licence.
        'open_source_output',
        -- The client owns the domain-specific work; the creator keeps the
        -- generic pieces they would otherwise have to rewrite next time.
        'retain_reusable_components',
        -- Delivered to the client and released openly at the same time.
        'dual_license'
    )),

    payment_model VARCHAR(30) NOT NULL DEFAULT 'fixed_price' CHECK (payment_model IN (
        'fixed_price', 'per_hour', 'per_deliverable', 'retainer_monthly', 'revenue_share'
    )),
    -- The headline figure, in euros. Meaning depends on the model: the whole
    -- job for fixed_price, the monthly figure for a retainer, the budget
    -- ceiling for per_hour and per_deliverable.
    budget_eur NUMERIC(10,2) CHECK (budget_eur IS NULL OR budget_eur > 0),
    hourly_rate_eur NUMERIC(10,2) CHECK (hourly_rate_eur IS NULL OR hourly_rate_eur > 0),
    revenue_share_percent NUMERIC(5,2)
        CHECK (revenue_share_percent IS NULL
               OR (revenue_share_percent > 0 AND revenue_share_percent <= 50)),

    -- What Skilluv keeps, frozen when the mission is published. Stored rather
    -- than computed at payout: changing the platform rate must not silently
    -- rewrite the terms of work already agreed.
    commission_percent NUMERIC(5,2) NOT NULL DEFAULT 15.00
        CHECK (commission_percent >= 0 AND commission_percent <= 30),

    remote_only BOOLEAN NOT NULL DEFAULT TRUE,
    urgency VARCHAR(20) NOT NULL DEFAULT 'normal'
        CHECK (urgency IN ('normal', 'soon', 'urgent')),
    estimated_days SMALLINT CHECK (estimated_days IS NULL OR estimated_days > 0),

    status VARCHAR(30) NOT NULL DEFAULT 'draft' CHECK (status IN (
        'draft',
        'published',
        -- Still open, but no longer taking applications.
        'applications_closed',
        'in_progress',
        'delivered',
        'closed',
        'cancelled'
    )),
    assigned_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    assigned_at TIMESTAMPTZ,
    delivered_at TIMESTAMPTZ,
    closed_at TIMESTAMPTZ,
    cancellation_reason TEXT,

    applications_close_at TIMESTAMPTZ,
    published_at TIMESTAMPTZ,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Each model needs its own number, and only its own. A retainer with an
    -- hourly rate and no monthly figure is a mission nobody can price.
    CONSTRAINT mission_price_matches_its_model CHECK (
        CASE payment_model
            WHEN 'per_hour' THEN hourly_rate_eur IS NOT NULL
            WHEN 'revenue_share' THEN revenue_share_percent IS NOT NULL
            ELSE budget_eur IS NOT NULL
        END
    ),

    -- Work in progress belongs to somebody.
    CONSTRAINT running_mission_has_somebody_on_it CHECK (
        status NOT IN ('in_progress', 'delivered') OR assigned_user_id IS NOT NULL
    ),

    -- Cancelling without saying why leaves the applicants with nothing to
    -- read. Note the `IS NOT NULL` first: btrim(NULL) is NULL, and a CHECK
    -- that evaluates to NULL passes.
    CONSTRAINT cancellation_carries_a_reason CHECK (
        status <> 'cancelled'
        OR (cancellation_reason IS NOT NULL AND btrim(cancellation_reason) <> '')
    )
);

COMMENT ON COLUMN missions.commission_percent IS
    'Frozen at publication. Changing the platform rate must not rewrite the '
    'terms of work somebody already agreed to.';

CREATE INDEX idx_missions_open
    ON missions (skill_domain, published_at DESC)
    WHERE status = 'published';
CREATE INDEX idx_missions_enterprise ON missions (enterprise_id, created_at DESC);
CREATE INDEX idx_missions_assigned
    ON missions (assigned_user_id, status)
    WHERE assigned_user_id IS NOT NULL;
CREATE INDEX idx_missions_languages ON missions USING GIN (target_languages);
CREATE INDEX idx_missions_frameworks ON missions USING GIN (target_frameworks);
CREATE INDEX idx_missions_orientation
    ON missions (orientation_id)
    WHERE orientation_id IS NOT NULL;

CREATE TABLE mission_applications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    mission_id UUID NOT NULL REFERENCES missions(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    cover_letter TEXT NOT NULL CHECK (btrim(cover_letter) <> ''),
    -- GitHub profiles, published packages, shipped applications. Free-form
    -- because the proof of code work does not live in one place.
    portfolio_urls TEXT[] NOT NULL DEFAULT '{}',
    -- [{"name": "rust", "years": 3}]. Self-declared, and read next to the
    -- attestations the platform issued, which are not.
    expertise JSONB NOT NULL DEFAULT '[]'::JSONB,
    past_similar_missions TEXT,
    availability_hours_per_week SMALLINT
        CHECK (availability_hours_per_week IS NULL
               OR availability_hours_per_week BETWEEN 1 AND 60),

    status VARCHAR(20) NOT NULL DEFAULT 'submitted' CHECK (status IN (
        'submitted', 'shortlisted', 'selected', 'rejected', 'withdrawn'
    )),
    decided_by UUID REFERENCES users(id) ON DELETE SET NULL,
    decided_at TIMESTAMPTZ,
    decision_reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (mission_id, user_id),

    CONSTRAINT expertise_is_an_array CHECK (jsonb_typeof(expertise) = 'array'),

    -- Somebody who put an hour into an application is owed a sentence.
    CONSTRAINT rejection_carries_a_reason CHECK (
        status <> 'rejected'
        OR (decision_reason IS NOT NULL AND btrim(decision_reason) <> '')
    )
);

COMMENT ON TABLE mission_applications IS
    'One application per person per mission. Rejections carry a reason: '
    'somebody who put an hour into an application is owed a sentence.';

CREATE INDEX idx_mission_applications_mission
    ON mission_applications (mission_id, status, created_at DESC);
CREATE INDEX idx_mission_applications_user
    ON mission_applications (user_id, created_at DESC);

-- Only one person can be selected for a mission. Partial unique rather than a
-- trigger: the rule is exactly what an index expresses.
CREATE UNIQUE INDEX uniq_mission_selected_applicant
    ON mission_applications (mission_id)
    WHERE status = 'selected';

CREATE OR REPLACE FUNCTION touch_missions_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_missions_updated_at
    BEFORE UPDATE ON missions
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

CREATE TRIGGER trg_mission_applications_updated_at
    BEFORE UPDATE ON mission_applications
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- Applications close when the mission does
-- ═══════════════════════════════════════════════════════════════════
--
-- A mission that is assigned, delivered or cancelled must not keep collecting
-- applications nobody will read.

CREATE OR REPLACE FUNCTION application_requires_an_open_mission()
RETURNS TRIGGER AS $$
DECLARE
    mission_status TEXT;
    closes_at TIMESTAMPTZ;
BEGIN
    SELECT status, applications_close_at INTO mission_status, closes_at
      FROM missions WHERE id = NEW.mission_id;

    IF mission_status <> 'published' THEN
        RAISE EXCEPTION 'this mission is %, not open to applications', mission_status;
    END IF;

    IF closes_at IS NOT NULL AND closes_at < NOW() THEN
        RAISE EXCEPTION 'applications for this mission closed on %', closes_at;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_application_requires_an_open_mission
    BEFORE INSERT ON mission_applications
    FOR EACH ROW EXECUTE FUNCTION application_requires_an_open_mission();

-- ═══════════════════════════════════════════════════════════════════
-- The twelve kinds of code work
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO mission_types (slug, skill_domain, name, description, sort_order)
VALUES
    ('web_app_dev', 'code', 'Application web',
     'Une application web, du premier écran à la mise en ligne.', 10),
    ('mobile_app_dev', 'code', 'Application mobile',
     'iOS, Android ou les deux, jusqu''à la publication sur les stores.', 20),
    ('desktop_app_dev', 'code', 'Application desktop',
     'Un logiciel installable, avec ce que cela implique de packaging par système.', 30),
    ('backend_service_dev', 'code', 'Service backend',
     'API, traitements, intégrations. Ce que personne ne voit et sur quoi tout repose.', 40),
    ('systems_lib_dev', 'code', 'Bibliothèque système',
     'Une brique bas niveau destinée à être utilisée par d''autres développeurs.', 50),
    ('embedded_firmware_dev', 'code', 'Firmware embarqué',
     'Du code qui tourne sur une carte, avec les contraintes de mémoire et de temps réel.', 60),
    ('smart_contract_dev', 'code', 'Smart contract',
     'Contrat déployé sur une chaîne. Audité, ou honnête sur le fait de ne pas l''être.', 70),
    ('devtool_creation', 'code', 'Outil de développement',
     'CLI, extension, plugin. Un outil que d''autres développeurs adoptent.', 80),
    ('bug_fix_urgent', 'code', 'Correction urgente',
     'Quelque chose est cassé en production et doit être réparé maintenant.', 90),
    ('performance_optimization', 'code', 'Optimisation',
     'Ça marche mais c''est trop lent, trop cher ou les deux.', 100),
    ('migration_rewrite', 'code', 'Migration ou réécriture',
     'Faire passer un système existant d''une technologie à une autre sans rien perdre.', 110),
    ('consulting_technical', 'code', 'Conseil technique',
     'De l''expertise, un avis, une revue d''architecture. Le livrable est un rapport.', 120);
