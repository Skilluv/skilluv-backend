-- What a subscription actually entitles somebody to, and the trial that
-- precedes a hire.
--
-- ## Two tickets, one entitlements table
--
-- The backlog describes an annual programme (02-03) and a light ATS (02-04)
-- as two tables. Both are a subscription with a tier, a price and a list of
-- what is included: credits, campaigns, a bounty pool, a discount, a ceiling
-- on open positions. The lists differ; the shape does not.
--
-- Two tables would mean two places to answer "has this client used up their
-- campaigns", and the answer would eventually differ between them. One
-- entitlements table, one consumption query.
--
-- ## Why an entitlement is a row rather than a column
--
-- `included_credits`, `included_campaigns`, `included_bounty_pool`,
-- `studios_discount`, `max_open_positions`, `max_pool_size` — six columns
-- today, mostly NULL, and a seventh the first time somebody sells a bundle
-- with something new in it. As rows: a new entitlement is an INSERT, and the
-- consumption query does not change.

CREATE TABLE entitlement_kinds (
    slug VARCHAR(60) PRIMARY KEY,
    label VARCHAR(120) NOT NULL,
    description TEXT NOT NULL,
    -- `quota` is used up and does not come back. `ceiling` is a limit that
    -- resets — ten open positions at a time, not ten ever. `discount` is a
    -- percentage applied elsewhere. `flag` is simply on or off.
    --
    -- The distinction matters because a quota at zero means "spent" and a
    -- ceiling at zero means "none allowed", and a dashboard that shows them
    -- the same way is a dashboard that lies about one of them.
    nature VARCHAR(20) NOT NULL CHECK (nature IN ('quota', 'ceiling', 'discount', 'flag')),
    unit VARCHAR(40),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO entitlement_kinds (slug, label, description, nature, unit) VALUES
    ('credits', 'Crédits inclus',
     'Des crédits versés à la souscription, dépensés sur la recherche et les mises en relation.',
     'quota', 'crédits'),
    ('recruitment_campaigns', 'Campagnes incluses',
     'Des campagnes de recrutement menées par Skilluv, comprises dans le contrat.',
     'quota', 'campagnes'),
    ('bounty_pool', 'Enveloppe de primes',
     'Un budget de primes provisionné, dépensé au fil des unités de travail posées.',
     'quota', 'montant'),
    ('open_positions', 'Postes ouverts simultanés',
     'Combien d''annonces peuvent être ouvertes en même temps. Une limite, pas un compteur.',
     'ceiling', 'postes'),
    ('talent_pool_size', 'Taille du vivier',
     'Combien de profils peuvent être suivis en même temps.',
     'ceiling', 'profils'),
    ('recruiter_seats', 'Comptes recruteurs',
     'Combien de personnes de l''entreprise peuvent accéder à la recherche.',
     'ceiling', 'comptes'),
    ('studios_discount', 'Remise Studios',
     'Une réduction sur les prestations menées par une équipe Skilluv.',
     'discount', 'pourcentage'),
    ('priority_talent_access', 'Accès prioritaire',
     'Les nouveaux profils sont visibles avant leur mise en ligne générale.',
     'flag', NULL),
    ('dedicated_account_manager', 'Interlocuteur dédié',
     'Une personne nommée chez Skilluv, plutôt qu''une adresse générique.',
     'flag', NULL),
    ('events_access', 'Accès aux événements',
     'La participation aux événements de la communauté est comprise.',
     'flag', NULL);

CREATE TABLE enterprise_entitlements (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- What bought it. Everything an entitlement grants comes from an
    -- engagement, so there is always something to point at when somebody
    -- asks why they have it.
    product_id UUID NOT NULL REFERENCES enterprise_products(id) ON DELETE CASCADE,
    kind VARCHAR(60) NOT NULL REFERENCES entitlement_kinds(slug) ON DELETE RESTRICT,

    -- How much. NULL for a flag, which is on by existing.
    granted NUMERIC(14,2) CHECK (granted IS NULL OR granted >= 0),
    -- How much has been used. Only meaningful for a quota; a ceiling is
    -- measured against reality, not against a counter that could drift.
    consumed NUMERIC(14,2) NOT NULL DEFAULT 0 CHECK (consumed >= 0),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (product_id, kind),

    -- Spending more than was granted is the failure this table exists to
    -- make impossible.
    CONSTRAINT nothing_is_overspent CHECK (granted IS NULL OR consumed <= granted)
);

COMMENT ON TABLE enterprise_entitlements IS
    'What a subscription includes, as rows. Six columns mostly NULL would '
    'become seven the first time somebody sells a bundle with something new '
    'in it; a row is an INSERT and the consumption query does not change.';

COMMENT ON COLUMN enterprise_entitlements.consumed IS
    'Only meaningful for a quota. A ceiling is measured against reality — '
    'counting open positions — rather than against a counter that drifts.';

CREATE INDEX idx_entitlements_product ON enterprise_entitlements (product_id);

CREATE TRIGGER trg_entitlements_updated_at
    BEFORE UPDATE ON enterprise_entitlements
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- A flag carries no number; a quota and a ceiling must. Enforced here rather
-- than by convention, because a quota with no figure is an unlimited quota by
-- accident.
CREATE OR REPLACE FUNCTION entitlement_matches_its_nature()
RETURNS TRIGGER AS $$
DECLARE
    kind_nature TEXT;
BEGIN
    SELECT nature INTO kind_nature FROM entitlement_kinds WHERE slug = NEW.kind;

    IF kind_nature = 'flag' AND NEW.granted IS NOT NULL THEN
        RAISE EXCEPTION 'a flag entitlement carries no amount — it is on by existing';
    END IF;
    IF kind_nature <> 'flag' AND NEW.granted IS NULL THEN
        RAISE EXCEPTION 'a % entitlement must say how much', kind_nature
            USING HINT = 'a quota with no figure is an unlimited quota by accident';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_entitlement_matches_its_nature
    BEFORE INSERT OR UPDATE ON enterprise_entitlements
    FOR EACH ROW EXECUTE FUNCTION entitlement_matches_its_nature();

-- ═══════════════════════════════════════════════════════════════════
-- Trial periods
-- ═══════════════════════════════════════════════════════════════════
--
-- Genuinely its own thing, unlike the two above: a trial is paid work, not
-- sourcing. The talent is paid by the hour for real hours, and the whole
-- point is that both sides can walk away without either having wasted a
-- recruitment.
--
-- ## Why the hours are rows and not JSON
--
-- The backlog proposes `hours_worked_json`. Hours are what somebody is paid
-- for; they get disputed, corrected and approved one entry at a time, and
-- every one of those is an UPDATE inside a blob with no constraint on it.
-- A row per entry can be approved individually and cannot silently change.

CREATE TABLE recruitment_trials (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    talent_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    -- The campaign it came out of, when there was one.
    campaign_id UUID REFERENCES recruitment_campaigns(id) ON DELETE SET NULL,

    duration_weeks SMALLINT NOT NULL CHECK (duration_weeks BETWEEN 1 AND 8),
    hourly_rate NUMERIC(10,2) NOT NULL CHECK (hourly_rate > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    -- What Skilluv keeps on the hours. Frozen here like every other
    -- commission on the platform.
    platform_fee_percent NUMERIC(5,2) NOT NULL DEFAULT 15.00
        CHECK (platform_fee_percent >= 0 AND platform_fee_percent <= 30),
    -- The reduced success fee if this converts. Lower than a direct hire
    -- because the trial already de-risked it for both sides, and because the
    -- client has already paid for the weeks.
    converted_success_fee_percent NUMERIC(5,2)
        CHECK (converted_success_fee_percent IS NULL
               OR (converted_success_fee_percent > 0
                   AND converted_success_fee_percent <= 30)),

    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ends_at TIMESTAMPTZ NOT NULL,
    ended_at TIMESTAMPTZ,
    outcome VARCHAR(30) CHECK (outcome IN (
        'ongoing',
        'converted_hire',
        -- Named separately on purpose. "It did not work out" hides which side
        -- walked away, and that is the single most useful thing to know when
        -- the same client tries again.
        'declined_by_enterprise',
        'declined_by_talent',
        'lapsed'
    )),
    outcome_note TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_trial_runs_forward CHECK (ends_at > started_at),
    CONSTRAINT an_ended_trial_has_an_outcome CHECK (
        ended_at IS NULL OR (outcome IS NOT NULL AND outcome <> 'ongoing')
    )
);

-- Somebody cannot be on two running trials with the same company: it would
-- double the hours and halve the point.
--
-- A partial unique index rather than an exclusion constraint over the date
-- range, which would need `btree_gist` — an extension this deployment does
-- not install, and not worth installing to catch a case that only arises
-- while both trials are open. Once a trial has ended, a second one with the
-- same company is a legitimate second attempt.
CREATE UNIQUE INDEX uniq_running_trial_per_pair
    ON recruitment_trials (enterprise_id, talent_user_id)
    WHERE ended_at IS NULL;

COMMENT ON TABLE recruitment_trials IS
    'Paid weeks before a hire. The outcome names which side walked away, '
    'because "it did not work out" hides the single most useful thing to '
    'know when the same client tries again.';

CREATE INDEX idx_trials_enterprise ON recruitment_trials (enterprise_id, started_at DESC);
CREATE INDEX idx_trials_talent ON recruitment_trials (talent_user_id, started_at DESC);
CREATE INDEX idx_trials_running
    ON recruitment_trials (ends_at)
    WHERE ended_at IS NULL;

CREATE TRIGGER trg_trials_updated_at
    BEFORE UPDATE ON recruitment_trials
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

CREATE TABLE recruitment_trial_hours (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    trial_id UUID NOT NULL REFERENCES recruitment_trials(id) ON DELETE CASCADE,
    worked_on DATE NOT NULL,
    hours NUMERIC(5,2) NOT NULL CHECK (hours > 0 AND hours <= 16),
    -- What was done. Not decoration: it is what the client approves against,
    -- and what the talent points at when an entry is questioned.
    summary TEXT NOT NULL CHECK (btrim(summary) <> ''),

    approved_at TIMESTAMPTZ,
    approved_by UUID REFERENCES users(id) ON DELETE SET NULL,
    rejected_at TIMESTAMPTZ,
    rejection_reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One entry per day per trial. Two entries for one day is how a
    -- timesheet becomes an argument.
    UNIQUE (trial_id, worked_on),

    CONSTRAINT an_entry_is_not_both CHECK (approved_at IS NULL OR rejected_at IS NULL),
    CONSTRAINT a_rejection_says_why CHECK (
        rejected_at IS NULL
        OR (rejection_reason IS NOT NULL AND btrim(rejection_reason) <> '')
    )
);

COMMENT ON TABLE recruitment_trial_hours IS
    'One row per day worked. Rows rather than a JSON blob: hours get '
    'disputed, corrected and approved one entry at a time, and every one of '
    'those inside a blob is an UPDATE with no constraint on it.';

CREATE INDEX idx_trial_hours_pending
    ON recruitment_trial_hours (trial_id, worked_on)
    WHERE approved_at IS NULL AND rejected_at IS NULL;

-- Hours belong inside the trial they are claimed against.
CREATE OR REPLACE FUNCTION trial_hours_fall_inside_the_trial()
RETURNS TRIGGER AS $$
DECLARE
    t RECORD;
BEGIN
    SELECT started_at, ends_at, ended_at INTO t
      FROM recruitment_trials WHERE id = NEW.trial_id;

    IF NEW.worked_on < t.started_at::DATE THEN
        RAISE EXCEPTION 'that day is before the trial started';
    END IF;
    IF NEW.worked_on > COALESCE(t.ended_at, t.ends_at)::DATE THEN
        RAISE EXCEPTION 'that day is after the trial ended';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_trial_hours_fall_inside_the_trial
    BEFORE INSERT OR UPDATE ON recruitment_trial_hours
    FOR EACH ROW EXECUTE FUNCTION trial_hours_fall_inside_the_trial();
