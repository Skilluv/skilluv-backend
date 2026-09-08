-- The code rite tells somebody what to do, and stops explaining itself.
--
-- Two changes, both asked for by the person whose product this is.
--
-- The closing paragraph goes:
--
--     FR  "Ce qui est lu : la pull request elle-même. Pas sa longueur — le
--          fait que quelqu'un qui arrive sur ton fork comprenne ce que tu
--          viens faire."
--     EN  "What is read: the pull request itself. Not its length — whether
--          somebody arriving on your fork can tell what you are here to do."
--
-- Somebody on their first act does not need to be told what the grader looks
-- at. They need to be told what to do, in a way that makes them want to do
-- it. The paragraph described the reviewer's job to a reader who has not yet
-- done theirs.
--
-- And the em dashes go, here and in user-facing copy generally. Replaced by
-- the punctuation that reads best in each place rather than by a hyphen
-- substituted mechanically: the title is rewritten outright, and step 1 takes
-- a comma.
--
-- The opening line "Le geste : une pull request sur un dépôt qui est le tien"
-- goes with them. The three steps say it, and a first-time reader should meet
-- the instruction before the framing.
--
-- What this migration does NOT do: the other eleven rites carry the same two
-- shapes -- a "Bonjour Skilluv — X" title and a "Ce qui est lu" closing --
-- and no replacement copy has been approved for them. Rewriting them here
-- would be inventing eleven trades' worth of text nobody has read. Counted so
-- the next pass knows its size: twelve titles, twelve closings, and
-- twenty-six em dashes across the French bodies.

UPDATE challenge_templates SET
    title = 'Your first commit',
    description = 'We fork a starter onto your account. You introduce yourself in it, you open the pull request.',
    instructions = E'1. Start the rite, we copy a starter onto your GitHub.\n2. Clone it and write in `HELLO.md`: who you are, what you want to build, what you already know. (No git locally? Edit the file straight on GitHub, it works the same.)\n3. Commit, push, and open the pull request from `main` to `showcase` on your fork.',
    title_i18n = jsonb_build_object(
        'en', 'Your first commit',
        'fr', 'Ton premier commit'
    ),
    description_i18n = jsonb_build_object(
        'en', 'We fork a starter onto your account. You introduce yourself in it, you open the pull request.',
        'fr', E'On te fork un starter. Tu te présentes dedans, tu ouvres la pull request.'
    ),
    instructions_i18n = jsonb_build_object(
        'en', E'1. Start the rite, we copy a starter onto your GitHub.\n2. Clone it and write in `HELLO.md`: who you are, what you want to build, what you already know. (No git locally? Edit the file straight on GitHub, it works the same.)\n3. Commit, push, and open the pull request from `main` to `showcase` on your fork.',
        'fr', E'1. Lance le rite, on copie un starter sur ton GitHub.\n2. Clone-le et écris dans `HELLO.md` : qui tu es, ce que tu veux construire, ce que tu sais déjà faire. (Pas de git en local ? Édite le fichier directement sur GitHub, ça marche pareil.)\n3. Commit, push, et ouvre la pull request de `main` vers `showcase` sur ton fork.'
    ),
    updated_at = NOW()
-- Addressed by what makes it the code rite, not by the id it happens to have.
-- `9a79dd29-...` is the id on the deployed database; ids are generated per
-- database, so targeting it would update nothing on a fresh one and the guard
-- below would fail in CI rather than in production. The unique index
-- `challenge_templates_one_rite_per_domain` guarantees this matches one row.
WHERE is_domain_rite = TRUE
  AND skill_domain = 'code'
  AND status = 'published';

-- The parenthetical about not having git locally was not in the approved
-- text, and is kept in French and added in English on purpose: a beginner
-- with no local git is exactly who this screen is for, and removing a barrier
-- serves the same intent as the rewrite. It is the one deviation, and it is
-- deliberate.

DO $guard$
DECLARE
    row_count BIGINT;
    leftover  BIGINT;
BEGIN
    SELECT count(*) INTO row_count
      FROM challenge_templates
     WHERE is_domain_rite = TRUE AND skill_domain = 'code' AND status = 'published'
       AND title = 'Your first commit';
    IF row_count <> 1 THEN
        RAISE EXCEPTION
            'the code rite was not rewritten: expected one row, found %', row_count;
    END IF;

    -- No em dash, and no closing paragraph, in either language.
    SELECT count(*) INTO leftover
      FROM challenge_templates ct,
           LATERAL (VALUES
               (ct.title_i18n ->> 'fr'), (ct.title_i18n ->> 'en'),
               (ct.description_i18n ->> 'fr'), (ct.description_i18n ->> 'en'),
               (ct.instructions_i18n ->> 'fr'), (ct.instructions_i18n ->> 'en'),
               (ct.title), (ct.description), (ct.instructions)
           ) AS s(text)
     WHERE ct.is_domain_rite = TRUE AND ct.skill_domain = 'code'
       AND ct.status = 'published'
       AND (s.text LIKE '%' || chr(8212) || '%'
            OR s.text LIKE '%Ce qui est lu%'
            OR s.text LIKE '%What is read%');
    IF leftover > 0 THEN
        RAISE EXCEPTION
            'the code rite still carries % em dash or closing-paragraph string(s)', leftover;
    END IF;
END $guard$;
