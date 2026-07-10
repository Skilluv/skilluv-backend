# Changelog

All notable changes to the Skilluv backend are documented here.

The format is inspired by [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project will follow semantic versioning once 1.0 is reached.

## [Unreleased]

The target model from the roadmap `docs/challenges-target-model-and-roadmap.md`
is now fully in place at the DB level. The backend is ready for product-driven
iteration beyond P0-P9.

### Added

- **P9.2** — Auto-creation of a mirror `project` for the GitHub repo on
  `POST /api/bounties` when no project matches `(repo_owner, repo_name)`.
  Simplifies the B2B onboarding path.
- **P8.5b** — Headers `Deprecation: true`, `Sunset: Fri, 31 Dec 2027 23:59:59 GMT`,
  `Link: </deliverables>; rel="successor-version"` on `POST /api/challenges/{id}/submit`.

### Changed

- **P9.3** — Table `challenges` was renamed to `challenge_templates` (migration 0075).
  The HTTP paths `/api/challenges/*` are **unchanged**; the rename is an
  implementation detail. The Rust struct `Challenge` is kept.
- **P9.2** — The bounty API is now entirely backed by `project_slices`
  (`funder_enterprise_id NOT NULL`). The HTTP response shape is preserved for
  frontend compatibility. The `paid` bounty status is mapped to `merged`
  internally; the external vocabulary is preserved.
- **P8.6** — The 3 endpoints that expose the skills summary on the profile
  (gamification, profile, public_api) now read from `user_skills + skill_nodes`
  (single source of truth).

### Removed

- **P9.3** — Old `challenges` table name (renamed, see above).
- **P9.2** — Tables `oss_bounties` + `oss_bounty_claims` (migration 0074).
  Column backfill into `project_slices` happens in 0073.
- **P9.1** — Columns `challenge_submissions.code|stdout|stderr` (migration 0072).
  Content is preserved in `deliverables.artifact_metadata.code_content` (rule
  A.4 — immutability of proofs).
- **P8.7** — Table `skill_fragments` (migration 0071). Backfill absorbed by
  `user_skills + skill_nodes` in P8.5c/6.
- **P8.3** — Columns `challenges.ai_allowed` + `challenges.prerequisite_fragments`
  (migration 0070). Replaced by typed `ai_policy` + the `challenge_prerequisites` DAG.

### Fixed

- **fix(routes)** — Route conflict on `/api/seasons` between `routes/tournament.rs`
  (Phase 2 Sprint 6) and `routes/seasons.rs` (P6). The tournament module now
  only registers the `/admin/seasons/*` endpoints.
- **fix(tests)** — Eliminated parallel-run flakies: Redis isolation per test
  binary (PID % 16), unique `X-Forwarded-For` per `TestApp`, and
  `SKILLUV_DISABLE_RATELIMIT=1` explicit bypass of `RateLimiter` in integration
  tests.

---

## Target model roadmap (P0 → P9)

Each roadmap phase corresponds to a `feat(challenges): P<n>` commit.
See `docs/challenges-target-model-and-roadmap.md` for the full spec.

### P0 — Foundation (`47cafc8`)

Foundations of the target model:
- `skill_nodes` (atomic skill graph, 337 nodes seeded across 7 domains)
- `project_slices` (claim-able unit of work, 9 slice types)
- `slice_skills` (M2M skills ↔ slices with `weight`)
- `deliverables` (verifiable artifact, replaces `challenge_submissions.code`)
- `user_skills` (proven_count, weighted_proven_count, proficiency_level 1-5)

### P1 — Unified slices + bounty integration (`b680a06`)

- `SlicesService`: list_open, get, claim/unclaim, expire_stale_claims
- Backfill of existing `oss_bounties` as `project_slices` (migration 0063)
- `projects.curated_labels` (webhook ingestion triggers)
- Exclusive 7-day claim with DB soft-lock

### P2.1 — Deliverables + GitHub webhook (`8e3095f`)

- `DeliverablesService::mark_pr_merged` — auto-verification via GitHub webhook
- `webhook_events` (idempotency by `delivery_id`)
- Automatic skill propagation on verification (workflow G.2)

### P2.2 — Human review queue (`1a74d40`)

- `review_tasks` (queue for deliverables with `verifiable_by='human_review'`)
- `ReviewsService`: submit verdict, reject, steward promotion
- `review_metrics` with `reputation_score` formula (see Q4 in the roadmap)

### P3 — Prerequisites DAG + tracks (`b846749`)

- `challenge_prerequisites` (DAG, `is_required` vs recommended)
- `tracks` + `track_challenges` + `user_tracks`
- `challenges.is_capstone` (phase-graduation masterpiece)
- Cycle checks (self-reference, direct, transitive)

### P4 — Skill graph propagation (`1bbf5a8`)

- `GET /api/profile/{username}/skills` — enriched "my skills" view
- `GET /api/skills/{slug}/talents` — recruiter search by skill + level
- `GET /api/users/me/recommendations/slices` — slice recos near a level-up

### P5 — Attestations ⭐ LAUNCH (`2bacfd1`)

**Killer feature.** Gesture / skill / compagnonnage attestations:
- Auto-issue on skill level-up (idempotent via UNIQUE index)
- HMAC-SHA256 signature (`attestation_signature`)
- Public `GET /api/attestations/{id}` + `GET /api/attestations/{id}/verify`
- Admin revocation with `revocation_reason`

### P6 — Seasons + project stewards (`4d18639`)

- `seasons` (Q1 2027 = first "Foundations" season)
- `project_stewards` (per-project admin delegation)
- `project_seasons` (a project's participation in a season)

### P7 — Outbound portfolio export (`340ddba`)

- `GET /api/users/{username}/portfolio` (JSON-LD schema.org)
- `GET /api/users/{username}/badge.svg` (public embeddable badge)
- Stable canonical URLs for external referencing

### P8 — Deprecations and cleanup (`e88eafb` → `4429a91`)

Delivered in 10 sub-phases (one per commit):
- **P8.1** — `admin.rs` accepts typed `ai_policy` + auto-derives from `ai_allowed` (backward compat).
- **P8.2** — `challenges.rs::start_challenge` gates via the DAG (`TracksService::check_eligibility`) with `prerequisite_fragments` fallback.
- **P8.3** — Migration 0070 DROP `ai_allowed` + `prerequisite_fragments`.
- **P8.4** — `bounties.rs::create_bounty` dual-writes to `project_slices` when `github_repo_owner/name` matches.
- **P8.5a** — `DeliverablesService::create_from_challenge_submission` (SHA-256 idempotent, `verifiable_by='automated_diff'`).
- **P8.5b** — HTTP headers `Deprecation` / `Sunset` / `Link` on `POST /challenges/{id}/submit`.
- **P8.5c** — Best-effort `user_skills` propagation on legacy challenge success.
- **P8.6** — Helper `list_user_skill_fragments_or_backfill` + migration of the 3 historical readers.
- **P8.6b** — Helper `list_user_top_skills` + migration of the 3 `talent_search / github` consumers.
- **P8.6c** — Leaderboards + data_export switch to `user_skills + skill_nodes`.
- **P8.7** — Migration 0071 DROP TABLE `skill_fragments` + consumer cleanup.
- **P8.8** — Comment cleanup + `docs/CHANGELOG-p8-completion.md`.

### P9 — Wrapping up P8 out-of-scope items (`dbcb28e` → `52ad13b`)

Delivered in 3 sub-phases:
- **P9.1** (`dbcb28e`) — Migration 0072 DROP `challenge_submissions.code|stdout|stderr` with backfill into `deliverables.artifact_metadata`. `create_from_challenge_submission` extended (language, stdout, stderr).
- **P9.2** (`d9d402b`) — Migrations 0073 + 0074: merge `oss_bounties` + `oss_bounty_claims` into `project_slices` + DROP tables. `routes/bounties.rs` fully rewritten. Auto-created mirror projects.
- **P9.3** (`52ad13b`) — Migration 0075: `ALTER TABLE challenges RENAME TO challenge_templates`. 15 `src/` files + 5 `tests/` files updated for SQL. HTTP API unchanged.

---

## Public governance and policy

### Initial public release (`97eae90`)

First public commit of the repository.

### OSS standards (`1df8ca2`)

- LICENSE AGPL-3.0
- SECURITY.md
- CONTRIBUTING.md
- CODE_OF_CONDUCT.md

### Documentation (`2498eb7`, `08aff33`, `289bbe4`)

- Primary README in English (narrative-mission tone), French version at `README.fr.md`.
- GitHub templates: issues + PR.
