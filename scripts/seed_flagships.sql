-- Seed 2 Skilluv flagships.
-- Steward temporaire : admin@skilluv.local. À réassigner à Jérémie en prod.
-- Idempotent : ON CONFLICT (slug) DO NOTHING.

INSERT INTO projects (
    slug, name, description, repo_url, tech_stack,
    is_oss, looking_for_contributors, owner_type, owner_id,
    curated_by_admin, is_flagship, flagship_steward_user_id,
    skilluv_partnership_level, skilluv_editorial_notes
) VALUES
(
    'hello-africa', 'Hello Africa',
    'Le premier « Hello World » Skilluv : landing multilingue qui accueille chaque nouveau·elle contributeur·rice africain·e de la tech. Onboarding vitrine, tremplin vers les vraies contributions OSS.',
    'https://github.com/skilluv-community/hello-africa',
    ARRAY['TypeScript','SvelteKit','i18n','TailwindCSS'],
    true, true, 'user', '527b047b-32a2-4b7d-a623-3bacdc751578',
    true, true, '527b047b-32a2-4b7d-a623-3bacdc751578',
    NULL,
    'Flagship #1 — onboarding. Objectif : produire l''artefact "premier commit merged" pour chaque nouveau user Skilluv. Ne jamais laisser stagner : issues good-first-issue triées manuellement chaque semaine. Steward Jérémie (à réassigner en prod).'
),
(
    'wax-icons', 'Wax Icons',
    'Bibliothèque d''icônes OSS inspirées des motifs wax et textiles d''Afrique de l''Ouest. SVG + design tokens, i18n dans les noms (FR/EN + wolof/lingala/bambara).',
    'https://github.com/skilluv-community/wax-icons',
    ARRAY['SVG','TypeScript','design system','i18n'],
    true, true, 'user', '527b047b-32a2-4b7d-a623-3bacdc751578',
    true, true, '527b047b-32a2-4b7d-a623-3bacdc751578',
    NULL,
    'Flagship #2 — design + culture. Objectif : premier design system africain OSS. Contribution accessible aux designers non-devs (SVG upload + naming), tremplin vers l''écriture de composants React/Svelte. Steward Jérémie (à réassigner en prod).'
)
ON CONFLICT (slug) DO NOTHING;

SELECT slug, name, is_flagship, flagship_steward_user_id IS NOT NULL AS has_steward
FROM projects
WHERE slug IN ('hello-africa','wax-icons')
ORDER BY slug;
