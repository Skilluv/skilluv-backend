-- The communication domain opens, with five trades.
--
-- ## Why communication is a domain and not a corner of `soft_skills`
--
-- The catalogue held exactly one of these trades — `tech-writer`, seeded in
-- 0088 — and it was filed under `soft_skills`, next to `code-review` and
-- `giving-feedback`. That said something false about the work. Writing an API
-- reference somebody can build against, giving a conference talk, running a
-- YouTube channel that teaches a framework, translating a manual into a
-- language its authors do not speak: these are trades people are hired into,
-- with their own tools, their own review criteria and their own portfolios.
-- `soft_skills` is the file for what a developer needs *besides* their trade,
-- and filing a technical writer there tells them the platform thinks writing
-- is something a developer does on the side.
--
-- The move is the one 0401 made for audio, for the same reason: the field is
-- defined by what it produces rather than by who it helps.
--
-- ## `tech-writer` moves, it is not copied
--
-- 0402 refused to duplicate `game-audio` under `audio` and 0209 refused to
-- duplicate `python` under `ai`. The reason applies exactly: two rows for one
-- trade give two answers to whether somebody holds it, and both get read.
-- The row keeps its id, so every `user_orientations` row, every gated slice
-- and every skill-map edge pointing at it survives the move untouched.
--
-- `open-source-maintainer` stays in `soft_skills`. It is the other legacy row
-- the backlog listed as a communication candidate, and it is not one: a
-- maintainer's job is triage, review and release management. It belongs to
-- the leadership split, and moving it here to make a list longer would put a
-- trade under a review family that cannot judge it.
--
-- ## Four review families, five trades
--
-- The backlog asked for one review capability per trade. 0176 settled why
-- that is wrong — review rights are granted by family, because nobody
-- reviews at trade granularity — and the families here are drawn by what a
-- reviewer has to be able to do:
--
--   * `documentation` — read as a stranger with a task, and say whether the
--     task got done. Judged on clarity, completeness and whether the examples
--     run.
--   * `advocacy` — the developer advocate and the tech content creator. Both
--     produce explanatory media for an audience that can leave, and both are
--     judged on whether that audience understood and stayed. A conference
--     talk and a fifteen-minute tutorial differ in production, not in what
--     makes them good. One family, two trades — the same call 0401 made for
--     the music implementer and the audio programmer.
--   * `translation` — needs somebody who reads both languages well enough to
--     hear a false friend. That is not a degree of the documentation family,
--     it is a different competence, and a reviewer who has it for French has
--     not got it for Swahili.
--   * `research-writing` — judged on rigour, citations and whether the claim
--     survives the method. A good documentation reviewer is not automatically
--     able to tell a sound benchmark from a marketing one.
--
-- ## Every trade points outside the domain
--
-- Like audio, and for the same reason: a communication artefact is *about*
-- something. A writer who cannot read the code they document writes prose
-- around an API rather than about it, and `secondary_domains` is where that
-- is written down rather than assumed.

UPDATE skill_domains
   SET is_active = TRUE, updated_at = NOW()
 WHERE slug = 'communication';

-- ═══════════════════════════════════════════════════════════════════
-- The legacy trade moves
-- ═══════════════════════════════════════════════════════════════════

UPDATE orientations
   SET primary_domain = 'communication',
       reviewer_group = 'documentation',
       name = 'Technical writer',
       description = 'Writing what a stranger has to read in order to use '
                     'something: documentation, tutorials, API references, '
                     'release notes. The reader arrived with a task, and the '
                     'page succeeded when the task got done.',
       secondary_domains = ARRAY['code', 'ops', 'design'],
       tags = ARRAY['documentation', 'tutorial', 'api', 'writing'],
       is_curated = TRUE,
       updated_at = NOW()
 WHERE slug = 'tech-writer';

-- ═══════════════════════════════════════════════════════════════════
-- The four that are new
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO orientations
    (slug, name, description, primary_domain, secondary_domains, tags, is_curated, reviewer_group)
VALUES

('developer-advocate', 'Developer advocate',
 'Making a technology exist in front of people: conference talks, live '
 'demonstrations, workshops, presence in a community. The trade is turning a '
 'technical decision into something a room remembers.',
 'communication', ARRAY['code', 'ai', 'ops'],
 ARRAY['devrel', 'conference', 'demo', 'community'], TRUE, 'advocacy'),

('content-creator-tech', 'Technical content creator',
 'Video, articles, streams, podcasts. An audience that can leave at any '
 'moment, and stays when what is being explained is worth its time.',
 'communication', ARRAY['code', 'game', 'design'],
 ARRAY['video', 'blog', 'podcast', 'live'], TRUE, 'advocacy'),

('technical-translator', 'Technical translator',
 'Making documentation usable in a language its authors do not speak. Not '
 'transposing sentences: holding one vocabulary steady across thousands of '
 'lines, and knowing what must not be translated at all.',
 'communication', ARRAY['code', 'ops'],
 ARRAY['translation', 'i18n', 'localisation', 'terminology'], TRUE, 'translation'),

('research-writer-tech', 'Technical research writer',
 'Whitepapers, industry reports, external specifications. A text whose value '
 'rests on its method: what was measured, how, and what it does not prove.',
 'communication', ARRAY['ai', 'security', 'code'],
 ARRAY['whitepaper', 'research', 'rfc', 'report'], TRUE, 'research-writing');

-- ═══════════════════════════════════════════════════════════════════
-- Running the domain
-- ═══════════════════════════════════════════════════════════════════
--
-- The reviewer capabilities are derived by the trigger of 0404 from the rows
-- above. The curator one is not derived — it is about a domain rather than
-- about a family of trades — so it is written here, as 0404 wrote the other
-- eight.

INSERT INTO capability_catalog (capability, family, scope, description) VALUES
    ('domain_curator:communication', 'domain_curator', 'communication',
     'Runs the communication domain: its challenges, its contests, its featurings.')
ON CONFLICT (capability) DO NOTHING;
