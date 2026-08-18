-- The vocabulary the design trades are proved in.
--
-- ## What was there
--
-- Migration 0057 seeded eight design categories and forty-two atomic design
-- skills, all clustered around one job: product work done in Figma. That was
-- enough while `design` meant "the person who draws the screens the frontend
-- developer implements".
--
-- ## Why it is not enough
--
-- Ten of the twenty-six design trades had no skill to attach a proof to. A
-- motion 3D artist, a type designer, a sound designer and a service designer
-- could not be mapped in `orientation_skill_map`, could not have a deliverable
-- tagged through `slice_skills`, could never register proficiency in
-- `user_skills`, and could not be found by recruiter search. The trade existed
-- on the platform and nothing on the platform could say what it was made of.
--
-- ## Two levels, and no more
--
-- Category then atomic skill, like every other domain. A third level would be
-- a taxonomy nobody maintains: the useful question is "can this person do
-- this gesture", and a gesture does not have sub-gestures worth recording.
--
-- Conventions kept from 0057:
--   - Two levels only: category (parent_id IS NULL) then atomic skill.
--   - Slugs are stable kebab-case English, unique platform-wide.
--   - `display_category` is derived from `domain` by the 0116 trigger
--     ('design' -> 'create'), so it is never set explicitly here.
--   - `is_skilluv_specific = TRUE` marks skills that only matter because of
--     how Skilluv itself works (critique grids, attestation-grade handoff).
--   - ON CONFLICT (slug) DO NOTHING keeps the migration replay-safe.

-- ═══════════════════════════════════════════════════════════════════
-- Step 1 — Categories (parent_id = NULL)
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, domain, parent_id, is_skilluv_specific)
VALUES
    ('web-design-craft',          'Web and e-commerce design',            'design', NULL, FALSE),
    ('editorial-design-craft',    'Editorial and long-form web design',   'design', NULL, FALSE),
    ('mobile-design-craft',       'Mobile design (iOS, Android, wearables)', 'design', NULL, FALSE),
    ('motion-production',         'Motion production (2D, 3D, broadcast)', 'design', NULL, FALSE),
    ('video-craft',               'Video editing and post-production',     'design', NULL, FALSE),
    ('illustration-craft',        'Illustration',                          'design', NULL, FALSE),
    ('iconography-craft',         'Iconography and pictogram systems',     'design', NULL, FALSE),
    ('character-craft',           'Character design',                      'design', NULL, FALSE),
    ('dataviz-craft',             'Data visualization and infographics',   'design', NULL, FALSE),
    ('ux-writing-craft',          'UX writing and content design',         'design', NULL, FALSE),
    ('marketing-design-craft',    'Marketing and campaign design',         'design', NULL, FALSE),
    ('game-interface-craft',      'Game UI and UX',                        'design', NULL, FALSE),
    ('environment-art-craft',     'Environment art and concept art',       'design', NULL, FALSE),
    ('archviz-craft',             'Architectural and interior visualization', 'design', NULL, FALSE),
    ('spatial-design-craft',      'AR, VR and spatial design',             'design', NULL, FALSE),
    ('sound-design-craft',        'Sound design for interfaces and media', 'design', NULL, FALSE),
    ('service-design-craft',      'Service design and systems design',     'design', NULL, FALSE),
    ('design-ops-craft',          'Design ops and design governance',      'design', NULL, FALSE),
    ('typeface-design-craft',     'Typeface design',                       'design', NULL, FALSE),
    ('verbal-identity-craft',     'Naming and verbal identity',            'design', NULL, FALSE),
    ('conversational-design-craft', 'Conversational and AI product design', 'design', NULL, FALSE),
    ('design-critique-craft',     'Design critique and structured feedback', 'design', NULL, TRUE)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Step 2 — Atomic skills, parent resolved by slug
-- ═══════════════════════════════════════════════════════════════════

WITH new_skills (slug, display_name, description, domain, parent_slug, is_skilluv_specific) AS (
    VALUES
    -- ── design-foundations: gaps in the 0057 seed ───────────────────
    ('gestalt-principles',        'Gestalt principles (proximity, closure, similarity)', NULL, 'design', 'design-foundations', FALSE),
    ('figure-ground-contrast',    'Figure/ground relationships',                         NULL, 'design', 'design-foundations', FALSE),
    ('optical-alignment',         'Optical alignment and optical sizing',                NULL, 'design', 'design-foundations', FALSE),
    ('moodboard-direction',       'Moodboarding and art direction',                      NULL, 'design', 'design-foundations', FALSE),
    ('reference-research',        'Visual reference research without copying',           NULL, 'design', 'design-foundations', FALSE),
    ('design-rationale-writing',  'Writing the rationale behind a design decision',      'Explaining why a design choice was made, in terms a non-designer can evaluate. Required to defend a deliverable during Skilluv validation.', 'design', 'design-foundations', TRUE),

    -- ── web-design-craft ────────────────────────────────────────────
    ('landing-page-structure',    'Landing page structure and conversion hierarchy',     NULL, 'design', 'web-design-craft', FALSE),
    ('cta-hierarchy',             'Call-to-action hierarchy',                            NULL, 'design', 'web-design-craft', FALSE),
    ('scroll-narrative',          'Scroll-driven narrative pacing',                      NULL, 'design', 'web-design-craft', FALSE),
    ('ecommerce-product-page',    'E-commerce product page design',                      NULL, 'design', 'web-design-craft', FALSE),
    ('ecommerce-checkout-flow',   'Checkout flow design (friction and trust)',           NULL, 'design', 'web-design-craft', FALSE),
    ('cms-constrained-design',    'Designing within CMS and no-code constraints',        NULL, 'design', 'web-design-craft', FALSE),
    ('web-performance-budget',    'Designing to a performance budget (weight, fonts, media)', NULL, 'design', 'web-design-craft', FALSE),
    ('seo-aware-layout',          'SEO-aware layout and heading structure',              NULL, 'design', 'web-design-craft', FALSE),

    -- ── editorial-design-craft ──────────────────────────────────────
    ('editorial-grid-systems',    'Editorial grid systems for long-form',                NULL, 'design', 'editorial-design-craft', FALSE),
    ('reading-rhythm-typesetting', 'Typesetting for reading rhythm (measure, leading)',  NULL, 'design', 'editorial-design-craft', FALSE),
    ('article-hierarchy',         'Article hierarchy (deck, pull quotes, captions)',     NULL, 'design', 'editorial-design-craft', FALSE),
    ('longform-illustration-integration', 'Integrating illustration into long-form',     NULL, 'design', 'editorial-design-craft', FALSE),
    ('scrollytelling-composition', 'Scrollytelling composition',                         NULL, 'design', 'editorial-design-craft', FALSE),

    -- ── mobile-design-craft ─────────────────────────────────────────
    ('ios-hig-compliance',        'Apple Human Interface Guidelines compliance',         NULL, 'design', 'mobile-design-craft', FALSE),
    ('material-design-compliance', 'Material Design compliance',                         NULL, 'design', 'mobile-design-craft', FALSE),
    ('native-gesture-design',     'Native gesture design (swipe, long press, pull)',     NULL, 'design', 'mobile-design-craft', FALSE),
    ('touch-target-sizing',       'Touch target sizing and thumb reach',                 NULL, 'design', 'mobile-design-craft', FALSE),
    ('safe-area-adaptation',      'Safe areas, notches and foldables',                   NULL, 'design', 'mobile-design-craft', FALSE),
    ('offline-first-ux',          'Offline-first and low-bandwidth UX',                  'Designing screens that stay usable on intermittent 3G — a default requirement for the African markets Skilluv serves, not an edge case.', 'design', 'mobile-design-craft', TRUE),
    ('wearable-glanceable-ui',    'Glanceable UI for wearables',                         NULL, 'design', 'mobile-design-craft', FALSE),
    ('app-store-asset-design',    'App store listing and screenshot design',             NULL, 'design', 'mobile-design-craft', FALSE),

    -- ── motion-production ───────────────────────────────────────────
    ('after-effects-compositing', 'After Effects compositing',                           NULL, 'design', 'motion-production', FALSE),
    ('keyframe-timing',           'Keyframe timing and spacing',                         NULL, 'design', 'motion-production', FALSE),
    ('easing-curve-authoring',    'Authoring easing curves',                             NULL, 'design', 'motion-production', FALSE),
    ('animation-storyboarding',   'Animation storyboarding and animatics',               NULL, 'design', 'motion-production', FALSE),
    ('kinetic-typography',        'Kinetic typography',                                  NULL, 'design', 'motion-production', FALSE),
    ('lottie-export-optimization', 'Lottie export and payload optimization',             NULL, 'design', 'motion-production', FALSE),
    ('rive-state-machines',       'Rive state machines for interactive motion',          NULL, 'design', 'motion-production', FALSE),
    ('cinema4d-motion',           'Cinema 4D motion graphics',                           NULL, 'design', 'motion-production', FALSE),
    ('blender-motion',            'Blender motion graphics and geometry nodes',          NULL, 'design', 'motion-production', FALSE),
    ('3d-product-animation',      '3D product animation',                                NULL, 'design', 'motion-production', FALSE),
    ('particle-simulation',       'Particle and dynamics simulation',                    NULL, 'design', 'motion-production', FALSE),
    ('render-pipeline-motion',    'Render pipelines for motion (passes, denoise, farm)', NULL, 'design', 'motion-production', FALSE),
    ('animated-logo-systems',     'Animated logo and title systems',                     NULL, 'design', 'motion-production', FALSE),
    ('loop-seamlessness',         'Seamless loop construction',                          NULL, 'design', 'motion-production', FALSE),

    -- ── video-craft ─────────────────────────────────────────────────
    ('video-editing-narrative',   'Narrative video editing',                             NULL, 'design', 'video-craft', FALSE),
    ('color-grading',             'Color grading and LUT authoring',                     NULL, 'design', 'video-craft', FALSE),
    ('audio-sync-mixing',         'Audio sync and mixing for video',                     NULL, 'design', 'video-craft', FALSE),
    ('subtitle-caption-design',   'Subtitle and caption design',                         NULL, 'design', 'video-craft', FALSE),
    ('video-codec-delivery',      'Codec and delivery specs (bitrate, container, HDR)',  NULL, 'design', 'video-craft', FALSE),
    ('social-video-reframing',    'Reframing for vertical and social formats',           NULL, 'design', 'video-craft', FALSE),

    -- ── illustration-craft ──────────────────────────────────────────
    ('illustration-linework',     'Linework and inking',                                 NULL, 'design', 'illustration-craft', FALSE),
    ('illustration-color-rendering', 'Color rendering and light in illustration',        NULL, 'design', 'illustration-craft', FALSE),
    ('editorial-illustration',    'Editorial illustration (concept to metaphor)',        NULL, 'design', 'illustration-craft', FALSE),
    ('product-spot-illustration', 'Product spot illustration and empty states',          NULL, 'design', 'illustration-craft', FALSE),
    ('info-illustration',         'Explanatory and info illustration',                   NULL, 'design', 'illustration-craft', FALSE),
    ('illustration-style-consistency', 'Holding a style across a set',                   NULL, 'design', 'illustration-craft', FALSE),
    ('vector-illustration-workflow', 'Vector illustration workflow (paths, booleans)',   NULL, 'design', 'illustration-craft', FALSE),

    -- ── iconography-craft ───────────────────────────────────────────
    ('icon-grid-keyline',         'Icon grids and keyline shapes',                       NULL, 'design', 'iconography-craft', FALSE),
    ('icon-stroke-consistency',   'Stroke weight and terminal consistency',              NULL, 'design', 'iconography-craft', FALSE),
    ('icon-metaphor-clarity',     'Icon metaphor clarity and cultural legibility',       NULL, 'design', 'iconography-craft', FALSE),
    ('icon-optical-balance',      'Optical balance across an icon set',                  NULL, 'design', 'iconography-craft', FALSE),
    ('svg-sprite-delivery',       'SVG sprite and icon font delivery',                   NULL, 'design', 'iconography-craft', FALSE),
    ('icon-set-scaling',          'Multi-size icon sets (16 to 48 px)',                  NULL, 'design', 'iconography-craft', FALSE),

    -- ── character-craft ─────────────────────────────────────────────
    ('character-silhouette',      'Character silhouette readability',                    NULL, 'design', 'character-craft', FALSE),
    ('character-expression-sheet', 'Expression and pose sheets',                         NULL, 'design', 'character-craft', FALSE),
    ('character-turnaround',      'Character turnarounds and model sheets',              NULL, 'design', 'character-craft', FALSE),
    ('character-costume-design',  'Costume and prop design',                             NULL, 'design', 'character-craft', FALSE),
    ('character-rig-readiness',   'Designing characters that are riggable',              NULL, 'design', 'character-craft', FALSE),
    ('mascot-brand-character',    'Brand mascot design',                                 NULL, 'design', 'character-craft', FALSE),

    -- ── dataviz-craft ───────────────────────────────────────────────
    ('chart-type-selection',      'Choosing the right chart for the question',           NULL, 'design', 'dataviz-craft', FALSE),
    ('dataviz-encoding-integrity', 'Encoding integrity (no truncated axes, no lie factor)', NULL, 'design', 'dataviz-craft', FALSE),
    ('dataviz-color-scales',      'Sequential, diverging and categorical color scales',  NULL, 'design', 'dataviz-craft', FALSE),
    ('dashboard-information-density', 'Dashboard information density',                   NULL, 'design', 'dataviz-craft', FALSE),
    ('infographic-narrative',     'Infographic narrative structure',                     NULL, 'design', 'dataviz-craft', FALSE),
    ('dataviz-accessibility',     'Accessible data visualization (colorblind-safe, labels)', NULL, 'design', 'dataviz-craft', FALSE),
    ('dataviz-interaction-design', 'Interaction design for exploratory charts',          NULL, 'design', 'dataviz-craft', FALSE),

    -- ── ux-writing-craft ────────────────────────────────────────────
    ('microcopy-writing',         'Microcopy (buttons, labels, hints)',                  NULL, 'design', 'ux-writing-craft', FALSE),
    ('tone-of-voice-systems',     'Tone of voice systems',                               NULL, 'design', 'ux-writing-craft', FALSE),
    ('error-message-design',      'Error message design (cause, consequence, recovery)', NULL, 'design', 'ux-writing-craft', FALSE),
    ('empty-state-writing',       'Empty state writing',                                 NULL, 'design', 'ux-writing-craft', FALSE),
    ('onboarding-copy',           'Onboarding and first-run copy',                       NULL, 'design', 'ux-writing-craft', FALSE),
    ('i18n-copy-readiness',       'Writing copy that survives translation',              'Avoiding concatenation, idioms and fixed-width assumptions. Skilluv ships FR and EN in parallel, so every string is translated by default.', 'design', 'ux-writing-craft', TRUE),
    ('content-style-guide',       'Authoring a content style guide',                     NULL, 'design', 'ux-writing-craft', FALSE),
    ('inclusive-language',        'Inclusive and non-exclusionary language',             NULL, 'design', 'ux-writing-craft', FALSE),

    -- ── marketing-design-craft ──────────────────────────────────────
    ('display-ad-systems',        'Display ad systems across sizes',                     NULL, 'design', 'marketing-design-craft', FALSE),
    ('social-media-templates',    'Social media template systems',                       NULL, 'design', 'marketing-design-craft', FALSE),
    ('email-design-constraints',  'Email design within client constraints',              NULL, 'design', 'marketing-design-craft', FALSE),
    ('presentation-deck-design',  'Presentation and pitch deck design',                  NULL, 'design', 'marketing-design-craft', FALSE),
    ('campaign-visual-system',    'Campaign visual systems (one idea, many surfaces)',   NULL, 'design', 'marketing-design-craft', FALSE),
    ('ab-test-creative-variants', 'Designing creative variants for A/B tests',           NULL, 'design', 'marketing-design-craft', FALSE),

    -- ── game-interface-craft ────────────────────────────────────────
    ('game-hud-design',           'HUD design and readability under load',               NULL, 'design', 'game-interface-craft', FALSE),
    ('game-menu-flow',            'Game menu and settings flow',                         NULL, 'design', 'game-interface-craft', FALSE),
    ('diegetic-ui',               'Diegetic and spatial game UI',                        NULL, 'design', 'game-interface-craft', FALSE),
    ('controller-navigation-ui',  'Controller and D-pad navigation design',              NULL, 'design', 'game-interface-craft', FALSE),
    ('game-ui-engine-constraints', 'Designing for engine UI constraints (Unity, Unreal, Godot)', NULL, 'design', 'game-interface-craft', FALSE),
    ('game-feedback-juice',       'Feedback and game feel in UI',                        NULL, 'design', 'game-interface-craft', FALSE),

    -- ── environment-art-craft ───────────────────────────────────────
    ('concept-art-thumbnailing',  'Concept art thumbnailing and iteration',              NULL, 'design', 'environment-art-craft', FALSE),
    ('environment-composition',   'Environment composition and focal guidance',          NULL, 'design', 'environment-art-craft', FALSE),
    ('world-building-visual-language', 'Visual language for world building',             NULL, 'design', 'environment-art-craft', FALSE),
    ('modular-asset-kits',        'Modular environment asset kits',                      NULL, 'design', 'environment-art-craft', FALSE),
    ('polycount-budgeting',       'Polycount and texture budgeting',                     NULL, 'design', 'environment-art-craft', FALSE),
    ('pbr-texturing',             'PBR texturing workflow',                              NULL, 'design', 'environment-art-craft', FALSE),
    ('trim-sheet-authoring',      'Trim sheets and tileable materials',                  NULL, 'design', 'environment-art-craft', FALSE),

    -- ── archviz-craft ───────────────────────────────────────────────
    ('archviz-camera-framing',    'Camera framing for architectural stills',             NULL, 'design', 'archviz-craft', FALSE),
    ('archviz-lighting-daylight', 'Daylight and artificial lighting setups',             NULL, 'design', 'archviz-craft', FALSE),
    ('archviz-material-realism',  'Material realism (wear, imperfection, scale)',        NULL, 'design', 'archviz-craft', FALSE),
    ('interior-styling',          'Interior styling and set dressing',                   NULL, 'design', 'archviz-craft', FALSE),
    ('archviz-postproduction',    'Archviz post-production and compositing',             NULL, 'design', 'archviz-craft', FALSE),
    ('flythrough-animation',      'Architectural flythrough animation',                  NULL, 'design', 'archviz-craft', FALSE),

    -- ── spatial-design-craft ────────────────────────────────────────
    ('spatial-affordance-design', 'Spatial affordances and depth cues',                  NULL, 'design', 'spatial-design-craft', FALSE),
    ('xr-comfort-constraints',    'XR comfort constraints (motion sickness, FOV)',       NULL, 'design', 'spatial-design-craft', FALSE),
    ('hand-tracking-interaction', 'Hand tracking and gaze interaction design',           NULL, 'design', 'spatial-design-craft', FALSE),
    ('ar-anchoring-placement',    'AR anchoring and real-world placement',               NULL, 'design', 'spatial-design-craft', FALSE),
    ('spatial-typography',        'Typography and readability in 3D space',              NULL, 'design', 'spatial-design-craft', FALSE),
    ('xr-onboarding-safety',      'XR onboarding and safety boundaries',                 NULL, 'design', 'spatial-design-craft', FALSE),

    -- ── sound-design-craft ──────────────────────────────────────────
    ('ui-sound-design',           'UI sound design (feedback, confirmation, error)',     NULL, 'design', 'sound-design-craft', FALSE),
    ('ambience-layering',         'Ambience and atmosphere layering',                    NULL, 'design', 'sound-design-craft', FALSE),
    ('foley-recording',           'Foley recording and editing',                         NULL, 'design', 'sound-design-craft', FALSE),
    ('sonic-branding',            'Sonic branding (audio logo, signature)',              NULL, 'design', 'sound-design-craft', FALSE),
    ('audio-loudness-normalization', 'Loudness normalization and delivery targets',      NULL, 'design', 'sound-design-craft', FALSE),
    ('audio-accessibility',       'Audio accessibility (captions, non-audio-only cues)', NULL, 'design', 'sound-design-craft', FALSE),

    -- ── service-design-craft ────────────────────────────────────────
    ('service-blueprinting',      'Service blueprinting (frontstage and backstage)',     NULL, 'design', 'service-design-craft', FALSE),
    ('stakeholder-mapping',       'Stakeholder and ecosystem mapping',                   NULL, 'design', 'service-design-craft', FALSE),
    ('touchpoint-orchestration',  'Touchpoint orchestration across channels',            NULL, 'design', 'service-design-craft', FALSE),
    ('co-design-workshop',        'Facilitating a co-design workshop',                   NULL, 'design', 'service-design-craft', FALSE),
    ('systems-thinking-loops',    'Systems thinking (feedback loops, leverage points)',  NULL, 'design', 'service-design-craft', FALSE),
    ('service-prototyping',       'Service prototyping (role play, Wizard of Oz)',       NULL, 'design', 'service-design-craft', FALSE),

    -- ── design-ops-craft ────────────────────────────────────────────
    ('design-file-hygiene',       'Design file hygiene and naming conventions',          NULL, 'design', 'design-ops-craft', FALSE),
    ('design-handoff-process',    'Design to engineering handoff process',               NULL, 'design', 'design-ops-craft', FALSE),
    ('design-review-rituals',     'Design review rituals and cadence',                   NULL, 'design', 'design-ops-craft', FALSE),
    ('design-system-contribution-model', 'Design system contribution model',             NULL, 'design', 'design-ops-craft', FALSE),
    ('design-metrics-instrumentation', 'Instrumenting design decisions with metrics',    NULL, 'design', 'design-ops-craft', FALSE),
    ('design-asset-licensing',    'Asset licensing and font compliance',                 'Knowing what a font EULA or a stock licence actually permits. Skilluv deliverables are public artifacts, so an unlicensed asset is a legal liability on the contributor.', 'design', 'design-ops-craft', TRUE),

    -- ── typeface-design-craft ───────────────────────────────────────
    ('letterform-construction',   'Letterform construction and skeleton',                NULL, 'design', 'typeface-design-craft', FALSE),
    ('typeface-spacing-kerning',  'Spacing and kerning a typeface',                      NULL, 'design', 'typeface-design-craft', FALSE),
    ('type-hinting-production',   'Hinting and font production (OTF, WOFF2)',            NULL, 'design', 'typeface-design-craft', FALSE),
    ('variable-font-axes',        'Variable font axes design',                           NULL, 'design', 'typeface-design-craft', FALSE),
    ('multiscript-type-design',   'Multi-script type design (Latin, Arabic, N''Ko, Tifinagh)', 'Designing coherent type across the scripts actually used across Africa, not Latin-only. Directly serves the Skilluv contributor base.', 'design', 'typeface-design-craft', TRUE),
    ('opentype-features',         'OpenType feature authoring (ligatures, alternates)',  NULL, 'design', 'typeface-design-craft', FALSE),

    -- ── verbal-identity-craft ───────────────────────────────────────
    ('naming-generation',         'Naming generation and shortlisting',                  NULL, 'design', 'verbal-identity-craft', FALSE),
    ('naming-trademark-screening', 'Trademark and domain pre-screening',                 NULL, 'design', 'verbal-identity-craft', FALSE),
    ('brand-narrative-writing',   'Brand narrative and manifesto writing',               NULL, 'design', 'verbal-identity-craft', FALSE),
    ('tagline-craft',             'Tagline and claim craft',                             NULL, 'design', 'verbal-identity-craft', FALSE),
    ('verbal-identity-guidelines', 'Verbal identity guidelines',                         NULL, 'design', 'verbal-identity-craft', FALSE),

    -- ── conversational-design-craft ─────────────────────────────────
    ('conversation-flow-design',  'Conversation flow design (happy path and repair)',    NULL, 'design', 'conversational-design-craft', FALSE),
    ('persona-voice-definition',  'Assistant persona and voice definition',              NULL, 'design', 'conversational-design-craft', FALSE),
    ('prompt-ux-patterns',        'Prompt UX patterns (affordances, suggestions)',       NULL, 'design', 'conversational-design-craft', FALSE),
    ('ai-uncertainty-disclosure', 'Designing for model uncertainty and failure',         NULL, 'design', 'conversational-design-craft', FALSE),
    ('voice-interface-design',    'Voice interface design (barge-in, confirmation)',     NULL, 'design', 'conversational-design-craft', FALSE),
    ('ai-consent-and-transparency', 'Consent and transparency surfaces for AI features', NULL, 'design', 'conversational-design-craft', FALSE),

    -- ── design-critique-craft (Skilluv-specific) ────────────────────
    ('structured-critique',       'Giving structured critique against a grid',           'Filling a Skilluv critique grid: criterion by criterion, scored, with a concrete change suggested for each. Replaces free-form "it looks off".', 'design', 'design-critique-craft', TRUE),
    ('receiving-critique',        'Receiving critique and turning it into iterations',   NULL, 'design', 'design-critique-craft', TRUE),
    ('brief-interrogation',       'Interrogating a brief before starting',               'Surfacing the missing constraints, the unstated audience and the real success criterion — the single highest-leverage habit against endless iteration rounds.', 'design', 'design-critique-craft', TRUE),
    ('iteration-storytelling',    'Telling the story of an iteration sequence',          'Presenting round 1 to round N as a reasoning trail. This is what turns a Skilluv design deliverable into a portfolio piece rather than a file.', 'design', 'design-critique-craft', TRUE),
    ('plagiarism-awareness',      'Distinguishing reference, homage and plagiarism',     NULL, 'design', 'design-critique-craft', TRUE)
)
INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id, is_skilluv_specific)
SELECT
    ns.slug,
    ns.display_name,
    ns.description,
    ns.domain,
    parent.id,
    ns.is_skilluv_specific
FROM new_skills ns
JOIN skill_nodes parent ON parent.slug = ns.parent_slug AND parent.parent_id IS NULL
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Post-conditions (informational — not executed)
-- ═══════════════════════════════════════════════════════════════════
--   SELECT COUNT(*) FROM skill_nodes
--    WHERE domain = 'design' AND parent_id IS NULL;      -- 30 categories
--   SELECT COUNT(*) FROM skill_nodes
--    WHERE domain = 'design' AND parent_id IS NOT NULL;  -- 194 atomic skills
