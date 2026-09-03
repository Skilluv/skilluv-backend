-- Every challenge is filed under the language it is actually written in.
--
-- ## What was wrong
--
-- Migration 0104 added `title_i18n`, `description_i18n` and
-- `instructions_i18n` as `{locale: text}` and backfilled everything into `fr`,
-- because in 2026 the whole catalogue was French. Then:
--
--   * nothing ever read those columns — no route, and not even the
--     `ChallengeTemplate` struct, which does not carry them. The API serves the
--     plain `title` / `description` / `instructions`;
--   * `admin.rs` writes `{"fr": title}` hard-coded on every create, so a
--     curator writing in English files it as French;
--   * and the catalogue stopped being French. 0453, 0465, 0512, 0528, 0558 and
--     0586 seeded 252 challenges in English, into the same plain column, with
--     nothing saying so.
--
-- So the table holds two languages in one column with no marker. A bilingual
-- front cannot use that: it cannot know what it is about to display. Nothing
-- fails — it just shows the wrong language, which is why this survived.
--
-- ## What this does
--
-- Files each row under the language its author wrote it in, by the migration
-- that seeded it. The domain is a reliable proxy because each catalogue was
-- seeded by one migration in one language:
--
--   French  — code (0185), ai (0219), design (0239), audio (0417/0423),
--             ops (0425/0430/0432)
--   English — quality (0453/0459), leadership (0465/0469),
--             communication (0512/0515), education (0528/0529),
--             security (0558), game (0586)
--
-- The twelve rites of 0607 are English and get a French translation here, so
-- the one page every new account sees is bilingual on the day this lands
-- rather than on the day somebody remembers.
--
-- ## Why the plain columns stay
--
-- They stay as the base text and the fallback: `localise` overwrites them from
-- `*_i18n` when the asked-for locale is there and leaves them alone otherwise.
-- A locale nobody translated falls back to something readable instead of to an
-- empty string. 0104 planned to drop them "once the code reads the i18n
-- columns"; the code reads them from this migration on, and dropping them
-- would remove the fallback, so they stay on purpose and not by omission.

-- ═══════════════════════════════════════════════════════════════════
-- 1. The French catalogues
-- ═══════════════════════════════════════════════════════════════════

UPDATE challenge_templates
SET title_i18n        = jsonb_build_object('fr', title),
    description_i18n  = jsonb_build_object('fr', description),
    instructions_i18n = jsonb_build_object('fr', instructions)
WHERE skill_domain IN ('code', 'ai', 'design', 'audio', 'ops')
  AND NOT is_domain_rite;

-- ═══════════════════════════════════════════════════════════════════
-- 2. The English catalogues
-- ═══════════════════════════════════════════════════════════════════

UPDATE challenge_templates
SET title_i18n        = jsonb_build_object('en', title),
    description_i18n  = jsonb_build_object('en', description),
    instructions_i18n = jsonb_build_object('en', instructions)
WHERE skill_domain IN ('quality', 'leadership', 'communication', 'education',
                       'security', 'game')
  AND NOT is_domain_rite;

-- The fifteen per-starter Bonjour Skilluv variants seeded by
-- `services::seed` already carry both keys; they are left alone.

-- ═══════════════════════════════════════════════════════════════════
-- 3. The twelve rites, in both languages
-- ═══════════════════════════════════════════════════════════════════

UPDATE challenge_templates ct
SET title_i18n = jsonb_build_object('en', ct.title, 'fr', v.title_fr),
    description_i18n = jsonb_build_object('en', ct.description, 'fr', v.description_fr),
    instructions_i18n = jsonb_build_object('en', ct.instructions, 'fr', v.instructions_fr)
FROM (VALUES
    ('code',
     'Bonjour Skilluv — le premier commit',
     'Forke un starter Skilluv, présente-toi dans HELLO.md, et ouvre la pull request.',
     E'Le geste : une pull request sur un dépôt qui est le tien.\n\n1. Lance le rite — la plateforme forke un `skilluv-community/starter-*` sur ton compte GitHub.\n2. Clone-le et édite `HELLO.md` : qui tu es, ce que tu veux construire, ce que tu sais déjà. (Pas de git en local ? Édite le fichier directement sur GitHub, ça marche pareil.)\n3. Commit, push, et ouvre une pull request de `main` vers `showcase` sur ton propre fork.\n\nCe qui est lu : la pull request elle-même. Pas sa longueur — le fait que quelqu''un qui arrive sur ton fork comprenne ce que tu viens faire.'),
    ('design',
     'Bonjour Skilluv — le premier écran',
     'Un écran contre un brief court, déposé, et lu par trois relecteurs.',
     E'Le geste : un écran, assez fini pour qu''on puisse le discuter.\n\n1. Le brief, et c''est tout le brief : un écran qu''on utilise une fois et qu''on ne devrait jamais avoir à réutiliser — une connexion, un premier lancement, une confirmation. Choisis lequel. Il ne dit pas à quoi l''écran ressemble ; cette partie est la tienne.\n2. Dessine un écran contre lui. Un. Un parcours de six demi-écrans n''est pas ce rite.\n3. Dépose-le, avec les deux ou trois phrases qui disent quelle décision chaque choix sert.\n\nCe qui est lu : l''accord entre le brief et l''écran. Un bel écran qui répond à un autre brief ne passe pas, et c''est toute la leçon.'),
    ('game',
     'Bonjour Skilluv — le premier playtest',
     'Joue une tranche d''un jeu Skilluv et rends un verdict de playtest exploitable.',
     E'Le geste : un verdict, pas un avis.\n\n1. Joue une tranche publiée, du début à la fin, au moins une fois.\n2. Écris le verdict : ce que la tranche t''a appris sans te le dire, où tu as bloqué et combien de temps, et le premier changement que tu ferais.\n3. Dis ce que tu n''as pas testé, pour que le lecteur suivant connaisse les bords de ton rapport.\n\nCe qui est lu : si l''auteur de la tranche peut faire quelque chose de ton verdict demain matin.'),
    ('security',
     'Bonjour Skilluv — le premier constat',
     'Lis le périmètre publié, teste uniquement dedans, et remonte un constat.',
     E'Le geste : un constat, dans le périmètre, écrit pour être reproduit.\n\n1. Lis le périmètre publié du programme de divulgation Skilluv. Il dit ce qui est dedans, et surtout ce qui n''y est pas.\n2. Ne teste que ce que le périmètre nomme. Un constat contre autre chose est refusé, aussi réel soit-il — cette règle est le métier.\n3. Remonte-le : ce que tu as fait, ce qui s''est passé, pourquoi ça compte, et ce que tu changerais.\n\nCe qui est lu : la reproductibilité. Un constat que personne ne peut reproduire à partir de ton texte n''est pas encore un constat.'),
    ('ops',
     'Bonjour Skilluv — la première lecture de SLO',
     'Lis un SLO du terrain ops Skilluv et propose une amélioration.',
     E'Le geste : lire la production avant d''y toucher.\n\n1. Ouvre le terrain ops et choisis un SLO. Lis ce qu''il promet, ce qu''il mesure, et quel est son budget d''erreur.\n2. Dis ce qu''il ne rattrape pas. Tout SLO rate quelque chose ; le nommer est la compétence.\n3. Propose un changement — de l''objectif, de la mesure, ou de ce qui se passe quand le budget brûle — et dis ce qu''il coûte.\n\nCe qui est lu : si la proposition survit à son propre arbitrage. « Ajouter des alertes » n''est pas une proposition.'),
    ('ai',
     'Bonjour Skilluv — la première étape de workspace',
     'Prends la première étape de workspace d''une mission d''entrée, et montre ce que tu as vérifié.',
     E'Le geste : une étape, et la preuve derrière.\n\n1. Ouvre une mission d''entrée et prends sa première étape de workspace.\n2. Fais le travail — et note ce que tu as vérifié : ce que tu as lancé, ce qui est revenu, ce que tu as rejeté et pourquoi.\n3. Dis ce dont tu n''es pas sûr. Une étape qui affiche une certitude qu''elle n''a pas est le mode de défaillance de ce métier.\n\nCe qui est lu : la vérification, pas la sortie. Une sortie que personne n''a vérifiée ne prouve rien.'),
    ('soft_skills',
     'Bonjour Skilluv — la première revue',
     'Relis un livrable public : ce qui tient, ce qu''il faut changer, et pourquoi.',
     E'Le geste : une revue dont l''auteur est content.\n\n1. Choisis un livrable public et lis-le pour de vrai — en entier, avant d''écrire un mot.\n2. Dis ce qui tient, et pourquoi ça tient. Une revue qui ne liste que des problèmes n''apprend rien sur ce qu''il faut garder.\n3. Dis ce que tu changerais, dans quel ordre, et ce dont tu n''es pas sûr.\n\nCe qui est lu : la précision et le ton ensemble. « C''est bien » et « c''est faux » échouent pour la même raison — aucun des deux n''est actionnable.'),
    ('audio',
     'Bonjour Skilluv — la première signature',
     'Vingt secondes de son, chaque source déclarée.',
     E'Le geste : vingt secondes dont tu peux rendre compte entièrement.\n\n1. Fais une signature de vingt secondes — une identité, un sting, une texture. Court exprès : vingt secondes ne cachent rien.\n2. Déclare chaque source : ce que tu as enregistré, ce que tu as synthétisé, ce que tu as samplé et sous quelle licence.\n3. Dis ce que tu corrigerais avec une heure de plus.\n\nCe qui est lu : le son, et l''honnêteté de la liste des sources. Un sample non déclaré met fin au rite, quel que soit le résultat.'),
    ('quality',
     'Bonjour Skilluv — le premier rapport de défaut',
     'Dépose un rapport de défaut sur le canvas Skilluv qui n''appelle aucune question.',
     E'Le geste : un rapport qui n''appelle aucune question de suivi.\n\n1. Utilise le canvas Skilluv comme un vrai utilisateur, et trouve une chose qui ne va pas.\n2. Écris-la : ce que tu as fait, étape par étape ; ce que tu attendais ; ce qui s''est passé à la place ; où, et sur quoi.\n3. Dis à quel point tu es sûr, et ce qui te donnerait tort.\n\nCe qui est lu : si quelqu''un qui n''a jamais vu ton écran peut le reproduire à partir de ton texte seul.'),
    ('leadership',
     'Bonjour Skilluv — la première rétro',
     'Écris une rétro sur un incident : ce qui s''est passé, ce que ça a coûté, ce qui change.',
     E'Le geste : une rétro qui nomme des causes, pas des personnes.\n\n1. Choisis un incident que tu peux lire de bout en bout — un que tu as vécu, ou n''importe quel post-mortem public. Nomme-le, et mets le lien s''il est public.\n2. Écris la rétro : ce qui s''est passé, ce que ça a coûté, ce qui l''a rendu possible, et ce qui l''a arrêté.\n3. Propose un changement, avec un porteur et un moyen de savoir s''il a marché.\n\nCe qui est lu : si le changement que tu proposes aurait réellement empêché cet incident. Le blâme n''est pas une cause, et « faire plus attention » n''est pas un changement.'),
    ('communication',
     'Bonjour Skilluv — la première traduction',
     'Traduis un paragraphe de guide, et défends les choix qui ne sont pas littéraux.',
     E'Le geste : un paragraphe, porté entier.\n\n1. Choisis un paragraphe d''un guide publié, dans un couple de langues que tu pratiques vraiment.\n2. Traduis-le pour qu''un lecteur dans la langue cible reçoive ce que reçoit le lecteur d''origine — pas les mêmes mots, la même compréhension.\n3. Note les deux ou trois endroits où tu n''as pas traduit littéralement, et dis pourquoi.\n\nCe qui est lu : les notes autant que le texte. Une traduction dont l''auteur ne sait pas expliquer ses écarts n''était pas une traduction.'),
    ('education',
     'Bonjour Skilluv — la première explication',
     'Explique un nœud de compétence en trois temps, à quelqu''un qui ne l''a pas encore.',
     E'Le geste : trois temps, dans l''ordre, pour un vrai débutant.\n\n1. Choisis un nœud de compétence dans l''arbre — un que tu as, et dont tu te souviens de ne pas l''avoir eu.\n2. Explique-le en trois temps : quel problème il résout, le plus petit exemple qui le montre, et l''erreur que tout le monde fait en premier.\n3. Écris-le pour quelqu''un qui n''a pas le prérequis que tu t''apprêtes à utiliser — ou nomme le prérequis.\n\nCe qui est lu : si un débutant en sort sans s''arrêter. Correct et illisible ne passe pas.')
) AS v(skill_domain, title_fr, description_fr, instructions_fr)
WHERE ct.is_domain_rite
  AND ct.skill_domain = v.skill_domain;

-- ═══════════════════════════════════════════════════════════════════
-- 4. Nothing is left unfiled
-- ═══════════════════════════════════════════════════════════════════

-- A row whose `title_i18n` is still `{}` would be served by the fallback
-- forever without anybody noticing, which is the state this migration exists
-- to end. Refused loudly instead.
DO $$
DECLARE
    unfiled BIGINT;
BEGIN
    SELECT count(*) INTO unfiled
      FROM challenge_templates
     WHERE title_i18n = '{}'::jsonb;
    IF unfiled > 0 THEN
        RAISE EXCEPTION
            '% challenge(s) carry no language. Add the domain to one of the '
            'two lists above rather than letting them fall through.', unfiled;
    END IF;
END $$;
