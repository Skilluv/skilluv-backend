-- Phase P3 — DAG des prérequis + capstones.
-- Migration 0066 : table `challenge_prerequisites` + colonne `is_capstone` sur challenges.
--
-- Rationale (voir docs/challenges-target-model-and-roadmap.md sections B.10 et Q7) :
--   - `challenge_prerequisites` remplace la logique flou `prerequisite_fragments`
--     (un seuil numérique de fragments) par un vrai graphe de dépendance :
--     "pour débloquer challenge X, tu dois avoir complété challenge Y". Le
--     graphe reste petit (quelques centaines de challenges max), la détection
--     de cycle se fait côté application.
--   - `is_capstone` distingue les challenges de graduation d'une phase
--     (bootstrap → katas → contribs → impact) des templates classiques.
--     Décision Q7 : `challenges` post-P8 = onboarding + capstones uniquement.

CREATE TABLE challenge_prerequisites (
    challenge_id UUID NOT NULL REFERENCES challenges(id) ON DELETE CASCADE,
    depends_on_challenge_id UUID NOT NULL REFERENCES challenges(id) ON DELETE CASCADE,
    -- false = recommandé (le user peut skip), true = obligatoire (bloque le start)
    required BOOLEAN NOT NULL DEFAULT TRUE,

    PRIMARY KEY (challenge_id, depends_on_challenge_id),

    -- Empêche `challenge_id → challenge_id` (self-reference)
    CONSTRAINT challenge_prerequisites_no_self CHECK (challenge_id != depends_on_challenge_id)
);

-- Retrouver les prérequis d'un challenge donné (lookup depuis /start)
CREATE INDEX idx_challenge_prereqs_lookup
    ON challenge_prerequisites (challenge_id);

-- Retrouver ce qu'un challenge débloque (utile pour tracks + suggestion "next")
CREATE INDEX idx_challenge_prereqs_reverse
    ON challenge_prerequisites (depends_on_challenge_id);

-- Extension de challenges pour distinguer capstones vs training vs project-linked
ALTER TABLE challenges
    ADD COLUMN is_capstone BOOLEAN NOT NULL DEFAULT FALSE;

CREATE INDEX idx_challenges_capstone
    ON challenges (is_capstone)
    WHERE is_capstone = TRUE;

-- Note : la détection de cycle dans le DAG est faite côté application (Rust).
-- Les migrations ne le contrôlent pas car un cycle sur un graphe orienté
-- est un problème algorithmique, pas une contrainte relationnelle.
--
-- Le service TracksService (Phase P3) fournit `check_dag_would_create_cycle`
-- avant chaque INSERT dans cette table.
