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

### Security domain

Written in English, like the rest of the security catalogue, because the people
these limits are aimed at are the ones reading `SECURITY.md`.

| Route | Bucket key | Granularity | Limit | Window | Rationale |
|---|---|---|---|---|---|
| `POST /security/reports` | `security_report` | user_id | **5** | 1h | Enough for somebody having a very good afternoon. Report volume is not a virtue here — the fragments are scaled by severity precisely so that filing thirty low-value reports pays less than filing one real one. |
| `POST /security/reports/uploads` | `security_proof_upload` | user_id | **20** | 1h | Proof files land in the private bucket at up to 20 MB each. Twenty an hour is four or five reports' worth of screenshots; beyond that it is storage, not evidence. |
| `POST /security/challenges/{id}/flag` | *(table, not Redis)* | user_id × challenge | **10** | 1h | `FLAG_ATTEMPTS_PER_HOUR` in `services/security_practice.rs`. Counted from `security_flag_attempts` rather than Redis because every attempt, right or wrong, is an audit row that survives a Redis flush — a flag is a secret and brute force against it has to be visible afterwards. |
| lab answers | *(table)* | user_id × challenge | `max_attempts` | then `LAB_COOLDOWN_HOURS` = 24 | A defensive lab has a fixed number of tries set per challenge, then closes for a day. Guessing a multiple-choice analysis is not analysis. |

**Research mode.** A declared research token multiplies every one of the above,
and every other bucket in the table, by `RATE_LIMIT_MULTIPLIER` = **10**
(`services/security_research.rs`). It multiplies rather than removes, which is
what keeps denial of service out of scope in fact and not only in the policy.

The multiplier reaches `RateLimiter::check` through a `tokio::task_local!` set by
`middleware/security_research.rs` — the limiter is a plain function called from
about a hundred handlers, and threading a parameter through all of them to
express one exception would have been the wrong hundred edits.

A token whose traffic exceeds `ABNORMAL_REQUESTS_PER_MINUTE` = **500** in any
minute is **revoked automatically**, the person is notified with the number, and
the revocation is recorded with its reason. That ceiling is deliberately far
above enthusiastic manual testing and far below a load test.

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
| 2026-08-24 | Skilluv Cyber | Security domain buckets (reports, proof uploads, flag attempts), research-token ×10 multiplier and the 500 req/min auto-revocation. |
