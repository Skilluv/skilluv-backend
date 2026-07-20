# Skilluv Backend

> **Building the OSS talent pipeline for Africa's next generation of developers, designers, security researchers, and game makers.**

> 🇬🇧 English (this page) · 🇫🇷 [Version française](README.fr.md)

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2024_edition-orange.svg)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/status-pre--launch-yellow.svg)](#roadmap)

---

## What is Skilluv?

Skilluv is a community platform training talents in **code, design, security, and game development** — not through disconnected kata exercises, but through **real contributions to real open source projects**. Every completed challenge produces a verifiable artifact (a merged pull request, a delivered Figma component, a submitted CVE report, a playable game build) that lives on the contributor's public portfolio and is exportable to recruiters.

The positioning is deliberate:

- **Talents never pay for access.** Companies do, because they benefit when African contributors become excellent.
- **We don't teach syntax — we teach craft.** The gestures a senior would teach at the workbench, at scale. Think *compagnonnage*, not school.
- **We build with AI, not against it.** Disclosure over prohibition. AI is a tool to be declared, not a taboo to be enforced.
- **Rooted in Africa, built for the world.** The wedge is African autodidactic devs and career-changers, but every artifact travels globally.

Public launch: **January 2027**. Private beta: **autumn 2026**.

## What this repo contains

This is the **backend API** of Skilluv, written in **Rust with Axum**. It provides:

- **Authentication** — JWT + Argon2 + TOTP + WebAuthn/passkeys + OAuth + magic links + enterprise OIDC SSO + SCIM 2.0
- **Challenge lifecycle** — creation, submission, sandboxed evaluation via Judge0 (35+ languages)
- **Gamification** — fragments, streaks, badges, guilds, tournaments, bounties, mentorship
- **Real-time community** — WebSocket manager, notifications, forum, direct messages, GitHub integration
- **Enterprise B2B** — dashboards, credits, subscriptions, talent search, sponsored challenges, KYC
- **Compliance** — GDPR export, generic audit log, AES-256-GCM encryption of SSO secrets at rest
- **Observability** — Prometheus metrics, structured tracing, Sentry/GlitchTip integration

Multi-tenant, production-oriented, 54+ SQL migrations.

## Companion repositories

- [`skilluv-frontend`](https://github.com/skilluv/skilluv-frontend) — SvelteKit 2 web app for talents and companies
- [`skilluv-admin`](https://github.com/skilluv/skilluv-admin) — SvelteKit admin panel for platform operators
- [`skilluv-ia`](https://github.com/skilluv/skilluv-ia) — Python AI microservice (gRPC + Redis Queue)

## Quick start

**Prerequisites**: Rust 1.80+ (2024 edition), Docker, Git.

```bash
git clone https://github.com/skilluv/skilluv-backend.git
cd skilluv-backend
cp .env.example .env
# edit .env with your values

docker compose up -d postgres redis minio
cargo build
cargo run
```

The API listens on `http://localhost:3001`. For the full developer setup (Judge0, seed data, staging), see [`README.fr.md`](README.fr.md) — English translation of the extended guide is in progress.

## Architecture at a glance

```
Clients (SvelteKit web app, browsers, third-party APIs)
    |
    v
[Axum HTTP Server]                          port 3001
    |
    +-- routes/       -->  HTTP handlers (validation, serialization)
    +-- middleware/   -->  Auth, rate limiting, security headers, API keys
    +-- services/     -->  Business logic
    +-- models/       -->  Postgres-mapped structs
    +-- websocket/    -->  Real-time rooms
    +-- grpc/         -->  Talks to skilluv-ia
    |
    +-- [PostgreSQL]  -->  Primary storage (54+ migrations)
    +-- [Redis]       -->  Cache, tokens, leaderboards, rate limiting
    +-- [MinIO]       -->  S3-compatible object storage
    +-- [Judge0 CE]   -->  Sandboxed code execution (35+ languages)
```

## Contributing

We welcome contributors — Rust devs, security researchers, technical writers, translators, community builders. See [CONTRIBUTING.md](CONTRIBUTING.md) for the workflow and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for community rules.

**AI-assisted contributions are welcome.** Please disclose the assistance level in your pull request description — we track this transparently across the project.

If you're new to open source and want a guided path, look for issues labeled `good first issue` or reach out via the community channels below.

## Security

For security disclosures, see [SECURITY.md](SECURITY.md). **Do not** open public issues for vulnerabilities.

## Roadmap

- **Now → autumn 2026**: consolidating the challenge → real artifact pipeline (from Judge0-evaluated code snippets to verifiable OSS contributions)
- **Autumn 2026**: private beta with 20–50 selected users
- **January 2027**: public launch — first wedge = French-speaking African autodidacts and career-changers
- **2027**: onboarding of the first curated OSS projects (African design systems, community game engine, banking OS proof-of-concept, etc.)
- **2027–2028**: first cohort graduates, first hires by partner companies

## Community

Community channels are being set up. Follow the maintainer on GitHub for launch announcements. Contributions and DMs are already welcome.

## License

Distributed under the [GNU Affero General Public License v3.0](LICENSE) (AGPL-3.0). The AGPL was chosen deliberately to protect the community core from closed commercial forks while keeping the door open for enterprise adoption via dual-licensing arrangements.

## Origin

Skilluv is built solo by [Jeremie Zitti](https://github.com/skilluv), a Beninese engineer, with the ambition of creating tangible, exportable proof of skill for the African OSS generation. If this resonates, get in touch — contributors, co-founders, mentors, and partner projects are all welcome.
