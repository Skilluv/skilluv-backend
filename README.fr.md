> 🇬🇧 [English version](README.md) · 🇫🇷 Version française (cette page)

---

# Skilluv Backend

Backend API pour Skilluv, plateforme gamifiee de demonstration de competences techniques. Construit en Rust avec Axum, PostgreSQL, Redis, MinIO et Judge0 CE.

## Table des matieres

- [Architecture](#architecture)
- [Stack technique](#stack-technique)
- [Prerequis](#prerequis)
- [Installation](#installation)
- [Configuration](#configuration)
- [Lancement](#lancement)
- [Tests](#tests)
- [Structure du projet](#structure-du-projet)
- [Fonctionnalites](#fonctionnalites)
- [API](#api)
- [Base de donnees](#base-de-donnees)
- [Services externes](#services-externes)
- [Securite](#securite)
- [WebSocket](#websocket)

---

## Architecture

Le backend suit une architecture modulaire en couches :

```
Clients (SvelteKit SSR, navigateurs, API tierces)
    |
    v
[Axum HTTP Server] --- port 3001
    |
    +-- routes/        --> Handlers HTTP (validation, serialisation)
    +-- middleware/     --> Auth JWT, Rate Limiting, Security Headers, API Key
    +-- services/      --> Logique metier (auth, leaderboard, notification, storage, webhook)
    +-- models/        --> Structs Rust mappes sur les tables PostgreSQL (sqlx::FromRow)
    +-- config/        --> Configuration depuis variables d'environnement
    +-- websocket/     --> WebSocket manager avec systeme de rooms
    +-- errors/        --> Gestion centralisee des erreurs avec codes HTTP
    |
    +-- [PostgreSQL]   --> Stockage principal (19 migrations)
    +-- [Redis]        --> Cache, tokens, leaderboards (Sorted Sets), rate limiting, notifications
    +-- [MinIO]        --> Stockage objets S3-compatible (avatars)
    +-- [Judge0 CE]    --> Execution de code sandboxee (35 langages)
```

Communication avec le futur service IA (Python FastAPI) prevue via gRPC.

## Stack technique

| Composant | Technologie | Version |
|-----------|-------------|---------|
| Langage | Rust | Edition 2024 |
| Framework web | Axum | 0.8 |
| Runtime async | Tokio | 1.x |
| Base de donnees | PostgreSQL | 17 (Alpine) |
| ORM/Driver | SQLx | 0.8 (compile-time checked) |
| Cache/Broker | Redis | 7 (Alpine) |
| Stockage objets | MinIO | latest |
| Execution de code | Judge0 CE | 1.13.1 |
| Auth | JWT (jsonwebtoken) + Argon2 + TOTP |
| Serialisation | serde + serde_json |
| HTTP client | reqwest | 0.12 |
| S3 client | rust-s3 | 0.35 |
| Webhooks | HMAC-SHA256 (hmac + sha2) |

## Prerequis

- Rust 1.80+ (edition 2024)
- Docker Desktop
- Git

## Installation

```bash
# Cloner le repo
git clone https://github.com/jeremie0342/skilluv-backend.git
cd skilluv-backend

# Copier la configuration
cp .env.example .env
# (ou creer .env manuellement, voir section Configuration)

# Lancer les services Docker
docker compose up -d postgres redis minio

# Compiler le projet
cargo build
```

## Configuration

Creer un fichier `.env` a la racine :

```env
DATABASE_URL=postgres://skilluv:skilluv_secret@localhost:5433/skilluv
REDIS_URL=redis://localhost:6379
HOST=0.0.0.0
PORT=3001
JWT_SECRET=dev-secret-change-in-production
BASE_URL=http://localhost:3001
JUDGE0_URL=http://localhost:2358
MINIO_ENDPOINT=http://localhost:9004
MINIO_ACCESS_KEY=skilluv
MINIO_SECRET_KEY=skilluv_secret
MINIO_BUCKET=avatars
RUST_LOG=skilluv_backend=debug,tower_http=debug
```

### Ports utilises

| Service | Port | Notes |
|---------|------|-------|
| Backend Axum | 3001 | Configurable via PORT |
| PostgreSQL | 5433 | Mappe sur 5432 interne |
| Redis | 6379 | Standard |
| MinIO API | 9004 | Configurable |
| MinIO Console | 9005 | Interface web admin |
| Judge0 | 2358 | Optionnel, fallback sans |

## Lancement

```bash
# Services Docker (PostgreSQL, Redis, MinIO)
docker compose up -d postgres redis minio

# Optionnel : Judge0 pour l'execution de code reelle
docker compose up -d judge0 judge0-postgres judge0-redis judge0-workers

# Lancer le serveur (les migrations s'executent automatiquement)
cargo run
```

Le serveur demarre sur `http://localhost:3001`. Verifier avec :

```bash
curl http://localhost:3001/api/health
```

Reponse attendue :
```json
{
  "data": {
    "status": "degraded",
    "version": "0.1.0",
    "services": {
      "postgresql": "connected",
      "redis": "connected",
      "judge0": "disconnected"
    }
  }
}
```

Le status est `degraded` si Judge0 n'est pas lance (le fallback d'evaluation fonctionne sans).

## Tests

Les tests d'integration testent les routes contre une vraie base PostgreSQL. Chaque test cree une base de donnees isolee, execute les migrations, lance un serveur sur un port aleatoire, et nettoie apres.

```bash
# Prerequis : PostgreSQL et Redis doivent tourner
docker compose up -d postgres redis minio

# Lancer tous les tests
cargo test -- --test-threads=1

# Tests par module
cargo test --test auth_test -- --test-threads=1
cargo test --test profile_test -- --test-threads=1
cargo test --test moderation_test -- --test-threads=1
cargo test --test api_keys_test -- --test-threads=1
```

L'option `--test-threads=1` est recommandee pour eviter les conflits Redis entre tests paralleles.

### Couverture actuelle

| Module | Tests | Couverture |
|--------|-------|------------|
| Auth | 8 | Register, login (email/username), me, delete account, duplicates, wrong password |
| Profile | 5 | Update bio, privacy settings, heatmap masque, profil inactif, display name |
| Moderation | 3 | Report, ban/unban + login verification, audit log |
| API Keys | 3 | Create key, public API avec/sans key |
| **Total** | **19** | Routes critiques couvertes |

## Structure du projet

```
skilluv-backend/
|-- Cargo.toml                    # Dependances Rust
|-- docker-compose.yml            # PostgreSQL, Redis, MinIO, Judge0
|-- .env                          # Variables d'environnement (git-ignored)
|-- migrations/                   # 19 fichiers SQL (executees auto au demarrage)
|   |-- 0001_init.sql             # Extensions uuid-ossp, pgcrypto
|   |-- 0002_create_users.sql     # Table users (25+ colonnes)
|   |-- 0003_create_challenges.sql # Challenges, submissions, skill_fragments
|   |-- 0004_create_activity_log.sql # Heatmap d'activite
|   |-- 0005_create_badges.sql    # Badges + 9 badges initiaux
|   |-- 0006_create_enterprises.sql # Entreprises + membres
|   |-- 0007_create_talent_lists.sql # Bookmarks + listes nommees
|   |-- 0008_create_contact_system.sql # Interets, conversations, messages, blocks
|   |-- 0009_create_notifications.sql # Notifications persistantes
|   |-- 0010_add_talent_search_indexes.sql # Full-text search tsvector
|   |-- 0011_add_recruiter_role.sql # Role recruiter
|   |-- 0012_add_profile_fields.sql # Bio, avatar, socials, privacy
|   |-- 0013_create_reports.sql   # Signalements
|   |-- 0014_create_audit_log.sql # Audit log admin
|   |-- 0015_add_challenge_tags.sql # Tags + categories + champs community
|   |-- 0016_create_challenge_votes.sql # Votes challenges
|   |-- 0017_create_teams.sql     # Equipes + timer
|   |-- 0018_create_api_keys.sql  # Cles API
|   |-- 0019_create_webhooks.sql  # Webhooks + deliveries
|-- src/
|   |-- lib.rs                    # Point d'entree bibliotheque (pour les tests)
|   |-- main.rs                   # Point d'entree binaire
|   |-- config/
|   |   |-- app.rs                # AppConfig depuis env vars
|   |   |-- database.rs           # Connexion PostgreSQL (PgPool)
|   |   |-- redis_config.rs       # Connexion Redis (ConnectionManager)
|   |-- errors/
|   |   |-- codes.rs              # Enum AppError avec codes HTTP + JSON
|   |-- middleware/
|   |   |-- auth.rs               # AuthUser extractor (JWT cookie)
|   |   |-- api_key.rs            # ApiKeyAuth extractor (Bearer token)
|   |   |-- rate_limit.rs         # RateLimiter Redis-backed
|   |   |-- security_headers.rs   # Headers de securite (tower Layer)
|   |-- models/
|   |   |-- user.rs               # User, UserPrivate, UserPublic
|   |   |-- challenge.rs          # Challenge, ChallengeSubmission, SkillFragment
|   |   |-- badge.rs              # Badge, UserBadge, BadgeWithEarnedAt
|   |   |-- enterprise.rs         # Enterprise, EnterpriseMember
|   |   |-- contact.rs            # InterestRequest, Conversation, Message
|   |   |-- notification.rs       # Notification
|   |   |-- talent_list.rs        # TalentList, EnterpriseBookmark
|   |-- routes/
|   |   |-- auth.rs               # 17 routes auth
|   |   |-- challenges.rs         # 6 routes challenges
|   |   |-- sandbox.rs            # 4 routes execution de code
|   |   |-- admin.rs              # 7 routes admin challenges
|   |   |-- gamification.rs       # 4 routes skill tree + heatmap
|   |   |-- leaderboard.rs        # 3 routes classements Redis
|   |   |-- profile.rs            # 1 route profil public SSR
|   |   |-- user_profile.rs       # 7 routes edition profil
|   |   |-- enterprise.rs         # 7 routes comptes entreprise
|   |   |-- talent_search.rs      # 2 routes recherche talents
|   |   |-- talent_lists.rs       # 10 routes bookmarks + listes
|   |   |-- contact.rs            # 10 routes interets + messagerie
|   |   |-- notifications.rs      # 4 routes notifications
|   |   |-- enterprise_dashboard.rs # 2 routes dashboard entreprise
|   |   |-- reports.rs            # 3 routes signalements
|   |   |-- admin_moderation.rs   # 8 routes moderation admin
|   |   |-- challenge_tags.rs     # 3 routes tags + categories
|   |   |-- community.rs          # 6 routes challenges communautaires
|   |   |-- admin_community.rs    # 3 routes curation admin
|   |   |-- challenge_teams.rs    # 6 routes equipes + timer
|   |   |-- developer.rs          # 10 routes API keys + webhooks
|   |   |-- public_api.rs         # 3 routes API publique v1
|   |   |-- openapi.rs            # 1 route spec OpenAPI
|   |   |-- health.rs             # 1 route health check
|   |-- services/
|   |   |-- auth.rs               # Hashing Argon2, JWT, refresh tokens
|   |   |-- email.rs              # Service email (log en dev, Brevo en prod)
|   |   |-- sandbox.rs            # Client Judge0 (execute, poll, languages)
|   |   |-- leaderboard.rs        # LeaderboardService (Redis Sorted Sets)
|   |   |-- notification.rs       # NotificationService (DB + WS + Redis counter)
|   |   |-- storage.rs            # StorageService (MinIO S3, avatars)
|   |   |-- webhook.rs            # WebhookService (deliver, HMAC sign, retry)
|   |-- websocket/
|       |-- manager.rs            # WsManager (rooms, broadcast, send_to_user)
|       |-- handler.rs            # WebSocket upgrade handler
|-- tests/
|   |-- common/mod.rs             # TestApp helper (spawn server, DB isolee)
|   |-- auth_test.rs              # 8 tests auth
|   |-- profile_test.rs           # 5 tests profil
|   |-- moderation_test.rs        # 3 tests moderation
|   |-- api_keys_test.rs          # 3 tests API keys
|-- docs/
    |-- API-ROUTES.md             # Reference complete des 117+ routes
```

## Fonctionnalites

Le backend est organise en 9 epics :

### Epic 1 : Auth et Onboarding
- Inscription avec email, username, mot de passe (Argon2)
- Login par email ou username
- JWT dans cookie HttpOnly Secure (15 min) + refresh token Redis (7 jours)
- Verification email
- Reset de mot de passe
- 2FA TOTP (Google Authenticator) et 2FA par email
- Suppression de compte RGPD (effacement total)

### Epic 2 : Sandbox et Execution de Code
- Execution de code via Judge0 CE (35 langages supportes)
- Mode synchrone et asynchrone (polling par token)
- Fallback d'evaluation quand Judge0 est indisponible
- WebSocket avec systeme de rooms (user, leaderboard, challenge)

### Epic 3 : Challenges et Gamification
- CRUD admin pour les challenges
- Catalogue avec filtres (domaine, difficulte), pagination, verrouillage par prerequis
- Start/submit avec evaluation automatique (Judge0 ou fallback)
- Fragments de competence (recompense par challenge)
- Fragments d'echec (recompense partielle) + bonus perseverance
- Arbre de competences par domaine/sous-competence
- Heatmap d'activite (365 jours)
- Streak quotidien avec bonus milestones (7j, 30j, 100j, 365j)
- Titres evolutifs : apprenti (0-499) -> artisan (500-1999) -> maitre (2000-4999) -> legende (5000+)
- Etoiles dorees pour les legendes (1 par 100 fragments au-dela de 5000)
- 9 badges automatiques (premier challenge, streaks, fragments)

### Epic 4 : Profil Public et Classements
- Profil public SSR-ready (accessible sans auth pour SvelteKit)
- Donnees : user info, stats, skill tree, heatmap 30j, badges
- Classements Redis Sorted Sets (global + par domaine)
- Periodes : alltime, weekly (auto-expire 8j), monthly (auto-expire 35j)
- Rang personnel dans /me et /leaderboards/{domain}/me
- Sync automatique apres chaque soumission reussie

### Epic 5 : Dashboard Entreprise et Recrutement
- Inscription entreprise (user role=enterprise + table enterprises)
- Profil entreprise (nom, description, site, logo, secteur, taille)
- Gestion d'equipe : invitation recruteurs par email, acceptation par token
- Recherche de talents : full-text search (tsvector), filtres (domaine, titre, pays, fragments), tri, pagination
- Carte talent legere pour les resultats de recherche
- Bookmarks (sauvegarder des profils)
- Listes nommees de talents (CRUD + ajout/retrait de membres)
- Systeme de contact hybride : demande d'interet avec message initial
- Acceptation/refus par le talent (cooldown 30 jours apres refus)
- Messagerie dans les conversations ouvertes
- Blocage d'entreprise (ferme les conversations)
- Notifications persistantes (DB) + temps reel (WebSocket) + compteur Redis
- Dashboard entreprise : stats plateforme (talents par domaine, niveaux) + stats propres (bookmarks, listes, interets)

### Epic 6 : Profil Editable
- Modification bio, liens sociaux (GitHub, LinkedIn, Twitter, site web), pays
- Upload avatar (multipart, JPEG/PNG/WebP, max 2MB, stocke dans MinIO)
- Suppression avatar
- Parametres de confidentialite : masquer heatmap, skill tree, badges, streak dans le profil public
- Modification display name et domaine de competence

### Epic 7 : Moderation et Securite
- Rate limiting Redis-backed (auth 10/min, sandbox 20/min, contact 5/h)
- Headers de securite (X-Frame-Options, HSTS, CSP, XSS Protection, nosniff)
- Signalement de contenu/utilisateurs (spam, harcelement, triche, etc.)
- Ban/unban admin avec consequences completes (revoke tokens, ZREM leaderboards, fermeture conversations, notification)
- Audit log des actions admin (action, cible, details, IP, timestamp)
- Dashboard moderation (users bannis, reports pending/resolved, actions recentes)

### Epic 8 : Challenges Avances
- Tags et categories (14 tags initiaux : 10 topics + 4 niveaux)
- Challenges communautaires soumis par les utilisateurs
- Workflow de curation : draft -> review -> approved/rejected par admin
- Systeme de votes (upvote/downvote) avec classement par popularite
- Challenges en equipe : creation equipe, rejoindre, soumission collective
- Timer avec expiration : expires_at sur les soumissions, rejet automatique si depasse
- Extension de timer par admin

### Epic 9 : API Publique et Documentation
- Cles API (format sk_live_xxx, hash Argon2, permissions granulaires)
- Gestion des cles : creation, listing, revocation, regeneration, stats d'utilisation
- Webhooks : enregistrement, evenements (challenge.completed, badge.earned, title.changed, leaderboard.updated)
- Livraison webhook avec signature HMAC-SHA256 (header X-Skilluv-Signature)
- Desactivation automatique apres 10 echecs consecutifs
- 3 endpoints API publique v1 (profil, badges, skills) authentifies par cle API
- Documentation OpenAPI 3.1.0 auto-generee (113 paths, 13 tags)

## API

La documentation complete de toutes les routes est dans `docs/API-ROUTES.md`.

Acces rapide a la spec OpenAPI :
```bash
curl http://localhost:3001/api/docs/openapi.json
```

### Format de reponse standard

Succes :
```json
{
  "data": { ... },
  "meta": {
    "request_id": "uuid",
    "timestamp": "2026-03-21T12:00:00+00:00"
  }
}
```

Avec pagination :
```json
{
  "data": [ ... ],
  "pagination": {
    "page": 1,
    "per_page": 20,
    "total": 150,
    "total_pages": 8
  },
  "meta": { ... }
}
```

Erreur :
```json
{
  "error": {
    "code": "AUTH_UNAUTHORIZED",
    "message": "Unauthorized"
  },
  "meta": { ... }
}
```

### Authentification

Trois methodes selon le contexte :

1. **Cookie JWT** (HttpOnly, Secure, SameSite=Strict) : utilise par le frontend SvelteKit. Le cookie est set automatiquement au login/register.

2. **API Key** (Bearer token) : pour les integrations tierces via l'API publique v1. Header `Authorization: Bearer sk_live_xxx`.

3. **Sans auth** : routes publiques marquees "SSR-ready" pour le rendu cote serveur SvelteKit (profils, leaderboards, challenges populaires, tags).

## Base de donnees

19 migrations executees automatiquement au demarrage. Tables principales :

| Table | Description |
|-------|-------------|
| users | Utilisateurs (25+ colonnes : auth, profil, gamification, social) |
| challenges | Challenges avec config (domaine, difficulte, mode, timer, communautaire) |
| challenge_submissions | Soumissions avec code, resultat, fragments, timer |
| skill_fragments | Fragments par user/domaine/sous-competence |
| user_activity | Activite quotidienne (heatmap) |
| badges / user_badges | Badges et attributions |
| enterprises / enterprise_members | Comptes entreprise et equipes |
| enterprise_bookmarks | Talents bookmarkes |
| talent_lists / talent_list_members | Listes nommees de talents |
| interest_requests | Demandes d'interet enterprise -> talent |
| conversations / messages | Messagerie |
| enterprise_blocks | Blocages |
| notifications | Notifications persistantes |
| reports | Signalements |
| admin_audit_log | Audit des actions admin |
| challenge_tags / challenge_tag_map | Tags et associations |
| challenge_votes | Votes challenges communautaires |
| challenge_teams / team_members | Equipes |
| api_keys | Cles API |
| webhooks / webhook_deliveries | Webhooks et log de livraison |
| user_privacy | Parametres de confidentialite |

## Services externes

### PostgreSQL
Stockage principal. Connexion via SQLx avec pool de 20 connexions. Full-text search via tsvector pour la recherche de talents.

### Redis
Utilise pour :
- Tokens d'authentification (refresh, email verification, password reset, 2FA)
- Leaderboards (Sorted Sets avec ZADD/ZREVRANGE/ZREVRANK)
- Rate limiting (INCR + EXPIRE)
- Compteur de notifications non lues
- Tokens d'invitation recruteur

### MinIO
Stockage S3-compatible pour les avatars utilisateurs. Bucket `avatars` cree automatiquement au demarrage. Fichiers stockes sous `{user_id}.{ext}`.

### Judge0 CE
Moteur d'execution de code sandboxe. Supporte 35 langages (20 Tier 1 + 15 Tier 2). Le backend fonctionne sans Judge0 grace a un systeme de fallback pour l'evaluation des challenges.

## Securite

- Mots de passe hashes avec Argon2
- JWT avec expiration courte (15 min) dans cookie HttpOnly Secure SameSite=Strict
- 2FA optionnel : TOTP (Google Authenticator) ou email
- Rate limiting par IP (auth) et par user/enterprise (sandbox, contact)
- Headers de securite sur toutes les reponses (HSTS, CSP, X-Frame-Options, nosniff)
- Cles API hashees en base (seul le prefixe est stocke en clair)
- Webhooks signes avec HMAC-SHA256
- Signalement de contenu avec moderation admin
- Audit log des actions administratives
- Suppression de compte RGPD avec effacement complet des donnees

## WebSocket

Connexion via `ws://localhost:3001/ws` avec authentification par cookie JWT.

Rooms disponibles :
- `user:{id}` : notifications personnelles
- `leaderboard:{domain}` : mises a jour classement
- `challenge:{id}` : activite sur un challenge

Events serveur :
- `fragment.earned` : fragments gagnes
- `badge.earned` : nouveau badge obtenu
- `leaderboard.updated` : classement mis a jour
- `challenge.submission` : soumission dans un challenge
- `notification` : notification temps reel (interet, message, moderation)

## Licence

Ce projet est distribue sous licence [GNU Affero General Public License v3.0](LICENSE) (AGPL-3.0).

## Contribuer

Voir [CONTRIBUTING.md](CONTRIBUTING.md) pour les modalites de contribution.
Voir [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) pour les regles de la communaute.

## Securite

Pour signaler une vulnerabilite, voir [SECURITY.md](SECURITY.md).
