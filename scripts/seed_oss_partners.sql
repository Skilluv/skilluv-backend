-- Seed 12 OSS partners (Tier 1 = curated_by_admin, no partnership_level yet)
-- Owner: admin@skilluv.local (dev fixture). En prod, réassigner à l'UUID Jérémie.
-- Idempotent : ON CONFLICT (slug) DO NOTHING.

INSERT INTO projects (
    slug, name, description, repo_url, tech_stack,
    is_oss, looking_for_contributors, owner_type, owner_id,
    curated_by_admin, is_flagship, skilluv_partnership_level, skilluv_editorial_notes
) VALUES
(
    'sqlx', 'sqlx',
    'Async, pure-Rust SQL toolkit with compile-time checked queries. Backend Skilluv l''utilise en prod.',
    'https://github.com/launchbadge/sqlx',
    ARRAY['Rust','PostgreSQL','MySQL','SQLite'],
    true, true, 'user', '527b047b-32a2-4b7d-a623-3bacdc751578',
    true, false, NULL,
    'Priorité stratégique. Relation directe @abonander (mainteneur). Cible MoU Q2 2027 après N>=5 PRs Skilluv mergées. Compagnonnage débutant Rust idéal (issues good-first-issue régulières).'
),
(
    'calcom', 'Cal.com',
    'Alternative OSS à Calendly, adoption massive en Afrique de l''Ouest chez freelances/consultants.',
    'https://github.com/calcom/cal.com',
    ARRAY['TypeScript','Next.js','tRPC','Prisma','PostgreSQL'],
    true, true, 'user', '527b047b-32a2-4b7d-a623-3bacdc751578',
    true, false, NULL,
    'Priorité stratégique. Cible MoU Q2 2027 : intégration mentorship Skilluv + label "Cal.com Contributor from Africa". Bonne rampe fullstack TS.'
),
(
    'axum', 'Axum',
    'Framework HTTP ergonomique Rust basé sur Tokio. Backend Skilluv l''utilise.',
    'https://github.com/tokio-rs/axum',
    ARRAY['Rust','Tokio','Tower','hyper'],
    true, true, 'user', '527b047b-32a2-4b7d-a623-3bacdc751578',
    true, false, NULL,
    'Contribution → crédibilité maximale équipe. Cibler tower-http + axum-extra en priorité. Prévoir un mentor Rust senior par cohorte.'
),
(
    'rust-i18n', 'rust-i18n',
    'Internationalisation Rust simple, macro-based. Utilisé par Skilluv-backend.',
    'https://github.com/longbridge/rust-i18n',
    ARRAY['Rust','i18n'],
    true, true, 'user', '527b047b-32a2-4b7d-a623-3bacdc751578',
    true, false, NULL,
    'Petit projet, mainteneurs réactifs, parfait pour première PR Rust. Bonus : ajouter locales africaines (wolof, lingala, bambara, éwé) = double impact.'
),
(
    'meilisearch', 'Meilisearch',
    'Moteur de recherche OSS français, alternative Algolia/Elasticsearch. Support vector search.',
    'https://github.com/meilisearch/meilisearch',
    ARRAY['Rust','search','embeddings'],
    true, true, 'user', '527b047b-32a2-4b7d-a623-3bacdc751578',
    true, false, NULL,
    'Sujet embeddings/vector search très porteur pour talents IA africains. Doc en cours de traduction FR — porte d''entrée non-code également.'
),
(
    'coolify', 'Coolify',
    'PaaS OSS self-hosted, alternative Vercel/Heroku. Skilluv self-host sa stack dessus.',
    'https://github.com/coollabsio/coolify',
    ARRAY['PHP','Laravel','Livewire','Alpine.js'],
    true, true, 'user', '527b047b-32a2-4b7d-a623-3bacdc751578',
    true, false, NULL,
    'Signature technique Skilluv. Souveraineté hébergement Afrique (VPS locaux). Public Laravel/PHP énorme sur le continent, filière recrutement enterprise clef.'
),
(
    'nestjs', 'NestJS',
    'Framework Node.js progressif TypeScript, architecture opinionated.',
    'https://github.com/nestjs/nest',
    ARRAY['TypeScript','Node.js','Express','Fastify'],
    true, true, 'user', '527b047b-32a2-4b7d-a623-3bacdc751578',
    true, false, NULL,
    'Filière backend TS haute demande marché africain. Communauté anglophone ouverte aux nouveaux contributeurs. Doc translations FR = porte d''entrée.'
),
(
    'excalidraw', 'Excalidraw',
    'Whiteboard virtuel main-drawn, OSS, très utilisé pour diagrammes tech.',
    'https://github.com/excalidraw/excalidraw',
    ARRAY['TypeScript','React','Canvas','WebSocket'],
    true, true, 'user', '527b047b-32a2-4b7d-a623-3bacdc751578',
    true, false, NULL,
    'Visibilité (carnet Jérémie). Communauté saine. Bonne rampe pour talents "design + code".'
),
(
    'directus', 'Directus',
    'Headless CMS OSS TypeScript, alternative Strapi.',
    'https://github.com/directus/directus',
    ARRAY['TypeScript','Vue.js','Node.js','PostgreSQL'],
    true, true, 'user', '527b047b-32a2-4b7d-a623-3bacdc751578',
    true, false, NULL,
    'Filière recrutement PME africaines très porteuse (agences web). Contribution TS + Vue accessible.'
),
(
    'flutter', 'Flutter',
    'Framework UI Google pour build mobile/web/desktop avec un seul codebase Dart.',
    'https://github.com/flutter/flutter',
    ARRAY['Dart','Flutter','mobile'],
    true, true, 'user', '527b047b-32a2-4b7d-a623-3bacdc751578',
    true, false, NULL,
    'Afrique = mobile-first. Flutter dominant chez agences dev mobile Lagos/Nairobi/Abidjan. Ecosystème pub.dev = terrain "premier package" idéal.'
),
(
    'prisma', 'Prisma',
    'ORM TypeScript avec moteur Rust. Utilisé par Cal.com (cohérence catalogue).',
    'https://github.com/prisma/prisma',
    ARRAY['TypeScript','Rust','ORM','PostgreSQL'],
    true, true, 'user', '527b047b-32a2-4b7d-a623-3bacdc751578',
    true, false, NULL,
    'ORM TS le plus populaire. Contribution mixte Rust (moteur) + TS (client). Passerelle naturelle Prisma→sqlx pour les Rustacés en herbe.'
),
(
    'bevy', 'Bevy',
    'Game engine Rust ECS-based, moderne, communauté d''accueil exemplaire.',
    'https://github.com/bevyengine/bevy',
    ARRAY['Rust','ECS','game engine','WGPU'],
    true, true, 'user', '527b047b-32a2-4b7d-a623-3bacdc751578',
    true, false, NULL,
    'Passion Jérémie (carnet). PAS moteur Skilluv — flag "orientation game-dev optionnelle". Doc PR super soignée, bonne école de code Rust idiomatique.'
)
ON CONFLICT (slug) DO NOTHING;

SELECT slug, name, curated_by_admin, is_flagship, array_length(tech_stack, 1) as tech_count
FROM projects
WHERE slug IN ('sqlx','calcom','axum','rust-i18n','meilisearch','coolify','nestjs','excalidraw','directus','flutter','prisma','bevy')
ORDER BY slug;
