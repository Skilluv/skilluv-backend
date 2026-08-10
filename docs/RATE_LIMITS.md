# Rate limits — reference

Audit exhaustif des rate limits appliqués côté backend (SKI-30, 2026-08-10).

## Implementation

- **Backend** : Redis sliding window via `src/middleware/rate_limit.rs::RateLimiter::check`.
- **Global escape hatches** :
  - `SKILLUV_DISABLE_RATELIMIT=1` — désactive TOUS les limiters (dev / tests locaux uniquement).
  - `SKILLUV_RATELIMIT_IP_WHITELIST=ip1,ip2,...` — bypass pour certaines IPs (CI runners, staging health checks).
- **Extraction IP** : `X-Forwarded-For` puis `X-Real-IP` puis IP directe. En prod, le reverse proxy Coolify DOIT set l'un des deux — sans ça, `ip = ""` et aucune limite ne s'applique aux requêtes non-authentifiées (documenté ligne 31-32 du middleware).

## Buckets actifs

| Route | Bucket key | Granularité | Limit | Window | Rationale |
|---|---|---|---|---|---|
| `POST /auth/register` | `auth:register` | IP | **20** | 1h | Bumped 5→20 par SKI-30 : le seuil 5/h bloquait les signup légitimes avec typos email/password. 20/h reste anti-abuse (un attaquant qui rate 20 comptes/h en 1h → 1 tentative toutes les 3min, très lent, largement détectable). |
| `POST /auth/login` | `auth:login` | IP | 20 | 60s | Tolère quelques typos (autocomplete confus, password manager mismatch) sans laisser une brute-force libre. |
| `POST /magic-link/*` | `magic_link` | IP | 5 | 60s | Le magic link envoie un email — chaque call = coût. 5/60s est le max avant que ce soit du spam. |
| `POST /enterprise/register` | `enterprise:register` | IP | (voir file) | — | Anti-farm de comptes enterprise pour bypass paywall. |
| `POST /invite/register` | `invite:register` | IP | 10 | 1h | Sign up via invitation — plus permissif que register direct (le lien invitation est déjà un facteur d'authenticité). |
| `admin destructive burst` | `admin_destructive_burst` | user_id | 10 | 60s | Un admin qui delete/revoke 10 rows en 60s = probablement OK, au-delà c'est suspect (mass action à confirmer). |
| `admin destructive hourly` | `admin_destructive_hourly` | user_id | 100 | 1h | Même admin, même 100 actions destructives par heure c'est déjà beaucoup — soit refactor ops (script en batch DB direct) soit erreur humaine. |
| `POST /ai-coach/performance/refresh` | `ai_performance_refresh` | user_id | (voir file) | — | Coût LLM par appel — limité pour éviter facture explosive. |
| `POST /ai-coach/career/refresh` | `ai_career_refresh` | user_id | (voir file) | — | Idem. |
| `POST /sandbox/*` | `sandbox` | user_id | (voir file) | — | Coût Judge0 par exécution. |
| `PATCH /user/profile` | `profile_update` | user_id | (voir file) | — | Anti-troll (empêche un user de changer son username 100 fois par heure pour saturer les mentions). |
| `POST /contact` | `contact` | enterprise_id | (voir file) | — | Anti-spam. |
| `POST /forum/questions` | `forum_question` | user_id | tier-based | — | Voir `services::forum::question_rate_limit_for_title` — apprenti = 3/24h, artisan = 10/24h, maitre/legende = illimité. Encourage les seniors à contribuer sans les brider. |

## Comment ajuster un seuil

1. Localiser l'appel `RateLimiter::check(...)` dans la route concernée.
2. Modifier les 4ᵉ + 5ᵉ arguments : `check(..., "key", &ip, LIMIT, WINDOW_SEC)`.
3. Ajouter un commentaire `SKI-XX (date): bumped X → Y` avec la raison.
4. Update cette doc.

## Vérification manuelle post-changement

**Scénario "user honnête"** — doit passer sans blocage :
- 3 tentatives login avec mot de passe faux, puis 4ᵉ tentative avec le bon → OK
- 1 register + resend verification email 2× → OK
- 1 forgot-password + 1 magic-link redemandés dans la même minute → OK

**Scénario "attaque"** — doit se faire couper :
- 30 tentatives login en 60s → couper au 21ᵉ (login = 20/60s)
- 25 register en 1h avec IP fixe → couper au 21ᵉ (register = 20/1h)
- 100 forgot-password en 60s → couper au 6ᵉ (magic_link = 5/60s)

## Historique des changements

| Date | Ticket | Change |
|---|---|---|
| 2026-08-10 | SKI-30 | Bump `auth:register` 5/h → 20/h (typos légitimes bloquées) + création de cette doc. |
