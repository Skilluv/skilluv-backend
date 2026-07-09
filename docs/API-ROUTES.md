# Skilluv Backend — API Routes Reference

> **Base URL:** `http://localhost:3001/api`
> **Auth:** JWT dans cookie HttpOnly `access_token` (sauf routes publiques)
> **Format réponse:** `{ "data": {...}, "meta": { "request_id", "timestamp" }, "pagination"?: {...} }`

---

## Auth (17 routes)

### Public

| Method | Path | Body | Response |
|--------|------|------|----------|
| POST | `/auth/register` | `{ email, username, password, first_name, last_name, skill_domain, country? }` | `{ user: UserPrivate, refresh_token, message }` — 201, set cookie |
| POST | `/auth/login` | `{ identifier, password, totp_code?, email_2fa_code? }` | `{ user: UserPrivate, refresh_token }` ou `{ requires_email_2fa, user_id }` — 200, set cookie |
| POST | `/auth/email-2fa/verify` | `{ code, user_id? }` | `{ user, refresh_token }` — set cookie |
| POST | `/auth/refresh` | `{ refresh_token, user_id }` | `{ refresh_token }` — set cookie |
| GET | `/auth/verify-email?token=xxx` | — | `{ message }` |
| POST | `/auth/forgot-password` | `{ email }` | `{ message }` (toujours succès) |
| POST | `/auth/reset-password` | `{ token, new_password }` | `{ message }` |

### Authenticated

| Method | Path | Body | Response |
|--------|------|------|----------|
| GET | `/auth/me` | — | `{ user: UserPrivate, rank: { global, domain } }` |
| POST | `/auth/logout` | — | `{ message }` — clear cookie |
| POST | `/auth/change-password` | `{ current_password, new_password }` | `{ message }` |
| POST | `/auth/resend-verification` | — | `{ message }` |
| POST | `/auth/totp/setup` | — | `{ otpauth_url, secret_base32, message }` |
| POST | `/auth/totp/enable` | `{ code }` | `{ message }` |
| POST | `/auth/totp/disable` | `{ code }` | `{ message }` |
| POST | `/auth/email-2fa/enable` | — | `{ message }` |
| POST | `/auth/email-2fa/disable` | `{ current_password, new_password }` | `{ message }` |
| DELETE | `/auth/account` | `{ password, totp_code? }` | `{ message }` — RGPD suppression totale |

---

## Profil utilisateur (8 routes)

### Public (SSR-ready)

| Method | Path | Response |
|--------|------|----------|
| GET | `/profile/{username}` | `{ user: { username, display_name, title, golden_stars, skill_domain, country, bio, avatar_url, github, linkedin, website, twitter, member_since }, stats, skill_tree?, heatmap_summary?, badges? }` — respecte privacy settings |

### Authenticated

| Method | Path | Body | Response |
|--------|------|------|----------|
| PUT | `/profile/me` | `{ bio?, github?, linkedin?, website?, twitter?, country? }` | `{ user: UserPrivate }` |
| POST | `/profile/me/avatar` | multipart `avatar` (JPEG/PNG/WebP, max 2MB) | `{ avatar_url, message }` |
| DELETE | `/profile/me/avatar` | — | `{ message }` |
| GET | `/profile/me/privacy` | — | `{ privacy: { show_email, show_heatmap, show_skill_tree, show_badges, show_streak, allow_interest_requests } }` |
| PUT | `/profile/me/privacy` | `{ show_email?, show_heatmap?, show_skill_tree?, show_badges?, show_streak?, allow_interest_requests? }` | `{ privacy }` |
| PUT | `/auth/me/display-name` | `{ display_name }` | `{ display_name, message }` |
| PUT | `/auth/me/skill-domain` | `{ skill_domain }` | `{ skill_domain, message }` |

---

## Challenges (6 routes)

| Method | Path | Auth | Body/Query | Response |
|--------|------|------|------------|----------|
| GET | `/challenges/onboarding?domain=code` | Oui | query: `domain` | `{ challenge }` |
| GET | `/challenges?domain=&difficulty=&page=&per_page=` | Oui | query params | `{ data: [{ challenge, locked }], pagination }` |
| GET | `/challenges/{id}` | Oui | — | `{ challenge }` |
| POST | `/challenges/{id}/start` | Oui | — | `{ submission, challenge }` — 201 ou 200 (resume) |
| POST | `/challenges/{id}/submit` | Oui | `{ code, language? }` | `{ submission, fragments_earned, perseverance_bonus, user: { total_fragments, title, golden_stars, streak_current, profile_active }, profile_activated?, message? }` |
| GET | `/challenges/{id}/submissions` | Oui | — | `{ submissions: [] }` |

---

## Challenge Tags (3 routes — public SSR)

| Method | Path | Response |
|--------|------|----------|
| GET | `/challenges/tags` | `{ tags: [{ id, name, category, challenge_count }] }` |
| GET | `/challenges/categories` | `{ categories: [{ category, tag_count }] }` |
| GET | `/challenges/featured` | `{ challenges: [Challenge] }` (top 20 featured) |

---

## Challenge Teams & Timer (6 routes)

| Method | Path | Auth | Body | Response |
|--------|------|------|------|----------|
| POST | `/challenges/{id}/team/create` | Oui | `{ name, max_members? }` | `{ team }` — 201 |
| POST | `/challenges/{id}/team/{team_id}/join` | Oui | — | `{ message }` |
| GET | `/challenges/{id}/teams` | Oui | — | `{ teams: [{ team, members, member_count }] }` |
| POST | `/challenges/{id}/team/{team_id}/submit` | Oui | `{ code, language? }` | `{ submission, fragments_per_member, team_members, message }` |
| GET | `/challenges/{id}/timer` | Oui | — | `{ submission_id, started_at, expires_at?, remaining_seconds?, expired, has_timer }` |
| POST | `/challenges/{id}/timer/extend` | Admin | `{ minutes }` | `{ message, submissions_affected }` |

---

## Community Challenges (6 routes)

| Method | Path | Auth | Body | Response |
|--------|------|------|------|----------|
| POST | `/community/challenges` | Oui | `{ title, description, instructions, skill_domain, difficulty, language?, expected_output?, test_cases?, reward_fragments?, duration_minutes?, tags?, submit_for_review? }` | `{ challenge, message }` — 201 |
| GET | `/community/challenges/mine` | Oui | — | `{ challenges: [] }` |
| PUT | `/community/challenges/{id}` | Oui (créateur) | `{ title?, description?, instructions?, difficulty?, language?, expected_output?, test_cases?, submit_for_review? }` | `{ challenge }` |
| POST | `/community/challenges/{id}/vote` | Oui | — | `{ message }` — 201 |
| DELETE | `/community/challenges/{id}/vote` | Oui | — | `{ message }` |
| GET | `/community/challenges/popular?page=&per_page=` | Non (SSR) | — | `{ data: [Challenge], pagination }` |

---

## Gamification (4 routes)

| Method | Path | Auth | Response |
|--------|------|------|----------|
| GET | `/skills/tree` | Oui | `{ user: { id, display_name, title, golden_stars, total_fragments }, tree: [{ domain, total_fragments, skills }] }` |
| GET | `/skills/tree/{user_id}` | Oui | idem (profil doit être actif) |
| GET | `/activity/heatmap` | Oui | `{ heatmap: [{ activity_date, challenges_completed, fragments_earned }], summary: { days_active, total_challenges, period_start, period_end } }` |
| GET | `/activity/heatmap/{user_id}` | Oui | idem |

---

## Leaderboard (3 routes)

| Method | Path | Auth | Query | Response |
|--------|------|------|-------|----------|
| GET | `/leaderboards` | Non (SSR) | — | `{ leaderboards: [{ domain, periods }] }` |
| GET | `/leaderboards/{domain}` | Non (SSR) | `period?`, `page?`, `per_page?` | `{ data: { domain, period, entries: [{ rank, user_id, username, display_name, title, golden_stars, country, score }] }, pagination }` |
| GET | `/leaderboards/{domain}/me` | Oui | `period?` | `{ domain, period, rank, score, total_participants }` |

**Domains:** `global`, `code`, `design`, `game`, `security`
**Periods:** `alltime`, `weekly`, `monthly`

---

## Sandbox (4 routes)

| Method | Path | Auth | Body | Response |
|--------|------|------|------|----------|
| POST | `/sandbox/execute` | Oui (rate: 20/min) | `{ source_code, language, stdin?, expected_output? }` | `{ execution, verdict, success }` |
| POST | `/sandbox/execute-async` | Oui | idem | `{ token, message }` |
| GET | `/sandbox/result/{token}` | Oui | — | `{ execution, verdict, success, processing }` |
| GET | `/sandbox/languages` | Oui | — | `{ tier1, tier2, total }` |

---

## Enterprise (7 routes)

| Method | Path | Auth | Body | Response |
|--------|------|------|------|----------|
| POST | `/enterprise/register` | Non | `{ email, username, password, first_name, last_name, company_name, website?, industry?, company_size, country? }` | `{ user, enterprise, refresh_token, message }` — 201, set cookie |
| GET | `/enterprise/profile` | Enterprise | — | `{ enterprise, member_count }` |
| PUT | `/enterprise/profile` | Enterprise (owner) | `{ company_name?, description?, website?, logo_url?, industry?, company_size? }` | `{ enterprise }` |
| POST | `/enterprise/invite` | Enterprise (owner) | `{ email }` | `{ message, invite_token }` |
| POST | `/enterprise/invite/accept` | Non (token) | `{ token }` | `{ message }` |
| GET | `/enterprise/members` | Enterprise | — | `{ members: [{ id, user_id, username, display_name, email, role, status, invited_at, accepted_at? }] }` |
| DELETE | `/enterprise/members/{user_id}` | Enterprise (owner) | — | `{ message }` |

**company_size:** `1-10`, `11-50`, `51-200`, `201-500`, `501-1000`, `1000+`

---

## Talent Search (2 routes — public SSR)

| Method | Path | Auth | Query | Response |
|--------|------|------|-------|----------|
| GET | `/talents/search` | Optionnel | `q?`, `skill_domain?`, `title?`, `country?`, `min_fragments?`, `sort_by?`, `page?`, `per_page?` | `{ data: [{ id, username, display_name, skill_domain, title, golden_stars, total_fragments, streak_current, country, member_since, is_bookmarked? }], pagination }` |
| GET | `/talents/{username}/card` | Non | — | `{ username, display_name, skill_domain, title, golden_stars, total_fragments, streak_current, country, member_since, top_skills, badge_count }` |

**sort_by:** `fragments` (défaut), `recent`, `relevance` (si `q` fourni)

---

## Bookmarks & Listes (10 routes — Enterprise auth)

### Bookmarks

| Method | Path | Body | Response |
|--------|------|------|----------|
| POST | `/enterprise/bookmarks/{talent_id}` | — | `{ message }` — 201 |
| DELETE | `/enterprise/bookmarks/{talent_id}` | — | `{ message }` |
| GET | `/enterprise/bookmarks?page=&per_page=` | — | `{ data: [{ id, username, display_name, skill_domain, title, golden_stars, total_fragments, country, bookmarked_at }], pagination }` |

### Listes nommées

| Method | Path | Body | Response |
|--------|------|------|----------|
| POST | `/enterprise/lists` | `{ name, description? }` | `{ list }` — 201 |
| GET | `/enterprise/lists` | — | `{ lists: [{ id, name, description, talent_count, created_at }] }` |
| GET | `/enterprise/lists/{list_id}` | — | `{ list, talents: [{ id, username, display_name, skill_domain, title, golden_stars, total_fragments, country }] }` |
| PUT | `/enterprise/lists/{list_id}` | `{ name?, description? }` | `{ list }` |
| DELETE | `/enterprise/lists/{list_id}` | — | `{ message }` |
| POST | `/enterprise/lists/{list_id}/talents/{talent_id}` | — | `{ message }` — 201 |
| DELETE | `/enterprise/lists/{list_id}/talents/{talent_id}` | — | `{ message }` |

---

## Contact & Messagerie (10 routes)

### Interest Requests

| Method | Path | Auth | Body | Response |
|--------|------|------|------|----------|
| POST | `/contact/interest` | Enterprise (rate: 5/h) | `{ talent_id, message }` | `{ interest_request, message }` — 201 |
| GET | `/contact/interest/sent?page=&per_page=` | Enterprise | — | `{ data: [{ id, talent_id, talent_username, talent_display_name, status, initial_message, created_at }], pagination }` |
| GET | `/contact/interest/received?page=&per_page=` | Oui (talent) | — | `{ data: [{ id, enterprise_id, enterprise_name, enterprise_logo, status, initial_message, created_at }], pagination }` |
| POST | `/contact/interest/{id}/accept` | Oui (talent) | — | `{ conversation, message }` — crée conversation + copie message initial |
| POST | `/contact/interest/{id}/decline` | Oui (talent) | — | `{ message }` — cooldown 30 jours |

### Conversations

| Method | Path | Auth | Body | Response |
|--------|------|------|------|----------|
| GET | `/contact/conversations` | Oui | — | `{ conversations: [{ id, closed, other_party: { type, name/username }, last_message?, unread_count, created_at }] }` |
| GET | `/contact/conversations/{id}?page=&per_page=` | Oui | — | `{ data: { conversation, messages }, pagination }` — marque messages comme lus |
| POST | `/contact/conversations/{id}/messages` | Oui | `{ content }` (1-5000 chars) | `{ message: Message }` — 201, notifie destinataire |

### Blocage

| Method | Path | Auth | Response |
|--------|------|------|----------|
| POST | `/contact/block/{enterprise_id}` | Oui (talent) | `{ message }` — ferme les conversations ouvertes |
| DELETE | `/contact/block/{enterprise_id}` | Oui (talent) | `{ message }` |

---

## Notifications (4 routes)

| Method | Path | Auth | Query | Response |
|--------|------|------|-------|----------|
| GET | `/notifications?read=false&page=&per_page=` | Oui | `read?`, `page?`, `per_page?` | `{ data: [Notification], pagination }` |
| POST | `/notifications/{id}/read` | Oui | — | `{ message }` |
| POST | `/notifications/read-all` | Oui | — | `{ message }` |
| GET | `/notifications/unread-count` | Oui | — | `{ unread_count }` |

**Notification types:** `interest_request_received`, `interest_accepted`, `interest_declined`, `new_message`, `challenge_approved`, `challenge_rejected`, `account_banned`, `account_unbanned`

---

## Reports (3 routes)

| Method | Path | Auth | Body | Response |
|--------|------|------|------|----------|
| POST | `/reports` | Oui | `{ target_type, target_id, reason, details? }` | `{ report, message }` — 201 |
| GET | `/reports/mine` | Oui | — | `{ reports: [] }` |
| DELETE | `/reports/{id}` | Oui | — | `{ message }` (seulement si status=pending) |

**target_type:** `user`, `challenge`, `message`, `enterprise`
**reason:** `spam`, `harassment`, `inappropriate`, `cheating`, `fake_profile`, `other`

---

## Developer — API Keys (5 routes)

| Method | Path | Auth | Body | Response |
|--------|------|------|------|----------|
| POST | `/developer/keys` | Oui | `{ name, permissions? }` | `{ key: ApiKeyInfo, secret: "sk_live_xxx", message }` — 201 |
| GET | `/developer/keys` | Oui | — | `{ keys: [ApiKeyInfo] }` |
| DELETE | `/developer/keys/{id}` | Oui | — | `{ message }` |
| POST | `/developer/keys/{id}/regenerate` | Oui | — | `{ secret, message }` |
| GET | `/developer/keys/{id}/usage` | Oui | — | `{ key_id, name, request_count, last_used_at?, active }` |

**Permissions:** `read:profile`, `read:skills`, `read:badges`, `read:leaderboard`, `*`

---

## Developer — Webhooks (5 routes)

| Method | Path | Auth | Body | Response |
|--------|------|------|------|----------|
| POST | `/developer/webhooks` | Oui | `{ url, events: [] }` | `{ webhook, secret: "whsec_xxx", message }` — 201 |
| GET | `/developer/webhooks` | Oui | — | `{ webhooks: [WebhookInfo] }` |
| PUT | `/developer/webhooks/{id}` | Oui | `{ url?, events?, active? }` | `{ webhook }` |
| DELETE | `/developer/webhooks/{id}` | Oui | — | `{ message }` |
| POST | `/developer/webhooks/{id}/test` | Oui | — | `{ message }` |

**Events:** `challenge.completed`, `badge.earned`, `title.changed`, `leaderboard.updated`
**Signature:** Header `X-Skilluv-Signature: sha256={hmac}` — HMAC-SHA256 du body avec le secret

---

## API Publique v1 (3 routes — API Key auth)

Auth via header `Authorization: Bearer sk_live_xxx` ou query `?api_key=sk_live_xxx`

| Method | Path | Permission | Response |
|--------|------|------------|----------|
| GET | `/v1/users/{username}` | `read:profile` | `{ user: { id, username, display_name, skill_domain, title, golden_stars, total_fragments, streak_current, country, bio, avatar_url, github, linkedin, website, twitter, challenges_completed, member_since } }` |
| GET | `/v1/users/{username}/badges` | `read:badges` | `{ username, badges: [{ slug, name, description, icon, category, earned_at }], total }` |
| GET | `/v1/users/{username}/skills` | `read:skills` | `{ username, skill_tree: [{ domain, total_fragments, skills }] }` |

---

## Admin — Modération (8 routes)

| Method | Path | Body/Query | Response |
|--------|------|------------|----------|
| GET | `/admin/users?role=&banned=&q=&page=&per_page=` | query params | `{ data: [UserSummary], pagination }` |
| GET | `/admin/users/{id}` | — | `{ user, reports_against, total_submissions }` |
| POST | `/admin/users/{id}/ban` | `{ reason }` | `{ message, reason }` — ban complet + notifications |
| POST | `/admin/users/{id}/unban` | — | `{ message }` |
| GET | `/admin/reports?status=&target_type=&page=&per_page=` | query params | `{ data: [{ report + reporter info }], pagination }` |
| PUT | `/admin/reports/{id}` | `{ status, admin_note? }` | `{ report, message }` — status: `resolved` ou `dismissed` |
| GET | `/admin/audit-log?action=&page=&per_page=` | query params | `{ data: [AuditEntry], pagination }` |
| GET | `/admin/dashboard/moderation` | — | `{ banned_users, reports: { pending, resolved, dismissed, total }, recent_bans_30d, admin_actions_today }` |

---

## Admin — Challenges (7 routes)

| Method | Path | Body | Response |
|--------|------|------|----------|
| POST | `/admin/challenges` | `{ title, description, instructions, skill_domain, difficulty, mode?, duration_minutes?, ai_allowed?, tone?, language?, prerequisite_fragments?, reward_fragments?, is_onboarding?, expected_output?, test_cases? }` | `{ challenge }` — 201 |
| GET | `/admin/challenges` | — | `{ challenges, total }` |
| PUT | `/admin/challenges/{id}` | champs optionnels | `{ challenge }` |
| POST | `/admin/challenges/{id}/publish` | — | `{ challenge }` |
| POST | `/admin/challenges/{id}/archive` | — | `{ challenge }` |
| GET | `/admin/stats` | — | `{ users, challenges, submissions, websocket }` |
| POST | `/admin/leaderboards/rebuild` | — | `{ message }` |

---

## Admin — Community (3 routes)

| Method | Path | Body | Response |
|--------|------|------|----------|
| GET | `/admin/community/review` | — | `{ challenges: [{ challenge, creator }], total }` |
| POST | `/admin/community/{id}/approve` | — | `{ challenge, message }` — publie + notifie créateur |
| POST | `/admin/community/{id}/reject` | `{ feedback }` | `{ challenge, message }` — notifie créateur |

---

## Enterprise Dashboard (2 routes)

| Method | Path | Auth | Response |
|--------|------|------|----------|
| GET | `/enterprise/dashboard/platform-stats` | Enterprise | `{ total_talents, by_domain, by_title, avg_fragments, active_last_30d }` |
| GET | `/enterprise/dashboard/my-stats` | Enterprise | `{ bookmarks, talent_lists, interest_requests: { total, pending, accepted, declined }, active_conversations, team_size }` |

---

## Health & Docs (2 routes)

| Method | Path | Auth | Response |
|--------|------|------|----------|
| GET | `/health` | Non | `{ services: { postgresql, redis, judge0 }, status, version, websocket }` |
| GET | `/docs/openapi.json` | Non | Spec OpenAPI 3.1.0 complète |

---

## WebSocket

| Path | Auth | Description |
|------|------|-------------|
| `/ws` | Cookie JWT | Connexion WebSocket temps réel |

**Client → Serveur:**
- `{ action: "join", room: "leaderboard:code" }` — rejoindre une room
- `{ action: "leave", room: "..." }` — quitter une room
- `{ action: "ping" }` — keepalive

**Serveur → Client (events):**
- `connected` — connexion établie
- `fragment.earned` — fragments gagnés
- `badge.earned` — nouveau badge
- `leaderboard.updated` — classement mis à jour
- `challenge.submission` — soumission dans une room challenge
- `notification` — notification temps réel (intérêt, message, etc.)

**Rooms:** `user:{id}`, `leaderboard:{domain}`, `challenge:{id}`

---

## Modèles de données

### UserPrivate (retourné par /auth/me, /auth/login, etc.)
```json
{
  "id": "uuid",
  "email": "string",
  "username": "string",
  "first_name": "string",
  "last_name": "string",
  "display_name": "string",
  "skill_domain": "code|design|game|security",
  "title": "apprenti|artisan|maitre|legende",
  "golden_stars": 0,
  "total_fragments": 0,
  "streak_current": 0,
  "trust_score": 100.0,
  "country": "BJ|null",
  "bio": "string|null",
  "avatar_url": "string|null",
  "github": "string|null",
  "linkedin": "string|null",
  "website": "string|null",
  "twitter": "string|null",
  "email_verified": false,
  "totp_enabled": false,
  "email_2fa_enabled": false,
  "profile_active": false,
  "created_at": "ISO8601"
}
```

### Challenge
```json
{
  "id": "uuid",
  "title": "string",
  "description": "string",
  "instructions": "string",
  "skill_domain": "code|design|game|security",
  "difficulty": 1-5,
  "mode": "solo|team",
  "duration_minutes": "number|null",
  "ai_allowed": false,
  "tone": "serious|fun|educational",
  "language": "string|null",
  "prerequisite_fragments": 0,
  "reward_fragments": 10,
  "is_onboarding": false,
  "status": "draft|published|archived",
  "is_community": false,
  "community_status": "draft|review|approved|rejected|null",
  "featured": false,
  "vote_count": 0,
  "test_cases": "json|null",
  "expected_output": "string|null",
  "created_by": "uuid|null",
  "created_at": "ISO8601",
  "updated_at": "ISO8601"
}
```

### Notification
```json
{
  "id": "uuid",
  "user_id": "uuid",
  "notification_type": "string",
  "title": "string",
  "body": "string|null",
  "data": "json|null",
  "read": false,
  "created_at": "ISO8601"
}
```

### Message
```json
{
  "id": "uuid",
  "conversation_id": "uuid",
  "sender_id": "uuid",
  "content": "string",
  "read_at": "ISO8601|null",
  "created_at": "ISO8601"
}
```

---

## Codes d'erreur

| Code | HTTP | Description |
|------|------|-------------|
| `RESOURCE_NOT_FOUND` | 404 | Ressource introuvable |
| `AUTH_INVALID_CREDENTIALS` | 401 | Identifiants incorrects |
| `AUTH_UNAUTHORIZED` | 401 | Non authentifié |
| `AUTH_FORBIDDEN` | 403 | Accès interdit |
| `VALIDATION_ERROR` | 400 | Erreur de validation |
| `AUTH_TOTP_REQUIRED` | 403 | Code TOTP requis |
| `AUTH_TOTP_INVALID` | 401 | Code TOTP invalide |
| `AUTH_EMAIL_2FA_INVALID` | 401 | Code email 2FA invalide |
| `CHALLENGE_PREREQUISITE_NOT_MET` | 403 | Prérequis non atteints |
| `RATE_LIMITED` | 429 | Trop de requêtes |
| `CONTACT_COOLDOWN_ACTIVE` | 429 | Cooldown après refus (30j) |
| `CONTACT_ALREADY_REQUESTED` | 409 | Demande déjà en cours |
| `CONTACT_BLOCKED` | 403 | Bloqué par l'utilisateur |
| `CONVERSATION_CLOSED` | 403 | Conversation fermée |

Format erreur :
```json
{
  "error": { "code": "ERROR_CODE", "message": "Description" },
  "meta": { "request_id": "uuid", "timestamp": "ISO8601" }
}
```

---

## Rate Limiting

| Endpoint | Limite | Fenêtre |
|----------|--------|---------|
| `/auth/register`, `/auth/login` | 10 req | par minute, par IP |
| `/sandbox/execute` | 20 req | par minute, par user |
| `/contact/interest` | 5 req | par heure, par enterprise |

---

## Security Headers

Toutes les réponses incluent :
- `X-Content-Type-Options: nosniff`
- `X-Frame-Options: DENY`
- `X-XSS-Protection: 1; mode=block`
- `Strict-Transport-Security: max-age=31536000; includeSubDomains`
- `Content-Security-Policy: default-src 'self'`
