-- What to do in your first month, per security trade.
--
-- ## Why five guides and not one
--
-- The five trades share almost nothing about a first month. A red teamer needs
-- a range and a proxy; a governance specialist needs a framework and a
-- document; a blue teamer needs an artefact and a rule engine. One guide would
-- have been the intersection of the five, which is "install Docker and read
-- OWASP" — advice nobody needed a platform for.
--
-- ## The thing every one of them says first
--
-- Where you are allowed to work. This is the only domain on the platform where
-- practising on the wrong target is a criminal offence rather than a wasted
-- afternoon, and a guide that leads with tooling is a guide that has buried it.
--
-- ## What none of them says
--
-- "Get a certification." They cost between three hundred and eight thousand
-- euros, they are how this trade excludes people, and every one of these
-- guides is a month of work that produces something readable instead. A
-- certification is worth having and it is not the entry.

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

-- ═══════════════════════════════════════════════════════════════════
-- Red team
-- ═══════════════════════════════════════════════════════════════════

('security-onboarding-red-team', 'onboarding', 'security', 'red-team', 'en',
 'Getting started in offensive security',
 'A first month that ends with a report somebody could follow — on targets built to be broken, and nothing else.',
$md$# Red team — the first month

## Read this part twice

You may attack: a range built to be attacked, or a system whose owner has
published a scope that includes it. Nothing else. Not your employer's website
"just to check". Not a site you noticed was slow. The difference between this
trade and the offence it resembles is a written permission, and it is the only
difference.

The scope for this platform is at `/security` and it is short. Read it before
you touch `staging.skill-uv.com`.

## Days 1–7 — the proxy becomes a place you live

Install Burp Suite Community or OWASP ZAP and put every request through it.
Not for a tutorial: for a week, browse ordinary sites through it and read
what goes past. Most of what a beginner is missing is not a technique, it is
familiarity with what normal traffic looks like.

Then the PortSwigger Web Security Academy. It is free, including the labs, and
nothing else free is as thorough. Do the access-control section first — broken
authorisation is the most common real finding and the least glamorous topic.

## Days 8–20 — a range, and notes

Juice Shop in one container, or WebGoat if you want the defect explained
first. Work through it with the proxy, and keep notes as you go: the request,
the response, what you expected. You are practising the report, not the
exploit.

The habit that separates the people who get paid: after each solve, write the
one sentence that says *which check was missing*. "The login form is
vulnerable to SQL injection" is a symptom. "The login query is built by string
concatenation and the parameter is not bound" is a finding.

## Days 21–30 — the first report

Pick something on this platform's staging deployment, inside the published
scope, and hunt for one hour a day. Most days you will find nothing. That is
the trade.

When you find something, the report is the deliverable:

- **Reproduction**, precise enough that a stranger gets to the same result.
  Requests, in order, with the payload. Not a screenshot of a payload.
- **Impact**, on this system. What an attacker could actually do — not the
  worst thing the vulnerability class has ever caused somewhere else.
- **A CVSS vector**, not a number. The vector is what makes a severity
  arguable, and a severity you cannot argue is one you will lose.
- **What you did not do.** Where you stopped, and what you believed was on
  the other side. This is the single most valuable habit in this trade and
  the one nothing can measure.

## What to expect

Your first report may be a duplicate, or out of scope, or not a
vulnerability. All three are normal and all three are answered with a reason
you can read. The second one is better because of the first.
$md$, 10),

('security-onboarding-red-team', 'onboarding', 'security', 'red-team', 'fr',
 'Débuter en sécurité offensive',
 'Un premier mois qui se termine par un rapport que quelqu''un peut suivre — sur des cibles faites pour ça, et rien d''autre.',
$md$# Red team — le premier mois

## À lire deux fois

Vous pouvez attaquer : une cible construite pour être attaquée, ou un système
dont le propriétaire a publié un périmètre qui l'inclut. Rien d'autre. Pas le
site de votre employeur « juste pour voir ». Pas un site que vous avez trouvé
lent. Ce qui distingue ce métier du délit qui lui ressemble, c'est une
autorisation écrite, et c'est la seule différence.

Le périmètre de cette plateforme est sur `/security` et il est court.
Lisez-le avant de toucher à `staging.skill-uv.com`.

## Jours 1 à 7 — le proxy devient un lieu où vous vivez

Installez Burp Suite Community ou OWASP ZAP et faites passer toutes vos
requêtes par lui. Pas pour un tutoriel : pendant une semaine, naviguez
normalement à travers lui et lisez ce qui passe. Ce qui manque le plus à un
débutant n'est pas une technique, c'est la familiarité avec ce à quoi
ressemble un trafic ordinaire.

Ensuite la PortSwigger Web Security Academy. Gratuite, labs inclus, et rien
d'autre de gratuit n'est aussi complet. Commencez par le contrôle d'accès :
l'autorisation cassée est la découverte réelle la plus fréquente et le sujet
le moins spectaculaire.

## Jours 8 à 20 — une cible, et des notes

Juice Shop dans un conteneur, ou WebGoat si vous préférez qu'on vous explique
le défaut d'abord. Travaillez avec le proxy et prenez des notes au fil de
l'eau : la requête, la réponse, ce que vous attendiez. Vous vous entraînez au
rapport, pas à l'exploit.

L'habitude qui distingue ceux qui sont payés : après chaque résolution,
écrivez la phrase qui dit *quelle vérification manquait*. « Le formulaire de
connexion est vulnérable à l'injection SQL » est un symptôme. « La requête de
connexion est construite par concaténation et le paramètre n'est pas lié » est
une découverte.

## Jours 21 à 30 — le premier rapport

Choisissez quelque chose sur le déploiement de préproduction de cette
plateforme, dans le périmètre publié, et cherchez une heure par jour. La
plupart des jours vous ne trouverez rien. C'est le métier.

Quand vous trouvez, le rapport est le livrable :

- **La reproduction**, assez précise pour qu'un inconnu arrive au même
  résultat. Les requêtes, dans l'ordre, avec la charge utile. Pas une capture
  d'écran d'une charge utile.
- **L'impact**, sur ce système-là. Ce qu'un attaquant pourrait réellement
  faire — pas le pire qu'ait causé cette classe de vulnérabilité ailleurs.
- **Un vecteur CVSS**, pas un chiffre. Le vecteur est ce qui rend une gravité
  discutable, et une gravité indiscutable est une gravité que vous perdrez.
- **Ce que vous n'avez pas fait.** Où vous vous êtes arrêté, et ce que vous
  pensiez qu'il y avait derrière. C'est l'habitude la plus précieuse de ce
  métier et la seule que rien ne peut mesurer.

## À quoi s'attendre

Votre premier rapport sera peut-être un doublon, hors périmètre, ou pas une
vulnérabilité. Les trois sont normaux et les trois reçoivent une raison que
vous pouvez lire. Le deuxième est meilleur grâce au premier.
$md$, 10),

-- ═══════════════════════════════════════════════════════════════════
-- Blue team
-- ═══════════════════════════════════════════════════════════════════

('security-onboarding-blue-team', 'onboarding', 'security', 'blue-team', 'en',
 'Getting started in defensive security',
 'A first month spent reading logs and captures somebody else published, and writing the rule that would have caught it.',
$md$# Blue team — the first month

## The good news, and the catch

You need nobody's permission. Every artefact in this trade — a capture, a log
set, a memory image — can be downloaded and read on your own machine, and the
public archives are decades deep.

The catch is that nothing tells you when you are right. There is no green
build here. What replaces it is a rule that fires on the sample and stays
quiet on a week of ordinary traffic, and showing both halves is the whole
skill.

## Days 1–7 — Wireshark, on somebody else's afternoon

Install Wireshark. Take one capture from Malware Traffic Analysis and answer
three questions about it: which host started it, what it contacted, and what
left the network.

Learn `Follow → TCP Stream` on day one. Most beginners read packets for a week
before discovering that the conversation is one click away.

Then the same capture with `tshark` on the command line, and get one number
out of it — a count, a top-ten, anything. That is where analysis stops being
clicking.

## Days 8–18 — logs, and the sentence that must not blur

Take a public log set. Find the brute-force attempt. Then write down, in two
separate sentences:

- what you **observed**: "1,400 authentication failures from one address in
  nine minutes, then one success";
- what you **infer**: "that account is compromised".

Keeping those apart is the discipline of this trade. An analysis that mixes
them cannot be checked by the person reading it, and the person reading it is
about to wake somebody up.

## Days 19–30 — write a detection, and try to break it

Take what you found and write a Sigma rule for it. Then do the part everybody
skips: run it against a period of ordinary activity and count the false
positives.

A rule with no false-positive figure is a rule nobody will keep enabled for a
month. Reporting "fires on the sample, 3 hits in a week of normal traffic, all
explainable" is what a working detection engineer produces, and almost nobody
arrives able to.

Read the SigmaHQ repository while you do it. It is the best free reading there
is on what separates a rule that works from one that alarms.

## What to do with it here

The defensive labs on this platform are exactly this shape: an artefact, some
questions, answers checked by hash. Start with an easy one and read the hints
on the questions you get wrong — they are written to teach rather than to
gatekeep.
$md$, 20),

('security-onboarding-blue-team', 'onboarding', 'security', 'blue-team', 'fr',
 'Débuter en sécurité défensive',
 'Un premier mois passé à lire des journaux et des captures publiés par d''autres, et à écrire la règle qui aurait détecté.',
$md$# Blue team — le premier mois

## La bonne nouvelle, et le piège

Vous n'avez besoin de l'autorisation de personne. Tous les artefacts de ce
métier — une capture, un jeu de journaux, une image mémoire — se téléchargent
et se lisent sur votre machine, et les archives publiques ont vingt ans de
profondeur.

Le piège, c'est que rien ne vous dit quand vous avez raison. Il n'y a pas de
build vert ici. Ce qui le remplace, c'est une règle qui se déclenche sur
l'échantillon et reste silencieuse sur une semaine de trafic ordinaire — et
montrer les deux moitiés est tout le savoir-faire.

## Jours 1 à 7 — Wireshark, sur l'après-midi de quelqu'un d'autre

Installez Wireshark. Prenez une capture chez Malware Traffic Analysis et
répondez à trois questions : quel hôte a commencé, avec quoi il a communiqué,
et ce qui est sorti du réseau.

Apprenez `Follow → TCP Stream` dès le premier jour. La plupart des débutants
lisent des paquets pendant une semaine avant de découvrir que la conversation
est à un clic.

Puis la même capture avec `tshark` en ligne de commande, et sortez-en un
chiffre — un compte, un top dix, n'importe quoi. C'est là que l'analyse cesse
d'être du clic.

## Jours 8 à 18 — les journaux, et la phrase qui ne doit pas se mélanger

Prenez un jeu de journaux public. Trouvez la tentative de force brute. Puis
écrivez, en deux phrases distinctes :

- ce que vous avez **observé** : « 1 400 échecs d'authentification depuis une
  adresse en neuf minutes, puis un succès » ;
- ce que vous en **déduisez** : « ce compte est compromis ».

Garder les deux séparées est la discipline de ce métier. Une analyse qui les
mélange n'est pas vérifiable par celui qui la lit, et celui qui la lit est sur
le point de réveiller quelqu'un.

## Jours 19 à 30 — écrire une détection, puis essayer de la casser

Prenez ce que vous avez trouvé et écrivez une règle Sigma. Puis faites ce que
tout le monde saute : passez-la sur une période d'activité normale et comptez
les faux positifs.

Une règle sans chiffre de faux positifs est une règle que personne ne laissera
active un mois. Annoncer « se déclenche sur l'échantillon, 3 occurrences sur
une semaine de trafic normal, toutes explicables » est ce que produit un
ingénieur de détection, et presque personne n'arrive en sachant le faire.

Lisez le dépôt SigmaHQ pendant ce temps. C'est la meilleure lecture gratuite
sur ce qui sépare une règle qui marche d'une règle qui sonne.

## Qu'en faire ici

Les labs défensifs de cette plateforme ont exactement cette forme : un
artefact, des questions, des réponses vérifiées par empreinte. Commencez par
un facile et lisez les indices sur les questions ratées — ils sont écrits pour
apprendre, pas pour filtrer.
$md$, 20),

-- ═══════════════════════════════════════════════════════════════════
-- Code security
-- ═══════════════════════════════════════════════════════════════════

('security-onboarding-code-audit', 'onboarding', 'security', 'code-audit', 'en',
 'Getting started in code security',
 'A first month reading real code for real defects, on a codebase that invites it — this one.',
$md$# Code security — the first month

## Where to start reading

Here. This platform's source is public and its authors have asked to be
audited: `skilluv-backend`, `skilluv-frontend`, `skilluv-admin`,
`skilluv-ia`. There is an audit exercise in the catalogue for the
authentication code, one for the authorisation code and one for file handling,
each with a scope and a reviewer.

Reading the deployed service is a different permission and needs the published
scope. Reading the code needs nothing.

## Days 1–10 — one defect class, all the way down

Pick injection. Learn what it looks like in the language in front of you, then
find every place in one codebase where untrusted input reaches a query, and
check each one.

You will find that almost all of them are fine. That is the job: an audit is
mostly establishing that things are correct, and the finding is what is left.

Write down what you checked and found sound. An audit that reports three
findings and no coverage tells the reader nothing about how carefully it was
read.

## Days 11–20 — the scanner, and its wrongness

Install Semgrep and run it on the same codebase. Then triage every hit:
real, unreachable, or a false positive, with the reason.

This is the exercise that makes a code auditor. A tool that produces two
hundred hits has told you nothing until somebody has read them, and the
reading is the skill nobody can automate. Write the triage down — the
dismissed hits with their reasons are half of what makes an audit
believable.

Then write one Semgrep rule of your own, for a defect you actually found. A
rule is a finding that keeps working after you have moved on.

## Days 21–30 — the report

Each finding needs four things and they are not negotiable:

- **The path**, from the entry point to the sink, with file and line at every
  step. A finding that names a sink without a reachable source is a scanner
  hit with a paragraph attached.
- **Reachability**: the configuration, the flag, the route that makes this
  code run.
- **A fix**, concrete and at the right layer. "Sanitise the input" is not one;
  the parameterised query is.
- **Impact**, in terms of what an attacker gets.

## The one thing to be careful about

If you find a live credential in code or in history, that is not a finding to
write up in public. Report it privately, immediately, and redact it in
everything you write. An audit that publishes a working key has caused the
incident it was looking for.
$md$, 30),

('security-onboarding-code-audit', 'onboarding', 'security', 'code-audit', 'fr',
 'Débuter en sécurité du code',
 'Un premier mois à lire du vrai code pour de vrais défauts, sur une base de code qui le demande — celle-ci.',
$md$# Sécurité du code — le premier mois

## Où commencer à lire

Ici. Le code de cette plateforme est public et ses auteurs demandent à être
audités : `skilluv-backend`, `skilluv-frontend`, `skilluv-admin`,
`skilluv-ia`. Le catalogue contient un exercice d'audit sur le code
d'authentification, un sur celui d'autorisation et un sur la gestion des
fichiers, chacun avec un périmètre et un relecteur.

Lire le service déployé est une autre autorisation et relève du périmètre
publié. Lire le code ne demande rien.

## Jours 1 à 10 — une classe de défaut, jusqu'au bout

Prenez l'injection. Apprenez à quoi elle ressemble dans le langage devant
vous, puis trouvez tous les endroits d'une base de code où une entrée non
fiable atteint une requête, et vérifiez-les un par un.

Vous constaterez que presque tous vont bien. C'est le travail : un audit
consiste surtout à établir que les choses sont correctes, et la découverte est
ce qui reste.

Écrivez ce que vous avez vérifié et jugé sain. Un audit qui rapporte trois
découvertes et aucune couverture ne dit rien au lecteur sur le soin avec
lequel il a été lu.

## Jours 11 à 20 — l'outil, et ce en quoi il se trompe

Installez Semgrep et passez-le sur la même base. Puis triez chaque
signalement : réel, inatteignable, ou faux positif, avec la raison.

C'est l'exercice qui fait un auditeur. Un outil qui produit deux cents
signalements ne vous a rien dit tant que personne ne les a lus, et cette
lecture est le savoir-faire que rien n'automatise. Écrivez le tri — les
signalements écartés avec leurs raisons sont la moitié de ce qui rend un audit
crédible.

Puis écrivez une règle Semgrep, pour un défaut que vous avez réellement
trouvé. Une règle est une découverte qui continue de fonctionner après votre
départ.

## Jours 21 à 30 — le rapport

Chaque découverte a besoin de quatre choses, non négociables :

- **Le chemin**, du point d'entrée au point d'arrivée, avec fichier et ligne à
  chaque étape. Une découverte qui nomme un point d'arrivée sans source
  atteignable est un signalement d'outil avec un paragraphe autour.
- **L'atteignabilité** : la configuration, le drapeau, la route qui fait
  exécuter ce code.
- **Un correctif**, concret et à la bonne couche. « Nettoyer l'entrée » n'en
  est pas un ; la requête paramétrée en est un.
- **L'impact**, en termes de ce que l'attaquant obtient.

## La seule chose à laquelle faire attention

Si vous trouvez un identifiant actif dans le code ou dans l'historique, ce
n'est pas une découverte à publier. Signalez-la en privé, immédiatement, et
masquez-la dans tout ce que vous écrivez. Un audit qui publie une clé valide a
causé l'incident qu'il cherchait.
$md$, 30),

-- ═══════════════════════════════════════════════════════════════════
-- Governance
-- ═══════════════════════════════════════════════════════════════════

('security-onboarding-governance', 'onboarding', 'security', 'governance', 'en',
 'Getting started in security governance',
 'A first month producing documents an auditor would accept — starting with an audit of ours.',
$md$# Security governance — the first month

## The trade, in one sentence

Writing down what an organisation actually does about risk, in a form somebody
external will accept, and then being audited on it. The artefact is a
document. The test is whether an auditor takes it.

## Why this is not the soft option

Every other trade in this domain is judged on whether something worked. This
one is judged on whether a stranger with a checklist agrees — which is harder
to fake and much harder to bluff, because the checklist is public.

## Days 1–8 — read a framework properly, once

Pick one: the GDPR if you are in Europe, ISO 27001 if you want the management
system, SOC 2 if you are aiming at companies selling to the United States.

Read the actual text, not a summary. Frameworks are shorter than their
commentary and far more precise. The GDPR is 99 articles and you need about
fifteen of them.

Then read OWASP ASVS end to end. It is the clearest thing ever written about
turning "is it secure" into a list somebody can be audited against.

## Days 9–20 — audit ours

This platform publishes `PRIVACY.md`, `THREAT_MODEL.md` and
`INCIDENT_RESPONSE.md`, and there is an audit exercise for each of them in the
catalogue. Take one.

Go article by article for the ones that apply. For each claim, ask the
question an auditor asks: **what evidence would I be shown, and does it
exist?** A policy that says "access is reviewed quarterly" with no review
record is a finding, and it is the most common one there is.

Report what is missing, what is wrong, and what is unevidenced — three
different things, and conflating them is what makes a compliance report
unreadable.

## Days 21–30 — write one, properly

Write a policy of your own: access control, retention, or incident response.
Three tests, and it must pass all of them:

- **Short enough to be read.** Nobody complies with eleven pages.
- **Specific enough to be audited.** "Appropriate measures" is not a control.
- **Possible on an ordinary day.** A control that requires heroism gets
  bypassed and then documented as met, which is worse than not having it.

Then write down the residual risk you are not fixing, why, and who accepted
it. An unowned acceptance is how a finding survives three audits.

## What this trade needs from you that the others do not

Patience with people. Every control you write is something somebody else has
to do, and a governance specialist who has never negotiated one is writing
fiction.
$md$, 40),

('security-onboarding-governance', 'onboarding', 'security', 'governance', 'fr',
 'Débuter en gouvernance de la sécurité',
 'Un premier mois à produire des documents qu''un auditeur accepterait — en commençant par auditer les nôtres.',
$md$# Gouvernance de la sécurité — le premier mois

## Le métier, en une phrase

Écrire ce qu'une organisation fait réellement du risque, sous une forme
qu'une personne extérieure accepte, puis être audité dessus. L'artefact est un
document. Le test est de savoir si un auditeur le prend.

## Pourquoi ce n'est pas la voie facile

Tous les autres métiers de ce domaine sont jugés sur le fait que quelque chose
a marché. Celui-ci est jugé sur l'accord d'un inconnu muni d'une liste — plus
difficile à simuler, et beaucoup plus difficile à bluffer, parce que la liste
est publique.

## Jours 1 à 8 — lire un référentiel correctement, une fois

Choisissez-en un : le RGPD si vous êtes en Europe, l'ISO 27001 si vous voulez
le système de management, le SOC 2 si vous visez des entreprises qui vendent
aux États-Unis.

Lisez le texte, pas un résumé. Les référentiels sont plus courts que leurs
commentaires et beaucoup plus précis. Le RGPD fait 99 articles et il vous en
faut une quinzaine.

Puis lisez l'OWASP ASVS de bout en bout. C'est ce qui a été écrit de plus
clair pour transformer « est-ce sécurisé » en une liste auditable.

## Jours 9 à 20 — auditez les nôtres

Cette plateforme publie `PRIVACY.md`, `THREAT_MODEL.md` et
`INCIDENT_RESPONSE.md`, et le catalogue contient un exercice d'audit pour
chacun. Prenez-en un.

Allez article par article pour ceux qui s'appliquent. Pour chaque affirmation,
posez la question de l'auditeur : **quelle preuve me montrerait-on, et
existe-t-elle ?** Une politique qui dit « les accès sont revus chaque
trimestre » sans trace de revue est une non-conformité, et c'est la plus
fréquente de toutes.

Signalez ce qui manque, ce qui est faux, et ce qui n'est pas prouvé — trois
choses différentes, et les confondre est ce qui rend un rapport de conformité
illisible.

## Jours 21 à 30 — en écrire une, correctement

Écrivez votre propre politique : contrôle d'accès, conservation, ou réponse à
incident. Trois tests, et elle doit passer les trois :

- **Assez courte pour être lue.** Personne ne se conforme à onze pages.
- **Assez précise pour être auditée.** « Mesures appropriées » n'est pas un
  contrôle.
- **Possible un jour ordinaire.** Un contrôle qui exige de l'héroïsme est
  contourné puis documenté comme respecté, ce qui est pire que son absence.

Puis écrivez le risque résiduel que vous ne corrigez pas, pourquoi, et qui
l'accepte. Une acceptation sans propriétaire est la façon dont une
non-conformité survit à trois audits.

## Ce que ce métier demande et que les autres non

De la patience avec les gens. Chaque contrôle que vous écrivez est quelque
chose que quelqu'un d'autre devra faire, et un spécialiste de la gouvernance
qui n'en a jamais négocié un écrit de la fiction.
$md$, 40),

-- ═══════════════════════════════════════════════════════════════════
-- Purple team
-- ═══════════════════════════════════════════════════════════════════

('security-onboarding-purple-team', 'onboarding', 'security', 'purple-team', 'en',
 'Getting started in purple team work',
 'A first month running known techniques on purpose and proving the detection for them fires.',
$md$# Purple team — the first month

## Not the union of the other two

A purple exercise is not "some attacking and some defending". It is one
question: **would we have seen this?** — asked by running the technique and
looking.

Which means the output is never a report. It is a detection that did not exist
that morning, committed somewhere, with the evidence that it fires.

## Before anything: somewhere you are allowed to break

You need an environment of your own. Two virtual machines and a network
between them is enough, and it must not be anything you or anybody else
depends on. Every technique you are about to run changes something, and the
cleanup sometimes fails.

Do not do this on a work machine. Do not do it on a machine on a work network.

## Days 1–10 — one technique, both sides

Install Atomic Red Team. Pick one test — a credential dump, a scheduled task,
a suspicious parent process — and:

1. Read what it does, and what it says it changes.
2. Run it.
3. Look at the telemetry: what appeared in the event log, in the process
   tree, on the network?
4. Write the detection.
5. Run the technique again and watch the detection fire.

Step 5 is the one everybody skips, and skipping it is how estates end up with
four hundred rules and no coverage.

## Days 11–20 — ATT&CK, as a vocabulary and not a poster

Map what you did to its ATT&CK technique identifier. Then take one tactic —
persistence, say — and work through five techniques in it the same way.

At the end, write your coverage statement. Not "we cover persistence": five
techniques run, three detected reliably, one detected with unacceptable noise,
one invisible. **The invisible one is the most valuable line in the
document**, and the reason coverage claims made without running anything are
worthless.

## Days 21–30 — facilitate something

Find one other person. One of you runs three techniques in a window; the other
watches and writes down what they saw and when. Then compare notes on one
timeline.

That conversation — "I ran it at 14:07 and you saw nothing until 14:22" — is
the entire value of the format, and it cannot be had alone.

Write it up: the techniques, the timeline from both sides, the gaps ranked by
what closing each would cost. That document is the artefact this trade is
judged on.

## Where to do it here

This platform runs purple exercises as competitions with two sides scored
separately, and the outcome is expected to be a detection. Read
`docs/security/COMPETITIONS-PLAYBOOK.md` before proposing one.
$md$, 50),

('security-onboarding-purple-team', 'onboarding', 'security', 'purple-team', 'fr',
 'Débuter en purple team',
 'Un premier mois à exécuter des techniques connues exprès et à prouver que la détection se déclenche.',
$md$# Purple team — le premier mois

## Pas la somme des deux autres

Un exercice purple n'est pas « certains attaquent, d'autres défendent ». C'est
une seule question : **l'aurions-nous vu ?** — posée en exécutant la technique
et en regardant.

Ce qui signifie que la sortie n'est jamais un rapport. C'est une détection qui
n'existait pas le matin, versionnée quelque part, avec la preuve qu'elle se
déclenche.

## Avant tout : un endroit où vous avez le droit de casser

Il vous faut un environnement à vous. Deux machines virtuelles et un réseau
entre elles suffisent, et cela ne doit être rien dont vous ou quelqu'un
d'autre dépendez. Chaque technique que vous allez exécuter modifie quelque
chose, et le nettoyage échoue parfois.

Pas sur une machine de travail. Pas sur une machine d'un réseau de travail.

## Jours 1 à 10 — une technique, des deux côtés

Installez Atomic Red Team. Choisissez un test — une extraction
d'identifiants, une tâche planifiée, un processus parent suspect — et :

1. Lisez ce qu'il fait et ce qu'il annonce modifier.
2. Exécutez-le.
3. Regardez la télémétrie : qu'est-ce qui est apparu dans le journal
   d'événements, dans l'arbre des processus, sur le réseau ?
4. Écrivez la détection.
5. Réexécutez la technique et regardez la détection se déclencher.

L'étape 5 est celle que tout le monde saute, et la sauter est la raison pour
laquelle des parcs finissent avec quatre cents règles et aucune couverture.

## Jours 11 à 20 — ATT&CK comme vocabulaire, pas comme affiche

Rattachez ce que vous avez fait à son identifiant de technique ATT&CK. Puis
prenez une tactique — la persistance, par exemple — et traitez cinq techniques
de la même façon.

À la fin, écrivez votre déclaration de couverture. Pas « nous couvrons la
persistance » : cinq techniques exécutées, trois détectées de façon fiable,
une détectée avec un bruit inacceptable, une invisible. **L'invisible est la
ligne la plus précieuse du document**, et la raison pour laquelle une
couverture annoncée sans rien exécuter ne vaut rien.

## Jours 21 à 30 — animez quelque chose

Trouvez une autre personne. L'un exécute trois techniques dans une fenêtre,
l'autre regarde et note ce qu'il a vu et quand. Puis comparez sur une seule
chronologie.

Cette conversation — « je l'ai lancé à 14 h 07 et tu n'as rien vu avant
14 h 22 » — est toute la valeur du format, et elle ne s'obtient pas seul.

Rédigez : les techniques, la chronologie des deux côtés, les angles morts
classés par ce que coûterait de les fermer. Ce document est l'artefact sur
lequel ce métier est jugé.

## Où le faire ici

Cette plateforme organise des exercices purple sous forme de compétitions avec
deux camps notés séparément, et le résultat attendu est une détection. Lisez
`docs/security/COMPETITIONS-PLAYBOOK.md` avant d'en proposer un.
$md$, 50);
