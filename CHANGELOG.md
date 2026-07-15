# Changelog

All notable changes to the Skilluv backend are documented here.

The format is inspired by [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project will follow semantic versioning once 1.0 is reached.

## [Unreleased]

Target model + P10-P15 (teams multi-role, GitHub ingestion, discovery,
real-money payouts, multi-tenancy + anti-fraud, mobile push +
AI-native verifier + team marketplace) all in place. The P10-P15
roadmap in `docs/roadmap-p10-p15.md` is closed; next iteration will
address KYC full, live AI wiring in prod, and RLS enforcement.

### Added

- **P24.3** — Enterprise type-specific config (`migrations/0097`,
  `routes/agency_clients.rs`): `enterprises.type_config JSONB
  NOT NULL DEFAULT '{}'` + GIN index. `GET/PATCH
  /api/enterprises/me/type-config` with per-type allowlist:
  `staffing_agency` accepts `{commission_rate, brand_white_label,
  default_client_id}`, `remote_international` accepts `{eor_provider,
  preferred_currency, timezone_requirement, tax_withholding_country}`,
  `direct_hire` accepts nothing. PATCH uses JSONB `||` merge to
  preserve untouched keys.
- **P24.2** — Staffing agency client book (`migrations/0096`,
  `routes/agency_clients.rs`): `agency_clients(id, enterprise_id,
  client_name, client_contact_email, notes, active)` with UNIQUE
  (enterprise_id, client_name). PG trigger
  `check_agency_client_enterprise_type` refuses inserts unless the
  parent enterprise is `staffing_agency` (defense in depth). CRUD
  routes at `/api/enterprises/me/agency-clients[/{id}]`.
- **P24.1** — Enterprise types (`migrations/0095`):
  `enterprises.enterprise_type` NOT NULL DEFAULT `'direct_hire'` CHECK
  IN `('direct_hire', 'staffing_agency', 'remote_international')`.
  All existing enterprises implicitly backfilled to `direct_hire` via
  DEFAULT. Splits enterprise workflows at the organization level, not
  the user level — `enterprise_recruiter` capability remains a single
  persona; what changes is the enterprise context.
- **P22.1** — RLS enforcement scaffolding: `services/rls.rs` exposes
  `set_tenant_context_on_tx(tx, tenant_id)` — no-op unless
  `SKILLUV_RLS_ENFORCED=1`. Companion doc
  `docs/RLS-ENFORCEMENT.md` walks through the production activation
  (create `skilluv_app` NOSUPERUSER NOBYPASSRLS role, wrap
  tenant-scoped code paths in transactions, run cross-tenant leak
  tests). Recommendation: only turn on when a compliance-driven
  enterprise customer requires it.
- **P21.1** — Unified `require_admin` across all 5 admin route
  modules (admin, admin_community, admin_moderation, admin_fraud,
  seasons) to delegate to
  `middleware::capabilities::require_capability("admin")`. Net −34
  lines of duplicated `SELECT role FROM users WHERE id = $1` logic.
  admin_fraud went sync → async (7 call sites updated). Test helper
  `register_admin` grants both `users.role='admin'` and the
  `admin` capability to keep pre-P18 tests green.
- **P20.2** — `routes/mentorship.rs::mark_completed` now spawns a
  best-effort `proof_hooks::recompute_all_for_user` for the mentor
  after incrementing `mentor_profiles.total_sessions`. Third
  completed session auto-grants the `mentor` capability.
- **P20.1** — `routes/attestations.rs::issue_compagnonnage` spawns a
  best-effort recompute for the recipient. gesture/skill attestation
  issuance was already covered transitively by the P19.2 hook in
  `ReviewsService::submit_verdict` (attestations are inserted
  in-transaction via `check_and_issue_for_skill_levelup`, so the
  post-commit recompute already sees them).
- **P19.4** — Prometheus metrics on proof engine recompute:
  `skilluv_capabilities_granted_total{capability}`,
  `skilluv_badges_awarded_total{rule}`,
  `skilluv_badges_revoked_total{rule}`,
  `skilluv_ranks_promoted_total{rank}`,
  `skilluv_proof_hook_recompute_total{result=ok|partial}`,
  `skilluv_proof_sweep_users_processed_total`. Unblocks a Grafana
  "engine health" dashboard.
- **P19.3** — `start_proof_sweep_task` background job wired in
  `main.rs`: weekly tokio interval (`SKILLUV_PROOF_SWEEP_INTERVAL_SECS`,
  default 604 800) that recomputes proof engines for every user with
  activity in the last N days (`SKILLUV_PROOF_SWEEP_WINDOW_DAYS`,
  default 30). Env-gated via `SKILLUV_PROOF_SWEEP_ENABLED=1` (off in
  dev). Safety net catching threshold changes, new rules, or failed
  inline hooks.
- **P19.2** — Inline `proof_hooks::recompute_all_for_user` calls (async
  `tokio::spawn`, best-effort) wired into the three proof-producing
  paths: `ReviewsService::submit_verdict` on `Verdict::Approve`,
  `DeliverablesService::create_from_pr_merged` when outcome is
  `Verified`, and `create_from_challenge_submission` on new insert.
  End-to-end test proves that 4 review approvals promote the author to
  Ranger within ~800 ms of the last verdict.
- **P19.1** — `services/proof_hooks.rs`:
  `recompute_all_for_user(db, user_id) -> ProofRecomputeReport` runs
  capabilities → badges → rank in sequence (order matters: Doyen
  depends on the mentor capability from P18.5). Per-engine
  best-effort with `tracing::warn` on failure, aggregated in
  `errors[]` for observability. Companion `sweep_active_users(days)`
  recomputes every user with a `deliverable.verified_at` or
  `attestation.issued_at` inside the window.
- **P18.5** — `services/ranks.rs` now reads mentor status from
  `user_capabilities` (canonical source) with a fallback on
  `users.role='mentor'` for pre-backfill DBs. The P17.4 hardcoded
  `users.role='mentor'` check is gone; Doyen requirement is now clean.
  New test covers the capability path explicitly.
- **P18.4** — Capabilities API (`routes/capabilities.rs`):
  `GET /api/users/{id}/capabilities` (public, active only),
  `GET /api/users/me/capabilities` (auth), `POST
  /api/admin/users/{id}/capabilities` body `{capability,
  granted_reason?, expires_at?}` protected by
  `require_capability("admin")`, `DELETE
  /api/admin/users/{id}/capabilities/{cap}` (soft revoke with
  `revoked_reason='admin_revoke:by_<uuid>'`).
- **P18.3** — `middleware/capabilities.rs`: `require_capability(db,
  user_id, cap)` returns `Forbidden` if the capability is absent,
  revoked, or expired. Companion helper `list_active_capabilities`
  filters by the same rules for `/me/capabilities`. Legacy per-route
  `require_admin` helpers still work (JWT-based `auth.role='admin'`),
  and the backfill from P18.1 keeps both mechanisms in sync during
  transition.
- **P18.2** — `services/capabilities_engine.rs`:
  `recompute_capabilities_for_user(user_id)` auto-promotes based on
  measurable activity — everyone gets `challenger`, mentor at ≥5
  attestations received OR ≥3 mentorship_sessions as mentor,
  pr_reviewer at ≥10 `reviews.verdict='approve'`, issue_proposer at ≥3
  published community `challenge_templates`, project_steward at ≥1
  owned project. Idempotent; never demotes (like the rank system).
  `admin`, `jury_tournament`, `bounty_funder`, and
  `enterprise_recruiter` remain manual-only.
- **P18.1** — Migration 0094: `user_capabilities(user_id, capability ∈
  9-value enum, granted_at, granted_reason, granted_by, expires_at,
  revoked_at, revoked_reason)`. Enum: challenger, mentor,
  project_steward, pr_reviewer, bounty_funder, issue_proposer,
  jury_tournament, admin, enterprise_recruiter. Partial UNIQUE (user_id,
  capability) WHERE revoked_at IS NULL — cumulable, revocable,
  auditable. Backfill from `users.role`: every user gets
  `challenger`, `role='mentor'`→`mentor`, `'admin'`→`admin`,
  `'enterprise'`/`'recruiter'`→`enterprise_recruiter`. Introduces the
  3rd orthogonal user axis alongside skills and orientations.
- **P17.6** — Events + participation (`migrations/0093`,
  `routes/events.rs`): `events(slug, name, starts_at, ends_at,
  visual_theme JSONB, is_partner, is_active)` +
  `user_event_participation(user_id, event_id, joined_at,
  contribution_ref)`. Routes namespaced as `/badge-events` to avoid
  collision with the pre-existing `/events` from tournaments. `GET
  /api/badge-events` (active only), `POST
  /api/badge-events/{slug}/join` (idempotent), `GET
  /api/users/me/badge-events`. Wires up Skilluv Fest / Hacktoberfest /
  seasons to eventually mint `event_stamp` badges via the P17.3 rules
  engine.
- **P17.5** — Badge API (`routes/badges.rs`): polymorphic `GET
  /api/users/{id}/badges` returns the rank + skill_patches[] +
  medals[] + seals_count + stamps_count + guild_crests[], with per-item
  rarity and source_proofs_count. Revoked badges are excluded. Fallback
  rank `apprenti` when the user has no `user_ranks` row (temporary
  until the P18 auto-create trigger lands). `GET /api/badge-rules`
  exposes the non-deprecated rules catalog for the frontend to render
  "badges you can earn".
- **P17.4** — Rank system (`migrations/0092`, `services/ranks.rs`):
  `user_ranks(user_id, rank, achieved_at, previous_rank)` +
  `user_rank_history`. `recompute_rank_for_user` derives one of
  {apprenti, ranger, artisan, maitre, doyen} from verified deliverables
  + received attestations + `users.role='mentor'`. Thresholds match the
  BMAD UX spec (4 → 11+1 → 26+3 → 50+5+mentor). **Unidirectional**:
  never demotes, transitions are audited in `user_rank_history` with a
  reason.
- **P17.3** — Rules engine (`services/badge_engine.rs`):
  `recompute_badges_for_user(user_id)` iterates non-deprecated
  `badge_rules`, interprets JSONB `conditions` (proof_types,
  min_count, skill_tag, display_category), counts matching proofs from
  `deliverables` verified + `attestations`. Auto-rarity from count (0-4
  common, 5-14 rare, 15-49 epic, 50+ legendary) when the rule is on
  `rarity='auto'`. Idempotent, revokes when conditions no longer met.
  Deprecated rules never produce new awards.
- **P17.2** — Display category (`migrations/0091`): added
  `skill_nodes.display_category ∈ {craft, create, understand, operate,
  share, meta}` aligned with the BMAD UX spec's 6 skill families.
  Deterministic backfill: code → craft, design + game → create,
  security + ops → operate, ai → understand, soft_skills → share. Meta
  is admin-curated (open-source-governance, product-thinking,
  growth-experimentation, strategy, community-building,
  roadmap-planning).
- **P17.1** — Proof Engine foundation (`migrations/0090`): new
  `badge_rules(slug, output_type ∈ {skill_patch, rank, guild_crest,
  challenge_seal, event_stamp, medal}, conditions JSONB, rarity,
  admin_editable, deprecated_at)` + extends `user_badges` with
  `rule_id`, `source_proofs UUID[]` (traceability), `rarity`,
  `revoked_at`, `revoked_reason`. Migrated the 9 legacy badges
  (streak/challenges/fragments) to `legacy_*` rules marked deprecated —
  no more auto-awards for connection streaks or raw action counts;
  those are now absorbed into the P17.4 rank system.
- **P16.5** — Onboarding playlist per orientation: `GET
  /api/users/me/orientations/{slug}/playlist` returns 3 training
  challenges (in the orientation's primary+secondary domains, not
  already verified by the user) + up to 5 open team-role-slots whose
  `required_skill_id` matches an orientation core skill (excluding the
  user's own teams). Data-driven via
  `services::orientations_playlist::playlist_for`.
- **P16.4** — Recruiter search v3 (`routes/talent_search_v3.rs`):
  `GET /api/talents/search/v3?orientation=X&skills=Y,Z&mode=active&only_primary=true&min_proficiency=3&working_language=fr`.
  Joins `user_orientations` + `user_skills` matched via slugs; sorts by
  cumulative weighted_proven_count on matched skills + primary + active.
  Excludes `mode=learning` by default (no aspirational-only pollution);
  `mode=both` opts them back in for internships/junior-hiring flows.
  Ended orientations always excluded.
- **P16.3** — Orientations routes (`routes/orientations.rs`):
  `GET /api/orientations` (paginated + domain/tag filters + archived
  toggle), `GET /api/orientations/{slug}` (detail with joined recommended
  skills), `GET/POST /api/users/me/orientations`, `PATCH/DELETE
  /api/users/me/orientations/{slug}`. Enforces app-level cap of 3 active
  orientations, auto-promotes the first registered to primary, ON
  CONFLICT DO UPDATE re-activates a previously ended orientation. DELETE
  historises via `ended_at` (never deletes rows — historical value for
  reconversion profiles).
- **P16.2** — Migration 0089: `user_orientations` — the link between
  each user and the orientations they claim. Columns: `mode` ∈
  {`learning`, `active`}, `is_primary` (partial UNIQUE per user
  amongst non-ended rows), `started_at`, `ended_at` (history-preserving
  soft-close), `working_languages TEXT[]`, `timezone`, `notes`. CHECK
  `ended_at >= started_at`. Backfill from `users.skill_domain` with
  deterministic mapping (code → dev-fullstack, design → web-designer,
  game → game-programmer, security → pentester-web, ai →
  prompt-engineer, ops → devops-engineer, soft_skills → tech-writer).
  Mode is `active` if the user has any proven `user_skills` row, else
  `learning`.
- **P16.1** — Migration 0088: `orientations` (career-track catalog) +
  `orientation_skill_map` (many-to-many with `is_core`, `is_recommended`,
  `weight`). Seed of 31 curated orientations covering all 7 domains:
  dev-frontend/backend/fullstack, mobile-android/ios/cross,
  systems-programmer, smart-contract-dev, web/mobile/motion-designer,
  illustrator, 3d-artist, game-artist-2d/3d, game-programmer/designer/
  sound-engineer, data/ml/prompt-engineer, data-analyst,
  devops-engineer, sre, cloud-architect, pentester-web/mobile,
  soc-analyst, security-engineer, tech-writer, open-source-maintainer.
  Slug regex + length constraint. Kept named `orientations` (not
  `tracks`) to avoid collision with the pre-existing P3 `tracks` table
  (curriculum sequences — different concept).
- **P15.4** — Rust model rename: `models::Challenge` → `models::ChallengeTemplate`.
  The DB has held the `challenge_templates` table since P9.3 (mig 0075);
  the Rust struct now aligns with the target vocabulary. All routes
  (`admin`, `admin_community`, `challenges`, `challenge_tags`,
  `challenge_teams`, `community`) updated. Error message strings and
  test seed labels intentionally preserved.
- **P15.3** — Team marketplace: `GET /api/teams/marketplace?role=&skill=&limit=`
  returns open `team_role_slots` enriched with team name + challenge
  title + required skill slug. Slot creation now fires an async
  `TeamRolesService::notify_eligible_users_for_slot`: queries
  `user_skills` matching the slot's `required_skill_id` at
  `proficiency_level >= min_proficiency_level`, inserts one
  `notifications` row per user (type `team_slot_open`), and pushes
  via mobile FCM/APNS best-effort. Slots without a `required_skill_id`
  do not broadcast (anti-spam by design).
- **P15.2** — AI-native challenge verifier: migration 0087 adds
  `'llm_evaluation'` to `deliverables.verifiable_by` CHECK and
  `challenge_templates.evaluation_rubric JSONB` (+ GIN index).
  `services/llm_verifier.rs` wraps the existing `AiClient::review_code`
  gRPC call to `skilluv-ia` (Python), normalizes `quality_score` to
  `[0,1]`, auto-verifies at ≥ 0.7 else routes to `pending_manual_review`
  with the full LLM report stored under `verification_signal.llm_verifier`.
  Fallback when `AiClient` is None marks the deliverable
  `pending_manual_review` with reason `ai_client_not_connected`. Admin
  endpoint `POST /api/admin/fraud/llm-evaluate/{id}` triggers evaluation.
  **No AI model is retrained here — Rust delegates to the existing
  `skilluv-ia` service per architecture rule.**
- **P15.1** — Mobile push: migration 0086 adds
  `user_push_tokens(user_id, platform 'fcm'|'apns', token, device_id,
  last_seen_at)` UNIQUE(user_id, device_id). `services/mobile_push.rs`
  ships `Platform` enum, `register_token`, `revoke_token`,
  `purge_stale`, `list_tokens_for_user`, `MobilePushProvider` trait
  with `FcmProvider` + `ApnsProvider` stubs (gated on `FCM_SERVER_KEY` /
  `APNS_KEY_ID`), and `push_to_user_mobile`. Routes
  `POST /users/me/push-tokens/register`, `DELETE /users/me/push-tokens/{device_id}`,
  `GET /users/me/push-tokens`. `NotificationService::send` now
  best-effort pushes mobile after WS. Web VAPID push remains
  untouched.
- **P14.5** — `routes/admin_fraud.rs` : `GET /api/admin/fraud/queue`,
  `POST /admin/fraud/deliverables/{id}/mark-valid|revoke`, `POST
  /admin/fraud/users/{id}/mark-valid`, `POST /admin/fraud/scan-deliverable/{id}`,
  `POST /admin/fraud/detect-multi-accounts`. Toutes require_admin.
- **P14.4** — Migration 0085: `user_fingerprints` (SHA-256 hashed IP/UA/canvas)
  + `users.suspected_multi_account`. `fingerprint::record/detect_multi_accounts/purge_old`.
- **P14.3** — Migration 0084: `deliverable_embeddings(embedding FLOAT4[])` +
  `deliverables.plagiarism_score/similar_to`. `plagiarism::cosine_similarity/
  store_embedding/scan_deliverable/list_flagged` — détection anti-copie
  cross-user par cosine sim > threshold sur fenêtre 30j tenant-scopée.
- **P14.2** — Migration 0083: RLS POC — policies `tenant_isolation_deliverables`
  + `tenant_isolation_attestations` + fonction `set_tenant_context(uuid)`.
  RLS DISABLED par défaut (activation prod = créer role NOSUPERUSER NOBYPASSRLS).
- **P14.1** — Migration 0082: `tenant_id` UUID sur 5 tables sensibles
  (challenge_submissions, deliverables, user_skills, attestations, project_slices).
  5 triggers BEFORE INSERT auto-tag depuis parent (challenge_templates,
  users.primary_tenant_id, funded/created_by).
- **P13.5** — `GET /api/users/me/wallet/statement.csv` (fiscal obligation
  + user self-audit). `WALLET_{DAILY,MONTHLY}_LIMIT_{EUR,XOF}` env vars
  enforce sliding-window withdraw limits.
- **P13.4** — Bounty merge webhook now credits the talent wallet in real
  currency (EUR or XOF based on `residency_country`) on top of fragments.
  Rates configured via `BOUNTY_CREDIT_TO_{EUR,XOF}` env vars.
- **P13.3** — `MobileMoneyProvider` trait + Orange/MTN/Wave impls.
  `POST /wallet/momo/phone` + `POST /wallet/withdraw/momo`. KYC-lite gate
  at 100 000 XOF before full KYC.
- **P13.2** — Stripe Connect Express onboarding + withdraw.
  `POST /wallet/stripe/onboard`, `POST /wallet/withdraw/stripe`,
  `POST /webhooks/stripe-connect` for `account.updated`.
- **P13.1** — Talent wallet (EUR + XOF balances). SHA-256 hash-chained
  ledger for audit-proof `talent_transactions`. `GET /wallet`,
  `/wallet/transactions`, `POST /wallet/residency`.
- **P12.4** — `GET /api/explore` — unified multi-criteria search across
  `project_slices` + `challenge_templates` with filters (kind, domain,
  difficulty, language, project_id, q text) and cross-source pagination.
- **P12.3** — `GET /api/feed/for-you` — personalized feed mixing 4 sources:
  open slices in favorite projects, level-up slice recommendations (P4),
  new challenges from enrolled tracks, and recent community attestations.
- **P12.2** — `POST/GET/DELETE /api/users/me/interests/projects` — user
  marks projects as interesting (onboarding + feed scoping). New table
  `user_project_interests` with score 0-100 (migration 0080).
- **P12.1** — `GET /api/users/me/recommendations/projects` — project
  recommendations scored by (matched_domain_wpc × health_score ×
  contributor_boost), excluding projects where the user already has a
  verified deliverable.
- **P11.4** — `GET /api/stewards/{project_id}/inbox` lists ingested drafts;
  `POST /api/slices/{id}/publish` (draft → open) and `POST /api/slices/{id}/reject`
  (draft → closed) require admin OR active steward on the project.
- **P11.3** — `SliceIngestor` trait exposes a `FigmaIngestor` stub (documentary,
  no-op) and `dispatch_ingestors` generic dispatcher — proves the ingestion
  pipeline is extensible to Notion, Trello, partner imports without coupling.
- **P11.2** — Extended `POST /api/webhooks/github`: `issues.labeled` events
  now ingest a slice in real-time if the label matches the project's
  `curated_labels` and the mode is `auto` or `curator_review`. PRs skipped.
- **P11.1** — New binary `skilluv-github-ingest`: polls all projects with
  `slice_ingestion_mode IN ('auto','curator_review')` and materializes issues
  with curated labels as `project_slices` (draft or open). Deploy as hourly
  cron. Idempotent via `uniq_slices_github_issue_per_project`.
- **P10.6** — `GET /api/guilds/{slug}/composition` — per-domain skill matrix
  (member_count, avg_level, top 3 skills) computed via CTE + window functions.
- **P10.5** — `POST /api/teams/{id}/guild` links a team as "official" of a guild;
  each team submit then also grants a 10% collective GP bonus to that guild
  (on top of the per-member 10%).
- **P10.4** — Team challenge submits now create a shared `deliverable` with
  contributors materialized in `artifact_metadata.contributors`. Hash includes
  `team_id` so two different teams with the same code produce distinct
  deliverables. Fragment distribution follows role slots (or equal split if none).
- **P10.3** — `challenge_templates.team_composition` JSONB template. Creating
  a team for such a challenge auto-provisions the role slots. Admin API
  (`POST/PUT /api/admin/challenges/*`) accepts `team_composition`.
- **P10.2** — `team_role_slots` table + marketplace endpoint
  `GET /api/team-slots/open?role=musician` to find teams looking for a role.
  Multi-disciplinary team compositions now first-class (musician + animator_3d
  + coder + designer with skill prerequisites per slot).
- **P10.1** — Persistent teams (`challenge_teams.is_persistent`) survive
  across challenges. Slice team-claims (`project_slices.claimed_by_team_id`
  XOR user claim). New `POST /api/teams` + `/api/slices/{id}/claim-as-team`.
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

### P10 — Teams multi-rôles + Guilds bridge (`dcac145` → `33daf75`)

Delivered in 6 sub-phases. Unlocks multi-disciplinary game-dev teams
(musician + animator_3d + coder + designer with per-role skill prerequisites)
and connects the ephemeral team system with the persistent guild economy.

- **P10.1** (`dcac145`) — Migration 0076: `challenge_teams.is_persistent` +
  `challenge_id` nullable; `project_slices.claimed_by_team_id` XOR user claim.
  `SlicesService::claim_as_team/unclaim_by_team/list_claimed_by_team`. Endpoints
  `POST /api/teams`, `POST /api/slices/{id}/claim-as-team`.
- **P10.2** (`9ad04f1`) — Migration 0077: `team_role_slots` table (free-form
  `role_slug`, optional `required_skill_id`, `min_proficiency_level`).
  `TeamRolesService` with create/fill/leave/delete + marketplace
  `find_open_slots_by_role`. UNIQUE partial prevents dual-slot per user per team.
- **P10.3** (`8473441`) — Migration 0078: `challenge_templates.team_composition`
  JSONB. `create_team` auto-provisions slots from the template. Admin API
  accepts `team_composition` on create/update.
- **P10.4** (`9ebc59a`) — `DeliverablesService::create_from_team_submission`
  with `TeamContributor` in `artifact_metadata`. Hash includes `team_id`.
  `submit_team` distributes fragments per contributor + per-user GP + creates
  the deliverable. Retires `#[allow(dead_code)]` on `body.code`.
- **P10.5** (`738517a`) — Migration 0079: `challenge_teams.guild_id`.
  `guild::award_bonus_gp_for_team` grants 10% collective bonus to the linked
  guild on team submits. Endpoints `POST/DELETE /api/teams/{id}/guild`.
- **P10.6** (`33daf75`) — `guild::guild_skill_matrix` (CTE + window func) →
  per-domain aggregate: member_count, avg_level, top 3 skills. Endpoint
  `GET /api/guilds/{slug}/composition`.

Full parallel regression after P10: 303 tests pass, 0 real failure
(1 flaky Mailpit test on `test_change_email_end_to_end` passes individually).

### P11 — Automatic GitHub slice ingestion (`2a3ec93` → `ec904e3`)

Delivered in 4 sub-phases. Completes the G.1 workflow: Skilluv-tracked
projects auto-detect new GitHub issues with curated labels and materialize
them as `project_slices` for humans to claim.

- **P11.1** (`2a3ec93`) — `services/slice_ingestion.rs` with `SliceIngestor`
  trait + `GitHubIngestor` impl. New binary `skilluv-github-ingest` for
  cron-based polling. Reuses `uniq_slices_github_issue_per_project` for
  idempotency. Mode `auto` → status='open', `curator_review` → 'draft'.
- **P11.2** (`59d4cce`) — Real-time webhook path: `POST /api/webhooks/github`
  now handles `issues.labeled`, matching repo + `curated_labels` +
  `slice_ingestion_mode`. Fixes ON CONFLICT WHERE to match the partial UNIQUE
  index (needed both `slice_type='github_issue'` AND `external_ref IS NOT NULL`).
- **P11.3** (`7ae29f2`) — `FigmaIngestor` stub + `dispatch_ingestors` generic
  dispatcher. 3 tests including a `FakeIngestor` composed via `Box<dyn SliceIngestor>`
  — proves the trait accepts third-party impls without coupling.
- **P11.4** (`ec904e3`) — `SlicesService::list_drafts_for_project` +
  `publish_draft` + `reject_draft`. Steward inbox endpoints. Admin OR
  `StewardsService::is_steward` authorization on all three.

Full parallel regression after P11: 319 tests pass, 0 real failure
(1 flaky Mailpit on `test_change_email_end_to_end`, passes individually).

### P12 — Discovery & recommendations (`f86d220` → `239d93f`)

Delivered in 4 sub-phases. Answers "the new user just landed on the home,
what do they claim first?" — the platform now surfaces matched projects,
personalized feeds, and open exploration.

- **P12.1** (`f86d220`) — `projects::recommend_for_user(db, user_id, limit)`
  scores projects by (sum of user WPC on matched domains × health_score ×
  1.5 contributor-boost). Excludes projects with existing verified deliverable.
  `Project` struct extended with `skill_domains` + `health_score`.
- **P12.2** (`f78a639`) — Migration 0080: `user_project_interests` table.
  `mark_interested_batch` for the onboarding "cochez les projets" step,
  `list_interests` scoped to non-archived projects with score > 0.
- **P12.3** (`5de34dc`) — `for_you_feed` handler mixes 4 sources with
  unified `FeedItem { kind, happened_at, payload }` shape. Uses P4 slice
  recommendations, P3 track enrollment, and P5 recent community attestations.
- **P12.4** (`239d93f`) — New `routes/explore.rs`. Cross-source SQL fetches
  `page * per_page` items each to guarantee in-memory pagination works.
  Mounted at `/api/explore` in `lib.rs`.

Full parallel regression after P12: 347 tests pass, 0 real failure.

### P13 — Real-money payouts (`a5a6807` → `5ee97ca`)

Delivered in 5 sub-phases. Fulfills the product promise "companies pay
talents, not the other way around" — talents can now withdraw real EUR
via Stripe Connect or XOF via Mobile Money (Orange/MTN/Wave).

- **P13.1** (`a5a6807`) — Migration 0081: `talent_wallets` +
  `talent_transactions` with SHA-256 hash-chained ledger (`prev_ledger_hash`,
  `ledger_hash`). `credit()`, `debit()` atomic with balance guard,
  `verify_ledger_chain()` for audit. `Utc::now()` truncated to microseconds
  before hash (PG TIMESTAMPTZ precision).
- **P13.2** (`0b52c0d`) — Stripe Connect Express onboarding + withdraw.
  Reuses `services/stripe.rs` from Phase 5.11 (mentorship payouts).
  Rollback (credit refund) if Stripe rejects the transfer.
- **P13.3** (`dfd5f97`) — `MobileMoneyProvider` trait +
  `OrangeMoneyProvider`, `MtnMobileMoneyProvider`, `WaveProvider` stubs.
  Orange checks for `ORANGE_MONEY_API_KEY` — stub returns `Pending` in dev.
  E.164 phone validation + XOF-only in this phase.
- **P13.4** (`1ce4c53`) — `handle_pull_request_event` in `bounties.rs`
  extended: on merge, in addition to fragments, credits the talent wallet
  in EUR or XOF based on `residency_country`. UEMOA countries →
  `BOUNTY_CREDIT_TO_XOF`, others → `BOUNTY_CREDIT_TO_EUR`. Best-effort.
- **P13.5** (`b6d53cf`) — `debits_within(user, currency, hours)` sums
  outgoing amounts on a sliding window. `enforce_limit()` helper called
  in stripe_withdraw + momo_withdraw with per-env limits. CSV statement
  export with proper Content-Type / Content-Disposition headers.

Test fix (`5ee97ca`): P13.2 + P13.5 tests mutate process-global env vars
(`STRIPE_SECRET_KEY`, `WALLET_DAILY_LIMIT_XOF`). A per-binary static
`Mutex<()>` serializes them so parallel tokio tests don't race on env.

Full parallel regression after P13: 375 tests pass, 0 real failure.

### P14 — Multi-tenancy + anti-fraude (`b67dd25` → `a6c3b39`)

Delivered in 5 sub-phases. Cross-tenant isolation en profondeur (5 tables
sensibles taggées via triggers, RLS POC prête à activer en prod) + moteurs
anti-fraude (plagiat cross-user via cosine similarity, détection multi-account
via fingerprinting) + dashboard admin de triage.

- **P14.1** (`b67dd25`) — Migration 0082 : `tenant_id` NULLABLE + FK sur 5
  tables + backfill via JOIN + 5 triggers BEFORE INSERT auto-tag depuis
  parent (respectent explicit tenant_id fourni).
- **P14.2** (`906f7e7`) — Migration 0083 : policies + `set_tenant_context()`.
  Tests documentent POC via SELECT explicite (rôle skilluv dev = superuser
  bypass RLS).
- **P14.3** (`b1accde`) — Migration 0084 : `deliverable_embeddings`
  (FLOAT4[], pas de dep pgvector) + `plagiarism_score`. `cosine_similarity`
  in-memory Rust, scan tenant-scoped fenêtre 30j.
- **P14.4** (`7244ced`) — Migration 0085 : `user_fingerprints` SHA-256
  (ip/ua/canvas) + `users.suspected_multi_account`. `detect_multi_accounts`
  GROUP BY (ip,ua) HAVING count >= min flag les groupes.
- **P14.5** (`a6c3b39`) — 6 endpoints admin fraud queue/mark-valid/revoke/scan/detect.

Full parallel regression after P14: 396 tests pass, 0 real failure.

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
