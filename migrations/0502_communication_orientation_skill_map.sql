-- What each communication trade is actually made of.
--
-- ## Core and recommended
--
-- Core is what the trade cannot exist without: remove it and the person is
-- doing something else. Three to five per orientation. A trade where
-- everything is core says nothing about what to learn first, which is the
-- only thing this map is read for.
--
-- ## Why every one of the five points outside `communication`
--
-- A communication artefact is *about* something. A writer who cannot read the
-- code they document produces prose around an API rather than about it, an
-- advocate who has never shipped anything is a presenter, and a translator
-- who does not know what a build is will translate "compile" wrongly and be
-- confident about it. The rows reaching into `code`, `ops`, `ai` and
-- `soft_skills` are where that is written down rather than assumed.
--
-- ## The one shared core
--
-- `written-communication` is core for all five, including the two whose
-- output is spoken. A talk that is not written first is a talk that wanders,
-- and a video script is writing with a different delivery.

INSERT INTO orientation_skill_map (orientation_id, skill_id, is_core, is_recommended, weight)
SELECT o.id, s.id, v.is_core, TRUE, v.weight
  FROM (VALUES

-- ── tech-writer ────────────────────────────────────────────────────
('tech-writer', 'written-communication',            TRUE,  1.0),
('tech-writer', 'documentation-craft',              TRUE,  1.0),
('tech-writer', 'docs-information-architecture',    TRUE,  1.0),
('tech-writer', 'tutorial-writing',                 TRUE,  1.0),
('tech-writer', 'api-reference-writing',            TRUE,  1.0),
('tech-writer', 'runnable-examples',                FALSE, 0.9),
('tech-writer', 'changelog-and-migration-guides',   FALSE, 0.8),
('tech-writer', 'docs-as-code',                     FALSE, 0.8),
('tech-writer', 'docs-linting',                     FALSE, 0.6),
('tech-writer', 'audience-analysis',                FALSE, 0.9),
('tech-writer', 'editing-and-revision',             FALSE, 0.9),
('tech-writer', 'readme-authoring',                 FALSE, 0.7),
('tech-writer', 'technical-writing',                FALSE, 0.9),
('tech-writer', 'adr-writing',                      FALSE, 0.6),
-- Outside the domain: you cannot document what you cannot read.
('tech-writer', 'git-workflow',                     FALSE, 0.7),

-- ── developer-advocate ─────────────────────────────────────────────
('developer-advocate', 'written-communication',     TRUE,  1.0),
('developer-advocate', 'public-speaking-tech',      TRUE,  1.0),
('developer-advocate', 'talk-structure',            TRUE,  1.0),
('developer-advocate', 'live-demo-resilience',      TRUE,  1.0),
('developer-advocate', 'community-engagement',      TRUE,  1.0),
('developer-advocate', 'conference-cfp-writing',    FALSE, 0.9),
('developer-advocate', 'slide-craft',               FALSE, 0.8),
('developer-advocate', 'workshop-facilitation-tech', FALSE, 0.8),
('developer-advocate', 'developer-empathy',         FALSE, 0.9),
('developer-advocate', 'technical-writing',         FALSE, 0.8),
('developer-advocate', 'audience-analysis',         FALSE, 0.7),
-- Outside the domain: an advocate who has never shipped is a presenter.
('developer-advocate', 'git-workflow',              FALSE, 0.7),
('developer-advocate', 'giving-feedback',           FALSE, 0.6),

-- ── content-creator-tech ───────────────────────────────────────────
('content-creator-tech', 'written-communication',   TRUE,  1.0),
('content-creator-tech', 'content-production',      TRUE,  1.0),
('content-creator-tech', 'video-scripting',         TRUE,  1.0),
('content-creator-tech', 'screen-recording-quality', TRUE, 1.0),
('content-creator-tech', 'audio-for-talking-head',  TRUE,  1.0),
('content-creator-tech', 'video-editing-basics',    FALSE, 0.9),
('content-creator-tech', 'thumbnail-and-title',     FALSE, 0.7),
('content-creator-tech', 'livestream-operations',   FALSE, 0.7),
('content-creator-tech', 'podcast-interviewing',    FALSE, 0.6),
('content-creator-tech', 'content-series-planning', FALSE, 0.8),
('content-creator-tech', 'public-speaking-tech',    FALSE, 0.8),
('content-creator-tech', 'audience-analysis',       FALSE, 0.8),

-- ── technical-translator ───────────────────────────────────────────
('technical-translator', 'written-communication',   TRUE,  1.0),
('technical-translator', 'localisation-craft',      TRUE,  1.0),
('technical-translator', 'terminology-management',  TRUE,  1.0),
('technical-translator', 'translation-review',      TRUE,  1.0),
('technical-translator', 'cultural-adaptation',     FALSE, 0.9),
('technical-translator', 'translation-memory-tools', FALSE, 0.8),
('technical-translator', 'i18n-extraction',         FALSE, 0.8),
('technical-translator', 'minority-language-tech-vocabulary', FALSE, 0.7),
('technical-translator', 'documentation-craft',     FALSE, 0.8),
('technical-translator', 'editing-and-revision',    FALSE, 0.8),
-- Outside the domain: extraction is a change to somebody's source tree.
('technical-translator', 'git-workflow',            FALSE, 0.7),

-- ── research-writer-tech ───────────────────────────────────────────
('research-writer-tech', 'written-communication',   TRUE,  1.0),
('research-writer-tech', 'research-writing-craft',  TRUE,  1.0),
('research-writer-tech', 'methodology-writing',     TRUE,  1.0),
('research-writer-tech', 'citation-discipline',     TRUE,  1.0),
('research-writer-tech', 'literature-review',       TRUE,  1.0),
('research-writer-tech', 'data-presentation',       FALSE, 0.9),
('research-writer-tech', 'whitepaper-structure',    FALSE, 0.9),
('research-writer-tech', 'external-rfc-drafting',   FALSE, 0.7),
('research-writer-tech', 'peer-review-participation', FALSE, 0.7),
('research-writer-tech', 'editing-and-revision',    FALSE, 0.8),
('research-writer-tech', 'adr-writing',             FALSE, 0.6)

  ) AS v(orientation_slug, skill_slug, is_core, weight)
  JOIN orientations o ON o.slug = v.orientation_slug
  JOIN skill_nodes  s ON s.slug = v.skill_slug
ON CONFLICT (orientation_id, skill_id) DO NOTHING;

-- Every slug above has to exist. The JOIN would otherwise drop an unknown one
-- silently, and a skill map short of three edges reads exactly like a skill
-- map that was written that way — which is the failure this block turns into
-- a migration that refuses to apply.
DO $$
DECLARE
    expected INT := 62;
    actual   INT;
BEGIN
    SELECT count(*) INTO actual
      FROM orientation_skill_map m
      JOIN orientations o ON o.id = m.orientation_id
     WHERE o.primary_domain = 'communication';

    IF actual <> expected THEN
        RAISE EXCEPTION
            'communication skill map: % edges written, % expected — a skill slug above does not exist',
            actual, expected;
    END IF;
END $$;
