-- Seed Saison 2 "Wax" + 10 deliverables.
-- Theme : Wax = design system africain OSS. Contributions design + code
-- avec un accent sur le flagship wax-icons + integration dans les partenaires.
-- Difficulte progresse : la S2 s'adresse a des users deja passes par la S1.
-- Idempotent : ON CONFLICT sur slug / title.

-- 1. Saison 2
INSERT INTO seasons (slug, name, theme, description, starts_at, ends_at, status)
VALUES (
    'saison-2-wax',
    'Saison 2 — Wax',
    'Wax',
    'La saison du design system africain. On construit Wax : une bibliotheque d''icones, tokens, patterns et composants inspires des textiles wax d''Afrique de l''Ouest, poussee dans les projets OSS partenaires. Les cohortes S1 reviennent avec 6 mois d''experience — la difficulte moyenne monte d''un cran.',
    '2027-07-01 00:00:00+00',
    '2027-12-31 23:59:59+00',
    'upcoming'
)
ON CONFLICT (slug) DO NOTHING;

-- 2. Focus projets Saison 2 : le flagship wax-icons + les partenaires ou
-- l'integration design a le plus de sens (Excalidraw, Cal.com, sqlx docs).
INSERT INTO project_seasons (project_id, season_id, focus_type)
SELECT p.id, s.id, 'primary'
FROM projects p CROSS JOIN seasons s
WHERE s.slug = 'saison-2-wax'
  AND p.slug IN ('wax-icons', 'excalidraw', 'calcom', 'hello-africa', 'rust-i18n', 'directus')
ON CONFLICT DO NOTHING;

INSERT INTO project_seasons (project_id, season_id, focus_type)
SELECT p.id, s.id, 'featured'
FROM projects p CROSS JOIN seasons s
WHERE s.slug = 'saison-2-wax'
  AND p.slug IN ('sqlx', 'meilisearch', 'coolify', 'flutter')
ON CONFLICT DO NOTHING;

-- 3. Les 10 deliverables Saison 2
WITH admin_user AS (SELECT id FROM users WHERE email = 'admin@skilluv.local'),
projects_map AS (SELECT slug, id FROM projects)

INSERT INTO challenge_templates (
    title, description, instructions,
    title_i18n, description_i18n, instructions_i18n,
    skill_domain, difficulty, mode, tone,
    reward_fragments, is_onboarding, status,
    project_id, ai_policy, is_capstone, created_by
)
SELECT
    v.title, v.description, v.instructions,
    v.title_i18n, v.description_i18n, v.instructions_i18n,
    v.skill_domain, v.difficulty, v.mode, v.tone,
    v.reward_fragments, false, 'published',
    pm.id, 'disclosure_required', false, au.id
FROM admin_user au
CROSS JOIN (VALUES
    -- 1. Wax color palette — design tokens
    (
        'Publier une palette de couleurs wax en design tokens',
        'Extraire les couleurs dominantes d''un motif wax de ton pays et les publier en tokens JSON conformes au W3C Design Tokens Community Group format.',
        E'1. Choisis un motif wax local (photo ref obligatoire, credit auteur si photographe pro)\n2. Extrais 5-8 couleurs dominantes (Coolors ou Adobe Color)\n3. Formatte en tokens W3C (`{ "value": "#XXXXXX", "type": "color" }`) avec metadata culturelle\n4. PR sur wax-icons dans `tokens/palettes/`\n5. Ajoute un README bilingue expliquant la signification culturelle des couleurs.',
        '{"fr":"Publier une palette de couleurs wax en design tokens","en":"Publish a wax color palette as design tokens"}'::jsonb,
        '{"fr":"Extraire les couleurs dominantes d''un motif wax et les publier en tokens W3C.","en":"Extract dominant colors from a wax pattern and publish as W3C design tokens."}'::jsonb,
        '{"fr":"1. Choisis un motif (photo ref). 2. Extrais 5-8 couleurs. 3. Format tokens W3C. 4. PR + README culturel bilingue.","en":"1. Pick a pattern (photo ref). 2. Extract 5-8 colors. 3. W3C token format. 4. PR + bilingual cultural README."}'::jsonb,
        'design', 2, 'solo', 'serious',
        30, 'wax-icons'
    ),
    -- 2. Wax animated icon
    (
        'Ajouter une icone wax animee (Lottie ou SVG SMIL)',
        'Prendre une icone wax existante et lui ajouter une animation subtile (rotation, pulsation, morph) publiable dans le repo wax-icons.',
        E'1. Choisis une icone wax deja mergee dans le repo\n2. Cree une variante animee (Lottie JSON ou SVG avec SMIL, pas de dependance JS)\n3. Duree < 3s, en boucle, pas de flash > 3Hz (a11y epilepsy)\n4. PR avec la version animee cote-a-cote de la statique\n5. Documente l''animation dans le README (intention, contexte d''usage).',
        '{"fr":"Ajouter une icone wax animee (Lottie ou SVG SMIL)","en":"Add an animated wax icon (Lottie or SVG SMIL)"}'::jsonb,
        '{"fr":"Prendre une icone wax existante et lui ajouter une animation subtile.","en":"Take an existing wax icon and add a subtle animation."}'::jsonb,
        '{"fr":"1. Choisis une icone. 2. Anime (Lottie/SMIL, pas de JS). 3. <3s en boucle, respect a11y. 4. PR cote-a-cote. 5. Documente l''intention.","en":"1. Pick an icon. 2. Animate (Lottie/SMIL, no JS). 3. <3s loop, a11y compliant. 4. PR side-by-side. 5. Document intent."}'::jsonb,
        'design', 3, 'solo', 'fun',
        40, 'wax-icons'
    ),
    -- 3. Wax dans Excalidraw shape library
    (
        'Contribuer un pack de formes wax a la Excalidraw libraries',
        'Excalidraw permet d''importer des shape libraries communautaires. Package les icones wax en `.excalidrawlib` et publie dans le repo Excalidraw officiel.',
        E'1. Selectionne 10 icones wax finalisees\n2. Convertit en format Excalidraw (voir doc `/dev/adding-shapes.md`)\n3. Cree le fichier `.excalidrawlib` avec metadata (auteur = "Skilluv Community", licence, cultural context)\n4. PR sur excalidraw/excalidraw-libraries\n5. Rattache ici l''URL de la PR + capture de la biblio importee dans Excalidraw local.',
        '{"fr":"Contribuer un pack de formes wax a la Excalidraw libraries","en":"Contribute a wax shape pack to Excalidraw libraries"}'::jsonb,
        '{"fr":"Package les icones wax en .excalidrawlib et publie dans le repo officiel.","en":"Package wax icons as .excalidrawlib and publish in the official repo."}'::jsonb,
        '{"fr":"1. Selectionne 10 icones. 2. Convertit en format Excalidraw. 3. .excalidrawlib avec metadata. 4. PR sur excalidraw-libraries. 5. Rattache PR + capture.","en":"1. Pick 10 icons. 2. Convert to Excalidraw format. 3. .excalidrawlib with metadata. 4. PR to excalidraw-libraries. 5. Link PR + screenshot."}'::jsonb,
        'design', 3, 'solo', 'serious',
        45, 'excalidraw'
    ),
    -- 4. Wax pattern generator (algo art)
    (
        'Coder un generateur de motifs wax procedural (canvas ou SVG)',
        'Ecrire une petite lib TypeScript qui genere des motifs wax procedurals a partir de parametres (palette, symetrie, densite). Base pour futurs backgrounds et hero images sur skilluv.io.',
        E'1. Etudie 3 familles de motifs wax (geometrique, floral, symbolique) et abstrait les invariants\n2. Implemente en TS avec Canvas API ou SVG (au choix), zero dependance\n3. API : `generateWaxPattern({ palette, seed, size, family })` -> Blob | string\n4. Exemples visuels dans un README avec 6 patterns generes\n5. Tests unitaires (determinisme via seed, contraintes de palette).',
        '{"fr":"Coder un generateur de motifs wax procedural","en":"Code a procedural wax pattern generator"}'::jsonb,
        '{"fr":"Petite lib TS qui genere des motifs wax procedurals a partir de parametres.","en":"Small TS lib that generates procedural wax patterns from parameters."}'::jsonb,
        '{"fr":"1. Etudie 3 familles. 2. TS + Canvas/SVG, zero dep. 3. API deterministe (seed). 4. README 6 exemples. 5. Tests unitaires.","en":"1. Study 3 families. 2. TS + Canvas/SVG, zero-dep. 3. Deterministic API (seed). 4. README with 6 examples. 5. Unit tests."}'::jsonb,
        'code', 4, 'solo', 'fun',
        60, 'wax-icons'
    ),
    -- 5. Cal.com theme (wax)
    (
        'Publier un theme wax pour Cal.com',
        'Cal.com supporte les themes user. Package les tokens wax + une palette en theme installable, PR upstream sur cal.com/themes.',
        E'1. Fork cal.com\n2. Explore l''architecture themes (`packages/ui/themes`)\n3. Cree `themes/wax-official.ts` en utilisant les tokens wax mergeed\n4. Screenshots avant/apres sur le booking flow\n5. PR upstream + rattache ici + demande le tag `community-theme`.',
        '{"fr":"Publier un theme wax pour Cal.com","en":"Publish a wax theme for Cal.com"}'::jsonb,
        '{"fr":"Package les tokens wax en theme installable sur Cal.com.","en":"Package wax tokens as an installable Cal.com theme."}'::jsonb,
        '{"fr":"1. Fork cal.com. 2. Structure themes. 3. wax-official.ts a partir des tokens. 4. Captures booking. 5. PR upstream + tag community-theme.","en":"1. Fork cal.com. 2. Themes structure. 3. wax-official.ts from tokens. 4. Booking screenshots. 5. Upstream PR + community-theme tag."}'::jsonb,
        'design', 3, 'solo', 'serious',
        50, 'calcom'
    ),
    -- 6. rust-i18n — full locale for wax naming
    (
        'Nommer toutes les icones wax dans une langue africaine complete',
        'Certaines icones wax ont un nom FR/EN. Ajouter la traduction complete dans une langue africaine que TU parles (wolof, lingala, bambara, ewe, yoruba, swahili, amharique...) + tests rust-i18n.',
        E'1. Recupere la liste actuelle des icones wax nommees (JSON dans le repo)\n2. Ajoute la locale (wo/ln/bm/ee/yo/sw/am/...) en respectant la syntaxe rust-i18n\n3. Prononciation IPA optionnelle dans un fichier `locales/{lang}/README.md`\n4. Tests rust-i18n : toutes les cles doivent avoir la traduction\n5. PR + validation par 1 native speaker de la communaute (comment sur la PR).',
        '{"fr":"Nommer toutes les icones wax dans une langue africaine complete","en":"Name every wax icon in one complete African language"}'::jsonb,
        '{"fr":"Ajouter la traduction complete dans une langue africaine que tu parles.","en":"Add the complete translation in an African language you speak natively."}'::jsonb,
        '{"fr":"1. Recupere liste actuelle. 2. Ajoute locale (syntaxe rust-i18n). 3. IPA optionnel. 4. Tests. 5. PR + validation native speaker.","en":"1. Get the list. 2. Add locale (rust-i18n syntax). 3. Optional IPA. 4. Tests. 5. PR + native-speaker validation."}'::jsonb,
        'code', 3, 'solo', 'educational',
        50, 'rust-i18n'
    ),
    -- 7. Directus — wax collection template
    (
        'Publier un content model wax pour Directus',
        'Creer un content model Directus (collections + fields + interfaces) qui expose wax-icons comme headless CMS pour d''autres projets. Import direct via Directus schema-snapshot.',
        E'1. Modelise wax-icons en collections Directus (icon, palette, pattern, cultural_note)\n2. Configure les interfaces (upload SVG, color picker, i18n text)\n3. Export via `directus schema snapshot` -> YAML\n4. PR sur directus/directus dans `contrib/schema-templates/`\n5. Documente l''usage + un screenshot du panneau admin.',
        '{"fr":"Publier un content model wax pour Directus","en":"Publish a wax content model for Directus"}'::jsonb,
        '{"fr":"Content model Directus qui expose wax-icons comme headless CMS.","en":"Directus content model exposing wax-icons as a headless CMS."}'::jsonb,
        '{"fr":"1. Modelise collections. 2. Configure interfaces. 3. Schema snapshot YAML. 4. PR sur contrib/schema-templates. 5. Doc + screenshot.","en":"1. Model collections. 2. Configure interfaces. 3. Schema snapshot YAML. 4. PR to contrib/schema-templates. 5. Docs + screenshot."}'::jsonb,
        'code', 4, 'solo', 'serious',
        55, 'directus'
    ),
    -- 8. Accessibility audit wax-icons
    (
        'Auditer l''accessibilite complete de wax-icons',
        'Audit systematique de la biblio wax-icons : contrast ratios (WCAG AA/AAA), noms accessibles, RTL support, dark mode. Rapport public + PRs correctives.',
        E'1. Setup un notebook Storybook local du repo wax-icons\n2. Run axe-core sur chaque page\n3. Verifie contrast ratios avec Stark ou WebAIM Contrast Checker (viser AAA)\n4. Rapport public en Markdown : quelles icones passent WCAG AA/AAA, lesquelles echouent, pourquoi\n5. Ouvre 3-5 PRs correctives selon les priorites du rapport.',
        '{"fr":"Auditer l''accessibilite complete de wax-icons","en":"Audit accessibility of wax-icons"}'::jsonb,
        '{"fr":"Audit WCAG systematique + rapport public + PRs correctives.","en":"Systematic WCAG audit + public report + corrective PRs."}'::jsonb,
        '{"fr":"1. Storybook local. 2. axe-core par page. 3. Contrast checker (AAA). 4. Rapport Markdown public. 5. 3-5 PRs correctives.","en":"1. Local Storybook. 2. axe-core per page. 3. Contrast checker (AAA). 4. Public Markdown report. 5. 3-5 corrective PRs."}'::jsonb,
        'design', 4, 'solo', 'serious',
        60, 'wax-icons'
    ),
    -- 9. sqlx docs illustration
    (
        'Illustrer la doc sqlx avec un motif wax en background subtil',
        'La doc sqlx (mdBook) beneficierait d''un touch design. Proposer une PR qui ajoute un motif wax en background CSS discret + credits + option opt-out.',
        E'1. Fork sqlx et lance la doc en local (`mdbook serve`)\n2. Cree une CSS surcharge qui ajoute un motif wax genere procedurallement en background body (opacity max 0.05)\n3. Respect prefers-reduced-motion + prefers-color-scheme (mode dark ok)\n4. Toggle utilisateur pour desactiver (localStorage)\n5. PR avec captures avant/apres + credit design system Skilluv Wax.',
        '{"fr":"Illustrer la doc sqlx avec un motif wax en background subtil","en":"Illustrate sqlx docs with a subtle wax background pattern"}'::jsonb,
        '{"fr":"PR CSS qui ajoute un motif wax discret + toggle utilisateur.","en":"CSS PR adding a subtle wax pattern + user toggle."}'::jsonb,
        '{"fr":"1. mdbook serve. 2. CSS surcharge opacity <=0.05. 3. Respect prefs-reduced-motion + dark mode. 4. Toggle localStorage. 5. PR avec captures.","en":"1. mdbook serve. 2. CSS overlay opacity <=0.05. 3. Respect prefers-reduced-motion + dark mode. 4. localStorage toggle. 5. PR + screenshots."}'::jsonb,
        'design', 3, 'solo', 'fun',
        45, 'sqlx'
    ),
    -- 10. Capstone S2 : Wax Design Language 1.0
    (
        'Capstone Saison 2 : publier Wax Design Language 1.0',
        'Le deliverable de cloture. En equipe (3-5 personnes, design + code obligatoire), publier la specification v1.0 du Skilluv Wax Design Language : tokens + icones + composants + guidelines + rendu Storybook public.',
        E'1. Forme une equipe de 3-5 : minimum 1 designer + 1 dev + 1 tech-writer\n2. Consolide tous les artefacts Saison 2 mergees en un package `wax-design-language@1.0.0` publiable sur npm\n3. Storybook public deploye sur wax.skilluv.io (Coolify) ou GitHub Pages\n4. Documente les guidelines d''usage (quand utiliser un motif, quand pas)\n5. Grande Epreuve : demo publique de fin de saison, presentation par l''equipe + Q&A ouvertes.',
        '{"fr":"Capstone Saison 2 : publier Wax Design Language 1.0","en":"Season 2 Capstone: publish Wax Design Language 1.0"}'::jsonb,
        '{"fr":"Deliverable de cloture, en equipe (3-5, design + code obligatoire).","en":"Season closing deliverable, team-based (3-5, design + code required)."}'::jsonb,
        '{"fr":"1. Equipe 3-5 (designer + dev + tech-writer). 2. Package npm wax-design-language@1.0.0. 3. Storybook public. 4. Guidelines. 5. Grande Epreuve.","en":"1. Team 3-5 (designer + dev + tech-writer). 2. wax-design-language@1.0.0 npm package. 3. Public Storybook. 4. Guidelines. 5. Grande Epreuve."}'::jsonb,
        'design', 5, 'team', 'serious',
        150, 'wax-icons'
    )
) AS v(
    title, description, instructions,
    title_i18n, description_i18n, instructions_i18n,
    skill_domain, difficulty, mode, tone,
    reward_fragments, project_slug
)
JOIN projects_map pm ON pm.slug = v.project_slug
WHERE NOT EXISTS (
    SELECT 1 FROM challenge_templates ct WHERE ct.title = v.title
);

UPDATE challenge_templates
SET is_capstone = true
WHERE title = 'Capstone Saison 2 : publier Wax Design Language 1.0';

-- Recap
SELECT ct.title, p.slug AS project, ct.skill_domain, ct.difficulty, ct.mode, ct.is_capstone
FROM challenge_templates ct
JOIN projects p ON p.id = ct.project_id
JOIN project_seasons ps ON ps.project_id = p.id
JOIN seasons s ON s.id = ps.season_id
WHERE s.slug = 'saison-2-wax'
  AND ct.title LIKE '%wax%' OR ct.title LIKE '%Wax%'
ORDER BY ct.difficulty, ct.title;
