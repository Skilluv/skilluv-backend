-- What each code trade is actually made of.
--
-- ## What `is_core` means here
--
-- Core is what the trade cannot exist without: remove it and the person is
-- doing something else. Recommended is what separates someone competent from
-- someone employable. Three to four core per orientation, the rest
-- recommended — a trade where everything is core says nothing about what to
-- learn first.
--
-- ## Why some rows name a family and others name a detail
--
-- The rule is: point at the coarsest node that is still specific to the
-- trade. A systems programmer needs Rust, the whole of it, so the row says
-- `rust` and the tree underneath describes what that contains. A GPU
-- developer does not need "GPU compute" — that is the trade's own name, and
-- saying it teaches nobody anything — so the rows say `cuda-kernels` and
-- `gpu-memory-hierarchy`.
--
-- This is why the twenty-three family roots added in 0174 are mostly unmapped
-- and their children are not. A root is a grouping; a person claims a skill,
-- not a grouping.
--
-- ## Why some rows point outside `code`
--
-- `distributed-tracing` lives under `ops` and `edge-ai` bridges into `ai`.
-- The map crosses domains on purpose: a distributed systems developer needs
-- tracing, and duplicating the node under `code` would give two answers to
-- whether someone knows it.
--
-- Weight is left at its default. It exists for later tuning of
-- recommendations, and inventing a spread now would be a number nobody
-- measured.

INSERT INTO orientation_skill_map (orientation_id, skill_id, is_core, is_recommended)
SELECT o.id, s.id, m.is_core, NOT m.is_core
FROM (VALUES

-- ── web-frontend-developer ─────────────────────────────────────────
('web-frontend-developer', 'frontend-frameworks', TRUE),
('web-frontend-developer', 'javascript-core', TRUE),
('web-frontend-developer', 'typescript', TRUE),
('web-frontend-developer', 'css-layout-flexbox', FALSE),
('web-frontend-developer', 'css-responsive-design', FALSE),
('web-frontend-developer', 'html-semantic', FALSE),
('web-frontend-developer', 'state-management-patterns', FALSE),
('web-frontend-developer', 'perf-core-web-vitals', FALSE),
('web-frontend-developer', 'e2e-testing-playwright', FALSE),

-- ── web-backend-developer ──────────────────────────────────────────
('web-backend-developer', 'backend-fundamentals', TRUE),
('web-backend-developer', 'rest-design', TRUE),
('web-backend-developer', 'databases', TRUE),
('web-backend-developer', 'graphql-schema-design', FALSE),
('web-backend-developer', 'postgres-indexing', FALSE),
('web-backend-developer', 'rate-limiting-backend', FALSE),
('web-backend-developer', 'idempotency', FALSE),
('web-backend-developer', 'webhook-design', FALSE),
('web-backend-developer', 'integration-testing', FALSE),

-- ── web-fullstack-developer ────────────────────────────────────────
('web-fullstack-developer', 'frontend-frameworks', TRUE),
('web-fullstack-developer', 'backend-fundamentals', TRUE),
('web-fullstack-developer', 'databases', TRUE),
('web-fullstack-developer', 'typescript', FALSE),
('web-fullstack-developer', 'rest-design', FALSE),
('web-fullstack-developer', 'sveltekit-form-actions', FALSE),
('web-fullstack-developer', 'stripe-integration', FALSE),
('web-fullstack-developer', 'e2e-testing-playwright', FALSE),

-- ── web-performance-engineer ───────────────────────────────────────
('web-performance-engineer', 'perf-core-web-vitals', TRUE),
('web-performance-engineer', 'perf-profiling', TRUE),
('web-performance-engineer', 'perf-frontend-bundle', TRUE),
('web-performance-engineer', 'perf-caching', FALSE),
('web-performance-engineer', 'perf-lazy-loading', FALSE),
('web-performance-engineer', 'perf-database-query', FALSE),
('web-performance-engineer', 'browser-networking', FALSE),
('web-performance-engineer', 'performance', FALSE),

-- ── web3-frontend-developer ────────────────────────────────────────
('web3-frontend-developer', 'wallet-integration', TRUE),
('web3-frontend-developer', 'frontend-frameworks', TRUE),
('web3-frontend-developer', 'onchain-data-indexing', TRUE),
('web3-frontend-developer', 'typescript', FALSE),
('web3-frontend-developer', 'smart-contract-security', FALSE),
('web3-frontend-developer', 'applied-cryptography', FALSE),
('web3-frontend-developer', 'js-error-handling', FALSE),

-- ── mobile-ios-developer ───────────────────────────────────────────
('mobile-ios-developer', 'swift-concurrency', TRUE),
('mobile-ios-developer', 'swiftui-layout', TRUE),
('mobile-ios-developer', 'ios-app-lifecycle', TRUE),
('mobile-ios-developer', 'ios-persistence', FALSE),
('mobile-ios-developer', 'uikit-interop', FALSE),
('mobile-ios-developer', 'app-store-release', FALSE),
('mobile-ios-developer', 'mobile-permissions', FALSE),
('mobile-ios-developer', 'mobile-push-notifications', FALSE),
('mobile-ios-developer', 'mobile-performance-profiling', FALSE),

-- ── mobile-android-developer ───────────────────────────────────────
('mobile-android-developer', 'kotlin-coroutines', TRUE),
('mobile-android-developer', 'jetpack-compose', TRUE),
('mobile-android-developer', 'android-app-lifecycle', TRUE),
('mobile-android-developer', 'android-persistence-room', FALSE),
('mobile-android-developer', 'play-store-release', FALSE),
('mobile-android-developer', 'mobile-permissions', FALSE),
('mobile-android-developer', 'mobile-push-notifications', FALSE),
('mobile-android-developer', 'mobile-performance-profiling', FALSE),

-- ── mobile-cross-platform-developer ────────────────────────────────
('mobile-cross-platform-developer', 'flutter-widgets', TRUE),
('mobile-cross-platform-developer', 'dart-language', TRUE),
('mobile-cross-platform-developer', 'react-native-bridge', TRUE),
('mobile-cross-platform-developer', 'expo-workflow', FALSE),
('mobile-cross-platform-developer', 'mobile-offline-sync', FALSE),
('mobile-cross-platform-developer', 'mobile-push-notifications', FALSE),
('mobile-cross-platform-developer', 'mobile-permissions', FALSE),
('mobile-cross-platform-developer', 'mobile-performance-profiling', FALSE),

-- ── desktop-app-developer ──────────────────────────────────────────
('desktop-app-developer', 'tauri-app', TRUE),
('desktop-app-developer', 'desktop-packaging-signing', TRUE),
('desktop-app-developer', 'os-integration', TRUE),
('desktop-app-developer', 'electron-app', FALSE),
('desktop-app-developer', 'native-desktop-toolkits', FALSE),
('desktop-app-developer', 'desktop-auto-update', FALSE),
('desktop-app-developer', 'rust', FALSE),

-- ── enterprise-software-developer ──────────────────────────────────
('enterprise-software-developer', 'enterprise-oidc-sso', TRUE),
('enterprise-software-developer', 'multi-tenant-isolation', TRUE),
('enterprise-software-developer', 'rbac-modelling', TRUE),
('enterprise-software-developer', 'scim-provisioning', FALSE),
('enterprise-software-developer', 'audit-logging-compliance', FALSE),
('enterprise-software-developer', 'data-residency', FALSE),
('enterprise-software-developer', 'db-migration-safety', FALSE),
('enterprise-software-developer', 'integration-testing', FALSE),

-- ── lowcode-platform-developer ─────────────────────────────────────
('lowcode-platform-developer', 'n8n-workflows', TRUE),
('lowcode-platform-developer', 'lowcode-custom-connectors', TRUE),
('lowcode-platform-developer', 'retool-apps', FALSE),
('lowcode-platform-developer', 'airtable-automation', FALSE),
('lowcode-platform-developer', 'lowcode-escape-hatch', FALSE),
('lowcode-platform-developer', 'rest-design', FALSE),
('lowcode-platform-developer', 'webhook-design', FALSE),

-- ── systems-programmer ─────────────────────────────────────────────
('systems-programmer', 'rust', TRUE),
('systems-programmer', 'rust-ownership', TRUE),
('systems-programmer', 'performance', TRUE),
('systems-programmer', 'rust-lifetimes', FALSE),
('systems-programmer', 'rust-async', FALSE),
('systems-programmer', 'perf-profiling', FALSE),
('systems-programmer', 'kernel-concurrency', FALSE),
('systems-programmer', 'property-based-testing', FALSE),

-- ── kernel-driver-developer ────────────────────────────────────────
('kernel-driver-developer', 'linux-kernel-modules', TRUE),
('kernel-driver-developer', 'device-drivers', TRUE),
('kernel-driver-developer', 'kernel-memory-management', TRUE),
('kernel-driver-developer', 'kernel-concurrency', FALSE),
('kernel-driver-developer', 'syscall-interface', FALSE),
('kernel-driver-developer', 'kernel-debugging', FALSE),
('kernel-driver-developer', 'rust', FALSE),
('kernel-driver-developer', 'performance', FALSE),

-- ── firmware-embedded-developer ────────────────────────────────────
('firmware-embedded-developer', 'microcontroller-programming', TRUE),
('firmware-embedded-developer', 'sensor-integration', TRUE),
('firmware-embedded-developer', 'low-power-networking', TRUE),
('firmware-embedded-developer', 'industrial-iot-protocols', FALSE),
('firmware-embedded-developer', 'edge-ai', FALSE),
('firmware-embedded-developer', 'kernel-concurrency', FALSE),
('firmware-embedded-developer', 'rust', FALSE),

-- ── robotics-software-developer ────────────────────────────────────
('robotics-software-developer', 'ros2-nodes', TRUE),
('robotics-software-developer', 'robot-kinematics', TRUE),
('robotics-software-developer', 'control-loops-pid', TRUE),
('robotics-software-developer', 'sensor-fusion', FALSE),
('robotics-software-developer', 'motion-planning', FALSE),
('robotics-software-developer', 'robotics-simulation', FALSE),
('robotics-software-developer', 'sensor-integration', FALSE),

-- ── safety-critical-developer ──────────────────────────────────────
('safety-critical-developer', 'requirements-traceability', TRUE),
('safety-critical-developer', 'do-178c-process', TRUE),
('safety-critical-developer', 'fault-tolerance-redundancy', TRUE),
('safety-critical-developer', 'iec-61508-sil', FALSE),
('safety-critical-developer', 'misra-c', FALSE),
('safety-critical-developer', 'static-analysis-certification', FALSE),
('safety-critical-developer', 'property-based-testing', FALSE),

-- ── smart-contract-developer ───────────────────────────────────────
('smart-contract-developer', 'solidity', TRUE),
('smart-contract-developer', 'smart-contract-security', TRUE),
('smart-contract-developer', 'evm-gas-optimisation', TRUE),
('smart-contract-developer', 'cairo-starknet', FALSE),
('smart-contract-developer', 'applied-cryptography', FALSE),
('smart-contract-developer', 'onchain-data-indexing', FALSE),
('smart-contract-developer', 'property-based-testing', FALSE),

-- ── blockchain-protocol-developer ──────────────────────────────────
('blockchain-protocol-developer', 'consensus-algorithms', TRUE),
('blockchain-protocol-developer', 'p2p-networking', TRUE),
('blockchain-protocol-developer', 'applied-cryptography', TRUE),
('blockchain-protocol-developer', 'consensus-raft-paxos', FALSE),
('blockchain-protocol-developer', 'rust', FALSE),
('blockchain-protocol-developer', 'performance', FALSE),
('blockchain-protocol-developer', 'failure-detection', FALSE),

-- ── compiler-language-developer ────────────────────────────────────
('compiler-language-developer', 'lexing-parsing', TRUE),
('compiler-language-developer', 'type-checking', TRUE),
('compiler-language-developer', 'code-generation', TRUE),
('compiler-language-developer', 'ast-design', FALSE),
('compiler-language-developer', 'llvm-ir', FALSE),
('compiler-language-developer', 'optimisation-passes', FALSE),
('compiler-language-developer', 'rust', FALSE),
('compiler-language-developer', 'property-based-testing', FALSE),

-- ── formal-methods-developer ───────────────────────────────────────
('formal-methods-developer', 'property-specification', TRUE),
('formal-methods-developer', 'model-checking', TRUE),
('formal-methods-developer', 'tla-plus', TRUE),
('formal-methods-developer', 'coq-proofs', FALSE),
('formal-methods-developer', 'smt-solvers', FALSE),
('formal-methods-developer', 'property-based-testing', FALSE),

-- ── database-engine-developer ──────────────────────────────────────
('database-engine-developer', 'storage-engines', TRUE),
('database-engine-developer', 'transaction-internals', TRUE),
('database-engine-developer', 'write-ahead-logging', TRUE),
('database-engine-developer', 'query-planning', FALSE),
('database-engine-developer', 'replication-internals', FALSE),
('database-engine-developer', 'db-transaction-isolation', FALSE),
('database-engine-developer', 'performance', FALSE),
('database-engine-developer', 'rust', FALSE),

-- ── search-engine-developer ────────────────────────────────────────
('search-engine-developer', 'inverted-index', TRUE),
('search-engine-developer', 'relevance-ranking', TRUE),
('search-engine-developer', 'text-analysis', TRUE),
('search-engine-developer', 'lucene-tantivy', FALSE),
('search-engine-developer', 'vector-search', FALSE),
('search-engine-developer', 'postgres-full-text', FALSE),
('search-engine-developer', 'performance', FALSE),

-- ── distributed-systems-developer ──────────────────────────────────
('distributed-systems-developer', 'consensus-raft-paxos', TRUE),
('distributed-systems-developer', 'partitioning-sharding', TRUE),
('distributed-systems-developer', 'replication-strategies', TRUE),
('distributed-systems-developer', 'failure-detection', FALSE),
('distributed-systems-developer', 'logical-clocks', FALSE),
('distributed-systems-developer', 'distributed-tracing', FALSE),
('distributed-systems-developer', 'idempotency', FALSE),
('distributed-systems-developer', 'grpc-service-design', FALSE),

-- ── stream-processing-developer ────────────────────────────────────
('stream-processing-developer', 'kafka-fundamentals', TRUE),
('stream-processing-developer', 'stream-windowing', TRUE),
('stream-processing-developer', 'exactly-once-semantics', TRUE),
('stream-processing-developer', 'flink-pipelines', FALSE),
('stream-processing-developer', 'backpressure-handling', FALSE),
('stream-processing-developer', 'idempotency', FALSE),
('stream-processing-developer', 'partitioning-sharding', FALSE),

-- ── scientific-computing-developer ─────────────────────────────────
('scientific-computing-developer', 'numerical-methods', TRUE),
('scientific-computing-developer', 'linear-algebra-computation', TRUE),
('scientific-computing-developer', 'simulation-reproducibility', TRUE),
('scientific-computing-developer', 'mpi-parallelism', FALSE),
('scientific-computing-developer', 'numpy-scipy-stack', FALSE),
('scientific-computing-developer', 'python', FALSE),
('scientific-computing-developer', 'performance', FALSE),

-- ── gpu-compute-developer ──────────────────────────────────────────
('gpu-compute-developer', 'cuda-kernels', TRUE),
('gpu-compute-developer', 'gpu-memory-hierarchy', TRUE),
('gpu-compute-developer', 'kernel-occupancy-tuning', TRUE),
('gpu-compute-developer', 'gpu-profiling', FALSE),
('gpu-compute-developer', 'opencl-webgpu', FALSE),
('gpu-compute-developer', 'linear-algebra-computation', FALSE),
('gpu-compute-developer', 'performance', FALSE),

-- ── hft-quant-developer ────────────────────────────────────────────
('hft-quant-developer', 'low-latency-programming', TRUE),
('hft-quant-developer', 'order-book-mechanics', TRUE),
('hft-quant-developer', 'backtesting-frameworks', TRUE),
('hft-quant-developer', 'market-data-feeds', FALSE),
('hft-quant-developer', 'risk-metrics', FALSE),
('hft-quant-developer', 'perf-profiling', FALSE),
('hft-quant-developer', 'tcp-internals', FALSE),

-- ── network-protocol-developer ─────────────────────────────────────
('network-protocol-developer', 'tcp-internals', TRUE),
('network-protocol-developer', 'rfc-implementation', TRUE),
('network-protocol-developer', 'tls-handshake', TRUE),
('network-protocol-developer', 'quic-http3', FALSE),
('network-protocol-developer', 'packet-capture-analysis', FALSE),
('network-protocol-developer', 'browser-networking', FALSE),
('network-protocol-developer', 'rust', FALSE),

-- ── cli-tools-developer ────────────────────────────────────────────
('cli-tools-developer', 'cli-ergonomics', TRUE),
('cli-tools-developer', 'cross-platform-distribution', TRUE),
('cli-tools-developer', 'rust', FALSE),
('cli-tools-developer', 'python-uv-workflow', FALSE),
('cli-tools-developer', 'integration-testing', FALSE),
('cli-tools-developer', 'reproducible-builds', FALSE),

-- ── ide-extension-developer ────────────────────────────────────────
('ide-extension-developer', 'language-server-protocol', TRUE),
('ide-extension-developer', 'editor-extension-apis', TRUE),
('ide-extension-developer', 'lexing-parsing', FALSE),
('ide-extension-developer', 'typescript', FALSE),
('ide-extension-developer', 'ast-design', FALSE),
('ide-extension-developer', 'performance', FALSE),

-- ── build-system-developer ─────────────────────────────────────────
('build-system-developer', 'build-graph-caching', TRUE),
('build-system-developer', 'reproducible-builds', TRUE),
('build-system-developer', 'bazel-nix-builds', TRUE),
('build-system-developer', 'cross-platform-distribution', FALSE),
('build-system-developer', 'performance', FALSE),

-- ── media-processing-developer ─────────────────────────────────────
('media-processing-developer', 'ffmpeg-pipelines', TRUE),
('media-processing-developer', 'video-codecs', TRUE),
('media-processing-developer', 'adaptive-bitrate-streaming', TRUE),
('media-processing-developer', 'audio-processing-code', FALSE),
('media-processing-developer', 'media-container-formats', FALSE),
('media-processing-developer', 'minio-s3-object-storage', FALSE),
('media-processing-developer', 'performance', FALSE),

-- ── platform-app-developer ─────────────────────────────────────────
('platform-app-developer', 'discord-bot-api', TRUE),
('platform-app-developer', 'platform-event-handling', TRUE),
('platform-app-developer', 'oauth-app-installation', TRUE),
('platform-app-developer', 'slack-app-framework', FALSE),
('platform-app-developer', 'telegram-bot-api', FALSE),
('platform-app-developer', 'webhook-signature-verify', FALSE),
('platform-app-developer', 'rate-limiting-backend', FALSE)

) AS m(orientation_slug, skill_slug, is_core)
JOIN orientations o ON o.slug = m.orientation_slug
JOIN skill_nodes s ON s.slug = m.skill_slug;
