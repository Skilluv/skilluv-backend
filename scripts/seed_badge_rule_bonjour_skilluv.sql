-- Seed la badge_rule Bonjour Skilluv.
-- Prerequis : migration 0111 appliquee + badge_engine extension proof_type
-- `onboarding_bonjour_completed` (voir src/services/badge_engine.rs).
--
-- Trigger : 1 ligne dans onboarding_bonjour_skilluv avec completed_at NOT NULL.
-- L'INSERT est fait par le webhook GitHub quand l'user merge sa premiere PR
-- sur son fork starter-*.
--
-- Idempotent : ON CONFLICT (slug) DO NOTHING.

INSERT INTO badge_rules (
    slug, output_type, output_variant, display_name, description,
    icon_key, conditions, rarity, admin_editable, ui_metadata
) VALUES (
    'bonjour_skilluv',
    'medal',
    'onboarding',
    'Bonjour Skilluv',
    'Marque la premiere contribution mergee sur Skilluv : ton HELLO.md publie via une PR sur ton fork starter-*. Le rite d''entree du compagnonnage.',
    'hand-wave',
    '{"proof_types": ["onboarding_bonjour_completed"], "min_count": 1}'::jsonb,
    'common',
    false,
    '{"color": "sunrise", "tone": "warm", "unlock_message": "Bienvenue dans la communaute Skilluv."}'::jsonb
)
ON CONFLICT (slug) DO NOTHING;

-- Verif
SELECT slug, display_name, conditions, rarity FROM badge_rules WHERE slug = 'bonjour_skilluv';
