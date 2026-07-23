-- Priorite moyenne #6 strategy doc §6 : la Grande Epreuve a 3 formats
-- (Marathon cooperatif, Hackathon competitif, Defi solitaire ultime). Le
-- kind check ne connaissait que individual/guild_war/hackathon. Ajout des
-- 2 nouveaux formats :
--
--   - `marathon` (Format 1) : saisons impaires. Objectif partage, tout le
--     monde contribue, un artefact geant final. Voir tests d'ingestion +
--     effort cumule dans deliverables saisonniers.
--   - `defi_solitaire` (Format 3) : background permanent. Un objectif unique
--     tres difficile qu'un individu tente en isolement (ex: implementer un
--     compilateur, publier un package majeur, gagner un CTF).
--
-- Format 2 = hackathon existant (kind = 'hackathon'), pas de changement.

ALTER TABLE tournaments
    DROP CONSTRAINT tournaments_kind_check;

ALTER TABLE tournaments
    ADD CONSTRAINT tournaments_kind_check
    CHECK (kind IN (
        'individual',
        'guild_war',
        'hackathon',
        'marathon',
        'defi_solitaire'
    ));

COMMENT ON COLUMN tournaments.kind IS
    'Format de tournoi. individual/guild_war = legacy P8. hackathon = Format 2 Grande Epreuve (saisons paires). marathon = Format 1 (saisons impaires, cooperatif). defi_solitaire = Format 3 (background permanent).';
