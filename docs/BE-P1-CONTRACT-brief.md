# BE-P1-CONTRACT — Contract testing complet (utoipa + schemathesis)

**Statut** : en cours · branche `feat/be-p1-contract`
**Effort estimé** : 30-40h · plusieurs sessions
**Livrable final** : PR unique batchée quand 100% terminé

---

## 1. Contexte — pourquoi cette tâche existe

Le backend Skilluv (Rust axum) sert 3 fronts (skilluv-frontend, skilluv-admin, skilluv-ai) qui consomment ~593 endpoints. Aujourd'hui **aucun contrat OpenAPI machine-lisible n'existe** — les divergences payload back↔front sont découvertes en runtime (voir l'audit BE-P0-01..14 de fin juillet 2026, qui a mis à jour 20 tickets Trello).

L'objectif de BE-P1-CONTRACT :

1. **Générer un OpenAPI 3.1** exhaustif à partir d'annotations `#[utoipa::path]` sur chaque handler
2. **Servir cet OpenAPI** à `GET /api/openapi.json` + une UI interactive à `GET /api/docs`
3. **Fuzzer le contrat** en CI via [schemathesis](https://schemathesis.readthedocs.io/) — property-based testing qui envoie des requêtes conformes au schéma et vérifie que les réponses matchent

Résultat attendu : plus jamais un `field: content` côté front qui trouve un `field: body` côté back sans que la CI le détecte immédiatement.

---

## 2. Où travailler

- **Repo** : `git@github.com:skilluv/skilluv-backend.git` (org Skilluv, GitHub)
- **Répertoire local** : le dev le clone où il veut
- **Branche de travail** : `feat/be-p1-contract` (déjà créée, l'infra est committée dessus)
- **Base** : `master` — rebase régulier recommandé
- **Ne PAS push tant que 100% terminé** — voir §11

Vérification initiale :

```bash
git clone git@github.com:skilluv/skilluv-backend.git
cd skilluv-backend
git checkout feat/be-p1-contract
git log --oneline -5   # devrait montrer le commit c3ec13c "feat(openapi): BE-P1-CONTRACT infrastructure"
cargo check            # doit passer sans erreur
```

Si `cargo check` échoue au démarrage, faire un rebase sur master avant d'aller plus loin :

```bash
git fetch origin
git rebase origin/master
```

---

## 3. Ce qui est déjà fait (commit c3ec13c)

L'infrastructure est en place. Ne pas la reécrire :

### 3.1 Dépendances ajoutées à `Cargo.toml`

```toml
utoipa = { version = "5", features = ["axum_extras", "chrono", "uuid", "decimal"] }
utoipa-swagger-ui = { version = "9", features = ["axum"] }
utoipa-axum = "0.2"
```

### 3.2 Fichier `src/openapi.rs`

Contient la struct `ApiDoc` avec `#[derive(OpenApi)]` + les tags par domaine + la fonction `attach()` qui wire les endpoints.

**IMPORTANT** : la macro `#[openapi(paths(...), components(...))]` doit être **enrichie manuellement** au fur et à mesure que le dev annote des handlers. C'est le principal travail mécanique de cette tâche. Voir §5.

### 3.3 Endpoints exposés

- `GET /api/openapi.json` — retourne le schéma OpenAPI 3.1 en JSON
- `GET /api/docs` — Swagger UI

Aujourd'hui le schéma est **vide** (0 chemins). L'objectif de cette tâche : y arriver à **593/593** chemins.

### 3.4 Module déclaré dans `src/lib.rs`

```rust
pub mod openapi;
```

Et l'attachement au router :

```rust
let router = openapi::attach(router);
```

---

## 4. Scope exact — ce qu'il faut faire

### 4.1 Ampleur

- **593 handlers async** dans `src/routes/*.rs` (86 fichiers) à annoter avec `#[utoipa::path(...)]`
- **~155 handlers** renvoient `Json<serde_json::Value>` (JSON construit à la main via `json!({})`) — ils doivent **être refactorisés** en structs typées avant annotation (sinon zéro valeur contrat)
- **~438 handlers** renvoient déjà des structs typées — annotation directe
- Toutes les structs request/response doivent porter `#[derive(utoipa::ToSchema)]`
- Toutes les paths, params, query params, request bodies, responses (statuts + bodies) doivent être documentés

### 4.2 Ne pas oublier

- **Workflow CI** `.github/workflows/contract-test.yml` — schemathesis contre l'API démarrée
- **Documentation** dans `docs/API-ROUTES.md` — noter que le canonique est désormais `/api/docs` (Swagger)
- **Rate-limiting sur `/api/docs`** — la protéger côté prod (soit auth-gated, soit désactivée en prod via env flag `SKILLUV_EXPOSE_SWAGGER=1`)

### 4.3 Hors scope

- **Ne pas** refactorer la logique métier des handlers — juste leur signature de retour
- **Ne pas** modifier les migrations, les services, les modèles DB
- **Ne pas** toucher aux tests d'intégration existants (ils doivent continuer à passer verbatim)
- **Ne pas** ajouter de dépendances autres que utoipa (déjà présentes)

---

## 5. Méthodologie handler-par-handler

### 5.1 Cas facile — handler avec response typée existante

Exemple, `src/routes/badges.rs` :

```rust
// AVANT
async fn list_user_badges(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<BadgeWithEarnedAt>>, AppError> {
    // ...
}
```

**Ajouts nécessaires** :

1. Sur la struct `BadgeWithEarnedAt` (`src/models/`) : ajouter `utoipa::ToSchema` au derive :
   ```rust
   #[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
   pub struct BadgeWithEarnedAt { ... }
   ```

2. Sur le handler : ajouter l'annotation `#[utoipa::path]` juste au-dessus :
   ```rust
   #[utoipa::path(
       get,
       path = "/api/badges/me",
       tag = "gamification",
       responses(
           (status = 200, body = Vec<BadgeWithEarnedAt>, description = "Badges gagnés par l'utilisateur"),
           (status = 401, description = "Non authentifié"),
       ),
       security(("cookie_auth" = [])),
   )]
   async fn list_user_badges(...) { ... }
   ```

3. Enregistrer le handler dans `src/openapi.rs` — ajouter à `paths(...)` :
   ```rust
   #[openapi(
       paths(
           crate::routes::badges::list_user_badges,
           // ...
       ),
       components(
           schemas(BadgeWithEarnedAt, /* ... */),
       ),
       // ...
   )]
   ```

4. `cargo check` — doit passer.

### 5.2 Cas dur — handler avec `Json<serde_json::Value>`

Exemple, `src/routes/profile.rs::public_profile` :

```rust
// AVANT
async fn public_profile(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    // ... construit un json!({...}) énorme ...
    Ok(Json(build_response(json!({
        "user": { "username": ..., "display_name": ..., ... },
        "stats": { ... },
        "skill_tree": ...,
        "heatmap_summary": ...,
        "badges": ...,
    }))))
}
```

**Étapes** :

1. **Créer une struct de réponse** (soit dans le même fichier route, soit dans `src/models/api/` si on veut la partager). Convention : `<Handler>Response` en `PascalCase`.

   ```rust
   #[derive(Debug, Serialize, utoipa::ToSchema)]
   struct PublicProfileResponse {
       user: PublicProfileUser,
       stats: PublicProfileStats,
       #[serde(skip_serializing_if = "Option::is_none")]
       skill_tree: Option<Vec<SkillTreeEntry>>,
       #[serde(skip_serializing_if = "Option::is_none")]
       heatmap_summary: Option<HeatmapSummary>,
       #[serde(skip_serializing_if = "Option::is_none")]
       badges: Option<Vec<BadgeEntry>>,
   }

   #[derive(Debug, Serialize, utoipa::ToSchema)]
   struct PublicProfileUser {
       username: String,
       display_name: String,
       // ... tous les champs
   }
   // ... idem pour Stats, SkillTreeEntry, HeatmapSummary, BadgeEntry
   ```

2. **Refactorer le handler** pour construire cette struct au lieu du `json!({})` :

   ```rust
   async fn public_profile(...) -> Result<Json<PublicProfileResponse>, AppError> {
       // ... même logique métier ...
       Ok(Json(PublicProfileResponse {
           user: PublicProfileUser { username: ..., ... },
           stats: PublicProfileStats { ... },
           skill_tree: if privacy.show_skill_tree { Some(skill_tree) } else { None },
           heatmap_summary: if privacy.show_heatmap { Some(...) } else { None },
           badges: if privacy.show_badges { Some(badges_data) } else { None },
       }))
   }
   ```

3. **Ne pas oublier le wrapper `build_response`** qui ajoute `meta: { request_id, timestamp }`. Deux options :
   - Option A (recommandée) : créer une struct générique `ApiResponse<T>` avec `data: T` + `meta: MetaInfo`, l'utiliser partout.
   - Option B : garder l'inline `Json<serde_json::Value>` pour l'enveloppe et typer juste le champ `data`. Moins propre mais moins invasif.

   **Décision à trancher tôt** : option A si tu veux du contrat 100% strict, option B si tu veux limiter la casse. Faire le choix, le documenter au début du travail, s'y tenir.

4. Annotation `#[utoipa::path]` sur le handler + enregistrement dans `src/openapi.rs` (comme §5.1).

5. `cargo check`.

### 5.3 Structures request body (POST / PATCH / PUT)

Pareil que les responses. Toute struct `#[derive(Deserialize)]` utilisée dans `Json<T>` en input doit porter `utoipa::ToSchema` :

```rust
#[derive(Deserialize, utoipa::ToSchema)]
struct LoginRequest {
    email: String,
    password: String,
    #[serde(default)]
    totp_code: Option<String>,
}
```

Et dans l'annotation :

```rust
#[utoipa::path(
    post,
    path = "/api/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, body = LoginResponse),
        (status = 401, body = ErrorResponse, description = "Invalid credentials"),
    ),
)]
```

### 5.4 Path params + query params

```rust
#[utoipa::path(
    get,
    path = "/api/users/{id}/rank-history",
    tag = "profile",
    params(
        ("id" = uuid::Uuid, Path, description = "User ID"),
    ),
    responses(...)
)]
```

Query params :

```rust
#[derive(Deserialize, utoipa::IntoParams)]
struct FeedQuery {
    #[serde(default)]
    limit: Option<u32>,
    cursor: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/feed",
    tag = "feed",
    params(FeedQuery),
    responses(...)
)]
```

### 5.5 Réponses d'erreur — struct `ErrorResponse` réutilisable

À définir dans `src/errors/codes.rs` (à côté de `AppError`) puis dériver `ToSchema` dessus. Chaque handler référence `ErrorResponse` dans ses réponses non-2xx.

```rust
#[derive(Serialize, utoipa::ToSchema)]
pub struct ErrorResponse {
    pub error: ErrorObject,
    pub meta: MetaInfo,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ErrorObject {
    /// Code métier stable (voir la table dans docs/errors.md)
    pub code: String,
    pub message: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct MetaInfo {
    pub request_id: String,
    pub timestamp: String,
}
```

### 5.6 Security schemes

Configurer les schémas d'auth dans `src/openapi.rs` via un `Modify` custom :

```rust
struct SecurityAddon;
impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::*;
        openapi.components.as_mut().unwrap().add_security_scheme(
            "cookie_auth",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("skilluv_session"))),
        );
        openapi.components.as_mut().unwrap().add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()
            ),
        );
    }
}
```

Et référence dans `#[derive(OpenApi)]` :

```rust
#[openapi(
    modifiers(&SecurityAddon),
    // ...
)]
```

---

## 6. Ordre de travail suggéré (priorité décroissante)

Suivre cet ordre pour :
- avoir de la valeur contract testing tôt (auth, profile, forum → très consommés par le front)
- éviter les blocages inter-modules
- pouvoir commit checkpoints propres

| Phase | Fichiers | Effort | Priorité |
|---|---|---|---|
| 1 | `src/routes/auth.rs` (25 handlers, 19 en Json<Value>) | 4-5h |  critique |
| 2 | `src/errors/codes.rs` — ajouter `ErrorResponse` réutilisable | 30min | bloquant |
| 3 | `src/routes/webauthn.rs`, `magic_link.rs`, `oauth.rs` (auth suite) | 3h | élevée |
| 4 | `src/routes/profile.rs`, `user_profile.rs`, `profile_extras.rs` | 3h | élevée |
| 5 | `src/routes/challenges.rs`, `slices.rs`, `challenge_teams.rs`, `challenge_tags.rs` | 4h | élevée |
| 6 | `src/routes/forum.rs`, `dm.rs`, `social.rs`, `notifications.rs`, `contact.rs` | 3-4h | élevée |
| 7 | `src/routes/feed.rs`, `explore.rs`, `orientations.rs`, `onboarding.rs` | 2-3h | élevée |
| 8 | `src/routes/guild.rs`, `guild-wars`, `tournament.rs`, `seasons.rs` | 3h | moyenne |
| 9 | `src/routes/enterprise*.rs`, `enterprise_credits.rs`, `enterprise_kyc.rs`, `enterprise_sso.rs`, `talent_search*.rs`, `talent_lists.rs` | 4-5h | élevée (business B2B) |
| 10 | `src/routes/talent_wallet.rs`, `stripe`, `momo`, `bounty` (payouts) | 2h | moyenne |
| 11 | `src/routes/badges.rs`, `gamification.rs`, `capability.rs`, `attestation.rs`, `leaderboard.rs`, `tracks.rs`, `skills.rs`, `mentorship.rs`, `projects.rs` | 4-5h | moyenne |
| 12 | `src/routes/moderation.rs`, `community.rs`, `reports.rs`, `push.rs`, `github.rs` | 2h | moyenne |
| 13 | `src/routes/admin*.rs` (admin, admin_moderation, admin_fraud, admin_community, admin_dashboard, admin_content_ops, admin_project) | 3-4h | faible |
| 14 | `src/routes/scim.rs`, `developer.rs`, `sponsored_challenges.rs`, `sandbox.rs`, `tenant.rs`, `ai_coach.rs`, `ai_job.rs`, `agency_client.rs`, `event.rs`, `certification.rs`, `enterprise_subscription.rs` | 3-4h | faible |
| 15 | `src/routes/i18n.rs`, `geo.rs`, `email_prefs.rs`, `legal.rs`, `legal_well_known.rs`, `metrics.rs`, `enterprise_pipeline.rs`, `enterprise_dashboard.rs`, `orientation.rs`, `deliverable.rs`, `review_queue.rs`, `season.rs`, `portfolio.rs`, `openapi.rs`, `public_api.rs`, `talent_search_v2.rs`, `talent_search_v3.rs`, `health.rs` | 3-4h | balance |
| 16 | Workflow CI `contract-test.yml` + doc `README` (section OpenAPI) + `docs/API-ROUTES.md` màj | 1-2h | final |

**Total réaliste : 42-50h** avec les vraies inconnues comptées. Le carnet initial disait 30-40h — sous-estimé.

---

## 7. Workflow CI `.github/workflows/contract-test.yml`

À créer **en fin de tâche** (après que tous les handlers sont annotés). Contenu attendu :

```yaml
name: Contract test
on:
  pull_request:
    paths:
      - 'src/**'
      - 'Cargo.lock'
      - 'Cargo.toml'
      - '.github/workflows/contract-test.yml'
  workflow_dispatch:

jobs:
  schemathesis:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:18-alpine
        env:
          POSTGRES_DB: skilluv
          POSTGRES_USER: skilluv
          POSTGRES_PASSWORD: skilluv_secret
        ports: ['5432:5432']
        options: --health-cmd pg_isready --health-interval 5s --health-timeout 3s --health-retries 5
      redis:
        image: redis:7-alpine
        ports: ['6379:6379']
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Start backend
        env:
          DATABASE_URL: postgres://skilluv:skilluv_secret@localhost:5432/skilluv
          REDIS_URL: redis://localhost:6379
          JWT_SECRET: schemathesis_test_secret_key_must_be_32_chars_or_more
          SKILLUV_EXPOSE_SWAGGER: "1"
        run: |
          cargo build --release
          ./target/release/skilluv-backend &
          for i in {1..30}; do
            curl -f http://localhost:3001/api/health && break || sleep 2
          done
      - name: Install schemathesis
        run: pip install schemathesis
      - name: Run schemathesis
        run: |
          schemathesis run \
            http://localhost:3001/api/openapi.json \
            --checks all \
            --hypothesis-max-examples=25 \
            --hypothesis-seed=42 \
            --workers=4 \
            --exitfirst
```

Notes :
- `--hypothesis-max-examples=25` — plafond bas pour respecter le quota GHA (chaque endpoint fuzzé 25 fois max)
- `--exitfirst` — stoppe au premier échec pour économiser du CI
- Le workflow tourne uniquement sur les PR qui touchent `src/**` — pas sur les changements de docs
- Aucun test contre `/api/docs` (Swagger UI est purement front)

---

## 8. Comment tester localement au fur et à mesure

Après chaque handler annoté :

```bash
cargo check                              # doit compiler
cargo run                                # démarrer le backend en local
curl -s http://localhost:3001/api/openapi.json | jq '.paths | keys' | head  # voir les chemins actuellement documentés
open http://localhost:3001/api/docs      # Swagger UI dans le navigateur
```

Pour valider un handler spécifique :

```bash
curl -s http://localhost:3001/api/openapi.json | jq '.paths."/api/auth/login"'
```

Test rapide schemathesis en local (Python + pip requis) :

```bash
pip install schemathesis
schemathesis run http://localhost:3001/api/openapi.json --checks all --hypothesis-max-examples=5
```

---

## 9. Anti-patterns à éviter

- FAIL Annoter un handler qui renvoie `Json<serde_json::Value>` **sans refactor typé** — utoipa documentera `type: object` (any) et schemathesis ne pourra rien vérifier. Aucune valeur.
- FAIL Créer une struct de réponse générique `type Response = serde_json::Value` — même problème.
- FAIL Utiliser `ToSchema` sur un enum sans configurer explicitement les variants — utoipa ne devine pas la sérialisation serde. Toujours `#[schema(as = ...)]` ou `#[schema(example = ...)]`.
- FAIL Oublier d'ajouter le handler à `paths(...)` dans `src/openapi.rs` — le handler compile mais n'apparaîtra pas dans le schéma.
- FAIL Copier-coller les mêmes descriptions partout — utoipa exige `description` explicite sur chaque réponse. Prendre le temps de rédiger des messages qui documentent vraiment.
- FAIL Toucher aux middlewares axum (`admin_gate`, `ensure_admin_2fa`, etc.) — hors scope.
- FAIL Regrouper trop de handlers dans un seul commit — voir §10.

---

## 10. Rythme de commits

**Un commit par fichier route annoté**, avec message clair :

```
feat(openapi): annotate src/routes/auth.rs — 25 handlers documented

- Refactored 19 Json<Value> handlers to typed responses
- Added ToSchema on LoginRequest, LoginResponse, TotpEnableResponse, ...
- Registered all 25 handlers in ApiDoc.paths in src/openapi.rs
- Added ErrorResponse component for non-2xx documentation
```

**Ne pas mettre de co-auteur** dans les commits (règle explicite de Jérémie — voir sa mémoire `feedback_verify_before_done`).

Vérification pré-commit :

```bash
cargo fmt              # format code
cargo clippy --all-targets --all-features -- -D warnings   # lint
cargo check --all-targets   # compile
cargo test --lib      # tests unitaires (les tests d'intégration Postgres ne devraient pas être affectés)
```

Si un des 4 échoue : **fix avant de commit**. La règle "pas de dette reportée" s'applique.

---

## 11. Règles push / PR — TRÈS IMPORTANT

**Ne PAS push tant que 100% des 593 handlers sont annotés et testés.** Raison : le user a une contrainte quota GitHub Actions et veut UNE seule PR batchée pour cette énorme tâche.

Ce qui déclencherait un push OK :
1. Tous les 86 fichiers de `src/routes/*.rs` ont leurs handlers annotés (100%)
2. Toutes les structs request/response ont `ToSchema`
3. `cargo check --all-targets` passe
4. `cargo clippy --all-targets --all-features -- -D warnings` passe
5. `cargo fmt --check` passe
6. `cargo test --lib` passe
7. Le workflow `contract-test.yml` est créé
8. Un smoke local a été fait : `curl http://localhost:3001/api/openapi.json | jq '.paths | length'` renvoie ≥ 590

Quand tout ça est OK :

```bash
git push -u origin feat/be-p1-contract
```

**Ne pas ouvrir la PR** — laisser Jérémie le faire (il veut valider l'état final avant que la CI ne tourne, à cause du quota GHA storage à 90%).

Prévenir Jérémie via WhatsApp / DM que la branche est prête à review.

Format du message :

```
BE-P1-CONTRACT prêt.
Branche: feat/be-p1-contract
Handlers annotés: XXX/593
Structs typées créées: YY (parmi les 155 refactorées)
Local: cargo check + clippy + fmt + tests OK, schemathesis smoke run passe.
Tu peux ouvrir la PR.
```

---

## 12. Si tu es bloqué

Contactable :
- Jérémie Zitti (fondateur, tech lead) — jeremiezitti@gmail.com
- Réponse sous 24h max en semaine

Blockers légitimes qui méritent un ping :
- Un handler renvoie un type que utoipa refuse (rare, mais possible sur des enums serde avec `#[serde(untagged)]`)
- Une struct existante ne peut pas dériver `ToSchema` (dep externe qui ne le fournit pas — solution : wrapper)
- Un design choice te bloque plus de 30 minutes (ex: option A vs B pour `ApiResponse<T>` en §5.2.3)

Blockers non-légitimes (à débrouiller seul avec la doc) :
- Comment déclarer un query param optionnel → doc utoipa `IntoParams`
- Comment documenter un webhook Stripe (payload dynamique) → utiliser `body = serde_json::Value` avec description explicite

Docs utiles :
- utoipa : https://docs.rs/utoipa
- utoipa-swagger-ui : https://docs.rs/utoipa-swagger-ui
- schemathesis : https://schemathesis.readthedocs.io/
- Codebase similaire (référence) : https://github.com/juhaku/utoipa/tree/master/examples/simple-axum

---

## 13. Références internes

- **Audit initial** : commits sur `master` autour de fin juillet 2026 — BE-P0-01..14 + BE-P0-34..40 (20 tickets Trello) qui documentent des divergences payload que ce contract testing aurait détectées automatiquement
- **Trello card** : `TATOA267` (BE-P1-CONTRACT) — mettre à jour le statut au fur et à mesure (En cours pendant le travail, PR ouverte quand poussé)
- **CLAUDE.md** : conventions du repo (règle "pas de co-auteur", "vérifier avant de dire fait", "pas de dette reportée")
- **CI existant** : `.github/workflows/ci.yml` (build & lint + integration tests) — le nouveau `contract-test.yml` s'ajoute en parallèle sans modifier celui-ci

---

## 14. Definition of Done

- [ ] 593 handlers annotés `#[utoipa::path]` (0 exception, même les webhooks internes)
- [ ] 155 handlers refactorés de `Json<serde_json::Value>` vers structs typées
- [ ] Toutes les structs request/response dérivent `ToSchema`
- [ ] `ApiDoc` (src/openapi.rs) référence tous les handlers dans `paths(...)`
- [ ] `ErrorResponse` utilisé partout où non-2xx est possible
- [ ] Security schemes `cookie_auth` + `bearer_auth` définis
- [ ] `.github/workflows/contract-test.yml` créé et testé localement (au moins un `--dry-run`)
- [ ] `cargo check --all-targets` OK
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` OK
- [ ] `cargo fmt --check` OK
- [ ] `cargo test --lib` OK (les tests unitaires — les tests d'intégration Postgres ne sont pas cassés)
- [ ] `curl /api/openapi.json | jq '.paths | length' >= 590`
- [ ] `curl /api/openapi.json | jq '.components.schemas | length' >= 250` (estimation basse)
- [ ] Smoke local schemathesis : `schemathesis run ... --hypothesis-max-examples=3` passe sans erreur fatale
- [ ] Commits granulaires (un par fichier route), messages descriptifs, aucun co-auteur
- [ ] Branche `feat/be-p1-contract` pushée mais PR **non ouverte** (Jérémie le fait)
- [ ] Message de notification envoyé à Jérémie avec le récap (voir §11)

---

*Fin du brief · Bonne route*

*— Jérémie · rédigé le 2026-07-28*
