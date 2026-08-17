-- Six AI categories, in the ceremony that already exists.
--
-- ## Why not a separate AI ceremony
--
-- The backlog called for annual Skilluv AI Awards. Migration 0190 built the
-- editions, the nominations, the 70/30 community-jury split and the vote —
-- and none of it is code-specific. A second ceremony would mean a second date
-- to organise, a second audience to gather, and two winners' lists nobody
-- reads together.
--
-- One ceremony with categories from every domain is also the more useful
-- shape for the thing an award is actually for: an AI researcher and a
-- library author being named on the same evening is what makes the AI
-- categories visible to people who would never have looked at them.
--
-- `award_categories` has no domain column and does not need one. A category
-- is named after the work, not after a taxonomy, and "Best Dataset Published"
-- says what it is without a label next to it.
--
-- ## Rookie
--
-- The code ceremony has its own newcomer category, and this adds a second
-- rather than sharing one. Deliberately: a first year in AI and a first year
-- in code are not comparable, and a single newcomer prize would go to
-- whichever domain had more voters.

INSERT INTO award_categories (slug, name, description, subject_type, sort_order)
VALUES
    ('best-ai-model',
     'Best AI Model of the Year',
     'Un modèle publié, téléchargeable, et évalué honnêtement. Jugé sur ce qu''il rend possible et sur la qualité de sa fiche, pas sur le nombre de paramètres.',
     'project', 200),

    ('best-dataset-published',
     'Best Dataset Published',
     'Un jeu de données que d''autres réutilisent, avec sa provenance, ses licences et ses biais écrits. Le travail le moins spectaculaire du domaine et le plus durable.',
     'project', 210),

    ('best-ai-application',
     'Best AI Application Deployed',
     'Un système en service, utilisé par des gens qui ne l''ont pas construit, et qui dit ce qu''il ne sait pas faire.',
     'project', 220),

    ('best-ai-safety-research',
     'Best AI Safety Research',
     'Une trouvaille reproduite, divulguée dans les règles, et accompagnée d''une atténuation. La divulgation compte autant que la trouvaille.',
     'deliverable', 230),

    ('best-prompt-engineering',
     'Best Prompt Engineering Innovation',
     'Une méthode d''évaluation ou de calibration que d''autres ont reprise. Ce qui se juge est le protocole, pas l''invite.',
     'deliverable', 240),

    ('rookie-ai-researcher',
     'Rookie AI Researcher',
     'Une première année dans le domaine. Compté depuis le premier artefact vérifié, pas depuis l''inscription.',
     'user', 250);
