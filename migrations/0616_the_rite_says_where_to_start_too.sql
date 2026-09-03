-- Two corrections to 0615, and guidance on the first brief anybody reads.
--
-- ## The invented link
--
-- 0615 attached `https://discord.gg/skilluv` to two challenges as a
-- `community` resource. No such invite exists anywhere in this repository —
-- it was written because a community link belonged there, not because that one
-- was real. A dead link in the first list a beginner is handed is worse than
-- an empty list: it teaches them the guidance is decorative.
--
-- Deleted rather than replaced. Where to ask is already answered by the
-- `help` block of the guidance, which points at this platform's own forum and
-- carries the challenge id — real, internal, and not something anybody has to
-- guess a URL for. A curated external community link can be added later by
-- somebody who has one.
--
-- ## The first brief had no guidance at all
--
-- `GET /api/challenges/onboarding` returned the rite and nothing around it.
-- So the one brief every account reads first — the only one where being
-- stranded means never starting — was the one with no resources, nowhere to
-- ask, and no count of who asked before. The endpoint serves guidance now, and
-- the code rite gets the reading that its gesture actually requires: what a
-- fork is, what a pull request is, and how to write a Markdown file.
--
-- The other eleven rites are left without external reading on purpose. Their
-- gesture is a piece of their own trade — a screen, a playtest verdict, twenty
-- seconds of sound — and there is no one link that is right for all of them.
-- Inventing eleven would repeat the mistake above. They keep the help channels
-- and the discussion count, which are real, and a domain curator adds the
-- reading their trade actually starts from.

DELETE FROM challenge_resources
 WHERE url = 'https://discord.gg/skilluv';

INSERT INTO challenge_resources
    (challenge_id, kind, title, url, language, summary, access_note, sort_order)
SELECT ct.id, v.kind, v.title, v.url, v.language, v.summary, v.access_note, v.sort_order
FROM (VALUES
    ('documentation', 'GitHub — Creating a pull request',
     'https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/proposing-changes-to-your-work-with-pull-requests/creating-a-pull-request',
     'en', 'The gesture this rite asks for, step by step.', 'Account needed.', 10),
    ('documentation', 'GitHub — Créer une pull request',
     'https://docs.github.com/fr/pull-requests/collaborating-with-pull-requests/proposing-changes-to-your-work-with-pull-requests/creating-a-pull-request',
     'fr', 'Le geste que ce rite demande, étape par étape.', 'Compte requis.', 15),
    ('course', 'Pro Git — the book',
     'https://git-scm.com/book/en/v2', 'en',
     'If this is your first time with git. Chapters 1 and 2 are enough for this rite.',
     'Free, and readable in the browser.', 20),
    ('course', 'Pro Git — le livre',
     'https://git-scm.com/book/fr/v2', 'fr',
     'Si c''est ta première fois avec git. Les chapitres 1 et 2 suffisent pour ce rite.',
     'Gratuit, lisible dans le navigateur.', 25),
    ('documentation', 'Markdown — basic syntax',
     'https://www.markdownguide.org/basic-syntax/', 'en',
     'HELLO.md is Markdown. This is the whole of what you need for it.',
     'Free.', 30)
) AS v(kind, title, url, language, summary, access_note, sort_order)
JOIN challenge_templates ct
  ON ct.is_domain_rite AND ct.skill_domain = 'code' AND ct.status = 'published'
ON CONFLICT (challenge_id, url) DO NOTHING;

-- Nothing points at a host this repository invented.
DO $$
DECLARE
    invented BIGINT;
BEGIN
    SELECT count(*) INTO invented
      FROM challenge_resources
     WHERE url LIKE '%discord.gg%';
    IF invented > 0 THEN
        RAISE EXCEPTION
            '% resource(s) still point at an invite nobody created', invented;
    END IF;
END $$;
