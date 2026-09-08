-- The design rite asks for one thing, and it is the same thing the code rite
-- asks for.
--
-- What was there: "Le geste : un écran, assez fini pour qu'on puisse le
-- discuter. 1. Le brief, et c'est tout le brief : un écran qu'on utilise une
-- fois et qu'on ne devrait jamais avoir à réutiliser [...] Choisis lequel."
--
-- Three problems with it, and the first is fatal.
--
-- The brief did not exist. Not as a resource, not as a link, not as a row.
-- Somebody was asked to design against a brief they had to invent from one
-- sentence, on their first act on the platform.
--
-- The description promised "lu par trois relecteurs". It is read by one, in
-- the generic queue, like all twelve rites. `continues_in` on the rite
-- catalogue is documentation and not routing, and the copy shown to the
-- person said otherwise.
--
-- And it only fits an interface designer. The design domain has 26
-- orientations: illustration, motion, 3D, naming, ux writing, sound. "Design
-- one screen" means nothing to most of them.
--
-- What replaces it is the code rite's own idea. HELLO.md asks a developer to
-- say who they are in the medium they work in: a file, in a repository,
-- through a pull request. The same request, in another medium.
--
--   Three things: your name, your trade, one thing you can do.
--   Said with your trade.
--
-- An illustrator draws it. A motion designer animates ten seconds. A UX
-- writer writes a hundred words. An interface designer makes a screen.
--
-- That covers all 26 without a brief per trade, and the medium is the proof:
-- somebody who introduces themselves by drawing has already shown that they
-- draw. It cannot be copied, because it is about them. And it is their first
-- portfolio piece rather than an exercise thrown away, exactly as the first
-- commit stays on their fork.
--
-- Nothing new is needed to receive it. `design_upload_sessions` already
-- carries eleven subtypes covering every form this can take, with their own
-- ceilings, and the four a browser cannot open already require a preview.

UPDATE challenge_templates SET
    title = 'Your HELLO',
    description = 'Introduce yourself using your trade. An image, a video, a text or a link. Jokes are welcome, an empty file is not.',
    instructions = E'1. Three things to get across: your name, your trade, one thing you can do.\n2. Say them with your trade. An illustrator draws it, a motion designer animates ten seconds, a UX writer writes a hundred words, an interface designer makes a screen. One artefact, not a series.\n3. Upload it, or paste the link if it lives in Figma.',
    title_i18n = jsonb_build_object(
        'en', 'Your HELLO',
        'fr', 'Ton HELLO'
    ),
    description_i18n = jsonb_build_object(
        'en', 'Introduce yourself using your trade. An image, a video, a text or a link. Jokes are welcome, an empty file is not.',
        'fr', E'Présente-toi en te servant de ton métier. Une image, une vidéo, un texte ou un lien. Les blagues sont autorisées, le fichier vide non.'
    ),
    instructions_i18n = jsonb_build_object(
        'en', E'1. Three things to get across: your name, your trade, one thing you can do.\n2. Say them with your trade. An illustrator draws it, a motion designer animates ten seconds, a UX writer writes a hundred words, an interface designer makes a screen. One artefact, not a series.\n3. Upload it, or paste the link if it lives in Figma.',
        'fr', E'1. Trois choses à faire passer : ton nom, ton métier, une chose que tu sais faire.\n2. Dis-les avec ton métier. Un illustrateur dessine, un motion designer anime dix secondes, un UX writer écrit cent mots, un designer d''interface fait un écran. Un seul artefact, pas une série.\n3. Dépose-le, ou colle le lien s''il vit chez Figma.'
    ),
    updated_at = NOW()
-- By what makes it the design rite, not by the id it happens to carry.
-- `challenge_templates.id` is generated per database, so an id would update
-- nothing on a fresh one. The unique index
-- `challenge_templates_one_rite_per_domain` guarantees this matches one row.
WHERE is_domain_rite = TRUE
  AND skill_domain = 'design'
  AND status = 'published';

-- No reading list, and that is a decision rather than an omission.
--
-- The code rite has five resources because opening a pull request is a
-- procedure somebody can be taught. Introducing yourself is not. What would
-- help here is an example of a good HELLO, which does not exist yet and which
-- would be design work rather than a link. Inventing a URL to fill the field
-- is how a rite ends up pointing at a page that was never written.

DO $guard$
DECLARE
    rewritten BIGINT;
    stale     BIGINT;
BEGIN
    SELECT count(*) INTO rewritten
      FROM challenge_templates
     WHERE is_domain_rite = TRUE AND skill_domain = 'design' AND status = 'published'
       AND title = 'Your HELLO';
    IF rewritten <> 1 THEN
        RAISE EXCEPTION
            'the design rite was not rewritten: expected one row, found %', rewritten;
    END IF;

    -- The three claims that had to go: three reviewers, the brief that does
    -- not exist, and the grader's job described to somebody who has not done
    -- theirs.
    SELECT count(*) INTO stale
      FROM challenge_templates ct,
           LATERAL (VALUES
               (ct.title_i18n ->> 'fr'), (ct.title_i18n ->> 'en'),
               (ct.description_i18n ->> 'fr'), (ct.description_i18n ->> 'en'),
               (ct.instructions_i18n ->> 'fr'), (ct.instructions_i18n ->> 'en'),
               (ct.title), (ct.description), (ct.instructions)
           ) AS s(text)
     WHERE ct.is_domain_rite = TRUE AND ct.skill_domain = 'design'
       AND ct.status = 'published'
       AND (s.text LIKE '%trois relecteurs%'
            OR s.text LIKE '%three reviewers%'
            OR s.text LIKE '%Ce qui est lu%'
            OR s.text LIKE '%What is read%');
    IF stale > 0 THEN
        RAISE EXCEPTION
            'the design rite still carries % stale claim(s)', stale;
    END IF;
END $guard$;
