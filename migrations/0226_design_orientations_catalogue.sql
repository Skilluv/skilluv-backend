-- The twenty-six design trades.
--
-- ## What was there
--
-- Five design orientations, seeded in 0088 from the point of view of a code
-- platform: `web-designer`, `mobile-designer`, `motion-designer`,
-- `illustrator`, `3d-artist`. They describe the designer as the person who
-- hands screens to a developer. A type designer, a sound designer, a service
-- designer and a motion 3D artist had nowhere to say what they do, so the
-- platform could not find them, could not brief them, and could not route
-- their work to somebody able to judge it.
--
-- ## Why twenty-six and not eight
--
-- The public catalogue opens with the eight code-adjacent trades and adds the
-- rest quarter by quarter. That is a communication decision; the database is
-- not where it belongs. A designer whose trade exists only in a roadmap
-- cannot be onboarded when they arrive, and the cost of holding the other
-- eighteen rows is one INSERT.
--
-- ## Why some of them look like other domains
--
-- `design-game-ui` and `design-game-environment` sit in `design`, not `game`,
-- with `game` as a secondary domain. The job is art direction under an engine
-- constraint; the engine is the constraint, not the trade. Same reasoning for
-- `design-dataviz`, which is tagged `ai` because the data comes from there
-- and the reader is often an analyst.
--
-- ## The old five
--
-- Archived with `replaced_by`, never deleted: `user_orientations` references
-- them by id, and the lineage is what lets a search on the new slug still
-- reach a profile carrying the old one.

INSERT INTO orientations (slug, name, description, primary_domain, secondary_domains, tags, is_curated) VALUES

-- ── Produit et système ──────────────────────────────────────────────
('design-product', 'Designer Produit / UI / UX',
 'Recherche, parcours, wireframes, interface, prototype, test d''utilisabilité. Le métier bout-en-bout, du problème à l''écran qui le résout.',
 'design', ARRAY['code','soft_skills'], ARRAY['product','ui','ux'], TRUE),

('design-system', 'Designer de Design System',
 'Tokens, composants, variantes, documentation et la gouvernance qui garde un système vivant quand plusieurs équipes y touchent.',
 'design', ARRAY['code'], ARRAY['product','tokens','system'], TRUE),

('design-ai-conversational', 'Designer IA / Conversationnel',
 'Chat, voix et surfaces agentiques : flux de conversation, incertitude du modèle, consentement et transparence.',
 'design', ARRAY['ai'], ARRAY['product','ai','voice'], TRUE),

-- ── Web ─────────────────────────────────────────────────────────────
('design-web', 'Designer Web',
 'Sites, pages d''atterrissage et e-commerce : hiérarchie de conversion, narration au défilement, contraintes CMS et no-code.',
 'design', ARRAY['code'], ARRAY['web','ecommerce'], TRUE),

('design-editorial-web', 'Designer Web Éditorial',
 'Magazines, formats longs et scrollytelling : grilles éditoriales, rythme de lecture, intégration de l''illustration.',
 'design', ARRAY['code'], ARRAY['web','editorial'], TRUE),

-- ── Mobile ──────────────────────────────────────────────────────────
('design-mobile', 'Designer Mobile',
 'iOS, Android, cross-platform et montres : gestes natifs, lignes directrices des plateformes, usage hors ligne et bas débit.',
 'design', ARRAY['code'], ARRAY['mobile'], TRUE),

-- ── Motion et vidéo ─────────────────────────────────────────────────
('design-motion-ui', 'Designer Motion UI',
 'Motion d''interface, transitions et logos animés, livrés en Lottie ou Rive avec un budget de performance.',
 'design', ARRAY['code'], ARRAY['motion','animation','ui'], TRUE),

('design-motion-2d', 'Designer Motion 2D',
 'Animation 2D narrative et typographie cinétique, de l''animatique au compositing final.',
 'design', ARRAY[]::TEXT[], ARRAY['motion','animation','2d'], TRUE),

('design-motion-3d', 'Designer Motion 3D',
 'Motion Cinema 4D et Blender, animation produit 3D, simulation et chaînes de rendu.',
 'design', ARRAY[]::TEXT[], ARRAY['motion','3d','animation'], TRUE),

('design-video', 'Monteur / Designer Vidéo',
 'Montage narratif, étalonnage, mixage et spécifications de livraison, du broadcast au format vertical.',
 'design', ARRAY[]::TEXT[], ARRAY['motion','video'], TRUE),

-- ── Marque ──────────────────────────────────────────────────────────
('design-brand-identity', 'Designer d''Identité de Marque',
 'Transformer un positionnement en système visuel : logotype, palette, typographie, applications et guidelines.',
 'design', ARRAY['soft_skills'], ARRAY['brand','identity'], TRUE),

('design-typography', 'Créateur de Caractères',
 'Dessin de police : construction des lettres, approche et crénage, hinting, axes variables, couverture multi-écritures.',
 'design', ARRAY[]::TEXT[], ARRAY['brand','typography'], TRUE),

('design-naming-verbal', 'Designer Naming & Identité Verbale',
 'Naming, ton de voix, récit de marque et les guidelines qui gardent une marque reconnaissable à l''oreille.',
 'design', ARRAY['soft_skills'], ARRAY['brand','naming','writing'], TRUE),

-- ── Image ───────────────────────────────────────────────────────────
('design-illustration', 'Illustrateur',
 'Illustration éditoriale, produit et explicative, tenue d''un style sur toute une série.',
 'design', ARRAY[]::TEXT[], ARRAY['illustration','art'], TRUE),

('design-iconography', 'Designer d''Iconographie',
 'Systèmes d''icônes : grilles et formes de référence, régularité des traits, clarté des métaphores, livraison multi-tailles.',
 'design', ARRAY['code'], ARRAY['illustration','icons'], TRUE),

('design-character', 'Character Designer',
 'Personnages pour le jeu, la marque et le récit : silhouette, planches d''expressions, tournettes, aptitude au rig.',
 'design', ARRAY['game'], ARRAY['illustration','character'], TRUE),

-- ── Données et mots ─────────────────────────────────────────────────
('design-dataviz', 'Designer Data Visualisation',
 'Graphiques, tableaux de bord et infographies qui tiennent l''intégrité de l''encodage et restent lisibles sous la densité.',
 'design', ARRAY['ai','code'], ARRAY['dataviz','analytics'], TRUE),

('design-ux-writing', 'UX Writer / Content Designer',
 'Microcopie, messages d''erreur, états vides, ton de voix et textes qui survivent à la traduction.',
 'design', ARRAY['soft_skills'], ARRAY['writing','content'], TRUE),

('design-marketing', 'Designer Marketing',
 'Systèmes de campagne sur display, social, email et présentations : une idée, plusieurs surfaces, des variantes testables.',
 'design', ARRAY['soft_skills'], ARRAY['marketing','campaign'], TRUE),

-- ── Jeu ─────────────────────────────────────────────────────────────
('design-game-ui', 'Designer UI / UX de Jeu',
 'HUD, menus et interfaces diégétiques pensés pour les contraintes du moteur et la navigation à la manette.',
 'design', ARRAY['game'], ARRAY['game','ui'], TRUE),

('design-game-environment', 'Concept Artist / Environnement de Jeu',
 'Concept art, décors et assets 3D dans les budgets de polygones et de textures du moteur.',
 'design', ARRAY['game'], ARRAY['game','3d','concept-art'], TRUE),

-- ── 3D et espace ────────────────────────────────────────────────────
('design-arch-interior-viz', 'Visualisateur Architecture / Intérieur',
 'Images fixes et travellings d''architecture, photoréalistes ou stylisés : cadrage, lumière, matériaux, post-production.',
 'design', ARRAY[]::TEXT[], ARRAY['3d','archviz'], TRUE),

('design-ar-vr-spatial', 'Designer AR / VR / Spatial',
 'Affordances spatiales, interaction à la main et au regard, confort XR et limites de sécurité à l''entrée.',
 'design', ARRAY['game','code'], ARRAY['immersive','xr','3d'], TRUE),

('design-sound', 'Sound Designer',
 'Sons d''interface, ambiances, bruitage et identité sonore, livrés aux cibles de niveau sonore.',
 'design', ARRAY['game'], ARRAY['immersive','audio'], TRUE),

-- ── Système et process ──────────────────────────────────────────────
('design-service', 'Designer de Service',
 'Blueprints, cartographie des parties prenantes, orchestration des points de contact et co-conception de services complets.',
 'design', ARRAY['soft_skills'], ARRAY['service','systems'], TRUE),

('design-ops', 'Praticien Design Ops',
 'Hygiène des fichiers, passage au développement, rituels de revue, modèle de contribution et les mesures qui rendent le design redevable.',
 'design', ARRAY['ops','soft_skills'], ARRAY['service','process'], TRUE)

ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- English
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO orientation_translations (orientation_id, locale, name, description)
SELECT o.id, 'en', t.name, t.description
FROM (VALUES
    ('design-product', 'Product / UI / UX Designer',
     'Research, flows, wireframes, interface, prototype, usability testing. The trade end to end, from the problem to the screen that answers it.'),
    ('design-system', 'Design System Designer',
     'Tokens, components, variants, documentation and the governance that keeps a system alive once several teams touch it.'),
    ('design-ai-conversational', 'AI / Conversational Designer',
     'Chat, voice and agentic surfaces: conversation flows, model uncertainty, consent and transparency.'),
    ('design-web', 'Web Designer',
     'Sites, landing pages and e-commerce: conversion hierarchy, scroll narrative, CMS and no-code constraints.'),
    ('design-editorial-web', 'Editorial Web Designer',
     'Magazines, long-form and scrollytelling: editorial grids, reading rhythm, illustration integration.'),
    ('design-mobile', 'Mobile Designer',
     'iOS, Android, cross-platform and wearables: native gestures, platform guidelines, offline and low-bandwidth use.'),
    ('design-motion-ui', 'Motion UI Designer',
     'Interface motion, transitions and animated logos, delivered as Lottie or Rive within a performance budget.'),
    ('design-motion-2d', 'Motion 2D Designer',
     'Narrative 2D animation and kinetic typography, from animatic to final compositing.'),
    ('design-motion-3d', 'Motion 3D Designer',
     'Cinema 4D and Blender motion, 3D product animation, simulation and render pipelines.'),
    ('design-video', 'Video Editor / Designer',
     'Narrative editing, colour grading, mix and delivery specs, from broadcast to vertical.'),
    ('design-brand-identity', 'Brand Identity Designer',
     'Turning a positioning into a visual system: logotype, palette, typography, applications and guidelines.'),
    ('design-typography', 'Type Designer',
     'Typeface design: letterform construction, spacing and kerning, hinting, variable axes, multi-script coverage.'),
    ('design-naming-verbal', 'Naming & Verbal Identity Designer',
     'Naming, tone of voice, brand narrative and the guidelines that keep a brand recognisable by ear.'),
    ('design-illustration', 'Illustrator',
     'Editorial, product and explanatory illustration, holding one style across a whole set.'),
    ('design-iconography', 'Iconography Designer',
     'Icon systems: keyline grids, stroke consistency, metaphor clarity, multi-size delivery.'),
    ('design-character', 'Character Designer',
     'Characters for games, brands and narrative: silhouette, expression sheets, turnarounds, rig readiness.'),
    ('design-dataviz', 'Data Visualization Designer',
     'Charts, dashboards and infographics that hold encoding integrity and stay readable under density.'),
    ('design-ux-writing', 'UX Writer / Content Designer',
     'Microcopy, error messages, empty states, tone of voice and copy that survives translation.'),
    ('design-marketing', 'Marketing Designer',
     'Campaign systems across display, social, email and decks: one idea, several surfaces, testable variants.'),
    ('design-game-ui', 'Game UI / UX Designer',
     'HUDs, menus and diegetic interfaces built for engine constraints and controller navigation.'),
    ('design-game-environment', 'Concept Artist / Game Environment',
     'Concept art, environments and 3D assets within the engine''s polygon and texture budgets.'),
    ('design-arch-interior-viz', 'Architectural / Interior Visualizer',
     'Architectural stills and flythroughs, photoreal or stylized: framing, lighting, materials, post-production.'),
    ('design-ar-vr-spatial', 'AR / VR / Spatial Designer',
     'Spatial affordances, hand and gaze interaction, XR comfort and safety boundaries on entry.'),
    ('design-sound', 'Sound Designer',
     'Interface sound, ambience, foley and sonic identity, delivered to loudness targets.'),
    ('design-service', 'Service Designer',
     'Blueprints, stakeholder mapping, touchpoint orchestration and co-design of end-to-end services.'),
    ('design-ops', 'Design Ops Practitioner',
     'File hygiene, engineering handoff, review rituals, contribution model and the measures that keep design accountable.')
) AS t(slug, name, description)
JOIN orientations o ON o.slug = t.slug
ON CONFLICT (orientation_id, locale) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- The old slugs, and where their people went
-- ═══════════════════════════════════════════════════════════════════
--
-- Archived rather than renamed. A rename would have been tidier in the
-- catalogue and wrong in the history: `3d-artist` was not `design-game-
-- environment` under another name — it was a wider, vaguer trade, and a
-- profile carrying it said something the new slug does not. `replaced_by`
-- forwards the search without rewriting what somebody declared.

UPDATE orientations AS old
   SET is_archived = TRUE,
       replaced_by = new.id,
       updated_at = NOW()
  FROM (VALUES
    ('web-designer',     'design-web'),
    ('mobile-designer',  'design-mobile'),
    ('motion-designer',  'design-motion-ui'),
    ('illustrator',      'design-illustration'),
    ('3d-artist',        'design-game-environment')
  ) AS lineage(old_slug, new_slug)
  JOIN orientations AS new ON new.slug = lineage.new_slug
 WHERE old.slug = lineage.old_slug;

-- ═══════════════════════════════════════════════════════════════════
-- Post-conditions (informational — not executed)
-- ═══════════════════════════════════════════════════════════════════
--   SELECT COUNT(*) FROM orientations
--    WHERE primary_domain = 'design' AND is_archived = FALSE;   -- 26
--   SELECT COUNT(*) FROM orientations
--    WHERE primary_domain = 'design' AND replaced_by IS NOT NULL; -- 5
