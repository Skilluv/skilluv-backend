-- Licences, and the promises a mission is allowed to make.
--
-- ## The problem this stops
--
-- An enterprise publishes a mission: "extend this repository, we own the
-- result". The repository is GPL. The enterprise does not own the result and
-- cannot, and nobody involved finds out until a lawyer does — usually after
-- the work is delivered and paid for.
--
-- That is not a rare edge case. It is the single most common legal accident
-- in commercial open source work, and the person who pays for it is almost
-- always the contractor, because they are the one who signed something they
-- could not deliver.
--
-- ## What this can and cannot do
--
-- It can refuse a mission whose IP terms contradict the licence it builds on,
-- and it does. It is not legal advice and does not pretend to be: the table
-- below carries a `caveat` for each licence saying what the platform is
-- confident about and what needs a lawyer.
--
-- ## Why the licences are rows
--
-- Because the list changes, because the categories are arguable, and because
-- somebody should be able to add SSPL or a source-available licence without a
-- deployment. Also because the caveats are text somebody will want to improve
-- after talking to a lawyer.

CREATE TABLE software_licenses (
    spdx_id VARCHAR(60) PRIMARY KEY,
    name VARCHAR(160) NOT NULL,
    category VARCHAR(30) NOT NULL CHECK (category IN (
        -- Do what you like, keep the notice. MIT, Apache, BSD.
        'permissive',
        -- Changes to the licensed files stay under it; the rest of your
        -- program does not. MPL, LGPL.
        'weak_copyleft',
        -- The whole derivative work carries the licence. GPL.
        'strong_copyleft',
        -- Strong copyleft that also triggers on running it as a service.
        -- AGPL, and the reason SaaS companies fear it.
        'network_copyleft',
        -- Readable, not freely usable. Business Source, SSPL, "source
        -- available".
        'source_available',
        'proprietary'
    )),
    -- Whether a client can be promised ownership of a derivative work.
    allows_client_ownership BOOLEAN NOT NULL,
    -- Whether the output must be released openly.
    requires_open_release BOOLEAN NOT NULL,
    -- Whether a NOTICE or attribution file has to travel with it.
    requires_attribution BOOLEAN NOT NULL DEFAULT TRUE,
    -- What the platform is confident about, and what needs a lawyer. Shown to
    -- whoever is publishing the mission.
    caveat TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE software_licenses IS
    'Licences and what they permit a mission to promise. Not legal advice: '
    'each row carries the caveat that says where the platform stops.';

INSERT INTO software_licenses
    (spdx_id, name, category, allows_client_ownership, requires_open_release,
     requires_attribution, caveat)
VALUES
    ('MIT', 'MIT License', 'permissive', TRUE, FALSE, TRUE,
     'Le plus simple. Garde le fichier de licence et la mention de copyright '
     'dans ce que tu livres — c''est la seule obligation, et elle est oubliée '
     'plus souvent qu''on ne le croit.'),

    ('Apache-2.0', 'Apache License 2.0', 'permissive', TRUE, FALSE, TRUE,
     'Comme MIT, avec une clause de brevets en plus et un fichier NOTICE à '
     'transmettre. Incompatible avec la GPLv2 : si ton livrable doit être '
     'intégré à un projet GPLv2, vérifie avant de commencer.'),

    ('BSD-3-Clause', 'BSD 3-Clause License', 'permissive', TRUE, FALSE, TRUE,
     'Permissive. La troisième clause interdit d''utiliser le nom des auteurs '
     'pour promouvoir ton produit.'),

    ('ISC', 'ISC License', 'permissive', TRUE, FALSE, TRUE,
     'Équivalente à MIT en pratique.'),

    ('MPL-2.0', 'Mozilla Public License 2.0', 'weak_copyleft', TRUE, FALSE, TRUE,
     'Copyleft par fichier : tes modifications des fichiers MPL restent MPL, '
     'le reste de ton programme non. Utilisable dans un produit propriétaire, '
     'à condition de publier les fichiers modifiés.'),

    ('LGPL-3.0-only', 'GNU Lesser General Public License v3.0', 'weak_copyleft',
     TRUE, FALSE, TRUE,
     'Utilisable dans un produit fermé si la bibliothèque reste remplaçable '
     'par l''utilisateur — ce qui est simple en liaison dynamique et devient '
     'un sujet juridique en liaison statique. À faire vérifier.'),

    ('GPL-2.0-only', 'GNU General Public License v2.0', 'strong_copyleft',
     FALSE, TRUE, TRUE,
     'Tout ce qui dérive de ce code se distribue sous GPLv2. Un client ne peut '
     'pas en devenir propriétaire exclusif, quoi qu''il ait signé.'),

    ('GPL-3.0-only', 'GNU General Public License v3.0', 'strong_copyleft',
     FALSE, TRUE, TRUE,
     'Comme GPLv2, avec des clauses de brevets et anti-tivoïsation. Un client '
     'ne peut pas en devenir propriétaire exclusif.'),

    ('AGPL-3.0-only', 'GNU Affero General Public License v3.0', 'network_copyleft',
     FALSE, TRUE, TRUE,
     'La GPL, plus le déclenchement par l''usage en service : une entreprise '
     'qui expose ton travail via une API doit en publier la source. C''est la '
     'licence qui surprend le plus de monde, et toujours trop tard.'),

    ('BSL-1.1', 'Business Source License 1.1', 'source_available', FALSE, FALSE, TRUE,
     'Lisible, pas librement utilisable : l''usage commercial est restreint '
     'jusqu''à une date de bascule. Chaque projet fixe ses propres conditions '
     '— lis celles de ce dépôt-là, pas un résumé général.'),

    ('SSPL-1.0', 'Server Side Public License', 'source_available', FALSE, FALSE, TRUE,
     'Va plus loin que l''AGPL : proposer le logiciel en service oblige à '
     'publier aussi l''infrastructure autour. Peu de juristes s''y risquent.'),

    ('Unlicense', 'The Unlicense', 'permissive', TRUE, FALSE, FALSE,
     'Domaine public autant que le droit le permet. Sa validité dans les pays '
     'de droit civil — dont la France et le Bénin — est discutée.'),

    ('PROPRIETARY', 'Propriétaire / fermé', 'proprietary', TRUE, FALSE, FALSE,
     'Aucun droit sauf ceux que le contrat accorde. Tout est dans le contrat.');

-- ═══════════════════════════════════════════════════════════════════
-- What a project is under
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE projects
    ADD COLUMN license_spdx VARCHAR(60) REFERENCES software_licenses(spdx_id);

COMMENT ON COLUMN projects.license_spdx IS
    'What the upstream repository is under. NULL means unknown, which is a '
    'real answer for a repository with no LICENSE file — and a warning sign.';

ALTER TABLE missions
    -- The licence the deliverable will derive from. Usually the project's,
    -- occasionally different: a consulting report about a GPL codebase is not
    -- itself a derivative work.
    ADD COLUMN upstream_license_spdx VARCHAR(60) REFERENCES software_licenses(spdx_id);

-- ═══════════════════════════════════════════════════════════════════
-- A mission cannot promise what the licence forbids
-- ═══════════════════════════════════════════════════════════════════
--
-- Only when the licence is stated. A mission that names no licence is not
-- refused: most work has no upstream, and demanding one would block the
-- ordinary case to catch the rare one. What is refused is a stated
-- contradiction.

CREATE OR REPLACE FUNCTION mission_ip_terms_match_the_license()
RETURNS TRIGGER AS $$
DECLARE
    l RECORD;
BEGIN
    IF NEW.upstream_license_spdx IS NULL THEN
        RETURN NEW;
    END IF;

    SELECT * INTO l FROM software_licenses WHERE spdx_id = NEW.upstream_license_spdx;

    IF NEW.ip_terms IN ('full_ownership_client', 'retain_reusable_components')
       AND NOT l.allows_client_ownership THEN
        RAISE EXCEPTION
            'a % mission cannot promise client ownership: %',
            NEW.upstream_license_spdx, l.caveat
            USING HINT = 'use open_source_output or dual_license, or check the licence again';
    END IF;

    IF l.requires_open_release AND NEW.ip_terms = 'full_ownership_client' THEN
        RAISE EXCEPTION
            'work derived from % must be released under it: %',
            NEW.upstream_license_spdx, l.caveat;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_mission_ip_terms_match_the_license
    BEFORE INSERT OR UPDATE ON missions
    FOR EACH ROW EXECUTE FUNCTION mission_ip_terms_match_the_license();

-- ═══════════════════════════════════════════════════════════════════
-- Disclosure of AI assistance
-- ═══════════════════════════════════════════════════════════════════
--
-- The platform's position is disclosure, not prohibition: using an assistant
-- is fine, hiding it is not. A maintainer who does not know where a
-- contribution came from cannot review it properly, and a client who does not
-- know cannot judge their own copyright exposure.
--
-- ## Why this is not a CHECK constraint
--
-- The obvious implementation is "a verified deliverable states a level". It
-- would break the main path: a merged pull request reaches `verified` through
-- a GitHub webhook, and a webhook has nobody to ask. The constraint would
-- refuse the one artefact the platform cares about most.
--
-- So the columns that were already there — `ai_disclosure_prompted_at` and
-- `ai_disclosure_deadline_at` — get the mechanism they were added for. The
-- artefact is verified, the author is asked, and there is a deadline. Past it,
-- an undeclared artefact stops counting: not revoked, because somebody on
-- holiday is not somebody hiding anything, but not credited either.

CREATE OR REPLACE FUNCTION prompt_for_ai_disclosure()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.verification_status = 'verified'
       AND OLD.verification_status IS DISTINCT FROM 'verified'
       AND NEW.ai_assistance_level IS NULL
       AND NEW.ai_disclosure_prompted_at IS NULL THEN
        NEW.ai_disclosure_prompted_at := NOW();
        -- Two weeks. Long enough for a holiday, short enough that the
        -- profile is not carrying an unanswered question for a season.
        NEW.ai_disclosure_deadline_at := NOW() + INTERVAL '14 days';
    END IF;

    -- Declaring it clears the question. Written here rather than in six
    -- callers, one of which would forget.
    IF NEW.ai_assistance_level IS NOT NULL THEN
        NEW.ai_disclosure_deadline_at := NULL;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_prompt_for_ai_disclosure
    BEFORE UPDATE ON deliverables
    FOR EACH ROW EXECUTE FUNCTION prompt_for_ai_disclosure();

COMMENT ON FUNCTION prompt_for_ai_disclosure() IS
    'Disclosure, not prohibition, and not a CHECK: a merged pull request is '
    'verified by a webhook, which has nobody to ask.';

-- What "stops counting" means, in one place both the craft score and the
-- profile can read.
CREATE OR REPLACE VIEW countable_deliverables AS
SELECT d.*
  FROM deliverables d
 WHERE d.verification_status = 'verified'
   AND d.revoked_at IS NULL
   AND (
       d.ai_assistance_level IS NOT NULL
       OR d.ai_disclosure_deadline_at IS NULL
       OR d.ai_disclosure_deadline_at > NOW()
   );

COMMENT ON VIEW countable_deliverables IS
    'Verified, not revoked, and either declared or still inside its '
    'disclosure window. An artefact past its deadline with nothing declared '
    'is not revoked — somebody on holiday is not somebody hiding something — '
    'but it is not credited either.';
