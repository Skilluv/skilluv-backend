# Skilluv Backend — API Routes Reference

> **Base URL:** `http://localhost:3001/api`
> **Auth:** JWT in the `access_token` HttpOnly cookie (public routes excepted)
> **Response shape:** `{ "data": {...}, "meta": { "request_id", "timestamp" }, "pagination"?: {...} }`

> **Scope:** a hand-written tour of the surfaces a front end reaches first —
> around 150 of the roughly 970 routes the service exposes. It is not the
> complete list and does not try to be. The generated OpenAPI document at
> `/api/docs` is: every route is registered there, and a test fails the build
> when a GET route is added without it.

---

## Auth (17 routes)

### Public

| Method | Path | Body | Response |
|--------|------|------|----------|
| POST | `/auth/register` | `{ email, username, password, first_name, last_name, skill_domain, country? }` | `{ user: UserPrivate, refresh_token, message }` — 201, set cookie |
| POST | `/auth/login` | `{ identifier, password, totp_code?, email_2fa_code? }` | `{ user: UserPrivate, refresh_token }` or `{ requires_email_2fa, user_id }` — 200, set cookie |
| POST | `/auth/email-2fa/verify` | `{ code, user_id? }` | `{ user, refresh_token }` — set cookie |
| POST | `/auth/refresh` | `{ refresh_token, user_id }` | `{ refresh_token }` — set cookie |
| GET | `/auth/verify-email?token=xxx` | — | `{ message }` |
| POST | `/auth/forgot-password` | `{ email }` | `{ message }` (always succeeds) |
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
| DELETE | `/auth/account` | `{ password, totp_code? }` | `{ message }` — full GDPR erasure |

---

## User profile (8 routes)

### Public (SSR-ready)

| Method | Path | Response |
|--------|------|----------|
| GET | `/profile/{username}` | `{ user: { username, display_name, title, golden_stars, skill_domain, country, bio, avatar_url, github, linkedin, website, twitter, member_since }, stats, skill_tree?, heatmap_summary?, badges? }` — honours the privacy settings. 404 when the account is banned or `profile_hidden = true`. Visible from sign-up onwards: `profile_active` gates only the listing surfaces (talent search, leaderboard, digest). |

### Authenticated

| Method | Path | Body | Response |
|--------|------|------|----------|
| PUT | `/profile/me` | `{ bio?, github?, linkedin?, website?, twitter?, country? }` | `{ user: UserPrivate }` |
| POST | `/profile/me/avatar` | multipart `avatar` (JPEG/PNG/WebP, max 2MB) | `{ avatar_url, message }` |
| DELETE | `/profile/me/avatar` | — | `{ message }` |
| GET | `/profile/me/privacy` | — | `{ privacy: { show_email, show_heatmap, show_skill_tree, show_badges, show_streak, allow_interest_requests, hide_profile } }` |
| PUT | `/profile/me/privacy` | `{ show_email?, show_heatmap?, show_skill_tree?, show_badges?, show_streak?, allow_interest_requests?, hide_profile? }` | `{ privacy }` |
| PUT | `/auth/me/display-name` | `{ display_name }` | `{ display_name, message }` |
| PUT | `/auth/me/skill-domain` | `{ skill_domain }` | `{ skill_domain, message }` |

---

## Challenges (6 routes)

| Method | Path | Auth | Body/Query | Response |
|--------|------|------|------------|----------|
| GET | `/challenges/onboarding?domain=code` | Yes | query: `domain` | `{ challenge }` |
| GET | `/challenges?domain=&difficulty=&page=&per_page=` | Yes | query params | `{ data: [{ challenge, locked }], pagination }` |
| GET | `/challenges/{id}` | Yes | — | `{ challenge }` |
| POST | `/challenges/{id}/start` | Yes | — | `{ submission, challenge }` — 201, or 200 on resume |
| POST | `/challenges/{id}/submit` | Yes | `{ code, language? }` | `{ submission, fragments_earned, perseverance_bonus, user: { total_fragments, title, golden_stars, streak_current, profile_active }, profile_activated?, message? }` |
| GET | `/challenges/{id}/submissions` | Yes | — | `{ submissions: [] }` |

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
| POST | `/challenges/{id}/team/create` | Yes | `{ name, max_members? }` | `{ team }` — 201 |
| POST | `/challenges/{id}/team/{team_id}/join` | Yes | — | `{ message }` |
| GET | `/challenges/{id}/teams` | Yes | — | `{ teams: [{ team, members, member_count }] }` |
| POST | `/challenges/{id}/team/{team_id}/submit` | Yes | `{ code, language? }` | `{ submission, fragments_per_member, team_members, message }` |
| GET | `/challenges/{id}/timer` | Yes | — | `{ submission_id, started_at, expires_at?, remaining_seconds?, expired, has_timer }` |
| POST | `/challenges/{id}/timer/extend` | Admin | `{ minutes }` | `{ message, submissions_affected }` |

---

## Community Challenges (6 routes)

| Method | Path | Auth | Body | Response |
|--------|------|------|------|----------|
| POST | `/community/challenges` | Yes | `{ title, description, instructions, skill_domain, difficulty, language?, expected_output?, test_cases?, reward_fragments?, duration_minutes?, tags?, submit_for_review? }` | `{ challenge, message }` — 201 |
| GET | `/community/challenges/mine` | Yes | — | `{ challenges: [] }` |
| PUT | `/community/challenges/{id}` | Yes (author) | `{ title?, description?, instructions?, difficulty?, language?, expected_output?, test_cases?, submit_for_review? }` | `{ challenge }` |
| POST | `/community/challenges/{id}/vote` | Yes | — | `{ message }` — 201 |
| DELETE | `/community/challenges/{id}/vote` | Yes | — | `{ message }` |
| GET | `/community/challenges/popular?page=&per_page=` | No (SSR) | — | `{ data: [Challenge], pagination }` |

---

## Gamification (4 routes)

| Method | Path | Auth | Response |
|--------|------|------|----------|
| GET | `/skills/tree` | Yes | `{ user: { id, display_name, title, golden_stars, total_fragments }, tree: [{ domain, total_fragments, skills }] }` |
| GET | `/skills/tree/{user_id}` | Yes | same (the profile has to be active) |
| GET | `/activity/heatmap` | Yes | `{ heatmap: [{ activity_date, challenges_completed, fragments_earned }], summary: { days_active, total_challenges, period_start, period_end } }` |
| GET | `/activity/heatmap/{user_id}` | Yes | same |

---

## Leaderboard (3 routes)

| Method | Path | Auth | Query | Response |
|--------|------|------|-------|----------|
| GET | `/leaderboards` | No (SSR) | — | `{ leaderboards: [{ domain, periods }] }` |
| GET | `/leaderboards/{domain}` | No (SSR) | `period?`, `page?`, `per_page?` | `{ data: { domain, period, entries: [{ rank, user_id, username, display_name, title, golden_stars, country, score }] }, pagination }` |
| GET | `/leaderboards/{domain}/me` | Yes | `period?` | `{ domain, period, rank, score, total_participants }` |

**Domains:** `global`, `code`, `design`, `game`, `security`
**Periods:** `alltime`, `weekly`, `monthly`

---

## Sandbox (4 routes)

| Method | Path | Auth | Body | Response |
|--------|------|------|------|----------|
| POST | `/sandbox/execute` | Yes (rate: 20/min) | `{ source_code, language, stdin?, expected_output? }` | `{ execution, verdict, success }` |
| POST | `/sandbox/execute-async` | Yes | same | `{ token, message }` |
| GET | `/sandbox/result/{token}` | Yes | — | `{ execution, verdict, success, processing }` |
| GET | `/sandbox/languages` | Yes | — | `{ tier1, tier2, total }` |

---

## Enterprise (7 routes)

| Method | Path | Auth | Body | Response |
|--------|------|------|------|----------|
| POST | `/enterprise/register` | No | `{ email, username, password, first_name, last_name, company_name, website?, industry?, company_size, country? }` | `{ user, enterprise, refresh_token, message }` — 201, set cookie |
| GET | `/enterprise/profile` | Company | — | `{ enterprise, member_count }` |
| PUT | `/enterprise/profile` | Company (owner) | `{ company_name?, description?, website?, logo_url?, industry?, company_size? }` | `{ enterprise }` |
| POST | `/enterprise/invite` | Company (owner) | `{ email }` | `{ message, invite_token }` |
| POST | `/enterprise/invite/accept` | No (token) | `{ token }` | `{ message }` |
| GET | `/enterprise/members` | Company | — | `{ members: [{ id, user_id, username, display_name, email, role, status, invited_at, accepted_at? }] }` |
| DELETE | `/enterprise/members/{user_id}` | Company (owner) | — | `{ message }` |

**company_size:** `1-10`, `11-50`, `51-200`, `201-500`, `501-1000`, `1000+`

---

## Talent Search (2 routes — public SSR)

| Method | Path | Auth | Query | Response |
|--------|------|------|-------|----------|
| GET | `/talents/search` | Optional | `q?`, `skill_domain?`, `title?`, `country?`, `min_fragments?`, `sort_by?`, `page?`, `per_page?` | `{ data: [{ id, username, display_name, skill_domain, title, golden_stars, total_fragments, streak_current, country, member_since, is_bookmarked? }], pagination }` |
| GET | `/talents/{username}/card` | No | — | `{ username, display_name, skill_domain, title, golden_stars, total_fragments, streak_current, country, member_since, top_skills, badge_count }` |

**sort_by:** `fragments` (default), `recent`, `relevance` (when `q` is given)

---

## Bookmarks and lists (10 routes — company auth)

### Bookmarks

| Method | Path | Body | Response |
|--------|------|------|----------|
| POST | `/enterprise/bookmarks/{talent_id}` | — | `{ message }` — 201 |
| DELETE | `/enterprise/bookmarks/{talent_id}` | — | `{ message }` |
| GET | `/enterprise/bookmarks?page=&per_page=` | — | `{ data: [{ id, username, display_name, skill_domain, title, golden_stars, total_fragments, country, bookmarked_at }], pagination }` |

### Named lists

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

## Contact and messaging (10 routes)

### Interest Requests

| Method | Path | Auth | Body | Response |
|--------|------|------|------|----------|
| POST | `/contact/interest` | Company (rate: 5/h) | `{ talent_id, message }` | `{ interest_request, message }` — 201 |
| GET | `/contact/interest/sent?page=&per_page=` | Company | — | `{ data: [{ id, talent_id, talent_username, talent_display_name, status, initial_message, created_at }], pagination }` |
| GET | `/contact/interest/received?page=&per_page=` | Yes (talent) | — | `{ data: [{ id, enterprise_id, enterprise_name, enterprise_logo, status, initial_message, created_at }], pagination }` |
| POST | `/contact/interest/{id}/accept` | Yes (talent) | — | `{ conversation, message }` — opens the conversation and copies the first message into it |
| POST | `/contact/interest/{id}/decline` | Yes (talent) | — | `{ message }` — 30-day cooling-off period |

### Conversations

| Method | Path | Auth | Body | Response |
|--------|------|------|------|----------|
| GET | `/contact/conversations` | Yes | — | `{ conversations: [{ id, closed, other_party: { type, name/username }, last_message?, unread_count, created_at }] }` |
| GET | `/contact/conversations/{id}?page=&per_page=` | Yes | — | `{ data: { conversation, messages }, pagination }` — marks the messages read |
| POST | `/contact/conversations/{id}/messages` | Yes | `{ content }` (1-5000 chars) | `{ message: Message }` — 201, notifies the recipient |

### Blocking

| Method | Path | Auth | Response |
|--------|------|------|----------|
| POST | `/contact/block/{enterprise_id}` | Yes (talent) | `{ message }` — closes any conversation still open |
| DELETE | `/contact/block/{enterprise_id}` | Yes (talent) | `{ message }` |

---

## Notifications (4 routes)

| Method | Path | Auth | Query | Response |
|--------|------|------|-------|----------|
| GET | `/notifications?read=false&page=&per_page=` | Yes | `read?`, `page?`, `per_page?` | `{ data: [Notification], pagination }` |
| POST | `/notifications/{id}/read` | Yes | — | `{ message }` |
| POST | `/notifications/read-all` | Yes | — | `{ message }` |
| GET | `/notifications/unread-count` | Yes | — | `{ unread_count }` |

**Notification types:** `interest_request_received`, `interest_accepted`, `interest_declined`, `new_message`, `challenge_approved`, `challenge_rejected`, `account_banned`, `account_unbanned`

---

## Reports (3 routes)

| Method | Path | Auth | Body | Response |
|--------|------|------|------|----------|
| POST | `/reports` | Yes | `{ target_type, target_id, reason, details? }` | `{ report, message }` — 201 |
| GET | `/reports/mine` | Yes | — | `{ reports: [] }` |
| DELETE | `/reports/{id}` | Yes | — | `{ message }` (only while status=pending) |

**target_type:** `user`, `challenge`, `message`, `enterprise`
**reason:** `spam`, `harassment`, `inappropriate`, `cheating`, `fake_profile`, `other`

---

## Developer — API Keys (5 routes)

| Method | Path | Auth | Body | Response |
|--------|------|------|------|----------|
| POST | `/developer/keys` | Yes | `{ name, permissions? }` | `{ key: ApiKeyInfo, secret: "sk_live_xxx", message }` — 201 |
| GET | `/developer/keys` | Yes | — | `{ keys: [ApiKeyInfo] }` |
| DELETE | `/developer/keys/{id}` | Yes | — | `{ message }` |
| POST | `/developer/keys/{id}/regenerate` | Yes | — | `{ secret, message }` |
| GET | `/developer/keys/{id}/usage` | Yes | — | `{ key_id, name, request_count, last_used_at?, active }` |

**Permissions:** `read:profile`, `read:skills`, `read:badges`, `read:leaderboard`, `*`

---

## Developer — Webhooks (5 routes)

| Method | Path | Auth | Body | Response |
|--------|------|------|------|----------|
| POST | `/developer/webhooks` | Yes | `{ url, events: [] }` | `{ webhook, secret: "whsec_xxx", message }` — 201 |
| GET | `/developer/webhooks` | Yes | — | `{ webhooks: [WebhookInfo] }` |
| PUT | `/developer/webhooks/{id}` | Yes | `{ url?, events?, active? }` | `{ webhook }` |
| DELETE | `/developer/webhooks/{id}` | Yes | — | `{ message }` |
| POST | `/developer/webhooks/{id}/test` | Yes | — | `{ message }` |

**Events:** `challenge.completed`, `badge.earned`, `title.changed`, `leaderboard.updated`
**Signature:** Header `X-Skilluv-Signature: sha256={hmac}` — HMAC-SHA256 of the body, keyed with the secret

---

## Public API v1 (3 routes — API key auth)

Authenticated with the `Authorization: Bearer sk_live_xxx` header, or with the `?api_key=sk_live_xxx` query parameter

| Method | Path | Permission | Response |
|--------|------|------------|----------|
| GET | `/v1/users/{username}` | `read:profile` | `{ user: { id, username, display_name, skill_domain, title, golden_stars, total_fragments, streak_current, country, bio, avatar_url, github, linkedin, website, twitter, challenges_completed, member_since } }` |
| GET | `/v1/users/{username}/badges` | `read:badges` | `{ username, badges: [{ slug, name, description, icon, category, earned_at }], total }` |
| GET | `/v1/users/{username}/skills` | `read:skills` | `{ username, skill_tree: [{ domain, total_fragments, skills }] }` |

---

## Admin — moderation (8 routes)

| Method | Path | Body/Query | Response |
|--------|------|------------|----------|
| GET | `/admin/users?role=&banned=&q=&page=&per_page=` | query params | `{ data: [UserSummary], pagination }` |
| GET | `/admin/users/{id}` | — | `{ user, reports_against, total_submissions }` |
| POST | `/admin/users/{id}/ban` | `{ reason }` | `{ message, reason }` — a full ban, with the notifications that follow |
| POST | `/admin/users/{id}/unban` | — | `{ message }` |
| GET | `/admin/reports?status=&target_type=&page=&per_page=` | query params | `{ data: [{ report + reporter info }], pagination }` |
| PUT | `/admin/reports/{id}` | `{ status, admin_note? }` | `{ report, message }` — status: `resolved` or `dismissed` |
| GET | `/admin/audit-log?action=&page=&per_page=` | query params | `{ data: [AuditEntry], pagination }` |
| GET | `/admin/dashboard/moderation` | — | `{ banned_users, reports: { pending, resolved, dismissed, total }, recent_bans_30d, admin_actions_today }` |

---

## Admin — Challenges (7 routes)

| Method | Path | Body | Response |
|--------|------|------|----------|
| POST | `/admin/challenges` | `{ title, description, instructions, skill_domain, difficulty, mode?, duration_minutes?, ai_allowed?, tone?, language?, prerequisite_fragments?, reward_fragments?, is_onboarding?, expected_output?, test_cases? }` | `{ challenge }` — 201 |
| GET | `/admin/challenges` | — | `{ challenges, total }` |
| PUT | `/admin/challenges/{id}` | any subset of the fields above | `{ challenge }` |
| POST | `/admin/challenges/{id}/publish` | — | `{ challenge }` |
| POST | `/admin/challenges/{id}/archive` | — | `{ challenge }` |
| GET | `/admin/stats` | — | `{ users, challenges, submissions, websocket }` |
| POST | `/admin/leaderboards/rebuild` | — | `{ message }` |

---

## Community moderation (3 routes)

Served to `community_curator` or `admin` (see `docs/MODERATION-vs-ADMIN.md`).
The prefix is `/api/community/...`, not `/api/admin/community/...` (fix BE-P0-05).

| Method | Path | Body | Response |
|--------|------|------|----------|
| GET | `/community/challenges/review` | — | `{ challenges: [{ challenge, creator }], total }` |
| POST | `/community/challenges/{id}/approve` | — | `{ challenge, message }` — publishes it, and tells the author |
| POST | `/community/challenges/{id}/reject` | `{ reason }` (at least 8 characters; `feedback` is accepted as a legacy alias) | `{ id, title, rejected: true }` — tells the author |

---

## Talent wallet — payouts (8 routes)

Payouts through Stripe Connect (EUR) or African mobile money (XOF).

One withdrawal endpoint serves every rail. There used to be one per rail,
each naming its own — stripe and momo — in the URL, and this page still
listed both long after they were merged away. Which rail reaches a recipient
is decided server-side from the currency and what they have on file.

| Method | Path | Body | Response |
|--------|------|------|----------|
| GET | `/users/me/wallet` | — | `{ wallet }` — balances, held and available |
| GET | `/users/me/wallet/transactions` | — | `{ transactions }` |
| POST | `/users/me/wallet/residency` | `{ country }` | `{ wallet }` — decides which rails are open |
| POST | `/users/me/wallet/stripe/onboard` | `{ refresh_url?, return_url? }` | `{ onboarding_url }` |
| POST | `/users/me/wallet/momo/phone` | `{ phone: "+22507...", verified?: bool, provider?: "orange"\|"mtn"\|"wave" }` (fix BE-P0-12 — `verified` defaults to true since P13.3, and becomes OTP-gated in P15) | `{ registered: true }` |
| POST | `/users/me/wallet/withdraw` | `{ amount: "12.50", currency?: "EUR"\|"XOF", rail?: "stripe"\|"momo" }` — the amount is in currency units, never minor units (fix BE-P0-11) | `{ transaction }` |
| GET | `/users/me/wallet/statement.csv` | — | A compliance CSV |
| POST | `/webhooks/stripe-connect` | Stripe payload | Stripe Connect account and payout events |

## DM messaging (3 routes)

| Method | Path | Body | Response |
|--------|------|------|----------|
| GET | `/dm/conversations` | — | `{ conversations }` |
| GET | `/dm/conversations/{id}/messages` | — | `{ messages }` |
| POST | `/dm/conversations/{id}/messages` | `{ body }` (`text` is accepted as an alias — fix BE-P0-09) | `{ message }` |

## Fraud / plagiarism review (3 routes)

Served to `plagiarism_reviewer` or `admin`. The target is one **deliverable**,
never a whole account (fix BE-P0-06).

| Method | Path | Body | Response |
|--------|------|------|----------|
| GET | `/fraud/deliverables/flagged` | — | `{ data: [{ id, user_id, ... }], pagination }` |
| POST | `/fraud/deliverables/{id}/mark-valid` | `{ reason? }` | `{ marked_valid: true, id }` — a false positive |
| POST | `/fraud/deliverables/{id}/revoke` | `{ reason }` (min 8 chars) | `{ revoked: true, id }` — plagiarism confirmed |

## Forum posts (5 core routes)

| Method | Path | Body | Response |
|--------|------|------|----------|
| GET | `/forum/posts` | — | `{ posts: [...], pagination }` |
| POST | `/forum/posts` | `{ category_slug, kind, title, body, bounty_fragments? }` (fix BE-P0-07) | `{ post }` |
| GET | `/forum/posts/{id}` | — | `{ post }` |
| PUT | `/forum/posts/{id}` | `{ title, body }` | `{ post }` |
| POST | `/forum/posts/{id}/accept-answer` | `{ answer_id }` (`comment_id` and `answer_comment_id` are accepted as aliases — fix BE-P0-08) | `{ accepted, bounty_transferred }` |

## Forum moderation (2 routes)

Served to `forum_moderator` or `admin`.

| Method | Path | Body | Response |
|--------|------|------|----------|
| POST | `/forum/posts/{id}/moderate` | `{ action: 'hide'\|'delete', reason }` | `{ moderated: true }` |
| POST | `/forum/users/{id}/mute` | `{ duration_hours, reason }` | `{ muted_until }` |

---

## Enterprise Dashboard (2 routes)

| Method | Path | Auth | Response |
|--------|------|------|----------|
| GET | `/enterprise/dashboard/platform-stats` | Company | `{ total_talents, by_domain, by_title, avg_fragments, active_last_30d }` |
| GET | `/enterprise/dashboard/my-stats` | Company | `{ bookmarks, talent_lists, interest_requests: { total, pending, accepted, declined }, active_conversations, team_size }` |

---

## Quality (12 routes)

> The `quality` domain: defect reports, imported test runs, cross-domain
> routing. Migrations 0450-0459.

### Public

| Method | Path | Body | Response |
|--------|------|------|----------|
| GET | `/quality/reference` | — | `{ orientations, reviewer_groups, report_subtypes, severities, reproducibilities, test_run_sources }` |
| GET | `/quality/reports?target_domain=&limit=` | — | `{ reports: [...] }` — verified artefacts only; an undeclared `target_domain` is 400 |
| GET | `/users/{username}/quality-profile` | — | `{ profile }` — score, confirmed defects, breakdown by target domain |
| GET | `/quality/slices/{slice_id}/test-runs` | — | `{ runs: [...] }` |

### Authenticated

| Method | Path | Body | Response |
|--------|------|------|----------|
| POST | `/quality/bugs` | `BugReportInput` | `{ report }` — a reproduction under 40 characters is 400 |
| GET | `/quality/bugs` | — | `{ reports: [...] }` |
| POST | `/quality/bugs/{id}/fix` | `{ fix_url }` | `{ report }` |
| POST | `/quality/bugs/{id}/confirm` | — | `{ report }` — the reporter, and nobody else |
| POST | `/quality/bugs/{id}/review` | `ReviewDecision` | `{ report }` — needs `quality_reviewer:{family}` for the trade behind the slice |
| GET | `/quality/bugs/review-queue` | — | `{ reports: [...] }` — any `quality_reviewer:*` capability |
| POST | `/quality/test-runs` | `TestRunInput` | `{ run }` — re-importing updates the row and drops its verification |
| POST | `/quality/test-runs/{id}/verify` | — | `{ run }` — never your own import |

---

## Leadership (18 routes)

> The `leadership` domain: redaction, retrospectives, coordination,
> cohorts. Migrations 0460-0470.

### Public

| Method | Path | Body | Response |
|--------|------|------|----------|
| GET | `/leadership/reference` | — | `{ orientations, reviewer_groups, artifact_subtypes, redaction_states, retrospective_formats, link_kinds, cohort_leave_reasons }` |
| GET | `/users/{username}/leadership-profile` | — | `{ profile }` — confidential artefacts count towards the score and appear only in the abstract |
| GET | `/leadership/slices/{id}/links` | — | `{ reach }` |
| GET | `/leadership/retrospectives/{id}/actions` | — | `{ actions, followthrough }` |
| GET | `/leadership/cohorts/{id}/outcomes` | — | `{ outcomes }` — the denominator travels with the rate |

### Authenticated

| Method | Path | Body | Response |
|--------|------|------|----------|
| POST | `/leadership/slices/{id}/redaction/declare` | — | `{ declared }` — the author, and nobody else |
| POST | `/leadership/slices/{id}/redaction/confirm` | — | `{ confirmed }` — never the author; any `leadership_reviewer:*` capability |
| POST | `/leadership/slices/{id}/adoption` | `{ evidence_url? }` | `{ adopted }` — written decisions only |
| POST | `/leadership/slices/{id}/links` | `LinkInput` | `{ link }` — `commits` and `depends_on` require a note |
| POST | `/leadership/links/{id}/acknowledge` | — | `{ link }` — the linked project's steward, never the author |
| POST | `/leadership/retrospectives` | `RetrospectiveInput` | `{ retrospective }` — notes under 200 characters are 400 |
| GET | `/leadership/retrospectives` | — | `{ retrospectives: [...] }` |
| POST | `/leadership/retrospectives/{id}/actions` | `ActionInput` | `{ action }` — an owner is required |
| POST | `/leadership/actions/{id}/resolve` | `{ abandoned_reason? }` | `{ action }` |
| POST | `/leadership/cohorts/{id}/lead` | `{ curriculum_slice_id?, target_domain? }` | `{ leading }` |
| POST | `/leadership/cohorts/{id}/graduate` | `{ member_id }` | `{ graduated }` — the lead, never the member |
| POST | `/leadership/cohorts/{id}/departure` | `{ member_id, reason, note? }` | `{ recorded }` |
| POST | `/leadership/cohorts/{id}/conclude` | `{ note? }` | `{ outcomes }` |

---

## Security (42 routes)

> The `security` domain: coordinated disclosure, practice ranges, research mode,
> external bounty claims. Migrations 0542-0563. The narrative documentation is
> `docs/security/`, and `SECURITY.md` is the policy these routes implement.

### Public — no account

| Method | Path | Body | Response |
|--------|------|------|----------|
| GET | `/security/reference` | — | `{ orientations, reviewer_groups, severities, weakness_classes, finding_states, round_kinds, security_kinds, attestation_bases, mission_types, competition_kinds }` |
| GET | `/security/scope` | — | `{ in_scope, out_of_scope, rules_of_engagement, safe_harbour, contact }` — the machine-readable scope, unauthenticated on purpose: a researcher decides what to touch before they have an account |
| GET | `/security/hall-of-fame` | — | `{ researchers: [...] }` — confirmed findings only, anonymous reporters counted and not named |
| GET | `/security/findings/{id}` | — | `{ finding }` — the public card. Before publication it carries the severity, the class and the dates and withholds the title and the reproduction |
| GET | `/security/ctf/scoreboard` | — | `{ entries, first_solves }` |
| GET | `/security/external-bounties` | `?platform=&min_reward=&limit=` | `{ programs: [...] }` — curated third-party programmes |
| GET | `/trust/summary` | — | `{ policy, timelines, contact, threat_model_url, disclosed }` — what a recruiter or a client reads before trusting the platform |
| GET | `/users/{username}/security-profile` | — | `{ profile }` — score, confirmed findings by severity, captured flags, embargoed findings counted without being described |

### Reporting — authenticated

| Method | Path | Body | Response |
|--------|------|------|----------|
| POST | `/security/reports` | `SubmitInput` | `{ finding }` — 5/h. Out-of-scope hosts are refused with the reason, and a CVSS vector is scored server-side rather than trusted |
| GET | `/security/reports` | — | `{ findings: [...] }` — your own, with the rounds and the reviewer's reasoning |
| POST | `/security/reports/uploads` | multipart `file` | `{ key, size }` — 20/h, 20 MB each, private bucket. Returns a key and never a URL |
| GET | `/security/proofs?key=` | — | `{ url }` — a one-hour signed link. The reporter, a triager, a security reviewer, an administrator; nobody else. `key` is the bucket path, a query parameter because it holds slashes |
| POST | `/security/reports/{id}/withdraw` | `{ reason? }` | `{ finding }` — the reporter's own, before confirmation |
| POST | `/security/reports/{id}/answer-round` | `{ round_no, answer }` | `{ round }` — answering a question the reviewer asked |
| GET | `/security/external-bounties/claims` | — | `{ claims: [...] }` |
| POST | `/security/external-bounties/claims` | `ClaimInput` | `{ claim }` — a finding filed elsewhere, declared here for the attestation. Declared is not verified, and the record says which it is |

### Practice — authenticated

| Method | Path | Body | Response |
|--------|------|------|----------|
| POST | `/security/challenges/{id}/flag` | `{ flag }` | `{ correct, attempts_left_this_hour, first_solve }` — 10 attempts/h/challenge, counted in `security_flag_attempts` so brute force is visible afterwards. Only the hash is ever stored |
| POST | `/security/challenges/{id}/answers` | `{ answers }` | `{ score, passed, attempts_left }` — a defensive lab; running out closes it for 24 h |

### Research mode — authenticated

| Method | Path | Body | Response |
|--------|------|------|----------|
| GET | `/security/research-token` | — | `{ token }` — the metadata, never the secret again |
| POST | `/security/research-token` | `{ purpose, targets? }` | `{ token, secret }` — the secret is shown once. Multiplies every rate limit by ten and grants nothing else |
| DELETE | `/security/research-token` | — | `{ revoked }` |

### Admin — triage and disclosure

> `security_triager` confirms nothing and publishes nothing; a security reviewer
> confirms; publication is administrator-only. Enforced by the transition table
> in `services/security_findings.rs`, not by a convention.

| Method | Path | Body | Response |
|--------|------|------|----------|
| GET | `/admin/security/findings` | `?state=&severity=&overdue=&limit=` | `{ findings, sla }` — ordered by what is closest to breaching the 7-day triage promise |
| GET | `/admin/security/findings/{id}` | — | `{ finding, events, rounds, proofs, similar }` |
| POST | `/admin/security/findings/{id}/transition` | `{ to, reason }` | `{ finding }` — a reason is required for every transition including a refusal |
| POST | `/admin/security/findings/{id}/severity` | `{ vector, reason }` | `{ finding }` — the reported severity is kept alongside the override, permanently |
| POST | `/admin/security/findings/{id}/rounds` | `RoundRequest` | `{ round }` — up to five, each with a kind |
| POST | `/admin/security/findings/{id}/rounds/resolve` | `{ round_no, verdict, note }` | `{ round }` |
| POST | `/admin/security/findings/{id}/vendor-notified` | `{ note? }` | `{ finding }` — starts the disclosure clock for a third-party finding |
| POST | `/admin/security/findings/{id}/extension` | `{ days, reason }` | `{ finding }` — asking the reporter, not deciding for them |
| POST | `/admin/security/findings/{id}/extension/grant` | `{ days, reason }` | `{ finding }` |
| POST | `/admin/security/findings/{id}/withhold` | `{ reason }` | `{ finding }` — publication withheld, with the reason on the public record |
| POST | `/admin/security/findings/{id}/rescan` | — | `{ similar }` — trigram similarity against every other finding |
| GET | `/admin/security/dedup-queue` | — | `{ pairs: [...] }` — candidates for a human. A similarity score never merges anything: a merge decides who is credited |
| POST | `/admin/security/embargo-sweep` | — | `{ reminded, expired }` — the same sweep the daily worker runs. An expired embargo is flagged for a person and never published automatically |
| POST | `/admin/security/challenges` | `NewChallenge` | `{ challenge }` — the only way a flag or a lab is created, because the person creating it has to know the answer |
| GET | `/admin/security/external-bounties` | — | `{ programs: [...] }` |
| POST | `/admin/security/external-bounties` | `CuratedBounty` | `{ program }` |
| POST | `/admin/security/research-tokens/{id}/revoke` | `{ reason }` | `{ revoked }` |
| POST | `/admin/security/findings/{id}/blue-lab` | `LabFromFinding` | `{ challenge }` — a defensive exercise built from a real attack. Requires a written confirmation that no third party's identifiers remain in the artefact |
| GET | `/admin/security/bounty-claims` | — | `{ claims: [...] }` |
| POST | `/admin/security/bounty-claims/{id}/verify` | `{ note? }` | `{ claim }` — moves a declaration to verified, and issues the attestation |
| POST | `/admin/security/bounty-claims/{id}/refuse` | `{ reason }` | `{ claim }` |

---

## Health & Docs (2 routes)

| Method | Path | Auth | Response |
|--------|------|------|----------|
| GET | `/health` | No | `{ services: { postgresql, redis, judge0 }, status, version, websocket }` |
| GET | `/api/openapi.json` | No | The generated OpenAPI document, and the only one there is now — the older path served a hand-written copy that stopped being reachable, and kept being listed here anyway |

Swagger UI reads it at `/api/docs`, except where `SKILLUV_HIDE_SWAGGER` is
set, which leaves the raw document served on its own.

---

## WebSocket

| Path | Auth | Description |
|------|------|-------------|
| `/ws` | JWT cookie | A live WebSocket connection |

**Client to server:**
- `{ action: "join", room: "leaderboard:code" }` — join a room
- `{ action: "leave", room: "..." }` — leave a room
- `{ action: "ping" }` — keepalive

**Server to client (events):**
- `connected` — the connection is open
- `fragment.earned` — fragments earned
- `badge.earned` — a new badge
- `leaderboard.updated` — the leaderboard moved
- `challenge.submission` — a submission in a challenge room
- `notification` — a live notification: an interest request, a message, and the rest

**Rooms:** `user:{id}`, `leaderboard:{domain}`, `challenge:{id}`

---

## Data models

### UserPrivate (returned by /auth/me, /auth/login and others)
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
  "profile_hidden": false,
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

## Error codes

| Code | HTTP | Description |
|------|------|-------------|
| `RESOURCE_NOT_FOUND` | 404 | No such resource |
| `AUTH_INVALID_CREDENTIALS` | 401 | Wrong credentials |
| `AUTH_UNAUTHORIZED` | 401 | Not authenticated |
| `AUTH_FORBIDDEN` | 403 | Not allowed |
| `VALIDATION_ERROR` | 400 | The request did not validate |
| `AUTH_TOTP_REQUIRED` | 403 | A TOTP code is required |
| `AUTH_TOTP_INVALID` | 401 | Wrong TOTP code |
| `AUTH_EMAIL_2FA_INVALID` | 401 | Wrong e-mail second-factor code |
| `CHALLENGE_PREREQUISITE_NOT_MET` | 403 | The prerequisites are not met |
| `RATE_LIMITED` | 429 | Too many requests |
| `CONTACT_COOLDOWN_ACTIVE` | 429 | Cooling off after a refusal (30 days) |
| `CONTACT_ALREADY_REQUESTED` | 409 | A request is already open |
| `CONTACT_BLOCKED` | 403 | Blocked by the person |
| `CONVERSATION_CLOSED` | 403 | The conversation is closed |

Error shape:
```json
{
  "error": { "code": "ERROR_CODE", "message": "Description" },
  "meta": { "request_id": "uuid", "timestamp": "ISO8601" }
}
```

---

## Rate Limiting

| Endpoint | Limit | Window |
|----------|--------|---------|
| `/auth/register`, `/auth/login` | 10 req | per minute, per IP |
| `/sandbox/execute` | 20 req | per minute, per account |
| `/contact/interest` | 5 req | per hour, per company |

---

## Security Headers

Every response carries:
- `X-Content-Type-Options: nosniff`
- `X-Frame-Options: DENY`
- `X-XSS-Protection: 1; mode=block`
- `Strict-Transport-Security: max-age=31536000; includeSubDomains`
- `Content-Security-Policy: default-src 'self'`
