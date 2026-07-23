-- Priorite haute #3 strategy doc §15 : seed les 15 challenge_templates Bonjour
-- Skilluv, un par starter template (voir Annexe G du strategy doc).
--
-- Chaque template a `is_onboarding = TRUE` + `is_training = TRUE` (contrainte
-- `challenge_templates_project_or_training` : published requiert is_training
-- OU project_id NOT NULL — on va sur is_training pour ne pas lier a un projet
-- specifique).
--
-- Difficulty = 1 (safe zone garantie).
-- Skill domain = code (majorite) ou design (frontend-svelte et frontend-react
-- servent aussi les web-designers).
-- reward_fragments = 10 (unlock badge Bonjour Skilluv qui vaut deja beaucoup
-- en soi, pas de sur-recompense).
--
-- Idempotent via WHERE NOT EXISTS sur title exact.

WITH admin_user AS (SELECT id FROM users WHERE email = 'admin@skilluv.local')

INSERT INTO challenge_templates (
    title, description, instructions,
    title_i18n, description_i18n, instructions_i18n,
    skill_domain, difficulty, mode, tone,
    reward_fragments, is_onboarding, is_training, status,
    ai_policy, is_capstone, created_by
)
SELECT
    v.title, v.description, v.instructions,
    v.title_i18n, v.description_i18n, v.instructions_i18n,
    v.skill_domain, 1, 'solo', 'fun',
    10, true, true, 'published',
    'disclosure_required', false, au.id
FROM admin_user au
CROSS JOIN (VALUES
    -- ── Fullstack (4)
    (
        'Bonjour Skilluv — Fullstack Rust',
        'Ton premier commit sur le starter Skilluv-signature : Rust + Axum + SvelteKit + Postgres.',
        E'1. Trigger POST /api/onboarding/bonjour-skilluv/start?starter=starter-fullstack-rust\n2. On te fork skilluv-community/starter-fullstack-rust sur ton compte GitHub\n3. Edite HELLO.md, ajoute ta presentation, commit + push\n4. Ouvre une PR main -> showcase sur ton fork\n5. Le webhook detecte + debloque le badge Bonjour Skilluv',
        '{"fr":"Bonjour Skilluv — Fullstack Rust","en":"Hello Skilluv — Fullstack Rust"}'::jsonb,
        '{"fr":"Ton premier commit sur le starter Skilluv-signature : Rust + Axum + SvelteKit + Postgres.","en":"Your first commit on the Skilluv-signature starter: Rust + Axum + SvelteKit + Postgres."}'::jsonb,
        '{"fr":"Fork starter-fullstack-rust. Edite HELLO.md. PR main->showcase. Webhook debloque le badge.","en":"Fork starter-fullstack-rust. Edit HELLO.md. Open PR main->showcase. Webhook unlocks the badge."}'::jsonb,
        'code'
    ),
    (
        'Bonjour Skilluv — Fullstack Python',
        'Ton premier commit sur SvelteKit + FastAPI + SQLAlchemy + Postgres.',
        E'1. Trigger POST /api/onboarding/bonjour-skilluv/start?starter=starter-fullstack-python\n2. Fork automatique de skilluv-community/starter-fullstack-python\n3. Edite HELLO.md avec ta presentation\n4. commit + push + ouvre la PR main -> showcase\n5. Le webhook debloque le badge.',
        '{"fr":"Bonjour Skilluv — Fullstack Python","en":"Hello Skilluv — Fullstack Python"}'::jsonb,
        '{"fr":"Ton premier commit sur SvelteKit + FastAPI + SQLAlchemy + Postgres.","en":"Your first commit on SvelteKit + FastAPI + SQLAlchemy + Postgres."}'::jsonb,
        '{"fr":"Fork starter-fullstack-python. HELLO.md. PR main->showcase.","en":"Fork starter-fullstack-python. HELLO.md. PR main->showcase."}'::jsonb,
        'code'
    ),
    (
        'Bonjour Skilluv — Fullstack Node',
        'Ton premier commit sur SvelteKit + NestJS + Prisma + Postgres.',
        E'1. Trigger POST /api/onboarding/bonjour-skilluv/start?starter=starter-fullstack-node\n2. Fork automatique de skilluv-community/starter-fullstack-node\n3. Edite HELLO.md avec ta presentation\n4. commit + push + ouvre la PR main -> showcase\n5. Le webhook debloque le badge.',
        '{"fr":"Bonjour Skilluv — Fullstack Node","en":"Hello Skilluv — Fullstack Node"}'::jsonb,
        '{"fr":"Ton premier commit sur SvelteKit + NestJS + Prisma + Postgres.","en":"Your first commit on SvelteKit + NestJS + Prisma + Postgres."}'::jsonb,
        '{"fr":"Fork starter-fullstack-node. HELLO.md. PR main->showcase.","en":"Fork starter-fullstack-node. HELLO.md. PR main->showcase."}'::jsonb,
        'code'
    ),
    (
        'Bonjour Skilluv — Fullstack Go',
        'Ton premier commit sur SvelteKit + Gin + sqlx-go + Postgres.',
        E'1. Trigger POST /api/onboarding/bonjour-skilluv/start?starter=starter-fullstack-go\n2. Fork automatique.\n3. Edite HELLO.md.\n4. PR main -> showcase.\n5. Badge debloque.',
        '{"fr":"Bonjour Skilluv — Fullstack Go","en":"Hello Skilluv — Fullstack Go"}'::jsonb,
        '{"fr":"Ton premier commit sur SvelteKit + Gin + sqlx-go + Postgres.","en":"Your first commit on SvelteKit + Gin + sqlx-go + Postgres."}'::jsonb,
        '{"fr":"Fork starter-fullstack-go. HELLO.md. PR main->showcase.","en":"Fork starter-fullstack-go. HELLO.md. PR main->showcase."}'::jsonb,
        'code'
    ),

    -- ── Frontend seul (3)
    (
        'Bonjour Skilluv — Frontend React',
        'Ton premier commit sur Vite + React 19 + TypeScript + Tailwind.',
        E'1. Trigger POST /api/onboarding/bonjour-skilluv/start?starter=starter-frontend-react\n2. Fork automatique.\n3. Edite HELLO.md.\n4. PR main -> showcase.\n5. Badge debloque.',
        '{"fr":"Bonjour Skilluv — Frontend React","en":"Hello Skilluv — Frontend React"}'::jsonb,
        '{"fr":"Ton premier commit sur Vite + React 19 + TypeScript + Tailwind.","en":"Your first commit on Vite + React 19 + TypeScript + Tailwind."}'::jsonb,
        '{"fr":"Fork starter-frontend-react. HELLO.md. PR main->showcase.","en":"Fork starter-frontend-react. HELLO.md. PR main->showcase."}'::jsonb,
        'code'
    ),
    (
        'Bonjour Skilluv — Frontend Svelte',
        'Ton premier commit sur Vite + Svelte 5 + TypeScript + Tailwind. Le starter Skilluv-signature front.',
        E'1. Trigger POST /api/onboarding/bonjour-skilluv/start?starter=starter-frontend-svelte\n2. Fork automatique.\n3. Edite HELLO.md.\n4. PR main -> showcase.\n5. Badge debloque.',
        '{"fr":"Bonjour Skilluv — Frontend Svelte","en":"Hello Skilluv — Frontend Svelte"}'::jsonb,
        '{"fr":"Ton premier commit sur Vite + Svelte 5 + TypeScript + Tailwind.","en":"Your first commit on Vite + Svelte 5 + TypeScript + Tailwind."}'::jsonb,
        '{"fr":"Fork starter-frontend-svelte. HELLO.md. PR main->showcase.","en":"Fork starter-frontend-svelte. HELLO.md. PR main->showcase."}'::jsonb,
        'code'
    ),
    (
        'Bonjour Skilluv — Frontend HTMX',
        'Ton premier commit sur Astro + HTMX + Alpine + Tailwind. La voie contrarian, minimaliste et rapide.',
        E'1. Trigger POST /api/onboarding/bonjour-skilluv/start?starter=starter-frontend-htmx\n2. Fork automatique.\n3. Edite HELLO.md.\n4. PR main -> showcase.\n5. Badge debloque.',
        '{"fr":"Bonjour Skilluv — Frontend HTMX","en":"Hello Skilluv — Frontend HTMX"}'::jsonb,
        '{"fr":"Ton premier commit sur Astro + HTMX + Alpine + Tailwind.","en":"Your first commit on Astro + HTMX + Alpine + Tailwind."}'::jsonb,
        '{"fr":"Fork starter-frontend-htmx. HELLO.md. PR main->showcase.","en":"Fork starter-frontend-htmx. HELLO.md. PR main->showcase."}'::jsonb,
        'code'
    ),

    -- ── Mobile (3)
    (
        'Bonjour Skilluv — Mobile React Native',
        'Ton premier commit sur Expo + React Native + TypeScript + TanStack Query.',
        E'1. Trigger POST /api/onboarding/bonjour-skilluv/start?starter=starter-mobile-react-native\n2. Fork automatique.\n3. Edite HELLO.md.\n4. PR main -> showcase.\n5. Badge debloque.',
        '{"fr":"Bonjour Skilluv — Mobile React Native","en":"Hello Skilluv — Mobile React Native"}'::jsonb,
        '{"fr":"Ton premier commit sur Expo + React Native + TypeScript + TanStack Query.","en":"Your first commit on Expo + React Native + TypeScript + TanStack Query."}'::jsonb,
        '{"fr":"Fork starter-mobile-react-native. HELLO.md. PR main->showcase.","en":"Fork starter-mobile-react-native. HELLO.md. PR main->showcase."}'::jsonb,
        'code'
    ),
    (
        'Bonjour Skilluv — Mobile Flutter',
        'Ton premier commit sur Flutter 3.x + Dart + Riverpod. Aligne DevFest Africa.',
        E'1. Trigger POST /api/onboarding/bonjour-skilluv/start?starter=starter-mobile-flutter\n2. Fork automatique.\n3. Edite HELLO.md.\n4. PR main -> showcase.\n5. Badge debloque.',
        '{"fr":"Bonjour Skilluv — Mobile Flutter","en":"Hello Skilluv — Mobile Flutter"}'::jsonb,
        '{"fr":"Ton premier commit sur Flutter 3.x + Dart + Riverpod.","en":"Your first commit on Flutter 3.x + Dart + Riverpod."}'::jsonb,
        '{"fr":"Fork starter-mobile-flutter. HELLO.md. PR main->showcase.","en":"Fork starter-mobile-flutter. HELLO.md. PR main->showcase."}'::jsonb,
        'code'
    ),
    (
        'Bonjour Skilluv — Mobile Kotlin',
        'Ton premier commit sur Android Studio + Kotlin + Jetpack Compose + Ktor client.',
        E'1. Trigger POST /api/onboarding/bonjour-skilluv/start?starter=starter-mobile-kotlin\n2. Fork automatique.\n3. Edite HELLO.md.\n4. PR main -> showcase.\n5. Badge debloque.',
        '{"fr":"Bonjour Skilluv — Mobile Kotlin","en":"Hello Skilluv — Mobile Kotlin"}'::jsonb,
        '{"fr":"Ton premier commit sur Android Studio + Kotlin + Jetpack Compose.","en":"Your first commit on Android Studio + Kotlin + Jetpack Compose."}'::jsonb,
        '{"fr":"Fork starter-mobile-kotlin. HELLO.md. PR main->showcase.","en":"Fork starter-mobile-kotlin. HELLO.md. PR main->showcase."}'::jsonb,
        'code'
    ),

    -- ── Game dev (2)
    (
        'Bonjour Skilluv — Game Godot',
        'Ton premier commit sur Godot 4.x + GDScript + tilemap 2D. La voie douce vers le game dev.',
        E'1. Trigger POST /api/onboarding/bonjour-skilluv/start?starter=starter-game-godot\n2. Fork automatique.\n3. Edite HELLO.md.\n4. PR main -> showcase.\n5. Badge debloque.',
        '{"fr":"Bonjour Skilluv — Game Godot","en":"Hello Skilluv — Game Godot"}'::jsonb,
        '{"fr":"Ton premier commit sur Godot 4.x + GDScript + tilemap 2D.","en":"Your first commit on Godot 4.x + GDScript + 2D tilemap."}'::jsonb,
        '{"fr":"Fork starter-game-godot. HELLO.md. PR main->showcase.","en":"Fork starter-game-godot. HELLO.md. PR main->showcase."}'::jsonb,
        'code'
    ),
    (
        'Bonjour Skilluv — Game Bevy',
        'Ton premier commit sur Bevy + Rust + WebGL. Le croisement Rust/game dev.',
        E'1. Trigger POST /api/onboarding/bonjour-skilluv/start?starter=starter-game-bevy\n2. Fork automatique.\n3. Edite HELLO.md.\n4. PR main -> showcase.\n5. Badge debloque.',
        '{"fr":"Bonjour Skilluv — Game Bevy","en":"Hello Skilluv — Game Bevy"}'::jsonb,
        '{"fr":"Ton premier commit sur Bevy + Rust + WebGL.","en":"Your first commit on Bevy + Rust + WebGL."}'::jsonb,
        '{"fr":"Fork starter-game-bevy. HELLO.md. PR main->showcase.","en":"Fork starter-game-bevy. HELLO.md. PR main->showcase."}'::jsonb,
        'code'
    ),

    -- ── Data / IA (1)
    (
        'Bonjour Skilluv — Data Python',
        'Ton premier commit sur Jupyter + pandas + scikit-learn + DuckDB, dataset africain d''exemple.',
        E'1. Trigger POST /api/onboarding/bonjour-skilluv/start?starter=starter-data-python\n2. Fork automatique.\n3. Edite HELLO.md.\n4. PR main -> showcase.\n5. Badge debloque.',
        '{"fr":"Bonjour Skilluv — Data Python","en":"Hello Skilluv — Data Python"}'::jsonb,
        '{"fr":"Ton premier commit sur Jupyter + pandas + scikit-learn + DuckDB.","en":"Your first commit on Jupyter + pandas + scikit-learn + DuckDB."}'::jsonb,
        '{"fr":"Fork starter-data-python. HELLO.md. PR main->showcase.","en":"Fork starter-data-python. HELLO.md. PR main->showcase."}'::jsonb,
        'code'
    ),

    -- ── Embarque / IoT (1)
    (
        'Bonjour Skilluv — IoT ESP32',
        'Ton premier commit sur Rust embedded (esp-hal) + MicroPython alternative + KiCad PCB base.',
        E'1. Trigger POST /api/onboarding/bonjour-skilluv/start?starter=starter-iot-esp32\n2. Fork automatique.\n3. Edite HELLO.md.\n4. PR main -> showcase.\n5. Badge debloque.',
        '{"fr":"Bonjour Skilluv — IoT ESP32","en":"Hello Skilluv — IoT ESP32"}'::jsonb,
        '{"fr":"Ton premier commit sur Rust embedded + MicroPython + KiCad.","en":"Your first commit on Rust embedded + MicroPython + KiCad."}'::jsonb,
        '{"fr":"Fork starter-iot-esp32. HELLO.md. PR main->showcase.","en":"Fork starter-iot-esp32. HELLO.md. PR main->showcase."}'::jsonb,
        'code'
    ),

    -- ── DevOps / Infra (1)
    (
        'Bonjour Skilluv — DevOps',
        'Ton premier commit sur Docker Compose + GHA CI/CD + Terraform + Coolify + Prometheus/Grafana.',
        E'1. Trigger POST /api/onboarding/bonjour-skilluv/start?starter=starter-devops\n2. Fork automatique.\n3. Edite HELLO.md.\n4. PR main -> showcase.\n5. Badge debloque.',
        '{"fr":"Bonjour Skilluv — DevOps","en":"Hello Skilluv — DevOps"}'::jsonb,
        '{"fr":"Ton premier commit sur Docker Compose + GHA CI/CD + Terraform + Coolify + Prometheus.","en":"Your first commit on Docker Compose + GHA CI/CD + Terraform + Coolify + Prometheus."}'::jsonb,
        '{"fr":"Fork starter-devops. HELLO.md. PR main->showcase.","en":"Fork starter-devops. HELLO.md. PR main->showcase."}'::jsonb,
        'code'
    )
) AS v(
    title, description, instructions,
    title_i18n, description_i18n, instructions_i18n,
    skill_domain
)
WHERE NOT EXISTS (
    SELECT 1 FROM challenge_templates ct WHERE ct.title = v.title
);

-- Recap
SELECT title, skill_domain, difficulty, is_onboarding, is_training, status
FROM challenge_templates
WHERE title LIKE 'Bonjour Skilluv — %'
ORDER BY title;
