-- 138 challenges, one set per code trade.
--
-- ## Why they are drafts
--
-- The title and the intent come from the backlog; the full brief — the
-- constraints, the numbers, what is out of scope — needs an author who knows
-- the trade. A challenge nobody has reviewed must not be offered to somebody
-- learning, and `draft` is the state the workflow already has for that.
--
-- Seeding them anyway is the point: thirty-three trades with an empty
-- catalogue are thirty-three trades the platform claims to support and
-- cannot. A draft is a starting point an operator edits; an empty list is
-- work nobody has begun.
--
-- ## Where the rubric comes from
--
-- Each challenge inherits the review grid of its trade's family, so
-- verification has criteria from the first day rather than from whenever
-- somebody remembers to write a rubric.
--
-- ## Difficulty
--
-- Set by family, and not a judgement about the people who work in it: it
-- says how much has to be true at once before the work is verifiable at all.
-- A kernel patch is not harder than a React component because kernels are
-- prestigious — it is harder because it cannot be half-done.

INSERT INTO challenge_templates
    (title, description, instructions, skill_domain, difficulty, language,
     status, is_training, evaluation_rubric)
SELECT
    c.title, c.description, c.instructions, 'code', d.difficulty, c.language,
    'draft', TRUE,
    COALESCE(
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'code' AND g.reviewer_group = o.reviewer_group),
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'code' AND g.reviewer_group IS NULL)
    )
FROM (VALUES
    ('web-frontend-developer', 'Component library reusable', 'publier component lib (React/Vue/Svelte) sur npm', '## Ce qu''il y a à faire

publier component lib (React/Vue/Svelte) sur npm.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'typescript'),
    ('web-frontend-developer', 'Performance refactor site', 'refactor site avec perf gains ≥ 30% (Lighthouse)', '## Ce qu''il y a à faire

refactor site avec perf gains ≥ 30% (Lighthouse).

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'typescript'),
    ('web-frontend-developer', 'A11y audit + fixes complets', 'a11y audit + PRs fixes WCAG AA', '## Ce qu''il y a à faire

a11y audit + PRs fixes WCAG AA.

## Ce qui est attendu

Un rapport vérifiable, avec ce qui a été trouvé et comment le reproduire.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'typescript'),
    ('web-frontend-developer', 'Progressive Web App', 'PWA complète avec offline + install', '## Ce qu''il y a à faire

PWA complète avec offline + install.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'typescript'),
    ('web-frontend-developer', 'Micro-frontend architecture', 'micro-frontend pattern implémenté', '## Ce qu''il y a à faire

micro-frontend pattern implémenté.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'typescript'),
    ('web-frontend-developer', 'State management library', 'publier state management lib alternative', '## Ce qu''il y a à faire

publier state management lib alternative.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'typescript'),
    ('web-backend-developer', 'REST API + OpenAPI spec', 'API REST complète + OpenAPI + docs auto', '## Ce qu''il y a à faire

API REST complète + OpenAPI + docs auto.

## Ce qui est attendu

Une contribution retenue dans une spécification ouverte.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('web-backend-developer', 'GraphQL schema + resolvers', 'schema GraphQL + resolvers + subscriptions', '## Ce qu''il y a à faire

schema GraphQL + resolvers + subscriptions.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('web-backend-developer', 'Authentication service', 'auth service (OAuth2/OIDC/SAML) complet', '## Ce qu''il y a à faire

auth service (OAuth2/OIDC/SAML) complet.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('web-backend-developer', 'Rate limiting + caching layer', 'rate limit + cache layer production-grade', '## Ce qu''il y a à faire

rate limit + cache layer production-grade.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('web-backend-developer', 'Webhook system + retry', 'webhook system avec retry + dead letter queue', '## Ce qu''il y a à faire

webhook system avec retry + dead letter queue.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('web-backend-developer', 'API versioning strategy', 'API versioning implémentée + migration path', '## Ce qu''il y a à faire

API versioning implémentée + migration path.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('web-fullstack-developer', 'SaaS mini app end-to-end', 'SaaS complet (auth + CRUD + billing + deploy) shipped', '## Ce qu''il y a à faire

SaaS complet (auth + CRUD + billing + deploy) shipped.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('web-fullstack-developer', 'Marketplace 2-sided app', 'marketplace 2-sided (buyer + seller) shipped', '## Ce qu''il y a à faire

marketplace 2-sided (buyer + seller) shipped.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('web-fullstack-developer', 'Real-time collab app', 'collab app real-time (WebSocket/WebRTC) shipped', '## Ce qu''il y a à faire

collab app real-time (WebSocket/WebRTC) shipped.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('web-fullstack-developer', 'E-commerce shop custom', 'e-commerce shop custom (pas Shopify) shipped', '## Ce qu''il y a à faire

e-commerce shop custom (pas Shopify) shipped.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('web-fullstack-developer', 'Blog platform custom', 'blog platform custom multi-tenant shipped', '## Ce qu''il y a à faire

blog platform custom multi-tenant shipped.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('web-performance-engineer', 'Perf audit + optimization', 'audit + PR fixes gains ≥ 40% mesurés', '## Ce qu''il y a à faire

audit + PR fixes gains ≥ 40% mesurés.

## Ce qui est attendu

Un rapport vérifiable, avec ce qui a été trouvé et comment le reproduire.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('web-performance-engineer', 'RUM setup + dashboard', 'Real User Monitoring setup + Grafana dashboard', '## Ce qu''il y a à faire

Real User Monitoring setup + Grafana dashboard.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('web-performance-engineer', 'Bundle size reduction ≥ 50%', 'bundle reduction avec code-splitting + tree-shaking + lazy loading', '## Ce qu''il y a à faire

bundle reduction avec code-splitting + tree-shaking + lazy loading.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('web-performance-engineer', 'Core Web Vitals passing', 'CWV LCP/CLS/INP passing sur site cible', '## Ce qu''il y a à faire

CWV LCP/CLS/INP passing sur site cible.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('web3-frontend-developer', 'dApp complet ethers.js', 'dApp interacting L1/L2 avec wallet connect + tx flow', '## Ce qu''il y a à faire

dApp interacting L1/L2 avec wallet connect + tx flow.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'typescript'),
    ('web3-frontend-developer', 'Multi-wallet abstraction', 'multi-wallet UX (MetaMask + Coinbase + WalletConnect + Rainbow)', '## Ce qu''il y a à faire

multi-wallet UX (MetaMask + Coinbase + WalletConnect + Rainbow).

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'typescript'),
    ('web3-frontend-developer', 'On-chain indexing frontend', 'frontend query subgraph + display', '## Ce qu''il y a à faire

frontend query subgraph + display.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'typescript'),
    ('web3-frontend-developer', 'Account abstraction UX', 'AA UX (ERC-4337) implémentée + demo', '## Ce qu''il y a à faire

AA UX (ERC-4337) implémentée + demo.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'typescript'),
    ('mobile-ios-developer', 'iOS app SwiftUI shipped', 'app SwiftUI shipped App Store (OR TestFlight)', '## Ce qu''il y a à faire

app SwiftUI shipped App Store (OR TestFlight).

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'swift'),
    ('mobile-ios-developer', 'HealthKit integration', 'feature HealthKit intégrée + shipped', '## Ce qu''il y a à faire

feature HealthKit intégrée + shipped.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'swift'),
    ('mobile-ios-developer', 'Widgets iOS 17+', 'widgets iOS 17+ implémentés + shipped', '## Ce qu''il y a à faire

widgets iOS 17+ implémentés + shipped.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'swift'),
    ('mobile-ios-developer', 'In-app purchase flow', 'IAP flow complet + StoreKit 2', '## Ce qu''il y a à faire

IAP flow complet + StoreKit 2.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'swift'),
    ('mobile-ios-developer', 'Push notifications rich', 'rich push notifications + attachments', '## Ce qu''il y a à faire

rich push notifications + attachments.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'swift'),
    ('mobile-android-developer', 'Android app Jetpack Compose shipped', 'app Compose shipped Play Store', '## Ce qu''il y a à faire

app Compose shipped Play Store.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'kotlin'),
    ('mobile-android-developer', 'Material 3 design system implementation', 'MD3 complet implémenté', '## Ce qu''il y a à faire

MD3 complet implémenté.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'kotlin'),
    ('mobile-android-developer', 'Background work WorkManager', 'background work robuste implémenté', '## Ce qu''il y a à faire

background work robuste implémenté.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'kotlin'),
    ('mobile-android-developer', 'Play Billing integration', 'billing complet', '## Ce qu''il y a à faire

billing complet.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'kotlin'),
    ('mobile-android-developer', 'Wear OS companion app', 'companion Wear OS shipped', '## Ce qu''il y a à faire

companion Wear OS shipped.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'kotlin'),
    ('mobile-cross-platform-developer', 'React Native app shipped', 'RN app shipped (both stores)', '## Ce qu''il y a à faire

RN app shipped (both stores).

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'dart'),
    ('mobile-cross-platform-developer', 'Flutter app shipped', 'Flutter app shipped', '## Ce qu''il y a à faire

Flutter app shipped.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'dart'),
    ('mobile-cross-platform-developer', 'Cross-platform native module', 'native module iOS + Android (Swift + Kotlin bridges)', '## Ce qu''il y a à faire

native module iOS + Android (Swift + Kotlin bridges).

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'dart'),
    ('mobile-cross-platform-developer', 'Offline-first architecture', 'offline-first RN OR Flutter avec sync', '## Ce qu''il y a à faire

offline-first RN OR Flutter avec sync.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'dart'),
    ('mobile-cross-platform-developer', 'Cross-platform DS component library', 'component library RN OR Flutter publiée', '## Ce qu''il y a à faire

component library RN OR Flutter publiée.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'dart'),
    ('desktop-app-developer', 'Tauri app shipped', 'Tauri app (Rust + web) shipped (Win/Mac/Linux)', '## Ce qu''il y a à faire

Tauri app (Rust + web) shipped (Win/Mac/Linux).

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('desktop-app-developer', 'Electron app shipped', 'Electron app shipped multi-plateforme', '## Ce qu''il y a à faire

Electron app shipped multi-plateforme.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('desktop-app-developer', 'Native Qt/GTK app', 'app native Qt OR GTK shipped Linux', '## Ce qu''il y a à faire

app native Qt OR GTK shipped Linux.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('desktop-app-developer', 'macOS AppKit app', 'app AppKit natif shipped', '## Ce qu''il y a à faire

app AppKit natif shipped.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('desktop-app-developer', '.NET WinUI 3 app', 'app WinUI 3 shipped Windows', '## Ce qu''il y a à faire

app WinUI 3 shipped Windows.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('enterprise-software-developer', 'SAP ABAP custom module', 'module ABAP custom shipped (proof employment OR sandbox)', '## Ce qu''il y a à faire

module ABAP custom shipped (proof employment OR sandbox).

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('enterprise-software-developer', 'Salesforce Apex + LWC', 'Apex + Lightning Web Component déployé', '## Ce qu''il y a à faire

Apex + Lightning Web Component déployé.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('enterprise-software-developer', 'ServiceNow scripted app', 'ServiceNow scripted app + integrations', '## Ce qu''il y a à faire

ServiceNow scripted app + integrations.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('enterprise-software-developer', 'Odoo custom module', 'module Odoo publié OSS OR entreprise', '## Ce qu''il y a à faire

module Odoo publié OSS OR entreprise.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('lowcode-platform-developer', 'Retool custom component published', 'Retool custom component publié + adopté équipe', '## Ce qu''il y a à faire

Retool custom component publié + adopté équipe.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('lowcode-platform-developer', 'n8n custom node published', 'n8n custom node publié community', '## Ce qu''il y a à faire

n8n custom node publié community.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('lowcode-platform-developer', 'Bubble plugin developed', 'Bubble plugin dev + published marketplace', '## Ce qu''il y a à faire

Bubble plugin dev + published marketplace.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('lowcode-platform-developer', 'Zapier custom app', 'custom app Zapier published', '## Ce qu''il y a à faire

custom app Zapier published.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('systems-programmer', 'Rust systems tool', 'tool systems Rust shipped + crate publiée', '## Ce qu''il y a à faire

tool systems Rust shipped + crate publiée.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('systems-programmer', 'C++ modern library', 'library C++ moderne (C++17+) publiée', '## Ce qu''il y a à faire

library C++ moderne (C++17+) publiée.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('systems-programmer', 'Zig systems experiment', 'projet Zig substantiel shipped', '## Ce qu''il y a à faire

projet Zig substantiel shipped.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('systems-programmer', 'Memory allocator custom', 'custom allocator implémenté + benchmarks', '## Ce qu''il y a à faire

custom allocator implémenté + benchmarks.

## Ce qui est attendu

Des mesures avec leur référence, leur méthode et le code qui les produit.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('systems-programmer', 'Async runtime custom', 'async runtime custom (Rust/C++) implémenté', '## Ce qu''il y a à faire

async runtime custom (Rust/C++) implémenté.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('systems-programmer', 'Bootloader custom', 'bootloader minimal fonctionnel', '## Ce qu''il y a à faire

bootloader minimal fonctionnel.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('kernel-driver-developer', 'Linux kernel patch merged', 'patch kernel merged upstream (mainline)', '## Ce qu''il y a à faire

patch kernel merged upstream (mainline).

## Ce qui est attendu

Une contribution acceptée dans un dépôt que tu ne contrôles pas.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'c'),
    ('kernel-driver-developer', 'Custom device driver', 'device driver Linux custom + tests', '## Ce qu''il y a à faire

device driver Linux custom + tests.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'c'),
    ('kernel-driver-developer', 'eBPF program deployed', 'eBPF program deployed production', '## Ce qu''il y a à faire

eBPF program deployed production.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'c'),
    ('kernel-driver-developer', 'Kernel module OSS', 'kernel module publié OSS', '## Ce qu''il y a à faire

kernel module publié OSS.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'c'),
    ('kernel-driver-developer', 'Windows driver (WDM/WDF)', 'driver Windows shipped', '## Ce qu''il y a à faire

driver Windows shipped.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'c'),
    ('firmware-embedded-developer', 'ESP32 IoT project shipped', 'projet IoT ESP32 avec firmware + protocole IoT', '## Ce qu''il y a à faire

projet IoT ESP32 avec firmware + protocole IoT.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('firmware-embedded-developer', 'STM32 RTOS project', 'projet STM32 avec RTOS (FreeRTOS/Zephyr) shipped', '## Ce qu''il y a à faire

projet STM32 avec RTOS (FreeRTOS/Zephyr) shipped.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('firmware-embedded-developer', 'Arduino library published', 'library Arduino publiée community', '## Ce qu''il y a à faire

library Arduino publiée community.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('firmware-embedded-developer', 'LoRaWAN device firmware', 'firmware device LoRaWAN complet', '## Ce qu''il y a à faire

firmware device LoRaWAN complet.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('firmware-embedded-developer', 'MQTT gateway custom', 'MQTT gateway custom + integrations', '## Ce qu''il y a à faire

MQTT gateway custom + integrations.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('firmware-embedded-developer', 'BLE peripheral firmware', 'firmware BLE peripheral avec services custom', '## Ce qu''il y a à faire

firmware BLE peripheral avec services custom.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('robotics-software-developer', 'ROS2 package published', 'ROS2 package publié + docs', '## Ce qu''il y a à faire

ROS2 package publié + docs.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('robotics-software-developer', 'Robot control custom', 'control loop robot (PID + kinematics) implémenté', '## Ce qu''il y a à faire

control loop robot (PID + kinematics) implémenté.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('robotics-software-developer', 'Drone autonomous flight', 'drone (PX4/ArduPilot) autonomous mission shipped', '## Ce qu''il y a à faire

drone (PX4/ArduPilot) autonomous mission shipped.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('robotics-software-developer', 'SLAM implementation', 'SLAM basic implémenté sur robot cible', '## Ce qu''il y a à faire

SLAM basic implémenté sur robot cible.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('safety-critical-developer', 'AUTOSAR module', 'module AUTOSAR conforme + tests (proof employment)', '## Ce qu''il y a à faire

module AUTOSAR conforme + tests (proof employment).

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('safety-critical-developer', 'DO-178C compliant subset', 'subset DO-178C aerospace compliant', '## Ce qu''il y a à faire

subset DO-178C aerospace compliant.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('safety-critical-developer', 'Medical device IEC 62304 module', 'module IEC 62304 conforme', '## Ce qu''il y a à faire

module IEC 62304 conforme.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('smart-contract-developer', 'ERC-20/721/1155 audited', 'token contract audited + deployed mainnet OU testnet', '## Ce qu''il y a à faire

token contract audited + deployed mainnet OU testnet.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'solidity'),
    ('smart-contract-developer', 'DeFi primitive contract', 'DeFi primitive (AMM/lending/staking) implémenté', '## Ce qu''il y a à faire

DeFi primitive (AMM/lending/staking) implémenté.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'solidity'),
    ('smart-contract-developer', 'DAO governance contract', 'DAO governance contract shipped', '## Ce qu''il y a à faire

DAO governance contract shipped.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'solidity'),
    ('smart-contract-developer', 'NFT collection contract advanced', 'NFT collection avec features (dynamic metadata, royalties EIP-2981)', '## Ce qu''il y a à faire

NFT collection avec features (dynamic metadata, royalties EIP-2981).

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'solidity'),
    ('smart-contract-developer', 'Cross-chain bridge contract', 'cross-chain bridge basic implémenté', '## Ce qu''il y a à faire

cross-chain bridge basic implémenté.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'solidity'),
    ('blockchain-protocol-developer', 'L2 rollup contribution', 'contribution PR merged sur L2 (Optimism/Arbitrum/zkSync core)', '## Ce qu''il y a à faire

contribution PR merged sur L2 (Optimism/Arbitrum/zkSync core).

## Ce qui est attendu

Une contribution acceptée dans un dépôt que tu ne contrôles pas.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('blockchain-protocol-developer', 'Consensus algorithm implementation', 'consensus algorithm (Raft/PBFT/HotStuff) implémenté', '## Ce qu''il y a à faire

consensus algorithm (Raft/PBFT/HotStuff) implémenté.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('blockchain-protocol-developer', 'ZK circuit implementation', 'ZK circuit (Circom/Halo2/Noir) implémenté + verified', '## Ce qu''il y a à faire

ZK circuit (Circom/Halo2/Noir) implémenté + verified.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('compiler-language-developer', 'Rust compiler PR merged', 'PR rustc merged', '## Ce qu''il y a à faire

PR rustc merged.

## Ce qui est attendu

Une contribution acceptée dans un dépôt que tu ne contrôles pas.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('compiler-language-developer', 'LLVM pass custom', 'LLVM pass custom implémenté', '## Ce qu''il y a à faire

LLVM pass custom implémenté.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('compiler-language-developer', 'Interpreter DSL', 'interpreter DSL implémenté + docs', '## Ce qu''il y a à faire

interpreter DSL implémenté + docs.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('compiler-language-developer', 'Custom linter', 'linter custom (Rust/JS/Python) publié', '## Ce qu''il y a à faire

linter custom (Rust/JS/Python) publié.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('formal-methods-developer', 'TLA+ specification', 'TLA+ spec système distribué complet', '## Ce qu''il y a à faire

TLA+ spec système distribué complet.

## Ce qui est attendu

Une contribution retenue dans une spécification ouverte.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('formal-methods-developer', 'Coq/Lean proof', 'proof formel Coq OR Lean d''algorithme', '## Ce qu''il y a à faire

proof formel Coq OR Lean d''algorithme.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('formal-methods-developer', 'Property-based testing library', 'property-based testing lib publiée', '## Ce qu''il y a à faire

property-based testing lib publiée.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('database-engine-developer', 'PostgreSQL extension published', 'extension PG (C/Rust via pgrx) publiée', '## Ce qu''il y a à faire

extension PG (C/Rust via pgrx) publiée.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('database-engine-developer', 'PG contribution merged', 'PR PostgreSQL merged upstream', '## Ce qu''il y a à faire

PR PostgreSQL merged upstream.

## Ce qui est attendu

Une contribution acceptée dans un dépôt que tu ne contrôles pas.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('database-engine-developer', 'ClickHouse function custom', 'user-defined function ClickHouse implémentée', '## Ce qu''il y a à faire

user-defined function ClickHouse implémentée.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('database-engine-developer', 'Vector DB extension', 'extension vector DB (pgvector optim, Qdrant plugin)', '## Ce qu''il y a à faire

extension vector DB (pgvector optim, Qdrant plugin).

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('search-engine-developer', 'Elasticsearch plugin custom', 'plugin ES custom publié', '## Ce qu''il y a à faire

plugin ES custom publié.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('search-engine-developer', 'Meilisearch contribution', 'PR merged Meilisearch core', '## Ce qu''il y a à faire

PR merged Meilisearch core.

## Ce qui est attendu

Une contribution acceptée dans un dépôt que tu ne contrôles pas.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('search-engine-developer', 'Custom search engine (Tantivy/Sonic)', 'search engine custom production-grade', '## Ce qu''il y a à faire

search engine custom production-grade.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('distributed-systems-developer', 'Consensus implementation (Raft/etc)', 'consensus algo implémenté', '## Ce qu''il y a à faire

consensus algo implémenté.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('distributed-systems-developer', 'Message broker custom', 'message broker custom implémenté + benchmarks', '## Ce qu''il y a à faire

message broker custom implémenté + benchmarks.

## Ce qui est attendu

Des mesures avec leur référence, leur méthode et le code qui les produit.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('distributed-systems-developer', 'Distributed cache', 'distributed cache (consistent hashing) implémenté', '## Ce qu''il y a à faire

distributed cache (consistent hashing) implémenté.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('distributed-systems-developer', 'libp2p application', 'application libp2p complète + docs', '## Ce qu''il y a à faire

application libp2p complète + docs.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('stream-processing-developer', 'Kafka Streams application', 'Kafka Streams app production + docs', '## Ce qu''il y a à faire

Kafka Streams app production + docs.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('stream-processing-developer', 'Flink job production', 'Flink job shipped production + monitoring', '## Ce qu''il y a à faire

Flink job shipped production + monitoring.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('stream-processing-developer', 'Beam pipeline cross-runner', 'Beam pipeline running Flink + Dataflow', '## Ce qu''il y a à faire

Beam pipeline running Flink + Dataflow.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('scientific-computing-developer', 'NumPy/SciPy contribution', 'PR merged NumPy OU SciPy', '## Ce qu''il y a à faire

PR merged NumPy OU SciPy.

## Ce qui est attendu

Une contribution acceptée dans un dépôt que tu ne contrôles pas.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'python'),
    ('scientific-computing-developer', 'Julia package published', 'Julia package publié Registry', '## Ce qu''il y a à faire

Julia package publié Registry.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'python'),
    ('scientific-computing-developer', 'Bioinformatics tool', 'tool bioinformatique publié (Biopython/Bioconductor)', '## Ce qu''il y a à faire

tool bioinformatique publié (Biopython/Bioconductor).

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'python'),
    ('scientific-computing-developer', 'Physics simulation', 'simulation physique publiée + docs', '## Ce qu''il y a à faire

simulation physique publiée + docs.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'python'),
    ('gpu-compute-developer', 'CUDA kernel optimized', 'CUDA kernel optimisé + benchmarks vs baseline', '## Ce qu''il y a à faire

CUDA kernel optimisé + benchmarks vs baseline.

## Ce qui est attendu

Des mesures avec leur référence, leur méthode et le code qui les produit.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'cuda'),
    ('gpu-compute-developer', 'WebGPU compute shader', 'WebGPU compute shader + demo web', '## Ce qu''il y a à faire

WebGPU compute shader + demo web.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'cuda'),
    ('gpu-compute-developer', 'ROCm port CUDA library', 'port CUDA library vers ROCm', '## Ce qu''il y a à faire

port CUDA library vers ROCm.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', 'cuda'),
    ('hft-quant-developer', 'Low-latency C++ trading engine minimal', 'trading engine minimal < 1ms latency', '## Ce qu''il y a à faire

trading engine minimal < 1ms latency.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('hft-quant-developer', 'Backtest framework Python', 'backtest framework publié', '## Ce qu''il y a à faire

backtest framework publié.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('hft-quant-developer', 'Risk model implementation', 'risk model implémenté + docs', '## Ce qu''il y a à faire

risk model implémenté + docs.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('network-protocol-developer', 'QUIC implementation minimal', 'QUIC minimal implémenté OU contribution existing (Quinn Rust, quiche)', '## Ce qu''il y a à faire

QUIC minimal implémenté OU contribution existing (Quinn Rust, quiche).

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('network-protocol-developer', 'Custom P2P protocol', 'protocol P2P custom implémenté', '## Ce qu''il y a à faire

protocol P2P custom implémenté.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('network-protocol-developer', 'Networking library published', 'networking lib publiée + benchmarks', '## Ce qu''il y a à faire

networking lib publiée + benchmarks.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('cli-tools-developer', 'CLI tool Rust/Go publié', 'CLI tool publié (crates.io / brew / apt) + 100+ users', '## Ce qu''il y a à faire

CLI tool publié (crates.io / brew / apt) + 100+ users.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('cli-tools-developer', 'TUI application', 'TUI application shipped (ratatui/bubbletea)', '## Ce qu''il y a à faire

TUI application shipped (ratatui/bubbletea).

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('cli-tools-developer', 'Developer productivity tool', 'productivity tool DevX (git alternative, task manager)', '## Ce qu''il y a à faire

productivity tool DevX (git alternative, task manager).

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('cli-tools-developer', 'CLI framework contribution', 'PR merged CLI framework (clap, cobra, click)', '## Ce qu''il y a à faire

PR merged CLI framework (clap, cobra, click).

## Ce qui est attendu

Une contribution acceptée dans un dépôt que tu ne contrôles pas.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('cli-tools-developer', 'Shell scripting library', 'library shell scripting robust (Nushell, Fish plugin)', '## Ce qu''il y a à faire

library shell scripting robust (Nushell, Fish plugin).

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('ide-extension-developer', 'VSCode extension published', 'extension publiée VSCode Marketplace + 1000+ installs', '## Ce qu''il y a à faire

extension publiée VSCode Marketplace + 1000+ installs.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('ide-extension-developer', 'Neovim plugin (Lua)', 'plugin Neovim publié + docs', '## Ce qu''il y a à faire

plugin Neovim publié + docs.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('ide-extension-developer', 'JetBrains plugin', 'plugin JetBrains publié Marketplace', '## Ce qu''il y a à faire

plugin JetBrains publié Marketplace.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('ide-extension-developer', 'LSP server custom', 'LSP server pour language cible + integration IDEs', '## Ce qu''il y a à faire

LSP server pour language cible + integration IDEs.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('build-system-developer', 'Nix flake substantial', 'Nix flake project substantial + docs', '## Ce qu''il y a à faire

Nix flake project substantial + docs.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('build-system-developer', 'Bazel rules custom', 'custom Bazel rules published', '## Ce qu''il y a à faire

custom Bazel rules published.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('build-system-developer', 'Cargo/npm plugin', 'plugin cargo OU npm dev + published', '## Ce qu''il y a à faire

plugin cargo OU npm dev + published.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('media-processing-developer', 'FFmpeg contribution OR wrapper library', 'PR FFmpeg merged OR wrapper library publiée', '## Ce qu''il y a à faire

PR FFmpeg merged OR wrapper library publiée.

## Ce qui est attendu

Une contribution acceptée dans un dépôt que tu ne contrôles pas.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('media-processing-developer', 'OpenCV pipeline', 'OpenCV pipeline computer vision production', '## Ce qu''il y a à faire

OpenCV pipeline computer vision production.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('media-processing-developer', 'Custom codec implementation', 'codec custom simple implémenté', '## Ce qu''il y a à faire

codec custom simple implémenté.

## Ce qui est attendu

Un dépôt public, avec le code, les tests et la documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('platform-app-developer', 'Discord bot serverless publié', 'Discord bot Skilluv-like publié + users', '## Ce qu''il y a à faire

Discord bot Skilluv-like publié + users.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('platform-app-developer', 'Slack app published + adopted', 'Slack app publié App Directory + orgs adoptées', '## Ce qu''il y a à faire

Slack app publié App Directory + orgs adoptées.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('platform-app-developer', 'Telegram bot production', 'Telegram bot production + users', '## Ce qu''il y a à faire

Telegram bot production + users.

## Ce qui est attendu

Quelque chose en service, avec une adresse où on peut le voir tourner.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL),
    ('platform-app-developer', 'Browser extension shipped', 'extension Chrome/Firefox publiée + 100+ users', '## Ce qu''il y a à faire

extension Chrome/Firefox publiée + 100+ users.

## Ce qui est attendu

Un paquet publié sur un registre public, avec sa version et sa documentation.

Avec, dans tous les cas : des tests qui décrivent le comportement
attendu, et une documentation permettant à un lecteur de lancer le
projet. Un code sans documentation est refusé.

## Ce qui sera regardé

La grille de revue de la famille s''applique, et elle est publique :
tu peux la lire avant de soumettre.', NULL)
) AS c(orientation_slug, title, description, instructions, language)
JOIN orientations o ON o.slug = c.orientation_slug
CROSS JOIN LATERAL (
    SELECT CASE o.reviewer_group
        WHEN 'compilers'  THEN 5
        WHEN 'systems'    THEN 5
        WHEN 'data'       THEN 4
        WHEN 'scientific' THEN 4
        WHEN 'blockchain' THEN 4
        ELSE 3
    END AS difficulty
) AS d;
