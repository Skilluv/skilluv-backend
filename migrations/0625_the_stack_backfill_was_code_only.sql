-- The stack backfill reached one domain out of twelve.
--
-- Migration 0619 built `tools` and `orientations.stack` as a general thing and
-- then filled them for code alone: every one of its thirty identifiers is a
-- language, a framework or a runtime. So `/api/orientations` answers a
-- developer with their languages and a designer with an empty array, and the
-- screen where somebody chooses their trade says less about eleven domains
-- than about one. That is not a design gap; design is simply where it was
-- noticed.
--
-- ## The rule is 0619's rule, unchanged
--
-- A stack is filled only where the orientation's own description names the
-- tool outright. `test_ski_367_orientation_stack` forbids the alternative in
-- as many words: an invented tool is worse than a missing one on the screen
-- where somebody picks a trade, and a stack derived from what a trade
-- probably uses is an invention with a citation.
--
-- Applying that rule to the other eleven catalogues yields seven orientations,
-- which is few and is the point. Most trades do not name their tools, and
-- their empty stack keeps meaning "not recorded", exactly as it does for most
-- code trades.
--
-- ## What was left out, deliberately
--
-- `quality` mentions Playwright and `ops` mentions Terraform and Prometheus,
-- but only in the migrations' own comments about artefact kinds, never in an
-- orientation description. Nothing is written for them.
--
-- `game-vfx-artist` says "VFX Graph, Niagara or Godot particles". VFX Graph is
-- Unity's and Niagara is Unreal's, and I know that, which is exactly why only
-- `godot` is written: the test is whether the text names the tool, not whether
-- the reader can identify it.

INSERT INTO tools (id, display_name, category) VALUES
    -- Creative applications: what somebody opens to do the work.
    ('blender',        'Blender',        'tool'),
    ('cinema-4d',      'Cinema 4D',      'tool'),
    ('rive',           'Rive',           'tool'),
    -- Runtime libraries and middleware: what the work is delivered into.
    ('lottie',         'Lottie',         'framework'),
    ('fmod',           'FMOD',           'framework'),
    ('wwise',          'Wwise',          'framework'),
    -- Game engines.
    ('godot',          'Godot',          'framework'),
    ('unity',          'Unity',          'framework'),
    ('unreal-engine',  'Unreal Engine',  'framework'),
    ('bevy',           'Bevy',           'framework'),
    -- And the one orientation whose name is its tool.
    ('kubernetes',     'Kubernetes',     'platform')
ON CONFLICT (id) DO NOTHING;

-- Seven orientations, each quoting the sentence that authorises it.

-- "Motion d'interface, transitions et logos animés, livrés en Lottie ou Rive"
UPDATE orientations SET stack = ARRAY['lottie', 'rive']
 WHERE slug = 'design-motion-ui' AND stack = '{}';

-- "Motion Cinema 4D et Blender, animation produit 3D"
UPDATE orientations SET stack = ARRAY['cinema-4d', 'blender']
 WHERE slug = 'design-motion-3d' AND stack = '{}';

-- "FMOD, Wwise ou le moteur nu. La partition est ecrite, le comportement se
--  programme."
UPDATE orientations SET stack = ARRAY['fmod', 'wwise']
 WHERE slug = 'audio-music-implementer' AND stack = '{}';

-- "Godot modules, Bevy crates, a custom engine."
UPDATE orientations SET stack = ARRAY['godot', 'bevy']
 WHERE slug = 'game-engine-programmer' AND stack = '{}';

-- "Blend-space ready for Unity or Unreal"
UPDATE orientations SET stack = ARRAY['unity', 'unreal-engine']
 WHERE slug = 'game-animator-3d' AND stack = '{}';

-- "VFX Graph, Niagara or Godot particles"
UPDATE orientations SET stack = ARRAY['godot']
 WHERE slug = 'game-vfx-artist' AND stack = '{}';

-- The trade is named "Specialiste Kubernetes". The tool is the job title.
UPDATE orientations SET stack = ARRAY['kubernetes']
 WHERE slug = 'kubernetes-specialist' AND stack = '{}';

-- `AND stack = '{}'` on every one of them: this migration fills blanks, it
-- does not overwrite a stack an operator curated by hand between 0619 and now.

DO $guard$
DECLARE
    filled  BIGINT;
    unknown BIGINT;
BEGIN
    SELECT count(*) INTO filled
      FROM orientations
     WHERE slug IN ('design-motion-ui', 'design-motion-3d',
                    'audio-music-implementer', 'game-engine-programmer',
                    'game-animator-3d', 'game-vfx-artist',
                    'kubernetes-specialist')
       AND cardinality(stack) > 0;
    IF filled <> 7 THEN
        RAISE EXCEPTION
            'expected seven orientations to carry a stack, found %', filled;
    END IF;

    -- Every identifier written has to be one the registry knows, which is the
    -- invariant 0619 introduced and the reason `tools` exists at all.
    SELECT count(*) INTO unknown
      FROM orientations o, unnest(o.stack) AS t
     WHERE NOT EXISTS (SELECT 1 FROM tools WHERE tools.id = t);
    IF unknown > 0 THEN
        RAISE EXCEPTION
            '% stack entries name a tool the registry does not carry', unknown;
    END IF;
END $guard$;
