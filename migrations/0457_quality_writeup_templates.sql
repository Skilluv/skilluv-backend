-- Ten quality write-up templates, in two languages.
--
-- In this domain the document *is* the artefact. A bug report nobody can
-- reproduce has not been written, however real the defect behind it; a study
-- whose protocol does not support its conclusion is an opinion with a sample
-- size. These are the skeletons, with the fields people skip marked as what
-- they are.
--
-- ## The field every one of them shares
--
-- What was **not** covered. It appears under different names — the omissions,
-- the scope holes, what the session did not reach, the false positives — and
-- it is always the field that separates a document a reviewer can trust from
-- one they have to take on faith. A report that lets a reader assume full
-- coverage is more dangerous than no report at all, and every template here
-- forces the question.
--
-- ## Why `rules-of-engagement` is a template and not a legal annex
--
-- Because it is the one that gets skipped, and skipping it is the single
-- refusal this domain applies without discussion. Making it a document
-- somebody fills in — rather than a paragraph in a charter nobody opens —
-- is the only version of it that gets written.

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary,
     body_md, sort_order)
VALUES

('quality-template-bug-report', 'writeup_template', 'quality', 'automation', 'en',
 'Defect report',
 'Written for somebody who has never seen the system and has to see the defect anyway.',
$md$
# {one line: what breaks, and where}

## Environment
| | |
|---|---|
| OS / version | |
| Browser or client / version | |
| Application build or commit | |
| Account or role used | |
| Network conditions, if relevant | |

## Steps to reproduce
1.
2.
3.

> Written for somebody who has never opened this system. If a step assumes
> knowledge you have, the report is not reproducible — it only looks like it.

## Expected
{what should have happened, and where that expectation comes from: a spec, a
documented behaviour, a consistent behaviour elsewhere in the product}

## Observed
{what happened, factually. No interpretation here.}

## Reproducibility
{always / often / sometimes / rare / once} — over {n} attempts.

> Kept separate from severity on purpose. A crash that happens once in a
> thousand runs and a cosmetic glitch that happens every time are both "one
> defect" without this field.

## Severity, and the argument for it
{critical / high / medium / low}

{what the user loses. Not a tool score. "An unauthenticated visitor reads
another account's invoices" is an argument; "CVSS 7.5" is not.}

## Attachments
- Screenshot / recording:
- Relevant log excerpt:

## What this report does not cover
{the adjacent things you did not check. Whether it affects other browsers,
other roles, the mobile client. Saying "not checked" costs nothing and saves
the reader from assuming you did.}

## Fix
| | |
|---|---|
| Fix link | |
| Re-checked on | |
| Re-checked by | |

> The last two lines are what turns this report into a proof. A merged fix is
> somebody else's claim that it is fixed; going back and looking is yours.
$md$, 110),

('quality-template-bug-report', 'writeup_template', 'quality', 'automation', 'fr',
 'Rapport d''anomalie',
 'Écrit pour quelqu''un qui n''a jamais vu le système et doit quand même voir l''anomalie.',
$md$
# {une ligne : ce qui casse, et où}

## Environnement
| | |
|---|---|
| Système / version | |
| Navigateur ou client / version | |
| Version applicative ou commit | |
| Compte ou rôle utilisé | |
| Conditions réseau, si pertinent | |

## Étapes de reproduction
1.
2.
3.

> Écrites pour quelqu''un qui n''a jamais ouvert ce système. Si une étape
> suppose une connaissance que tu as, le rapport n''est pas reproductible — il
> en a seulement l''air.

## Attendu
{ce qui aurait dû se passer, et d''où vient cette attente : une spécification,
un comportement documenté, un comportement cohérent ailleurs dans le produit}

## Observé
{ce qui s''est passé, factuellement. Aucune interprétation ici.}

## Reproductibilité
{toujours / souvent / parfois / rare / une fois} — sur {n} tentatives.

> Séparé de la gravité à dessein. Un plantage qui arrive une fois sur mille et
> un défaut cosmétique qui arrive à chaque fois sont tous les deux « une
> anomalie » sans ce champ.

## Gravité, et son argument
{critique / majeure / moyenne / mineure}

{ce que l''utilisateur perd. Pas un score d''outil. « Un visiteur non
authentifié lit les factures d''un autre compte » est un argument ; « CVSS
7.5 » n''en est pas un.}

## Pièces jointes
- Capture / enregistrement :
- Extrait de journal pertinent :

## Ce que ce rapport ne couvre pas
{les choses voisines que tu n''as pas vérifiées. Si ça touche d''autres
navigateurs, d''autres rôles, le client mobile. Dire « non vérifié » ne coûte
rien et évite au lecteur de supposer que tu l''as fait.}

## Correctif
| | |
|---|---|
| Lien du correctif | |
| Revérifié le | |
| Revérifié par | |

> Les deux dernières lignes sont ce qui transforme ce rapport en preuve. Un
> correctif fusionné est l''affirmation de quelqu''un d''autre ; retourner voir
> est la tienne.
$md$, 110),

('quality-template-test-plan', 'writeup_template', 'quality', 'automation', 'en',
 'Feature test plan',
 'What will be covered, at which level, and what will deliberately not be.',
$md$
# Test plan — {feature}

## What this feature is supposed to do
{two or three sentences, in the words of somebody who will use it}

## What can go wrong
| Failure | What it costs | Likelihood |
|---|---|---|
| | | |

> This table comes before the test list, not after. A plan written from the
> tests outwards covers what is easy to cover.

## What will be covered
| Risk | Level | Why this level |
|---|---|---|
| | unit / integration / end-to-end / manual | |

> "Why this level" is the column reviewers read. An end-to-end test where an
> integration test would do costs the team ninety seconds on every merge,
> forever.

## What will NOT be covered
| Not covered | Risk accepted | Accepted by |
|---|---|---|
| | | |

> The section that makes this a plan rather than a wish list. A plan with an
> empty table here has decided nothing.

## Test data
{what each test needs, and how it produces it. If the answer is "the shared
database already has it", say so — it is an invisible dependency and the
reviewer will find it eventually.}

## Cost
- Estimated run time added to the pipeline:
- Estimated authoring time:
- Ongoing maintenance expected:

## When this plan is wrong
{what would have to change for it to need rewriting: a new client, a new
integration, a change of scale}
$md$, 120),

('quality-template-test-plan', 'writeup_template', 'quality', 'automation', 'fr',
 'Plan de test d''une fonctionnalité',
 'Ce qui sera couvert, à quel niveau, et ce qui ne le sera délibérément pas.',
$md$
# Plan de test — {fonctionnalité}

## Ce que cette fonctionnalité est censée faire
{deux ou trois phrases, dans les mots de quelqu''un qui va s''en servir}

## Ce qui peut mal tourner
| Défaillance | Ce qu''elle coûte | Vraisemblance |
|---|---|---|
| | | |

> Ce tableau vient avant la liste des tests, pas après. Un plan écrit à partir
> des tests couvre ce qui est facile à couvrir.

## Ce qui sera couvert
| Risque | Niveau | Pourquoi ce niveau |
|---|---|---|
| | unitaire / intégration / bout en bout / manuel | |

> « Pourquoi ce niveau » est la colonne que lisent les relecteurs. Un test de
> bout en bout là où un test d''intégration suffirait coûte à l''équipe
> quatre-vingt-dix secondes à chaque fusion, indéfiniment.

## Ce qui ne sera PAS couvert
| Non couvert | Risque accepté | Accepté par |
|---|---|---|
| | | |

> La section qui fait de ceci un plan et non une liste de souhaits. Un plan
> dont ce tableau est vide n''a rien décidé.

## Données de test
{ce dont chaque test a besoin, et comment il le produit. Si la réponse est
« la base partagée les a déjà », dis-le — c''est une dépendance invisible et le
relecteur finira par la trouver.}

## Coût
- Temps d''exécution ajouté à la chaîne :
- Temps de rédaction estimé :
- Maintenance attendue :

## Quand ce plan sera faux
{ce qui devrait changer pour qu''il faille le réécrire : un nouveau client, une
nouvelle intégration, un changement d''échelle}
$md$, 120),

('quality-template-test-strategy', 'writeup_template', 'quality', 'strategy', 'en',
 'Team test strategy',
 'A list of owned omissions, with what they cost and who accepted them.',
$md$
# Test strategy — {team or product}

## The three numbers
| | Today | Target |
|---|---|---|
| Suite run time, end to end | | |
| Share of last month's failures that were real | | |
| Time between "ready" and "merged" | | |

> The second number is the one nobody has. It is also the one that decides
> whether the team trusts its own tests, and a team that does not trust its
> tests has a ritual rather than a strategy.

## What we put to the test
| Level | What it covers | Who writes it | Who maintains it | Cost per merge |
|---|---|---|---|---|
| | | | | |

> "The team" is not an owner. Name a role.

## What we do not put to the test
| Not covered | Risk accepted | Accepted by | Revisit when |
|---|---|---|---|
| | | | |

## What we are removing
{tests being deleted, and why. A strategy that only adds is a strategy whose
cost grows without limit.}

## Culture
- What is being asked of people:
- What makes the new path easier than the old one:

> An imposed ritual empties out in six months. The ones that hold removed
> friction somewhere else.

## How we will know
{the indicator that will move if this works — and that can also move
downwards if it does not. An indicator that can only improve is a
scoreboard.}

## Survival
{what happens to this strategy if its author leaves. If the answer is "it
stops", it is a personal practice.}
$md$, 130),

('quality-template-test-strategy', 'writeup_template', 'quality', 'strategy', 'fr',
 'Stratégie de test d''équipe',
 'Une liste de renoncements assumés, avec leur coût et qui les a acceptés.',
$md$
# Stratégie de test — {équipe ou produit}

## Les trois chiffres
| | Aujourd''hui | Cible |
|---|---|---|
| Temps d''exécution de la suite, de bout en bout | | |
| Part des échecs du mois dernier qui étaient réels | | |
| Temps entre « prêt » et « fusionné » | | |

> Le deuxième chiffre est celui que personne n''a. C''est aussi celui qui décide
> si l''équipe fait confiance à ses propres tests, et une équipe qui ne leur
> fait pas confiance a un rituel plutôt qu''une stratégie.

## Ce que nous éprouvons
| Niveau | Ce que ça couvre | Qui l''écrit | Qui le maintient | Coût par fusion |
|---|---|---|---|---|
| | | | | |

> « L''équipe » n''est pas un porteur. Nomme un rôle.

## Ce que nous n''éprouvons pas
| Non couvert | Risque accepté | Accepté par | À revoir quand |
|---|---|---|---|
| | | | |

## Ce que nous retirons
{les tests supprimés, et pourquoi. Une stratégie qui n''ajoute que du test est
une stratégie dont le coût croît sans limite.}

## Culture
- Ce qu''on demande aux gens :
- Ce qui rend le nouveau chemin plus facile que l''ancien :

> Un rituel imposé se vide en six mois. Ceux qui tiennent ont retiré de la
> friction ailleurs.

## Comment nous le saurons
{l''indicateur qui bougera si ça marche — et qui peut aussi baisser si ça ne
marche pas. Un indicateur qui ne peut que s''améliorer est un tableau
d''affichage.}

## Survie
{ce qu''il advient de cette stratégie si son auteur part. Si la réponse est
« elle s''arrête », c''est une pratique personnelle.}
$md$, 130),

('quality-template-coverage-analysis', 'writeup_template', 'quality', 'strategy', 'en',
 'Coverage analysis',
 'Where the gaps are, which ones matter, and in what order to close them.',
$md$
# Coverage analysis — {project}

## The figure, and its source
| | |
|---|---|
| Line coverage | |
| Branch coverage | |
| Report link | |
| Commit measured | |

> A percentage with no report behind it is refused. The link is the analysis's
> only claim to being checkable.

## Why the figure is not the answer
{one paragraph. A module at 95% whose only uncovered branch is the error path
is worse covered than one at 60% whose gaps are logging.}

## The gaps that matter
| Uncovered | What it does | What happens if it is wrong | Cost to cover |
|---|---|---|---|
| | | | |

> Ranked by the third column, never by the size of the gap.

## The gaps that do not
{what is uncovered and should stay uncovered. Generated code, logging,
debug paths. Saying so stops the next person re-finding them.}

## The first one closed
{the gap you actually covered, and the test that covers it. An analysis with
no worked example is a list.}

## What this analysis does not tell you
{coverage measures what ran, not what was checked. A suite with no assertions
reaches 100%. Say whether you looked at that.}
$md$, 140),

('quality-template-coverage-analysis', 'writeup_template', 'quality', 'strategy', 'fr',
 'Analyse de couverture',
 'Où sont les trous, lesquels comptent, et dans quel ordre les combler.',
$md$
# Analyse de couverture — {projet}

## Le chiffre, et sa source
| | |
|---|---|
| Couverture de lignes | |
| Couverture de branches | |
| Lien du rapport | |
| Commit mesuré | |

> Un pourcentage sans son rapport est refusé. Le lien est la seule chose qui
> rende l''analyse vérifiable.

## Pourquoi le chiffre n''est pas la réponse
{un paragraphe. Un module à 95 % dont la seule branche non couverte est le
chemin d''erreur est moins bien couvert qu''un module à 60 % dont les trous sont
de la journalisation.}

## Les trous qui comptent
| Non couvert | Ce que ça fait | Ce qui arrive si c''est faux | Coût pour couvrir |
|---|---|---|---|
| | | | |

> Classés par la troisième colonne, jamais par la taille du trou.

## Les trous qui ne comptent pas
{ce qui n''est pas couvert et doit le rester. Code généré, journalisation,
chemins de débogage. Le dire évite au suivant de les retrouver.}

## Le premier comblé
{le trou que tu as réellement couvert, et le test qui le couvre. Une analyse
sans exemple traité est une liste.}

## Ce que cette analyse ne dit pas
{la couverture mesure ce qui a été exécuté, pas ce qui a été vérifié. Une
suite sans assertion atteint 100 %. Dis si tu as regardé ça.}
$md$, 140),

('quality-template-usability-study', 'writeup_template', 'quality', 'usability', 'en',
 'Usability study report',
 'Protocol, sessions, and what was seen kept apart from what is concluded.',
$md$
# Usability study — {product, journey}

## The question
{the one thing this study was run to find out. One sentence. A study with
three questions answers none of them.}

## Protocol
- Tasks given to the participant:
- What they were told beforehand:
- Session length:
- What was recorded, and the consent obtained:
- The closing questions, in the exact words used every time:

> Identical across sessions. Sessions run under different protocols do not
> add up.

## Participants
| # | Profile | Why this person | Recruited how |
|---|---|---|---|
| | | | |

> Five colleagues are five people who already know the product exists. If
> that is who you had, say so — it is a limitation, not a disqualification.

## What was observed
{facts. "Four of five clicked the logo to go back." "Participant 3 read the
error message aloud twice, then reloaded."}

### Raw quotes
> "

> "

{What the person said, not what they meant.}

## What is inferred
{your reading. A reader is entitled to disagree with this section while
keeping the one above.}

## What this study cannot conclude
{sample size, profile skew, the journeys not covered, the environment. The
section that stops somebody quoting this as proof of something it does not
show.}

## Recommendations
| Finding | Possible change | Cost | Confidence |
|---|---|---|---|
| | | | |
$md$, 150),

('quality-template-usability-study', 'writeup_template', 'quality', 'usability', 'fr',
 'Rapport d''étude d''utilisabilité',
 'Protocole, séances, et ce qui a été vu tenu à l''écart de ce qu''on en conclut.',
$md$
# Étude d''utilisabilité — {produit, parcours}

## La question
{la seule chose que cette étude a été menée pour découvrir. Une phrase. Une
étude avec trois questions n''en traite aucune.}

## Protocole
- Tâches données au participant :
- Ce qui lui a été dit avant :
- Durée de séance :
- Ce qui a été enregistré, et le consentement obtenu :
- Les questions de fin, dans les mots exacts utilisés à chaque fois :

> Identique d''une séance à l''autre. Des séances menées sous des protocoles
> différents ne s''additionnent pas.

## Participants
| n° | Profil | Pourquoi cette personne | Recruté comment |
|---|---|---|---|
| | | | |

> Cinq collègues sont cinq personnes qui savent déjà que le produit existe. Si
> c''est ce que tu avais, dis-le — c''est une limite, pas une disqualification.

## Ce qui a été observé
{des faits. « Quatre sur cinq ont cliqué sur le logo pour revenir. » « Le
participant 3 a lu le message d''erreur à voix haute deux fois, puis a
rechargé. »}

### Verbatims bruts
> «  »

> «  »

{Ce que la personne a dit, pas ce qu''elle voulait dire.}

## Ce qu''on en déduit
{ta lecture. Un lecteur a le droit de ne pas être d''accord avec cette section
tout en gardant celle du dessus.}

## Ce que cette étude ne peut pas conclure
{taille de l''échantillon, biais de profil, parcours non couverts,
environnement. La section qui empêche quelqu''un de citer ceci comme preuve de
quelque chose qu''elle ne montre pas.}

## Recommandations
| Constat | Changement possible | Coût | Confiance |
|---|---|---|---|
| | | | |
$md$, 150),

('quality-template-a11y-audit', 'writeup_template', 'quality', 'usability', 'en',
 'Accessibility audit',
 'Against a named standard and level, with a cost next to every fix.',
$md$
# Accessibility audit — {page or flow}

## Scope
| | |
|---|---|
| Standard and level | e.g. WCAG 2.2 AA |
| Pages or flows audited | |
| Date and build | |
| Tools used | |
| Assistive technology used, and version | |

> "Not accessible" is not a finding. Every defect below names its criterion.

## Method
- Automated pass: {tool, what it covered}
- Keyboard-only pass: {what was walked}
- Screen reader pass: {which one, how long}
- Zoom / reflow pass: {to what level}

> Automated tools find roughly a third of real issues. An audit that is only
> a tool report says so in this section rather than pretending otherwise.

## Defects
| Criterion | Where | What happens | Severity | Proposed fix | Est. cost |
|---|---|---|---|---|---|
| 1.4.3 | | | | | |

## What works
{the parts that are already right. A report that only lists failures gets
read once.}

## Not audited
{what was out of scope, and what that means a reader must not conclude.}

## Retest
| | |
|---|---|
| Fixes shipped | |
| Retested on | |
| Remaining | |
$md$, 160),

('quality-template-a11y-audit', 'writeup_template', 'quality', 'usability', 'fr',
 'Audit d''accessibilité',
 'Contre une norme et un niveau nommés, avec un coût à côté de chaque correctif.',
$md$
# Audit d''accessibilité — {page ou parcours}

## Périmètre
| | |
|---|---|
| Norme et niveau | ex. WCAG 2.2 AA |
| Pages ou parcours audités | |
| Date et version | |
| Outils utilisés | |
| Technologie d''assistance utilisée, et version | |

> « Pas accessible » n''est pas un constat. Chaque défaut ci-dessous nomme son
> critère.

## Méthode
- Passe automatique : {outil, ce qu''il a couvert}
- Passe clavier seul : {ce qui a été parcouru}
- Passe lecteur d''écran : {lequel, pendant combien de temps}
- Passe zoom / redistribution : {jusqu''à quel niveau}

> Les outils automatiques trouvent environ un tiers des vrais problèmes. Un
> audit qui n''est qu''un rapport d''outil le dit dans cette section plutôt que
> de faire semblant.

## Défauts
| Critère | Où | Ce qui se passe | Gravité | Correctif proposé | Coût est. |
|---|---|---|---|---|---|
| 1.4.3 | | | | | |

## Ce qui fonctionne
{les parties déjà correctes. Un rapport qui ne liste que des échecs se fait
lire une fois.}

## Non audité
{ce qui était hors périmètre, et ce qu''un lecteur ne doit donc pas en
conclure.}

## Retest
| | |
|---|---|
| Correctifs livrés | |
| Retesté le | |
| Restant | |
$md$, 160),

('quality-template-playtest-report', 'writeup_template', 'quality', 'playtest', 'en',
 'Playtest report',
 'What the players did, and the trade-offs the team can now choose between.',
$md$
# Playtest report — {game}, {n} sessions

## Protocol
- What players were asked to do:
- What they were told beforehand:
- Session length:
- Build tested:
- What was recorded, and consent:

> Identical across all {n} sessions. If one differed, it is reported
> separately below and not summed with the others.

## Players
| # | Familiar with the genre | Hours played before | Where recruited |
|---|---|---|---|
| | | | |

> The genre column changes how every observation reads. A report without it
> is not usable by the team.

## What happened
| Moment | Players affected | What they did |
|---|---|---|
| | /{n} | |

> Facts. "Re-read the tutorial three times" beats "found it confusing".

## Where I wanted to help
{the moments you had to stop yourself intervening. These are usually the
findings.}

## Balance data, if any
| Option | Matches | Win rate |
|---|---|---|
| | | |

> A win rate with no match count is not a measurement.

## Trade-offs for the team
| Finding | Option A (cost) | Option B (cost) | Option C (cost) |
|---|---|---|---|
| | | | |

> The team decides. A report that decides for them gets thanked and shelved.

## What these sessions did not reach
{content not played, modes not opened, the difficulty nobody got to.}
$md$, 170),

('quality-template-playtest-report', 'writeup_template', 'quality', 'playtest', 'fr',
 'Compte rendu de playtest',
 'Ce que les joueurs ont fait, et les arbitrages entre lesquels l''équipe peut maintenant choisir.',
$md$
# Compte rendu de playtest — {jeu}, {n} séances

## Protocole
- Ce qu''on a demandé aux joueurs :
- Ce qu''on leur a dit avant :
- Durée de séance :
- Version testée :
- Ce qui a été enregistré, et le consentement :

> Identique sur les {n} séances. Si l''une a différé, elle est rapportée
> séparément ci-dessous et non additionnée aux autres.

## Joueurs
| n° | Habitué du genre | Heures jouées avant | Recruté où |
|---|---|---|---|
| | | | |

> La colonne « genre » change la lecture de chaque observation. Un rapport
> sans elle n''est pas utilisable par l''équipe.

## Ce qui s''est passé
| Moment | Joueurs concernés | Ce qu''ils ont fait |
|---|---|---|
| | /{n} | |

> Des faits. « A relu le tutoriel trois fois » vaut mieux que « a trouvé ça
> confus ».

## Là où j''ai voulu aider
{les moments où tu as dû t''empêcher d''intervenir. Ce sont en général les
constats.}

## Données d''équilibrage, s''il y en a
| Option | Parties | Taux de victoire |
|---|---|---|
| | | |

> Un taux de victoire sans nombre de parties n''est pas une mesure.

## Arbitrages pour l''équipe
| Constat | Option A (coût) | Option B (coût) | Option C (coût) |
|---|---|---|---|
| | | | |

> L''équipe tranche. Un rapport qui tranche à sa place est remercié puis rangé.

## Ce que ces séances n''ont pas atteint
{contenu non joué, modes non ouverts, la difficulté que personne n''a
atteinte.}
$md$, 170),

('quality-template-pentest-report', 'writeup_template', 'quality', 'intrusion', 'en',
 'Penetration test report',
 'Scope, method, replayable findings, and the false positives you dismissed.',
$md$
# Penetration test report — {target}

## Scope
| | |
|---|---|
| Rules of engagement | {link — required} |
| In scope | |
| Explicitly out of scope | |
| Window tested | |
| Method followed | e.g. OWASP Testing Guide |

> No signed rules of engagement, no report. This is the one refusal applied
> without discussion.

## Summary for somebody who will not read the rest
{three sentences. What an attacker could do today, and what it would cost
them.}

## Findings
### {ID} — {title}
| | |
|---|---|
| Severity | |
| Affected | |
| Prerequisite for exploitation | |

**Reproduction**
```
{request / payload}
```
```
{response}
```

**What an attacker gets**
{the argument for the severity. Not a tool score.}

**Suggested remediation**

---

## False positives dismissed
| Flagged by | What it claimed | Why it is not one |
|---|---|---|
| | | |

> The section that separates a report from a scanner export.

## Not tested
{what the method covers that this engagement did not reach, and why: time,
scope, credentials unavailable. A report silent here reads as full coverage.}

## Disclosure
| | |
|---|---|
| Reported to | |
| On | |
| Agreed publication date | |
$md$, 180),

('quality-template-pentest-report', 'writeup_template', 'quality', 'intrusion', 'fr',
 'Rapport de test d''intrusion',
 'Périmètre, méthode, constats rejouables, et les faux positifs que tu as écartés.',
$md$
# Rapport de test d''intrusion — {cible}

## Périmètre
| | |
|---|---|
| Règles d''engagement | {lien — obligatoire} |
| Dans le périmètre | |
| Explicitement hors périmètre | |
| Fenêtre testée | |
| Méthode suivie | ex. OWASP Testing Guide |

> Pas de règles d''engagement signées, pas de rapport. C''est le seul refus
> appliqué sans discussion.

## Résumé pour qui ne lira pas la suite
{trois phrases. Ce qu''un attaquant pourrait faire aujourd''hui, et ce que ça
lui coûterait.}

## Constats
### {ID} — {titre}
| | |
|---|---|
| Gravité | |
| Concerné | |
| Prérequis d''exploitation | |

**Reproduction**
```
{requête / charge}
```
```
{réponse}
```

**Ce que l''attaquant obtient**
{l''argument de la gravité. Pas un score d''outil.}

**Remédiation suggérée**

---

## Faux positifs écartés
| Signalé par | Ce qu''il prétendait | Pourquoi ce n''en est pas un |
|---|---|---|
| | | |

> La section qui sépare un rapport d''un export de scanner.

## Non testé
{ce que la méthode couvre et que cette mission n''a pas atteint, et pourquoi :
temps, périmètre, identifiants indisponibles. Un rapport muet ici se lit comme
une couverture complète.}

## Divulgation
| | |
|---|---|
| Signalé à | |
| Le | |
| Date de publication convenue | |
$md$, 180),

('quality-template-rules-of-engagement', 'writeup_template', 'quality', 'intrusion', 'en',
 'Rules of engagement',
 'The document that has to exist before anything is touched.',
$md$
# Rules of engagement — {target}

## Parties
| | |
|---|---|
| System owner | |
| Tester | |
| Emergency contact, both sides | |

## Authorisation
{The owner's written statement that this test is authorised. A forwarded
email counts; a verbal "go ahead" does not.}

Signed: ______________  Date: __________

## In scope
- Domains / IP ranges:
- Applications and versions:
- Accounts provided:

## Out of scope
- {named explicitly. Everything not listed in "in scope" is out of scope by
  default, and this list is for the things somebody might reasonably have
  assumed were in.}

## Window
| | |
|---|---|
| Start | |
| End | |
| Hours permitted | |

## Techniques not permitted
- Denial of service: {yes / no}
- Social engineering of staff: {yes / no}
- Physical access: {yes / no}
- Testing against production data: {yes / no}
- Automated scanning rate limit:

## If something breaks
{who to call, how fast, and what the tester stops doing immediately}

## Handling of what is found
- Data accessed accidentally: {what the tester does — usually: stop, record
  the fact, do not download}
- Storage of evidence, and for how long:
- Deletion at the end of the engagement:

## Disclosure
| | |
|---|---|
| Report delivered by | |
| Remediation window | |
| Publication permitted after | |
| Anything never publishable | |
$md$, 190),

('quality-template-rules-of-engagement', 'writeup_template', 'quality', 'intrusion', 'fr',
 'Règles d''engagement',
 'Le document qui doit exister avant qu''on touche à quoi que ce soit.',
$md$
# Règles d''engagement — {cible}

## Parties
| | |
|---|---|
| Propriétaire du système | |
| Testeur | |
| Contact d''urgence, des deux côtés | |

## Autorisation
{La déclaration écrite du propriétaire autorisant ce test. Un courriel
transféré compte ; un « vas-y » oral non.}

Signé : ______________  Date : __________

## Dans le périmètre
- Domaines / plages d''adresses :
- Applications et versions :
- Comptes fournis :

## Hors périmètre
- {nommés explicitement. Tout ce qui n''est pas listé dans « dans le
  périmètre » en est exclu par défaut, et cette liste sert pour les choses
  qu''on aurait raisonnablement pu croire dedans.}

## Fenêtre
| | |
|---|---|
| Début | |
| Fin | |
| Heures autorisées | |

## Techniques non autorisées
- Déni de service : {oui / non}
- Ingénierie sociale du personnel : {oui / non}
- Accès physique : {oui / non}
- Test sur données de production : {oui / non}
- Limite de débit du balayage automatique :

## Si quelque chose casse
{qui appeler, en combien de temps, et ce que le testeur arrête immédiatement}

## Traitement de ce qui est trouvé
- Données consultées accidentellement : {ce que fait le testeur — en général :
  arrêter, consigner le fait, ne rien télécharger}
- Conservation des preuves, et pour combien de temps :
- Suppression à la fin de la mission :

## Divulgation
| | |
|---|---|
| Rapport remis avant le | |
| Fenêtre de remédiation | |
| Publication autorisée après | |
| Ce qui ne sera jamais publiable | |
$md$, 190),

('quality-template-escape-analysis', 'writeup_template', 'quality', 'strategy', 'en',
 'Escaped defect analysis',
 'A defect reached users. The question is not who missed it.',
$md$
# Escaped defect analysis — {defect}

## What reached users
| | |
|---|---|
| Defect | |
| Shipped on | |
| Found on | |
| Found by | {a user / support / monitoring / internal} |
| Impact | |

## Why the tests did not catch it
{what the system allowed. Not who wrote which test.}

Pick the honest one, or write a better one:

- There was no test for this path.
- There was a test, and it would have passed with the defect present.
- There was a test, and it was disabled or in a retry loop.
- The path is not testable in the current architecture.
- It was covered, and the environment differed from production in a way
  nobody had recorded.

## Which of our omissions this was
{if the test strategy listed this as a deliberate omission, say so and stop.
An accepted risk that materialised is not a failure of testing — it is the
risk being what it was. If it was *not* on the list, that is the finding.}

## What changes
| Action | Owner | By when |
|---|---|---|
| | | |

> An action item with no owner and no date does not exist. This is the table
> that decides whether the same defect ships twice.

## What deliberately does not change
{the fixes considered and rejected, with the reason. A post-mortem that adds
a control every time ends up with a process nobody follows.}
$md$, 200),

('quality-template-escape-analysis', 'writeup_template', 'quality', 'strategy', 'fr',
 'Analyse d''anomalie échappée',
 'Une anomalie a atteint les utilisateurs. La question n''est pas qui l''a ratée.',
$md$
# Analyse d''anomalie échappée — {anomalie}

## Ce qui a atteint les utilisateurs
| | |
|---|---|
| Anomalie | |
| Livrée le | |
| Trouvée le | |
| Trouvée par | {un utilisateur / le support / la supervision / en interne} |
| Impact | |

## Pourquoi les tests ne l''ont pas attrapée
{ce que le système a permis. Pas qui a écrit quel test.}

Choisis la réponse honnête, ou écris-en une meilleure :

- Il n''y avait pas de test pour ce chemin.
- Il y avait un test, et il serait passé avec l''anomalie présente.
- Il y avait un test, et il était désactivé ou en boucle de réessai.
- Le chemin n''est pas testable dans l''architecture actuelle.
- C''était couvert, et l''environnement différait de la production d''une
  manière que personne n''avait consignée.

## Duquel de nos renoncements il s''agissait
{si la stratégie de test listait ceci comme un renoncement délibéré, dis-le et
arrête-toi. Un risque accepté qui se réalise n''est pas un échec du test —
c''est le risque étant ce qu''il était. S''il n''était **pas** sur la liste,
c''est ça, le constat.}

## Ce qui change
| Action | Porteur | Pour quand |
|---|---|---|
| | | |

> Une action sans porteur et sans date n''existe pas. C''est ce tableau qui
> décide si la même anomalie sera livrée deux fois.

## Ce qui ne change délibérément pas
{les correctifs envisagés puis écartés, avec la raison. Un post-mortem qui
ajoute un contrôle à chaque fois finit avec un processus que personne ne
suit.}
$md$, 200);
