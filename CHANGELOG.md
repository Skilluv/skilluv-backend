# Changelog

All notable changes to the Skilluv backend are documented here.

The format is inspired by [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project will follow semantic versioning once 1.0 is reached.

## [0.2.0](https://github.com/Skilluv/skilluv-backend/compare/v0.1.0...v0.2.0) (2026-09-04)


### Features

* **admin:** the lists behind the buttons, and four smaller repairs ([#108](https://github.com/Skilluv/skilluv-backend/issues/108)) ([cba5fc0](https://github.com/Skilluv/skilluv-backend/commit/cba5fc03761f3d4dc086d24b7f97a35789cce2d8))
* api key scopes, the derived CTF catalogue, and the moment a trade opens ([#109](https://github.com/Skilluv/skilluv-backend/issues/109)) ([93f57fb](https://github.com/Skilluv/skilluv-backend/commit/93f57fb266a891fee6ba5a7c9a2afeaf0afed3c5))
* **discord:** a server this repository declares, and the capability catalogue the admin panel could not hold ([#102](https://github.com/Skilluv/skilluv-backend/issues/102)) ([678d98b](https://github.com/Skilluv/skilluv-backend/commit/678d98b8e7c2ef1b915fa3a72c2fa9e6cbb89813))
* **discord:** the link that makes every role on the server possible ([#107](https://github.com/Skilluv/skilluv-backend/issues/107)) ([98aa199](https://github.com/Skilluv/skilluv-backend/commit/98aa19920575194e5ff25fd87223051ae2328e27))
* five domains open, and the surfaces the front end could not reach ([#94](https://github.com/Skilluv/skilluv-backend/issues/94)) ([2cccfa2](https://github.com/Skilluv/skilluv-backend/commit/2cccfa207e32c180b1af086386a58d19bd111680))
* five unreachable surfaces, the cyber and design rooms, and the lint that never saw the bot ([#110](https://github.com/Skilluv/skilluv-backend/issues/110)) ([934ac34](https://github.com/Skilluv/skilluv-backend/commit/934ac342f244f1ae28dc26f9a9f99327e9c0359d))
* **onboarding:** the newcomer path runs end to end ([#120](https://github.com/Skilluv/skilluv-backend/issues/120)) ([0aaf180](https://github.com/Skilluv/skilluv-backend/commit/0aaf180bd62234d032f8f28657164ed8a21af8f1))
* the game domain, a deployment that seeds itself, and the admin surfaces the front could not reach ([#98](https://github.com/Skilluv/skilluv-backend/issues/98)) ([a8fca0e](https://github.com/Skilluv/skilluv-backend/commit/a8fca0ec300f69ec53761c8a669ce83bdf0ad60b))
* the game domain, and the backend halves of the front-end feedback batch ([#96](https://github.com/Skilluv/skilluv-backend/issues/96)) ([a28e879](https://github.com/Skilluv/skilluv-backend/commit/a28e879bea1f4cee44237f81bcbd5b195caea9fd))


### Bug Fixes

* **attestations:** tell four kinds of claim apart on the printed sheet ([#101](https://github.com/Skilluv/skilluv-backend/issues/101)) ([679d731](https://github.com/Skilluv/skilluv-backend/commit/679d731800a67a9e32bb11d58053cf5630847909))
* **ci:** make the nightly stack, fuzz, and smoke actually run ([#100](https://github.com/Skilluv/skilluv-backend/issues/100)) ([536903b](https://github.com/Skilluv/skilluv-backend/commit/536903b9bba391a3ffbfb4bad8a6cdc216715afd))
* **discord:** the slash commands the bot could not register ([#106](https://github.com/Skilluv/skilluv-backend/issues/106)) ([16dbbe4](https://github.com/Skilluv/skilluv-backend/commit/16dbbe4b7ff02395ae94a7c651e50a4139e6ba9d))
* **docker:** warm the dependency cache with the features the build uses ([#105](https://github.com/Skilluv/skilluv-backend/issues/105)) ([e8f9666](https://github.com/Skilluv/skilluv-backend/commit/e8f9666ae8fff5bc1b735dd5cceb9e786841a8bf))
* four production defects, and a CI that tested every tree twice ([#122](https://github.com/Skilluv/skilluv-backend/issues/122)) ([d794fdc](https://github.com/Skilluv/skilluv-backend/commit/d794fdc377d833c5e6e4a4b052f814486e1f3194))
* **migration:** unblock the staging deploy, and two things it exposed ([#121](https://github.com/Skilluv/skilluv-backend/issues/121)) ([9f45325](https://github.com/Skilluv/skilluv-backend/commit/9f45325fe12eb2b80fa116ed44c5508849c3f8d6))


### Performance Improvements

* **ci:** lint is a pure function of the source, so skip it on a tree already linted ([#123](https://github.com/Skilluv/skilluv-backend/issues/123)) ([20d8c1d](https://github.com/Skilluv/skilluv-backend/commit/20d8c1d2c2f111e94d42459599f27b737b0cb0c2))

## [Unreleased]

### Added

* **security:** the domain opens, and with it the first thing this platform
  runs against itself. Five orientations — red team, blue team, code audit,
  governance, purple team — replace four that named job titles and reviewed
  nothing. Forty-six skill nodes, ninety-four edges, six review grids,
  seventeen attestation bases, twenty-nine badges, twenty craft-score weights,
  eleven mission types, nine terrains, ten onboarding guides, twenty templates
  and forty-six seeded challenge drafts. Migrations 0542-0563.
* **security:** `security_findings` — the one genuinely new object in the
  domain, and the migration that adds it argues why: a live target, a severity
  two people negotiate, and an embargo. A state machine with a written reason
  on every transition, five rounds of questions, append-only events, trigram
  deduplication that proposes and never merges, and a 90-day disclosure clock
  that reminds at 30, 7 and 1 days and then hands the decision to a person.
  Nothing is published by a cron job.
* **security:** a confirmed finding creates a `deliverables` row and therefore
  moves the cross-domain rank, exactly as a merged pull request does. A
  captured flag issues an attestation and creates no deliverable, so a training
  range cannot promote anybody. That asymmetry is the point of the design.
* **security:** CVSS 3.1 base scoring in `services/cvss.rs`, including the
  specification's integer `Roundup`, so the severity on an attestation is
  computed from the vector rather than taken on trust.
* **security:** research mode. `POST /api/security/research-token` multiplies
  every rate limit by ten and grants nothing else; traffic beyond 500 requests
  in a minute revokes the token automatically and says so. The multiplier
  reaches the limiter through a task-local rather than through a parameter
  added to a hundred handlers.
* **security:** practice ranges. CTF challenges and defensive labs are
  `challenge_templates` — many solvers, repeatable — and only hashes of flags
  and answers are stored, so a database dump does not hand anybody the answers.
  A defensive lab can be generated from a real attack on the platform, once an
  operator confirms in writing that no third party's identifiers remain in it.
* **security:** paid work. Eleven mission types, an authorisation trigger that
  refuses to let an offensive engagement leave draft without rules of
  engagement, rank and credential gates on applications, and two versioned
  confidentiality agreements whose signature records the SHA-256 of the exact
  text shown.
* **security:** the published policy. `SECURITY.md` gains a safe harbour and
  timelines that match the constants that enforce them; `THREAT_MODEL.md`,
  `PRIVACY.md` and `INCIDENT_RESPONSE.md` are new, and each is also an audit
  exercise in the catalogue — a document nobody has attacked is a document
  nobody has checked.
* **security:** external bounty claims. A finding filed on HackerOne or
  Bugcrowd can be declared here for an attestation, and the record says
  *declared* until somebody verifies it.
* **communication:** the domain opens, with five trades — technical writer,
  developer advocate, technical content creator, technical translator,
  research writer — four review families, and every table a domain needs:
  skill nodes and their map, review grids, six attestation bases, twelve
  badges, a slice type with six subtypes, craft-score weights, seven mission
  types, ten portfolio platforms, two contest formats, five award categories,
  thirty seeded challenges and the full set of guides, briefs and writeup
  templates.
* **communication:** translation review. A translation is the one artefact on
  the platform never attested automatically: somebody declares in writing the
  languages they read, and signs a review in one of them. Ticket W-04 asked
  for one capability per language; there are seven thousand languages, and the
  declaration is `user_review_languages` instead.
* **education:** the domain opens, with three trades — technical trainer,
  coding teacher, curriculum designer — two review families, and the same full
  set of tables, plus taught cohorts, per-learner outcome records and
  curriculum adoption.
* **education:** the learner-data gate. Every delivery that reports on
  learners waits, unattested, until its author states in writing that no
  identifiable learner remains; a testimonial cannot be stored without
  consent, and the schema enforces it rather than trusting anybody to
  remember.
* **opportunities:** `/api/opportunities` — curated calls for papers, speaker
  slots and teaching positions, public to read and curated to write. One table
  where the communication and education backlogs each asked for their own.
* **portfolios:** `/api/portfolios` and `/api/portfolio-platforms` now serve
  every domain, and `/api/slices/{id}/revisions` likewise. Both were audio
  endpoints reading tables that already carried a domain.
* **registries:** DEV, Hashnode, Medium, YouTube, Speaker Deck, arXiv and
  Zenodo are recognised as places a published artefact lives, with fetchers
  for the four that answer.
* **moderation:** `GET /moderation/vouchings?status=live|broken|expired`. The
  break endpoint shipped with no way to find the id it takes, and a broken
  vouching was readable nowhere at all.
* **admin:** `GET /admin/cohorts` and `POST /admin/cohorts/{id}/archive`.
  Archiving freezes the chat, because members went on reading it — which
  answered the organizer and not the people who had worked there.
* **admin:** `GET /admin/talent-offers` with `/deactivate` and `/reinstate`.
  The moderation hold is its own column: `active` is the author's own pause
  switch, and a hold placed there would be undone by their next PATCH. The row
  survives, because a dispute is instructed against what was published.
* **onboarding:** one Bonjour Skilluv rite per domain, twelve gestures. `code`
  still forks a starter and opens a pull request; the other eleven hand in an
  artifact in the shape of their trade — a screen, a playtest verdict, a
  finding, an SLO reading, a defect report, a workspace step, a review, twenty
  seconds of sound, a retro, a translation, an explanation — and land in the
  human review queue. `POST /onboarding/bonjour-skilluv/start` no longer loads
  a GitHub token before doing anything else, `GET /onboarding/rites` serves the
  whole catalogue for the signup screen, and the status endpoint describes the
  caller's gesture before they have started it. Migrations 0607 and 0609.
* **orientations:** `GET /orientations` carries the number of rows its filter
  matches, and `GET /orientations/counts` answers "how many specialities per
  class" in one call. The catalogue is ~255 curated rows against a `limit` that
  caps at 200 and defaults to 50, so a client with no parameters received a
  fifth of the trades with nothing saying so.

* **admin:** `GET /admin/assistant/stats` and
  `GET /admin/users/{id}/assistant-interactions`. Two facts were
  unrecoverable — a cache hit could not be told from a call, and a refusal
  recorded nothing at all.

### Fixed

* **challenges:** a challenge is served in the language the reader asked for.
  Migration 0104 added `title_i18n` / `description_i18n` / `instructions_i18n`
  and nothing ever read them — not a route, not even the struct — while the
  catalogue quietly stopped being French: 404 rows are French, 254 English, in
  the same column with no marker. A bilingual front could not know what it was
  about to display. The API resolves `Accept-Language` now, says which
  languages a challenge exists in, and falls back to the base text rather than
  to an empty string. Migration 0613.
* **challenges:** a brief says where to start reading. `challenge_resources`
  attaches documentation, courses, articles, videos, communities and the
  repository the work lands in — links to material somebody else hosts, never
  copies, each with the language it is in and what it costs to reach it. The
  guidance narrows as a rank climbs and stops entirely at `doyen`: handing a
  fifteen-year practitioner a link to the official docs is noise, and noise on
  every page teaches people to stop reading the page. Migration 0614.
* **forum:** a thread can name the challenge it is about. `posts.challenge_id`
  is what makes a question asked once readable by everybody who starts that
  challenge afterwards — the most valuable teaching material the platform will
  produce, and the one that writes itself. Migration 0614.
* **challenges:** six published code exercises, in both languages, in order.
  Each says what is out of scope, names the repository it lands in, and has a
  next one — `challenge_prerequisites` was empty across the whole platform, so
  no challenge had a successor and "what do I do now" had no answer that was
  not a search box. Migration 0615.
* **onboarding:** a trade is chosen before the first gesture. The trade picks
  the starter to fork, feeds the playlist and the recommendations, and is what
  a reviewer is matched on; starting without one meant forking the broad-appeal
  default and then being recommended nothing in particular. One is required,
  three are allowed.

* **onboarding:** the starter a trade forks is resolved from its family, not
  from a list of slugs written when there were 32 orientations. The table holds
  150, so 118 of them silently forked `starter-fullstack-node` — a
  `compiler-language-developer` was handed a Node fullstack app on the strength
  of the trade they had just declared, and 32 of the 41 code trades were in
  that group. Resolution now goes per-slug exception, then
  `orientations.reviewer_group`, then `None`; the caller applies the default
  once, so "nothing knows this trade" stays visible. The check that should have
  caught this looped over a constant of 32 slugs commented "snapshot au
  2026-07-22" — a snapshot of what the mapping already covered, comparing the
  list to itself. It reads `orientations` now, and is an integration test.

* **challenges:** every seeded challenge names the trade it belongs to.
  Migration 0606 backfilled `orientation_id` for the 130 design seeds and left
  524 drafts across the other ten domains at NULL — and
  `POST /admin/orientations/{slug}/challenges/publish`, the one surface that
  opens a catalogue one trade at a time, selects on exactly that column. It
  published nothing for eleven domains of twelve, which is why the platform
  stood at 654 drafts and zero reachable published challenges: somebody
  finished their first gesture and `GET /api/challenges` handed them an empty
  list. The pairs are read back out of the seed migrations, which all grouped
  their VALUES by trade to pick a review grid and then discarded it.
  Migration 0612.

* **challenges:** a submission hands in its artifact, and the reviewer can open
  it. `POST /challenges/{id}/submit` takes `attachments` — `design_upload:<uuid>`
  or `audio_file:<uuid>`, each checked to belong to the caller. References
  rather than URLs, because a free-text URL lets somebody be reviewed on
  another candidate's screen. And `design_uploads` was owner-scoped, so even
  attached, the reviewer got a 404 on the only thing they were asked to judge;
  a reviewer holding an open task on a deliverable that references the upload
  can now read it, and stops being able to once the verdict is in.

* **reviews:** nobody signs off their own work, and a verdict is a competence
  rather than a login. `POST /deliverables/{id}/reviews` accepted any
  authenticated account — including the deliverable's own author, who could
  approve themselves the fragments, the settled submission and the active
  profile that an `approve` now carries. Self-review is refused outright; a
  verdict requires `admin`, `mentor`, or `domain_curator` for that
  deliverable's domain. The design critique loop has enforced both from the
  start; this is the same rule on the generic queue, which P2.2 had left open
  for the cold start.
* **onboarding:** an approved pull request completes the code rite. The webhook
  took it to `pr_opened` and nothing moved it further, while
  `badge_rules.bonjour_skilluv` fires on `completed_at IS NOT NULL` — so the
  founding badge was unreachable on the one path that had shipped. The pull
  request now becomes a deliverable in the same review queue as the other
  eleven rites, and the verdict closes it. Opening a pull request proves
  somebody pushed a branch, not that a person read what is on it.

* **challenges:** a non-code submission of a hundred characters is no longer a
  pass. `evaluate_basic` returned `success` and its fragments for any
  submission of a hundred characters in every domain but `code`, and returned
  an unconditional `success` for a code challenge declaring no expected output.
  On a public profile that mark was indistinguishable from one a reviewer gave,
  in the one thing this platform asks anybody to trust it about. A submission
  nothing can score is now `pending_review`: its deliverable is queued for a
  person, and the fragments follow their verdict rather than its length.
  Migration 0608.
* **challenges:** every declarable domain has a published onboarding challenge.
  Eight of the twelve answered `GET /challenges/onboarding` with "No onboarding
  challenge found", so the first screen after signing up was an error for two
  thirds of the platform. The four 2024 seeds it did have asked, in French, for
  "minimum 100 mots" — written against the character count above — and are
  retired. The lookup is also deterministic now: `code` carries fifteen
  onboarding templates, one per starter, and `LIMIT 1` returned an arbitrary
  one of them.
* **openapi:** `skill_domain` lists the twelve domains the server accepts,
  everywhere a request carries one. `RegisterRequest` documented four, so a
  generated client refused seven valid domains and the contract fuzzer never
  sent `audio` or `leadership`. The list had been transcribed eight times
  across request bodies and query parameters; every copy now emits
  `validators::SKILL_DOMAINS`, and a test walks the built document and fails on
  the next copy.

* **profiles:** `GET /profile/{username}` returns `id`. Four public endpoints
  are addressed by UUID and this was the only place a visitor could resolve a
  username to one, so the front end could render those sections on your own
  profile and on nobody else's.
* **vouchings:** a vouching carries `voucher_username`, and `/users/me/vouchings`
  resolves the other party on both sides. Profiles are addressed by username,
  so a display name was the one thing a link could not be built from — on the
  section whose whole point is going to check who vouched.
* **moderation:** deleting an external signal requires a motive and writes what
  it destroyed into the audit log. It was the most destructive endpoint of the
  batch, it did not accept a reason, and the missing journal entry was only the
  consequence of that.
* **openapi:** ninety-four handlers shared a generated `operationId` with
  another handler, because utoipa derives it from the function name and
  `list`, `create` and `detail` are not unique across forty-five route files.
  Every generated client had colliding methods. The ids are explicit now and a
  test refuses a duplicate.
* **attestations:** issuing the same artefact attestation twice raised a unique
  violation instead of returning the existing one, so any generator that ran a
  second time — a sweep, a retry, a webhook redelivery — failed. The insert
  reads the row back on conflict.
* **practice:** the toolkit listing matched a resource's domain and nothing
  else, so a tool tagged for an orientation in that domain was invisible.
* **missions:** `production_access` missions required an NDA and had no way to
  name which one, which the new constraint made explicit; creation now
  defaults to the standard mutual agreement.
* **tests:** the orientation cap-of-three test asserted a 400 using a slug that
  had since been archived, so it passed for the wrong reason.
* **missions:** `licensing_scope` was not settable through the API, so from
  migration 0413 onwards an audio mission could not be created at all — the
  insert was refused by a constraint naming a column the request had no way to
  reach.
* **craft-score:** the code score summed downloads across *every* published
  artefact a person had, so a HuggingFace model or a container image paid into
  a term called `library_downloads`. `/code-profile` listed them too.
  `publication_registries` carries the domain, and both queries now say what
  they mean.
* **badges:** a `skill_domain` condition read the challenge behind a
  deliverable and nothing else, so every domain badge counted only training
  challenges and ignored work delivered against a project slice — which is the
  work the platform exists to produce.
* **guides:** a guide with no row in the requested locale was hidden rather
  than served in the next best one. The fallback is now requested → English →
  French, and English is the default when the caller expresses no preference.

## 0.1.0 (2026-08-21)


### Features

* **admin-extras:** POST /admin/users/{id}/recompute-capabilities + GET/POST/PUT /admin/skills CRUD (5 tests) ([30c1ede](https://github.com/Skilluv/skilluv-backend/commit/30c1ede5a61a14f913b73f30d5935fffa56bf1aa))
* **admin-gaps:** GET /admin/enterprises/{id} + GET /admin/badge-events (paginé, filtres is_active/is_partner) — comble les zones d'ombre du wiring admin front ([1b2bdc1](https://github.com/Skilluv/skilluv-backend/commit/1b2bdc1e96e1cf7c2b6bf915061df1a9e4964071))
* **admin-mvp:** ADM-M3+M4+M5 — orientations + badge_rules + enterprises + users admin (13 endpoints, 18 tests) ([8e7916d](https://github.com/Skilluv/skilluv-backend/commit/8e7916dc49aa68c0f517e3966814f640757b5ca5))
* **admin-ops:** ADM-M5+ — POST /admin/proof-hooks/sweep + POST /admin/users/{id}/gdpr-export + GET /users/{id}/rank-history (5 tests) ([2133ded](https://github.com/Skilluv/skilluv-backend/commit/2133dedf91591d2e22eaacb89bbf1a035b36389d))
* **admin-security:** BE-A + BE-B + BE-C — 2FA mandatory admin + reset-2fa endpoint + origin gate middleware ([e40fc87](https://github.com/Skilluv/skilluv-backend/commit/e40fc87753d2b09e4bb269fee5aa9fa73a5b1f38))
* **admin-security:** BE-D + BE-E + BE-F — rate-limit destructif + dry-run + audit append-only + audit on 5 handlers ([e289d37](https://github.com/Skilluv/skilluv-backend/commit/e289d373ee24e75e117936cfdafafde5ad48be4f))
* **ai:** P15.2 LLM verifier wrapper skilluv-ia (verifiable_by llm_evaluation) ([cfedc48](https://github.com/Skilluv/skilluv-backend/commit/cfedc487b37d9e8472c2f5b5ba22078f91ed75be))
* **badge-events:** POST /api/admin/badge-events pour créer un event (Hacktoberfest, Skilluv Fest) + 3 tests ([ea9862e](https://github.com/Skilluv/skilluv-backend/commit/ea9862e91412d29278c268d6ccc4c885cab063c5))
* **badges:** P17.1 badge_rules + user_badges proof-engine (source_proofs, rarity, revoked_at) + migrate 9 legacy badges ([778ab2f](https://github.com/Skilluv/skilluv-backend/commit/778ab2f2f5da566488548f3b4ddebf7ef624efa9))
* **badges:** P17.2 skill_nodes.display_category (Craft/Create/Understand/Operate/Share/Meta) + deterministic backfill ([ffc95d4](https://github.com/Skilluv/skilluv-backend/commit/ffc95d408298b08923d42778c4f220c3d9131d08))
* **badges:** P17.3 rules engine (evaluate JSONB conditions, auto-rarity, revoke on unmet) ([84b9ab3](https://github.com/Skilluv/skilluv-backend/commit/84b9ab3390dfbc33f5870477f9905c5b818f9517))
* **badges:** P17.4 rank system Apprenti-&gt;Doyen (auto-promotion unidirectionnelle + historique) ([78a5b1f](https://github.com/Skilluv/skilluv-backend/commit/78a5b1f8e2978d71a22cd357d56741c62baab0f2))
* **badges:** P17.5 API polymorphique GET /users/{id}/badges + GET /badge-rules ([a83ae77](https://github.com/Skilluv/skilluv-backend/commit/a83ae7701b86fa00a8f18fdf6590b5c6e7cb729f))
* **badges:** P17.6 events + participation (namespaced /badge-events) + CHANGELOG P17 ([53240d5](https://github.com/Skilluv/skilluv-backend/commit/53240d589cc90ed713eb49c34899006f103470ab))
* **capabilities:** P18.1 user_capabilities cumulables + backfill users.role ([97fd9f9](https://github.com/Skilluv/skilluv-backend/commit/97fd9f949431d307d357c909d3d880fa08413f71))
* **capabilities:** P18.2 capabilities_engine auto-promotion (challenger/mentor/pr_reviewer/issue_proposer/project_steward) ([2866f7f](https://github.com/Skilluv/skilluv-backend/commit/2866f7f4ade4d98c777afdad899ac1e95f47883b))
* **capabilities:** P18.3 middleware require_capability (exclut revoked + expired) ([a4adee4](https://github.com/Skilluv/skilluv-backend/commit/a4adee4a1dcbc8c53392f074a0651625d72a635e))
* **capabilities:** P18.4 API GET/POST/DELETE capabilities (public + admin grant/revoke) ([130f11a](https://github.com/Skilluv/skilluv-backend/commit/130f11ac47befe3483194efa13f9b00e7119158e))
* **capabilities:** P25.1 extend enum with 5 community moderator caps (front-only, not admin panel) ([5b7530f](https://github.com/Skilluv/skilluv-backend/commit/5b7530f8b8ada4b49c876f2d06940aed73bf4fc3))
* **capabilities:** P25.2 auto-promotion community_curator + forum_moderator + umbrella community_moderator (plagiarism/kyc manual-only) ([3cc351f](https://github.com/Skilluv/skilluv-backend/commit/3cc351fabb741e8d40a128349e1ffe3cfcad1b0c))
* **capabilities:** P25.3 helper require_any_capability + doc MODERATION-vs-ADMIN ([813d91b](https://github.com/Skilluv/skilluv-backend/commit/813d91bf97677d8ba3893d2456749e68c303667d))
* **challenges:** P0 fondations du modèle cible (slices, deliverables, skill graph) ([47cafc8](https://github.com/Skilluv/skilluv-backend/commit/47cafc80c7dd1eadee79b1260a3d18ab305a322a))
* **challenges:** P1 project_slices service + routes + backfill bounties ([b680a06](https://github.com/Skilluv/skilluv-backend/commit/b680a06c05bb1e36eb5bfafd2f230bc8db74efe0))
* **challenges:** P2.1 deliverables + webhook GitHub PR merged → auto-verified ([8e3095f](https://github.com/Skilluv/skilluv-backend/commit/8e3095f3e32803f400aceb672c3dbeaf130b4a9a))
* **challenges:** P2.2 review queue humaine (review_tasks + verdict flow) ([1a74d40](https://github.com/Skilluv/skilluv-backend/commit/1a74d401261d4289ecbcb1c443bdf2086cc0aade))
* **challenges:** P3 DAG des prérequis + tracks + eligibility ([b846749](https://github.com/Skilluv/skilluv-backend/commit/b846749a22e68c20f908f0a59fd51fd78739e3b7))
* **challenges:** P4 skill graph exposé au profil + recherche recruteur + recos ([1bbf5a8](https://github.com/Skilluv/skilluv-backend/commit/1bbf5a86ead3095f10e6ebc0ffb8e18f5bf6d076))
* **challenges:** P5 ⭐ attestations — KILLER FEATURE (auto-issue + verify + revoke) ([2bacfd1](https://github.com/Skilluv/skilluv-backend/commit/2bacfd1136fc13f2ad5a8e3912a3e6c68132bada))
* **challenges:** P6 seasons + project stewards ([4d18639](https://github.com/Skilluv/skilluv-backend/commit/4d1863984031d83c3b1e3a039b7dc3c7a1bf2943))
* **challenges:** P7 portfolio export (JSON-LD schema.org + badge SVG) ([340ddba](https://github.com/Skilluv/skilluv-backend/commit/340ddbada9469a91a3c37c52b8c618014782976a))
* **challenges:** P8.1 admin.rs accepte ai_policy + expose champs P0-P3 sur Challenge model ([e88eafb](https://github.com/Skilluv/skilluv-backend/commit/e88eafb066c25a5a04dc5afd8d967d29168f77a6))
* **challenges:** P8.2 DAG check dans /api/challenges/{id}/start avec fallback ([a135b95](https://github.com/Skilluv/skilluv-backend/commit/a135b95ba8bcd37835968d29f27e67833e46f978))
* **challenges:** P8.3 drop ai_allowed + prerequisite_fragments ([626eeac](https://github.com/Skilluv/skilluv-backend/commit/626eeac448f0e54fa339d59e201fbe213cbdf514))
* **challenges:** P8.4 bounties.rs dual-write vers project_slices ([fd120d3](https://github.com/Skilluv/skilluv-backend/commit/fd120d337de76e4381dbef86d301e8fa43c129f4))
* **challenges:** P8.5a submit dual-write challenge_submissions → deliverables ([cc34880](https://github.com/Skilluv/skilluv-backend/commit/cc34880ee5ac28104bdaf78cf78816320438789e))
* **challenges:** P8.5b headers Deprecation/Sunset/Link sur submit legacy ([6a33e75](https://github.com/Skilluv/skilluv-backend/commit/6a33e75585ea139a793868ca10644b0c511ba6bc))
* **challenges:** P8.5c user_skills propagation depuis challenge legacy ([71dbd1f](https://github.com/Skilluv/skilluv-backend/commit/71dbd1fdfcff1fb3bd3a9ef351db211d933e7311))
* **challenges:** P8.6 skill_fragments consumers fallback vers user_skills ([ef00e90](https://github.com/Skilluv/skilluv-backend/commit/ef00e9017aa90a7daa4bf182a1218931210cf279))
* **challenges:** P8.6b top-skills consumers fallback (talent_search + github) ([c9b2c90](https://github.com/Skilluv/skilluv-backend/commit/c9b2c90e4f2c023d985ed8452af7f6d6975872b3))
* **challenges:** P8.6c leaderboard + data_export MAX(legacy, user_skills) ([37795db](https://github.com/Skilluv/skilluv-backend/commit/37795db66a4abe5373804f405c32c00f737fc10e))
* **challenges:** P8.7 drop table skill_fragments et cleanup consumers ([3426eac](https://github.com/Skilluv/skilluv-backend/commit/3426eac066f9a33cc10ba21303b0a65069af928e))
* **challenges:** P9.1 drop challenge_submissions.code|stdout|stderr ([dbcb28e](https://github.com/Skilluv/skilluv-backend/commit/dbcb28ec52104f11e1e3eab862428c8928aa5dfc))
* **challenges:** P9.2 fusion oss_bounties -&gt; project_slices + drop ([d9d402b](https://github.com/Skilluv/skilluv-backend/commit/d9d402b9c771a3f6d267bb6a0cd4a82f5fa12f0e))
* **challenges:** P9.3 rename table challenges -&gt; challenge_templates ([52ad13b](https://github.com/Skilluv/skilluv-backend/commit/52ad13bf453bd60850592141073dce119b8a7465))
* content strategy migrations batch 2 (onboarding Bonjour Skilluv + admin ops + workers) ([#26](https://github.com/Skilluv/skilluv-backend/issues/26)) ([8d2c858](https://github.com/Skilluv/skilluv-backend/commit/8d2c85869a755ba904421d6d51b7d8f8c64ea63e))
* **content:** 4 migrations pour décisions stratégiques 2027 — mentorship, profil vivant, hello wall, disclosure IA ([#22](https://github.com/Skilluv/skilluv-backend/issues/22)) ([c39db14](https://github.com/Skilluv/skilluv-backend/commit/c39db146d8318f11c70c4f810d41c5f0df842ef3))
* **content:** stratégie contenu 2027 — i18n challenges + 32e orientation IoT + reframe blockchain ([#21](https://github.com/Skilluv/skilluv-backend/issues/21)) ([a0e4d9e](https://github.com/Skilluv/skilluv-backend/commit/a0e4d9ece421155cac8cebe931a659b9a82bdf73))
* **dev:** SKILLUV_DEV_MODE endpoint /api/dev/verify-tokens/{email} + rate-limit IP whitelist ([#39](https://github.com/Skilluv/skilluv-backend/issues/39)) ([d9236c0](https://github.com/Skilluv/skilluv-backend/commit/d9236c063dfdc538f8a182179fbb4313b05025be))
* **discovery:** P12.1 recommend_for_user projet matching ([f86d220](https://github.com/Skilluv/skilluv-backend/commit/f86d220231e15fe3887de9a81d69a1a79818d5a0))
* **discovery:** P12.2 user_project_interests + onboarding endpoints ([f78a639](https://github.com/Skilluv/skilluv-backend/commit/f78a6390dd4affe77478f69e844d66f385463ef0))
* **discovery:** P12.3 GET /api/feed/for-you (mix personnalise) ([5de34dc](https://github.com/Skilluv/skilluv-backend/commit/5de34dce479d08b6415d2104494f54daf8240b01))
* **discovery:** P12.4 GET /api/explore multi-criteres ([239d93f](https://github.com/Skilluv/skilluv-backend/commit/239d93fcff11a417693e57115c42f7825a3dcb56))
* **enterprises:** P24.1 enterprise_type enum (direct_hire/staffing_agency/remote_international) ([40ff62e](https://github.com/Skilluv/skilluv-backend/commit/40ff62efe7fcb10f6396b353783aa0eedf2a9dd8))
* **enterprises:** P24.2 agency_clients + trigger PG (reserve staffing_agency) + routes CRUD ([a45ec2a](https://github.com/Skilluv/skilluv-backend/commit/a45ec2a3c9dd471e22a352871e6649c331af329f))
* **enterprises:** P24.3 type_config JSONB + routes GET/PATCH avec allowlist par type ([10ff8a1](https://github.com/Skilluv/skilluv-backend/commit/10ff8a18607941ecfad3bcabfe780fb3e9506d09))
* **fraud:** P14.3 anti-plagiat cross-user via cosine similarity ([b1accde](https://github.com/Skilluv/skilluv-backend/commit/b1accdeea97418d1f00b35e34a76d62968915af2))
* **fraud:** P14.4 fingerprinting login + detection multi-account ([7244ced](https://github.com/Skilluv/skilluv-backend/commit/7244ceddf7a1f17350cbe29c132c0bf92850f9fa))
* **fraud:** P14.5 admin fraud dashboard endpoints ([a6c3b39](https://github.com/Skilluv/skilluv-backend/commit/a6c3b3984a1b3aaea3aa7ac266e84857b999116e))
* **guilds:** P10.6 guild skill matrix (agregat par domaine) ([33daf75](https://github.com/Skilluv/skilluv-backend/commit/33daf75e8ff220d257ff046d789bb4cb375df950))
* **ia-integration:** IA-A sync proto v2 + refactor AiClient (+ generate_variant + timeout 60s) ([6bcf724](https://github.com/Skilluv/skilluv-backend/commit/6bcf724ea515bfad895d8957a07902a47f11e1d6))
* **ia-integration:** IA-B route POST /admin/fraud/deep-scan/{id} (LLM plagiarism hybride) ([614c6e2](https://github.com/Skilluv/skilluv-backend/commit/614c6e26d02b10fa933c28fd95d4a794e77f7ada))
* **ia-integration:** IA-C.1+C.2+C.3 — variant + performance + suggest orientations ([efe633f](https://github.com/Skilluv/skilluv-backend/commit/efe633f44d07b12d0832bbb240d2cbab60ddaac0))
* **ia-integration:** IA-D ai_call_log migration 0101 + services/ai_log helper + wire on 5 gRPC call sites ([29c5173](https://github.com/Skilluv/skilluv-backend/commit/29c5173fdbb7df2b453e28563639ff16b17413eb))
* **ingest:** P11.1 GitHub polling worker + SliceIngestor trait ([2a3ec93](https://github.com/Skilluv/skilluv-backend/commit/2a3ec935f1794c23241d0a4195da4d5ef7e3ca87))
* **ingest:** P11.2 webhook issues.labeled -&gt; ingestion real-time ([59d4cce](https://github.com/Skilluv/skilluv-backend/commit/59d4ccefd9541781d177d77836ca54f80ba934b4))
* **ingest:** P11.3 SliceIngestor extensibility (Figma stub + dispatch) ([7ae29f2](https://github.com/Skilluv/skilluv-backend/commit/7ae29f2f41429d21f23bc0b48ac0ff0f68c315e8))
* **ingest:** P11.4 steward inbox + publish/reject drafts ([ec904e3](https://github.com/Skilluv/skilluv-backend/commit/ec904e3b2f1760c0efceca8a23ae614aa72c6a45))
* **invoices:** add GET /enterprise/invoices/{id}/pdf via external renderer (BE-P0-36) ([#36](https://github.com/Skilluv/skilluv-backend/issues/36)) ([1ac855c](https://github.com/Skilluv/skilluv-backend/commit/1ac855c98b41da251733b09d164d2969eb95c6d3))
* **moderation:** FE-M9 — 8 routes modération inline (community + fraud + forum) via require_any_capability + user_mutes migration + 6 tests ([c947ab6](https://github.com/Skilluv/skilluv-backend/commit/c947ab624c04d29978076639683e895de7b435bc))
* **monetization:** BE-P26 bounty platform fee 8% + platform_revenues ledger ([3b1df59](https://github.com/Skilluv/skilluv-backend/commit/3b1df59db917a9890cda70e97077606f14680d70))
* **onboarding+content:** Bonjour Skilluv + admin projects + seed Saison 1 ([#23](https://github.com/Skilluv/skilluv-backend/issues/23)) ([be85700](https://github.com/Skilluv/skilluv-backend/commit/be8570009e83674330255b614ce2551974ee3bff))
* **orientations:** FE-M1 — GET /api/users/{id}/orientations route publique + privacy via profile_active + 3 tests ([46f3df5](https://github.com/Skilluv/skilluv-backend/commit/46f3df57761073e35e55d506d328d210cb65d927))
* **orientations:** P16.1 catalogue orientations métier + mapping skills (31 tracks curated) ([ef8cc5c](https://github.com/Skilluv/skilluv-backend/commit/ef8cc5cefbe9cbf78d7243cc33a8397e742a7902))
* **orientations:** P16.2 user_orientations + backfill depuis users.skill_domain ([7b1e46e](https://github.com/Skilluv/skilluv-backend/commit/7b1e46e99ba69b134d83be129f19799e147bb261))
* **orientations:** P16.3 routes catalogue + user_orientations CRUD (cap 3, historisation) ([0b49bd2](https://github.com/Skilluv/skilluv-backend/commit/0b49bd2ca27031b3e439736382feecd23d515248))
* **orientations:** P16.4 talent search v3 (orientation + skills + mode) ([bd1dd95](https://github.com/Skilluv/skilluv-backend/commit/bd1dd951d237101a4f9772cc7c868ad0e834ca4e))
* **orientations:** P16.5 onboarding playlist (training challenges + open slots by orientation) ([012fb9e](https://github.com/Skilluv/skilluv-backend/commit/012fb9e326024bb8674235215aa52fdf46b00051))
* **p26-v2:** challenge workflow end-to-end + Phase-2 externals + opposabilité + community bundle ([#67](https://github.com/Skilluv/skilluv-backend/issues/67)) ([df25584](https://github.com/Skilluv/skilluv-backend/commit/df25584fb84d01b5869d401360169031951d51ff))
* **p26:** beginner SAS + Dependabot batch (8 bumps) + release-please fix ([#50](https://github.com/Skilluv/skilluv-backend/issues/50)) ([de2d91a](https://github.com/Skilluv/skilluv-backend/commit/de2d91a93baef7caed0ca2ee1c40c32ad32055c2))
* **payout:** P13.1 talent_wallets + transactions ledger avec hash chain ([a5a6807](https://github.com/Skilluv/skilluv-backend/commit/a5a680771b9e1a93afdb3bde9ddf41cd6f38fb52))
* **payout:** P13.2 Stripe Connect Express onboarding + withdraw ([0b52c0d](https://github.com/Skilluv/skilluv-backend/commit/0b52c0ddefd2d605f590a914c8740f244d82312b))
* **payout:** P13.3 MobileMoneyProvider trait + Orange/MTN/Wave stubs ([dfd5f97](https://github.com/Skilluv/skilluv-backend/commit/dfd5f978f180dd035a2182754a83199ddde261b9))
* **payout:** P13.4 bounty dual payout (fragments + wallet fiat) ([1ce4c53](https://github.com/Skilluv/skilluv-backend/commit/1ce4c537a5898c62623dd2757fa3a21dae7e19a2))
* **payout:** P13.5 compliance limites journalieres/mensuelles + statement CSV ([b6d53cf](https://github.com/Skilluv/skilluv-backend/commit/b6d53cf72ea59b5c2167961b936b925c745d8936))
* **post-mvp:** tiers 1-3, staging contract fixes and full admin OpenAPI typing ([#69](https://github.com/Skilluv/skilluv-backend/issues/69)) ([1d12b2e](https://github.com/Skilluv/skilluv-backend/commit/1d12b2e886af683794b6ce47ef2259b6a8a34e85))
* **proof-hooks:** P19.1 orchestrateur central recompute_all_for_user (capabilities+badges+rank) + sweep ([d87e198](https://github.com/Skilluv/skilluv-backend/commit/d87e198786ebb5218a75f584c58df666d4f9e1cd))
* **proof-hooks:** P19.2 wire proof_hooks depuis reviews.submit_verdict + deliverables create paths ([f7cc967](https://github.com/Skilluv/skilluv-backend/commit/f7cc9673471c08338e1397a962914c18d8bd205f))
* **proof-hooks:** P19.3 background sweep task (weekly, env-gated SKILLUV_PROOF_SWEEP_ENABLED) ([72e38b3](https://github.com/Skilluv/skilluv-backend/commit/72e38b33c8ec79799346ecf0c9a851c22a9700a9))
* **proof-hooks:** P19.4 metrics Prometheus granulaires (capabilities/badges/ranks per slug) ([6e44b2d](https://github.com/Skilluv/skilluv-backend/commit/6e44b2db59752a6764ecdb0658ad065a8345ac21))
* **proof-hooks:** P20.1 hook compagnonnage attestation issue (gesture/skill deja couverts via P19.2) ([10506de](https://github.com/Skilluv/skilluv-backend/commit/10506dedb5d930bb765e30c85039dc1fa8746e8e))
* **proof-hooks:** P20.2 hook mentorship session mark_completed (peut auto-promouvoir mentor capability) ([2b54275](https://github.com/Skilluv/skilluv-backend/commit/2b54275522cf3e346a3cb79beb0aed429a2e3ff0))
* **push:** P15.1 mobile push tokens FCM + APNS + auto-push notifs ([c006fee](https://github.com/Skilluv/skilluv-backend/commit/c006fee80cbf327ac2411876e018089788aff792))
* **rls:** P22.1 helper set_tenant_context_on_tx + doc activation prod (opt-in via SKILLUV_RLS_ENFORCED) ([095611a](https://github.com/Skilluv/skilluv-backend/commit/095611a57bcd8f5495cb106c1ea34238cce4004c))
* **security:** consolidate BE-P1-CONTRACT utoipa + CI baseline + admin bugs (BE-P0-41, BE-P2-OPS-DEPLOY, BE-P2-CI-CONTRACT-EXPAND) ([#40](https://github.com/Skilluv/skilluv-backend/issues/40)) ([63d343c](https://github.com/Skilluv/skilluv-backend/commit/63d343cefcee6f07b11ee20bbdf1651bdf0149c1))
* **seed:** SKI-92 seed admin with mandatory password + skill-uv.com default ([#56](https://github.com/Skilluv/skilluv-backend/issues/56)) ([a41839a](https://github.com/Skilluv/skilluv-backend/commit/a41839a54e4a9581cc31aa5a334868ba4f722da6))
* **storage:** split public/private buckets to isolate KYC + GDPR exports ([#32](https://github.com/Skilluv/skilluv-backend/issues/32)) ([cfcbdc0](https://github.com/Skilluv/skilluv-backend/commit/cfcbdc06cb9a8a7e3c92cfbf7878b466bc6d8c3d))
* **teams:** P10.1 teams persistentes + team-claim sur project_slices ([dcac145](https://github.com/Skilluv/skilluv-backend/commit/dcac1455ababd504aabe7a4c34686303c52762fd))
* **teams:** P10.2 role slots multidisciplinaires ([9ad04f1](https://github.com/Skilluv/skilluv-backend/commit/9ad04f1e188a9f62e82b16e261b8a071d4c792fe))
* **teams:** P10.3 team_composition template sur challenge_templates ([8473441](https://github.com/Skilluv/skilluv-backend/commit/8473441736aae6af620085e7d0e08630287a7371))
* **teams:** P10.4 team submit -&gt; deliverable partage + contributors ([9ebc59a](https://github.com/Skilluv/skilluv-backend/commit/9ebc59ac5cb9afec17329386c5282ce4ba745970))
* **teams:** P10.5 bridge Guild &lt;-&gt; Team + bonus GP collectif ([738517a](https://github.com/Skilluv/skilluv-backend/commit/738517a29d8ef4f3716e2c522f1fcd42dbc0de11))
* **teams:** P15.3 marketplace slots + notif skill-matched ([31a41c9](https://github.com/Skilluv/skilluv-backend/commit/31a41c9042c98c7edff54685dc5a42c833ce2c29))
* **tenancy:** P14.1 tenant_id sur tables sensibles + triggers auto-tag ([b67dd25](https://github.com/Skilluv/skilluv-backend/commit/b67dd25d22a5e5c61770ac442f88f5bb73ea88ff))
* **tenancy:** P14.2 RLS POC + set_tenant_context helper ([906f7e7](https://github.com/Skilluv/skilluv-backend/commit/906f7e7be350c9bcbef8bc7df919cc7c78ab2c7a))
* the ops domain, the business model, the tracker, and the AI/audio and design branches folded in ([#82](https://github.com/Skilluv/skilluv-backend/issues/82)) ([0824535](https://github.com/Skilluv/skilluv-backend/commit/08245358c8fc36e8ac8894a505d80c6154d05914))


### Bug Fixes

* **admin+docker:** 6 admin bugs + ship seed binaries in prod image ([#38](https://github.com/Skilluv/skilluv-backend/issues/38)) ([7bd36d5](https://github.com/Skilluv/skilluv-backend/commit/7bd36d50fc3cc5752a6ccd36ecfa407952476391))
* **admin:** align GET /admin/community/review payload on {data, pagination} ([#55](https://github.com/Skilluv/skilluv-backend/issues/55)) ([552cde0](https://github.com/Skilluv/skilluv-backend/commit/552cde0b7d629c992e92f616d4e48e3c320eedc2))
* **api:** contract bugs, silent payouts, and the ledger and notification foundations ([#72](https://github.com/Skilluv/skilluv-backend/issues/72)) ([0bd8bd3](https://github.com/Skilluv/skilluv-backend/commit/0bd8bd315f3c44ba5a702837abd5aa19524b3302))
* **backend:** 20 P0/P1/P2 bugs from front↔back audit + CI hardening ([#34](https://github.com/Skilluv/skilluv-backend/issues/34)) ([ecc43a5](https://github.com/Skilluv/skilluv-backend/commit/ecc43a556234b11a9fc5dd2e20f3ea5568e1dd3b))
* **ci:** bump MinIO tag — 2025-10-15 tag garbage-collected on Docker Hub ([#24](https://github.com/Skilluv/skilluv-backend/issues/24)) ([a87eb9b](https://github.com/Skilluv/skilluv-backend/commit/a87eb9b3e5cf9a21d2ce1f9ef54041c3f10f0439))
* **ci:** Coolify deploy webhook needs Bearer token auth ([#53](https://github.com/Skilluv/skilluv-backend/issues/53)) ([d36edd4](https://github.com/Skilluv/skilluv-backend/commit/d36edd4e3be16d5f0be64aae1fcdd74c00da1c97))
* **ci:** coolify deploy webhook uses POST not GET ([#54](https://github.com/Skilluv/skilluv-backend/issues/54)) ([1c4c5ca](https://github.com/Skilluv/skilluv-backend/commit/1c4c5cac1a3a6526400a26b4fbec161746bd46cd))
* **ci:** hardcode lowercase 'skilluv' org in GHCR image tags ([#37](https://github.com/Skilluv/skilluv-backend/issues/37)) ([d476caf](https://github.com/Skilluv/skilluv-backend/commit/d476caf1eb02eb82a85a95bf077bedd11445718b))
* **ci:** run MinIO as a docker step (fix Integration Tests root cause) ([#25](https://github.com/Skilluv/skilluv-backend/issues/25)) ([f806d02](https://github.com/Skilluv/skilluv-backend/commit/f806d026c12c66e9f6331297ef7419b54123b380))
* **ci:** sign and attest the image under the name it is published with ([#71](https://github.com/Skilluv/skilluv-backend/issues/71)) ([e6b99bf](https://github.com/Skilluv/skilluv-backend/commit/e6b99bfeff5b95d73922d85698bf56f17af2784f))
* **ci:** stop the four test shards from racing to save the same cache ([#70](https://github.com/Skilluv/skilluv-backend/issues/70)) ([1d9d7fc](https://github.com/Skilluv/skilluv-backend/commit/1d9d7fc63b933c6ca33e63befecd8fdafb16599a))
* **migrations:** restore 0068 checksum + conditional healthcheck for non-HTTP binaries ([#68](https://github.com/Skilluv/skilluv-backend/issues/68)) ([87cf06f](https://github.com/Skilluv/skilluv-backend/commit/87cf06fecc91066dd2c65d9d17ae2d6ca7811b7c))
* **routes:** resout conflit routes /seasons entre tournament.rs et seasons.rs ([2dfc4e1](https://github.com/Skilluv/skilluv-backend/commit/2dfc4e1230282a3a6848f638829791c118ff9540))
* **tests:** elimine les flakies en parallele ([a71be79](https://github.com/Skilluv/skilluv-backend/commit/a71be799b787789748f5564ee51dcb816a03437c))
* **tests:** P13.2 + P13.5 env mutation guarded by Mutex ([5ee97ca](https://github.com/Skilluv/skilluv-backend/commit/5ee97ca8cf8a24d7b717811f336f81271c5d7881))

### In detail

The list above is generated from the commit messages and is the index.
This is the account: what changed, and why it was done that way. It was
written as the work happened, under `[Unreleased]`, and everything in it
shipped in 0.1.0 — so it belongs here rather than above a release it is
no longer waiting for.


Target model + P10-P15 (teams multi-role, GitHub ingestion, discovery,
real-money payouts, multi-tenancy + anti-fraud, mobile push +
AI-native verifier + team marketplace) all in place. The P10-P15
roadmap in `docs/roadmap-p10-p15.md` is closed; next iteration will
address KYC full, live AI wiring in prod, and RLS enforcement.

#### Added

- **An accusation the accused can answer.** `plagiarism_cases`, deliberately
  not a `reports` row: a report has nowhere for the accused to reply, and the
  reply is the substance of a procedure whose outcome is a disqualification, a
  confiscated prize and a public record. Eighty characters minimum both ways --
  on the accusation and on the decision -- and an evidence link is required,
  because an accusation with nothing to compare against cannot be checked by
  anybody, the reviewer included.
- **Erasure leaves a tombstone.** `DELETE FROM users` took the contest entries,
  the podium places and the attestations with it, destroying more than the
  person asked for and other people's records besides: a contest whose second
  place vanished leaves first and third unexplained. `erasure::erase` deletes
  every row wholly about the person and empties the `users` row instead. The
  table list is checked against the schema before anything is deleted, and
  every statement after that is fatal -- half an erasure is worse than none.
- **The migrations get their own CI job.** `scripts/check-migrations.sh`
  applies every migration to an empty database in order, then asserts the
  invariants the schema is supposed to hold; the workflow runs it as
  `Migrations Apply And The Schema Holds`, and the eight test shards now wait
  on it. A chain that does not apply used to make all eight shards red with
  the same unrelated error forty-five minutes later. This names the migration
  and the line in about two, and spends one runner doing it.
- **A check that every capability a route guards itself with can be granted.**
  `scripts/capabilities-named-in-code.py` reads the names out of the
  `require_capability` call sites — by balanced parentheses, so a literal that
  merely sits nearby is not mistaken for one passed in — and the job refuses
  any the catalogue has no row for. This is not a misconfiguration that
  degrades: the grant is refused, so the guard refuses everybody, silently and
  forever. It found `mission_arbiter`.
- **Answering a wizard comes back with advice.** `PUT
  /api/users/me/domain-profile/{domain}` carries a `recommendation`: a
  headline, the reasoning behind it so it can be argued with, guides to open
  and a ready-made query against the domain's feed. Absent from the reads,
  deliberately -- it answers having just answered, and a profile that carried
  it would invite a front end to show month-one advice to somebody in their
  sixth month.
- **The wizards' declared handles become portfolio rows in every domain.**
  Previously audio only. A GitHub username given to the code wizard and a
  HuggingFace one given to the AI wizard were both stored and read by nothing.
  Migration 0441 adds the `huggingface` platform, without which the insert
  would have been logged and dropped. Claimed, never proved — only the OAuth
  callback sets `verified_at`.

- **Audio — a domain of its own.** Five trades (`audio-composer`,
  `audio-music-implementer`, `audio-sound-designer`, `audio-voice-actor`,
  `audio-programmer`) in four review families, 61 skill nodes, 24 seeded
  challenges, 13 badges, 5 review grids, 7 mission types, 7 attestation
  bases and a craft score. Migrations 0400-0421. The one legacy trade,
  `game-sound-engineer`, is archived with a `replaced_by` lineage.
- **Audio deliveries are files, and the files are measured.**
  `audio_artifact_files` holds a master, its stems, a generated preview and
  the project archive, with duration, sample rate, bit depth, channels,
  integrated loudness, true peak and loudness range read out of the file by
  `ffprobe`/`ffmpeg` — never taken from the uploader. Both tools are
  optional: where they are absent, files are accepted and served and the
  measurements stay NULL with `analysis_status = 'skipped'`.
- **Source declarations gate the attestations that need them.**
  `audio_source_licences` plus `project_slices.audio_sources_declared_at`.
  A composition or a sound pack is not attested until the author states the
  source list is complete — an empty list with a declaration means "all
  original", an empty list without one means nobody filled the form in.
- **Voice castings, blind by default.** `voice_castings` and
  `voice_audition_submissions`: one brief, the same lines for everybody, one
  choice at the end, and names withheld until it is made.
- **Revision rounds, for every domain rather than for audio.**
  `slice_revision_rounds` with a per-domain limit enforced in the database,
  because the count is the one both sides quote. Audio: five.
- **`mission_licensing_scopes`** — what the client may do with the work,
  orthogonal to `ip_terms` which says who owns it. Required on audio
  missions, optional elsewhere. Exactly one scope (`buyout`) takes the
  creator's portfolio away, and it says so.

#### Changed

- **Six dependency bumps, folded in rather than merged separately.** rand_core
  0.6 to 0.10, jsonwebtoken 10 to 11, zip 3 to 8, and three GitHub Actions.
  Only rand_core needed code: 0.10 removed `OsRng` from the crate root, and the
  three call sites now use `getrandom::fill` -- the same OS entropy `OsRng`
  wrapped, and what the rest of this codebase already reached for a few lines
  away. One of the three was dead code kept only to silence a warning about a
  dependency that has since become direct.
- **One onboarding wizard, and the code wizard folded into it.** Migration
  0258 moves the eight `users.code_*` columns into `user_domain_profiles` and
  drops them; `POST /api/code/onboarding` and `/code/onboarding/skip` are gone,
  answered by `PUT /api/users/me/domain-profile/code` like every other domain.
  Two questions are renamed on the way: `objective` is `goal` and
  `main_languages` is `main_tools`, because the generic wizard already called
  them that and two names for one question is how a reader ends up checking
  the wrong key. `main_tools` is the one tools question with no vocabulary —
  the set of things a developer works in is not a list this platform owns.
- **`POST /api/users/me/domain-profile/{domain}/skip` answers 204.** It used
  to answer 200 with a body it made up: the write touches `skipped_at` alone,
  so the body reported no answers and no completion whatever the row held, and
  told somebody who had answered and then skipped that they never answered.
- **Migration 0306 is empty.** It did what 0258 does, written on another branch
  at the same time and numbered after it, so it failed the chain on the first
  database that saw both. The file stays with the reason in it rather than
  being deleted, because a gap explained reads better than a number missing.

- **The lists that could only be rewritten became rows.** Migrations 0228 and
  0305 documented the failure mode — a CHECK cannot be extended, only
  replaced, so every addition is a chance to silently delete somebody else's
  value, and 0223 had already deleted two of 0189's. Five of those lists are
  now tables with foreign keys: `skill_domains` (0400, replacing ten CHECK
  constraints and closing ten columns that had none), `capability_catalog`
  (0404, with the reviewer capabilities derived from `orientations` by
  trigger), `attestation_bases` (0406, carrying `requires_deliverable` and the
  wording each basis is issued with), `slice_types` (0408),
  `mission_deliverable_formats` (0413) and `tournament_kinds` (0416, carrying
  what three separate Rust constants held).
- **`user_code_portfolios` is now `user_external_portfolios`**, with a
  `portfolio_platforms` catalogue keyed by domain and a `figures_are_declared`
  flag. A SoundCloud account is the same row, the same verification problem
  and the same staleness problem as a GitHub one. Declared audience figures
  are accepted, marked, and counted at half weight.
- **The domain wizard asks questions from a registry** rather than from a
  struct field per question, and publishes them at
  `GET /api/users/me/domain-profile/{domain}/questions` so a front end renders
  the form instead of shipping its own copy of the list. The wire format is
  unchanged.
- **`validators::SKILL_DOMAINS` is the only domain list in Rust.** There were
  eight, three of them stale — `ai` was accepted by the skill tree and refused
  by the explore filter for a year, and the leaderboard had offered four
  domains since 2024. A test asserts the constant against the table.
- **`craft_score::assemble`** extracted when audio became the third domain to
  score: everything after the measuring is identical, and three copies is
  three places for the cap or the skip-on-zero to drift.


- **P25.3** — `middleware::capabilities::require_any_capability(db,
  user_id, &[caps])` — passes if any listed capability is active (not
  revoked, not expired). Empty list rejects (defense in depth). New
  doc `docs/MODERATION-vs-ADMIN.md` maps every capability to its
  authorized front (`skilluv-frontend` vs `skilluv-admin`), lists what
  moderators can NOT do, and shows wiring examples for
  `require_any_capability` on shared moderation endpoints.
- **P25.2** — `capabilities_engine` auto-promotes 3 community
  moderator caps: `community_curator` at ≥3 published community
  `challenge_templates` (co-granted with `issue_proposer`),
  `forum_moderator` at ≥20 forum `posts` authored, and the umbrella
  `community_moderator` auto-granted whenever any sub-cap
  (`forum_moderator` / `plagiarism_reviewer` / `kyc_reviewer` /
  `community_curator`) is active. `plagiarism_reviewer` and
  `kyc_reviewer` remain manual-only (nomination by admin) — those
  actions touch users' economic life so no threshold auto-promotion.
- **P25.1** — Migration 0098 extends the `user_capabilities`
  capability CHECK to include 5 new values: `community_moderator`
  (umbrella), `forum_moderator`, `plagiarism_reviewer`,
  `kyc_reviewer`, `community_curator`. These caps unlock inline
  moderation UI on `skilluv-frontend` without ever granting access to
  the `skilluv-admin` staff panel — `admin` remains a distinct
  capability strictly reserved for Skilluv HQ staff.
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
  contribution_ref)`. `GET /api/events` (active only), `POST
  /api/events/{slug}/join` (idempotent), `GET /api/users/me/events`.
  (This entry originally said the routes were namespaced as
  `/badge-events` to avoid a collision with the tournament `/events`.
  They never were — the collision was real and was resolved instead by
  moving the tournament route to `/api/tournaments/feed`, which is what
  had been making the router panic at startup.) Wires up Skilluv Fest / Hacktoberfest /
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

#### Changed

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

#### Removed

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

#### Fixed

- **Searching for `C++` was a server error.** `forum::search_posts` and
  `admin_moderation::list_users` handed user input to `to_tsquery`, which
  parses its argument as a query expression — so `&`, `|`, `!`, `(`, `)` and
  `:` reached the parser as syntax and raised `syntax error in tsquery`.
  `R&D`, `(brouillon)` and `design:system` were all 500s. Both now use
  `websearch_to_tsquery`, which never raises on its input and reads quoted
  phrases and a leading `-` the way somebody typing into a search box expects.
- **A cancelled mission never gave the escrow back.** `missions::set_status`
  released the escrow on `closed` and did nothing on `cancelled`, so a mission
  cancelled from `in_progress` with a paid invoice left the talent's share in
  `pending` for ever — no path could release it, no path could return it, and
  nothing counted it. `mission_billing::refund_all` mirrors `release_all`:
  provider first then the books, the commission back with the rest, and only
  what is still pending. Money already released stays released; that is what
  the dispute machinery is for.
- **Arbitration documented money it never moved.** `POST
  /api/admin/missions/{slug}/arbitrate` said "the delivery stands and the money
  is released" and "the escrow goes back" while writing the status with a raw
  UPDATE that bypassed the one function where both live. It also stopped at
  `delivered`, which waits on the client accepting delivery — the act
  arbitration exists because the client refused. It now goes through
  `set_status` to `closed`, and `delivered -> cancelled` is a legal transition.
  No arbitration test had ever put an invoice on a mission, which is why an
  endpoint that moved nothing passed.
- **An upheld plagiarism case never took the prize back.** `contest_prizes`
  awards into `pending` rather than `available` and says why: "the release
  window is what makes a contested result recoverable". Nothing ever recovered
  one, so a contest could hold a winner who was both disqualified and paid.
  `confiscate` returns the awarded amount to the contest escrow — not to the
  sponsor and not to the runner-up, because both are decisions that do not
  belong in a function nobody is reading.

- **fix(ops)** — `GET /users/{username}/ops-profile` answered 500 to every
  request it had ever received. Twelve of its thirteen figures are `count(*)`,
  which PostgreSQL returns as bigint, and the struct reads all thirteen as
  `i64`; the thirteenth went through `date_part` and was cast `::INT`, making
  it the one int4 in the row. sqlx does not widen an integer to fit, so it
  refused to decode the row. The four sibling services — AI, audio, design,
  code — all cast `::BIGINT`; ops was the only one written otherwise.
- **fix(enterprise)** — `/api/enterprises/me/agency-clients` and
  `/api/enterprises/me/type-config` ordered by `enterprise_members.created_at`,
  a column that does not exist: the table records when somebody was invited
  and when they accepted. Both endpoints 500'd on every call. The function is
  a second copy of `enterprise::resolve_active_enterprise` that had drifted;
  realigning it also restored the `status = 'active'` filter it had lost,
  without which a pending invitation outranked a membership somebody holds.
- **fix(accounting)** — `/api/admin/accounting/export` selected
  `enterprises.country`, which does not exist either. The country lives on the
  invoice, which is the correct source: VAT follows where the customer was
  when they were billed, so a company that has since moved must not
  retroactively change last year's return.
- **fix(payments, sandbox)** — An unconfigured Stripe and an unreachable
  Judge0 were reported as internal errors. That tells a caller the server
  failed when the honest answer is that the integration was never set up on
  this deployment, or is not running. Both answer 503 now. For Judge0 the
  distinction is drawn on the `reqwest` error: a transport failure is
  unavailability, a response we received and disliked stays a 500, because
  then the request itself is the suspect.
- **fix(search)** — `capability` and `skills` are closed vocabularies and were
  accepted as free text, so a filter that could match nothing answered 200
  with an empty list — telling a recruiter that nobody holds the capability
  rather than that what they typed is not one. Both are checked as shapes now,
  deliberately not against the catalogue: a capability that exists and that
  nobody holds must still return an empty list, because that is true.
- **fix(migrations)** — `domain_curator:all` was inserted with a NULL scope
  against a CHECK requiring `family || ':' || scope`. The insert was refused,
  which failed the migration chain, which meant the backend never started —
  one row costing an entire CI run.

#### Added

- **`scripts/check-migrations.sh`** — applies every migration to a throwaway
  database and asserts what the schema should hold. Fifteen seconds against a
  local PostgreSQL; no Docker, no test suite. A migration is checked by
  nothing else — not `cargo check`, not clippy, not a unit test — so a bad row
  does not fail one test, it fails the chain and every shard with it.
- **`tests/test_read_endpoints_answer.rs`** — calls the 192 GET routes that no
  other test reaches, as a stranger, a member and an admin. An audit found 404
  of 922 registered routes called by no test at all; this covers every GET
  among them, the parameterised ones with an id nothing owns — a handler
  naming a column that does not exist fails when the query runs, not when a
  row matches. It found three dead endpoints on its first run. It asserts the
  shape of the answer rather than its content: 401, 403, 404 and 503 are all
  correct, and a 500 to a well-formed request is what a query that has stopped
  decoding produces.

#### Changed

- **ci** — Eight test shards instead of four, and a 45-minute budget instead
  of 35. At four, three shards out of four were being killed by the timeout,
  so a run reported a quarter of its failures and lost the rest. A shard is
  ~12 min of compilation plus ~16 min of tests and only the second half
  divides, so eight shards is ~20 min rather than half of 28 — the gain that
  matters is that results survive to be read.

#### Fixed

- **fix(goals)** — A capability goal could not be created at all. The target
  was validated by pattern-matching the *text* of
  `user_capabilities_capability_check`, and migration 0404 made the
  capabilities rows and dropped that constraint, so the lookup answered false
  for every capability that exists — and said the capability was unknown.
  Reads `capability_catalog` now.
- **fix(feed)** — The public feed's keyset cursor was `<rfc3339>|<uuid>`, and
  an RFC3339 offset carries a `+`, which a query string decodes as a space.
  The only thing anybody does with `next_cursor` is put it in a URL, so the
  second page answered 400 to a cursor the server had just issued. base64url
  now, which also makes it opaque again.
- **fix(credentials)** — `declare` read its own insert through a
  data-modifying CTE. Every part of a statement sees one snapshot, so the
  SELECT over `credentials_with_currency` found nothing and the endpoint 500'd
  on a credential it had in fact written.
- **fix(ops)** — An incident could be opened with a start date in the future.
  Resolving stamps `NOW()`, which then lands before the start and trips
  `an_incident_runs_forward`: a 500 at the end of an outage, on the step meant
  to close it.
- **fix(api)** — A NUL byte in any JSON string reached PostgreSQL, which
  cannot hold one in a text column at all, and surfaced as a 500 blaming the
  server for input no text column anywhere will ever accept. Refused at the
  edge with a 400. A middleware rather than a check per field: every endpoint
  taking free text has the same exposure.
- **fix(missions)** — `GET /api/missions` declared maximum lengths on seven
  filters and enforced none of them, answering 200 with an empty list to a
  malformed query.
- **fix(schema)** — Migration 0440 puts a foreign key on the seven domain
  columns that were still free strings — `academy_cohorts`, `consultations`,
  `external_resources`, `featured_talents`, `marketplace_items`,
  `mentoring_programs`, `tournament_series`. A typo in one of them inserted
  cleanly and was invisible to every listing that joins the catalogue.
- **fix(migrations)** — Three CHECK-to-table conversions had dropped another
  branch's values. 0408 listed the two slice types 0231 had already folded
  into `design_artifact`; 0416 omitted the `duel` and `brief_contest` formats
  from 0235; 0431 re-added a deliverable-format CHECK holding only the code
  and ops values after 0413 had replaced it with a table, which would have
  refused every AI, audio and design mission.
- **fix(wizard)** — The design domain's own two questions,
  `challenge_preference` and `main_tool`, were never carried into the shared
  question registry, so `PUT /domain-profile/design` refused its own fields.
  A field sent to the wrong wizard now names the domain that owns it, and an
  empty `preferred_families` is stored rather than dropped — "no family in
  particular" is an answer somebody gave, and never opening the wizard is not.
- **fix(ci)** — Integration test shards ran the runner out of disk while
  linking: 184 test binaries each statically linking the whole dependency
  graph, with full DWARF. Three shards died with ENOSPC and the fourth with a
  linker SIGBUS, which is the same full disk seen through an mmap.
  `debug = "line-tables-only"` on the dev and test profiles, and the job now
  reclaims the preinstalled SDKs it never uses.

- **fix(openapi)** — Every operation and every schema now has a name of its
  own. utoipa derives `operationId` from the handler's function name and a
  component name from the Rust type alone, and neither was unique: 126
  handlers collapsed onto 56 operation ids and 51 structs onto 18 component
  names, each collision quietly overwriting the last. `POST
  /api/legal/consent` documented the data-line consent body — a fuzzer sent
  the documented `{"agree": true}` and the endpoint refused it. Two unit
  tests in `src/openapi.rs` now fail on the next collision.
- **fix(openapi)** — `/api/admin/ai/churn` and `/api/admin/ai/hidden-gems`
  were documented at paths the router never served; the handlers are at
  `/api/admin/assistant/…`. A documented path nothing routes answers 404, and
  404 is an allowed answer to nearly every contract check, so the gap was
  invisible from the outside.
- **fix(openapi)** — `EcosystemRow.community_links` and `notable_events` are
  JSONB arrays declared as `object`, so a generated client could not iterate
  what it was handed. Both now have real item schemas.
- **fix(openapi)** — Query parameters that the handler validated against a
  closed vocabulary said `string` in the contract: `looking_for` on the
  talent search and `urgency` on the mission board are enums, and the
  keyset cursors on `/api/feed/public` and `/api/talents/search` carry the
  pattern they are actually parsed with. A schema-compliant request is now
  one the API accepts.
- **fix(routes)** — Removed the `my_mentions` handler in `routes/social.rs`,
  left behind when SKI-293 unregistered `/api/social/mentions/me`.
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

### P5 — Attestations * LAUNCH (`2bacfd1`)

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
