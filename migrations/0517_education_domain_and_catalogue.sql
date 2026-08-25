-- The education domain opens, with three trades.
--
-- ## Why education is a domain and not a corner of `soft_skills`
--
-- The catalogue held nothing for it. Teaching appeared only as
-- `mentoring-junior` and `technical-1on1`, two skill nodes filed under
-- `soft_skills` as things a developer does alongside their own work.
--
-- That is mentoring, and mentoring is not teaching. A mentor takes one person
-- as they come and follows them for months; a trainer takes twenty people
-- with a stated starting point, moves them through a structure decided in
-- advance, and is answerable for whether they arrived. The second is a
-- profession with its own tools, its own failure modes and its own hiring
-- market, and it is invisible on a platform that only knows the first.
--
-- The distinction is written down here rather than left implicit, because it
-- is the one this domain will be argued about: `leadership.lead-mentor`, when
-- that domain opens, is the continuous one-to-one relationship. These three
-- are structured delivery to a group.
--
-- ## Three trades, and no more
--
-- The backlog said "compact — no inflation", and it was right to. Five would
-- have meant splitting the trainer by audience (corporate, bootcamp,
-- community), which describes a client rather than a craft: the same person
-- does all three in a year, with the same skills.
--
-- ## Two review families, three trades
--
-- Migration 0176 settled that review rights are granted by family, because
-- nobody reviews at trade granularity. The line here is what the reviewer has
-- to be able to do:
--
--   * `teaching` — the technical trainer and the coding teacher. Both deliver
--     to a room and are judged on whether the room moved: engagement, clarity
--     under a question nobody prepared for, and outcomes that were measured
--     rather than felt. A workshop and a semester differ in length, not in
--     what makes them good.
--   * `curriculum` — read as a document, not watched. Judged on whether the
--     objectives are stated, whether the progression holds, and whether the
--     assessment measures what the objectives claimed. A very good trainer is
--     not automatically able to tell a sound learning path from a plausible
--     list of topics.
--
-- ## Why all three point outside the domain
--
-- Teaching is teaching *something*. A trainer who cannot do the thing they
-- teach is running a slide deck, and `secondary_domains` is where that is
-- written down rather than assumed.

UPDATE skill_domains
   SET is_active = TRUE, updated_at = NOW()
 WHERE slug = 'education';

INSERT INTO orientations
    (slug, name, description, primary_domain, secondary_domains, tags, is_curated, reviewer_group)
VALUES

('technical-trainer', 'Technical trainer',
 'Delivering to a group that arrived with a stated starting point and has to '
 'leave somewhere else: workshops, cohorts, corporate training. Answerable '
 'for whether they got there, not for whether the slides were good.',
 'education', ARRAY['code', 'ops', 'security'],
 ARRAY['workshop', 'cohort', 'training', 'delivery'], TRUE, 'teaching'),

('coding-teacher', 'Coding teacher',
 'Teaching people to program, day after day, usually beginners. The trade '
 'where the hard part is not the subject: it is watching somebody be stuck '
 'and knowing which of the four possible reasons it is.',
 'education', ARRAY['code', 'design'],
 ARRAY['beginners', 'school', 'bootcamp', 'pedagogy'], TRUE, 'teaching'),

('curriculum-designer', 'Curriculum designer',
 'Deciding what is learned, in what order, and how anybody knows it worked. '
 'Learning paths, skill matrices, assessment frameworks — read as documents '
 'and judged on whether the progression holds.',
 'education', ARRAY['code', 'ai', 'design'],
 ARRAY['curriculum', 'learning-path', 'assessment', 'instructional-design'],
 TRUE, 'curriculum');

-- ═══════════════════════════════════════════════════════════════════
-- Running the domain
-- ═══════════════════════════════════════════════════════════════════
--
-- The reviewer capabilities are derived by the trigger of 0404 from the rows
-- above. The curator one is not derived — it is about a domain rather than
-- about a family of trades — so it is written here, as 0404 and 0500 wrote
-- the others.

INSERT INTO capability_catalog (capability, family, scope, description) VALUES
    ('domain_curator:education', 'domain_curator', 'education',
     'Runs the education domain: its challenges, its contests, its featurings.')
ON CONFLICT (capability) DO NOTHING;
