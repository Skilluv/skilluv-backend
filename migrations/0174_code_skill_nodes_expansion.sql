-- The skills the new code trades are made of.
--
-- ## Why this migration exists at all
--
-- Migration 0173 named thirty-three trades. The skill catalogue behind them
-- covered web, Rust, Python and TypeScript — which is what Skilluv teaches
-- today, and roughly twenty of the thirty-three had nothing to attach to.
-- An orientation with an empty skill map looks supported and is not: someone
-- picks "Développeur Noyau et Pilotes" and the platform has no idea what that
-- involves, so it can recommend nothing and verify nothing.
--
-- ## What is deliberately not here
--
-- Depth beyond what can be stated accurately. Each node names concrete
-- technologies because a skill called "Advanced Kernel Concepts" is a label,
-- not a skill — nobody can tell whether they have it. Where a field is
-- regulated (avionics, medical), the nodes name the standards rather than
-- paraphrase them.

-- ═══════════════════════════════════════════════════════════════════
-- Roots
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain) VALUES
('swift-platform',        'Swift et plateforme Apple',      'Swift, SwiftUI, UIKit. Le langage et le cadre applicatif d''iOS.', 'code'),
('kotlin-platform',       'Kotlin et plateforme Android',   'Kotlin, Jetpack Compose. Le langage et le cadre applicatif d''Android.', 'code'),
('cross-platform-mobile', 'Mobile multiplateforme',         'Flutter, React Native. Une base de code pour deux magasins.', 'code'),
('mobile-craft',          'Métier du mobile',               'Ce qui vaut sur les deux plateformes : hors-ligne, permissions, notifications, batterie.', 'code'),
('desktop-runtimes',      'Applications de bureau',         'Tauri, Electron, toolkits natifs. Installation, mise à jour, intégration système.', 'code'),
('lowcode-platforms',     'Plateformes low-code',           'Retool, Airtable, n8n. Étendre et connecter plutôt que réécrire.', 'code'),
('kernel-development',    'Développement noyau',            'Espace noyau Linux, pilotes, appels système. Sans filet.', 'code'),
('robotics-software',     'Logiciel robotique',             'ROS, cinématique, contrôle. Du code qui déplace de la matière.', 'code'),
('safety-critical-engineering', 'Ingénierie des systèmes critiques', 'DO-178C, IEC 61508, traçabilité. Là où une défaillance blesse.', 'code'),
('blockchain-engineering', 'Ingénierie blockchain',         'Contrats on-chain, consensus, cryptographie appliquée.', 'code'),
('compiler-construction', 'Construction de compilateurs',   'Analyse, typage, génération de code, LLVM.', 'code'),
('formal-verification',   'Vérification formelle',          'TLA+, Coq, model checking. Démontrer plutôt que tester.', 'code'),
('database-internals',    'Internes des bases de données',  'Moteur de stockage, planificateur, transactions, journal.', 'code'),
('search-internals',      'Internes de la recherche',       'Index inversé, analyse linguistique, pertinence.', 'code'),
('distributed-systems',   'Systèmes distribués',            'Consensus, partitionnement, réplication, détection de panne.', 'code'),
('stream-processing',     'Traitement de flux',             'Kafka, Flink, fenêtrage, sémantiques de livraison.', 'code'),
('scientific-computing',  'Calcul scientifique',            'Méthodes numériques, algèbre linéaire, parallélisme, reproductibilité.', 'code'),
('gpu-compute',           'Calcul GPU',                     'CUDA, kernels, hiérarchie mémoire, occupation.', 'code'),
('quant-finance-engineering', 'Ingénierie quantitative',    'Carnet d''ordres, latence, backtesting, risque.', 'code'),
('network-protocols',     'Protocoles réseau',              'TCP, QUIC, TLS, implémentation de RFC.', 'code'),
('developer-tooling',     'Outillage de développement',     'CLI, serveurs de langage, extensions, systèmes de build.', 'code'),
('media-engineering',     'Ingénierie média',               'FFmpeg, codecs, transcodage, streaming adaptatif.', 'code'),
('platform-integrations-apps', 'Applications de plateforme', 'Bots et intégrations Discord, Slack, Telegram.', 'code');

-- ═══════════════════════════════════════════════════════════════════
-- Children
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT c.slug, c.display_name, c.description, 'code', p.id
FROM (VALUES

-- ── Apple ──────────────────────────────────────────────────────────
('swift-concurrency',        'Concurrence Swift',            'async/await, acteurs, Sendable. Éviter les courses sans bloquer l''interface.', 'swift-platform'),
('swiftui-layout',           'Mise en page SwiftUI',         'Vues déclaratives, modificateurs, cycle de rendu, previews.', 'swift-platform'),
('uikit-interop',            'Interopérabilité UIKit',       'Ponts UIViewRepresentable, code hérité, navigation mixte.', 'swift-platform'),
('ios-app-lifecycle',        'Cycle de vie iOS',             'États d''application, tâches en arrière-plan, restauration d''état.', 'swift-platform'),
('ios-persistence',          'Persistance iOS',              'SwiftData, Core Data, Keychain. Ce qui survit à la fermeture.', 'swift-platform'),
('app-store-release',        'Publication App Store',        'Signature, TestFlight, revue Apple, phased release.', 'swift-platform'),

-- ── Android ────────────────────────────────────────────────────────
('kotlin-coroutines',        'Coroutines Kotlin',            'suspend, Flow, portées structurées, annulation.', 'kotlin-platform'),
('jetpack-compose',          'Jetpack Compose',              'Composables, recomposition, état, thèmes Material.', 'kotlin-platform'),
('android-app-lifecycle',    'Cycle de vie Android',         'Activités, ViewModel, WorkManager, mort du processus.', 'kotlin-platform'),
('android-persistence-room', 'Persistance Android',          'Room, DataStore, migrations de schéma sur appareil.', 'kotlin-platform'),
('play-store-release',       'Publication Play Store',       'Signature, bundles, canaux de test, politiques Google.', 'kotlin-platform'),

-- ── Multiplateforme ────────────────────────────────────────────────
('dart-language',            'Langage Dart',                 'Typage sain, isolates, null safety.', 'cross-platform-mobile'),
('flutter-widgets',          'Widgets Flutter',              'Arbre de widgets, état, rendu personnalisé, plateformes cibles.', 'cross-platform-mobile'),
('react-native-bridge',      'Pont React Native',            'Modules natifs, nouvelle architecture, coût des passages de frontière.', 'cross-platform-mobile'),
('expo-workflow',            'Chaîne Expo',                  'EAS build, mises à jour over-the-air, modules natifs.', 'cross-platform-mobile'),

-- ── Métier mobile, commun aux plateformes ──────────────────────────
('mobile-offline-sync',      'Synchronisation hors-ligne',   'File d''écritures locale, résolution de conflits, reprise sur réseau instable.', 'mobile-craft'),
('mobile-push-notifications','Notifications mobiles',        'APNS, FCM, jetons, silencieuses, permission refusée.', 'mobile-craft'),
('mobile-permissions',       'Permissions et vie privée',    'Demandes contextuelles, refus définitif, étiquettes de confidentialité.', 'mobile-craft'),
('mobile-performance-profiling','Profilage mobile',          'Démarrage à froid, images sautées, batterie, mémoire.', 'mobile-craft'),

-- ── Bureau ─────────────────────────────────────────────────────────
('tauri-app',                'Applications Tauri',           'Cœur Rust, interface web, commandes, permissions.', 'desktop-runtimes'),
('electron-app',             'Applications Electron',        'Processus principal et rendu, IPC, empreinte mémoire.', 'desktop-runtimes'),
('native-desktop-toolkits',  'Toolkits natifs',              'Qt, GTK, Cocoa, WinUI. Aspect et comportement du système.', 'desktop-runtimes'),
('desktop-auto-update',      'Mise à jour automatique',      'Canaux de diffusion, différentiels, retour arrière.', 'desktop-runtimes'),
('desktop-packaging-signing','Empaquetage et signature',     'MSI, DMG, AppImage, notarisation, certificats.', 'desktop-runtimes'),
('os-integration',           'Intégration système',          'Zone de notification, système de fichiers, presse-papiers, démarrage.', 'desktop-runtimes'),

-- ── Low-code ───────────────────────────────────────────────────────
('retool-apps',              'Applications Retool',          'Requêtes, composants, permissions, outils internes.', 'lowcode-platforms'),
('airtable-automation',      'Automatisation Airtable',      'Formules, automatisations, scripts, limites d''API.', 'lowcode-platforms'),
('n8n-workflows',            'Workflows n8n',                'Nœuds, déclencheurs, reprise sur erreur, auto-hébergement.', 'lowcode-platforms'),
('lowcode-custom-connectors','Connecteurs sur mesure',       'Étendre une plateforme fermée par du code appelé depuis elle.', 'lowcode-platforms'),
('lowcode-escape-hatch',     'Sortie du low-code',           'Reconnaître la limite et migrer vers du code sans tout casser.', 'lowcode-platforms'),

-- ── Noyau ──────────────────────────────────────────────────────────
('linux-kernel-modules',     'Modules noyau Linux',          'Chargement, symboles exportés, interface avec l''espace utilisateur.', 'kernel-development'),
('device-drivers',           'Pilotes de périphériques',     'Modèle de périphérique, interruptions, DMA, arbre de périphériques.', 'kernel-development'),
('kernel-memory-management', 'Mémoire noyau',                'Allocateurs, pagination, copy_from_user, fuites qui ne pardonnent pas.', 'kernel-development'),
('syscall-interface',        'Interface d''appels système',  'Frontière utilisateur-noyau, validation, compatibilité ascendante.', 'kernel-development'),
('kernel-debugging',         'Débogage noyau',               'ftrace, kprobes, analyse de panique, débogage à distance.', 'kernel-development'),
('kernel-concurrency',       'Concurrence noyau',            'Verrous tournants, RCU, contextes d''interruption, préemption.', 'kernel-development'),

-- ── Robotique ──────────────────────────────────────────────────────
('ros2-nodes',               'Nœuds ROS 2',                  'Publication-souscription, services, actions, graphe de calcul.', 'robotics-software'),
('robot-kinematics',         'Cinématique',                  'Directe et inverse, repères, transformations.', 'robotics-software'),
('sensor-fusion',            'Fusion de capteurs',           'Filtre de Kalman, odométrie, recalage, dérive.', 'robotics-software'),
('motion-planning',          'Planification de trajectoire', 'Évitement d''obstacles, échantillonnage, contraintes.', 'robotics-software'),
('control-loops-pid',        'Boucles de contrôle',          'PID, réglage, stabilité, temps de réponse.', 'robotics-software'),
('robotics-simulation',      'Simulation robotique',         'Gazebo, physique, écart simulation-réalité.', 'robotics-software'),

-- ── Systèmes critiques ─────────────────────────────────────────────
('do-178c-process',          'Processus DO-178C',            'Niveaux DAL, objectifs, données de vie du logiciel avionique.', 'safety-critical-engineering'),
('iec-61508-sil',            'IEC 61508 et niveaux SIL',     'Analyse de risque, intégrité de sécurité, industriel et ferroviaire.', 'safety-critical-engineering'),
('requirements-traceability','Traçabilité des exigences',    'Exigence vers code vers test, couverture bidirectionnelle, preuve.', 'safety-critical-engineering'),
('misra-c',                  'MISRA C',                      'Sous-ensemble sûr du langage, justification des dérogations.', 'safety-critical-engineering'),
('static-analysis-certification','Analyse statique certifiée','Absence d''erreur d''exécution, interprétation abstraite, outils qualifiés.', 'safety-critical-engineering'),
('fault-tolerance-redundancy','Tolérance aux pannes',        'Redondance, vote, mode dégradé, comportement à la défaillance.', 'safety-critical-engineering'),

-- ── Blockchain ─────────────────────────────────────────────────────
('solidity',                 'Solidity',                     'Contrats EVM, stockage, modificateurs, événements.', 'blockchain-engineering'),
('smart-contract-security',  'Sécurité des smart contracts', 'Réentrance, dépassements, oracles, audits, post-mortems.', 'blockchain-engineering'),
('evm-gas-optimisation',     'Optimisation du gas',          'Coût du stockage, empaquetage, motifs coûteux.', 'blockchain-engineering'),
('cairo-starknet',           'Cairo et StarkNet',            'Preuves de validité, modèle de compte, rollup.', 'blockchain-engineering'),
('wallet-integration',       'Intégration de portefeuille',  'WalletConnect, signature, changement de réseau, refus utilisateur.', 'blockchain-engineering'),
('onchain-data-indexing',    'Indexation on-chain',          'The Graph, journaux d''événements, réorganisations de chaîne.', 'blockchain-engineering'),
('consensus-algorithms',     'Algorithmes de consensus',     'Preuve d''enjeu, BFT, finalité, incitations.', 'blockchain-engineering'),
('p2p-networking',           'Réseau pair-à-pair',           'Découverte, propagation, résistance à l''éclipse.', 'blockchain-engineering'),
('applied-cryptography',     'Cryptographie appliquée',      'Signatures, hachage, arbres de Merkle, courbes elliptiques.', 'blockchain-engineering'),

-- ── Compilation ────────────────────────────────────────────────────
('lexing-parsing',           'Analyse lexicale et syntaxique','Grammaires, descente récursive, récupération d''erreur.', 'compiler-construction'),
('ast-design',               'Conception d''AST',            'Représentation, visiteurs, positions source.', 'compiler-construction'),
('type-checking',            'Vérification de types',        'Inférence, unification, messages d''erreur lisibles.', 'compiler-construction'),
('llvm-ir',                  'LLVM IR',                      'Forme SSA, passes, adaptation à une cible.', 'compiler-construction'),
('code-generation',          'Génération de code',           'Sélection d''instructions, allocation de registres, ABI.', 'compiler-construction'),
('optimisation-passes',      'Passes d''optimisation',       'Élimination de code mort, inlining, mesure du gain réel.', 'compiler-construction'),

-- ── Méthodes formelles ─────────────────────────────────────────────
('tla-plus',                 'TLA+',                         'Spécification d''algorithmes concurrents, invariants, TLC.', 'formal-verification'),
('coq-proofs',               'Preuves Coq',                  'Assistants de preuve, tactiques, extraction de programme.', 'formal-verification'),
('model-checking',           'Model checking',               'Exploration d''états, explosion combinatoire, abstraction.', 'formal-verification'),
('property-specification',   'Spécification de propriétés',  'Sûreté, vivacité, équité. Dire ce qui doit rester vrai.', 'formal-verification'),
('smt-solvers',              'Solveurs SMT',                 'Z3, encodage de contraintes, limites de décidabilité.', 'formal-verification'),

-- ── Moteurs de base de données ─────────────────────────────────────
('storage-engines',          'Moteurs de stockage',          'Arbres B, LSM, compaction, format de page.', 'database-internals'),
('query-planning',           'Planification de requêtes',    'Estimation de cardinalité, choix de jointure, coût.', 'database-internals'),
('transaction-internals',    'Internes des transactions',    'Verrouillage, MVCC, niveaux d''isolation implémentés.', 'database-internals'),
('write-ahead-logging',      'Journalisation WAL',           'Durabilité, points de reprise, restauration après crash.', 'database-internals'),
('replication-internals',    'Réplication',                  'Flux physique et logique, retard, bascule.', 'database-internals'),

-- ── Recherche ──────────────────────────────────────────────────────
('inverted-index',           'Index inversé',                'Listes de postings, compression, fusion de segments.', 'search-internals'),
('text-analysis',            'Analyse de texte',             'Tokenisation, racinisation, langues non anglaises.', 'search-internals'),
('relevance-ranking',        'Classement par pertinence',    'BM25, pondération de champs, évaluation hors ligne.', 'search-internals'),
('lucene-tantivy',           'Lucene et Tantivy',            'Bibliothèques d''indexation, cycle de vie des segments.', 'search-internals'),
('vector-search',            'Recherche vectorielle',        'Plongements, ANN, HNSW, recherche hybride.', 'search-internals'),

-- ── Systèmes distribués ────────────────────────────────────────────
('consensus-raft-paxos',     'Consensus Raft et Paxos',      'Élection, réplication de journal, changement de membres.', 'distributed-systems'),
('partitioning-sharding',    'Partitionnement',              'Hachage cohérent, rééquilibrage, points chauds.', 'distributed-systems'),
('replication-strategies',   'Stratégies de réplication',    'Quorums, cohérence éventuelle, lecture de sa propre écriture.', 'distributed-systems'),
('failure-detection',        'Détection de panne',           'Battements de cœur, phi accrual, faux positifs.', 'distributed-systems'),
-- No `distributed-tracing` here: it already exists under `ops`, which is
-- where it belongs. The orientation reaches it through the skill map, which
-- crosses domains on purpose — duplicating the node would give two answers
-- to "does this person know tracing".
('logical-clocks',           'Horloges logiques',            'Horloges de Lamport, horloges vectorielles, dérive NTP, ordre causal.', 'distributed-systems'),

-- ── Flux ───────────────────────────────────────────────────────────
('kafka-fundamentals',       'Fondamentaux Kafka',           'Partitions, groupes de consommateurs, décalages, rétention.', 'stream-processing'),
('stream-windowing',         'Fenêtrage',                    'Fenêtres glissantes, filigranes, données en retard.', 'stream-processing'),
('exactly-once-semantics',   'Sémantique exactly-once',      'Transactions, idempotence, ce que la garantie ne couvre pas.', 'stream-processing'),
('flink-pipelines',          'Pipelines Flink',              'État, points de contrôle, reprise.', 'stream-processing'),
('backpressure-handling',    'Contre-pression',              'Débit amont supérieur à l''aval, files, abandon contrôlé.', 'stream-processing'),

-- ── Calcul scientifique ────────────────────────────────────────────
('numerical-methods',        'Méthodes numériques',          'Stabilité, conditionnement, erreur d''arrondi accumulée.', 'scientific-computing'),
('linear-algebra-computation','Algèbre linéaire calculatoire','BLAS, LAPACK, décompositions, matrices creuses.', 'scientific-computing'),
('mpi-parallelism',          'Parallélisme MPI',             'Découpage de domaine, communication collective, passage à l''échelle.', 'scientific-computing'),
('simulation-reproducibility','Reproductibilité',            'Graines, environnements figés, résultats rejouables.', 'scientific-computing'),
('numpy-scipy-stack',        'Pile NumPy et SciPy',          'Vectorisation, diffusion, coût des copies.', 'scientific-computing'),

-- ── GPU ────────────────────────────────────────────────────────────
('cuda-kernels',             'Kernels CUDA',                 'Grilles, blocs, fils, synchronisation.', 'gpu-compute'),
('gpu-memory-hierarchy',     'Hiérarchie mémoire GPU',       'Globale, partagée, registres, fusion des accès.', 'gpu-compute'),
('kernel-occupancy-tuning',  'Réglage de l''occupation',     'Registres par fil, taille de bloc, compromis mesurés.', 'gpu-compute'),
('gpu-profiling',            'Profilage GPU',                'Nsight, goulots mémoire contre calcul.', 'gpu-compute'),
('opencl-webgpu',            'OpenCL et WebGPU',             'Calcul portable hors écosystème NVIDIA.', 'gpu-compute'),

-- ── Quantitatif ────────────────────────────────────────────────────
('order-book-mechanics',     'Mécanique du carnet d''ordres','Types d''ordres, appariement, priorité prix-temps.', 'quant-finance-engineering'),
('low-latency-programming',  'Programmation basse latence',  'Absence d''allocation, cache, épinglage de cœur, gigue.', 'quant-finance-engineering'),
('backtesting-frameworks',   'Backtesting',                  'Biais de survivance, anticipation, coûts de transaction.', 'quant-finance-engineering'),
('market-data-feeds',        'Flux de données de marché',    'Protocoles, reconstruction, décalage d''horloge.', 'quant-finance-engineering'),
('risk-metrics',             'Métriques de risque',          'VaR, exposition, limites, coupe-circuits.', 'quant-finance-engineering'),

-- ── Réseau ─────────────────────────────────────────────────────────
('tcp-internals',            'Internes de TCP',              'Fenêtre, contrôle de congestion, retransmission.', 'network-protocols'),
('quic-http3',               'QUIC et HTTP/3',               'Flux, reprise de connexion, chiffrement intégré.', 'network-protocols'),
('tls-handshake',            'Poignée de main TLS',          'Chaînes de certificats, suites, épinglage, mTLS.', 'network-protocols'),
('rfc-implementation',       'Implémentation de RFC',        'Lire une spécification et en écrire une version interopérable.', 'network-protocols'),
('packet-capture-analysis',  'Capture et analyse',           'tcpdump, Wireshark, lecture d''une trace.', 'network-protocols'),

-- ── Outillage ──────────────────────────────────────────────────────
('cli-ergonomics',           'Ergonomie en ligne de commande','Sous-commandes, sorties lisibles par machine, codes de retour.', 'developer-tooling'),
('cross-platform-distribution','Distribution multiplateforme','Binaires statiques, gestionnaires de paquets, mise à jour.', 'developer-tooling'),
('language-server-protocol', 'Language Server Protocol',     'Complétion, diagnostics, navigation, analyse incrémentale.', 'developer-tooling'),
('editor-extension-apis',    'API d''extensions d''éditeur',  'VS Code, JetBrains, cycle de vie et limites d''API.', 'developer-tooling'),
('build-graph-caching',      'Cache de graphe de build',     'Empreintes d''entrées, cache distant, taux de réussite.', 'developer-tooling'),
('bazel-nix-builds',         'Bazel et Nix',                 'Déclaration hermétique des dépendances, bacs à sable.', 'developer-tooling'),
('reproducible-builds',      'Builds reproductibles',        'Horodatages, ordre, même entrée même sortie.', 'developer-tooling'),

-- ── Média ──────────────────────────────────────────────────────────
('ffmpeg-pipelines',         'Pipelines FFmpeg',             'Filtres, remultiplexage, traitement par lots.', 'media-engineering'),
('video-codecs',             'Codecs vidéo',                 'H.264, AV1, débit contre qualité, accélération matérielle.', 'media-engineering'),
('audio-processing-code',    'Traitement audio',             'Rééchantillonnage, normalisation, latence de traitement.', 'media-engineering'),
('adaptive-bitrate-streaming','Streaming adaptatif',         'HLS, DASH, segmentation, changement de qualité.', 'media-engineering'),
('media-container-formats',  'Formats de conteneur',         'MP4, MKV, pistes, métadonnées, synchronisation.', 'media-engineering'),

-- ── Applications de plateforme ─────────────────────────────────────
('discord-bot-api',          'API bots Discord',             'Passerelle, commandes applicatives, intentions, limites de débit.', 'platform-integrations-apps'),
('slack-app-framework',      'Applications Slack',           'Événements, vues modales, portées OAuth.', 'platform-integrations-apps'),
('telegram-bot-api',         'API bots Telegram',            'Interrogation longue contre webhook, claviers en ligne.', 'platform-integrations-apps'),
('platform-event-handling',  'Traitement d''événements',     'Livraisons dupliquées, ordre non garanti, reprise.', 'platform-integrations-apps'),
('oauth-app-installation',   'Installation OAuth',           'Flux d''installation, jetons par espace de travail, révocation.', 'platform-integrations-apps')

) AS c(slug, display_name, description, parent_slug)
JOIN skill_nodes p ON p.slug = c.parent_slug AND p.domain = 'code';

-- ═══════════════════════════════════════════════════════════════════
-- Enterprise software: three gaps next to what already exists
-- ═══════════════════════════════════════════════════════════════════
--
-- `enterprise-oidc-sso`, `scim-provisioning` and `multi-tenant-isolation`
-- were already seeded. These three complete the trade.

INSERT INTO skill_nodes (slug, display_name, description, domain) VALUES
('audit-logging-compliance', 'Journalisation d''audit',  'Qui a fait quoi et quand, immuable, exportable pour un auditeur.', 'code'),
('rbac-modelling',           'Modélisation des droits',  'Rôles, portées, héritage, principe du moindre privilège.', 'code'),
('data-residency',           'Résidence des données',    'Où les données vivent, ce qui peut traverser une frontière.', 'code');
