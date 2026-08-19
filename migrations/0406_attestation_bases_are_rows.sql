-- What an attestation can rest on, as rows — and seven of them for audio.
--
-- ## The third list of the same shape
--
-- `attestations.basis` was created by 0178 with seven code values and a second
-- CHECK naming the subset that must point at a deliverable. 0213 restated both
-- to add seven AI values. Audio would restate both again, and quality and
-- leadership after it — the pattern migrations 0228 and 0305 documented, and
-- that 0400 and 0404 stopped for domains and capabilities.
--
-- Two CHECKs become one table and one trigger. Adding a basis is an INSERT.
--
-- ## Why the deliverable rule becomes a trigger
--
-- The rule is per-basis — "a shipped model must name the artefact, an
-- editorial feature need not" — and a CHECK cannot read another table to find
-- out which. It was expressible as a CHECK only by writing the subset out,
-- which is the thing being removed. `requires_deliverable` is a column on the
-- row it describes, and the trigger reads it.
--
-- ## Why the wording moves here
--
-- `services::ai_attestations::wording` holds the title and description of each
-- basis in Rust, on the stated grounds that an attestation keeps the words it
-- was issued with. It does — the words are copied onto the attestation row at
-- issue — and that is true whether they came from a constant or a table. What
-- the table adds is that a typo in a title somebody will read on a public
-- profile is fixed by an operator rather than by a deployment, which is the
-- same argument that put review grids, badge rules, craft-score weights and
-- content guides in rows.
--
-- ## The seven audio bases
--
-- Six rest on something a stranger can open — a track, a pack, a reel, a
-- middleware project in a shipped build, a merged audio contribution, a credit
-- on a released work. The seventh is editorial, like `featured_coder` and
-- `featured_ai_researcher`, and names a person rather than an artefact.
--
-- `audio_project_credited` is the one that has no equivalent in the other
-- domains, and it is the one that matters most here: the normal outcome of
-- audio work is that it ships inside somebody else's thing, under somebody
-- else's name, and the credit is the only trace. A domain that could not
-- attest that would be unable to describe most of its own field.

CREATE TABLE attestation_bases (
    basis VARCHAR(40) PRIMARY KEY,
    skill_domain VARCHAR(30) NOT NULL REFERENCES skill_domains(slug) ON UPDATE CASCADE,
    -- The words the attestation is issued with, in the default locale.
    title VARCHAR(120) NOT NULL,
    description TEXT NOT NULL,
    -- Whether the claim has to name the deliverable that carries it. FALSE for
    -- the editorial ones, which are a decision about a person.
    requires_deliverable BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order SMALLINT NOT NULL DEFAULT 100,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE attestation_bases IS
    'Everything an attestation can rest on. Replaces two CHECK constraints '
    'that every domain had to restate — see migrations 0178 and 0213 — and '
    'carries the wording each basis is issued with.';

COMMENT ON COLUMN attestation_bases.requires_deliverable IS
    'Whether an attestation on this basis must link the deliverable that '
    'carries it. Enforced by trigger rather than CHECK: the answer differs per '
    'basis, and a CHECK cannot read this column to find out which.';

INSERT INTO attestation_bases
    (basis, skill_domain, title, description, requires_deliverable, sort_order) VALUES

-- Code (migration 0178)
('code_pr_merged_upstream', 'code', 'Contribution fusionnée en amont',
 'Une contribution acceptée dans un dépôt que la personne ne contrôle pas.', TRUE, 10),
('code_project_shipped', 'code', 'Projet mis en service',
 'Un projet livré et accessible, avec une adresse où il répond.', TRUE, 20),
('code_library_published', 'code', 'Bibliothèque publiée',
 'Une bibliothèque publiée sur un registre public, installable par un inconnu.', TRUE, 30),
('code_rfc_accepted', 'code', 'Proposition acceptée',
 'Une RFC ou une proposition de conception retenue par un projet.', FALSE, 40),
('code_standard_contribution', 'code', 'Contribution à un standard',
 'Une contribution à une spécification ouverte.', FALSE, 50),
('code_devtool_adopted', 'code', 'Outil adopté',
 'Un outil de développement repris par d''autres que son auteur.', FALSE, 60),
('featured_coder', 'code', 'Mis en avant',
 'Un travail retenu par la rédaction pour son exemplarité.', FALSE, 70),

-- AI (migration 0213)
('ai_model_shipped', 'ai', 'Modèle mis en service',
 'Un modèle publié à une adresse où un inconnu peut l''obtenir et l''exécuter.', TRUE, 110),
('ai_dataset_published', 'ai', 'Jeu de données publié',
 'Un jeu de données publié avec sa fiche : provenance, licence et limites.', TRUE, 120),
('ai_agent_system_deployed', 'ai', 'Système d''agents déployé',
 'Un système d''agents en service, avec ses évaluations et ses garde-fous.', TRUE, 130),
('ai_paper_published', 'ai', 'Article publié',
 'Un article paru, préprint ou conférence, avec le code qui le soutient.', TRUE, 140),
('ai_benchmark_result', 'ai', 'Résultat de banc reproduit',
 'Un résultat mesuré sur un banc public, qu''un relecteur a rejoué et retrouvé.', TRUE, 150),
('ai_safety_finding_validated', 'ai', 'Trouvaille de sûreté validée',
 'Une trouvaille reproduite, évaluée en gravité et divulguée dans les règles.', TRUE, 160),
('featured_ai_researcher', 'ai', 'Mis en avant',
 'Un travail IA retenu par la rédaction pour son exemplarité.', FALSE, 170),

-- Design (migration 0233). The branch that added these was open when this
-- table was written, and a CHECK-to-table conversion that forgets a value
-- does not fail loudly — it fails later, on the foreign key, the first time
-- somebody wins a design contest.
('design_deliverable_validated', 'design', 'Livrable validé',
 'Un livrable de design validé après critique.', TRUE, 310),
('design_brand_system_delivered', 'design', 'Identité livrée',
 'Une identité complète et ses règles d''usage.', TRUE, 320),
('design_typeface_released', 'design', 'Caractère publié',
 'Une famille de caractères publiée avec ses fichiers de production.', TRUE, 330),
('design_system_adopted', 'design', 'Système adopté',
 'Un système de design repris par une équipe qui construit dessus.', TRUE, 340),
('design_contest_won', 'design', 'Concours remporté',
 'Une place sur le podium d''un concours de design.', FALSE, 350),
('design_mission_delivered', 'design', 'Mission livrée',
 'Une mission payée, livrée et acceptée par le client.', FALSE, 360),
('featured_designer', 'design', 'Mis en avant',
 'Un travail de design retenu par la rédaction pour son exemplarité.', FALSE, 370),

-- Audio
('audio_composition_published', 'audio', 'Composition publiée',
 'Une composition originale livrée, écoutable, avec ses stems et ses licences en règle.', TRUE, 210),
('audio_soundpack_delivered', 'audio', 'Pack sonore livré',
 'Un ensemble de sons cohérent, nommé et documenté, utilisable tel quel.', TRUE, 220),
('audio_voice_reel_validated', 'audio', 'Bande démo validée',
 'Une bande démo de comédien voix jugée exploitable par un relecteur du métier.', TRUE, 230),
('audio_adaptive_system_shipped', 'audio', 'Système musical adaptatif en service',
 'Une musique interactive intégrée et vérifiée dans une build jouable.', TRUE, 240),
('audio_programming_contribution', 'audio', 'Contribution de programmation audio',
 'Une fonctionnalité audio — DSP, spatialisation, synthèse — livrée dans un moteur ou une bibliothèque.', TRUE, 250),
('audio_project_credited', 'audio', 'Crédité sur une œuvre publiée',
 'Un crédit sur un jeu, un film, un podcast ou une pièce sortie. Le travail audio vit d''ordinaire à l''intérieur de celui d''un autre, et le crédit en est la seule trace.', TRUE, 260),
('featured_audio_creator', 'audio', 'Mis en avant',
 'Un travail audio retenu par la rédaction pour son exemplarité.', FALSE, 270);

CREATE TRIGGER trg_attestation_bases_updated_at
    BEFORE UPDATE ON attestation_bases
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- The two CHECKs go
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE attestations
    DROP CONSTRAINT IF EXISTS attestations_basis_check,
    DROP CONSTRAINT IF EXISTS attestations_artifact_basis_links_a_deliverable,
    ADD CONSTRAINT attestations_basis_fkey
        FOREIGN KEY (basis) REFERENCES attestation_bases(basis) ON UPDATE CASCADE;

CREATE FUNCTION trg_attestations_basis_links_a_deliverable() RETURNS TRIGGER AS $$
DECLARE
    needed BOOLEAN;
BEGIN
    IF NEW.basis IS NULL THEN
        RETURN NEW;
    END IF;

    SELECT requires_deliverable INTO needed
      FROM attestation_bases WHERE basis = NEW.basis;

    IF needed AND cardinality(NEW.linked_deliverable_ids) < 1 THEN
        RAISE EXCEPTION
            'an attestation on basis % must link the deliverable that carries it', NEW.basis
            USING ERRCODE = 'check_violation';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION trg_attestations_basis_links_a_deliverable() IS
    'The rule migration 0178 wrote as a CHECK over a hand-listed subset: a '
    'basis that names something public has to point at it, or it is a label '
    'rather than a claim a stranger can check.';

CREATE TRIGGER trg_attestations_basis_links_a_deliverable
    BEFORE INSERT OR UPDATE OF basis, linked_deliverable_ids ON attestations
    FOR EACH ROW EXECUTE FUNCTION trg_attestations_basis_links_a_deliverable();
