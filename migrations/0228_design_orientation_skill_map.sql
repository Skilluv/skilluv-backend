-- Which skills each design trade is made of.
--
-- ## What `is_core` decides
--
-- A core skill is one the trade does not exist without: a type designer who
-- cannot space a typeface is not a type designer. It is what promotes a
-- declared orientation from `learning` to `active` once the person has proved
-- one, so marking too many things core makes the promotion unreachable and
-- marking too few makes it meaningless.
--
-- ## Why the critique vocabulary is on every trade
--
-- On Skilluv a deliverable is not finished when it looks right; it is
-- finished when it has been defended in front of somebody and iterated on.
-- Reading a critique, answering it, and telling the story of the rounds are
-- part of every design trade here, so they are attached to all twenty-six
-- rather than to a "soft skills" trade nobody would pick.
--
-- ## Why a temporary table
--
-- A mistyped slug in a JOIN drops the relation and says nothing. Loading the
-- pairs first and checking them against `skill_nodes` and `orientations`
-- turns that silence into a failed migration, which is the only moment
-- anybody would notice.

CREATE TEMP TABLE tmp_design_skill_map (
    orientation_slug TEXT NOT NULL,
    skill_slug TEXT NOT NULL,
    is_core BOOLEAN NOT NULL,
    weight REAL NOT NULL
) ON COMMIT DROP;

INSERT INTO tmp_design_skill_map (orientation_slug, skill_slug, is_core, weight) VALUES
-- ── design-product ──────────────────────────────────────────────────
('design-product', 'user-flow-mapping',            TRUE,  3.0),
('design-product', 'wireframing',                  TRUE,  3.0),
('design-product', 'information-architecture',     TRUE,  2.5),
('design-product', 'visual-hierarchy',             TRUE,  3.0),
('design-product', 'figma-auto-layout',            TRUE,  2.5),
('design-product', 'figma-prototyping',            TRUE,  2.5),
('design-product', 'usability-heuristics',         TRUE,  2.5),
('design-product', 'usability-testing',            FALSE, 2.0),
('design-product', 'user-research-basics',         FALSE, 2.0),
('design-product', 'persona-development',          FALSE, 1.5),
('design-product', 'journey-mapping',              FALSE, 1.5),
('design-product', 'responsive-components',        FALSE, 2.0),
('design-product', 'a11y-color-contrast',          FALSE, 2.0),
('design-product', 'a11y-focus-states',            FALSE, 1.5),
('design-product', 'micro-interactions',           FALSE, 1.5),
('design-product', 'figma-dev-mode-handoff',       FALSE, 2.0),
('design-product', 'design-rationale-writing',     FALSE, 2.0),
('design-product', 'brief-interrogation',          FALSE, 1.5),

-- ── design-system ───────────────────────────────────────────────────
('design-system', 'design-system-thinking',        TRUE,  3.0),
('design-system', 'design-tokens',                 TRUE,  3.0),
('design-system', 'component-variants',            TRUE,  2.5),
('design-system', 'component-composition',         TRUE,  2.5),
('design-system', 'color-system-design',           TRUE,  2.5),
('design-system', 'design-system-governance',      TRUE,  2.5),
('design-system', 'figma-variables-tokens',        FALSE, 2.5),
('design-system', 'figma-libraries-publishing',    FALSE, 2.0),
('design-system', 'figma-variants',                FALSE, 2.0),
('design-system', 'design-tokens-multi-brand',     FALSE, 2.0),
('design-system', 'dark-mode-design',              FALSE, 2.0),
('design-system', 'responsive-components',         FALSE, 2.0),
('design-system', 'design-system-contribution-model', FALSE, 2.0),
('design-system', 'figma-dev-mode-handoff',        FALSE, 2.0),
('design-system', 'penpot-tokens',                 FALSE, 1.0),
('design-system', 'a11y-color-contrast',           FALSE, 1.5),

-- ── design-ai-conversational ────────────────────────────────────────
('design-ai-conversational', 'conversation-flow-design',      TRUE,  3.0),
('design-ai-conversational', 'persona-voice-definition',      TRUE,  2.5),
('design-ai-conversational', 'ai-uncertainty-disclosure',     TRUE,  2.5),
('design-ai-conversational', 'prompt-ux-patterns',            TRUE,  2.5),
('design-ai-conversational', 'ai-consent-and-transparency',   TRUE,  2.5),
('design-ai-conversational', 'voice-interface-design',        FALSE, 2.0),
('design-ai-conversational', 'microcopy-writing',             FALSE, 2.0),
('design-ai-conversational', 'tone-of-voice-systems',         FALSE, 2.0),
('design-ai-conversational', 'error-message-design',          FALSE, 2.0),
('design-ai-conversational', 'user-flow-mapping',             FALSE, 1.5),
('design-ai-conversational', 'usability-testing',             FALSE, 1.5),

-- ── design-web ──────────────────────────────────────────────────────
('design-web', 'landing-page-structure',           TRUE,  3.0),
('design-web', 'cta-hierarchy',                    TRUE,  2.5),
('design-web', 'visual-hierarchy',                 TRUE,  2.5),
('design-web', 'composition-grids',                TRUE,  2.5),
('design-web', 'responsive-components',            TRUE,  2.5),
('design-web', 'ecommerce-product-page',           FALSE, 2.0),
('design-web', 'ecommerce-checkout-flow',          FALSE, 2.0),
('design-web', 'scroll-narrative',                 FALSE, 2.0),
('design-web', 'cms-constrained-design',           FALSE, 2.0),
('design-web', 'web-performance-budget',           FALSE, 2.0),
('design-web', 'seo-aware-layout',                 FALSE, 1.5),
('design-web', 'typography-pairing',               FALSE, 2.0),
('design-web', 'a11y-color-contrast',              FALSE, 2.0),
('design-web', 'figma-prototyping',                FALSE, 1.5),

-- ── design-editorial-web ────────────────────────────────────────────
('design-editorial-web', 'editorial-grid-systems',            TRUE,  3.0),
('design-editorial-web', 'reading-rhythm-typesetting',        TRUE,  3.0),
('design-editorial-web', 'article-hierarchy',                 TRUE,  2.5),
('design-editorial-web', 'typography-basics',                 TRUE,  2.5),
('design-editorial-web', 'typography-pairing',                TRUE,  2.5),
('design-editorial-web', 'scrollytelling-composition',        FALSE, 2.0),
('design-editorial-web', 'longform-illustration-integration', FALSE, 2.0),
('design-editorial-web', 'whitespace-usage',                  FALSE, 2.0),
('design-editorial-web', 'web-performance-budget',            FALSE, 1.5),
('design-editorial-web', 'a11y-screen-reader-design',         FALSE, 1.5),

-- ── design-mobile ───────────────────────────────────────────────────
('design-mobile', 'ios-hig-compliance',            TRUE,  2.5),
('design-mobile', 'material-design-compliance',    TRUE,  2.5),
('design-mobile', 'native-gesture-design',         TRUE,  2.5),
('design-mobile', 'touch-target-sizing',           TRUE,  2.5),
('design-mobile', 'responsive-components',         TRUE,  2.0),
('design-mobile', 'safe-area-adaptation',          FALSE, 2.0),
('design-mobile', 'offline-first-ux',              FALSE, 2.5),
('design-mobile', 'wearable-glanceable-ui',        FALSE, 1.0),
('design-mobile', 'app-store-asset-design',        FALSE, 1.0),
('design-mobile', 'micro-interactions',            FALSE, 2.0),
('design-mobile', 'figma-prototyping',             FALSE, 2.0),
('design-mobile', 'a11y-focus-states',             FALSE, 1.5),

-- ── design-motion-ui ────────────────────────────────────────────────
('design-motion-ui', 'motion-principles',          TRUE,  3.0),
('design-motion-ui', 'easing-curve-authoring',     TRUE,  3.0),
('design-motion-ui', 'micro-interactions',         TRUE,  2.5),
('design-motion-ui', 'keyframe-timing',            TRUE,  2.5),
('design-motion-ui', 'lottie-export-optimization', TRUE,  2.5),
('design-motion-ui', 'rive-state-machines',        FALSE, 2.5),
('design-motion-ui', 'prototyping-motion',         FALSE, 2.0),
('design-motion-ui', 'animated-logo-systems',      FALSE, 2.0),
('design-motion-ui', 'loop-seamlessness',          FALSE, 1.5),
('design-motion-ui', 'a11y-motion-reduce',         FALSE, 2.5),
('design-motion-ui', 'interaction-affordance',     FALSE, 1.5),

-- ── design-motion-2d ────────────────────────────────────────────────
('design-motion-2d', 'after-effects-compositing',  TRUE,  3.0),
('design-motion-2d', 'keyframe-timing',            TRUE,  3.0),
('design-motion-2d', 'easing-curve-authoring',     TRUE,  2.5),
('design-motion-2d', 'animation-storyboarding',    TRUE,  2.5),
('design-motion-2d', 'motion-principles',          TRUE,  2.5),
('design-motion-2d', 'kinetic-typography',         FALSE, 2.0),
('design-motion-2d', 'loop-seamlessness',          FALSE, 1.5),
('design-motion-2d', 'audio-sync-mixing',          FALSE, 1.5),
('design-motion-2d', 'illustration-linework',      FALSE, 1.5),
('design-motion-2d', 'video-codec-delivery',       FALSE, 1.5),

-- ── design-motion-3d ────────────────────────────────────────────────
('design-motion-3d', 'blender-motion',             TRUE,  3.0),
('design-motion-3d', 'cinema4d-motion',            TRUE,  2.5),
('design-motion-3d', 'keyframe-timing',            TRUE,  2.5),
('design-motion-3d', '3d-product-animation',       TRUE,  2.5),
('design-motion-3d', 'render-pipeline-motion',     TRUE,  2.5),
('design-motion-3d', 'particle-simulation',        FALSE, 2.0),
('design-motion-3d', 'pbr-texturing',              FALSE, 2.0),
('design-motion-3d', 'animation-storyboarding',    FALSE, 1.5),
('design-motion-3d', 'motion-principles',          FALSE, 2.0),
('design-motion-3d', 'color-grading',              FALSE, 1.5),

-- ── design-video ────────────────────────────────────────────────────
('design-video', 'video-editing-narrative',        TRUE,  3.0),
('design-video', 'color-grading',                  TRUE,  2.5),
('design-video', 'audio-sync-mixing',              TRUE,  2.5),
('design-video', 'video-codec-delivery',           TRUE,  2.5),
('design-video', 'subtitle-caption-design',        FALSE, 2.0),
('design-video', 'social-video-reframing',         FALSE, 2.0),
('design-video', 'animation-storyboarding',        FALSE, 1.5),
('design-video', 'after-effects-compositing',      FALSE, 2.0),
('design-video', 'audio-loudness-normalization',   FALSE, 1.5),
('design-video', 'audio-accessibility',            FALSE, 1.5),

-- ── design-brand-identity ───────────────────────────────────────────
('design-brand-identity', 'brand-identity-basics',   TRUE,  3.0),
('design-brand-identity', 'logo-design-basics',      TRUE,  3.0),
('design-brand-identity', 'typography-pairing',      TRUE,  2.5),
('design-brand-identity', 'color-theory',            TRUE,  2.5),
('design-brand-identity', 'moodboard-direction',     TRUE,  2.5),
('design-brand-identity', 'brand-tokens-application', FALSE, 2.0),
('design-brand-identity', 'optical-alignment',       FALSE, 2.0),
('design-brand-identity', 'brand-narrative-writing',  FALSE, 2.0),
('design-brand-identity', 'reference-research',      FALSE, 1.5),
('design-brand-identity', 'design-asset-licensing',  FALSE, 1.5),
('design-brand-identity', 'design-rationale-writing', FALSE, 2.0),

-- ── design-typography ───────────────────────────────────────────────
('design-typography', 'letterform-construction',    TRUE,  3.0),
('design-typography', 'typeface-spacing-kerning',   TRUE,  3.0),
('design-typography', 'type-hinting-production',    TRUE,  2.5),
('design-typography', 'opentype-features',          TRUE,  2.5),
('design-typography', 'typography-basics',          TRUE,  2.5),
('design-typography', 'variable-font-axes',         FALSE, 2.0),
('design-typography', 'multiscript-type-design',    FALSE, 2.5),
('design-typography', 'optical-alignment',          FALSE, 2.0),
('design-typography', 'design-asset-licensing',     FALSE, 1.5),

-- ── design-naming-verbal ────────────────────────────────────────────
('design-naming-verbal', 'naming-generation',           TRUE,  3.0),
('design-naming-verbal', 'brand-narrative-writing',     TRUE,  2.5),
('design-naming-verbal', 'tone-of-voice-systems',       TRUE,  2.5),
('design-naming-verbal', 'tagline-craft',               TRUE,  2.5),
('design-naming-verbal', 'verbal-identity-guidelines',  TRUE,  2.5),
('design-naming-verbal', 'naming-trademark-screening',  FALSE, 2.0),
('design-naming-verbal', 'inclusive-language',          FALSE, 1.5),
('design-naming-verbal', 'i18n-copy-readiness',         FALSE, 2.0),
('design-naming-verbal', 'brand-identity-basics',       FALSE, 1.5),

-- ── design-illustration ─────────────────────────────────────────────
('design-illustration', 'illustration-linework',              TRUE,  3.0),
('design-illustration', 'illustration-color-rendering',       TRUE,  3.0),
('design-illustration', 'illustration-style-consistency',     TRUE,  2.5),
('design-illustration', 'composition-grids',                  TRUE,  2.0),
('design-illustration', 'editorial-illustration',             FALSE, 2.5),
('design-illustration', 'product-spot-illustration',          FALSE, 2.5),
('design-illustration', 'info-illustration',                  FALSE, 2.0),
('design-illustration', 'vector-illustration-workflow',       FALSE, 2.0),
('design-illustration', 'color-theory',                       FALSE, 2.0),
('design-illustration', 'plagiarism-awareness',               FALSE, 2.0),
('design-illustration', 'design-asset-licensing',             FALSE, 1.5),

-- ── design-iconography ──────────────────────────────────────────────
('design-iconography', 'icon-grid-keyline',        TRUE,  3.0),
('design-iconography', 'icon-stroke-consistency',  TRUE,  3.0),
('design-iconography', 'icon-metaphor-clarity',    TRUE,  2.5),
('design-iconography', 'icon-optical-balance',     TRUE,  2.5),
('design-iconography', 'icon-set-scaling',         TRUE,  2.5),
('design-iconography', 'svg-sprite-delivery',      FALSE, 2.0),
('design-iconography', 'vector-illustration-workflow', FALSE, 2.0),
('design-iconography', 'optical-alignment',        FALSE, 2.0),
('design-iconography', 'design-tokens',            FALSE, 1.0),

-- ── design-character ────────────────────────────────────────────────
('design-character', 'character-silhouette',        TRUE,  3.0),
('design-character', 'character-expression-sheet',  TRUE,  2.5),
('design-character', 'character-turnaround',        TRUE,  2.5),
('design-character', 'illustration-linework',       TRUE,  2.0),
('design-character', 'character-costume-design',    FALSE, 2.0),
('design-character', 'character-rig-readiness',     FALSE, 2.0),
('design-character', 'mascot-brand-character',      FALSE, 2.0),
('design-character', 'illustration-color-rendering', FALSE, 2.0),
('design-character', 'world-building-visual-language', FALSE, 1.5),

-- ── design-dataviz ──────────────────────────────────────────────────
('design-dataviz', 'chart-type-selection',             TRUE,  3.0),
('design-dataviz', 'dataviz-encoding-integrity',       TRUE,  3.0),
('design-dataviz', 'dataviz-color-scales',             TRUE,  2.5),
('design-dataviz', 'dashboard-information-density',    TRUE,  2.5),
('design-dataviz', 'dataviz-accessibility',            TRUE,  2.5),
('design-dataviz', 'infographic-narrative',            FALSE, 2.0),
('design-dataviz', 'dataviz-interaction-design',       FALSE, 2.0),
('design-dataviz', 'visual-hierarchy',                 FALSE, 2.0),
('design-dataviz', 'a11y-color-contrast',              FALSE, 2.0),
('design-dataviz', 'design-rationale-writing',         FALSE, 1.5),

-- ── design-ux-writing ───────────────────────────────────────────────
('design-ux-writing', 'microcopy-writing',         TRUE,  3.0),
('design-ux-writing', 'error-message-design',      TRUE,  3.0),
('design-ux-writing', 'empty-state-writing',       TRUE,  2.5),
('design-ux-writing', 'tone-of-voice-systems',     TRUE,  2.5),
('design-ux-writing', 'i18n-copy-readiness',       TRUE,  2.5),
('design-ux-writing', 'onboarding-copy',           FALSE, 2.0),
('design-ux-writing', 'content-style-guide',       FALSE, 2.0),
('design-ux-writing', 'inclusive-language',        FALSE, 2.0),
('design-ux-writing', 'information-architecture',  FALSE, 1.5),
('design-ux-writing', 'usability-testing',         FALSE, 1.5),

-- ── design-marketing ────────────────────────────────────────────────
('design-marketing', 'campaign-visual-system',     TRUE,  3.0),
('design-marketing', 'display-ad-systems',         TRUE,  2.5),
('design-marketing', 'social-media-templates',     TRUE,  2.5),
('design-marketing', 'cta-hierarchy',              TRUE,  2.5),
('design-marketing', 'email-design-constraints',   FALSE, 2.0),
('design-marketing', 'presentation-deck-design',   FALSE, 2.0),
('design-marketing', 'ab-test-creative-variants',  FALSE, 2.0),
('design-marketing', 'brand-tokens-application',   FALSE, 2.0),
('design-marketing', 'typography-pairing',         FALSE, 1.5),
('design-marketing', 'tagline-craft',              FALSE, 1.5),

-- ── design-game-ui ──────────────────────────────────────────────────
('design-game-ui', 'game-hud-design',              TRUE,  3.0),
('design-game-ui', 'game-menu-flow',               TRUE,  2.5),
('design-game-ui', 'controller-navigation-ui',     TRUE,  2.5),
('design-game-ui', 'game-ui-engine-constraints',   TRUE,  2.5),
('design-game-ui', 'diegetic-ui',                  FALSE, 2.0),
('design-game-ui', 'game-feedback-juice',          FALSE, 2.0),
('design-game-ui', 'visual-hierarchy',             FALSE, 2.0),
('design-game-ui', 'icon-metaphor-clarity',        FALSE, 1.5),
('design-game-ui', 'micro-interactions',           FALSE, 1.5),
('design-game-ui', 'a11y-color-contrast',          FALSE, 1.5),

-- ── design-game-environment ─────────────────────────────────────────
('design-game-environment', 'concept-art-thumbnailing',        TRUE,  3.0),
('design-game-environment', 'environment-composition',         TRUE,  3.0),
('design-game-environment', 'modular-asset-kits',              TRUE,  2.5),
('design-game-environment', 'polycount-budgeting',             TRUE,  2.5),
('design-game-environment', 'pbr-texturing',                   TRUE,  2.5),
('design-game-environment', 'world-building-visual-language',  FALSE, 2.0),
('design-game-environment', 'trim-sheet-authoring',            FALSE, 2.0),
('design-game-environment', 'illustration-color-rendering',    FALSE, 1.5),
('design-game-environment', 'reference-research',              FALSE, 1.5),

-- ── design-arch-interior-viz ────────────────────────────────────────
('design-arch-interior-viz', 'archviz-camera-framing',      TRUE,  3.0),
('design-arch-interior-viz', 'archviz-lighting-daylight',   TRUE,  3.0),
('design-arch-interior-viz', 'archviz-material-realism',    TRUE,  2.5),
('design-arch-interior-viz', 'archviz-postproduction',      TRUE,  2.5),
('design-arch-interior-viz', 'interior-styling',            FALSE, 2.0),
('design-arch-interior-viz', 'flythrough-animation',        FALSE, 2.0),
('design-arch-interior-viz', 'pbr-texturing',               FALSE, 2.0),
('design-arch-interior-viz', 'color-grading',               FALSE, 1.5),
('design-arch-interior-viz', 'composition-grids',           FALSE, 1.5),

-- ── design-ar-vr-spatial ────────────────────────────────────────────
('design-ar-vr-spatial', 'spatial-affordance-design',   TRUE,  3.0),
('design-ar-vr-spatial', 'xr-comfort-constraints',      TRUE,  3.0),
('design-ar-vr-spatial', 'hand-tracking-interaction',   TRUE,  2.5),
('design-ar-vr-spatial', 'ar-anchoring-placement',      TRUE,  2.5),
('design-ar-vr-spatial', 'xr-onboarding-safety',        TRUE,  2.5),
('design-ar-vr-spatial', 'spatial-typography',          FALSE, 2.0),
('design-ar-vr-spatial', 'ui-sound-design',             FALSE, 1.5),
('design-ar-vr-spatial', 'interaction-affordance',      FALSE, 2.0),
('design-ar-vr-spatial', 'motion-principles',           FALSE, 1.5),

-- ── design-sound ────────────────────────────────────────────────────
('design-sound', 'ui-sound-design',                    TRUE,  3.0),
('design-sound', 'ambience-layering',                  TRUE,  2.5),
('design-sound', 'sonic-branding',                     TRUE,  2.5),
('design-sound', 'audio-loudness-normalization',       TRUE,  2.5),
('design-sound', 'foley-recording',                    FALSE, 2.0),
('design-sound', 'audio-accessibility',                FALSE, 2.0),
('design-sound', 'audio-sync-mixing',                  FALSE, 2.0),
('design-sound', 'micro-interactions',                 FALSE, 1.5),

-- ── design-service ──────────────────────────────────────────────────
('design-service', 'service-blueprinting',         TRUE,  3.0),
('design-service', 'stakeholder-mapping',          TRUE,  2.5),
('design-service', 'touchpoint-orchestration',     TRUE,  2.5),
('design-service', 'systems-thinking-loops',       TRUE,  2.5),
('design-service', 'journey-mapping',              TRUE,  2.5),
('design-service', 'co-design-workshop',           FALSE, 2.0),
('design-service', 'service-prototyping',          FALSE, 2.0),
('design-service', 'user-research-basics',         FALSE, 2.0),
('design-service', 'persona-development',          FALSE, 1.5),

-- ── design-ops ──────────────────────────────────────────────────────
('design-ops', 'design-file-hygiene',                  TRUE,  3.0),
('design-ops', 'design-handoff-process',               TRUE,  3.0),
('design-ops', 'design-review-rituals',                TRUE,  2.5),
('design-ops', 'design-system-contribution-model',     TRUE,  2.5),
('design-ops', 'design-metrics-instrumentation',       FALSE, 2.0),
('design-ops', 'design-asset-licensing',               FALSE, 2.0),
('design-ops', 'design-system-governance',             FALSE, 2.0),
('design-ops', 'structured-critique',                  FALSE, 2.0),
('design-ops', 'figma-libraries-publishing',           FALSE, 1.5);

-- Every design trade also gets the critique vocabulary.
INSERT INTO tmp_design_skill_map (orientation_slug, skill_slug, is_core, weight)
SELECT DISTINCT m.orientation_slug, s.skill_slug, FALSE, 1.0
FROM tmp_design_skill_map m
CROSS JOIN (VALUES ('structured-critique'), ('receiving-critique'), ('iteration-storytelling')) AS s(skill_slug)
WHERE NOT EXISTS (
    SELECT 1 FROM tmp_design_skill_map x
    WHERE x.orientation_slug = m.orientation_slug AND x.skill_slug = s.skill_slug
);

-- Fail loudly on a mistyped slug instead of silently dropping the relation.
DO $$
DECLARE
    unknown_skills TEXT;
    unknown_orientations TEXT;
BEGIN
    SELECT string_agg(DISTINCT m.skill_slug, ', ') INTO unknown_skills
    FROM tmp_design_skill_map m
    WHERE NOT EXISTS (SELECT 1 FROM skill_nodes n WHERE n.slug = m.skill_slug);

    IF unknown_skills IS NOT NULL THEN
        RAISE EXCEPTION 'unknown skill slugs in design skill map: %', unknown_skills;
    END IF;

    SELECT string_agg(DISTINCT m.orientation_slug, ', ') INTO unknown_orientations
    FROM tmp_design_skill_map m
    WHERE NOT EXISTS (SELECT 1 FROM orientations o WHERE o.slug = m.orientation_slug);

    IF unknown_orientations IS NOT NULL THEN
        RAISE EXCEPTION 'unknown orientation slugs in design skill map: %', unknown_orientations;
    END IF;
END $$;

INSERT INTO orientation_skill_map (orientation_id, skill_id, is_core, is_recommended, weight)
SELECT o.id, n.id, m.is_core, NOT m.is_core, m.weight
FROM tmp_design_skill_map m
JOIN orientations o ON o.slug = m.orientation_slug
JOIN skill_nodes n ON n.slug = m.skill_slug
ON CONFLICT (orientation_id, skill_id) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Post-conditions (informational — not executed)
-- ═══════════════════════════════════════════════════════════════════
--   SELECT COUNT(*) FROM orientation_skill_map m
--     JOIN orientations o ON o.id = m.orientation_id
--    WHERE o.primary_domain = 'design';                          -- 350
