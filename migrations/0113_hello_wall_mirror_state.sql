-- Priorite haute #2 strategy doc §15 : mirror job Hello Wall vers repo GitHub.
--
-- La migration 0109 modelait deja hello_wall_entries mais le champ
-- github_entry_url etait insere avec une URL "cible" (le fichier a l'URL
-- n'existait pas encore sur le repo skilluv-community/hello-wall — voir
-- commentaire "placeholder URL until then" dans routes/onboarding.rs).
--
-- Cette migration ajoute :
-- - `mirrored_at` : timestamp de succes du push GitHub. NULL = pas encore mirror.
-- - `mirror_error` : derniere erreur GitHub (pour observabilite / retry).
--
-- Le service `hello_wall_mirror` peut alors query WHERE mirrored_at IS NULL
-- pour la queue de retry.

ALTER TABLE hello_wall_entries
    ADD COLUMN mirrored_at TIMESTAMPTZ,
    ADD COLUMN mirror_error TEXT,
    ADD COLUMN mirror_attempt_count INTEGER NOT NULL DEFAULT 0;

-- Index pour la selection de la queue de mirroring : "pas encore mirror,
-- pas d'erreur permanente au-dela de 5 tentatives".
CREATE INDEX idx_hello_wall_entries_mirror_queue
    ON hello_wall_entries (archived_at ASC)
    WHERE deleted_at IS NULL AND mirrored_at IS NULL AND mirror_attempt_count < 5;

COMMENT ON COLUMN hello_wall_entries.mirrored_at IS
    'Set to NOW() by hello_wall_mirror service after a successful GitHub PUT to skilluv-community/hello-wall/entries/{username}.md. NULL until then.';

COMMENT ON COLUMN hello_wall_entries.mirror_error IS
    'Last error string from GitHub API PUT (e.g. rate limit, network error). Reset to NULL on success. Used for observabilite.';

COMMENT ON COLUMN hello_wall_entries.mirror_attempt_count IS
    'Nombre de tentatives de mirror (succes + echecs). Le service arrete de retenter au-dela de 5 pour eviter les boucles infinies sur erreurs permanentes.';
