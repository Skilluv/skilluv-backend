//! skilluv-seed-projects — provision the repositories Skilluv draws work
//! from, wired to the P11 GitHubIngestor.
//!
//! Three catalogues:
//!
//!   * our own repos, where dogfooding happens;
//!   * the twelve partner repos of Annexe F, curated unilaterally — no
//!     permission is needed to point people at a public issue tracker;
//!   * the large ecosystem projects, which take contributions in every trade.
//!
//! Labels are per repository. Rust projects say `E-easy`, most say `good
//! first issue`, and our own say `skilluv-challenge`; one global list only
//! ever fitted the last of those.
//!
//! Phase 1 dogfooding (P26 v2 SKI-74): these 4 rows are what makes the P11
//! ingestor pick up issues from our own repos and materialise them as slices.
//!
//! Idempotent: uses `INSERT ... ON CONFLICT (slug) DO UPDATE` to refresh
//! `curated_labels` and `slice_ingestion_mode` on re-runs. Safe from CI /
//! Coolify post-deploy / manual command.
//!
//! Requires the admin user (see `bin/seed_admin.rs`) to already exist —
//! the projects are owned by that user. If the admin isn't found, the
//! binary fails with a clear error pointing to seed_admin.
//!
//! Usage:
//!   cargo run --bin skilluv-seed-projects
//!       # uses SEED_ADMIN_EMAIL or defaults to admin@skill-uv.com
//!   cargo run --bin skilluv-seed-projects -- --owner-email admin@example.com
//!
//! Env vars (used only when the matching CLI arg is missing):
//!   SEED_ADMIN_EMAIL       default: admin@skill-uv.com
//!   DATABASE_URL           standard sqlx connection string

use anyhow::{Context, Result};
use clap::Parser;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(
    name = "skilluv-seed-projects",
    about = "Seed the 4 Skilluv public repos as Projects (idempotent)"
)]
struct Cli {
    /// Email of the admin user that will own the seeded projects.
    /// Defaults to SEED_ADMIN_EMAIL env, then to admin@skill-uv.com.
    #[arg(long)]
    owner_email: Option<String>,
}

/// A repository Skilluv draws work from.
struct SeedProject {
    slug: &'static str,
    name: &'static str,
    description: &'static str,
    github_owner: &'static str,
    github_repo: &'static str,
    skill_domains: &'static [&'static str],
    tech_stack: &'static [&'static str],
    /// Issues carrying one of these become slices. Empty means the repo is
    /// listed but nothing is ingested from it yet.
    curated_labels: &'static [&'static str],
    /// Which trade an issue belongs to, by label. Written against whichever
    /// slug the catalogue used — `resolve_orientation` follows a rename, so
    /// entries naming `dev-frontend` land on the trade that replaced it.
    label_orientations: &'static [(&'static str, &'static str)],
    /// False for our own repos: we own those rather than curate them.
    curated: bool,
}

/// Our own repositories. Dogfooding: the platform is built by people using
/// the platform, and a challenge on our own backend is the shortest path
/// from "I want to contribute" to a merged pull request.
const SKILLUV_REPOS: &[SeedProject] = &[
    SeedProject {
        slug: "skilluv-backend",
        name: "Skilluv Backend",
        description: "Rust/axum backend powering the Skilluv platform.",
        github_owner: "skilluv",
        github_repo: "skilluv-backend",
        skill_domains: &["code", "ops"],
        tech_stack: &["rust", "axum", "postgres", "sqlx"],
        curated_labels: &["skilluv-challenge"],
        label_orientations: &[("skilluv-challenge", "web-backend-developer")],
        curated: false,
    },
    SeedProject {
        slug: "skilluv-frontend",
        name: "Skilluv Frontend",
        description: "SvelteKit frontend for the Skilluv user experience.",
        github_owner: "skilluv",
        github_repo: "skilluv-frontend",
        skill_domains: &["code", "design"],
        tech_stack: &["typescript", "sveltekit", "tailwind"],
        curated_labels: &["skilluv-challenge"],
        label_orientations: &[("skilluv-challenge", "web-frontend-developer")],
        curated: false,
    },
    SeedProject {
        slug: "skilluv-admin",
        name: "Skilluv Admin Panel",
        description: "SvelteKit admin panel (moderation, ops, analytics).",
        github_owner: "skilluv",
        github_repo: "skilluv-admin",
        skill_domains: &["code", "ops"],
        tech_stack: &["typescript", "sveltekit"],
        curated_labels: &["skilluv-challenge"],
        label_orientations: &[("skilluv-challenge", "web-frontend-developer")],
        curated: false,
    },
    SeedProject {
        slug: "skilluv-ia",
        name: "Skilluv IA",
        description: "AI/ML services (verifier, coach, embeddings).",
        github_owner: "skilluv",
        github_repo: "skilluv-ia",
        skill_domains: &["ai", "code"],
        tech_stack: &["python", "fastapi", "grpc"],
        curated_labels: &["skilluv-challenge"],
        // `skilluv-challenge` is deliberately not mapped here. It marks a
        // repository's issues as available, not what trade they belong to,
        // and mapping it to one would have filed every verifier fix under
        // backend development. The area labels say the trade, and this is our
        // own repository, so the convention is ours to keep.
        //
        // Two mapped labels pointing at different trades leave the slice
        // untyped by design — so an issue carries one area label, not three.
        label_orientations: &[
            ("area/llm", "llm-engineer"),
            ("area/prompt", "prompt-engineer"),
            ("area/mlops", "mlops-engineer"),
            ("area/safety", "ai-safety-researcher"),
            ("area/nlp", "nlp-engineer"),
            ("area/data", "data-engineer"),
        ],
        curated: false,
    },
    SeedProject {
        slug: "skilluv-discord-bot",
        name: "Skilluv Discord Bot",
        description: "Rust/serenity bot: onboarding, notifications, community rituals.",
        github_owner: "skilluv",
        github_repo: "skilluv-discord",
        skill_domains: &["code"],
        tech_stack: &["rust", "serenity"],
        curated_labels: &["skilluv-challenge"],
        label_orientations: &[("skilluv-challenge", "platform-app-developer")],
        curated: false,
    },
    SeedProject {
        slug: "skilluv-community-repos",
        name: "Skilluv Community Starters",
        description: "Starter repositories community members fork and extend.",
        github_owner: "skilluv",
        github_repo: "skilluv-community-repos",
        skill_domains: &["code"],
        tech_stack: &["typescript", "rust", "python"],
        curated_labels: &["skilluv-challenge"],
        label_orientations: &[("skilluv-challenge", "web-fullstack-developer")],
        curated: false,
    },
];

/// The twelve partner repositories of Annexe F. Curated unilaterally: no
/// permission is required to point somebody at a public issue tracker, and
/// the partnership conversation comes after there is something to show.
///
/// The orientations are the ones the annexe assigns, written with the slugs
/// it used — several were renamed in migration 0173, and the lineage handles
/// that rather than this list drifting from the document it came from.
const PARTNER_REPOS: &[SeedProject] = &[
    SeedProject {
        slug: "sqlx",
        name: "sqlx",
        description: "Async, compile-time checked SQL for Rust. Relation directe, et ce que Skilluv utilise.",
        github_owner: "launchbadge",
        github_repo: "sqlx",
        skill_domains: &["code"],
        tech_stack: &["rust", "postgres"],
        curated_labels: &["good first issue", "E-easy", "help wanted"],
        label_orientations: &[
            ("good first issue", "dev-backend"),
            ("E-easy", "dev-backend"),
            ("help wanted", "systems-programmer"),
        ],
        curated: true,
    },
    SeedProject {
        slug: "bevy",
        name: "Bevy Engine",
        description: "Moteur de jeu Rust, orienté données. Contributions gameplay, rendu et outils.",
        github_owner: "bevyengine",
        github_repo: "bevy",
        skill_domains: &["game", "code"],
        tech_stack: &["rust", "wgpu"],
        curated_labels: &["D-Good-First-Issue", "good first issue"],
        label_orientations: &[
            ("D-Good-First-Issue", "systems-programmer"),
            ("good first issue", "systems-programmer"),
        ],
        curated: true,
    },
    SeedProject {
        slug: "esp-hal",
        name: "esp-hal",
        description: "Couche d''abstraction matérielle Rust pour les puces ESP32.",
        github_owner: "esp-rs",
        github_repo: "esp-hal",
        skill_domains: &["code"],
        tech_stack: &["rust", "embedded"],
        curated_labels: &["good first issue", "help wanted"],
        label_orientations: &[
            ("good first issue", "dev-embarque-iot"),
            ("help wanted", "dev-embarque-iot"),
        ],
        curated: true,
    },
    SeedProject {
        slug: "cal-com",
        name: "Cal.com",
        description: "Planification open source. Culture explicite de recrutement parmi les contributeurs.",
        github_owner: "calcom",
        github_repo: "cal.com",
        skill_domains: &["code"],
        tech_stack: &["typescript", "nextjs", "prisma"],
        curated_labels: &["good first issue", "🐛 bug", "help wanted"],
        label_orientations: &[
            ("good first issue", "dev-frontend"),
            ("help wanted", "dev-fullstack"),
            ("🐛 bug", "dev-backend"),
        ],
        curated: true,
    },
    SeedProject {
        slug: "excalidraw",
        name: "Excalidraw",
        description: "Tableau blanc collaboratif. Contributions visibles immédiatement à l''écran.",
        github_owner: "excalidraw",
        github_repo: "excalidraw",
        skill_domains: &["code", "design"],
        tech_stack: &["typescript", "react", "canvas"],
        curated_labels: &["good first issue", "help wanted", "design", "ui"],
        label_orientations: &[
            ("good first issue", "dev-frontend"),
            ("help wanted", "dev-frontend"),
            ("design", "design-product"),
            ("ui", "design-product"),
        ],
        curated: true,
    },
    SeedProject {
        slug: "astro",
        name: "Astro",
        description: "Framework web orienté contenu. Utilisé par les livrables Skilluv.",
        github_owner: "withastro",
        github_repo: "astro",
        skill_domains: &["code"],
        tech_stack: &["typescript"],
        curated_labels: &["good first issue", "help wanted"],
        label_orientations: &[
            ("good first issue", "dev-frontend"),
            ("help wanted", "dev-fullstack"),
        ],
        curated: true,
    },
    SeedProject {
        slug: "fastapi",
        name: "FastAPI",
        description: "Framework web Python typé. Synergie avec le tutoriel FastAPI francophone.",
        github_owner: "tiangolo",
        github_repo: "fastapi",
        skill_domains: &["code"],
        tech_stack: &["python"],
        curated_labels: &["good first issue", "help wanted"],
        label_orientations: &[
            ("good first issue", "dev-backend"),
            ("help wanted", "dev-backend"),
        ],
        curated: true,
    },
    SeedProject {
        slug: "hf-transformers",
        name: "HuggingFace Transformers",
        description: "Bibliothèque de modèles. Porte d''entrée pour les initiatives sur les langues africaines.",
        github_owner: "huggingface",
        github_repo: "transformers",
        skill_domains: &["ai", "code"],
        tech_stack: &["python", "pytorch"],
        curated_labels: &["good first issue", "Good Second Issue"],
        label_orientations: &[
            ("good first issue", "scientific-computing-developer"),
            ("Good Second Issue", "scientific-computing-developer"),
        ],
        curated: true,
    },
    SeedProject {
        slug: "tldraw",
        name: "tldraw",
        description: "Tableau blanc infini, et un design system tenu de bout en bout. Le dépôt où une décision d'interface se lit dans le code.",
        github_owner: "tldraw",
        github_repo: "tldraw",
        skill_domains: &["design", "code"],
        tech_stack: &["typescript", "react", "canvas"],
        curated_labels: &["design", "ux", "accessibility", "good first issue"],
        label_orientations: &[
            ("design", "design-product"),
            ("ux", "design-product"),
            ("accessibility", "design-product"),
            ("good first issue", "dev-frontend"),
        ],
        curated: true,
    },
    SeedProject {
        slug: "storybook",
        name: "Storybook",
        description: "L'atelier où un design system se documente et se teste. Beaucoup de travail d'interface et d'accessibilité, peu de gens pour le faire.",
        github_owner: "storybookjs",
        github_repo: "storybook",
        skill_domains: &["design", "code"],
        tech_stack: &["typescript", "react", "vite"],
        curated_labels: &["ui", "accessibility", "documentation", "good first issue"],
        label_orientations: &[
            ("ui", "design-system"),
            ("accessibility", "design-system"),
            ("documentation", "design-ux-writing"),
            ("good first issue", "dev-frontend"),
        ],
        curated: true,
    },
    SeedProject {
        slug: "penpot",
        name: "Penpot",
        description: "Conception d''interfaces open source. Ligne éditoriale Skilluv sur le design libre.",
        github_owner: "penpot",
        github_repo: "penpot",
        skill_domains: &["design", "code"],
        tech_stack: &["clojurescript", "react"],
        curated_labels: &["good first issue", "help wanted", "design", "ux"],
        label_orientations: &[
            ("good first issue", "dev-frontend"),
            ("help wanted", "dev-backend"),
            // The point of curating a design tool: its design issues have to
            // reach designers, or the repo is just another frontend backlog.
            ("design", "design-product"),
            ("ux", "design-product"),
        ],
        curated: true,
    },
    SeedProject {
        slug: "mdn-content",
        name: "MDN Web Docs",
        description: "La documentation web de référence. La barrière technique la plus basse du catalogue, pour un impact large.",
        github_owner: "mdn",
        github_repo: "content",
        skill_domains: &["code"],
        tech_stack: &["markdown"],
        curated_labels: &["good first issue", "help wanted"],
        label_orientations: &[
            ("good first issue", "dev-frontend"),
            ("help wanted", "dev-frontend"),
        ],
        curated: true,
    },
    SeedProject {
        slug: "africastalking-python",
        name: "Africa''s Talking Python",
        description: "SDK Python pour SMS et USSD. Infrastructure critique sur le continent.",
        github_owner: "AfricasTalkingLtd",
        github_repo: "africastalking-python",
        skill_domains: &["code"],
        tech_stack: &["python"],
        curated_labels: &["good first issue", "help wanted"],
        label_orientations: &[
            ("good first issue", "dev-backend"),
            ("help wanted", "dev-backend"),
        ],
        curated: true,
    },
    SeedProject {
        slug: "masakhane-ner",
        name: "Masakhane NER",
        description: "Reconnaissance d''entités pour les langues africaines. Travail de recherche ouvert.",
        github_owner: "masakhane-io",
        github_repo: "masakhane-ner",
        skill_domains: &["ai", "code"],
        tech_stack: &["python", "jupyter"],
        curated_labels: &["good first issue", "help wanted"],
        label_orientations: &[
            ("good first issue", "scientific-computing-developer"),
            ("help wanted", "scientific-computing-developer"),
        ],
        curated: true,
    },
];

/// Large ecosystem projects, which take contributions across every trade.
///
/// Listed with no curated labels on purpose: their issue volume is enormous,
/// their contribution processes differ wildly — the Linux kernel does not use
/// GitHub issues at all — and ingesting them blindly would bury the partner
/// repos under thousands of tickets nobody vetted. They are here so the
/// catalogue names them and an operator can enable one deliberately.
const ECOSYSTEM_REPOS: &[SeedProject] = &[
    SeedProject {
        slug: "rust-lang",
        name: "Rust",
        description: "Le langage lui-même : compilateur, bibliothèque standard, outils.",
        github_owner: "rust-lang",
        github_repo: "rust",
        skill_domains: &["code"],
        tech_stack: &["rust"],
        curated_labels: &[],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "cpython",
        name: "CPython",
        description: "L''implémentation de référence de Python.",
        github_owner: "python",
        github_repo: "cpython",
        skill_domains: &["code"],
        tech_stack: &["c", "python"],
        curated_labels: &[],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "nodejs",
        name: "Node.js",
        description: "Le runtime JavaScript serveur.",
        github_owner: "nodejs",
        github_repo: "node",
        skill_domains: &["code"],
        tech_stack: &["c++", "javascript"],
        curated_labels: &[],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "postgres",
        name: "PostgreSQL",
        description: "Le moteur de base de données. Contributions par liste de diffusion, pas par pull request.",
        github_owner: "postgres",
        github_repo: "postgres",
        skill_domains: &["code"],
        tech_stack: &["c"],
        curated_labels: &[],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "linux-kernel",
        name: "Linux",
        description: "Le noyau. Contributions par correctifs sur liste de diffusion ; le processus est strict et documenté.",
        github_owner: "torvalds",
        github_repo: "linux",
        skill_domains: &["code"],
        tech_stack: &["c"],
        curated_labels: &[],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "godot",
        name: "Godot Engine",
        description: "Moteur de jeu libre. Gameplay, rendu, éditeur.",
        github_owner: "godotengine",
        github_repo: "godot",
        skill_domains: &["game", "code"],
        tech_stack: &["c++"],
        curated_labels: &[],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "vscode",
        name: "Visual Studio Code",
        description: "L''éditeur et son protocole de serveur de langage.",
        github_owner: "microsoft",
        github_repo: "vscode",
        skill_domains: &["code"],
        tech_stack: &["typescript"],
        curated_labels: &[],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "kubernetes",
        name: "Kubernetes",
        description: "L''orchestrateur de conteneurs.",
        github_owner: "kubernetes",
        github_repo: "kubernetes",
        skill_domains: &["ops", "code"],
        tech_stack: &["go"],
        curated_labels: &[],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "deno",
        name: "Deno",
        description: "Runtime JavaScript et TypeScript écrit en Rust.",
        github_owner: "denoland",
        github_repo: "deno",
        skill_domains: &["code"],
        tech_stack: &["rust", "typescript"],
        curated_labels: &[],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "ruff",
        name: "Ruff",
        description: "Analyseur et formateur Python écrit en Rust.",
        github_owner: "astral-sh",
        github_repo: "ruff",
        skill_domains: &["code"],
        tech_stack: &["rust", "python"],
        curated_labels: &[],
        label_orientations: &[],
        curated: true,
    },
];

/// The AI ecosystem.
///
/// Separate from `ECOSYSTEM_REPOS` because the domain differs, not because
/// the curation rules do: these are public trackers, and pointing people at a
/// public tracker needs nobody's permission.
///
/// `label_orientations` is empty throughout, and that is the honest state
/// rather than an unfinished one. On a repository we do not own, `good first
/// issue` means "small", not "vision" — an issue so labelled on Transformers
/// is as likely to be documentation as a tokeniser fix. Guessing would credit
/// somebody with a speciality they never worked in, and the mapping is
/// per-project in the admin panel precisely so a maintainer relationship can
/// fill it in later with real knowledge.
const AI_REPOS: &[SeedProject] = &[
    SeedProject {
        slug: "hf-transformers",
        name: "HuggingFace Transformers",
        description: "La bibliothèque par laquelle passe presque tout modèle publié.",
        github_owner: "huggingface",
        github_repo: "transformers",
        skill_domains: &["ai", "code"],
        tech_stack: &["python", "pytorch"],
        curated_labels: &["Good First Issue"],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "hf-datasets",
        name: "HuggingFace Datasets",
        description: "Chargement et partage de jeux de données, du prototype au téraoctet.",
        github_owner: "huggingface",
        github_repo: "datasets",
        skill_domains: &["ai", "code"],
        tech_stack: &["python"],
        curated_labels: &["good first issue"],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "hf-diffusers",
        name: "HuggingFace Diffusers",
        description: "Les chaînes de diffusion : génération d'images, ControlNet, LoRA.",
        github_owner: "huggingface",
        github_repo: "diffusers",
        skill_domains: &["ai", "design"],
        tech_stack: &["python", "pytorch"],
        curated_labels: &["good first issue"],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "candle",
        name: "Candle",
        description: "Inférence en Rust, sans Python à l'exécution. Le pont entre nos deux écosystèmes.",
        github_owner: "huggingface",
        github_repo: "candle",
        skill_domains: &["ai", "code"],
        tech_stack: &["rust"],
        curated_labels: &[],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "pytorch",
        name: "PyTorch",
        description: "Le cadre de calcul sous presque toute la recherche publiée.",
        github_owner: "pytorch",
        github_repo: "pytorch",
        skill_domains: &["ai", "code"],
        tech_stack: &["python", "c++"],
        curated_labels: &[],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "jax",
        name: "JAX",
        description: "Différentiation et compilation. Exigeant, et rapide quand le calcul est le goulot.",
        github_owner: "jax-ml",
        github_repo: "jax",
        skill_domains: &["ai", "code"],
        tech_stack: &["python"],
        curated_labels: &[],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "vllm",
        name: "vLLM",
        description: "Servir un modèle de langage avec un débit sérieux.",
        github_owner: "vllm-project",
        github_repo: "vllm",
        skill_domains: &["ai", "ops"],
        tech_stack: &["python", "cuda"],
        curated_labels: &["good first issue"],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "langchain",
        name: "LangChain",
        description: "Chaînes et agents. Très fréquenté, donc des relectures rapides.",
        github_owner: "langchain-ai",
        github_repo: "langchain",
        skill_domains: &["ai", "code"],
        tech_stack: &["python"],
        curated_labels: &["good first issue"],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "llamaindex",
        name: "LlamaIndex",
        description: "Indexation et récupération pour RAG.",
        github_owner: "run-llama",
        github_repo: "llama_index",
        skill_domains: &["ai", "code"],
        tech_stack: &["python"],
        curated_labels: &["good first issue"],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "dspy",
        name: "DSPy",
        description: "Optimiser des invites par mesure plutôt que par intuition.",
        github_owner: "stanfordnlp",
        github_repo: "dspy",
        skill_domains: &["ai"],
        tech_stack: &["python"],
        curated_labels: &[],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "masakhane-mt",
        name: "Masakhane",
        description: "TAL pour les langues africaines, mené depuis le continent. Le terrain le plus proche de nous.",
        github_owner: "masakhane-io",
        github_repo: "masakhane-mt",
        skill_domains: &["ai"],
        tech_stack: &["python"],
        curated_labels: &[],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "evidently",
        name: "Evidently",
        description: "Détection de dérive et rapports de qualité pour modèles en production.",
        github_owner: "evidentlyai",
        github_repo: "evidently",
        skill_domains: &["ai", "ops"],
        tech_stack: &["python"],
        curated_labels: &["good first issue"],
        label_orientations: &[],
        curated: true,
    },
    SeedProject {
        slug: "dbt-core",
        name: "dbt Core",
        description: "Transformations versionnées et testées dans l'entrepôt.",
        github_owner: "dbt-labs",
        github_repo: "dbt-core",
        skill_domains: &["ai", "code"],
        tech_stack: &["python", "sql"],
        curated_labels: &["good_first_issue"],
        label_orientations: &[],
        curated: true,
    },
];

/// Everything, in the order it should be seeded.
const ALL_PROJECTS: &[&[SeedProject]] = &[
    SKILLUV_REPOS,
    PARTNER_REPOS,
    ECOSYSTEM_REPOS,
    AI_REPOS,
];

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .compact()
        .init();

    let cli = Cli::parse();
    let owner_email = cli
        .owner_email
        .or_else(|| std::env::var("SEED_ADMIN_EMAIL").ok())
        .unwrap_or_else(|| "admin@skill-uv.com".to_string())
        .to_lowercase();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
    let db = PgPool::connect(&database_url)
        .await
        .context("failed to connect to Postgres")?;

    let owner_id: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM users WHERE email = $1 AND role = 'admin'")
            .bind(&owner_email)
            .fetch_optional(&db)
            .await
            .context("failed to look up admin user")?;

    let owner_id = owner_id
        .ok_or_else(|| {
            anyhow::anyhow!(
                "admin user {owner_email} not found. Run `cargo run --bin skilluv-seed-admin` \
                 first (with SEED_ADMIN_PASSWORD set)."
            )
        })?
        .0;

    let mut created = 0usize;
    let mut updated = 0usize;

    for repo in ALL_PROJECTS.iter().copied().flatten() {
        let skill_domains: Vec<String> = repo.skill_domains.iter().map(|s| s.to_string()).collect();
        let tech_stack: Vec<String> = repo.tech_stack.iter().map(|s| s.to_string()).collect();
        let curated_labels: Vec<String> =
            repo.curated_labels.iter().map(|s| s.to_string()).collect();
        // Nothing to ingest yet means nothing to ingest: `auto` on an
        // empty label list would pull every issue the repository has.
        let ingestion_mode = if repo.curated_labels.is_empty() {
            "manual_only"
        } else {
            "auto"
        };
        let repo_url = format!(
            "https://github.com/{}/{}",
            repo.github_owner, repo.github_repo
        );

        let row: (bool, Uuid) = sqlx::query_as(
            r#"
            INSERT INTO projects
                (slug, name, description, repo_url, tech_stack, is_oss,
                 looking_for_contributors, owner_type, owner_id, curated_by_admin,
                 skill_domains, lifecycle_status,
                 github_repo_owner, github_repo_name,
                 slice_ingestion_mode, curated_labels)
            VALUES ($1, $2, $3, $4, $5, TRUE,
                    TRUE, 'user', $6, $12,
                    $7, 'active',
                    $8, $9,
                    $11, $10)
            ON CONFLICT (slug) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                repo_url = EXCLUDED.repo_url,
                tech_stack = EXCLUDED.tech_stack,
                looking_for_contributors = TRUE,
                curated_by_admin = EXCLUDED.curated_by_admin,
                skill_domains = EXCLUDED.skill_domains,
                github_repo_owner = EXCLUDED.github_repo_owner,
                github_repo_name = EXCLUDED.github_repo_name,
                slice_ingestion_mode = EXCLUDED.slice_ingestion_mode,
                curated_labels = EXCLUDED.curated_labels,
                updated_at = NOW()
            RETURNING (xmax = 0) AS inserted, id
            "#,
        )
        .bind(repo.slug)
        .bind(repo.name)
        .bind(repo.description)
        .bind(&repo_url)
        .bind(&tech_stack)
        .bind(owner_id)
        .bind(&skill_domains)
        .bind(repo.github_owner)
        .bind(repo.github_repo)
        .bind(&curated_labels)
        .bind(ingestion_mode)
        .bind(repo.curated)
        .fetch_one(&db)
        .await
        .with_context(|| format!("failed to upsert project {}", repo.slug))?;

        // What each upstream label means, for this project. Replaced wholesale
        // rather than merged: the catalogue in this binary is the statement of
        // record, and a mapping removed from it should disappear rather than
        // linger in a table nobody is reading any more.
        sqlx::query("DELETE FROM project_label_orientations WHERE project_id = $1")
            .bind(row.1)
            .execute(&db)
            .await
            .with_context(|| format!("failed to clear label map for {}", repo.slug))?;

        for (label, orientation_slug) in repo.label_orientations {
            // `resolve_orientation` follows one rename, so a catalogue written
            // against `dev-frontend` lands on the trade that replaced it. A
            // slug that resolves to nothing is reported rather than skipped:
            // silence here means a whole project ingests untyped work.
            let mapped: Option<Uuid> = sqlx::query_scalar(
                "INSERT INTO project_label_orientations (project_id, label, orientation_id)
                 SELECT $1, $2, resolve_orientation($3)
                  WHERE resolve_orientation($3) IS NOT NULL
                 ON CONFLICT (project_id, label) DO UPDATE
                     SET orientation_id = EXCLUDED.orientation_id
                 RETURNING orientation_id",
            )
            .bind(row.1)
            .bind(label)
            .bind(orientation_slug)
            .fetch_optional(&db)
            .await
            .with_context(|| format!("failed to map {label} for {}", repo.slug))?;

            if mapped.is_none() {
                tracing::warn!(
                    slug = repo.slug,
                    label,
                    orientation = orientation_slug,
                    "orientation not found — issues with this label will be ingested untyped"
                );
            }
        }

        if row.0 {
            created += 1;
            tracing::info!(slug = repo.slug, "project created");
        } else {
            updated += 1;
            tracing::info!(slug = repo.slug, "project updated");
        }
    }

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("  Projects seeded ({created} created, {updated} updated)");
    println!("═══════════════════════════════════════════════════════════");

    for (heading, catalogue) in [
        ("Skilluv repositories (dogfooding)", SKILLUV_REPOS),
        ("Partner repositories (Annexe F)", PARTNER_REPOS),
        (
            "Ecosystem projects (listed, ingestion off)",
            ECOSYSTEM_REPOS,
        ),
    ] {
        println!();
        println!("  {heading}");
        for repo in catalogue {
            let labels = if repo.curated_labels.is_empty() {
                "no ingestion".to_string()
            } else {
                repo.curated_labels.join(", ")
            };
            println!(
                "    • {} → github.com/{}/{}  [{}]",
                repo.slug, repo.github_owner, repo.github_repo, labels
            );
        }
    }

    println!();
    println!("  Ecosystem projects carry no labels on purpose: their issue");
    println!("  volume would bury the partner repositories. Enable one");
    println!("  deliberately when somebody is ready to steward it.");
    println!("  Owner: {owner_email}");
    println!("═══════════════════════════════════════════════════════════");

    Ok(())
}
