-- Six exercises a new design account can actually do, in order.
--
-- ## The same hole, in another domain
--
-- Migration 0615 wrote this for code and named the problem: the catalogue is
-- drafts, so somebody finishes the entry rite and `GET /api/challenges` hands
-- them an empty list, the platform meeting a person on their best day and
-- having nothing to give them.
--
-- Design has that hole untouched. Its 130 seeded briefs are all `draft`, and
-- 0239 says why in its own header: they are backlog notes written as portfolio
-- pieces for somebody with three years of practice, in French only. A person
-- who has just handed in their HELLO is not that person.
--
-- ## What makes one of these different from a seeded draft
--
-- 0615's five properties, kept:
--
--   * **It says what is out of scope.** The commonest way a beginner loses a
--     week is doing more than was asked.
--   * **It is one sitting.** `duration_minutes` is a hint and not a timer.
--   * **It has a next one.** The second chain in `challenge_prerequisites`.
--   * **It exists in both languages** from the first day.
--   * **It lands somewhere real.**
--
-- The fifth is the one that does not transpose. Every code exercise names a
-- repository, and there is no repository for a poster. The equivalent here is
-- the design work Skilluv actually needs: our icons, our empty states, our
-- motion, our error messages, our event poster, our screens. Real, ours to
-- give, and the same dogfooding the code ladder rests on. Nothing below is an
-- exercise thrown away; each one produces something the platform can use.
--
-- ## The reading is thin on purpose
--
-- Four of the six carry the repository and nothing else. 0616 deleted an
-- invented `discord.gg/skilluv` from this table and wrote the rule that
-- replaced it: a dead link in the first list a beginner is handed is worse
-- than an empty list, because it teaches them the guidance is decorative.
--
-- So only documentation whose URL is certain is written here. What a good
-- empty state or a good poster looks like is worth linking, and the links
-- worth linking are ones a design curator knows and I do not. They can add
-- them; the table is theirs to fill.

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
    'design', v.difficulty, 'solo', 'educational', v.minutes,
    v.reward, FALSE, TRUE, FALSE,
    FALSE, 'published', 'disclosure_required', NULL,
    (SELECT id FROM orientations WHERE slug = v.trade),
    (SELECT id FROM projects WHERE slug = 'skilluv-frontend'),
    (SELECT g.criteria FROM review_grids g
      WHERE g.domain = 'design' AND g.reviewer_group IS NULL LIMIT 1)
FROM (VALUES
    (
        'design-iconography', 1, 90, 10,
        'One icon for the Skilluv set',
        'Draw one icon the platform is missing. One, not a family.',
        E'The gesture: one small thing, finished.\n\n1. Find a place in the Skilluv interface where an icon is missing or is standing in for another. Name it.\n2. Draw one icon for it. Deliver an SVG, on the grid the rest of the set uses, and say which grid that is.\n3. Show it at 16px and at 24px. If it stops reading at 16, it is not finished.\n4. Say in one line what it means, and what you rejected.\n\nOut of scope: the whole family, the global icon style, and redrawing icons that already exist.\n\nWhat is read: whether it still reads at 16px, and whether it sits beside the existing icons without announcing itself as new.',
        'Une icône du set Skilluv',
        'Dessine une icône qui manque à la plateforme. Une, pas une famille.',
        E'Le geste : une petite chose, finie.\n\n1. Trouve un endroit de l''interface Skilluv où une icône manque, ou en remplace une autre faute de mieux. Nomme-le.\n2. Dessine une icône pour cet endroit. Rends un SVG, sur la grille qu''utilise le reste du set, et dis laquelle.\n3. Montre-la en 16px et en 24px. Si elle cesse de se lire à 16, elle n''est pas finie.\n4. Dis en une ligne ce qu''elle signifie, et ce que tu as écarté.\n\nHors périmètre : la famille entière, le style global du set, et redessiner des icônes qui existent déjà.\n\nCe qui est lu : si elle se lit encore à 16px, et si elle tient à côté des icônes existantes sans s''annoncer comme nouvelle.'
    ),
    (
        'design-product', 2, 120, 15,
        'The screen when there is nothing yet',
        'Design the empty state a designer sees before they have handed in anything.',
        E'The gesture: the screen where somebody has done nothing is the screen that decides whether they do something.\n\n1. Take the Skilluv profile of a person who has just arrived: no deliverable, no attestation, no badge.\n2. Design what they see. The text is part of the deliverable, not a placeholder: write it.\n3. There is exactly one next action on that screen. Say which, and why the others were cut.\n4. Show the same screen in French and in English. If your layout only holds in one, it does not hold.\n\nOut of scope: the rest of the profile, the navigation, and any state where the person has already delivered something.\n\nWhat is read: whether the screen reads as a beginning rather than as a failure, and whether the single action survives the longer of the two languages.',
        'L''écran quand il n''y a encore rien',
        'Dessine l''état vide qu''un designer voit avant d''avoir rendu quoi que ce soit.',
        E'Le geste : l''écran où quelqu''un n''a rien fait est l''écran qui décide s''il fera quelque chose.\n\n1. Prends le profil Skilluv d''une personne qui vient d''arriver : aucun rendu, aucune attestation, aucun badge.\n2. Dessine ce qu''elle voit. Le texte fait partie du rendu, ce n''est pas du faux texte : écris-le.\n3. Il y a exactement une action suivante sur cet écran. Dis laquelle, et pourquoi les autres ont été coupées.\n4. Montre le même écran en français et en anglais. Si ta mise en page ne tient que dans une des deux, elle ne tient pas.\n\nHors périmètre : le reste du profil, la navigation, et tout état où la personne a déjà rendu quelque chose.\n\nCe qui est lu : si l''écran se lit comme un début et non comme un échec, et si l''action unique survit à la plus longue des deux langues.'
    ),
    (
        'design-ux-writing', 2, 90, 15,
        'Three error messages somebody can act on',
        'Rewrite three messages the platform shows when something fails.',
        E'The gesture: an error is a message to a person having a bad day. Not a pixel is drawn here.\n\n1. Find three messages the Skilluv interface shows when something goes wrong. Copy them as they are.\n2. Rewrite each so it names what happened, why, and the next action. No apology, no blame, no error code standing in for a sentence.\n3. Check each one leaks nothing: whether an account exists, an internal path, a field somebody could probe.\n4. Give the French and the English of all three. They are two writings, not one and a translation.\n\nOut of scope: changing what the platform does, inventing errors it does not have, and touching the visual design of the message.\n\nWhat is read: whether somebody who has never seen the product knows what to do next. And the leak check: a message that helps the caller and tells an attacker which addresses are registered is worse than the one it replaced.',
        'Trois messages d''erreur sur lesquels on peut agir',
        'Réécris trois messages que la plateforme affiche quand quelque chose échoue.',
        E'Le geste : une erreur est un message à quelqu''un qui passe une mauvaise journée. Aucun pixel n''est dessiné ici.\n\n1. Trouve trois messages que l''interface Skilluv affiche quand quelque chose ne va pas. Recopie-les tels quels.\n2. Réécris chacun pour qu''il nomme ce qui s''est passé, pourquoi, et l''action suivante. Pas d''excuse, pas de reproche, pas de code d''erreur à la place d''une phrase.\n3. Vérifie que chacun ne fuite rien : l''existence d''un compte, un chemin interne, un champ qu''on pourrait sonder.\n4. Donne le français et l''anglais des trois. Ce sont deux écritures, pas une et sa traduction.\n\nHors périmètre : changer ce que fait la plateforme, inventer des erreurs qu''elle n''a pas, et toucher au design visuel du message.\n\nCe qui est lu : si quelqu''un qui n''a jamais vu le produit sait quoi faire ensuite. Et la vérification de fuite : un message qui aide l''appelant et dit à un attaquant quelles adresses sont enregistrées est pire que celui qu''il remplace.'
    ),
    (
        'design-motion-ui', 2, 150, 20,
        'Ten seconds of motion, inside a budget',
        'Animate the Skilluv mark for ten seconds, under a weight you announce first.',
        E'The gesture: motion that a slow connection can afford.\n\n1. Announce your ceiling before you start: a file weight, in kilobytes. Write it down.\n2. Animate the Skilluv mark. Ten seconds, no more, and it has to loop without a visible seam.\n3. Deliver it in a form a browser can play, and say what you delivered and why that form.\n4. Honour `prefers-reduced-motion`: say what somebody who has asked for less motion sees instead. A still frame is a valid answer; nothing at all is not.\n5. State the final weight beside the ceiling you announced. Missing your own ceiling and saying so passes.\n\nOut of scope: redrawing the mark, sound, and anything longer than ten seconds.\n\nWhat is read: the loop seam, the weight against the ceiling you set yourself, and the reduced motion answer. A beautiful ten seconds that ignores somebody who gets sick from motion has not finished.',
        'Dix secondes de motion, dans un budget',
        'Anime la marque Skilluv sur dix secondes, sous un poids que tu annonces d''abord.',
        E'Le geste : du motion qu''une connexion lente peut se payer.\n\n1. Annonce ton plafond avant de commencer : un poids de fichier, en kilo-octets. Écris-le.\n2. Anime la marque Skilluv. Dix secondes, pas plus, et la boucle ne doit pas montrer sa couture.\n3. Rends-la dans une forme qu''un navigateur sait jouer, et dis laquelle et pourquoi celle-là.\n4. Respecte `prefers-reduced-motion` : dis ce que voit quelqu''un qui a demandé moins d''animation. Une image fixe est une réponse valable ; rien du tout n''en est pas une.\n5. Donne le poids final à côté du plafond annoncé. Rater son propre plafond et le dire, ça passe.\n\nHors périmètre : redessiner la marque, le son, et tout ce qui dépasse dix secondes.\n\nCe qui est lu : la couture de la boucle, le poids face au plafond que tu t''es fixé, et la réponse au mouvement réduit. Dix belles secondes qui ignorent quelqu''un que le mouvement rend malade n''ont pas fini.'
    ),
    (
        'design-marketing', 3, 120, 20,
        'A poster for one Skilluv event',
        'One poster, one format, one version. Announcing something that really happens.',
        E'The gesture: one piece, decided, not three options for somebody else to choose from.\n\n1. Take one thing Skilluv really does: a Discord session, a domain opening, a review evening. Ask if you do not know which.\n2. Design one poster for it. One format, chosen by you and stated: screen, print, or a story on a phone.\n3. Everything a reader needs is on it and nothing else: what, when, where, and what to do about it.\n4. Deliver one version. Not three, not a series. Choosing is the exercise.\n5. Name the fonts and images you used and the licence each is used under.\n\nOut of scope: a campaign, several formats, and a brand identity.\n\nWhat is read: whether somebody who does not know Skilluv understands what is being announced in the time it takes to walk past. And the licences, which are part of the grid and not a formality.',
        'L''affiche d''un événement Skilluv',
        'Une affiche, un format, une version. Pour annoncer quelque chose qui a vraiment lieu.',
        E'Le geste : une pièce, tranchée, pas trois propositions à faire choisir par quelqu''un d''autre.\n\n1. Prends une chose que Skilluv fait vraiment : une session Discord, l''ouverture d''un domaine, une soirée de revue. Demande si tu ne sais pas laquelle.\n2. Dessine une affiche pour cet événement. Un format, choisi par toi et annoncé : écran, impression, ou story sur téléphone.\n3. Tout ce dont un lecteur a besoin y est, et rien d''autre : quoi, quand, où, et quoi faire ensuite.\n4. Rends une version. Pas trois, pas une série. Trancher est l''exercice.\n5. Nomme les polices et les images utilisées, et sous quelle licence chacune est utilisée.\n\nHors périmètre : une campagne, plusieurs formats, et une identité de marque.\n\nCe qui est lu : si quelqu''un qui ne connaît pas Skilluv comprend ce qui est annoncé dans le temps qu''il met à passer devant. Et les licences, qui font partie de la grille et pas de la politesse.'
    ),
    (
        'design-product', 3, 180, 25,
        'One screen redrawn, and why',
        'Take a screen the platform already has and redraw it, saying what each choice serves.',
        E'The gesture: the one that used to be the entry rite, and that belongs here instead, after five smaller pieces.\n\n1. Pick one screen of Skilluv that exists today. Screenshot it as it is.\n2. Say what is wrong with it in three sentences. Not "it is ugly": what a person cannot do, or does slowly, or does wrong.\n3. Redraw it. Same content, same constraints, the layout is yours.\n4. Beside each choice, one line on what it serves. A choice you cannot explain is a choice to remove.\n5. Show the before and the after side by side.\n\nOut of scope: the rest of the product, a design system, and adding features the screen does not have.\n\nWhat is read: the three sentences of step 2 first. A redraw that is prettier and fixes nothing somebody could name is decoration, and it is the failure this exercise is placed sixth to avoid.',
        'Un écran redessiné, et pourquoi',
        'Prends un écran que la plateforme a déjà et redessine-le, en disant ce que chaque choix sert.',
        E'Le geste : celui qui servait de rite d''entrée, et qui a sa place ici à la place, après cinq pièces plus petites.\n\n1. Choisis un écran de Skilluv qui existe aujourd''hui. Fais-en une capture telle quelle.\n2. Dis ce qui ne va pas, en trois phrases. Pas « c''est moche » : ce qu''une personne ne peut pas faire, ou fait lentement, ou fait de travers.\n3. Redessine-le. Même contenu, mêmes contraintes, la mise en page est la tienne.\n4. À côté de chaque choix, une ligne sur ce qu''il sert. Un choix que tu ne peux pas expliquer est un choix à enlever.\n5. Montre l''avant et l''après côte à côte.\n\nHors périmètre : le reste du produit, un design system, et ajouter des fonctionnalités que l''écran n''a pas.\n\nCe qui est lu : les trois phrases de l''étape 2 d''abord. Un redessin plus joli qui ne corrige rien que quelqu''un puisse nommer est de la décoration, et c''est l''échec que cet exercice évite en arrivant en sixième.'
    )
) AS v(
    trade, difficulty, minutes, reward,
    title_en, description_en, instructions_en,
    title_fr, description_fr, instructions_fr
)
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- 2. The order
-- ═══════════════════════════════════════════════════════════════════
--
-- `required = FALSE` throughout, as in 0615: recommended and not enforced.
-- Somebody who already writes interface copy should not have to draw an icon
-- first, and `check_eligibility` only blocks on required edges. The chain is
-- advice the recommendation engine reads, and advice is what a ladder is for.
--
-- It branches once and rejoins, which the code chain also does. The icon is
-- the smallest finishable thing and comes first. Writing branches off it
-- immediately, because a UX writer should not be made to draw before they
-- write. The redraw sits last and takes both parents: it is the exercise that
-- needs a screen and a sentence at the same time.

INSERT INTO challenge_prerequisites (challenge_id, depends_on_challenge_id, required)
SELECT c.id, d.id, FALSE
FROM (VALUES
    ('The screen when there is nothing yet',       'One icon for the Skilluv set'),
    ('Three error messages somebody can act on',   'One icon for the Skilluv set'),
    ('Ten seconds of motion, inside a budget',     'The screen when there is nothing yet'),
    ('A poster for one Skilluv event',             'The screen when there is nothing yet'),
    ('One screen redrawn, and why',                'The screen when there is nothing yet'),
    ('One screen redrawn, and why',                'Three error messages somebody can act on')
) AS v(child, parent)
JOIN challenge_templates c ON c.title = v.child
JOIN challenge_templates d ON d.title = v.parent
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- 3. Where to start reading
-- ═══════════════════════════════════════════════════════════════════
--
-- Sparse, and see the header for why. The repository is on every one of them
-- because it is where the thing being redesigned actually lives, and it is
-- the same URL the code ladder already uses. Beyond that, only pages whose
-- address is certain.

INSERT INTO challenge_resources
    (challenge_id, kind, title, url, language, summary, access_note, sort_order)
SELECT ct.id, v.kind, v.title, v.url, v.language, v.summary, v.access_note, v.sort_order
FROM (VALUES
    ('One icon for the Skilluv set', 'repository', 'skilluv-frontend',
     'https://github.com/skilluv/skilluv-frontend', 'en',
     'The interface this icon is drawn for.', '', 10),
    ('One icon for the Skilluv set', 'documentation', 'MDN, SVG',
     'https://developer.mozilla.org/en-US/docs/Web/SVG', 'en',
     'What the format can do, and what survives being scaled down.', 'Free.', 20),
    ('One icon for the Skilluv set', 'documentation', 'MDN, SVG (fr)',
     'https://developer.mozilla.org/fr/docs/Web/SVG', 'fr',
     'La même documentation en français.', 'Gratuit.', 25),
    ('One icon for the Skilluv set', 'documentation', 'Material Symbols',
     'https://fonts.google.com/icons', 'en',
     'A large set to look at for grid and weight decisions. To read, not to copy.', 'Free.', 30),

    ('The screen when there is nothing yet', 'repository', 'skilluv-frontend',
     'https://github.com/skilluv/skilluv-frontend', 'en',
     'The profile screen this exercise fills.', '', 10),
    ('The screen when there is nothing yet', 'documentation', 'WCAG 2.2, contrast minimum',
     'https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html', 'en',
     'The contrast floor the review grid asks about, and how it is measured.', 'Free.', 20),

    ('Three error messages somebody can act on', 'repository', 'skilluv-frontend',
     'https://github.com/skilluv/skilluv-frontend', 'en',
     'Where the messages being rewritten are shown.', '', 10),

    ('Ten seconds of motion, inside a budget', 'repository', 'skilluv-frontend',
     'https://github.com/skilluv/skilluv-frontend', 'en',
     'Where the mark and the pages it plays on live.', '', 10),
    ('Ten seconds of motion, inside a budget', 'documentation', 'MDN, prefers-reduced-motion',
     'https://developer.mozilla.org/en-US/docs/Web/CSS/@media/prefers-reduced-motion', 'en',
     'How somebody asks for less motion, and how a page hears them.', 'Free.', 20),
    ('Ten seconds of motion, inside a budget', 'documentation', 'MDN, prefers-reduced-motion (fr)',
     'https://developer.mozilla.org/fr/docs/Web/CSS/@media/prefers-reduced-motion', 'fr',
     'La même page en français.', 'Gratuit.', 25),

    ('A poster for one Skilluv event', 'repository', 'skilluv-frontend',
     'https://github.com/skilluv/skilluv-frontend', 'en',
     'Where the mark and the platform colours live.', '', 10),

    ('One screen redrawn, and why', 'repository', 'skilluv-frontend',
     'https://github.com/skilluv/skilluv-frontend', 'en',
     'The screens this exercise redraws.', '', 10),
    ('One screen redrawn, and why', 'documentation', 'WCAG 2.2, contrast minimum',
     'https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html', 'en',
     'The contrast floor the review grid asks about, and how it is measured.', 'Free.', 20)
) AS v(challenge_title, kind, title, url, language, summary, access_note, sort_order)
JOIN challenge_templates ct ON ct.title = v.challenge_title
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- 4. What has to be true afterwards
-- ═══════════════════════════════════════════════════════════════════

DO $guard$
DECLARE
    published BIGINT;
    orphaned  BIGINT;
    edges     BIGINT;
    invented  BIGINT;
BEGIN
    SELECT count(*) INTO published
      FROM challenge_templates
     WHERE skill_domain = 'design' AND status = 'published'
       AND is_training = TRUE AND is_domain_rite = FALSE;
    IF published <> 6 THEN
        RAISE EXCEPTION
            'expected six published design exercises, found %', published;
    END IF;

    -- Every one names a trade that exists. A NULL here would be a brief
    -- nobody is recommended, which is the silent half of the empty list this
    -- migration exists to fill.
    SELECT count(*) INTO orphaned
      FROM challenge_templates
     WHERE skill_domain = 'design' AND status = 'published'
       AND is_training = TRUE AND is_domain_rite = FALSE
       AND orientation_id IS NULL;
    IF orphaned > 0 THEN
        RAISE EXCEPTION
            '% design exercises name no trade', orphaned;
    END IF;

    SELECT count(*) INTO edges
      FROM challenge_prerequisites p
      JOIN challenge_templates c ON c.id = p.challenge_id
     WHERE c.skill_domain = 'design';
    IF edges <> 6 THEN
        RAISE EXCEPTION
            'expected six edges in the design chain, found %', edges;
    END IF;

    -- The rule 0616 wrote after an invented Discord invite shipped here.
    SELECT count(*) INTO invented
      FROM challenge_resources r
      JOIN challenge_templates c ON c.id = r.challenge_id
     WHERE c.skill_domain = 'design'
       AND r.url LIKE '%discord.gg%';
    IF invented > 0 THEN
        RAISE EXCEPTION
            '% design resources point at an invite nobody has minted', invented;
    END IF;
END $guard$;
