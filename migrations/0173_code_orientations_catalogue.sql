-- The `code` catalogue, from nine entries to thirty-three.
--
-- ## What was wrong with nine
--
-- "Développeur Backend" is not a trade, it is a family of them. Someone who
-- writes a database engine and someone who wires a REST API both answered to
-- it, which made the orientation useless for the only thing it exists for:
-- saying what a person actually does. Thirty-three named trades do that.
--
-- ## Naming
--
-- The default locale carries the French name, the way the eight seeded in
-- migration 0088 already do. English lives in `orientation_translations`.
-- Both are written here rather than left for later, because an orientation
-- with no English name is invisible to half the audience and nothing in the
-- product would ever surface the omission.
--
-- ## What happens to the old slugs
--
-- Eight are archived, each pointing at what it became. `dev-embarque-iot`
-- is among them — it was added in migration 0105, after the backlog that
-- planned this was written, and would otherwise have been duplicated by
-- `firmware-embedded-developer`. `systems-programmer` is untouched: it names
-- a real trade at the right granularity.

-- ═══════════════════════════════════════════════════════════════════
-- The thirty-two new trades
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO orientations (slug, name, description, primary_domain, secondary_domains, tags, is_curated) VALUES

-- ── Web (5) ────────────────────────────────────────────────────────
('web-frontend-developer', 'Développeur Web Frontend',
 'React, Vue ou Svelte, TypeScript, CSS moderne. Construit ce que la personne voit et manipule.',
 'code', ARRAY['design'], ARRAY['web'], TRUE),

('web-backend-developer', 'Développeur Web Backend',
 'API REST ou GraphQL, bases relationnelles, authentification. Rust, Go, Node, Python.',
 'code', ARRAY['ops'], ARRAY['web','api'], TRUE),

('web-fullstack-developer', 'Développeur Web Fullstack',
 'Front et back en T-shaped. Autonome sur un produit web de bout en bout.',
 'code', ARRAY['design','ops'], ARRAY['web'], TRUE),

('web-performance-engineer', 'Ingénieur Performance Web',
 'Core Web Vitals, budget de bundle, cache, profilage. Rend mesurable ce qui était ressenti.',
 'code', ARRAY['ops'], ARRAY['web','perf'], TRUE),

('web3-frontend-developer', 'Développeur Frontend Web3',
 'Connexion de portefeuille, signature de transactions, lecture on-chain. Interfaces sur lesquelles on engage de l''argent.',
 'code', ARRAY['design','security'], ARRAY['web3','dapp'], TRUE),

-- ── Mobile (3) ─────────────────────────────────────────────────────
('mobile-ios-developer', 'Développeur iOS',
 'Swift, SwiftUI, écosystème Apple. Cycle de vie, permissions, publication App Store.',
 'code', ARRAY['design'], ARRAY['mobile'], TRUE),

('mobile-android-developer', 'Développeur Android',
 'Kotlin, Jetpack Compose, écosystème Google. Fragmentation matérielle et publication Play.',
 'code', ARRAY['design'], ARRAY['mobile'], TRUE),

('mobile-cross-platform-developer', 'Développeur Mobile Cross-Platform',
 'Flutter ou React Native. Une base de code, deux magasins, et les compromis que cela impose.',
 'code', ARRAY['design'], ARRAY['mobile'], TRUE),

-- ── Desktop et logiciel d''entreprise (3) ───────────────────────────
('desktop-app-developer', 'Développeur Application Desktop',
 'Tauri, Electron ou natif. Installation, mise à jour automatique, intégration au système.',
 'code', ARRAY['design'], ARRAY['desktop'], TRUE),

('enterprise-software-developer', 'Développeur Logiciel d''Entreprise',
 'SSO, multi-tenant, audit, provisioning. Contraintes d''organisation avant contraintes techniques.',
 'code', ARRAY['ops','security'], ARRAY['enterprise'], TRUE),

('lowcode-platform-developer', 'Développeur Plateforme Low-Code',
 'Extensions, connecteurs et scripts sur Retool, Airtable, n8n. Automatiser sans tout réécrire.',
 'code', ARRAY['design'], ARRAY['lowcode'], TRUE),

-- ── Systèmes et bas niveau (4) ─────────────────────────────────────
('kernel-driver-developer', 'Développeur Noyau et Pilotes',
 'Espace noyau Linux, pilotes de périphériques, appels système. Là où une erreur arrête la machine.',
 'code', ARRAY['ops','security'], ARRAY['low-level','os'], TRUE),

('firmware-embedded-developer', 'Développeur Firmware et Embarqué',
 'Microcontrôleurs, capteurs, protocoles industriels, basse consommation. Contraint par la mémoire et l''énergie.',
 'code', ARRAY['ops'], ARRAY['embedded','iot'], TRUE),

('robotics-software-developer', 'Développeur Logiciel Robotique',
 'ROS, cinématique, fusion de capteurs, boucles de contrôle. Du code qui déplace de la matière.',
 'code', ARRAY['ai'], ARRAY['robotics'], TRUE),

('safety-critical-developer', 'Développeur Systèmes Critiques',
 'Avionique, médical, ferroviaire. Normes DO-178C ou IEC 61508, traçabilité exigence-code-test.',
 'code', ARRAY['security'], ARRAY['safety','regulated'], TRUE),

-- ── Blockchain (2) ─────────────────────────────────────────────────
('smart-contract-developer', 'Développeur Smart Contracts',
 'Solidity ou Cairo, contrats on-chain, audit de sécurité. Un déploiement ne se corrige pas.',
 'code', ARRAY['security'], ARRAY['blockchain'], TRUE),

('blockchain-protocol-developer', 'Développeur Protocole Blockchain',
 'Consensus, couche réseau pair-à-pair, exécution. Construit la chaîne, pas ce qui tourne dessus.',
 'code', ARRAY['security'], ARRAY['blockchain','crypto'], TRUE),

-- ── Compilation et méthodes formelles (2) ──────────────────────────
('compiler-language-developer', 'Développeur Compilateur et Langage',
 'Analyse lexicale et syntaxique, typage, LLVM, optimisation. Outils dont dépendent les autres outils.',
 'code', ARRAY[]::TEXT[], ARRAY['compiler'], TRUE),

('formal-methods-developer', 'Développeur Méthodes Formelles',
 'TLA+, Coq, model checking, preuve de propriétés. Démontre au lieu de tester.',
 'code', ARRAY['security'], ARRAY['formal'], TRUE),

-- ── Données et recherche (4) ───────────────────────────────────────
('database-engine-developer', 'Développeur Moteur de Base de Données',
 'Moteur de stockage, planificateur de requêtes, transactions, WAL. Construit la base, ne l''utilise pas.',
 'code', ARRAY['ops'], ARRAY['database'], TRUE),

('search-engine-developer', 'Développeur Moteur de Recherche',
 'Index inversé, analyse linguistique, pertinence, Lucene ou Tantivy. Trouver vite dans beaucoup.',
 'code', ARRAY['ai'], ARRAY['search'], TRUE),

('distributed-systems-developer', 'Développeur Systèmes Distribués',
 'Consensus, partitionnement, idempotence, tolérance aux pannes. Raisonne sur ce qui tombe.',
 'code', ARRAY['ops'], ARRAY['distributed'], TRUE),

('stream-processing-developer', 'Développeur Traitement de Flux',
 'Kafka, Flink, fenêtrage, exactly-once. Traite ce qui arrive sans jamais s''arrêter.',
 'code', ARRAY['ops','ai'], ARRAY['streaming'], TRUE),

-- ── Calcul scientifique et haute performance (3) ────────────────────
('scientific-computing-developer', 'Développeur Calcul Scientifique',
 'Simulation numérique, algèbre linéaire, MPI, reproductibilité. Le code est un instrument de mesure.',
 'code', ARRAY['ai'], ARRAY['scientific','hpc'], TRUE),

('gpu-compute-developer', 'Développeur Calcul GPU',
 'CUDA, kernels, hiérarchie mémoire, occupation. Pense en milliers de fils simultanés.',
 'code', ARRAY['ai'], ARRAY['gpu','cuda'], TRUE),

('hft-quant-developer', 'Développeur Quantitatif et Haute Fréquence',
 'Latence à la microseconde, carnet d''ordres, backtesting, gestion du risque. La lenteur coûte de l''argent.',
 'code', ARRAY['ai'], ARRAY['fintech','quant'], TRUE),

-- ── Réseau (1) ─────────────────────────────────────────────────────
('network-protocol-developer', 'Développeur Protocoles Réseau',
 'TCP, QUIC, TLS, implémentation de RFC, capture et analyse. Sous la couche applicative.',
 'code', ARRAY['ops','security'], ARRAY['networking'], TRUE),

-- ── Outils de développement (3) ────────────────────────────────────
('cli-tools-developer', 'Développeur Outils en Ligne de Commande',
 'Ergonomie du terminal, distribution multi-plateforme, scripts. Outils que d''autres exécutent mille fois.',
 'code', ARRAY['ops'], ARRAY['devtools'], TRUE),

('ide-extension-developer', 'Développeur Extensions IDE',
 'Language Server Protocol, extensions VS Code ou JetBrains, coloration et diagnostics.',
 'code', ARRAY['design'], ARRAY['devtools','ide'], TRUE),

('build-system-developer', 'Développeur Systèmes de Build',
 'Bazel, Nix, caches distribués, builds reproductibles. Fait passer une compilation de vingt minutes à deux.',
 'code', ARRAY['ops'], ARRAY['devtools','build'], TRUE),

-- ── Média et applications de plateforme (2) ────────────────────────
('media-processing-developer', 'Développeur Traitement Média',
 'FFmpeg, codecs, transcodage, streaming adaptatif. Manipule l''image et le son à l''échelle.',
 'code', ARRAY['design'], ARRAY['media'], TRUE),

('platform-app-developer', 'Développeur Applications de Plateforme',
 'Bots et intégrations Discord, Slack, Telegram. Vit dans le produit de quelqu''un d''autre.',
 'code', ARRAY['design'], ARRAY['platform','bots'], TRUE);

-- ═══════════════════════════════════════════════════════════════════
-- English
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO orientation_translations (orientation_id, locale, name, description)
SELECT o.id, 'en', t.name, t.description
FROM (VALUES
    ('web-frontend-developer', 'Web Frontend Developer',
     'React, Vue or Svelte, TypeScript, modern CSS. Builds what a person sees and touches.'),
    ('web-backend-developer', 'Web Backend Developer',
     'REST or GraphQL APIs, relational databases, authentication. Rust, Go, Node, Python.'),
    ('web-fullstack-developer', 'Web Fullstack Developer',
     'Front and back, T-shaped. Owns a web product end to end.'),
    ('web-performance-engineer', 'Web Performance Engineer',
     'Core Web Vitals, bundle budgets, caching, profiling. Makes measurable what was only felt.'),
    ('web3-frontend-developer', 'Web3 Frontend Developer',
     'Wallet connection, transaction signing, on-chain reads. Interfaces where people commit money.'),
    ('mobile-ios-developer', 'iOS Developer',
     'Swift, SwiftUI, the Apple ecosystem. Lifecycle, permissions, App Store review.'),
    ('mobile-android-developer', 'Android Developer',
     'Kotlin, Jetpack Compose, the Google ecosystem. Hardware fragmentation and Play releases.'),
    ('mobile-cross-platform-developer', 'Cross-Platform Mobile Developer',
     'Flutter or React Native. One codebase, two stores, and the trade-offs that implies.'),
    ('desktop-app-developer', 'Desktop App Developer',
     'Tauri, Electron or native. Installers, auto-update, operating system integration.'),
    ('enterprise-software-developer', 'Enterprise Software Developer',
     'SSO, multi-tenancy, audit trails, provisioning. Organisational constraints before technical ones.'),
    ('lowcode-platform-developer', 'Low-Code Platform Developer',
     'Extensions, connectors and scripts on Retool, Airtable, n8n. Automating without rewriting.'),
    ('kernel-driver-developer', 'Kernel and Driver Developer',
     'Linux kernel space, device drivers, system calls. Where a mistake stops the machine.'),
    ('firmware-embedded-developer', 'Firmware and Embedded Developer',
     'Microcontrollers, sensors, industrial protocols, low power. Bound by memory and energy.'),
    ('robotics-software-developer', 'Robotics Software Developer',
     'ROS, kinematics, sensor fusion, control loops. Code that moves matter.'),
    ('safety-critical-developer', 'Safety-Critical Software Developer',
     'Avionics, medical, rail. DO-178C or IEC 61508, requirement-to-code-to-test traceability.'),
    ('smart-contract-developer', 'Smart Contract Developer',
     'Solidity or Cairo, on-chain contracts, security audits. A deployment cannot be taken back.'),
    ('blockchain-protocol-developer', 'Blockchain Protocol Developer',
     'Consensus, peer-to-peer networking, execution. Builds the chain, not what runs on it.'),
    ('compiler-language-developer', 'Compiler and Language Developer',
     'Lexing, parsing, type systems, LLVM, optimisation. Tools every other tool depends on.'),
    ('formal-methods-developer', 'Formal Methods Developer',
     'TLA+, Coq, model checking, property proofs. Proves instead of testing.'),
    ('database-engine-developer', 'Database Engine Developer',
     'Storage engines, query planners, transactions, WAL. Builds the database rather than using it.'),
    ('search-engine-developer', 'Search Engine Developer',
     'Inverted indexes, linguistic analysis, relevance, Lucene or Tantivy. Finding fast among many.'),
    ('distributed-systems-developer', 'Distributed Systems Developer',
     'Consensus, partitioning, idempotency, fault tolerance. Reasons about what fails.'),
    ('stream-processing-developer', 'Stream Processing Developer',
     'Kafka, Flink, windowing, exactly-once. Handles what arrives without ever stopping.'),
    ('scientific-computing-developer', 'Scientific Computing Developer',
     'Numerical simulation, linear algebra, MPI, reproducibility. The code is a measuring instrument.'),
    ('gpu-compute-developer', 'GPU Compute Developer',
     'CUDA, kernels, memory hierarchy, occupancy. Thinks in thousands of simultaneous threads.'),
    ('hft-quant-developer', 'Quantitative and High-Frequency Developer',
     'Microsecond latency, order books, backtesting, risk. Slowness costs money.'),
    ('network-protocol-developer', 'Network Protocol Developer',
     'TCP, QUIC, TLS, RFC implementation, capture and analysis. Below the application layer.'),
    ('cli-tools-developer', 'CLI Tools Developer',
     'Terminal ergonomics, cross-platform distribution, scripting. Tools others run a thousand times.'),
    ('ide-extension-developer', 'IDE Extension Developer',
     'Language Server Protocol, VS Code or JetBrains extensions, highlighting and diagnostics.'),
    ('build-system-developer', 'Build System Developer',
     'Bazel, Nix, distributed caches, reproducible builds. Turns a twenty-minute build into two.'),
    ('media-processing-developer', 'Media Processing Developer',
     'FFmpeg, codecs, transcoding, adaptive streaming. Handles picture and sound at scale.'),
    ('platform-app-developer', 'Platform App Developer',
     'Discord, Slack and Telegram bots and integrations. Lives inside somebody else''s product.'),
    -- Seeded in migration 0088 and left as it is, because it names a real
    -- trade at the right granularity. It still needs its English name: a
    -- catalogue with thirty-two entries in English and one in French is a
    -- catalogue that looks broken.
    ('systems-programmer', 'Systems Programmer',
     'Rust, C++, low level, performance, memory. Works where the abstraction ends.')
) AS t(slug, name, description)
JOIN orientations o ON o.slug = t.slug;

-- ═══════════════════════════════════════════════════════════════════
-- The old slugs, and where their people went
-- ═══════════════════════════════════════════════════════════════════
--
-- Archived rather than deleted: `user_orientations` references these by id
-- and the history is an asset. `replaced_by` is what lets a search on the new
-- slug still reach a profile that carries the old one.

UPDATE orientations AS old
   SET is_archived = TRUE,
       replaced_by = new.id,
       updated_at = NOW()
  FROM (VALUES
    ('dev-frontend',       'web-frontend-developer'),
    ('dev-backend',        'web-backend-developer'),
    ('dev-fullstack',      'web-fullstack-developer'),
    ('mobile-ios',         'mobile-ios-developer'),
    ('mobile-android',     'mobile-android-developer'),
    ('mobile-cross',       'mobile-cross-platform-developer'),
    ('smart-contract-dev', 'smart-contract-developer'),
    ('dev-embarque-iot',   'firmware-embedded-developer')
  ) AS lineage(old_slug, new_slug)
  JOIN orientations AS new ON new.slug = lineage.new_slug
 WHERE old.slug = lineage.old_slug;
