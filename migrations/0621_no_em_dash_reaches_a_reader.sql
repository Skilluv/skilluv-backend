-- Every em dash still sitting in a row, removed (SKI-370).
--
-- Em dashes read as machine-written, and in this product they carried nothing
-- a hyphen could not. The files were cleaned in the same change: src, docs,
-- locales, assets, proto, and the seed SQL under src/services/seed/sql. This
-- is the other half, and the only half that reaches an existing database.
--
-- Why a sweep and not an edit of the migrations that wrote them:
--
--   `sqlx::migrate!()` validates the checksum of every applied migration at
--   boot. Changing one byte of 0173_code_orientations_catalogue.sql makes
--   every deployment refuse to start with VersionMismatch. And it would not
--   help anyway: the em dashes that matter are already rows, and rewriting
--   the file that inserted them changes nothing about the rows.
--
--   Running last, this leaves both an existing database and a freshly built
--   one clean, because a fresh one replays the old migrations and then this.
--
-- Why it walks information_schema instead of naming columns:
--
--   409 text columns across the schema today, and the number moves with every
--   migration. A hand-written list would be correct on the day it was written
--   and quietly incomplete a month later. The walk cannot go stale.
--
-- What it does NOT touch: `_sqlx_migrations`, which is sqlx's own bookkeeping
-- and not ours to rewrite.
--
-- A note on scope, since a blanket rewrite of user content would be a
-- different thing entirely: at the time this runs the platform has no users
-- and no user-authored rows. Every string it touches is seeded copy. A
-- migration runs once, so nothing anybody writes afterwards is affected.

DO $sweep$
DECLARE
    col     RECORD;
    touched BIGINT := 0;
    total   BIGINT := 0;
BEGIN
    -- Plain text columns.
    FOR col IN
        SELECT c.table_name, c.column_name
          FROM information_schema.columns c
          JOIN information_schema.tables t
            ON t.table_name = c.table_name
           AND t.table_schema = c.table_schema
         WHERE c.table_schema = 'public'
           AND t.table_type = 'BASE TABLE'
           AND c.table_name <> '_sqlx_migrations'
           AND c.data_type IN ('text', 'character varying', 'character')
         ORDER BY c.table_name, c.column_name
    LOOP
        EXECUTE format(
            'UPDATE public.%I SET %I = replace(%I, %L, %L) WHERE %I LIKE %L',
            col.table_name, col.column_name, col.column_name,
            chr(8212), '-', col.column_name, '%' || chr(8212) || '%'
        );
        GET DIAGNOSTICS touched = ROW_COUNT;
        total := total + touched;
    END LOOP;

    -- JSONB columns, which is where the bilingual copy lives: title_i18n,
    -- description_i18n, instructions_i18n, badge_rules and the rest.
    --
    -- Rewritten through the text representation. An em dash can only appear
    -- inside a string value here (no key in this schema contains one), and
    -- replacing it with a hyphen leaves the document valid either way.
    FOR col IN
        SELECT c.table_name, c.column_name
          FROM information_schema.columns c
          JOIN information_schema.tables t
            ON t.table_name = c.table_name
           AND t.table_schema = c.table_schema
         WHERE c.table_schema = 'public'
           AND t.table_type = 'BASE TABLE'
           AND c.data_type IN ('jsonb', 'json')
         ORDER BY c.table_name, c.column_name
    LOOP
        EXECUTE format(
            'UPDATE public.%I SET %I = replace(%I::text, %L, %L)::jsonb '
            'WHERE %I::text LIKE %L',
            col.table_name, col.column_name, col.column_name,
            chr(8212), '-', col.column_name, '%' || chr(8212) || '%'
        );
        GET DIAGNOSTICS touched = ROW_COUNT;
        total := total + touched;
    END LOOP;

    RAISE NOTICE 'em dash sweep: % row(s) rewritten', total;
END $sweep$;

-- And prove it, rather than trusting the loop above.
--
-- The same walk, asserting nothing is left. If a column type is added later
-- that can hold prose and is not covered by either loop, this fails and names
-- it instead of letting an em dash survive unnoticed.
DO $verify$
DECLARE
    col       RECORD;
    remaining BIGINT;
    offenders TEXT[] := '{}';
BEGIN
    FOR col IN
        SELECT c.table_name, c.column_name, c.data_type
          FROM information_schema.columns c
          JOIN information_schema.tables t
            ON t.table_name = c.table_name
           AND t.table_schema = c.table_schema
         WHERE c.table_schema = 'public'
           AND t.table_type = 'BASE TABLE'
           AND c.table_name <> '_sqlx_migrations'
           AND c.data_type IN ('text', 'character varying', 'character',
                               'jsonb', 'json')
    LOOP
        EXECUTE format(
            'SELECT count(*) FROM public.%I WHERE %I::text LIKE %L',
            col.table_name, col.column_name, '%' || chr(8212) || '%'
        ) INTO remaining;

        IF remaining > 0 THEN
            offenders := offenders || format('%s.%s (%s rows)',
                col.table_name, col.column_name, remaining);
        END IF;
    END LOOP;

    IF array_length(offenders, 1) > 0 THEN
        RAISE EXCEPTION 'em dashes survived the sweep in: %',
            array_to_string(offenders, ', ');
    END IF;
END $verify$;
