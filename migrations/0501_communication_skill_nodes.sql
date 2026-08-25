-- The skills the communication trades are made of.
--
-- ## What the catalogue held before this
--
-- `written-communication` and its five children, seeded in 0057 under
-- `soft_skills`, plus `technical-writing` filed as one leaf among them. That
-- is the whole of a five-trade domain: enough to say a developer writes
-- decent pull-request descriptions, not enough to describe a single one of
-- the trades opened in 0500.
--
-- ## What moves, and what stays
--
-- Four nodes move: `technical-writing`, `readme-authoring`, `adr-writing` and
-- the `written-communication` root that holds them. They describe the craft
-- of writing something a stranger reads, which is this domain.
--
-- `commit-message-quality`, `pr-description-quality`, `bug-report-quality`,
-- `async-communication` and `stakeholder-communication` stay in
-- `soft_skills`. They are things a practitioner of any trade does inside
-- their own work, and moving them would make every developer a partial
-- technical writer — which is the confusion 0500 exists to end.
--
-- Moving keeps the ids, so everything anybody already proved against them
-- survives.
--
-- ## Naming
--
-- Each node names a technique, a format or a tool, never a level. "Good
-- writer" is a label nobody can claim honestly; writing an API reference from
-- a signature is something a person has either done or not.
--
-- ## Where the tree deliberately stays shallow
--
-- Two levels, like the rest of the catalogue.

-- ═══════════════════════════════════════════════════════════════════
-- The four that move
-- ═══════════════════════════════════════════════════════════════════
--
-- `display_category` is set explicitly: the trigger of 0116 only fires on
-- INSERT, so a row that changes domain keeps the category of the domain it
-- left unless somebody says otherwise. Both domains happen to read as
-- `share`, which makes the omission invisible rather than harmless.

UPDATE skill_nodes
   SET domain = 'communication',
       display_category = skill_nodes_default_display_category('communication'),
       display_name = 'Writing for a reader you do not know',
       description = 'The gesture the whole domain shares: writing for somebody whose knowledge you cannot assume and whose reason for arriving you do not know.',
       updated_at = NOW()
 WHERE slug = 'written-communication';

UPDATE skill_nodes
   SET domain = 'communication',
       display_category = skill_nodes_default_display_category('communication'),
       updated_at = NOW()
 WHERE slug IN ('technical-writing', 'readme-authoring', 'adr-writing');

-- ═══════════════════════════════════════════════════════════════════
-- Roots
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain) VALUES
('documentation-craft', 'Documentation',
 'What makes a page usable: its structure, its example, and whether it answers the question the reader actually had.',
 'communication'),
('public-speaking-tech', 'Speaking publicly about technical work',
 'Holding a room or a camera on a subject that does not tell itself.',
 'communication'),
('content-production', 'Content production',
 'What has to surround the point for it to be watchable: picture, sound, editing, pace.',
 'communication'),
('localisation-craft', 'Translation and localisation',
 'Making a technical text correct in another language, and consistent with itself over its whole length.',
 'communication'),
('research-writing-craft', 'Research writing',
 'Writing something whose value rests on its method: what was measured, how, and what it does not prove.',
 'communication')
  ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Documentation
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT v.slug, v.display_name, v.description, 'communication', p.id
  FROM (VALUES
    ('docs-information-architecture', 'Documentation information architecture',
     'Deciding what is a tutorial, a guide, a reference and an explanation — and not mixing the four on one page.'),
    ('tutorial-writing', 'Writing a tutorial that works',
     'A path from the first prerequisite to a visible result, replayed on a clean machine, where no step assumes knowledge that was never announced.'),
    ('api-reference-writing', 'Writing an API reference',
     'Starting from the signatures and making every parameter, return and error usable without reading the source.'),
    ('runnable-examples', 'Runnable examples',
     'A snippet somebody can copy, paste and run. An example that does not compile costs more than no example.'),
    ('changelog-and-migration-guides', 'Changelogs and migration guides',
     'Saying what breaks, for whom, and what to do about it. An undocumented break is a break twice over.'),
    ('docs-as-code', 'Docs as code',
     'Documentation versioned with the code, reviewed in review, published by the pipeline.'),
    ('docs-linting', 'Automated prose checking',
     'Vale, textlint, link checkers: whatever can be verified without a human reader should be, before one has to.'),
    ('audience-analysis', 'Reader analysis',
     'Knowing who you are writing for: what they already know, what they came for, and when they give up.'),
    ('editing-and-revision', 'Editing and cutting',
     'Removing what does not help. The least spectacular skill and the most visible one.')
  ) AS v(slug, display_name, description)
  CROSS JOIN (SELECT id FROM skill_nodes WHERE slug = 'documentation-craft') p
  ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Speaking
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT v.slug, v.display_name, v.description, 'communication', p.id
  FROM (VALUES
    ('talk-structure', 'Building a talk',
     'A promise at the start, a demonstration in the middle, one thing to take away at the end.'),
    ('conference-cfp-writing', 'Writing a conference proposal',
     'Two hundred words telling a committee why this room, this year, on this subject.'),
    ('live-demo-resilience', 'A live demonstration that survives',
     'Preparing for the moment the connection dies: pinned environment, fallback path, recording of last resort.'),
    ('slide-craft', 'Making slides people can read',
     'One idea per slide, code legible from the back of the room, no text read aloud.'),
    ('community-engagement', 'Holding a community',
     'Answering, redirecting, defusing. The invisible work that decides whether people come back.'),
    ('workshop-facilitation-tech', 'Running a technical workshop',
     'Making a room work rather than listen: environment prepared in advance, pace held, help given without taking over.'),
    ('developer-empathy', 'Hearing what developers are actually saying',
     'Carrying a disagreement back to a product team without smoothing it over or amplifying it.')
  ) AS v(slug, display_name, description)
  CROSS JOIN (SELECT id FROM skill_nodes WHERE slug = 'public-speaking-tech') p
  ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Content production
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT v.slug, v.display_name, v.description, 'communication', p.id
  FROM (VALUES
    ('video-scripting', 'Writing a video script',
     'What is said, in what order, and what the picture is showing while it is said.'),
    ('screen-recording-quality', 'Clean screen capture',
     'Resolution, font size, visible cursor, a terminal legible on a phone.'),
    ('video-editing-basics', 'Editing',
     'Cutting the silences, holding the pace, not letting a ten-second operation take two minutes.'),
    ('audio-for-talking-head', 'Voice recording quality',
     'A placed microphone, a treated room, a steady level. The first reason anybody closes a video.'),
    ('thumbnail-and-title', 'Title and thumbnail',
     'Saying what is inside without lying. A title that promises more than the video costs you the next audience.'),
    ('livestream-operations', 'Running a live stream',
     'Scenes, alerts, moderation, and a plan for when screen sharing dies.'),
    ('podcast-interviewing', 'Conducting an interview',
     'Preparing, listening to the answer rather than to your next question, and letting the silence work.'),
    ('content-series-planning', 'Sustaining a series',
     'A thread that connects the episodes and a cadence you can hold for six months.')
  ) AS v(slug, display_name, description)
  CROSS JOIN (SELECT id FROM skill_nodes WHERE slug = 'content-production') p
  ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Translation
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT v.slug, v.display_name, v.description, 'communication', p.id
  FROM (VALUES
    ('terminology-management', 'Holding a terminology',
     'A glossary decided once and honoured everywhere. Two translations of one term make the translated docs harder than the original.'),
    ('translation-memory-tools', 'Translation memory tooling',
     'Weblate, Crowdin, Poedit, PO files: working in segments and reusing what has been validated.'),
    ('i18n-extraction', 'String extraction',
     'Making software translatable: strings out of the code, plurals handled, sentences never concatenated.'),
    ('cultural-adaptation', 'Cultural adaptation',
     'Date formats, reading direction, examples, images. What translates badly is not always text.'),
    ('translation-review', 'Bilingual review',
     'Reading both versions and catching the false friend, the approximation, and the correct sentence that says something else.'),
    ('minority-language-tech-vocabulary', 'Technical vocabulary in an under-resourced language',
     'Deciding how to say "buffer" or "race condition" when the language has no word yet, and writing the decision down.')
  ) AS v(slug, display_name, description)
  CROSS JOIN (SELECT id FROM skill_nodes WHERE slug = 'localisation-craft') p
  ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Research writing
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT v.slug, v.display_name, v.description, 'communication', p.id
  FROM (VALUES
    ('literature-review', 'Prior art',
     'Knowing what has already been written, and saying so, before announcing something new.'),
    ('citation-discipline', 'Citing',
     'Attributing every claim that is not yours, with a reference a reader can reach.'),
    ('methodology-writing', 'Writing a method',
     'Describing a protocol precisely enough that a stranger replays it and gets the same result.'),
    ('data-presentation', 'Presenting figures',
     'Tables and charts that show what was measured, with their uncertainties and without a misleading axis.'),
    ('whitepaper-structure', 'Whitepaper structure',
     'The problem, the method, the result, the limits. A whitepaper with no limits section is a brochure.'),
    ('external-rfc-drafting', 'Drafting an external specification',
     'Writing a normative document: constrained vocabulary, edge cases stated, compatibility addressed.'),
    ('peer-review-participation', 'Taking part in peer review',
     'Reviewing somebody else by attacking the method rather than the person, and accepting the same in return.')
  ) AS v(slug, display_name, description)
  CROSS JOIN (SELECT id FROM skill_nodes WHERE slug = 'research-writing-craft') p
  ON CONFLICT (slug) DO NOTHING;
