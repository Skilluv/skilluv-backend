-- An orientation carries its tools as data, not as prose.
--
-- The signup screen shows a trade card and wants to draw the logos of the
-- tools that trade uses. Today that information exists only inside
-- `description`:
--
--     'React, Vue ou Svelte, TypeScript, CSS moderne. Construit ce que la
--      personne voit et manipule.'
--
-- That is a sentence written to be read, with alternatives ("ou"), qualifiers
-- ("moderne") and punctuation. Parsing it for tool names would produce wrong
-- logos and missing ones, invisibly: a card would show three right marks and
-- a fourth invented, with nothing to signal the error. On the screen that
-- asks somebody to choose their trade, a wrong logo is worse than no logo --
-- so the frontend did not try, and asked for this instead.
--
-- `tags` cannot serve either. Its values are broad categories -- web, api,
-- design, mobile -- useful to filter by, unusable to draw a stack.
--
-- Two tables rather than one array of free text:
--
--   * `tools` is the registry. An identifier that is not in it cannot be
--     written, which is what makes the set verifiable rather than a
--     convention nobody enforces. It also carries a display name, so a client
--     with no logo for something can still name it.
--
--   * `orientations.stack` holds the identifiers, in reading order. An array,
--     because the stack has to arrive with the orientation in one query -- a
--     second call per trade would undo the work done on /orientation-counts.
--
-- Identifiers are stable by contract: a rename silently breaks every client
-- mapping them to a logo, so a tool that changes name keeps its id and
-- changes `display_name`.

CREATE TABLE tools (
    id           VARCHAR(40) PRIMARY KEY
                 CHECK (id ~ '^[a-z0-9][a-z0-9.+-]*$'),
    display_name VARCHAR(60) NOT NULL,
    category     VARCHAR(20) NOT NULL
                 CHECK (category IN (
                     'language', 'framework', 'runtime', 'database',
                     'platform', 'protocol', 'os', 'tool'
                 ))
);

COMMENT ON TABLE tools IS
    'Registry of tool identifiers an orientation may name in its stack. Identifiers are stable: rename display_name, never id.';

ALTER TABLE orientations
    ADD COLUMN stack TEXT[] NOT NULL DEFAULT '{}';

COMMENT ON COLUMN orientations.stack IS
    'Tool identifiers, in reading order. Every element exists in tools(id), enforced by orientation_stack_is_known().';

-- A foreign key cannot reach inside an array, so the check is a trigger.
-- Without it the column would be free text with a nicer name, and the first
-- typo would reach the signup screen as a missing logo nobody could explain.
CREATE OR REPLACE FUNCTION orientation_stack_is_known() RETURNS TRIGGER AS $fn$
DECLARE
    unknown TEXT[];
BEGIN
    SELECT array_agg(t) INTO unknown
      FROM unnest(NEW.stack) AS t
     WHERE NOT EXISTS (SELECT 1 FROM tools WHERE tools.id = t);

    IF unknown IS NOT NULL THEN
        RAISE EXCEPTION
            'orientation %: unknown tool identifier(s) % -- add them to tools first',
            NEW.slug, unknown;
    END IF;

    IF (SELECT count(*) FROM unnest(NEW.stack)) <>
       (SELECT count(DISTINCT t) FROM unnest(NEW.stack) AS t) THEN
        RAISE EXCEPTION 'orientation %: stack repeats a tool', NEW.slug;
    END IF;

    RETURN NEW;
END;
$fn$ LANGUAGE plpgsql;

CREATE TRIGGER orientations_stack_is_known
    BEFORE INSERT OR UPDATE OF stack ON orientations
    FOR EACH ROW EXECUTE FUNCTION orientation_stack_is_known();

-- The registry. Only what the backfill below actually uses: an unused
-- identifier is a promise to a client that nothing keeps.
INSERT INTO tools (id, display_name, category) VALUES
    ('react',           'React',            'framework'),
    ('vue',             'Vue',              'framework'),
    ('svelte',          'Svelte',           'framework'),
    ('typescript',      'TypeScript',       'language'),
    ('css',             'CSS',              'language'),
    ('graphql',         'GraphQL',          'protocol'),
    ('rest',            'REST',             'protocol'),
    ('rust',            'Rust',             'language'),
    ('go',              'Go',               'language'),
    ('nodejs',          'Node.js',          'runtime'),
    ('python',          'Python',           'language'),
    ('postgresql',      'PostgreSQL',       'database'),
    ('swift',           'Swift',            'language'),
    ('swiftui',         'SwiftUI',          'framework'),
    ('kotlin',          'Kotlin',           'language'),
    ('jetpack-compose', 'Jetpack Compose',  'framework'),
    ('flutter',         'Flutter',          'framework'),
    ('react-native',    'React Native',     'framework'),
    ('tauri',           'Tauri',            'framework'),
    ('electron',        'Electron',         'framework'),
    ('retool',          'Retool',           'platform'),
    ('airtable',        'Airtable',         'platform'),
    ('n8n',             'n8n',              'platform'),
    ('linux',           'Linux',            'os'),
    ('ros',             'ROS',              'framework'),
    ('solidity',        'Solidity',         'language'),
    ('cairo',           'Cairo',            'language'),
    ('llvm',            'LLVM',             'tool'),
    ('tla-plus',        'TLA+',             'language'),
    ('coq',             'Coq',              'language'),
    ('lucene',          'Lucene',           'tool'),
    ('tantivy',         'Tantivy',          'tool')
ON CONFLICT (id) DO NOTHING;

-- The backfill, written by hand from what each description already says
-- rather than derived from it. Only the trades whose own text names their
-- tools outright are filled; the rest keep an empty stack, which the card
-- renders as no marks at all. That is the intended degradation -- an empty
-- stack says "not recorded yet", an invented one says something false.
UPDATE orientations SET stack = v.stack
  FROM (VALUES
    ('web-frontend-developer',          ARRAY['react','vue','svelte','typescript','css']),
    ('web-backend-developer',           ARRAY['rust','go','nodejs','python','postgresql','rest','graphql']),
    ('mobile-ios-developer',            ARRAY['swift','swiftui']),
    ('mobile-android-developer',        ARRAY['kotlin','jetpack-compose']),
    ('mobile-cross-platform-developer', ARRAY['flutter','react-native']),
    ('desktop-app-developer',           ARRAY['tauri','electron']),
    ('lowcode-platform-developer',      ARRAY['retool','airtable','n8n']),
    ('kernel-driver-developer',         ARRAY['linux']),
    ('robotics-software-developer',     ARRAY['ros']),
    ('smart-contract-developer',        ARRAY['solidity','cairo']),
    ('compiler-language-developer',     ARRAY['llvm']),
    ('formal-methods-developer',        ARRAY['tla-plus','coq']),
    ('search-engine-developer',         ARRAY['lucene','tantivy'])
  ) AS v(slug, stack)
 WHERE orientations.slug = v.slug;

-- Asserts what this migration wrote, and nothing about rows other people
-- created -- the lesson of 0615, where a check on somebody else's data
-- stopped a deploy.
DO $guard$
DECLARE
    filled BIGINT;
BEGIN
    SELECT count(*) INTO filled
      FROM orientations
     WHERE slug IN (
        'web-frontend-developer', 'web-backend-developer',
        'mobile-ios-developer', 'mobile-android-developer',
        'mobile-cross-platform-developer', 'desktop-app-developer',
        'lowcode-platform-developer', 'kernel-driver-developer',
        'robotics-software-developer', 'smart-contract-developer',
        'compiler-language-developer', 'formal-methods-developer',
        'search-engine-developer'
     ) AND stack <> '{}';

    IF filled <> 13 THEN
        RAISE EXCEPTION
            'expected the thirteen trades this migration fills, found %', filled;
    END IF;
END $guard$;
