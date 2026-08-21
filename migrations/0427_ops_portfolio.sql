-- Where an ops contributor's work already lives, and what they already hold.
--
-- ## Two tickets, one new table between them
--
-- The backlog asked for `user_ops_portfolios(user_id, platform,
-- metadata_json)`. That table already exists twice over under other names,
-- and a third JSONB-per-platform blob would mean a third reader parsing a
-- third shape:
--
--   * an *account* on a platform is `user_external_portfolios`, which
--     migration 0415 renamed away from `user_code_portfolios` and gave a
--     `portfolio_platforms` table — so a new registry is a row there;
--   * a *published artefact* and its usage figures is
--     `published_artifact_stats`, which migration 0216 already generalised
--     away from `code_package_stats` for exactly this reason. A Terraform
--     module on the registry and a crate on crates.io answer the same
--     question with the same row.
--
-- What genuinely has no home is a certification issued by somebody else:
-- AWS, Google, the CNCF, HashiCorp. It is not an account, it is not an
-- artefact, it expires, and nobody on this platform can verify it by calling
-- an API. That is the new table.

-- ═══════════════════════════════════════════════════════════════════
-- Four registries an ops contributor publishes to
-- ═══════════════════════════════════════════════════════════════════
--
-- Rows on `portfolio_platforms`, which carries what each one's numbers mean.
-- That is the part a CHECK could not hold and the part that matters here:
-- ArtifactHub counts stars and Docker Hub counts pulls, and printing either
-- under the word "downloads" would claim something neither measured.

INSERT INTO portfolio_platforms
    (slug, skill_domain, name, profile_url_pattern, items_label, reach_label,
     has_public_api, sort_order)
VALUES
    ('terraform_registry', 'ops', 'Terraform Registry',
     'https://registry.terraform.io/namespaces/{handle}',
     'modules', 'téléchargements', TRUE, 210),
    ('ansible_galaxy', 'ops', 'Ansible Galaxy',
     'https://galaxy.ansible.com/ui/standalone/namespaces/{handle}',
     'collections', 'téléchargements', TRUE, 220),
    ('artifacthub', 'ops', 'ArtifactHub',
     'https://artifacthub.io/packages/search?user={handle}',
     'paquets', 'étoiles', TRUE, 230),
    ('docker_hub', 'ops', 'Docker Hub',
     'https://hub.docker.com/u/{handle}',
     'images', 'pulls', TRUE, 240)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Published artefacts: four more registries
-- ═══════════════════════════════════════════════════════════════════
--
-- Every value restated: a CHECK cannot be extended, and dropping one by
-- accident is how two tournament kinds vanished in migration 0223.
--
-- None of these four publishes a download count the way crates.io does.
-- ArtifactHub publishes a subscriber count, the Terraform registry publishes
-- downloads per version. Where a figure is absent it stays NULL rather than
-- zero, which the table was built for: a registry that measures nothing must
-- not read as a package nobody uses.

ALTER TABLE published_artifact_stats
    DROP CONSTRAINT IF EXISTS published_artifact_stats_registry_check;

ALTER TABLE published_artifact_stats
    ADD CONSTRAINT published_artifact_stats_registry_check
    CHECK (registry IN (
        -- Package registries (migration 0183).
        'crates_io', 'npm', 'pypi', 'go_modules', 'maven_central',
        'rubygems', 'nuget', 'packagist', 'hex_pm', 'homebrew',
        -- Model and dataset hubs (migration 0216).
        'huggingface_models', 'huggingface_datasets', 'kaggle_datasets',
        -- Infrastructure registries (this migration).
        'terraform_registry', 'ansible_galaxy', 'artifacthub', 'docker_hub'
    ));

-- ═══════════════════════════════════════════════════════════════════
-- Certifications somebody else issued
-- ═══════════════════════════════════════════════════════════════════
--
-- ## Why this is not `certifications` or `program_certifications`
--
-- Those two are things Skilluv issues. This is a thing Skilluv records
-- somebody else issuing, and the difference decides everything about the
-- table: Skilluv cannot revoke it, cannot re-verify it after the fact, and
-- must never present it with the same weight as an attestation it stands
-- behind.
--
-- ## Why it expires, loudly
--
-- Almost every credential in this list runs out — three years for the AWS
-- ones, two for the CNCF ones. A profile showing a lapsed certification as
-- current is the platform making a false claim on somebody's behalf. The
-- expiry date is required for any credential whose programme has one, and
-- `is_current` is derived rather than stored so it cannot drift.
--
-- ## Why verification is a person and not an API
--
-- None of these issuers offers a public lookup by holder. Credly and Acclaim
-- publish a badge page per credential, which is a link a human can open and
-- compare against the name on the account — so that is the check: a
-- reviewer opens it, and records that they did. An unverified credential
-- shows as claimed, exactly like an unverified forge handle.

CREATE TABLE external_credentials (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    issuer VARCHAR(30) NOT NULL CHECK (issuer IN (
        'aws', 'google_cloud', 'microsoft_azure', 'cncf', 'hashicorp',
        'red_hat', 'oracle', 'other'
    )),
    -- The credential's own name, as the issuer writes it. Not an enum: the
    -- list changes every year and a CHECK would make the platform's
    -- vocabulary lag the industry's by a release cycle.
    name VARCHAR(160) NOT NULL CHECK (btrim(name) <> ''),
    -- Roughly how hard it is, in the issuer's own ladder. Used by the score;
    -- a professional-level certification is not a foundational one.
    level VARCHAR(20) NOT NULL DEFAULT 'associate'
        CHECK (level IN ('foundational', 'associate', 'professional', 'specialty')),

    credential_id VARCHAR(120),
    -- The public page a reviewer opens. Required: a certification nobody can
    -- look at is a line on a CV, and this platform exists because those are
    -- not enough.
    evidence_url VARCHAR(500) NOT NULL CHECK (evidence_url ~ '^https://'),

    issued_on DATE NOT NULL,
    expires_on DATE,

    verified_by UUID REFERENCES users(id) ON DELETE SET NULL,
    verified_at TIMESTAMPTZ,
    -- What the reviewer saw. Kept because "verified" means something
    -- different depending on what was actually opened.
    verification_note TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- The same credential twice on one profile is a data-entry mistake, not
    -- two credentials.
    UNIQUE (user_id, issuer, name, issued_on),

    CONSTRAINT a_credential_runs_forward
        CHECK (expires_on IS NULL OR expires_on > issued_on),

    -- A verification with no reviewer is one nobody can be asked about.
    CONSTRAINT verification_names_its_reviewer
        CHECK (verified_at IS NULL OR verified_by IS NOT NULL)
);

COMMENT ON TABLE external_credentials IS
    'Certifications issued by somebody else — AWS, Google, the CNCF, '
    'HashiCorp. Recorded, never issued: Skilluv cannot revoke these and must '
    'not present them with the weight of an attestation it stands behind.';

-- ## Why "still valid" is not a column
--
-- It was one, briefly: a stored generated column over `expires_on >=
-- CURRENT_DATE`. PostgreSQL refuses that, and it is right to — the value
-- would be computed once at write time and then be wrong from the following
-- midnight onwards, which is precisely the failure this table exists to
-- avoid. A credential that lapsed last night must not read as current
-- because nobody wrote to the row.
--
-- So it is a view, computed on read, and every reader goes through it.

CREATE VIEW credentials_with_currency AS
    SELECT c.*,
           (c.expires_on IS NULL OR c.expires_on >= CURRENT_DATE) AS is_current
      FROM external_credentials c;

COMMENT ON VIEW credentials_with_currency IS
    'external_credentials with `is_current` computed on read. A stored column '
    'would be right on the day it was written and wrong the morning after.';

CREATE INDEX idx_external_credentials_user
    ON external_credentials (user_id, expires_on DESC NULLS FIRST, issued_on DESC);

CREATE INDEX idx_external_credentials_unverified
    ON external_credentials (created_at)
    WHERE verified_at IS NULL;

-- Which ones are about to lapse, for the notice that goes out before they do.
CREATE INDEX idx_external_credentials_expiring
    ON external_credentials (expires_on)
    WHERE expires_on IS NOT NULL AND verified_at IS NOT NULL;

CREATE TRIGGER trg_external_credentials_updated_at
    BEFORE UPDATE ON external_credentials
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- What a credential is worth in the ops score
-- ═══════════════════════════════════════════════════════════════════
--
-- Less than an artefact, and that ordering is the position rather than an
-- accident of tuning. A certification says somebody passed an exam a company
-- wrote about its own product; a verified module says somebody built a thing
-- another person now runs. Both are worth recording. Only one of them is
-- what this platform is for.
--
-- Expired credentials count for nothing, and unverified ones count for
-- nothing, which is the same rule every other term follows.

INSERT INTO craft_score_weights
    (skill_domain, term, weight, kind, baseline, explanation, sort_order)
VALUES
    ('ops', 'credentials_current', 20, 'count', NULL,
     'Chaque certification externe vérifiée et encore valide. Vaut moins '
     'qu''un artefact livré : un examen dit qu''on a révisé, un module dit '
     'qu''on a construit.', 115)
ON CONFLICT DO NOTHING;
