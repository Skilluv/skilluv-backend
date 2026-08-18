-- The ops domain, brought up to the shape the other domains already have.
-- Migration 0243.
--
-- ## What was already there
--
-- Migration 0088 gave `ops` three orientations: devops-engineer, sre,
-- cloud-architect. That was the right start and it has become too coarse.
-- Somebody who runs a Kubernetes platform, somebody who owns the
-- observability stack and somebody who tunes a Postgres cluster are all
-- "devops-engineer" today, and a client searching for one of them gets all
-- three.
--
-- Five orientations are added, taking ops to eight, and the same machinery
-- every other domain uses follows: reviewer groups, slice subtypes,
-- attestation bases.
--
-- ## Why `incident-commander` is an ops trade and not a leadership one
--
-- It overlaps with leadership on purpose. Running an incident is a
-- coordination job, and somebody could reasonably file it under management.
-- It sits here because the thing that makes it hard is technical: knowing
-- which question to ask at three in the morning, and which dashboard answers
-- it. The leadership version of the same skill is running a reorganisation,
-- and the two people are rarely the same one.

-- ═══════════════════════════════════════════════════════════════════
-- Five more trades
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO orientations
    (slug, name, description, primary_domain, secondary_domains, tags, is_curated)
VALUES
    ('platform-engineer',
     'Platform Engineer',
     'Construit la plateforme interne sur laquelle les autres équipes '
     'livrent : chemins par défaut, libre-service, expérience développeur.',
     'ops', ARRAY['code'], ARRAY['dx', 'self-service'], TRUE),

    ('kubernetes-specialist',
     'Spécialiste Kubernetes',
     'Opérateurs, maillage de services, GitOps. Le métier de faire tenir un '
     'cluster que quelqu''un d''autre utilisera sans y penser.',
     'ops', ARRAY['code'], ARRAY['k8s', 'operators', 'gitops'], TRUE),

    ('observability-engineer',
     'Ingénieur observabilité',
     'Métriques, journaux, traces et l''outillage qui les relie. Rend '
     'diagnosticable ce que personne ne peut plus lire en entier.',
     'ops', ARRAY['code'], ARRAY['metrics', 'traces', 'logs'], TRUE),

    ('incident-commander',
     'Responsable d''incident',
     'Conduit la réponse à un incident et écrit le post-mortem. Métier '
     'technique : savoir quelle question poser à trois heures du matin.',
     'ops', ARRAY['soft_skills'], ARRAY['incident', 'response'], TRUE),

    ('database-administrator',
     'Administrateur de bases de données',
     'PostgreSQL, MySQL, ClickHouse : réplication, réglage, reprise. Le '
     'métier où une erreur ne se rattrape pas par un redéploiement.',
     'ops', ARRAY['code'], ARRAY['dba', 'tuning', 'replication'], TRUE)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Who may review ops work
-- ═══════════════════════════════════════════════════════════════════
--
-- Same mechanism as migrations 0176 and 0210: the capability is
-- `{primary_domain}_reviewer:{reviewer_group}`, derived rather than invented
-- per domain.
--
-- Five families. Somebody who can read a Terraform plan can read a Helm
-- chart and cannot judge a query plan; somebody who has run incidents can
-- judge a post-mortem and has no opinion worth having on an index. The
-- groups are the shape of the competence.

ALTER TABLE user_capabilities
    DROP CONSTRAINT IF EXISTS user_capabilities_capability_check;

-- Every value, restated. A CHECK cannot be extended, only replaced, so this
-- carries everything 0094, 0098, 0117, 0120, 0176 and 0210 added — dropping
-- one would silently make that capability ungrantable and every guard
-- reading it would start refusing everybody.
ALTER TABLE user_capabilities
    ADD CONSTRAINT user_capabilities_capability_check
    CHECK (capability IN (
        -- P18 base
        'challenger', 'mentor', 'project_steward', 'pr_reviewer',
        'bounty_funder', 'issue_proposer', 'jury_tournament', 'admin',
        'enterprise_recruiter',
        -- P25 community moderation
        'community_moderator', 'forum_moderator',
        'plagiarism_reviewer', 'kyc_reviewer', 'community_curator',
        -- P26 beginner sas (migration 0117)
        'verified_apprentice', 'apprentice_verifier',
        -- P26 v2 per-domain challenge validators (migration 0120)
        'challenge_validator:code',
        'challenge_validator:design',
        'challenge_validator:game',
        'challenge_validator:security',
        'challenge_validator:ops',
        'challenge_validator:ai',
        'challenge_validator:soft_skills',
        -- Code review, by family of trade (migration 0176)
        'code_reviewer:web',
        'code_reviewer:mobile',
        'code_reviewer:systems',
        'code_reviewer:blockchain',
        'code_reviewer:compilers',
        'code_reviewer:data',
        'code_reviewer:scientific',
        'code_reviewer:devtools-media',
        'code_reviewer:all',
        -- AI review, by family of trade (migration 0210)
        'ai_reviewer:data',
        'ai_reviewer:ml',
        'ai_reviewer:llm-nlp',
        'ai_reviewer:cv',
        'ai_reviewer:safety',
        'ai_reviewer:all',
        -- Ops review, by family of trade (this migration)
        'ops_reviewer:infra',
        'ops_reviewer:reliability',
        'ops_reviewer:cloud',
        'ops_reviewer:observability',
        'ops_reviewer:data',
        'ops_reviewer:all'
    ));

UPDATE orientations SET reviewer_group = g.grp
  FROM (VALUES
    -- What is built and how it is shipped (3)
    ('devops-engineer',        'infra'),
    ('platform-engineer',      'infra'),
    ('kubernetes-specialist',  'infra'),
    -- Whether it keeps working, and what happens when it does not (2)
    ('sre',                    'reliability'),
    ('incident-commander',     'reliability'),
    -- Where it runs and what it costs (1)
    ('cloud-architect',        'cloud'),
    -- Whether anybody can see what it is doing (1)
    ('observability-engineer', 'observability'),
    -- The part that cannot be fixed by redeploying (1)
    ('database-administrator', 'data')
  ) AS g(slug, grp)
 WHERE orientations.slug = g.slug;

-- ═══════════════════════════════════════════════════════════════════
-- Ops artefacts
-- ═══════════════════════════════════════════════════════════════════
--
-- The thing an ops contributor produces is rarely a feature. It is a module,
-- a chart, a pipeline, a dashboard, a runbook, a migration — and every one of
-- them is judged by whether somebody else can run it without the author in
-- the room.

ALTER TABLE project_slices
    DROP CONSTRAINT IF EXISTS project_slices_slice_type_check;

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_slice_type_check
    CHECK (slice_type IN (
        'github_issue', 'figma_frame', 'game_level', 'game_asset',
        'sec_target', 'cli_task', 'design_token', 'documentation',
        'code_artifact', 'ai_artifact', 'ops_artifact',
        'other'
    ));

ALTER TABLE project_slices
    ADD COLUMN ops_subtype VARCHAR(30),
    -- Where it runs. Plural because a module that works on AWS and on-prem
    -- is worth more than one that works on either, and flattening it to a
    -- single name would hide exactly that.
    ADD COLUMN ops_target_platforms TEXT[] NOT NULL DEFAULT '{}',
    -- The tools it is written against: terraform, pulumi, helm, argocd,
    -- prometheus. Same reasoning as `code_languages`.
    ADD COLUMN ops_tooling TEXT[] NOT NULL DEFAULT '{}';

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_ops_subtype_values
    CHECK (ops_subtype IS NULL OR ops_subtype IN (
        'iac_terraform',         -- modules and the documentation to use them
        'kubernetes_manifests',  -- manifests, charts, an operator
        'cicd_pipeline',         -- a pipeline somebody else's repo can adopt
        'observability_config',  -- dashboards, alerts, instrumentation
        'runbook_incident',      -- what to do at three in the morning
        'db_migration_scheme'    -- a migration or a tuning change, with its plan
    ));

-- A subtype only means something on an ops artefact, and an ops artefact
-- without one says nothing about what was actually built.
ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_ops_subtype_belongs CHECK (
        (slice_type = 'ops_artifact') = (ops_subtype IS NOT NULL)
    );

COMMENT ON COLUMN project_slices.ops_target_platforms IS
    'Plural: a module that runs on AWS and on-prem is worth more than one '
    'that runs on either, and a single name would hide exactly that.';

CREATE INDEX idx_slices_ops_subtype
    ON project_slices (ops_subtype)
    WHERE ops_subtype IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- What an ops attestation can rest on
-- ═══════════════════════════════════════════════════════════════════
--
-- Same discipline as code and AI: each basis names something a stranger can
-- check. "Reliable" is not a basis; a service that met a stated SLO over a
-- stated window is.

ALTER TABLE attestations
    DROP CONSTRAINT IF EXISTS attestations_basis_check;

ALTER TABLE attestations
    ADD CONSTRAINT attestations_basis_check
    CHECK (basis IS NULL OR basis IN (
        -- Code (migration 0178)
        'code_pr_merged_upstream',
        'code_project_shipped',
        'code_library_published',
        'code_rfc_accepted',
        'code_standard_contribution',
        'code_devtool_adopted',
        'featured_coder',
        -- AI (migration 0213)
        'ai_model_shipped',
        'ai_dataset_published',
        'ai_agent_system_deployed',
        'ai_paper_published',
        'ai_benchmark_result',
        'ai_safety_finding_validated',
        'featured_ai_researcher',
        -- Contests (migration 0234)
        'contest_finalist',
        'contest_hired',
        -- Ops (this migration)
        'ops_infra_shipped',
        'ops_uptime_achievement',
        'ops_incident_led',
        'ops_migration_completed',
        'ops_observability_stack_shipped',
        'ops_cost_optimization',
        'featured_ops_engineer'
    ));

-- The ones that rest on an artefact have to name it, like their code and AI
-- equivalents. The incident and uptime ones do not: they rest on a period,
-- which is recorded elsewhere in this migration.
ALTER TABLE attestations
    DROP CONSTRAINT IF EXISTS attestations_artifact_basis_links_a_deliverable;

ALTER TABLE attestations
    ADD CONSTRAINT attestations_artifact_basis_links_a_deliverable
    CHECK (
        basis IS NULL
        OR basis NOT IN ('code_pr_merged_upstream', 'code_project_shipped',
                         'code_library_published',
                         'ai_model_shipped', 'ai_dataset_published',
                         'ai_agent_system_deployed', 'ai_paper_published',
                         'ai_benchmark_result', 'ai_safety_finding_validated',
                         'ops_infra_shipped', 'ops_migration_completed',
                         'ops_observability_stack_shipped')
        OR cardinality(linked_deliverable_ids) >= 1
    );

-- ═══════════════════════════════════════════════════════════════════
-- Reliability, incidents and cost — the three things ops is measured on
-- ═══════════════════════════════════════════════════════════════════

-- A service objective somebody has committed to, and what actually happened.
--
-- Two numbers rather than one word. "Ninety-nine point nine" and "we were up
-- most of the time" are different claims, and only one of them can be
-- disputed.
CREATE TABLE ops_service_objectives (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slice_id UUID REFERENCES project_slices(id) ON DELETE CASCADE,
    project_id UUID REFERENCES projects(id) ON DELETE CASCADE,
    owner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    service_name VARCHAR(120) NOT NULL CHECK (btrim(service_name) <> ''),
    -- What is promised, as a percentage over the window.
    target_percent NUMERIC(6,3) NOT NULL
        CHECK (target_percent > 0 AND target_percent <= 100),
    window_days SMALLINT NOT NULL CHECK (window_days BETWEEN 7 AND 365),

    -- What happened. NULL until the window closes.
    achieved_percent NUMERIC(6,3)
        CHECK (achieved_percent IS NULL
               OR (achieved_percent >= 0 AND achieved_percent <= 100)),
    -- Where the figure comes from. A number with no source is a claim.
    evidence_url VARCHAR(500)
        CHECK (evidence_url IS NULL OR evidence_url ~ '^https://'),

    started_on DATE NOT NULL,
    closed_at TIMESTAMPTZ,
    verified_by UUID REFERENCES users(id) ON DELETE SET NULL,
    verified_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT one_parent CHECK (
        (slice_id IS NOT NULL)::int + (project_id IS NOT NULL)::int >= 1
    ),
    -- A closed window states what was achieved and where that came from.
    -- Without both, the attestation it would produce rests on nothing.
    CONSTRAINT a_closed_window_reports_and_shows CHECK (
        closed_at IS NULL
        OR (achieved_percent IS NOT NULL AND evidence_url IS NOT NULL)
    ),
    CONSTRAINT verification_follows_closing CHECK (
        verified_at IS NULL OR closed_at IS NOT NULL
    )
);

COMMENT ON CONSTRAINT a_closed_window_reports_and_shows ON ops_service_objectives IS
    'A closed window states what was achieved and where the figure came from. '
    'Without both, the attestation it would produce rests on nothing.';

CREATE INDEX idx_objectives_owner
    ON ops_service_objectives (owner_user_id, started_on DESC);
CREATE INDEX idx_objectives_open
    ON ops_service_objectives (started_on)
    WHERE closed_at IS NULL;

-- Incidents, and the post-mortem that has to follow.
--
-- Blameless is not a value statement here, it is a constraint: the
-- post-mortem records what the system allowed, and there is no column for
-- who did it. A post-mortem naming a person is a post-mortem nobody writes
-- honestly the second time.
CREATE TABLE ops_incidents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID REFERENCES projects(id) ON DELETE CASCADE,
    commander_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,

    title VARCHAR(200) NOT NULL CHECK (btrim(title) <> ''),
    severity VARCHAR(10) NOT NULL CHECK (severity IN ('sev1', 'sev2', 'sev3', 'sev4')),
    -- Minutes. The two numbers every incident review starts from.
    time_to_detect_minutes INTEGER
        CHECK (time_to_detect_minutes IS NULL OR time_to_detect_minutes >= 0),
    time_to_resolve_minutes INTEGER
        CHECK (time_to_resolve_minutes IS NULL OR time_to_resolve_minutes >= 0),

    started_at TIMESTAMPTZ NOT NULL,
    resolved_at TIMESTAMPTZ,

    -- What the system allowed. Not who typed what.
    postmortem_md TEXT,
    postmortem_published_at TIMESTAMPTZ,
    postmortem_url VARCHAR(500)
        CHECK (postmortem_url IS NULL OR postmortem_url ~ '^https://'),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT an_incident_runs_forward CHECK (
        resolved_at IS NULL OR resolved_at >= started_at
    ),
    -- A published post-mortem has a body. An empty one published on time is
    -- the ritual without the practice.
    CONSTRAINT a_published_postmortem_says_something CHECK (
        postmortem_published_at IS NULL
        OR (postmortem_md IS NOT NULL AND length(btrim(postmortem_md)) >= 200)
    )
);

COMMENT ON TABLE ops_incidents IS
    'Blameless is a constraint here, not a value statement: there is no '
    'column for who did it. A post-mortem naming a person is one nobody '
    'writes honestly the second time.';

CREATE INDEX idx_incidents_commander
    ON ops_incidents (commander_user_id, started_at DESC);
CREATE INDEX idx_incidents_open
    ON ops_incidents (started_at)
    WHERE resolved_at IS NULL;

CREATE OR REPLACE FUNCTION touch_ops_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_incidents_updated_at
    BEFORE UPDATE ON ops_incidents
    FOR EACH ROW EXECUTE FUNCTION touch_ops_updated_at();

-- What the post-mortem said would be done. The part everybody skips.
CREATE TABLE ops_incident_actions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    incident_id UUID NOT NULL REFERENCES ops_incidents(id) ON DELETE CASCADE,

    description TEXT NOT NULL CHECK (btrim(description) <> ''),
    owner_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    due_on DATE,
    done_at TIMESTAMPTZ,
    -- Why it was dropped, when it was. An action item that quietly
    -- disappears is how the same incident happens twice.
    abandoned_reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT abandoning_says_why CHECK (
        abandoned_reason IS NULL OR done_at IS NULL
    )
);

COMMENT ON TABLE ops_incident_actions IS
    'The part everybody skips. An action item that quietly disappears is how '
    'the same incident happens twice.';

CREATE INDEX idx_incident_actions_open
    ON ops_incident_actions (due_on)
    WHERE done_at IS NULL AND abandoned_reason IS NULL;

-- Cost work, with the two numbers that make it a claim rather than a story.
CREATE TABLE ops_cost_optimisations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slice_id UUID REFERENCES project_slices(id) ON DELETE CASCADE,
    project_id UUID REFERENCES projects(id) ON DELETE CASCADE,
    owner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    scope VARCHAR(200) NOT NULL CHECK (btrim(scope) <> ''),
    monthly_before NUMERIC(12,2) NOT NULL CHECK (monthly_before > 0),
    monthly_after NUMERIC(12,2) NOT NULL CHECK (monthly_after >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'USD' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    -- What was changed. A saving with no explanation is a saving somebody
    -- made by turning off something that was needed.
    change_md TEXT NOT NULL CHECK (length(btrim(change_md)) >= 100),
    evidence_url VARCHAR(500)
        CHECK (evidence_url IS NULL OR evidence_url ~ '^https://'),

    -- Whether the thing still works. A cost reduction that broke the service
    -- is an outage with a spreadsheet.
    measured_over_days SMALLINT NOT NULL DEFAULT 30
        CHECK (measured_over_days BETWEEN 7 AND 180),
    service_still_meets_slo BOOLEAN,

    verified_by UUID REFERENCES users(id) ON DELETE SET NULL,
    verified_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT one_parent CHECK (
        (slice_id IS NOT NULL)::int + (project_id IS NOT NULL)::int >= 1
    ),
    -- A saving that is not a saving.
    CONSTRAINT a_reduction_reduces CHECK (monthly_after < monthly_before),
    -- Verified means somebody checked both halves.
    CONSTRAINT verification_covers_the_service_too CHECK (
        verified_at IS NULL OR service_still_meets_slo IS NOT NULL
    )
);

COMMENT ON CONSTRAINT verification_covers_the_service_too ON ops_cost_optimisations IS
    'A cost reduction that broke the service is an outage with a '
    'spreadsheet. Verifying one half without the other says nothing.';

CREATE INDEX idx_cost_optimisations_owner
    ON ops_cost_optimisations (owner_user_id, created_at DESC);
