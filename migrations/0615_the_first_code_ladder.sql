-- Six exercises a new code account can actually do, in order.
--
-- ## Why these exist
--
-- The catalogue is 654 drafts and nothing published. Somebody finishes the
-- entry rite and `GET /api/challenges` hands them an empty list — the platform
-- meeting a person on their best day and having nothing to give them. Opening
-- a seeded trade would not fix it either: those briefs are one-line backlog
-- notes ("publier component lib sur npm"), written as portfolio pieces for
-- somebody with three years of practice, in French only, attached to no
-- repository and to no ladder.
--
-- These six are written for who actually arrives: students, self-taught
-- developers, people changing career. Not easier — smaller, and with a first
-- step that exists.
--
-- ## What makes one of these different from a seeded draft
--
--   * **It says what is out of scope.** The commonest way a beginner loses a
--     week is doing more than was asked and running out of energy before the
--     part that was.
--   * **It is one sitting.** `duration_minutes` is a hint, not a timer — the
--     rites carry none and neither do these; the estimate is for the person
--     deciding whether to start tonight.
--   * **It names a repository.** Every one lands in a Skilluv repo, which is
--     the platform's own hard rule (0061) that all 654 drafts took the
--     training exception from.
--   * **It has a next one.** `challenge_prerequisites` was empty; this is the
--     first chain in it, so "what do I do after this" has an answer that is
--     not a search box.
--   * **It exists in both languages**, from the first day rather than from
--     whenever somebody remembers.
--
-- ## Why they are published and the other 654 are not
--
-- Because these were written to be read, and the honest reason the rest are
-- drafts is that they were not. That distinction is the whole point of the
-- `draft` state, and this migration respects it rather than working around it.
-- A domain curator can and should rewrite any of these; `is_training` and the
-- admin edit surface are how.
--
-- ## project_id
--
-- Set where the project exists, NULL where it does not, and `is_training`
-- either way. `projects` rows are written by `services::seed` at boot, which
-- runs after migrations — so on a fresh database this leaves NULL and the
-- repository still reaches the learner through the `repository` resource,
-- which is always there.

-- ═══════════════════════════════════════════════════════════════════
-- 1. The six briefs
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO challenge_templates (
    title, description, instructions,
    title_i18n, description_i18n, instructions_i18n,
    skill_domain, difficulty, mode, tone, duration_minutes,
    reward_fragments, is_onboarding, is_training, is_domain_rite,
    is_capstone, status, ai_policy, language, orientation_id, project_id,
    evaluation_rubric
)
SELECT
    v.title_en, v.description_en, v.instructions_en,
    jsonb_build_object('en', v.title_en, 'fr', v.title_fr),
    jsonb_build_object('en', v.description_en, 'fr', v.description_fr),
    jsonb_build_object('en', v.instructions_en, 'fr', v.instructions_fr),
    'code', v.difficulty, 'solo', 'educational', v.minutes,
    v.reward, FALSE, TRUE, FALSE,
    FALSE, 'published', 'disclosure_required', v.language,
    (SELECT id FROM orientations WHERE slug = v.trade),
    (SELECT id FROM projects WHERE slug = v.project_slug),
    (SELECT g.criteria FROM review_grids g
      WHERE g.domain = 'code' AND g.reviewer_group IS NULL LIMIT 1)
FROM (VALUES
    (
        'web-backend-developer', 'skilluv-backend', 1, 90, 10, 'sql',
        'Read a query and say what it costs',
        'Pick one endpoint, read the query behind it, and write down what it does to the database.',
        E'The gesture: read production code before changing any.\n\n1. Pick one `GET` endpoint of the Skilluv backend. Any one — the smaller the better for a first pass.\n2. Find the SQL behind it and write, in your own words: which tables it touches, how many rows it can return at worst, and which index it relies on.\n3. Name one thing that would make it slow. Not fix it — name it.\n\nOut of scope: changing anything. This is a reading exercise and the deliverable is prose.\n\nWhat is read: whether somebody who has never opened this file could follow your description to the same query. Getting the answer wrong while showing the reasoning passes; getting it right with no reasoning does not.',
        'Lire une requête et dire ce qu''elle coûte',
        'Choisis un endpoint, lis la requête derrière, et écris ce qu''elle fait à la base.',
        E'Le geste : lire du code de production avant d''en changer.\n\n1. Choisis un endpoint `GET` du backend Skilluv. N''importe lequel — le plus petit est le meilleur pour un premier passage.\n2. Trouve le SQL derrière et écris, avec tes mots : quelles tables il touche, combien de lignes il peut renvoyer au pire, et sur quel index il compte.\n3. Nomme une chose qui le rendrait lent. Pas la corriger — la nommer.\n\nHors périmètre : modifier quoi que ce soit. C''est un exercice de lecture et le rendu est du texte.\n\nCe qui est lu : si quelqu''un qui n''a jamais ouvert ce fichier peut suivre ta description jusqu''à la même requête. Se tromper en montrant le raisonnement passe ; avoir juste sans raisonnement, non.'
    ),
    (
        'web-backend-developer', 'skilluv-backend', 1, 120, 15, 'rust',
        'A test that fails for the right reason',
        'Write one test that fails, and make the failure message say what is wrong.',
        E'The gesture: a test is a sentence about behaviour, not a checkmark.\n\n1. Take any function in the Skilluv backend with no test of its own.\n2. Write a test for a case it handles. Run it. It should pass.\n3. Now break the function on purpose and read the failure. If the message does not tell you what is wrong, rewrite the assertion until it does.\n4. Restore the function. Hand in the test and the failure message it produced.\n\nOut of scope: fixing bugs, adding features, testing more than one case.\n\nWhat is read: the failure message. A test that says `assertion failed: left == right` has not finished; one that says which behaviour broke has.',
        'Un test qui échoue pour la bonne raison',
        'Écris un test qui échoue, et fais que le message d''échec dise ce qui ne va pas.',
        E'Le geste : un test est une phrase sur un comportement, pas une case cochée.\n\n1. Prends n''importe quelle fonction du backend Skilluv qui n''a pas de test.\n2. Écris un test pour un cas qu''elle gère. Lance-le. Il doit passer.\n3. Maintenant casse la fonction exprès et lis l''échec. Si le message ne te dit pas ce qui ne va pas, réécris l''assertion jusqu''à ce qu''il le dise.\n4. Remets la fonction en état. Rends le test et le message d''échec qu''il a produit.\n\nHors périmètre : corriger des bugs, ajouter des fonctionnalités, tester plus d''un cas.\n\nCe qui est lu : le message d''échec. Un test qui dit `assertion failed: left == right` n''a pas fini ; un qui dit quel comportement a cassé, oui.'
    ),
    (
        'web-frontend-developer', 'skilluv-frontend', 2, 150, 20, 'html',
        'A page that holds without JavaScript',
        'Build one page of a real flow that works with JavaScript switched off.',
        E'The gesture: the page works before the script arrives.\n\n1. Pick one page of the Skilluv front — a sign-in, a listing, a form.\n2. Rebuild it in HTML and CSS only. Zero lines of JavaScript.\n3. It must be usable with a keyboard alone, and a screen reader must announce every control.\n4. Say, in three lines, what you had to give up and what you did not.\n\nOut of scope: matching the design pixel for pixel, and any build tooling.\n\nWhat is read: the keyboard path and the form semantics. A page that looks right and traps a keyboard user does not pass — that is the whole exercise.',
        'Une page qui tient sans JavaScript',
        'Construis une page d''un vrai parcours qui fonctionne JavaScript désactivé.',
        E'Le geste : la page marche avant que le script arrive.\n\n1. Choisis une page du front Skilluv — une connexion, une liste, un formulaire.\n2. Reconstruis-la en HTML et CSS uniquement. Zéro ligne de JavaScript.\n3. Elle doit être utilisable au clavier seul, et un lecteur d''écran doit annoncer chaque contrôle.\n4. Dis, en trois lignes, ce que tu as dû abandonner et ce que tu n''as pas abandonné.\n\nHors périmètre : reproduire le design au pixel, et tout outillage de build.\n\nCe qui est lu : le parcours clavier et la sémantique du formulaire. Une page qui a l''air juste et piège un utilisateur au clavier ne passe pas — c''est tout l''exercice.'
    ),
    (
        'web-backend-developer', 'skilluv-backend', 2, 120, 20, 'rust',
        'The form that refuses to be submitted twice',
        'Make one write endpoint safe to call twice, and prove it.',
        E'The gesture: the same request twice leaves the same state once.\n\n1. Take a write endpoint — yours, or one of the Skilluv backend''s.\n2. Call it twice with the same input and show what goes wrong: two rows, two emails, two charges.\n3. Make the second call a no-op. A unique key, an idempotency key, an `ON CONFLICT` — the mechanism is your choice, and saying why you chose it is part of the hand-in.\n4. Write the test that would have caught the original bug.\n\nOut of scope: distributed locking, and anything involving more than one service.\n\nWhat is read: whether the fix holds when the two calls arrive at the same moment, not just one after the other. If it does not, say so — knowing the limit of your own fix is worth more than a fix that pretends not to have one.',
        'Le formulaire qui refuse d''être soumis deux fois',
        'Rends un endpoint d''écriture sûr à appeler deux fois, et prouve-le.',
        E'Le geste : la même requête deux fois laisse le même état une fois.\n\n1. Prends un endpoint d''écriture — le tien, ou un du backend Skilluv.\n2. Appelle-le deux fois avec la même entrée et montre ce qui casse : deux lignes, deux e-mails, deux débits.\n3. Fais que le deuxième appel ne fasse rien. Une clé unique, une clé d''idempotence, un `ON CONFLICT` — le mécanisme est ton choix, et dire pourquoi tu l''as choisi fait partie du rendu.\n4. Écris le test qui aurait attrapé le bug d''origine.\n\nHors périmètre : le verrouillage distribué, et tout ce qui implique plus d''un service.\n\nCe qui est lu : si le correctif tient quand les deux appels arrivent au même instant, et pas seulement l''un après l''autre. Si ce n''est pas le cas, dis-le — connaître la limite de son propre correctif vaut mieux qu''un correctif qui prétend ne pas en avoir.'
    ),
    (
        'web-backend-developer', 'skilluv-backend', 3, 180, 25, 'rust',
        'An error somebody can act on',
        'Take one error message that helps nobody, and make it tell the caller what to do.',
        E'The gesture: an error is a message to a person having a bad day.\n\n1. Find an error the Skilluv backend returns that says what failed and not what to do about it.\n2. Rewrite it so it names what went wrong, why, and the next action. No apology, no blame, no stack trace.\n3. Check it does not leak anything: whether an account exists, an internal path, a query.\n4. Add the test that pins the new message.\n\nOut of scope: changing status codes, and reworking the error type itself.\n\nWhat is read: whether somebody who has never seen the codebase knows what to do next after reading it. And the leak check — an error that helps the caller and tells an attacker which emails are registered is a worse error than the one you replaced.',
        'Une erreur sur laquelle on peut agir',
        'Prends un message d''erreur qui n''aide personne, et fais-lui dire quoi faire.',
        E'Le geste : une erreur est un message à quelqu''un qui passe une mauvaise journée.\n\n1. Trouve une erreur que le backend Skilluv renvoie et qui dit ce qui a échoué sans dire quoi faire.\n2. Réécris-la pour qu''elle nomme ce qui n''a pas marché, pourquoi, et l''action suivante. Pas d''excuse, pas de reproche, pas de stack trace.\n3. Vérifie qu''elle ne fuite rien : l''existence d''un compte, un chemin interne, une requête.\n4. Ajoute le test qui fige le nouveau message.\n\nHors périmètre : changer les codes de statut, et refondre le type d''erreur.\n\nCe qui est lu : si quelqu''un qui n''a jamais vu le code sait quoi faire après l''avoir lue. Et la vérification de fuite — une erreur qui aide l''appelant et dit à un attaquant quels e-mails sont enregistrés est pire que celle que tu remplaces.'
    ),
    (
        'web-backend-developer', 'skilluv-backend', 3, 240, 30, 'rust',
        'Your first pull request on Skilluv',
        'Find one real issue, fix it, and open the pull request.',
        E'The gesture: the whole loop, once, on real code.\n\n1. Find an open issue on a Skilluv repository labelled for newcomers. If none fits, one of the four exercises before this one will have shown you something worth fixing — open the issue yourself first.\n2. Fix it. One thing, the smallest version that is complete.\n3. Write the pull request description: what was wrong, what you changed, how you checked. Three paragraphs is plenty.\n4. Open it, and answer the review.\n\nOut of scope: refactoring anything you were not asked about. A pull request that fixes one thing and reformats forty files is a pull request nobody can review.\n\nWhat is read: the description and the answer to the review. The change is usually small; how you explain and defend it is the trade.',
        'Ta première pull request sur Skilluv',
        'Trouve une vraie issue, corrige-la, et ouvre la pull request.',
        E'Le geste : la boucle entière, une fois, sur du vrai code.\n\n1. Trouve une issue ouverte sur un dépôt Skilluv étiquetée pour les nouveaux venus. Si aucune ne convient, l''un des quatre exercices précédents t''aura montré quelque chose qui mérite d''être corrigé — ouvre l''issue toi-même d''abord.\n2. Corrige. Une seule chose, la plus petite version qui soit complète.\n3. Écris la description de la pull request : ce qui n''allait pas, ce que tu as changé, comment tu as vérifié. Trois paragraphes suffisent.\n4. Ouvre-la, et réponds à la revue.\n\nHors périmètre : refactorer ce qu''on ne t''a pas demandé. Une pull request qui corrige une chose et reformate quarante fichiers est une pull request que personne ne peut relire.\n\nCe qui est lu : la description et la réponse à la revue. Le changement est souvent petit ; la façon dont tu l''expliques et le défends, c''est le métier.'
    )
) AS v(trade, project_slug, difficulty, minutes, reward, language,
       title_en, description_en, instructions_en,
       title_fr, description_fr, instructions_fr)
WHERE NOT EXISTS (
    SELECT 1 FROM challenge_templates ct WHERE ct.title = v.title_en
);

-- ═══════════════════════════════════════════════════════════════════
-- 2. The ladder
-- ═══════════════════════════════════════════════════════════════════
--
-- `challenge_prerequisites` was empty across the whole platform, so no
-- challenge had a "next". A straight chain rather than a graph: for six
-- exercises a person does once, an order somebody can hold in their head beats
-- a lattice that has to be drawn.
--
-- `required = FALSE` throughout — recommended, not enforced. Somebody who
-- already writes tests should not have to prove they can read a query first,
-- and `check_eligibility` only blocks on required edges. The chain is advice
-- the recommendation engine reads, and advice is what a ladder is for.

INSERT INTO challenge_prerequisites (challenge_id, depends_on_challenge_id, required)
SELECT c.id, d.id, FALSE
FROM (VALUES
    ('A test that fails for the right reason',        'Read a query and say what it costs'),
    ('A page that holds without JavaScript',          'Read a query and say what it costs'),
    ('The form that refuses to be submitted twice',   'A test that fails for the right reason'),
    ('An error somebody can act on',                  'The form that refuses to be submitted twice'),
    ('Your first pull request on Skilluv',            'An error somebody can act on')
) AS v(child, parent)
JOIN challenge_templates c ON c.title = v.child
JOIN challenge_templates d ON d.title = v.parent
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- 3. Where to start reading
-- ═══════════════════════════════════════════════════════════════════
--
-- Links, never copies. Official documentation first because it is the thing
-- that stays true; a French resource where a good one exists, because half the
-- audience reads French and "most of it is in English" is a fact to work
-- around rather than one to hand somebody as an answer.

INSERT INTO challenge_resources
    (challenge_id, kind, title, url, language, summary, access_note, sort_order)
SELECT ct.id, v.kind, v.title, v.url, v.language, v.summary, v.access_note, v.sort_order
FROM (VALUES
    ('Read a query and say what it costs', 'repository', 'skilluv-backend',
     'https://github.com/skilluv/skilluv-backend', 'en',
     'The code this exercise is read against.', '', 10),
    ('Read a query and say what it costs', 'documentation', 'PostgreSQL — Using EXPLAIN',
     'https://www.postgresql.org/docs/current/using-explain.html', 'en',
     'How to ask the database what it is about to do, and how to read the answer.', 'Free.', 20),
    ('Read a query and say what it costs', 'documentation', 'PostgreSQL — Utiliser EXPLAIN (fr)',
     'https://docs.postgresql.fr/current/using-explain.html', 'fr',
     'La même page, traduite et tenue à jour par la communauté francophone.', 'Gratuit.', 25),
    ('Read a query and say what it costs', 'community', 'Skilluv — le salon code',
     'https://discord.gg/skilluv', 'mul',
     'Where to ask when the plan does not say what you expected.', 'Account needed.', 40),

    ('A test that fails for the right reason', 'repository', 'skilluv-backend',
     'https://github.com/skilluv/skilluv-backend', 'en',
     'The code this exercise is written against.', '', 10),
    ('A test that fails for the right reason', 'documentation', 'Rust Book — Writing automated tests',
     'https://doc.rust-lang.org/book/ch11-00-testing.html', 'en',
     'The chapter that covers assertions and what a failure prints.', 'Free.', 20),
    ('A test that fails for the right reason', 'documentation', 'Le langage Rust — Écrire des tests (fr)',
     'https://jimskapt.github.io/rust-book-fr/ch11-00-testing.html', 'fr',
     'La traduction française du livre officiel.', 'Gratuit.', 25),

    ('A page that holds without JavaScript', 'repository', 'skilluv-frontend',
     'https://github.com/skilluv/skilluv-frontend', 'en',
     'The pages this exercise rebuilds.', '', 10),
    ('A page that holds without JavaScript', 'documentation', 'MDN — HTML forms',
     'https://developer.mozilla.org/en-US/docs/Learn_web_development/Extensions/Forms', 'en',
     'What a form does on its own, before any script.', 'Free.', 20),
    ('A page that holds without JavaScript', 'documentation', 'MDN — Formulaires HTML (fr)',
     'https://developer.mozilla.org/fr/docs/Learn_web_development/Extensions/Forms', 'fr',
     'La même chose en français.', 'Gratuit.', 25),
    ('A page that holds without JavaScript', 'documentation', 'WAI — Keyboard accessibility',
     'https://www.w3.org/WAI/perspective-videos/keyboard/', 'en',
     'Why the keyboard path is the exercise and not a detail.', 'Free.', 30),

    ('The form that refuses to be submitted twice', 'repository', 'skilluv-backend',
     'https://github.com/skilluv/skilluv-backend', 'en',
     'Where to find a write endpoint to work on.', '', 10),
    ('The form that refuses to be submitted twice', 'documentation', 'PostgreSQL — INSERT ... ON CONFLICT',
     'https://www.postgresql.org/docs/current/sql-insert.html', 'en',
     'One of the three mechanisms, and the one the database gives you for free.', 'Free.', 20),
    ('The form that refuses to be submitted twice', 'article', 'Stripe — Idempotent requests',
     'https://docs.stripe.com/api/idempotent_requests', 'en',
     'How an idempotency key is meant to behave, from people who had to get it right.', 'Free.', 30),

    ('An error somebody can act on', 'repository', 'skilluv-backend',
     'https://github.com/skilluv/skilluv-backend', 'en',
     'The errors this exercise rewrites.', '', 10),
    ('An error somebody can act on', 'documentation', 'OWASP — Improper error handling',
     'https://owasp.org/www-community/Improper_Error_Handling', 'en',
     'What an error must not say, which is the half of this exercise that is easy to miss.', 'Free.', 20),
    ('An error somebody can act on', 'article', 'Nielsen Norman — Error message guidelines',
     'https://www.nngroup.com/articles/error-message-guidelines/', 'en',
     'What a message has to contain to be actionable.', 'Free.', 30),

    ('Your first pull request on Skilluv', 'repository', 'skilluv-backend',
     'https://github.com/skilluv/skilluv-backend', 'en',
     'Where the issues are.', '', 10),
    ('Your first pull request on Skilluv', 'documentation', 'GitHub — Creating a pull request',
     'https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/proposing-changes-to-your-work-with-pull-requests/creating-a-pull-request',
     'en', 'The mechanics, if this is your first.', 'Account needed.', 20),
    ('Your first pull request on Skilluv', 'documentation', 'GitHub — Créer une pull request (fr)',
     'https://docs.github.com/fr/pull-requests/collaborating-with-pull-requests/proposing-changes-to-your-work-with-pull-requests/creating-a-pull-request',
     'fr', 'La même page en français.', 'Compte requis.', 25),
    ('Your first pull request on Skilluv', 'community', 'Skilluv — le salon code',
     'https://discord.gg/skilluv', 'mul',
     'Where to ask before you open it, and after the review lands.', 'Account needed.', 40)
) AS v(challenge_title, kind, title, url, language, summary, access_note, sort_order)
JOIN challenge_templates ct ON ct.title = v.challenge_title
ON CONFLICT (challenge_id, url) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- 4. The ladder is whole
-- ═══════════════════════════════════════════════════════════════════

-- Scoped to the six rows this migration is responsible for, and to nothing
-- else.
--
-- It first asked whether *every* published code exercise had resources, which
-- aborted the whole chain on any database where somebody had already opened a
-- trade through POST /admin/orientations/{slug}/challenges/publish — a
-- documented workflow that legitimately publishes briefs with no reading list
-- yet. Staging had eleven such rows, the migration raised, the container could
-- not start, and the deploy rolled back.
--
-- A migration may assert what it wrote. Asserting a property of rows other
-- people created is how a data check becomes an outage.
DO $$
DECLARE
    mine CONSTANT TEXT[] := ARRAY[
        'Read a query and say what it costs',
        'A test that fails for the right reason',
        'A page that holds without JavaScript',
        'The form that refuses to be submitted twice',
        'An error somebody can act on',
        'Your first pull request on Skilluv'
    ];
    present BIGINT;
    unresourced BIGINT;
BEGIN
    SELECT count(*) INTO present
      FROM challenge_templates
     WHERE title = ANY(mine) AND skill_domain = 'code' AND status = 'published';
    IF present <> 6 THEN
        RAISE EXCEPTION
            'expected the six exercises of this migration, found %', present;
    END IF;

    SELECT count(*) INTO unresourced
      FROM challenge_templates ct
     WHERE ct.title = ANY(mine)
       AND NOT EXISTS (SELECT 1 FROM challenge_resources r WHERE r.challenge_id = ct.id);
    IF unresourced > 0 THEN
        RAISE EXCEPTION
            '% of this migration''s exercises have nowhere to start reading',
            unresourced;
    END IF;
END $$;
