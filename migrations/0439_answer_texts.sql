-- One function for reading a wizard answer, whatever shape it was stored in.
--
-- `user_domain_profiles.answers` is JSONB, and the wizards disagree about
-- cardinality for questions that are otherwise the same. Code asks which
-- languages you work in and stores an array; design asks which tool you work
-- in and stores a string. Both are correct — a designer really does work in
-- one tool — and every query that reads them had to know which.
--
-- `jsonb_array_elements_text` over a bare string does not return nothing; it
-- raises, so the query fails rather than returning an empty list. That is why
-- design ended up with its own copy of the matcher instead of a parameter.
--
-- IMMUTABLE, so it can be used in an index or a generated column later
-- without a second version. Deliberately not STRICT: the profile row comes
-- through a LEFT JOIN, and a NULL `answers` means "never answered the
-- wizard", which reads the same way as an absent key rather than as NULL.

CREATE OR REPLACE FUNCTION answer_texts(answers JSONB, key TEXT)
RETURNS TEXT[]
LANGUAGE sql
IMMUTABLE
PARALLEL SAFE
AS $$
    SELECT CASE jsonb_typeof(answers -> key)
        WHEN 'array'  THEN COALESCE(
                              ARRAY(SELECT jsonb_array_elements_text(answers -> key)),
                              '{}')
        WHEN 'string' THEN ARRAY[answers ->> key]
        -- Absent, null, a number, an object: no answer, which is not the same
        -- as an empty answer but is read the same way by everything upstream.
        ELSE '{}'::TEXT[]
    END
$$;

COMMENT ON FUNCTION answer_texts(JSONB, TEXT) IS
    'Wizard answer as a text array, whether it was stored as an array or a '
    'single string. Returns an empty array for anything else, including a '
    'missing key.';
