# Changelog

Toutes les évolutions notables du backend Skilluv sont documentées ici.

Le format s'inspire de [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/)
et le projet suit le versionnage sémantique une fois la 1.0 atteinte.

## [Unreleased]

Le modèle cible du roadmap `docs/challenges-target-model-and-roadmap.md` est
désormais entièrement en place côté DB. Le backend est prêt pour l'itération
produit hors P0-P9.

### Ajouts

- **P9.2** — Auto-création d'un `project` miroir du repo GitHub lors du
  `POST /api/bounties` si aucun projet ne matche `(repo_owner, repo_name)`.
  Simplifie l'onboarding B2B.
- **P8.5b** — Headers `Deprecation: true`, `Sunset: Fri, 31 Dec 2027 23:59:59 GMT`,
  `Link: </deliverables>; rel="successor-version"` sur `POST /api/challenges/{id}/submit`.

### Modifications

- **P9.3** — La table `challenges` est renommée `challenge_templates` (mig 0075).
  Les paths HTTP `/api/challenges/*` sont **inchangés** ; le renommage est un
  détail d'implémentation. Le struct Rust `Challenge` est conservé.
- **P9.2** — L'API bounty est désormais entièrement portée par `project_slices`
  (colonne `funder_enterprise_id` non-null). La shape de réponse HTTP reste
  identique pour compat frontend. Le statut `paid` est mappé sur `merged` en
  interne, le vocabulaire externe est préservé.
- **P8.6** — Les 3 endpoints qui exposent le résumé skills du profil
  (gamification, profile, public_api) tirent depuis `user_skills + skill_nodes`
  (source unique).

### Suppressions

- **P9.3** — Ancienne table `challenges` (renommée, cf ci-dessus).
- **P9.2** — Tables `oss_bounties` + `oss_bounty_claims` (mig 0074). Backfill
  des colonnes vers `project_slices` en 0073.
- **P9.1** — Colonnes `challenge_submissions.code|stdout|stderr` (mig 0072).
  Le contenu est préservé dans `deliverables.artifact_metadata.code_content`
  (règle A.4 immuabilité des preuves).
- **P8.7** — Table `skill_fragments` (mig 0071). Backfill absorbé par
  `user_skills + skill_nodes` en P8.5c/6.
- **P8.3** — Colonnes `challenges.ai_allowed` + `challenges.prerequisite_fragments`
  (mig 0070). Remplacées par `ai_policy` (typé) + le DAG `challenge_prerequisites`.

### Corrections

- **fix(routes)** — Conflit de routes `/api/seasons` entre `routes/tournament.rs`
  (Phase 2 Sprint 6) et `routes/seasons.rs` (P6). Le module tournament n'inscrit
  plus que les endpoints `/admin/seasons/*`.
- **fix(tests)** — Élimination des flakies en parallèle : isolation Redis par
  binaire de test (PID % 16), `X-Forwarded-For` unique par `TestApp`, et
  `SKILLUV_DISABLE_RATELIMIT=1` bypass explicite du `RateLimiter` dans les
  tests d'intégration.

---

## Roadmap du modèle cible (P0 → P9)

Chaque phase du roadmap correspond à un commit `feat(challenges): P<n>`.
Voir `docs/challenges-target-model-and-roadmap.md` pour la spec complète.

### P0 — Foundation (`47cafc8`)

Fondations du modèle cible :
- `skill_nodes` (skill graph atomique, 337 nœuds seedés sur 7 domaines)
- `project_slices` (unité de travail claim-able, 9 slice_types)
- `slice_skills` (M2M skills ↔ slices avec `weight`)
- `deliverables` (artefact vérifiable, remplace `challenge_submissions.code`)
- `user_skills` (proven_count, weighted_proven_count, proficiency_level 1-5)

### P1 — Slices unifiées + intégration bounties (`b680a06`)

- `SlicesService` : list_open, get, claim/unclaim, expire_stale_claims
- Backfill des `oss_bounties` existantes en `project_slices` (mig 0063)
- `projects.curated_labels` (déclencheurs d'ingestion webhook)
- Claim exclusif 7 jours avec soft-lock DB

### P2.1 — Deliverables + webhook GitHub (`8e3095f`)

- `DeliverablesService::mark_pr_merged` — auto-vérification via webhook GitHub
- `webhook_events` (idempotence par `delivery_id`)
- Propagation automatique des skills à la vérification (workflow G.2)

### P2.2 — Review queue humaine (`1a74d40`)

- `review_tasks` (queue de reviews pour deliverables `verifiable_by='human_review'`)
- `ReviewsService` : soumettre verdict, rejeter, promouvoir à steward
- `review_metrics` avec formule `reputation_score` (voir Q4)

### P3 — DAG des prérequis + tracks (`b846749`)

- `challenge_prerequisites` (DAG, `is_required` vs recommandé)
- `tracks` + `track_challenges` + `user_tracks`
- `challenges.is_capstone` (chef-d'œuvre de fin de phase)
- Anti-cycle checks (self-reference, direct, transitive)

### P4 — Skill graph propagation (`1bbf5a8`)

- `GET /api/profile/{username}/skills` — vue "mes skills" enrichie
- `GET /api/skills/{slug}/talents` — recherche recruteur par skill + level
- `GET /api/users/me/recommendations/slices` — reco slices proches d'un level-up

### P5 — Attestations ⭐ LAUNCH (`2bacfd1`)

**Feature killer.** Attestations gesture / skill / compagnonnage :
- Auto-issue au level-up d'un skill (idempotent via UNIQUE index)
- Signature HMAC-SHA256 (`attestation_signature`)
- `GET /api/attestations/{id}` public + `GET /api/attestations/{id}/verify`
- Révocation admin avec `revocation_reason`

### P6 — Seasons + project stewards (`4d18639`)

- `seasons` (Q1 2027 = première saison "Fondations")
- `project_stewards` (délégation d'admin par projet)
- `project_seasons` (participation d'un projet à une saison)

### P7 — Portfolio export sortant (`340ddba`)

- `GET /api/users/{username}/portfolio` (JSON-LD schema.org)
- `GET /api/users/{username}/badge.svg` (badge public embeddable)
- URLs canoniques stables pour référencement externe

### P8 — Deprecations et nettoyage (`e88eafb` → `4429a91`)

Éliminée en 10 sous-phases (une par commit) :
- **P8.1** — `admin.rs` accepte `ai_policy` typée + auto-dérivation depuis `ai_allowed` (backward compat).
- **P8.2** — `challenges.rs::start_challenge` gate via DAG (`TracksService::check_eligibility`) avec fallback `prerequisite_fragments`.
- **P8.3** — Migration 0070 DROP `ai_allowed` + `prerequisite_fragments`.
- **P8.4** — `bounties.rs::create_bounty` dual-write vers `project_slices` quand match `github_repo_owner/name`.
- **P8.5a** — `DeliverablesService::create_from_challenge_submission` (SHA-256 idempotent, `verifiable_by='automated_diff'`).
- **P8.5b** — Headers HTTP `Deprecation` / `Sunset` / `Link` sur `POST /challenges/{id}/submit`.
- **P8.5c** — Propagation `user_skills` best-effort au succès d'un challenge legacy.
- **P8.6** — Helper `list_user_skill_fragments_or_backfill` + migration des 3 readers historiques.
- **P8.6b** — Helper `list_user_top_skills` + migration des 3 consumers `talent_search / github`.
- **P8.6c** — Leaderboards + data_export basculent sur `user_skills + skill_nodes`.
- **P8.7** — Migration 0071 DROP TABLE `skill_fragments` + cleanup consumers.
- **P8.8** — Cleanup commentaires + `docs/CHANGELOG-p8-completion.md`.

### P9 — Fin des hors-scope P8 (`dbcb28e` → `52ad13b`)

Terminée en 3 sous-phases :
- **P9.1** (`dbcb28e`) — Migration 0072 DROP `challenge_submissions.code|stdout|stderr` avec backfill vers `deliverables.artifact_metadata`. `create_from_challenge_submission` étendu (language, stdout, stderr).
- **P9.2** (`d9d402b`) — Migrations 0073 + 0074 : fusion `oss_bounties` + `oss_bounty_claims` dans `project_slices` + DROP tables. `routes/bounties.rs` intégralement réécrit. Auto-création de project miroir.
- **P9.3** (`52ad13b`) — Migration 0075 : `ALTER TABLE challenges RENAME TO challenge_templates`. 15 fichiers `src/` + 5 fichiers `tests/` updates SQL. API HTTP inchangée.

---

## Politique et gouvernance publique

### Initial public release (`97eae90`)

Premier commit public du repo.

### OSS standards (`1df8ca2`)

- LICENSE AGPL-3.0
- SECURITY.md
- CONTRIBUTING.md
- CODE_OF_CONDUCT.md

### Documentation (`2498eb7`, `08aff33`, `289bbe4`)

- README principal en anglais (ton narrative-mission), version française à `README.fr.md`.
- Templates GitHub : issues + PR.
