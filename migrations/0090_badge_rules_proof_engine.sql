-- Phase P17.1 — Refonte badges vers "Proof Engine".
-- Migration 0090.
--
-- Rationale :
--   Le système `badges`+`user_badges` (mig 0005) est un enum étroit
--   (category ∈ {streak, challenge, fragment, special}, condition_type +
--   condition_value INT). C'est de la gamification générique type Duolingo —
--   pas Skilluv.
--
--   La spec UX BMAD (`badge-system-design.md`, `HANDOFF-backend-proof-engine.md`)
--   propose une architecture 2 couches :
--
--     COUCHE 1 : proofs (deliverables + attestations = déjà en DB, immuables)
--                     ▼
--     COUCHE 2 : badges (vues dérivées via BadgeRule JSONB)
--
--   Avec en sortie 6 familles de badges (skill_patch, rank chevron, guild_crest,
--   challenge_seal, event_stamp, medal) et 4 raretés (common → legendary)
--   dérivées du nombre de preuves accumulées.
--
--   Cette migration met en place l'infrastructure. Les rules et l'engine
--   suivent en P17.3, le rank system en P17.4, l'API en P17.5.
--
--   `badges` table historique reste temporairement (source du seed legacy),
--   sera supprimée en P17.6 après migration complète des 9 badges vers rules.

-- ═══════════════════════════════════════════════════════════════════
-- badge_rules : la source de vérité des badges (data-driven, editable)
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE badge_rules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(80) NOT NULL UNIQUE
        CHECK (slug ~ '^[a-z0-9_-]+$' AND length(slug) BETWEEN 3 AND 80),
    output_type VARCHAR(20) NOT NULL
        CHECK (output_type IN (
            'skill_patch',      -- rond brodé, "je sais faire X"
            'rank',             -- chevron V (Apprenti→Doyen), 1 par user
            'guild_crest',      -- écusson de guilde
            'challenge_seal',   -- sceau octogonal "j'ai validé challenge X"
            'event_stamp',      -- tampon hexagonal Hacktoberfest/Skilluv Fest
            'medal'             -- médaille rare (First Blood, mentor of year)
        )),
    -- Variant permet de distinguer plusieurs rules du même type
    -- (ex: 'first_blood' vs 'community_pillar' sont deux medals distinctes).
    output_variant VARCHAR(50),
    -- Contenu affichable côté frontend (name, description, icon_key).
    display_name VARCHAR(120) NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    icon_key VARCHAR(50),
    -- Conditions évaluées par le rules engine (P17.3). Structure libre :
    -- { "proof_types": ["deliverable_verified"], "min_count": 5,
    --   "skill_tag": "react", "verified_by": ["peer","mentor"],
    --   "within_days": 30 }
    conditions JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- Rareté fixe (medals) ou dérivée par l'engine (skill_patches).
    -- 'auto' = l'engine calcule à partir de min_count/thresholds.
    rarity VARCHAR(15) NOT NULL DEFAULT 'auto'
        CHECK (rarity IN ('auto', 'common', 'rare', 'epic', 'legendary')),
    -- Rules éditables via admin dashboard vs hardcodées (rank thresholds).
    admin_editable BOOLEAN NOT NULL DEFAULT TRUE,
    -- Deprecation soft : la rule cesse de produire de nouveaux user_badges
    -- mais les anciens restent (traçabilité historique).
    deprecated_at TIMESTAMPTZ,
    -- Métadonnées d'affichage (couleur override, tags UX, etc.).
    ui_metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_badge_rules_active
    ON badge_rules (output_type, deprecated_at NULLS FIRST);

CREATE INDEX idx_badge_rules_conditions_gin
    ON badge_rules USING gin (conditions);

-- ═══════════════════════════════════════════════════════════════════
-- user_badges : refactor pour aligner sur le proof engine
-- ═══════════════════════════════════════════════════════════════════
-- L'ancienne table (mig 0005) est conservée pour rétro-compat ; on ajoute
-- les colonnes proof-engine sans casser le schéma existant :
--   - rule_id : lien vers badge_rules (nullable pour rétro-compat legacy)
--   - source_proofs : UUID[] des deliverables/attestations qui ont déclenché
--   - rarity : rareté effective au moment de l'award (peut différer de la
--     rule si celle-ci a évolué depuis)
--   - revoked_at : révocation en cascade quand une preuve source disparaît

ALTER TABLE user_badges
    ADD COLUMN IF NOT EXISTS rule_id UUID REFERENCES badge_rules(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS source_proofs UUID[] NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS rarity VARCHAR(15) NOT NULL DEFAULT 'common'
        CHECK (rarity IN ('common', 'rare', 'epic', 'legendary')),
    ADD COLUMN IF NOT EXISTS revoked_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS revoked_reason TEXT;

-- L'index dashboard user (mig 0005) reste valide.
-- On ajoute deux index utiles :

CREATE INDEX IF NOT EXISTS idx_user_badges_by_rule
    ON user_badges (rule_id, revoked_at NULLS FIRST);

CREATE INDEX IF NOT EXISTS idx_user_badges_source_proofs_gin
    ON user_badges USING gin (source_proofs);

-- ═══════════════════════════════════════════════════════════════════
-- SEED — Migration des 9 badges legacy vers badge_rules "legacy_*",
-- marquées deprecated (elles ne produiront plus rien après P17.3 mais
-- l'API GET /users/{id}/badges les affichera si présentes en historique).
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO badge_rules (slug, output_type, output_variant, display_name,
                          description, icon_key, conditions, rarity,
                          admin_editable, deprecated_at) VALUES
-- Compteurs de challenges — anti-vision Skilluv (compte les actions, pas les preuves).
('legacy_first_challenge', 'medal', 'first_blood', 'First Blood',
    'Legacy: your first completed challenge (deprecated — absorbed into rank system).',
    'trophy',
    '{"proof_types":["challenge_completed"],"min_count":1}'::jsonb,
    'common', FALSE, NOW()),
('legacy_challenges_10', 'medal', NULL, '10 Challenges',
    'Legacy count-based (deprecated).', 'sword',
    '{"proof_types":["challenge_completed"],"min_count":10}'::jsonb,
    'common', FALSE, NOW()),
('legacy_challenges_50', 'medal', NULL, '50 Challenges',
    'Legacy count-based (deprecated).', 'colosseum',
    '{"proof_types":["challenge_completed"],"min_count":50}'::jsonb,
    'rare', FALSE, NOW()),

-- Streaks — valorisent la connexion, pas la production. Retirés.
('legacy_streak_7', 'medal', NULL, '7-day Streak',
    'Legacy streak-based (deprecated — Skilluv values artifacts, not logins).',
    'flame', '{"proof_types":["login_streak"],"min_count":7}'::jsonb,
    'common', FALSE, NOW()),
('legacy_streak_30', 'medal', NULL, '30-day Streak',
    'Legacy streak-based (deprecated).', 'fire',
    '{"proof_types":["login_streak"],"min_count":30}'::jsonb,
    'rare', FALSE, NOW()),
('legacy_streak_100', 'medal', NULL, '100-day Streak',
    'Legacy streak-based (deprecated).', 'shield',
    '{"proof_types":["login_streak"],"min_count":100}'::jsonb,
    'epic', FALSE, NOW()),

-- Fragments — redondants avec le futur rank system (P17.4).
('legacy_fragments_100', 'medal', NULL, '100 Fragments',
    'Legacy fragments count (deprecated — absorbed into Apprenti rank).',
    'gem', '{"proof_types":["fragments_earned"],"min_count":100}'::jsonb,
    'common', FALSE, NOW()),
('legacy_fragments_500', 'medal', NULL, '500 Fragments',
    'Legacy fragments count (deprecated — absorbed into Ranger rank).',
    'diamond', '{"proof_types":["fragments_earned"],"min_count":500}'::jsonb,
    'rare', FALSE, NOW()),
('legacy_fragments_2000', 'medal', NULL, '2000 Fragments',
    'Legacy fragments count (deprecated — absorbed into Artisan rank).',
    'crown', '{"proof_types":["fragments_earned"],"min_count":2000}'::jsonb,
    'epic', FALSE, NOW());
