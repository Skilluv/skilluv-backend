-- Seed Saison 1 "Hello World" + 10 deliverables (challenge_templates).
-- Idempotent : ON CONFLICT sur slug / title.

-- 1. Saison 1
INSERT INTO seasons (slug, name, theme, description, starts_at, ends_at, status)
VALUES (
    'saison-1-hello-world',
    'Saison 1 — Hello World',
    'Hello World',
    'La saison des premiers commits. Chaque contributeur·rice repart avec au moins une PR mergée sur un projet OSS réel. Pas de simulacres, pas de sandbox : de vraies contributions opposables.',
    '2027-01-01 00:00:00+00',
    '2027-06-30 23:59:59+00',
    'upcoming'
)
ON CONFLICT (slug) DO NOTHING;

-- 2. Focus projets Saison 1 (primary = hello-africa flagship + Tier 1 les plus accessibles)
INSERT INTO project_seasons (project_id, season_id, focus_type)
SELECT p.id, s.id, 'primary'
FROM projects p
CROSS JOIN seasons s
WHERE s.slug = 'saison-1-hello-world'
  AND p.slug IN ('hello-africa', 'sqlx', 'rust-i18n', 'excalidraw', 'wax-icons', 'flutter')
ON CONFLICT DO NOTHING;

INSERT INTO project_seasons (project_id, season_id, focus_type)
SELECT p.id, s.id, 'featured'
FROM projects p
CROSS JOIN seasons s
WHERE s.slug = 'saison-1-hello-world'
  AND p.slug IN ('calcom', 'nestjs', 'meilisearch', 'coolify')
ON CONFLICT DO NOTHING;

-- 3. Les 10 deliverables Saison 1
-- Structure : title en français direct, i18n JSONB avec fr+en, project_id lié.
-- created_by = admin@skilluv.local (dev fixture, à ré-owner en prod)
-- status='published' → contrainte force project_id NOT NULL, ce qui est le cas ici.

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
    v.reward_fragments, v.is_onboarding, 'published',
    pm.id, 'disclosure_required', false, au.id
FROM admin_user au
CROSS JOIN (VALUES
    -- 1. First README typo — Hello Africa
    (
        'Ton premier commit : corriger un typo dans le README',
        'La contribution la plus simple : ouvrir une PR qui corrige une faute de frappe dans un README. Petit geste, grande porte d''entrée dans l''OSS.',
        E'1. Va sur https://github.com/skilluv-community/hello-africa\n2. Ouvre le README, repère une faute (il y en a exprès)\n3. Fork, corrige, ouvre la PR\n4. Rattache la PR ici pour validation.',
        '{"fr":"Ton premier commit : corriger un typo dans le README","en":"Your first commit: fix a typo in the README"}'::jsonb,
        '{"fr":"La contribution la plus simple : ouvrir une PR qui corrige une faute de frappe. Petit geste, grande porte d''entrée dans l''OSS.","en":"The simplest contribution: opening a PR that fixes a typo. Small gesture, big gateway into OSS."}'::jsonb,
        '{"fr":"1. Va sur le repo Hello Africa. 2. Ouvre le README, repère une faute. 3. Fork, corrige, ouvre la PR. 4. Rattache-la ici.","en":"1. Visit the Hello Africa repo. 2. Open the README, find a typo. 3. Fork, fix, open a PR. 4. Link it here."}'::jsonb,
        'code', 1, 'solo', 'fun',
        10, true, 'hello-africa'
    ),
    -- 2. Add Wolof locale — rust-i18n
    (
        'Ajouter la locale wolof à rust-i18n',
        'Traduire quelques clés démo en wolof et ouvrir une PR sur rust-i18n. Impact linguistique + technique en une PR.',
        E'1. Fork longbridge/rust-i18n\n2. Ajoute un fichier `locales/wo.yml` avec 5 clés minimum\n3. Vérifie que les tests passent (cargo test)\n4. Ouvre la PR + rattache-la ici.',
        '{"fr":"Ajouter la locale wolof à rust-i18n","en":"Add Wolof locale to rust-i18n"}'::jsonb,
        '{"fr":"Traduire quelques clés démo en wolof et ouvrir une PR sur rust-i18n.","en":"Translate a few demo keys into Wolof and open a PR on rust-i18n."}'::jsonb,
        '{"fr":"1. Fork rust-i18n. 2. Ajoute `locales/wo.yml` avec 5 clés. 3. cargo test. 4. PR + lien ici.","en":"1. Fork rust-i18n. 2. Add `locales/wo.yml` with 5 keys. 3. cargo test. 4. PR + link here."}'::jsonb,
        'code', 2, 'solo', 'serious',
        20, false, 'rust-i18n'
    ),
    -- 3. Excalidraw — accessibility label
    (
        'Ajouter un aria-label manquant dans Excalidraw',
        'Accessibilité web : traquer un bouton sans aria-label dans Excalidraw et ouvrir une PR. Contribution TS + culture a11y.',
        E'1. Clone excalidraw/excalidraw\n2. Lance en local, active le devtools axe-core\n3. Identifie un bouton sans aria-label\n4. Ouvre PR avec fix + capture axe avant/après.',
        '{"fr":"Ajouter un aria-label manquant dans Excalidraw","en":"Add a missing aria-label in Excalidraw"}'::jsonb,
        '{"fr":"Accessibilité web : trouver un bouton sans aria-label et ouvrir une PR.","en":"Web accessibility: find a button missing an aria-label and open a PR."}'::jsonb,
        '{"fr":"1. Clone Excalidraw. 2. Lance en local + axe-core. 3. Trouve un bouton sans label. 4. PR avec captures.","en":"1. Clone Excalidraw. 2. Run locally with axe-core. 3. Spot an unlabelled button. 4. PR with before/after."}'::jsonb,
        'code', 2, 'solo', 'serious',
        20, false, 'excalidraw'
    ),
    -- 4. Wax icon donation (design)
    (
        'Contribuer une icône Wax inspirée d''un motif local',
        'Dessine une icône SVG inspirée d''un motif wax de ton pays / ta région et propose-la à wax-icons.',
        E'1. Choisis un motif wax local (référence photo obligatoire)\n2. Vectorise en SVG 24×24, viewbox propre, un seul path si possible\n3. Ajoute au repo avec naming FR + EN + langue locale\n4. PR + explication culturelle courte (2-3 lignes).',
        '{"fr":"Contribuer une icône Wax inspirée d''un motif local","en":"Donate a Wax-inspired icon based on a local pattern"}'::jsonb,
        '{"fr":"Dessine une icône SVG inspirée d''un motif wax de ta région.","en":"Design an SVG icon inspired by a wax pattern from your region."}'::jsonb,
        '{"fr":"1. Choisis un motif wax local (photo ref). 2. Vectorise SVG 24×24. 3. Naming FR/EN + langue locale. 4. PR + note culturelle.","en":"1. Pick a local wax pattern (photo ref). 2. Vector SVG 24×24. 3. Naming FR/EN + local language. 4. PR + cultural note."}'::jsonb,
        'design', 1, 'solo', 'fun',
        15, true, 'wax-icons'
    ),
    -- 5. sqlx — good-first-issue hunt
    (
        'Résoudre une good-first-issue sur sqlx',
        'Choisir une issue taggée `good-first-issue` sur sqlx, la comprendre, la coder, la tester. Ta première PR Rust dans un projet majeur.',
        E'1. Va sur https://github.com/launchbadge/sqlx/labels/good-first-issue\n2. Prends-en une qui n''a pas déjà d''assignee\n3. Comment sur l''issue pour signaler que tu travailles dessus\n4. Fork, code, teste, PR. Documente ta démarche dans le deliverable.',
        '{"fr":"Résoudre une good-first-issue sur sqlx","en":"Solve a good-first-issue on sqlx"}'::jsonb,
        '{"fr":"Ta première PR Rust dans un projet majeur.","en":"Your first Rust PR to a major project."}'::jsonb,
        '{"fr":"1. Liste les good-first-issue sur sqlx. 2. Choisis-en une non assignée. 3. Comment que tu la prends. 4. Fork, code, teste, PR.","en":"1. Browse good-first-issues on sqlx. 2. Pick an unassigned one. 3. Comment to claim it. 4. Fork, code, test, PR."}'::jsonb,
        'code', 3, 'solo', 'serious',
        40, false, 'sqlx'
    ),
    -- 6. Flutter — first pub.dev package
    (
        'Publier ton premier package Flutter sur pub.dev',
        'Package minimal, utile, publiable : un widget "African Greeting" qui affiche "Hello" dans une langue africaine aléatoire.',
        E'1. Crée un package Flutter (`flutter create --template=package`)\n2. Implémente le widget + tests\n3. Publie sur pub.dev (compte gratuit)\n4. Rattache le lien pub.dev + repo GitHub ici.',
        '{"fr":"Publier ton premier package Flutter sur pub.dev","en":"Publish your first Flutter package on pub.dev"}'::jsonb,
        '{"fr":"Package minimal : un widget African Greeting.","en":"Minimal package: an African Greeting widget."}'::jsonb,
        '{"fr":"1. flutter create --template=package. 2. Widget + tests. 3. Publish pub.dev. 4. Lien pub.dev + repo ici.","en":"1. flutter create --template=package. 2. Widget + tests. 3. Publish pub.dev. 4. Link here."}'::jsonb,
        'code', 3, 'solo', 'fun',
        50, false, 'flutter'
    ),
    -- 7. Cal.com — translation FR-CI (Côte d'Ivoire specifics)
    (
        'Améliorer les traductions FR de Cal.com',
        'Traquer les strings anglaises qui traînent encore dans l''UI FR de Cal.com et ouvrir une PR de traduction.',
        E'1. Clone cal.com et lance en local en `locale=fr`\n2. Note 5 strings encore en anglais\n3. Trouve les fichiers `locales/fr/common.json`\n4. PR avec les traductions + screenshots.',
        '{"fr":"Améliorer les traductions FR de Cal.com","en":"Improve Cal.com French translations"}'::jsonb,
        '{"fr":"Traquer les strings anglaises qui traînent encore dans l''UI FR.","en":"Hunt down English strings still lingering in the French UI."}'::jsonb,
        '{"fr":"1. Clone + locale=fr. 2. Note 5 strings anglaises. 3. Édite locales/fr/common.json. 4. PR + captures.","en":"1. Clone + locale=fr. 2. Note 5 English strings. 3. Edit locales/fr/common.json. 4. PR + screenshots."}'::jsonb,
        'code', 2, 'solo', 'serious',
        25, false, 'calcom'
    ),
    -- 8. Coolify — bug reproduction repo
    (
        'Ouvrir un bug report Coolify avec reproduction minimale',
        'Prendre un bug ouvert non-reproduit sur Coolify, reproduire, documenter et ajouter un repro repo. Contribution non-code de haute valeur.',
        E'1. Va sur les issues Coolify sans label `needs-repro`\n2. Choisis-en une qui te parle, reproduis-la en local\n3. Ajoute un commentaire structuré (steps + logs + docker-compose minimal)\n4. Rattache ici l''URL de ton commentaire.',
        '{"fr":"Ouvrir un bug report Coolify avec reproduction minimale","en":"File a Coolify bug report with a minimal reproduction"}'::jsonb,
        '{"fr":"Contribution non-code de haute valeur : reproduire et documenter un bug ouvert.","en":"High-value non-code contribution: reproduce and document an open bug."}'::jsonb,
        '{"fr":"1. Issues Coolify non-reproduites. 2. Reproduis en local. 3. Commentaire structuré avec repro. 4. Lien du commentaire ici.","en":"1. Unreproduced Coolify issues. 2. Reproduce locally. 3. Structured comment with repro. 4. Comment URL here."}'::jsonb,
        'code', 3, 'solo', 'serious',
        30, false, 'coolify'
    ),
    -- 9. Meilisearch — doc translation FR
    (
        'Traduire une page de documentation Meilisearch en français',
        'La doc Meilisearch se traduit progressivement en FR. Prends une page anglaise, traduis, ouvre une PR sur le repo docs.',
        E'1. Va sur meilisearch/documentation\n2. Compare docs FR/EN, choisis une page manquante en FR\n3. Traduis en respectant le style guide (dans CONTRIBUTING)\n4. PR + rattache ici.',
        '{"fr":"Traduire une page de documentation Meilisearch en français","en":"Translate a Meilisearch documentation page into French"}'::jsonb,
        '{"fr":"La doc Meilisearch se traduit progressivement en FR.","en":"Meilisearch docs are being translated into French, page by page."}'::jsonb,
        '{"fr":"1. Repo meilisearch/documentation. 2. Page EN manquante en FR. 3. Traduis selon style guide. 4. PR + lien ici.","en":"1. Repo meilisearch/documentation. 2. Missing FR page. 3. Translate per style guide. 4. PR + link here."}'::jsonb,
        'code', 2, 'solo', 'educational',
        25, false, 'meilisearch'
    ),
    -- 10. Hello Africa — capstone team deliverable (fin de saison)
    (
        'Capstone Saison 1 : ajouter une langue africaine à Hello Africa',
        'Le deliverable de clôture. En équipe (2-4 personnes), ajouter une langue africaine complète au flagship Hello Africa : locale i18n + assets culturels + tests + doc contributeur.',
        E'1. Forme une équipe de 2-4 personnes sur Skilluv (feature d''équipe)\n2. Choisissez une langue non encore représentée\n3. Ajoutez la locale complète (i18n + assets)\n4. Documentez la démarche pour les prochaines équipes\n5. PR + démo publique en Grande Épreuve fin de saison.',
        '{"fr":"Capstone Saison 1 : ajouter une langue africaine à Hello Africa","en":"Season 1 Capstone: add an African language to Hello Africa"}'::jsonb,
        '{"fr":"Deliverable de clôture, en équipe (2-4 personnes).","en":"Season closing deliverable, team-based (2-4 people)."}'::jsonb,
        '{"fr":"1. Forme équipe 2-4. 2. Choisis une langue non représentée. 3. Ajoute locale + assets. 4. Documente. 5. PR + Grande Épreuve.","en":"1. Team of 2-4. 2. Pick an unrepresented language. 3. Add locale + assets. 4. Document. 5. PR + Grande Épreuve."}'::jsonb,
        'code', 4, 'team', 'serious',
        100, false, 'hello-africa'
    )
) AS v(
    title, description, instructions,
    title_i18n, description_i18n, instructions_i18n,
    skill_domain, difficulty, mode, tone,
    reward_fragments, is_onboarding, project_slug
)
JOIN projects_map pm ON pm.slug = v.project_slug
WHERE NOT EXISTS (
    SELECT 1 FROM challenge_templates ct WHERE ct.title = v.title
);

-- Marque le capstone
UPDATE challenge_templates
SET is_capstone = true
WHERE title = 'Capstone Saison 1 : ajouter une langue africaine à Hello Africa';

-- Résumé
SELECT
    ct.title,
    p.slug AS project,
    ct.skill_domain,
    ct.difficulty,
    ct.mode,
    ct.is_capstone,
    ct.status
FROM challenge_templates ct
JOIN projects p ON p.id = ct.project_id
WHERE p.slug IN ('hello-africa','sqlx','rust-i18n','excalidraw','wax-icons','flutter','calcom','coolify','meilisearch')
ORDER BY ct.difficulty, ct.title;
