-- What an ops mission needs that the others do not.
--
-- Not a second missions table: `missions` already carries `nda_required`,
-- `ip_terms`, `retainer_monthly` and a frozen commission, and a parallel
-- table would have meant a second application flow, a second invoice path and
-- a second place to hold an escrow.
--
-- Four things are genuinely missing, and each one is a way an ops engagement
-- goes wrong that no other domain has.

-- ═══════════════════════════════════════════════════════════════════
-- 1. What gets handed over
-- ═══════════════════════════════════════════════════════════════════
--
-- A pull request is not the deliverable here. Four rows rather than a
-- restated CHECK: 0413 made these a table precisely so that the fifth domain
-- would be an INSERT, and restating the constraint here would have dropped
-- everything AI, audio and design had added to it.

INSERT INTO mission_deliverable_formats
    (slug, skill_domain, name, description, sort_order)
VALUES
    ('iac_repository', 'ops', 'Infrastructure as code',
     'Un dépôt Terraform, Pulumi ou Ansible livré avec ses modules, ses '
     'variables documentées et son plan appliqué.', 310),
    ('runbooks', 'ops', 'Runbooks',
     'Les procédures qu''une astreinte suit à trois heures du matin. Ici '
     'c''est le livrable lui-même, pas la documentation qui l''accompagne.', 320),
    ('dashboards', 'ops', 'Tableaux de bord',
     'Les tableaux et les alertes qui vont avec, avec ce que chaque seuil '
     'veut dire et qui il réveille.', 330),
    ('migration_executed', 'ops', 'Migration menée',
     'Une migration exécutée de bout en bout, avec son plan de retour '
     'arrière et ce qui a été vérifié après.', 340)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- 2. Where it runs
-- ═══════════════════════════════════════════════════════════════════
--
-- An applicant needs to know before applying, and a search needs to filter on
-- it. An array rather than one value: multi-cloud engagements exist and
-- writing 'multi' would lose which two.

ALTER TABLE missions
    ADD COLUMN target_platforms TEXT[] NOT NULL DEFAULT '{}';

COMMENT ON COLUMN missions.target_platforms IS
    'aws, gcp, azure, on-prem. Empty means the mission does not depend on '
    'one — a Terraform module review, a runbook, a post-mortem.';

CREATE INDEX idx_missions_target_platforms
    ON missions USING gin (target_platforms);

-- ═══════════════════════════════════════════════════════════════════
-- 3. On-call, said out loud or not included
-- ═══════════════════════════════════════════════════════════════════
--
-- Being reachable is work whether or not anything happens, and unpaid
-- availability is the single most common way this trade is exploited. The
-- constraint below is the platform's position from docs/ops/LEGAL.md, made
-- unarguable: a mission that includes on-call states the window, the
-- response time and a monthly retainer, or it does not include on-call.
--
-- Response time is acknowledgement, not resolution. A clause promising a
-- system back in thirty minutes is a clause nobody can honour, and writing
-- it into the schema would be writing a lie into every such mission.

ALTER TABLE missions
    ADD COLUMN includes_oncall BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN oncall_window VARCHAR(120),
    ADD COLUMN oncall_response_minutes SMALLINT
        CHECK (oncall_response_minutes IS NULL
               OR oncall_response_minutes BETWEEN 5 AND 1440),
    ADD COLUMN oncall_has_backup BOOLEAN NOT NULL DEFAULT FALSE,

    ADD CONSTRAINT oncall_missions_state_their_terms CHECK (
        NOT includes_oncall
        OR (oncall_window IS NOT NULL
            AND btrim(oncall_window) <> ''
            AND oncall_response_minutes IS NOT NULL
            AND payment_model = 'retainer_monthly'
            AND budget_eur IS NOT NULL)
    );

COMMENT ON COLUMN missions.includes_oncall IS
    'Whether the person is expected to be reachable. True requires a window, '
    'a response time and a monthly retainer: unpaid availability is the most '
    'common way this trade is exploited, and the schema refuses it.';

COMMENT ON COLUMN missions.oncall_response_minutes IS
    'Time to acknowledge, not to resolve. A clause promising resolution in '
    'thirty minutes is one nobody can honour.';

COMMENT ON COLUMN missions.oncall_has_backup IS
    'Whether somebody else is called when this person does not answer. A '
    'rotation of one is not a rotation; false is allowed and visible, so an '
    'applicant can see what they are agreeing to.';

-- ═══════════════════════════════════════════════════════════════════
-- 4. Production access, and what it drags in
-- ═══════════════════════════════════════════════════════════════════
--
-- Ops is the one domain where the work runs on somebody's estate rather than
-- sitting in a branch. A mission that needs credentials must say so before
-- anybody applies, and must name the frameworks that will govern the work —
-- an applicant refused after a background check they were never told about
-- has wasted their time on the platform's account.

ALTER TABLE missions
    ADD COLUMN production_access_required BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN compliance_frameworks TEXT[] NOT NULL DEFAULT '{}',

    -- Migration 0243's charter: no ops mission with production access opens
    -- before the reinforced NDA exists. This is the half of that sentence a
    -- schema can hold.
    ADD CONSTRAINT production_access_requires_an_nda CHECK (
        NOT production_access_required OR nda_required
    );

COMMENT ON COLUMN missions.production_access_required IS
    'Whether the work needs credentials on the client''s live estate. Said '
    'before anybody applies, and it forces an NDA.';

COMMENT ON COLUMN missions.compliance_frameworks IS
    'soc2, iso27001, hipaa, pci_dss. Named in the brief so an applicant '
    'learns about the background check before applying rather than after '
    'being refused. Skilluv does not certify compliance and does not audit '
    'it; it makes the requirement visible.';

CREATE INDEX idx_missions_production_access
    ON missions (skill_domain, status)
    WHERE production_access_required;

-- ═══════════════════════════════════════════════════════════════════
-- What an ops applicant has to answer
-- ═══════════════════════════════════════════════════════════════════
--
-- `expertise` and `past_similar_missions` already exist and already carry
-- most of it. On-call does not fit either: it is not expertise and it is not
-- a past mission, it is whether this person can be woken up — and answering
-- it late, after selection, is how somebody ends up agreeing to a rotation
-- they cannot hold.

ALTER TABLE mission_applications
    ADD COLUMN oncall_available BOOLEAN,
    ADD COLUMN oncall_experience VARCHAR(20)
        CHECK (oncall_experience IS NULL OR oncall_experience IN (
            'never', 'occasional', 'regular', 'always_on'
        ));

COMMENT ON COLUMN mission_applications.oncall_available IS
    'Whether this person can hold the rotation this mission describes. NULL '
    'on missions that include none. Answered before selection, because '
    'answering after is how somebody agrees to a rotation they cannot hold.';
