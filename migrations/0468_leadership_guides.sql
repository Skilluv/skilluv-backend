-- Six leadership onboarding guides, one per trade, in two languages.
--
-- Rows and not files, for the reason migration 0199 gave: they have to be
-- translated and edited by somebody who is not deploying.
--
-- ## What every one of them has to answer
--
-- **How to start without an employer.** In this domain that is not a
-- footnote, it is the whole obstacle: every conventional route into these
-- trades runs through already having a team. A first month that requires one
-- is a first month that does not happen, and a catalogue full of those filters
-- on employment — which is the filter this platform exists to get around.
--
-- Every guide names a route that works from nothing.

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary,
     body_md, sort_order)
VALUES

-- ═══════════════════════════════════════════════════════════════════
-- lead-product
-- ═══════════════════════════════════════════════════════════════════

('leadership-onboarding-lead-product', 'onboarding', 'leadership', 'delivery', 'en',
 'Getting started as a product manager',
 'You do not need a product. You need somebody else''s backlog and the discipline to read it properly.',
$md$
# Product manager — the first month

## The thing that is actually hard

Not writing the roadmap. Deciding what **not** to build, and being able to say
why in a sentence somebody who disagrees can argue with.

A roadmap that pursues everything on the list has decided nothing, and every
reviewer here looks for the omissions before they look at anything else.

## Where to start with no product of your own

Every open-source project has a public issue tracker full of feature requests
nobody has framed. That is the raw material, and it is free.

1. Pick a project you actually use. Not a famous one — one you use.
2. Read three months of issues. Not the code.
3. Find the request that keeps coming back in different words.

That last one is a product finding, and most projects have several.

## Days 1–10 — the problem behind the request

Take one request and rewrite it as a problem:

- **Who** has it. Specifically — "users" is not an answer.
- **How often**, and what it costs them each time.
- **What they do instead today.** This is the field that decides whether it
  matters: a workaround people are happy with is not a problem.

Then propose at least one solution that is **not** the one requested. If you
cannot, you have restated the request rather than framed it.

## Days 11–20 — a quarter, defended

Write a quarterly roadmap for that project. You are proposing, not deciding,
and the document should say so — presenting a plan for somebody else's project
as though it were settled is how a good proposal gets ignored.

The order is what is judged. "This before that because it unblocks three other
things" is an argument. "This before that because it is ready" is a queue.

And the section that makes it a roadmap:

> **Not this quarter:** X, because Y. Revisit if Z.

## Days 21–30 — talk to five people

Five users of an open-source tool are reachable in a week: the issue tracker,
the discussion forum, the chat. Ask them what they were doing when they last
got stuck.

Then write it up in three sections that must not blur:

- **What they said**, in their words.
- **What that means**, as your reading.
- **What you would do**, as a proposal.

A reader is entitled to disagree with the third while keeping the first.

## What gets a submission refused here

- A roadmap with nothing in the "not doing" section.
- A success measure that cannot move downwards.
- A claim about users with no evidence of having spoken to any.
- A document that identifies a company or a person who did not agree to it.

## Where to ask

`#lead-product` on Discord. Bring the issue you framed — the conversation is
much better with a concrete one.
$md$, 10),

('leadership-onboarding-lead-product', 'onboarding', 'leadership', 'delivery', 'fr',
 'Débuter comme product manager',
 'Tu n''as pas besoin d''un produit. Tu as besoin du backlog de quelqu''un d''autre et de la discipline de le lire correctement.',
$md$
# Product manager — le premier mois

## Ce qui est réellement difficile

Pas écrire la feuille de route. Décider ce qu'on ne construira **pas**, et
savoir dire pourquoi en une phrase avec laquelle quelqu'un qui n'est pas
d'accord peut discuter.

Une feuille de route qui poursuit tout ce qui est sur la liste n'a rien décidé,
et tous les relecteurs d'ici cherchent les renoncements avant de regarder quoi
que ce soit d'autre.

## Par où commencer sans produit à soi

Tout projet libre a un gestionnaire de tickets public plein de demandes que
personne n'a cadrées. C'est la matière première, et elle est gratuite.

1. Prends un projet que tu utilises vraiment. Pas un projet célèbre — un projet
   que tu utilises.
2. Lis trois mois de tickets. Pas le code.
3. Trouve la demande qui revient sous des mots différents.

Celle-là est un constat produit, et la plupart des projets en ont plusieurs.

## Jours 1 à 10 — le problème derrière la demande

Prends une demande et réécris-la en problème :

- **Qui** l'a. Précisément — « les utilisateurs » n'est pas une réponse.
- **À quelle fréquence**, et ce que ça leur coûte à chaque fois.
- **Ce qu'ils font à la place aujourd'hui.** C'est ce champ qui décide si ça
  compte : un contournement qui satisfait les gens n'est pas un problème.

Puis propose au moins une solution qui n'est **pas** celle demandée. Si tu n'y
arrives pas, tu as reformulé la demande, pas cadré le problème.

## Jours 11 à 20 — un trimestre, défendu

Écris une feuille de route trimestrielle pour ce projet. Tu proposes, tu ne
décides pas, et le document doit le dire — présenter un plan pour le projet de
quelqu'un d'autre comme s'il était acté est la meilleure façon de le faire
ignorer.

C'est l'ordre qui est jugé. « Ceci avant cela parce que ça débloque trois
autres choses » est un argument. « Ceci avant cela parce que c'est prêt » est
une file d'attente.

Et la section qui en fait une feuille de route :

> **Pas ce trimestre :** X, parce que Y. À revoir si Z.

## Jours 21 à 30 — parle à cinq personnes

Cinq utilisateurs d'un outil libre sont joignables en une semaine : le
gestionnaire de tickets, le forum, le salon de discussion. Demande-leur ce
qu'ils faisaient la dernière fois qu'ils ont bloqué.

Puis restitue en trois sections qui ne doivent pas se mélanger :

- **Ce qu'ils ont dit**, dans leurs mots.
- **Ce que ça veut dire**, en tant que ta lecture.
- **Ce que tu ferais**, en tant que proposition.

Un lecteur a le droit de ne pas être d'accord avec la troisième tout en gardant
la première.

## Ce qui fait refuser une soumission ici

- Une feuille de route dont la section « ce qu'on ne fait pas » est vide.
- Un indicateur de succès qui ne peut pas baisser.
- Une affirmation sur les utilisateurs sans trace d'avoir parlé à un seul.
- Un document qui identifie une entreprise ou une personne qui n'y a pas
  consenti.

## Où demander

`#lead-product` sur Discord. Viens avec le ticket que tu as cadré — la
conversation est bien meilleure avec un cas concret.
$md$, 10),

-- ═══════════════════════════════════════════════════════════════════
-- lead-tech
-- ═══════════════════════════════════════════════════════════════════

('leadership-onboarding-lead-tech', 'onboarding', 'leadership', 'technical', 'en',
 'Getting started as a tech lead',
 'The trade is writing the decision down before it is taken. You can practise on decisions other people already made.',
$md$
# Tech lead — the first month

## What separates this from being a senior engineer

Not seniority. **Writing the decision down before it is taken**, in a form
somebody who was not in the conversation can argue with a year later.

Most technical decisions in most organisations exist only as a commit and a
memory. The trade is the document.

## Where to practise: decisions somebody already made

Every open-source project's history is a sequence of decisions with no record.
Reconstructing one is real practice and produces a real artefact.

1. Find a commit or a pull request that changed a direction — a dependency
   swapped, a pattern adopted, an approach abandoned.
2. Reconstruct what the alternatives were **at that time**, not with what is
   known now.
3. Write the record that should have existed.

The projects with active RFC processes — Rust, Bevy, Godot — are the other
route: read the accepted ones, then write one for something the tracker keeps
asking for.

## Days 1–15 — the alternatives are the document

The section every weak decision record skips.

At least two options other than the chosen one, each described well enough
that **its advocate would recognise it**. A straw man is worse than no
alternative: it signals the author was not really deciding.

Then the field that separates a decision from an opinion:

> **This would be the wrong call if:** …

If you cannot fill it in, you have not made a decision, you have expressed a
preference.

## Days 16–25 — the state in between

Any decision that changes something already running has a period where both
shapes exist. That period is where the work actually is, and it is what most
proposals omit.

Write it: what runs in parallel, for how long, who is responsible for taking
the old one out, and what happens if that never gets done.

## Days 26–30 — cost the reversal

Some decisions are one-way. Saying which is more useful than being right.

For your record, add:
- What it takes to undo, in time and in risk.
- Whether that is still possible after six months of use.

## What gets a submission refused here

- A single option presented as a decision.
- A straw-man alternative.
- No condition under which the decision should be revisited.
- An architecture with no numbers — the load, the volume, the team size it
  assumes.

## Where to ask

`#lead-tech` on Discord, and the `Decision Review` voice room when somebody is
reading one out loud, which is the fastest way to find the missing alternative.
$md$, 20),

('leadership-onboarding-lead-tech', 'onboarding', 'leadership', 'technical', 'fr',
 'Débuter comme tech lead',
 'Le métier consiste à écrire la décision avant qu''elle soit prise. On s''y exerce sur des décisions que d''autres ont déjà prises.',
$md$
# Tech lead — le premier mois

## Ce qui sépare ça d'être ingénieur senior

Pas l'ancienneté. **Écrire la décision avant qu'elle soit prise**, dans une
forme avec laquelle quelqu'un qui n'était pas dans la conversation peut
discuter un an plus tard.

La plupart des décisions techniques n'existent que sous forme d'un commit et
d'un souvenir. Le métier, c'est le document.

## Où s'exercer : des décisions déjà prises

L'historique de tout projet libre est une suite de décisions sans trace. En
reconstituer une est un exercice réel qui produit un artefact réel.

1. Trouve un commit ou une contribution qui a changé une direction — une
   dépendance remplacée, un motif adopté, une approche abandonnée.
2. Reconstitue les alternatives telles qu'elles étaient **à ce moment-là**, pas
   avec ce qu'on sait aujourd'hui.
3. Écris la fiche de décision qui aurait dû exister.

Les projets avec un processus de RFC actif — Rust, Bevy, Godot — sont l'autre
route : lis celles qui ont été acceptées, puis écris-en une pour ce que le
gestionnaire de tickets réclame en boucle.

## Jours 1 à 15 — les alternatives sont le document

La section que toute fiche de décision faible saute.

Au moins deux options autres que celle retenue, chacune décrite assez bien pour
que **son défenseur la reconnaisse**. Un homme de paille est pire qu'aucune
alternative : ça signale que l'auteur ne décidait pas vraiment.

Puis le champ qui sépare une décision d'un avis :

> **Ce serait le mauvais choix si :** …

Si tu ne peux pas le remplir, tu n'as pas pris une décision, tu as exprimé une
préférence.

## Jours 16 à 25 — l'état intermédiaire

Toute décision qui modifie quelque chose qui tourne déjà a une période où les
deux formes coexistent. C'est là qu'est le travail, et c'est ce que la plupart
des propositions omettent.

Écris-le : ce qui tourne en parallèle, combien de temps, qui est responsable de
retirer l'ancien, et ce qui se passe si ce n'est jamais fait.

## Jours 26 à 30 — chiffre le retour arrière

Certaines décisions sont à sens unique. Dire lesquelles est plus utile que
d'avoir raison.

Ajoute à ta fiche :
- Ce qu'il faut pour défaire, en temps et en risque.
- Si c'est encore possible après six mois d'usage.

## Ce qui fait refuser une soumission ici

- Une seule option présentée comme une décision.
- Une alternative en homme de paille.
- Aucune condition de réexamen.
- Une architecture sans chiffres — la charge, le volume, la taille d'équipe
  qu'elle suppose.

## Où demander

`#lead-tech` sur Discord, et le salon vocal `Decision Review` quand quelqu'un en
lit une à voix haute : c'est la façon la plus rapide de trouver l'alternative
manquante.
$md$, 20),

-- ═══════════════════════════════════════════════════════════════════
-- lead-project
-- ═══════════════════════════════════════════════════════════════════

('leadership-onboarding-lead-project', 'onboarding', 'leadership', 'delivery', 'en',
 'Getting started as a delivery lead',
 'Three people is a project. You already have one.',
$md$
# Delivery lead — the first month

## What is being judged

Whether the plan survives its own assumptions. Anybody can produce a date;
the trade is producing one with the assumptions attached and the first one
that would break it named.

## Where to practise

**A group you are already in.** Three people is a project: a game jam team, a
study cohort, a community running an event. It has dependencies, a date, and
people who will be disappointed — which is everything.

**A public project that has slipped.** The tracker is the evidence, and the
work is producing the plan that should replace the one that failed.

## Days 1–10 — map what depends on what

Not a list of tasks. A map of what cannot start until something else finishes,
**including the dependencies outside the group** — a review somebody else has
to do, a service somebody else has to enable, a decision somebody else has to
take.

Then the question that makes it real: **has that person agreed?** A dependency
on somebody who does not know they are on the critical path is not a plan, it
is a hope.

## Days 11–20 — risks with responses

A risk register with no responses is a list of things to be sad about later.

For each one:
- What would happen.
- What is being done about it, by whom.
- **The signal** that it is happening — the thing you would notice first.

Then pick two you are **accepting** rather than mitigating, and write why.
That section is what separates a register from a worry list.

## Days 21–30 — an honest date

Produce a date, with:
- The assumptions it rests on, listed.
- The first assumption that would break it.
- What comes out of scope if the date has to hold.

That last one is the deliverable. A plan with no cut list has not planned for
being wrong, and every plan is wrong somewhere.

## The thing nobody tells you

Most of this trade is telling people bad news early. A slipped date announced
six weeks out costs an argument; the same date announced on the day costs
trust. The playbook you write should say what a bad week sounds like, before
there is one.

## What gets a submission refused here

- Dependencies with no owners.
- Risks with no responses.
- A date with no stated assumptions.
- A plan that has never been shown to the people it commits.

## Where to ask

`#lead-project` on Discord.
$md$, 30),

('leadership-onboarding-lead-project', 'onboarding', 'leadership', 'delivery', 'fr',
 'Débuter comme responsable de livraison',
 'Trois personnes, c''est un projet. Tu en as déjà un.',
$md$
# Responsable de livraison — le premier mois

## Ce qui est jugé

Si le plan survit à ses propres hypothèses. N'importe qui peut produire une
date ; le métier consiste à en produire une avec les hypothèses attachées et la
première qui la ferait tomber, nommée.

## Où s'exercer

**Un groupe dont tu fais déjà partie.** Trois personnes, c'est un projet : une
équipe de game jam, un groupe d'étude, une communauté qui organise un
événement. Il y a des dépendances, une date, et des gens qui seront déçus — ce
qui est tout ce qu'il faut.

**Un projet public qui a glissé.** Le gestionnaire de tickets est la preuve, et
le travail consiste à produire le plan qui doit remplacer celui qui a échoué.

## Jours 1 à 10 — cartographie ce qui dépend de quoi

Pas une liste de tâches. Une carte de ce qui ne peut pas commencer avant que
quelque chose d'autre finisse, **y compris les dépendances hors du groupe** —
une relecture que quelqu'un doit faire, un service que quelqu'un doit activer,
une décision que quelqu'un doit prendre.

Puis la question qui rend ça réel : **est-ce que cette personne est
d'accord ?** Une dépendance sur quelqu'un qui ignore qu'il est sur le chemin
critique n'est pas un plan, c'est un espoir.

## Jours 11 à 20 — des risques avec des réponses

Un registre de risques sans réponses est une liste de choses dont on sera
triste plus tard.

Pour chacun :
- Ce qui arriverait.
- Ce qu'on fait à ce sujet, et par qui.
- **Le signal** que c'est en train d'arriver — ce que tu remarquerais en
  premier.

Puis choisis-en deux que tu **acceptes** plutôt que de les traiter, et écris
pourquoi. C'est cette section qui sépare un registre d'une liste d'inquiétudes.

## Jours 21 à 30 — une date honnête

Produis une date, avec :
- Les hypothèses sur lesquelles elle repose, listées.
- La première hypothèse qui la ferait tomber.
- Ce qui sort du périmètre si la date doit tenir.

Cette dernière est le livrable. Un plan sans liste de coupes n'a pas prévu de
se tromper, et tout plan se trompe quelque part.

## Ce que personne ne te dit

L'essentiel de ce métier consiste à annoncer de mauvaises nouvelles tôt. Une
date qui glisse annoncée six semaines à l'avance coûte une discussion ; la même
annoncée le jour même coûte la confiance. Le manuel que tu écris devrait dire à
quoi ressemble une mauvaise semaine, avant qu'il y en ait une.

## Ce qui fait refuser une soumission ici

- Des dépendances sans porteur.
- Des risques sans réponse.
- Une date sans hypothèses énoncées.
- Un plan qui n'a jamais été montré aux gens qu'il engage.

## Où demander

`#lead-project` sur Discord.
$md$, 30),

-- ═══════════════════════════════════════════════════════════════════
-- lead-people
-- ═══════════════════════════════════════════════════════════════════

('leadership-onboarding-lead-people', 'onboarding', 'leadership', 'people', 'en',
 'Getting started as a people manager',
 'The trade with the strictest evidence rule on the platform, because it is the easiest one to fake.',
$md$
# People manager — the first month

## Read this first

This is the domain where unfalsifiable claims are easiest to make. "The team
was happier", "morale improved", "we built psychological safety" — none of
those can be checked, and all of them are common.

So the rule here is stricter than anywhere else on the platform: **a claim
about people comes with what was measured and when, or it is refused.** Not
downgraded. Refused.

That is not scepticism about the work. It is what makes the work worth
something on a profile.

## Where to practise with no reports

You do not need direct reports to produce the artefacts of this trade. You need
a described situation and rigorous reasoning.

- **A career ladder** for a hypothetical team, with the team described in the
  document: size, product, stage. The reasoning is what is reviewed.
- **A hiring loop** for a real open role somebody has posted publicly.
- **A team health check** on a group you are actually in — a cohort, a
  volunteer team, a community. Three people is a team.

## Days 1–12 — a ladder described by behaviour

Five levels for one track. Each described by **things somebody does**, not by
adjectives.

"Senior engineers show ownership" is not a ladder. "Senior engineers take a
problem from a vague report to a shipped fix without being assigned it, and
tell the people affected before they are asked" is.

Test each level with two examples: somebody at it, and somebody not yet there.
If you cannot tell the two apart from the wording, the wording is decoration.

## Days 13–22 — a loop somebody can fail fairly

Design an interview process for one role:

- The stages, and what each one is actually testing.
- The questions, the same for everybody.
- A rubric, so two interviewers score the same evidence the same way.
- **What a rejection tells the candidate.** The part everybody skips, and the
  only part the candidate experiences.

## Days 23–30 — ask a real group what it thinks

Take a group you are in. Design a health check, run it, write the plan.

Two things are judged harder than the findings:

- **How anonymity was preserved.** In a group of five, "which sub-team are you
  in" identifies people. Design around it or drop the question.
- **The cost of the plan.** In somebody's hours. An initiative that costs
  nothing is one nobody is doing.

## Confidentiality

Everything in this trade is about people who did not choose to be written
about. Every submission is anonymised, and "anonymised" means nobody is
identifiable **including by a detail only they would have** — the one person
who joined in March, the only designer on the team.

A reviewer confirms this before anything is published. It is the one thing that
blocks publication outright.

## What gets a submission refused here

- A claim about people with no evidence.
- Somebody identifiable in an anonymised document.
- A ladder written in adjectives.
- A plan with no hours attached.

## Where to ask

`#lead-people` on Discord.
$md$, 40),

('leadership-onboarding-lead-people', 'onboarding', 'leadership', 'people', 'fr',
 'Débuter comme responsable d''équipe',
 'Le métier avec la règle de preuve la plus stricte de la plateforme, parce que c''est le plus facile à simuler.',
$md$
# Responsable d'équipe — le premier mois

## À lire d'abord

C'est le domaine où les affirmations invérifiables sont les plus faciles à
faire. « L'équipe était plus heureuse », « le moral s'est amélioré », « on a
construit de la sécurité psychologique » — aucune ne peut être vérifiée, et
toutes sont courantes.

La règle ici est donc plus stricte que partout ailleurs : **une affirmation sur
des personnes vient avec ce qui a été mesuré et quand, sinon elle est
refusée.** Pas dévaluée. Refusée.

Ce n'est pas du scepticisme sur le travail. C'est ce qui donne sa valeur au
travail sur un profil.

## Où s'exercer sans avoir d'équipe

Tu n'as pas besoin de gérer des gens pour produire les artefacts de ce métier.
Tu as besoin d'une situation décrite et d'un raisonnement rigoureux.

- **Une grille de progression** pour une équipe hypothétique, décrite dans le
  document : taille, produit, stade. C'est le raisonnement qui est relu.
- **Un processus de recrutement** pour un poste réellement publié quelque part.
- **Un diagnostic d'équipe** sur un groupe dont tu fais partie — une cohorte,
  une équipe bénévole, une communauté. Trois personnes, c'est une équipe.

## Jours 1 à 12 — une grille décrite par des comportements

Cinq niveaux pour une filière. Chacun décrit par **des choses que quelqu'un
fait**, pas par des adjectifs.

« Les ingénieurs seniors font preuve d'appropriation » n'est pas une grille.
« Les ingénieurs seniors prennent un problème d'un signalement vague jusqu'au
correctif livré sans qu'on le leur assigne, et préviennent les personnes
concernées avant qu'on le leur demande » en est une.

Teste chaque niveau avec deux exemples : quelqu'un qui y est, et quelqu'un qui
n'y est pas encore. Si tu ne peux pas les distinguer à partir de la formulation,
la formulation est décorative.

## Jours 13 à 22 — un processus où on peut échouer équitablement

Conçois le recrutement d'un poste :

- Les étapes, et ce que chacune teste réellement.
- Les questions, les mêmes pour tout le monde.
- Une grille, pour que deux personnes notent la même preuve de la même façon.
- **Ce qu'un refus dit au candidat.** La partie que tout le monde saute, et la
  seule que le candidat vit.

## Jours 23 à 30 — demande à un vrai groupe ce qu'il pense

Prends un groupe dont tu fais partie. Conçois un diagnostic, mène-le, écris le
plan.

Deux choses sont jugées plus durement que les constats :

- **Comment l'anonymat a été préservé.** Dans un groupe de cinq, « dans quelle
  sous-équipe es-tu » identifie les gens. Conçois autour, ou retire la question.
- **Le coût du plan.** En heures de quelqu'un. Une initiative qui ne coûte rien
  est une initiative que personne ne fait.

## Confidentialité

Tout dans ce métier concerne des personnes qui n'ont pas choisi qu'on écrive
sur elles. Chaque soumission est anonymisée, et « anonymisée » veut dire que
personne n'est identifiable **y compris par un détail que seule cette personne
aurait** — la seule qui est arrivée en mars, l'unique designer de l'équipe.

Un relecteur le confirme avant toute publication. C'est la seule chose qui
bloque la publication sans discussion.

## Ce qui fait refuser une soumission ici

- Une affirmation sur des personnes sans preuve.
- Quelqu'un d'identifiable dans un document anonymisé.
- Une grille écrite en adjectifs.
- Un plan sans heures attachées.

## Où demander

`#lead-people` sur Discord.
$md$, 40),

-- ═══════════════════════════════════════════════════════════════════
-- lead-community
-- ═══════════════════════════════════════════════════════════════════

('leadership-onboarding-lead-community', 'onboarding', 'leadership', 'community', 'en',
 'Getting started as a community lead',
 'Counting who showed up once measures the announcement. Measure the second visit.',
$md$
# Community lead — the first month

## The number that matters, and the one everybody reports

Everybody reports arrivals: members joined, event attendance, followers. That
number measures the announcement.

The number that says whether a community exists is the **second visit**. How
many of the people who arrived last month came back this month, and can you
say what brought them.

Every submission here is read for that distinction first.

## Where to practise

You are almost certainly already in three communities that are badly run. Ask
one of them if you can help. The answer is yes far more often than people
expect, because the person running it is tired.

Failing that: this platform's own spaces, and the `#leadership-community`
channel, where the work of running them is done in the open.

## Days 1–10 — who it is for, and who it is not

The hardest sentence in this trade:

> This community is **not** for …

A community built for everybody retains nobody, because nobody recognises
themselves in it. Naming who it is not for is what makes the people it *is*
for feel found.

Write the positioning: who, what they get, and what they would go elsewhere
for.

## Days 11–20 — the second-visit mechanism

Design one concrete thing that brings somebody back. Not "engaging content" —
a mechanism:

- A recurring session at a fixed hour.
- A thread somebody is expected to answer.
- A thing that is only finished if they return.

Then say how you will know it worked, with a number that can go down.

## Days 21–30 — the rules, written before the incident

Every community eventually has an incident, and the ones that survive it wrote
the rules first.

- What is out of bounds.
- Who decides.
- What the appeal is.
- How a moderator hands over when they burn out — because they will.

Then name three cases your playbook does **not** cover. Every playbook has
them, and naming them is what stops somebody improvising badly at 2am.

## The part that is uncomfortable

Communities run on volunteers, and volunteer programmes are where good
intentions do the most damage. Any programme you design should state honestly:
what is asked, in hours; what is given back; and what happens when somebody
stops. A programme that cannot answer the third is a programme that will burn
people.

## What gets a submission refused here

- A strategy that counts arrivals instead of returns.
- A community "for everybody".
- A volunteer programme with no account of what it costs the volunteers.
- Moderation rules written after the first incident rather than before.

## Where to ask

`#lead-community` on Discord.
$md$, 50),

('leadership-onboarding-lead-community', 'onboarding', 'leadership', 'community', 'fr',
 'Débuter comme responsable de communauté',
 'Compter qui est venu une fois mesure l''annonce. Mesure la deuxième visite.',
$md$
# Responsable de communauté — le premier mois

## Le chiffre qui compte, et celui que tout le monde rapporte

Tout le monde rapporte les arrivées : membres inscrits, présence à
l'événement, abonnés. Ce chiffre mesure l'annonce.

Le chiffre qui dit si une communauté existe, c'est la **deuxième visite**.
Combien de gens arrivés le mois dernier sont revenus ce mois-ci, et sais-tu
dire ce qui les a fait revenir.

Chaque soumission ici est lue à travers cette distinction d'abord.

## Où s'exercer

Tu fais très probablement déjà partie de trois communautés mal animées.
Demande à l'une d'elles si tu peux aider. La réponse est oui bien plus souvent
qu'on ne le croit, parce que la personne qui l'anime est fatiguée.

À défaut : les espaces de cette plateforme, et le canal
`#leadership-community`, où le travail d'animation se fait à découvert.

## Jours 1 à 10 — pour qui, et pour qui pas

La phrase la plus difficile de ce métier :

> Cette communauté n'est **pas** pour…

Une communauté construite pour tout le monde ne retient personne, parce que
personne ne s'y reconnaît. Nommer ceux pour qui elle n'est pas est ce qui fait
que ceux pour qui elle *est* se sentent trouvés.

Écris le positionnement : qui, ce qu'ils y trouvent, et ce pour quoi ils
iraient ailleurs.

## Jours 11 à 20 — le mécanisme de retour

Conçois une chose concrète qui fait revenir. Pas « du contenu engageant » — un
mécanisme :

- Une séance récurrente à heure fixe.
- Un fil auquel quelqu'un est attendu.
- Une chose qui n'est finie que s'ils reviennent.

Puis dis comment tu sauras que ça a marché, avec un chiffre qui peut baisser.

## Jours 21 à 30 — les règles, écrites avant l'incident

Toute communauté finit par avoir un incident, et celles qui y survivent avaient
écrit les règles avant.

- Ce qui est hors limites.
- Qui décide.
- Comment on fait appel.
- Comment un modérateur passe la main quand il s'épuise — parce que ça
  arrivera.

Puis nomme trois cas que ton manuel ne couvre **pas**. Tout manuel en a, et les
nommer est ce qui évite qu'on improvise mal à deux heures du matin.

## La partie inconfortable

Les communautés tournent sur des bénévoles, et les programmes de bénévolat sont
là où les bonnes intentions font le plus de dégâts. Tout programme que tu
conçois devrait dire honnêtement : ce qui est demandé, en heures ; ce qui est
rendu ; et ce qui se passe quand quelqu'un s'arrête. Un programme qui ne peut
pas répondre à la troisième est un programme qui va épuiser des gens.

## Ce qui fait refuser une soumission ici

- Une stratégie qui compte les arrivées au lieu des retours.
- Une communauté « pour tout le monde ».
- Un programme de bénévolat sans compte de ce qu'il coûte aux bénévoles.
- Des règles de modération écrites après le premier incident plutôt qu'avant.

## Où demander

`#lead-community` sur Discord.
$md$, 50),

-- ═══════════════════════════════════════════════════════════════════
-- lead-mentor
-- ═══════════════════════════════════════════════════════════════════

('leadership-onboarding-lead-mentor', 'onboarding', 'leadership', 'teaching', 'en',
 'Getting started as a mentor and curriculum lead',
 'Mentoring one person is a relationship. Designing the path twenty will take is a document.',
$md$
# Mentor and curriculum lead — the first month

## Where the line falls

Accompanying one person is a relationship, and it is a real thing — it lives
under `soft_skills` on this platform, and it is not this trade.

This trade is designing the path a **group** will take, and then running it.
The artefact is a curriculum; the proof is what happened to the people.

## Where to practise

Skilluv's own cohorts. Anybody can start one, and it is the one thing in this
domain that genuinely cannot be simulated — the cohort challenge exists
because a curriculum nobody ran is a document, and this trade is judged on the
run.

Start small. Five people and six weeks teaches more than a design for twenty
over six months that never happens.

## Days 1–10 — the entry condition

The part most curricula skip and the reason half a cohort is lost by week two.

> To start this, you need to already be able to: …

Be specific and be honest. A curriculum whose first week assumes something
unstated does not fail visibly — the people who did not have it just quietly
stop showing up, and the design looks fine from the outside.

## Days 11–20 — each step produces something

Not "understands X". **Produces Y.**

A learner who has finished a step should be holding an artefact: a working
thing, a written thing, a reviewed thing. Two reasons:

- They can tell they are on track without asking you, which is what lets a
  cohort scale past your attention.
- It is what they show afterwards.

## Days 21–25 — design for falling behind

Somebody will miss two weeks. What happens?

Most curricula have no answer, so the answer is that the person leaves. Design
one:

- Which steps can be caught up, and which are gates.
- What a returning person is asked to do first.
- At what point it is honest to say this run is not for them, and what is
  offered instead.

## Days 26–30 — the outcome report

When a run ends, report with the denominator:

> Twelve joined. Eight finished. Two left for the schedule, one for a job,
> one stopped answering.

A graduation rate over the survivors is not a graduation rate. The platform's
own `leadership_cohort_outcomes` computes it over everybody who joined for
exactly that reason — except people who left because they found work, who are
removed rather than counted as losses.

## What gets a submission refused here

- A curriculum with no entry condition.
- Steps described by understanding rather than by output.
- An outcome report without the denominator.
- A design that assumes nobody falls behind.

## Where to ask

`#lead-mentor` on Discord, and `#leadership-cohorts` for the running of them.
$md$, 60),

('leadership-onboarding-lead-mentor', 'onboarding', 'leadership', 'teaching', 'fr',
 'Débuter comme mentor et concepteur de parcours',
 'Accompagner une personne est une relation. Concevoir le chemin que vingt vont prendre est un document.',
$md$
# Mentor et concepteur de parcours — le premier mois

## Où passe la ligne

Accompagner une personne est une relation, et c'en est une vraie — elle vit
sous `soft_skills` sur cette plateforme, et ce n'est pas ce métier.

Ce métier consiste à concevoir le chemin qu'un **groupe** va prendre, puis à le
faire. L'artefact est un parcours ; la preuve est ce qui est arrivé aux gens.

## Où s'exercer

Les cohortes de Skilluv. N'importe qui peut en démarrer une, et c'est la seule
chose de ce domaine qui ne peut vraiment pas être simulée — le défi de cohorte
existe parce qu'un parcours que personne n'a mené est un document, et ce métier
se juge sur la conduite.

Commence petit. Cinq personnes sur six semaines apprennent plus qu'un dispositif
pour vingt sur six mois qui n'a jamais lieu.

## Jours 1 à 10 — la condition d'entrée

Ce que la plupart des parcours sautent, et la raison pour laquelle la moitié
d'une cohorte disparaît en semaine deux.

> Pour commencer ceci, il faut déjà savoir : …

Sois précis et honnête. Un parcours dont la première semaine suppose quelque
chose de non dit n'échoue pas visiblement — ceux qui ne l'avaient pas cessent
simplement de venir, et le dispositif a l'air très bien vu de l'extérieur.

## Jours 11 à 20 — chaque étape produit quelque chose

Pas « comprend X ». **Produit Y.**

Quelqu'un qui a fini une étape doit tenir un artefact : une chose qui marche,
une chose écrite, une chose relue. Deux raisons :

- Il peut savoir s'il est dans les clous sans te le demander, ce qui permet à
  une cohorte de dépasser ton attention.
- C'est ce qu'il montrera après.

## Jours 21 à 25 — conçois pour ceux qui décrochent

Quelqu'un va rater deux semaines. Que se passe-t-il ?

La plupart des parcours n'ont pas de réponse, donc la réponse est que la
personne part. Conçois-en une :

- Quelles étapes se rattrapent, et lesquelles sont des portes.
- Ce qu'on demande d'abord à quelqu'un qui revient.
- À quel moment il est honnête de dire que cette session n'est pas pour lui, et
  ce qu'on lui propose à la place.

## Jours 26 à 30 — le rapport de résultats

Quand une session finit, rapporte avec le dénominateur :

> Douze inscrits. Huit ont terminé. Deux sont partis pour l'emploi du temps, un
> pour un poste, un a cessé de répondre.

Un taux de réussite calculé sur les survivants n'est pas un taux de réussite.
La vue `leadership_cohort_outcomes` de la plateforme le calcule sur tous ceux
qui ont rejoint, exactement pour cette raison — sauf ceux qui sont partis parce
qu'ils ont trouvé du travail, qui sont retirés au lieu d'être comptés comme des
pertes.

## Ce qui fait refuser une soumission ici

- Un parcours sans condition d'entrée.
- Des étapes décrites par la compréhension plutôt que par la production.
- Un rapport de résultats sans dénominateur.
- Un dispositif qui suppose que personne ne décroche.

## Où demander

`#lead-mentor` sur Discord, et `#leadership-cohorts` pour la conduite.
$md$, 60);
