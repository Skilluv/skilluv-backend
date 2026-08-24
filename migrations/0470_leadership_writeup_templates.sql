-- Ten leadership write-up templates, in two languages.
--
-- In this domain the document is the entire artefact. There is no running
-- system behind it, no merged contribution, no reproducible finding — a
-- roadmap that cannot be acted on has not been written, however good the
-- thinking behind it was.
--
-- ## The field every one of them shares
--
-- **What is being given up.** It appears under different names — the
-- non-goals, the omissions, the alternatives rejected, the risks accepted,
-- what the curriculum does not teach — and it is always the field that
-- separates a decision from a wish. A document with nothing in it has decided
-- nothing, and every review grid in this domain refuses one outright.
--
-- ## The second field, and why it is here
--
-- **What would make this wrong.** Every template asks it. This is the domain
-- where unfalsifiable claims are easiest to make, and the question is the
-- cheapest available defence against making one by accident.

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary,
     body_md, sort_order)
VALUES

('leadership-template-roadmap', 'writeup_template', 'leadership', 'delivery', 'en',
 'Quarterly roadmap',
 'What the quarter is for, in what order, and what it is not for.',
$md$
# Roadmap — {team or product}, {period}

## What this period is for
{one paragraph. If it takes three, the quarter has three directions and no
direction.}

## Where this came from
{the evidence. User conversations, usage figures, a support backlog, a
strategic bet. A roadmap whose origin is "leadership decided" is one nobody
can argue with, which sounds like strength and is not.}

## What we are doing
| # | What | Why this before the next | Unblocks | Done means |
|---|---|---|---|---|
| 1 | | | | |

> "Why this before the next" is the column reviewers read. Ready is not a
> reason; unblocks and learns are.

## What we are NOT doing
| Not this period | Why | Revisit when |
|---|---|---|
| | | |

> The section that makes this a roadmap. A document with an empty table here
> has decided nothing, and it is refused in review.

## Dependencies outside this team
| What | Who owns it | Have they agreed? |
|---|---|---|
| | | |

> The third column. A dependency on somebody who does not know they are on the
> critical path is a hope.

## How we will know
{the indicator that moves if this works — and that can also move down if it
does not. A measure that can only improve is a scoreboard.}

## What would make this wrong
{the assumption that, if it turned out false, would mean this quarter was
misspent. Name one.}
$md$, 210),

('leadership-template-roadmap', 'writeup_template', 'leadership', 'delivery', 'fr',
 'Feuille de route trimestrielle',
 'À quoi sert le trimestre, dans quel ordre, et à quoi il ne sert pas.',
$md$
# Feuille de route — {équipe ou produit}, {période}

## À quoi sert cette période
{un paragraphe. S'il en faut trois, le trimestre a trois directions et donc
aucune.}

## D'où ça vient
{la preuve. Conversations utilisateurs, chiffres d'usage, arriéré du support,
un pari stratégique. Une feuille de route dont l'origine est « la direction a
décidé » est une feuille de route avec laquelle personne ne peut discuter — ce
qui a l'air d'une force et n'en est pas une.}

## Ce que nous faisons
| n° | Quoi | Pourquoi avant le suivant | Débloque | « Fini » veut dire |
|---|---|---|---|---|
| 1 | | | | |

> « Pourquoi avant le suivant » est la colonne que lisent les relecteurs.
> « C'est prêt » n'est pas une raison ; débloquer et apprendre en sont.

## Ce que nous ne faisons PAS
| Pas cette période | Pourquoi | À revoir quand |
|---|---|---|
| | | |

> La section qui en fait une feuille de route. Un document dont ce tableau est
> vide n'a rien décidé, et il est refusé en relecture.

## Dépendances hors de cette équipe
| Quoi | Qui la porte | Sont-ils d'accord ? |
|---|---|---|
| | | |

> La troisième colonne. Une dépendance sur quelqu'un qui ignore qu'il est sur
> le chemin critique est un espoir.

## Comment nous le saurons
{l'indicateur qui bouge si ça marche — et qui peut aussi baisser si ça ne
marche pas. Un indicateur qui ne peut que s'améliorer est un tableau
d'affichage.}

## Ce qui rendrait ceci faux
{l'hypothèse qui, si elle s'avérait fausse, signifierait que ce trimestre a été
mal dépensé. Nomme-en une.}
$md$, 210),

('leadership-template-prd', 'writeup_template', 'leadership', 'delivery', 'en',
 'Product specification',
 'One feature, argued from a problem rather than from a request.',
$md$
# {feature}

## The problem
| | |
|---|---|
| Who has it | {specifically. "Users" is not an answer} |
| How often | |
| What it costs them | |
| What they do instead today | |

> The last row decides whether this is worth building. A workaround people are
> happy with is not a problem.

## How we know
{the evidence. Conversations, tickets, figures — with how many and how they
were selected.}

## What we are building
{the shape of it, in the words of somebody who will use it. Not the
implementation.}

## What we are not building
{the adjacent things people will ask for once this exists, and why they are
out of scope now.}

## Alternatives considered
| Option | Why not |
|---|---|
| Doing nothing | |
| | |

> "Doing nothing" is a row on purpose. It is the option every specification
> should have had to argue against.

## Success
{what will be different, measured. Include the figure it is at today —
without the baseline the target is decoration.}

## Launch
{who is told, when, and what happens to people using the old path.}

## What would make this wrong
{the assumption most likely to be false.}
$md$, 220),

('leadership-template-prd', 'writeup_template', 'leadership', 'delivery', 'fr',
 'Spécification produit',
 'Une fonctionnalité, argumentée à partir d''un problème plutôt que d''une demande.',
$md$
# {fonctionnalité}

## Le problème
| | |
|---|---|
| Qui l'a | {précisément. « Les utilisateurs » n'est pas une réponse} |
| À quelle fréquence | |
| Ce que ça leur coûte | |
| Ce qu'ils font à la place aujourd'hui | |

> La dernière ligne décide si ça vaut la peine d'être construit. Un
> contournement qui satisfait les gens n'est pas un problème.

## Comment on le sait
{la preuve. Conversations, tickets, chiffres — avec combien et comment ils ont
été choisis.}

## Ce qu'on construit
{la forme, dans les mots de quelqu'un qui va s'en servir. Pas
l'implémentation.}

## Ce qu'on ne construit pas
{les choses voisines qu'on demandera une fois que ça existera, et pourquoi
elles sont hors périmètre maintenant.}

## Alternatives envisagées
| Option | Pourquoi non |
|---|---|
| Ne rien faire | |
| | |

> « Ne rien faire » est une ligne à dessein. C'est l'option contre laquelle
> toute spécification aurait dû avoir à argumenter.

## Succès
{ce qui sera différent, mesuré. Inclus le chiffre d'aujourd'hui — sans la base
de départ, la cible est décorative.}

## Mise en service
{qui est prévenu, quand, et ce qu'il advient de ceux qui utilisent l'ancien
chemin.}

## Ce qui rendrait ceci faux
{l'hypothèse la plus susceptible d'être fausse.}
$md$, 220),

('leadership-template-rfc', 'writeup_template', 'leadership', 'technical', 'en',
 'Technical decision record',
 'Written before the decision is taken, so it can be argued with a year later.',
$md$
# {decision}

| | |
|---|---|
| Status | draft / proposed / accepted / rejected / superseded by … |
| Date | |
| Author | |
| Deciders | |

## Context
{what is true today that makes this a question. The constraints, the scale —
the load, the data volume, the team size. An architecture with no numbers fits
every situation and suits none.}

## The options

### Option A — {name}
**What it is.**

**What it gives.**

**What it costs.**

### Option B — {name}
{described well enough that its advocate would recognise it. A straw man is
worse than no alternative: it signals the author was not really deciding.}

### Option C — do nothing
{always a row. What happens if this is not decided.}

## Decision
{which, and the reasoning — not a restatement of the advantages.}

## What we are giving up
{the specific thing the rejected option would have given us, named. If nothing
is given up, the alternatives were not real.}

## Migration
| | |
|---|---|
| Path | {including the state where both exist} |
| Who removes the old one | |
| What if that never happens | |

## Reversal
| | |
|---|---|
| Cost to undo | |
| Still possible after six months? | |

> Some decisions are one-way. Saying which is more useful than being right.

## What would make this wrong
{the condition under which this should be revisited. The field that separates
a decision from an opinion.}
$md$, 230),

('leadership-template-rfc', 'writeup_template', 'leadership', 'technical', 'fr',
 'Fiche de décision technique',
 'Écrite avant que la décision soit prise, pour qu''on puisse en discuter un an après.',
$md$
# {décision}

| | |
|---|---|
| Statut | brouillon / proposée / acceptée / rejetée / remplacée par… |
| Date | |
| Auteur | |
| Décideurs | |

## Contexte
{ce qui est vrai aujourd'hui et qui fait de ceci une question. Les contraintes,
l'échelle — la charge, le volume de données, la taille d'équipe. Une
architecture sans chiffres convient à toutes les situations et n'en sert
aucune.}

## Les options

### Option A — {nom}
**Ce que c'est.**

**Ce que ça apporte.**

**Ce que ça coûte.**

### Option B — {nom}
{décrite assez bien pour que son défenseur la reconnaisse. Un homme de paille
est pire qu'aucune alternative : ça signale que l'auteur ne décidait pas
vraiment.}

### Option C — ne rien faire
{toujours une ligne. Ce qui se passe si ceci n'est pas tranché.}

## Décision
{laquelle, et le raisonnement — pas une reformulation des avantages.}

## Ce à quoi nous renonçons
{la chose précise que l'option rejetée nous aurait donnée, nommée. Si on ne
renonce à rien, les alternatives n'étaient pas réelles.}

## Migration
| | |
|---|---|
| Chemin | {y compris l'état où les deux coexistent} |
| Qui retire l'ancien | |
| Et si ce n'est jamais fait | |

## Retour arrière
| | |
|---|---|
| Coût pour défaire | |
| Encore possible après six mois ? | |

> Certaines décisions sont à sens unique. Dire lesquelles est plus utile que
> d'avoir raison.

## Ce qui rendrait ceci faux
{la condition de réexamen. Le champ qui sépare une décision d'un avis.}
$md$, 230),

('leadership-template-delivery-plan', 'writeup_template', 'leadership', 'delivery', 'en',
 'Delivery plan',
 'A date somebody outside the team can rely on, with its assumptions attached.',
$md$
# Delivery plan — {project}

## What is being delivered
{one sentence a stakeholder would recognise.}

## Milestones
| # | What | By | Depends on | Owner |
|---|---|---|---|---|
| 1 | | | | |

## Dependencies outside the team
| What | Owner | Agreed? | If it slips |
|---|---|---|---|
| | | | |

## Risks
| Risk | Response | Owner | Signal it is happening |
|---|---|---|---|
| | | | |

> The last column. A risk you would notice only once it had happened is not
> being managed, it is being feared.

## Risks we are accepting
| Risk | Why we accept it | Accepted by |
|---|---|---|
| | | |

## The date
| | |
|---|---|
| Date | |
| It assumes | |
| The first assumption that would break it | |

## The cut list
{what comes out, in order, if the date has to hold. Written now, while nobody
is under pressure.}

## Communication
| Who | What they get | How often |
|---|---|---|
| | | |

{And: what a bad week sounds like. Decide the wording before there is one.}
$md$, 240),

('leadership-template-delivery-plan', 'writeup_template', 'leadership', 'delivery', 'fr',
 'Plan de livraison',
 'Une date sur laquelle quelqu''un hors de l''équipe peut compter, avec ses hypothèses attachées.',
$md$
# Plan de livraison — {projet}

## Ce qui est livré
{une phrase qu'une partie prenante reconnaîtrait.}

## Jalons
| n° | Quoi | Pour le | Dépend de | Porteur |
|---|---|---|---|---|
| 1 | | | | |

## Dépendances hors de l'équipe
| Quoi | Porteur | D'accord ? | Si ça glisse |
|---|---|---|---|
| | | | |

## Risques
| Risque | Réponse | Porteur | Signal que ça arrive |
|---|---|---|---|
| | | | |

> La dernière colonne. Un risque qu'on ne remarquerait qu'une fois survenu
> n'est pas géré, il est redouté.

## Risques que nous acceptons
| Risque | Pourquoi on l'accepte | Accepté par |
|---|---|---|
| | | |

## La date
| | |
|---|---|
| Date | |
| Elle suppose | |
| La première hypothèse qui la ferait tomber | |

## La liste de coupes
{ce qui sort, dans l'ordre, si la date doit tenir. Écrite maintenant, pendant
que personne n'est sous pression.}

## Communication
| Qui | Ce qu'ils reçoivent | À quelle fréquence |
|---|---|---|
| | | |

{Et : à quoi ressemble une mauvaise semaine. Décide la formulation avant qu'il
y en ait une.}
$md$, 240),

('leadership-template-retrospective', 'writeup_template', 'leadership', 'delivery', 'en',
 'Retrospective',
 'The notes are half of it. The action items three months later are the other half.',
$md$
# Retrospective — {what it was about}

| | |
|---|---|
| Format | start-stop-continue / 4Ls / sailboat / mad-sad-glad / timeline |
| Held on | |
| Participants | {a number. A retrospective of two is a conversation, and saying so lets a reader calibrate} |
| Facilitated by | |

## What happened
{the timeline, before any opinion about it. This section is facts.}

## What was said
{in the room's own words, with nobody named.}

> There is no column here for who caused what, and there should not be. A
> retrospective that names a person is one nobody speaks honestly in the
> second time.

## What the system allowed
{the reading. What made the outcome likely, independent of who was involved.}

## Actions
| What | Owner | By when | Status |
|---|---|---|---|
| | | | open / done / dropped — {why} |

> An action with no owner and no date does not exist. Dropping one is a
> decision and counts as resolved — an item that quietly disappears is how the
> same retrospective happens twice.

## Sent back to the participants on
{date. A retrospective whose output the room never saw is one they will not
speak in next time.}

## Ninety days later
| | |
|---|---|
| Actions resolved | / |
| What changed because of this | |
| What did not, and why | |
$md$, 250),

('leadership-template-retrospective', 'writeup_template', 'leadership', 'delivery', 'fr',
 'Rétrospective',
 'Les notes sont la moitié. Les actions trois mois après sont l''autre moitié.',
$md$
# Rétrospective — {sur quoi}

| | |
|---|---|
| Format | start-stop-continue / 4L / voilier / mad-sad-glad / chronologie |
| Tenue le | |
| Participants | {un nombre. Une rétrospective à deux est une conversation, et le dire permet au lecteur de calibrer} |
| Animée par | |

## Ce qui s'est passé
{la chronologie, avant tout avis. Cette section, ce sont des faits.}

## Ce qui a été dit
{dans les mots de la salle, sans nommer personne.}

> Il n'y a pas ici de colonne pour qui a causé quoi, et il ne devrait pas y en
> avoir. Une rétrospective qui nomme une personne est une rétrospective où
> personne ne parlera honnêtement la fois suivante.

## Ce que le système a permis
{la lecture. Ce qui a rendu le résultat probable, indépendamment de qui était
là.}

## Actions
| Quoi | Porteur | Pour quand | Statut |
|---|---|---|---|
| | | | ouverte / faite / abandonnée — {pourquoi} |

> Une action sans porteur et sans date n'existe pas. En abandonner une est une
> décision et compte comme résolue — une action qui disparaît discrètement est
> la façon dont la même rétrospective se reproduit.

## Renvoyée aux participants le
{date. Une rétrospective dont la salle n'a jamais vu la sortie est une
rétrospective où elle ne parlera plus.}

## Quatre-vingt-dix jours plus tard
| | |
|---|---|
| Actions résolues | / |
| Ce qui a changé grâce à ça | |
| Ce qui n'a pas changé, et pourquoi | |
$md$, 250),

('leadership-template-career-ladder', 'writeup_template', 'leadership', 'people', 'en',
 'Career ladder',
 'Levels described by what people do, not by adjectives.',
$md$
# Career ladder — {track}

## The team this is for
{size, product, stage. A ladder written for a team of six and one for a team
of sixty are different documents, and a reader has to know which this is.}

## Levels

### {Level 1} — {name}
**Scope.** {what they are trusted with}

**Looks like.**
- {something somebody does. Observable. "Shows ownership" is not.}
-

**Does not yet.**
- {the thing that separates them from the next level}

**Someone at this level.** {a short anonymised example}

**Someone not yet.** {and why}

---

{repeat per level}

## How somebody moves
{who decides, on what evidence, and how often it is looked at.}

## What happens when somebody is not meeting the level
{written before anybody is in that situation. The section every ladder omits
and every person eventually needs.}

## What this ladder does not cover
{compensation, if it does not. Titles, if they are separate. Say so — a ladder
silently implying pay bands causes an argument later.}

## What would make this wrong
{the change in the team that would mean rewriting it.}
$md$, 260),

('leadership-template-career-ladder', 'writeup_template', 'leadership', 'people', 'fr',
 'Grille de progression',
 'Des niveaux décrits par ce que les gens font, pas par des adjectifs.',
$md$
# Grille de progression — {filière}

## L'équipe visée
{taille, produit, stade. Une grille écrite pour six personnes et une pour
soixante sont des documents différents, et le lecteur doit savoir laquelle
c'est.}

## Niveaux

### {Niveau 1} — {nom}
**Périmètre.** {ce qu'on leur confie}

**Ressemble à.**
- {quelque chose que la personne fait. Observable. « Fait preuve
  d'appropriation » n'en est pas.}
-

**Pas encore.**
- {ce qui la sépare du niveau suivant}

**Quelqu'un à ce niveau.** {un court exemple anonymisé}

**Quelqu'un qui n'y est pas encore.** {et pourquoi}

---

{répéter par niveau}

## Comment on avance
{qui décide, sur quelles preuves, et à quelle fréquence c'est réexaminé.}

## Ce qui se passe quand quelqu'un n'atteint pas son niveau
{écrit avant que quiconque soit dans cette situation. La section que toute
grille omet et dont tout le monde finit par avoir besoin.}

## Ce que cette grille ne couvre pas
{la rémunération, si ce n'est pas le cas. Les titres, s'ils sont séparés.
Dis-le — une grille qui laisse implicitement entendre des fourchettes provoque
une dispute plus tard.}

## Ce qui rendrait ceci faux
{le changement dans l'équipe qui obligerait à la réécrire.}
$md$, 260),

('leadership-template-hiring-process', 'writeup_template', 'leadership', 'people', 'en',
 'Hiring process',
 'A loop somebody can fail fairly.',
$md$
# Hiring process — {role}

## What we are actually hiring for
{the work, in a paragraph. Not the technology list.}

## Must be able to
- {behaviours, testable in the loop below}

## Nice to have
{and the honest note: everything in this list will be treated as a
requirement by somebody. Keep it short.}

## The loop
| Stage | Length | Tests for | Who runs it |
|---|---|---|---|
| | | | |

> Every stage tests something that is on the "must be able to" list. A stage
> that tests nothing on that list is a stage that tests rapport.

## Questions
{the same for every candidate, in the same order. A loop that varies by
candidate measures the interviewer.}

## Rubric
| What we are looking for | 1 | 2 | 3 | 4 |
|---|---|---|---|---|
| | {what a 1 looks like} | | | |

## Calibration
{how two interviewers are made to score the same evidence the same way, and
how often that is checked.}

## Rejection
{what the candidate is told, by whom, and how fast. The part everybody skips
and the only part most candidates experience.}

## Accessibility
{what is offered to somebody who needs an adjustment, and how they are told
they can ask without it counting against them.}

## What would make this wrong
{the signal that the loop is selecting for the wrong thing.}
$md$, 270),

('leadership-template-hiring-process', 'writeup_template', 'leadership', 'people', 'fr',
 'Processus de recrutement',
 'Un parcours où l''on peut échouer équitablement.',
$md$
# Processus de recrutement — {poste}

## Ce pour quoi on recrute vraiment
{le travail, en un paragraphe. Pas la liste de technologies.}

## Doit savoir
- {des comportements, vérifiables dans le parcours ci-dessous}

## Serait un plus
{et la note honnête : tout ce qui est dans cette liste sera traité comme une
exigence par quelqu'un. Garde-la courte.}

## Le parcours
| Étape | Durée | Teste | Qui la mène |
|---|---|---|---|
| | | | |

> Chaque étape teste quelque chose de la liste « doit savoir ». Une étape qui
> ne teste rien de cette liste teste l'affinité.

## Questions
{les mêmes pour chaque candidat, dans le même ordre. Un parcours qui varie
selon le candidat mesure le recruteur.}

## Grille
| Ce qu'on cherche | 1 | 2 | 3 | 4 |
|---|---|---|---|---|
| | {à quoi ressemble un 1} | | | |

## Calibrage
{comment on fait en sorte que deux recruteurs notent la même preuve de la même
façon, et à quelle fréquence on le vérifie.}

## Refus
{ce qu'on dit au candidat, par qui, et en combien de temps. La partie que tout
le monde saute et la seule que la plupart des candidats vivent.}

## Accessibilité
{ce qu'on propose à quelqu'un qui a besoin d'un aménagement, et comment on lui
dit qu'il peut le demander sans que ça lui soit reproché.}

## Ce qui rendrait ceci faux
{le signal que le parcours sélectionne la mauvaise chose.}
$md$, 270),

('leadership-template-team-health', 'writeup_template', 'leadership', 'people', 'en',
 'Team health audit',
 'A claim about people, made checkable.',
$md$
# Team health — {team}, {date}

## What was asked
{the questions, verbatim. A summary of the questions is not a method.}

## Who was asked, and how
| | |
|---|---|
| Team size | |
| Responded | |
| Method | {survey / conversations / both} |
| How anonymity was preserved | |

> In a team of five, "which sub-team are you in" identifies people. If a
> question could not be made anonymous, it was dropped — say which.

## What came back
{the figures, and the words, kept apart from the reading of them.}

## What this cannot tell us
{response rate, self-selection, who did not answer. The section that stops
somebody quoting this as proof of something it does not show.}

## The plan
| What | Owner | Hours | By when |
|---|---|---|---|
| | | | |

> The hours column. An initiative that costs nothing is one nobody is doing.

## What we are not addressing
{the thing that came back and that we are not going to act on, and why. Silence
here reads as a promise.}

## When we ask again
{date. And the figure we expect to have moved — which can also move down.}
$md$, 280),

('leadership-template-team-health', 'writeup_template', 'leadership', 'people', 'fr',
 'Diagnostic de santé d''équipe',
 'Une affirmation sur des personnes, rendue vérifiable.',
$md$
# Santé d'équipe — {équipe}, {date}

## Ce qui a été demandé
{les questions, mot pour mot. Un résumé des questions n'est pas une méthode.}

## À qui, et comment
| | |
|---|---|
| Taille de l'équipe | |
| Réponses | |
| Méthode | {questionnaire / entretiens / les deux} |
| Comment l'anonymat a été préservé | |

> Dans une équipe de cinq, « dans quelle sous-équipe es-tu » identifie les
> gens. Si une question ne pouvait pas être anonymisée, elle a été retirée —
> dis laquelle.

## Ce qui est revenu
{les chiffres, et les mots, tenus à l'écart de leur lecture.}

## Ce que ça ne peut pas nous dire
{taux de réponse, auto-sélection, qui n'a pas répondu. La section qui empêche
quelqu'un de citer ceci comme preuve de ce que ça ne montre pas.}

## Le plan
| Quoi | Porteur | Heures | Pour quand |
|---|---|---|---|
| | | | |

> La colonne heures. Une initiative qui ne coûte rien est une initiative que
> personne ne fait.

## Ce que nous ne traitons pas
{ce qui est remonté et sur quoi nous n'allons pas agir, et pourquoi. Le silence
ici se lit comme une promesse.}

## Quand on redemande
{date. Et le chiffre qu'on s'attend à voir bouger — qui peut aussi baisser.}
$md$, 280),

('leadership-template-community-strategy', 'writeup_template', 'leadership', 'community', 'en',
 'Community strategy',
 'Who it is for, who it is not for, and what brings people back.',
$md$
# Community strategy — {community}, {period}

## Who this is for
{specifically enough that somebody reading it recognises themselves or does
not.}

## Who this is not for
{the hardest sentence in the trade. A community built for everybody retains
nobody, because nobody recognises themselves in it.}

## What they get here that they cannot get elsewhere
{one thing. If there are four, there is none.}

## The second-visit mechanism
{a concrete thing that brings somebody back. Not "engaging content" — a
recurring session, a thread somebody is expected to answer, a thing only
finished if they return.}

## The people who run it
| Role | Asked of them (hours) | What they get | When they stop |
|---|---|---|---|
| | | | |

> The last column. A programme that cannot answer it will burn people.

## Moderation
{what is out of bounds, who decides, how an appeal works — and three cases
this does not cover.}

## How we will know
| | Today | Target |
|---|---|---|
| Returned within 30 days | | |
| | | |

> Returns, not arrivals. Counting who showed up once measures the
> announcement.

## What would make this wrong
{the assumption about the audience most likely to be false.}
$md$, 290),

('leadership-template-community-strategy', 'writeup_template', 'leadership', 'community', 'fr',
 'Stratégie de communauté',
 'Pour qui, pour qui pas, et ce qui fait revenir.',
$md$
# Stratégie de communauté — {communauté}, {période}

## Pour qui
{assez précisément pour que quelqu'un qui lit s'y reconnaisse ou non.}

## Pour qui ce n'est pas
{la phrase la plus difficile du métier. Une communauté construite pour tout le
monde ne retient personne, parce que personne ne s'y reconnaît.}

## Ce qu'ils y trouvent et qu'ils ne trouvent pas ailleurs
{une chose. S'il y en a quatre, il n'y en a aucune.}

## Le mécanisme de deuxième visite
{une chose concrète qui fait revenir. Pas « du contenu engageant » — une séance
récurrente, un fil auquel quelqu'un est attendu, une chose qui n'est finie que
s'ils reviennent.}

## Les gens qui l'animent
| Rôle | Ce qu'on leur demande (heures) | Ce qu'ils reçoivent | Quand ils s'arrêtent |
|---|---|---|---|
| | | | |

> La dernière colonne. Un programme qui ne peut pas y répondre épuisera des
> gens.

## Modération
{ce qui est hors limites, qui décide, comment on fait appel — et trois cas que
ceci ne couvre pas.}

## Comment nous le saurons
| | Aujourd'hui | Cible |
|---|---|---|
| Revenus sous 30 jours | | |
| | | |

> Les retours, pas les arrivées. Compter qui est venu une fois mesure
> l'annonce.

## Ce qui rendrait ceci faux
{l'hypothèse sur le public la plus susceptible d'être fausse.}
$md$, 290),

('leadership-template-curriculum', 'writeup_template', 'leadership', 'teaching', 'en',
 'Cohort curriculum',
 'A sequence where each step produces something, with a stated entry condition.',
$md$
# Curriculum — {trade}, {duration}

## Who this is for
| | |
|---|---|
| To start, you need to already be able to | {be specific and honest} |
| This is not for | |
| Time expected per week | |

> The entry condition is the part most curricula skip, and it is why half a
> cohort is lost by week two. A curriculum whose first week assumes something
> unstated does not fail visibly — the people who did not have it just stop
> showing up.

## What somebody can do at the end
{that they could not at the start, stated so it can be checked.}

## The steps
| # | What | Produces | How they know they got it |
|---|---|---|---|
| 1 | | | |

> "Produces" is an artefact, not an understanding. It is what lets a learner
> tell they are on track without asking, and what they show afterwards.

## Gates
{which steps must be passed before continuing, and which can be caught up.}

## Falling behind
{what happens to somebody who misses two weeks: what they are asked to do
first on returning, and at what point it is honest to say this run is not for
them — and what is offered instead.}

## What this does not teach
{and where to go for it.}

## Running it
| | |
|---|---|
| Cohort size | |
| Sessions | {how often, how long} |
| Mentor time per week | |
| Could somebody else run this from what is written? | {yes / no — and if no, say so} |
$md$, 300),

('leadership-template-curriculum', 'writeup_template', 'leadership', 'teaching', 'fr',
 'Parcours de cohorte',
 'Une séquence où chaque étape produit quelque chose, avec une condition d''entrée énoncée.',
$md$
# Parcours — {métier}, {durée}

## Pour qui
| | |
|---|---|
| Pour commencer, il faut déjà savoir | {sois précis et honnête} |
| Ce n'est pas pour | |
| Temps attendu par semaine | |

> La condition d'entrée est ce que la plupart des parcours sautent, et c'est
> pour ça que la moitié d'une cohorte disparaît en semaine deux. Un parcours
> dont la première semaine suppose quelque chose de non dit n'échoue pas
> visiblement — ceux qui ne l'avaient pas cessent simplement de venir.

## Ce que quelqu'un saura faire à la fin
{et qu'il ne savait pas au début, énoncé de façon vérifiable.}

## Les étapes
| n° | Quoi | Produit | Comment il sait qu'il a compris |
|---|---|---|---|
| 1 | | | |

> « Produit » est un artefact, pas une compréhension. C'est ce qui permet à
> l'apprenant de savoir s'il est dans les clous sans demander, et c'est ce
> qu'il montrera après.

## Portes
{quelles étapes doivent être passées avant de continuer, et lesquelles se
rattrapent.}

## Décrochage
{ce qui arrive à quelqu'un qui rate deux semaines : ce qu'on lui demande
d'abord à son retour, et à quel moment il est honnête de dire que cette session
n'est pas pour lui — et ce qu'on lui propose à la place.}

## Ce que ceci n'enseigne pas
{et où aller pour ça.}

## Conduite
| | |
|---|---|
| Taille de cohorte | |
| Séances | {à quelle fréquence, quelle durée} |
| Temps mentor par semaine | |
| Quelqu'un d'autre pourrait-il le mener à partir de ce qui est écrit ? | {oui / non — et si non, dis-le} |
$md$, 300),

('leadership-template-cohort-outcomes', 'writeup_template', 'leadership', 'teaching', 'en',
 'Cohort outcome report',
 'The denominator travels with the rate.',
$md$
# Cohort outcomes — {cohort}, {dates}

## The numbers
| | |
|---|---|
| Joined | |
| Finished | |
| Left | |
| Of whom, left because they found work | |

> A graduation rate over the survivors is not a graduation rate. It improves
> every time somebody gives up, which makes it reward the failure it should
> detect.

## Why people left
| Reason | How many |
|---|---|
| Schedule | |
| The curriculum assumed something they did not have | |
| Personal | |
| Found work | |
| Stopped answering | |

> The second row is the one that is yours to act on. The others are facts
> about the world; that one is a fact about the design.

## Where people got to
{what the people who finished can now do, and how you know — an artefact, a
role, a contribution.}

## What this does not show
{the people who did not apply. The ones who finished and did not use it.}

## What I would change
{concretely, in the curriculum. Not "communicate more".}

## What I would keep
{the part that worked, so the next version does not lose it while fixing the
rest.}
$md$, 310),

('leadership-template-cohort-outcomes', 'writeup_template', 'leadership', 'teaching', 'fr',
 'Rapport de résultats de cohorte',
 'Le dénominateur voyage avec le taux.',
$md$
# Résultats de cohorte — {cohorte}, {dates}

## Les chiffres
| | |
|---|---|
| Inscrits | |
| Ont terminé | |
| Sont partis | |
| Dont partis parce qu'ils ont trouvé du travail | |

> Un taux de réussite calculé sur les survivants n'est pas un taux de
> réussite. Il s'améliore chaque fois que quelqu'un abandonne, ce qui le fait
> récompenser l'échec qu'il devrait détecter.

## Pourquoi les gens sont partis
| Raison | Combien |
|---|---|
| Emploi du temps | |
| Le parcours supposait quelque chose qu'ils n'avaient pas | |
| Personnel | |
| Ont trouvé du travail | |
| Ont cessé de répondre | |

> La deuxième ligne est celle sur laquelle tu peux agir. Les autres sont des
> faits sur le monde ; celle-là est un fait sur le dispositif.

## Où les gens sont arrivés
{ce que ceux qui ont terminé savent faire maintenant, et comment tu le sais —
un artefact, un poste, une contribution.}

## Ce que ceci ne montre pas
{ceux qui n'ont pas candidaté. Ceux qui ont terminé et ne s'en sont pas
servis.}

## Ce que je changerais
{concrètement, dans le parcours. Pas « mieux communiquer ».}

## Ce que je garderais
{la partie qui a marché, pour que la version suivante ne la perde pas en
corrigeant le reste.}
$md$, 310);
