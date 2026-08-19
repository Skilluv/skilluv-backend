-- Paid audio work, on the mission table that already exists.
--
-- ## Why there is no `audio_missions`
--
-- The backlog asked for one, with its own application flow and its own
-- payment path. Migration 0301 already refused that for AI and gave the
-- reason: `missions` is keyed by `skill_domain` and carries the applications,
-- the payment models, the IP terms, the commission and the state machine. A
-- second table means a second answer to "how many missions has this person
-- finished", and both get quoted.
--
-- `mission_applications` already holds `portfolio_urls` and `expertise`,
-- which is the whole of what M-02 asked for. Nothing new is needed there.
--
-- ## The fourth CHECK of the same shape
--
-- `missions.deliverable_format` was written by 0192 with four values and
-- restated by 0301 to add four more. Audio needs five. Same answer as 0400,
-- 0404, 0406 and 0408: a table.
--
-- ## `licensing_scope` is new, and it is not `ip_terms`
--
-- `ip_terms` says who owns the work. `licensing_scope` says what the client
-- may do with it, and in music those are routinely different: a composer who
-- keeps ownership still grants a licence, and the licence is where the money
-- and the disputes are. Territory, medium, duration and exclusivity are four
-- different questions that one ownership enum cannot answer.
--
-- It is nullable and general rather than audio-only. Illustration and
-- photography have exactly the same split, and a column named `audio_*` would
-- have to be renamed the first time design used it.
--
-- ## The royalty deal already exists
--
-- The backlog asked for a `royalty_deal` payment model. `revenue_share` was
-- written by 0192 and is the same arrangement under the name the rest of the
-- platform uses, with the percentage capped at fifty. Adding a synonym would
-- mean two ways to express one deal and two branches in the payout code.

CREATE TABLE mission_deliverable_formats (
    slug VARCHAR(30) PRIMARY KEY,
    skill_domain VARCHAR(30) REFERENCES skill_domains(slug) ON UPDATE CASCADE,
    name VARCHAR(100) NOT NULL,
    description TEXT NOT NULL,
    sort_order SMALLINT NOT NULL DEFAULT 100
);

COMMENT ON TABLE mission_deliverable_formats IS
    'What a mission hands over at the end. A table rather than a CHECK '
    'restated once per domain — 0192, 0301 — because the handover of a set of '
    'weights, a middleware project and a pull request have nothing in common '
    'except that somebody has to receive them.';

INSERT INTO mission_deliverable_formats (slug, skill_domain, name, description, sort_order) VALUES
    -- Migration 0192
    ('github_pr', 'code', 'Contribution fusionnée',
     'Une ou plusieurs contributions acceptées dans le dépôt du client.', 10),
    ('repository_handover', 'code', 'Remise de dépôt',
     'Un dépôt complet transféré, avec son historique et sa documentation.', 20),
    ('library_published', 'code', 'Bibliothèque publiée',
     'Une bibliothèque publiée sur un registre, sous la licence convenue.', 30),
    ('consulting_report', NULL, 'Rapport',
     'Un rapport écrit, avec son protocole et ses recommandations.', 40),
    -- Migration 0301
    ('model_weights', 'ai', 'Poids de modèle',
     'Les poids et la fiche qui les rend utilisables.', 110),
    ('dataset_delivered', 'ai', 'Jeu de données',
     'Un jeu de données avec sa provenance documentée.', 120),
    ('deployed_endpoint', 'ai', 'Service déployé',
     'Un service en fonctionnement, avec la documentation de son API.', 130),
    ('evaluation_report', 'ai', 'Rapport d''évaluation',
     'Un audit ou une évaluation, avec son protocole.', 140),
    -- Migration 0254. Design was on its own branch when this table was
    -- written; its five formats had been added to the CHECK this replaces,
    -- and dropping the CHECK without carrying them would have made every
    -- design mission unpublishable.
    ('design_source_files', 'design', 'Sources ouvrables',
     'Les fichiers sources et ce qu''il faut pour les rouvrir. Un livrable que personne ne peut rouvrir n''est pas livré.', 150),
    ('brand_package', 'design', 'Identité de marque',
     'Les marques, la palette, la typographie, et les règles qui disent comment s''en servir.', 160),
    ('motion_package', 'design', 'Animation et projet',
     'Une animation rendue et le projet qui la produit.', 170),
    ('prototype_link', 'design', 'Prototype navigable',
     'Un prototype que quelqu''un peut parcourir depuis un lien.', 180),
    ('design_system_handover', 'design', 'Design system remis',
     'Les tokens, les composants et leur documentation, remis à une équipe qui va construire dessus.', 190),
    -- Audio
    ('audio_master_stems', 'audio', 'Master et stems',
     'La pièce finale plus ses pistes séparées, alignées et nommées. Sans les stems, le client ne peut plus rien ajuster sans revenir vers l''auteur.', 210),
    ('audio_sound_pack', 'audio', 'Pack sonore',
     'Un ensemble de sons cohérent, nommé selon une convention, avec sa feuille d''usage.', 220),
    ('audio_voice_recording', 'audio', 'Enregistrement de voix',
     'Les prises retenues, montées et livrées au format demandé, avec l''étendue d''usage écrite.', 230),
    ('audio_middleware_project', 'audio', 'Projet middleware intégré',
     'Un projet FMOD ou Wwise et son intégration, vérifiés dans une build jouable.', 240),
    ('audio_code_contribution', 'audio', 'Contribution audio en code',
     'Du code audio livré dans le moteur du client, avec sa démonstration et sa documentation.', 250);

ALTER TABLE missions
    DROP CONSTRAINT IF EXISTS missions_deliverable_format_check,
    ADD CONSTRAINT missions_deliverable_format_fkey
        FOREIGN KEY (deliverable_format)
        REFERENCES mission_deliverable_formats(slug) ON UPDATE CASCADE;

-- ═══════════════════════════════════════════════════════════════════
-- What the client may do with it
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE mission_licensing_scopes (
    slug VARCHAR(30) PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    description TEXT NOT NULL,
    -- Whether the creator may still show the work in their portfolio. The
    -- answer is yes almost always, and the exception has to be visible: a
    -- creator who cannot show what they made cannot prove they made it, which
    -- on this platform is the entire currency.
    permits_portfolio_use BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order SMALLINT NOT NULL DEFAULT 100
);

INSERT INTO mission_licensing_scopes
    (slug, name, description, permits_portfolio_use, sort_order) VALUES
    ('sync_only', 'Synchronisation seule',
     'Usage à l''image dans l''œuvre nommée, et rien d''autre. Le plus étroit, et le plus courant pour une musique de jeu ou de film.', TRUE, 10),
    ('commercial_limited', 'Commercial limité',
     'Usage commercial borné : un support, un territoire, une durée. Ce qui est hors du cadre demande un avenant.', TRUE, 20),
    ('commercial_worldwide', 'Commercial mondial',
     'Usage commercial sans limite de territoire ni de durée, sur les supports convenus.', TRUE, 30),
    ('non_exclusive', 'Non exclusif',
     'Le client obtient un droit d''usage ; l''auteur peut concéder le même à d''autres.', TRUE, 40),
    ('exclusive', 'Exclusif',
     'Le client est seul à pouvoir utiliser l''œuvre. L''auteur ne la reconcède pas, et ce que cela coûte se paie.', TRUE, 50),
    ('buyout', 'Rachat total',
     'Cession complète, portfolio compris. À réserver aux cas où le client a une raison de le demander et l''a payée : l''auteur perd la possibilité de prouver qu''il a fait le travail.', FALSE, 60);

ALTER TABLE missions
    ADD COLUMN licensing_scope VARCHAR(30)
        REFERENCES mission_licensing_scopes(slug) ON UPDATE CASCADE;

COMMENT ON COLUMN missions.licensing_scope IS
    'What the client may do with the delivered work. Orthogonal to ip_terms, '
    'which says who owns it: a creator who keeps ownership still grants a '
    'licence, and the licence is where the disputes are. Required for audio '
    'missions, optional elsewhere until a domain says otherwise.';

-- Audio missions have to state it. A commissioned track with no stated scope
-- is the single most common way this goes wrong: the client assumes worldwide
-- and the composer assumes one game.
ALTER TABLE missions
    ADD CONSTRAINT missions_audio_states_its_licensing_scope CHECK (
        skill_domain <> 'audio' OR licensing_scope IS NOT NULL
    );

-- ═══════════════════════════════════════════════════════════════════
-- The seven kinds of audio commission
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO mission_types (slug, skill_domain, name, description, sort_order) VALUES
    ('audio_original_composition', 'audio', 'Composition originale',
     'Une musique écrite pour le projet du client : thème, score, jingle, générique.', 10),
    ('audio_custom_sound_pack', 'audio', 'Pack sonore sur mesure',
     'Un ensemble de bruitages ou de sons d''interface conçus pour un produit précis.', 20),
    ('audio_voice_over', 'audio', 'Voix off',
     'Narration, lecture commerciale, tutoriel. Livrée montée et prête à poser.', 30),
    ('audio_character_voice', 'audio', 'Voix de personnage',
     'Interprétation d''un ou plusieurs personnages, avec direction et prises alternatives.', 40),
    ('audio_adaptive_integration', 'audio', 'Intégration musicale adaptative',
     'Faire réagir la musique au produit : couches, transitions, budget mémoire tenu.', 50),
    ('audio_programming_feature', 'audio', 'Fonctionnalité audio en code',
     'Du DSP, de la spatialisation ou de la synthèse livrée dans le moteur du client.', 60),
    ('audio_direction', 'audio', 'Direction audio',
     'Tenir la cohérence sonore d''un projet entier : charte, arbitrages, coordination des intervenants.', 70);
