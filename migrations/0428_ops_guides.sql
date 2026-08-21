-- Eight ops onboarding guides, one per trade, in two languages.
--
-- Rows and not files, for the reason migration 0199 gave: they have to be
-- translated and edited by somebody who is not deploying.
--
-- ## Why one per trade rather than one per family
--
-- The review grids are per family, because a person who reads a Terraform
-- plan reads a Helm chart. Arriving is different: somebody who wants to
-- become a database administrator and somebody who wants to become a
-- platform engineer share a reviewer and share almost nothing else about
-- their first month. `reviewer_group` still carries the family, so the
-- listing can group them.
--
-- ## What each one has to answer
--
-- Where to practise without a budget. In this domain that is not a footnote:
-- a first month that requires a cloud account nobody can pay for is a first
-- month that does not happen. Every guide names something free and real.

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary,
     body_md, sort_order)
VALUES

-- ═══════════════════════════════════════════════════════════════════
-- DevOps Engineer
-- ═══════════════════════════════════════════════════════════════════

('ops-onboarding-devops', 'onboarding', 'ops', 'infra', 'fr',
 'Débuter comme DevOps Engineer',
 'Construire, tester, livrer — et rendre le tout rejouable par quelqu''un d''autre.',
$md$
# Débuter comme DevOps Engineer

Le métier tient en une phrase : faire en sorte que ce qui marche sur ta
machine marche ailleurs, sans toi. Tout le reste — pipelines, conteneurs,
modules — sert cette phrase.

## Trente jours

**Semaine 1 — un conteneur.** Prends un projet que tu connais et écris son
Dockerfile. L'image doit démarrer sans que tu touches à rien. Note ce que tu
as dû découvrir en cours de route : c'est de la documentation qui manquait.

**Semaine 2 — un pipeline.** Le même projet, un pipeline qui construit, teste
et publie l'image. GitHub Actions est gratuit sur dépôt public : commence
là. Un pipeline qui échoue pour une bonne raison vaut mieux qu'un pipeline
vert qui ne teste rien.

**Semaine 3 — un module.** Décris une petite infrastructure en Terraform ou
OpenTofu, applique-la deux fois, et vérifie que la seconde ne change rien.
C'est la propriété qui sépare l'infrastructure comme code du script shell.

**Semaine 4 — les secrets.** Reprends les trois semaines et cherche ce qui
traîne en clair. Un secret dans un dépôt est un refus automatique en
relecture, quel que soit le reste.

## Où pratiquer sans budget

Oracle Cloud Free Tier donne plusieurs cœurs ARM en permanence. Cloudflare
Workers ne demande pas de carte bancaire. Pour le reste, Docker et k3s
tournent sur ta machine.

## Premier défi

« Un pipeline depuis rien » : un dépôt sans intégration continue, et un
README que quelqu'un d'autre suit sans toi.

## Où sont les gens

`#ops-devops` sur le Discord.
$md$,
 10),

('ops-onboarding-devops', 'onboarding', 'ops', 'infra', 'en',
 'Getting started as a DevOps Engineer',
 'Build, test, ship — and make the whole thing replayable by somebody else.',
$md$
# Getting started as a DevOps Engineer

The trade is one sentence: make what works on your machine work elsewhere,
without you. Pipelines, containers and modules all serve that sentence.

## Thirty days

**Week 1 — a container.** Take a project you know and write its Dockerfile.
The image must start without you touching anything. Write down what you had
to discover along the way: that is documentation somebody was missing.

**Week 2 — a pipeline.** Same project, a pipeline that builds, tests and
publishes the image. GitHub Actions is free on public repositories, so start
there. A pipeline that fails for a good reason beats a green one that tests
nothing.

**Week 3 — a module.** Describe a small piece of infrastructure in Terraform
or OpenTofu, apply it twice, and check the second run changes nothing. That
property is what separates infrastructure as code from a shell script.

**Week 4 — secrets.** Go back over the three weeks and look for anything in
plain text. A secret in a repository is an automatic refusal at review,
whatever the rest looks like.

## Where to practise with no budget

Oracle Cloud's free tier gives several ARM cores permanently. Cloudflare
Workers asks for no card at all. Everything else — Docker, k3s — runs on your
own machine.

## First challenge

"A pipeline from nothing": a repository with no CI, and a README somebody
else follows without you.

## Where people are

`#ops-devops` on Discord.
$md$,
 10),

-- ═══════════════════════════════════════════════════════════════════
-- SRE
-- ═══════════════════════════════════════════════════════════════════

('ops-onboarding-sre', 'onboarding', 'ops', 'reliability', 'fr',
 'Débuter comme SRE',
 'Promettre un chiffre, le mesurer, et dire ce qui se passe quand il n''est pas tenu.',
$md$
# Débuter comme SRE

Le métier n'est pas « faire en sorte que ça ne tombe jamais ». C'est
décider combien d'indisponibilité est acceptable, le dire à voix haute, et
organiser le travail autour de cette décision.

## Trente jours

**Semaine 1 — choisir un service.** N'importe lequel, même un projet
personnel. Écris en une phrase ce qu'il fait, telle que son utilisateur la
reconnaîtrait. Si tu n'y arrives pas, tu ne peux pas encore définir sa
disponibilité.

**Semaine 2 — un objectif.** Une cible, une fenêtre, une source de mesure.
« 99,9 % sur trente jours, mesuré par une sonde externe » est un objectif ;
« fiable » n'en est pas un.

**Semaine 3 — l'instrumenter.** Prometheus en local suffit. Ce qui compte
est que le chiffre soit lisible sans te demander.

**Semaine 4 — le budget d'erreur.** Calcule ce que ta cible laisse comme
marge, et écris ce que tu ferais si elle était épuisée. Ce document est le
livrable ; la cible seule ne change le comportement de personne.

## Ce qui fait échouer une relecture

Un objectif que l'architecture rend impossible. Un runbook qui commence par
« demander à ». Un budget jamais entamé, qui signale une cible trop basse et
personne pour le dire.

## Premier défi

« Un premier objectif de service » : la cible, la source accessible au
relecteur, et le résultat après trente jours.

## Où sont les gens

`#ops-sre` sur le Discord. Le livre SRE de Google est lisible en ligne
gratuitement, en entier.
$md$,
 20),

('ops-onboarding-sre', 'onboarding', 'ops', 'reliability', 'en',
 'Getting started as an SRE',
 'Promise a number, measure it, and say what happens when it is missed.',
$md$
# Getting started as an SRE

The trade is not "make sure it never falls over". It is deciding how much
unavailability is acceptable, saying so out loud, and organising the work
around that decision.

## Thirty days

**Week 1 — pick a service.** Any of them, a personal project included. Write
in one sentence what it does, phrased so its user would recognise it. If you
cannot, you cannot yet define its availability.

**Week 2 — an objective.** A target, a window, a source of measurement.
"99.9% over thirty days, measured by an external probe" is an objective;
"reliable" is not.

**Week 3 — instrument it.** Prometheus on your own machine is enough. What
matters is that the figure is readable without asking you.

**Week 4 — the error budget.** Work out what your target leaves as margin,
and write what you would do if it ran out. That document is the deliverable;
the target alone changes nobody's behaviour.

## What fails a review

An objective the architecture makes impossible. A runbook that starts with
"ask". A budget never touched, which signals a target set too low and nobody
willing to say so.

## First challenge

"A first service objective": the target, a source the reviewer can open, and
the result after thirty days.

## Where people are

`#ops-sre` on Discord. Google's SRE book is free to read online, in full.
$md$,
 20),

-- ═══════════════════════════════════════════════════════════════════
-- Cloud Architect
-- ═══════════════════════════════════════════════════════════════════

('ops-onboarding-cloud', 'onboarding', 'ops', 'cloud', 'fr',
 'Débuter comme architecte cloud',
 'Un schéma qui ne dit pas ce qu''il coûte n''est pas une architecture.',
$md$
# Débuter comme architecte cloud

Concevoir n'est pas dessiner. Une architecture est une suite de compromis,
et le travail consiste à les écrire : ce qui a été choisi, ce qui a été
écarté, et ce que ça coûtera à la fin du mois.

## Trente jours

**Semaine 1 — lire une facture.** Prends une facture cloud réelle, la tienne
ou une publique, et explique chaque ligne. La plupart des architectes ne
savent pas ce qu'ils paient.

**Semaine 2 — une charge.** Choisis une application et écris sa charge
attendue : au repos, au pic, dans douze mois. Sans ces trois chiffres,
dimensionner est deviner.

**Semaine 3 — le schéma.** Dessine, puis chiffre poste par poste. Un schéma
sans facture estimée est une architecture qu'on découvrira.

**Semaine 4 — la sortie.** Écris ce qu'il faudrait réécrire pour changer de
fournisseur. Pas pour en changer : pour savoir ce que l'enfermement coûte,
et l'accepter en connaissance de cause.

## Où pratiquer sans budget

Les paliers gratuits des quatre grands, avec une alerte de budget à un euro
posée avant tout le reste. Oracle a le palier permanent le plus généreux.

## Premier défi

« Une architecture chiffrée » : le schéma, les hypothèses de charge, et le
coût mensuel estimé.

## Où sont les gens

`#ops-cloud` sur le Discord.
$md$,
 30),

('ops-onboarding-cloud', 'onboarding', 'ops', 'cloud', 'en',
 'Getting started as a cloud architect',
 'A diagram that does not say what it costs is not an architecture.',
$md$
# Getting started as a cloud architect

Designing is not drawing. An architecture is a sequence of trade-offs, and
the work is writing them down: what was chosen, what was set aside, and what
it will cost at the end of the month.

## Thirty days

**Week 1 — read a bill.** Take a real cloud invoice, yours or a public one,
and explain every line. Most architects do not know what they are paying for.

**Week 2 — a load.** Pick an application and write its expected load: at
rest, at peak, in twelve months. Without those three figures, sizing is
guessing.

**Week 3 — the diagram.** Draw it, then price it line by line. A diagram with
no estimated bill is an architecture somebody will discover.

**Week 4 — the exit.** Write what would have to be rewritten to change
provider. Not in order to change: in order to know what the lock-in costs,
and to accept it knowingly.

## Where to practise with no budget

The free tiers of the big four, with a one-euro budget alert set before
anything else. Oracle has the most generous permanent tier.

## First challenge

"A priced architecture": the diagram, the load assumptions, and the estimated
monthly cost.

## Where people are

`#ops-cloud` on Discord.
$md$,
 30),

-- ═══════════════════════════════════════════════════════════════════
-- Platform Engineer
-- ═══════════════════════════════════════════════════════════════════

('ops-onboarding-platform', 'onboarding', 'ops', 'infra', 'fr',
 'Débuter comme platform engineer',
 'Ton utilisateur est un développeur, et il ne lira pas ta documentation s''il peut l''éviter.',
$md$
# Débuter comme platform engineer

Une plateforme interne est un produit, et ses utilisateurs sont tes
collègues. Ce qui la juge n'est pas son élégance : c'est le temps entre
« j'ai une idée » et « c'est en production ».

## Trente jours

**Semaine 1 — mesurer.** Chronomètre le chemin actuel entre un commit et sa
mise en production, dans un projet réel. Le chiffre sera plus mauvais que ce
que tout le monde croit.

**Semaine 2 — écouter.** Note les cinq questions les plus posées à l'équipe
d'infrastructure. Mesurées, pas devinées : ce que les gens demandent n'est
presque jamais ce qu'on croit.

**Semaine 3 — un chemin par défaut.** Construis le trajet le plus court pour
mettre un service en production, et documente-le.

**Semaine 4 — le faire suivre.** Quelqu'un qui n'est pas toi le suit du
début à la fin. Ce qu'il bloque est ton vrai retour ; tout ce que tu penses
avant ce moment est une hypothèse.

## Ce qui distingue ce métier du DevOps

Le DevOps résout le problème. Le platform engineer fait en sorte que le
problème ne se pose plus pour les vingt personnes suivantes. Si ta solution
ne sert qu'une équipe, c'est du DevOps, et c'est très bien — mais ce n'est
pas ce métier.

## Premier défi

« Un chemin par défaut », avec la trace de quelqu'un d'autre l'ayant suivi.

## Où sont les gens

`#ops-platform` sur le Discord.
$md$,
 40),

('ops-onboarding-platform', 'onboarding', 'ops', 'infra', 'en',
 'Getting started as a platform engineer',
 'Your user is a developer, and they will avoid reading your documentation if they can.',
$md$
# Getting started as a platform engineer

An internal platform is a product and its users are your colleagues. What
judges it is not its elegance: it is the time between "I have an idea" and
"it is in production".

## Thirty days

**Week 1 — measure.** Time the current path from a commit to production, in
a real project. The number will be worse than everybody believes.

**Week 2 — listen.** Write down the five questions the infrastructure team is
asked most. Measured, not guessed: what people ask for is almost never what
you assume.

**Week 3 — a golden path.** Build the shortest route to putting a service in
production, and document it.

**Week 4 — have it followed.** Somebody who is not you follows it end to end.
Where they get stuck is your real feedback; everything you thought before
that moment was a hypothesis.

## What separates this from DevOps

DevOps solves the problem. Platform engineering makes the problem stop
happening for the next twenty people. If your solution serves one team, it is
DevOps, and that is fine — but it is not this trade.

## First challenge

"A golden path", with the trace of somebody else having followed it.

## Where people are

`#ops-platform` on Discord.
$md$,
 40),

-- ═══════════════════════════════════════════════════════════════════
-- Kubernetes Specialist
-- ═══════════════════════════════════════════════════════════════════

('ops-onboarding-kubernetes', 'onboarding', 'ops', 'infra', 'fr',
 'Débuter comme spécialiste Kubernetes',
 'Un cluster local d''abord. Personne n''apprend ce métier sur la production de quelqu''un.',
$md$
# Débuter comme spécialiste Kubernetes

Kubernetes récompense la patience et punit la précipitation. Ce qui sépare
quelqu'un qui l'utilise de quelqu'un qui le connaît, c'est de savoir ce
qui se passe entre `kubectl apply` et le conteneur qui démarre.

## Trente jours

**Semaine 1 — un cluster à toi.** k3s ou kind, sur ta machine. Déploie
quelque chose de simple et casse-le exprès. Un pod qui refuse de démarrer est
le meilleur exercice du mois.

**Semaine 2 — les objets.** Deployment, Service, Ingress, ConfigMap, Secret.
Écris-les à la main avant d'utiliser un chart : Helm cache la moitié de ce
qu'il y a à comprendre.

**Semaine 3 — un chart.** Empaquette ton application, et vérifie qu'une
montée de version conserve les données. C'est là que la plupart des charts
échouent.

**Semaine 4 — GitOps.** Argo CD ou Flux, sur ton cluster local. Le
déploiement cesse d'être un geste et devient un état écrit dans un dépôt.

## L'étape d'après

Un opérateur. Kubebuilder est le point d'entrée, et un opérateur qui gère un
objet simple de bout en bout est une contribution sérieuse à un portfolio.

## Ce qui fait échouer une relecture

Des limites de ressources posées au hasard. Un secret dans un manifeste. Un
`apply` qui ne peut être joué qu'une fois.

## Où sont les gens

`#ops-k8s` sur le Discord.
$md$,
 50),

('ops-onboarding-kubernetes', 'onboarding', 'ops', 'infra', 'en',
 'Getting started as a Kubernetes specialist',
 'A local cluster first. Nobody learns this trade on somebody else''s production.',
$md$
# Getting started as a Kubernetes specialist

Kubernetes rewards patience and punishes haste. What separates somebody who
uses it from somebody who knows it is understanding what happens between
`kubectl apply` and the container starting.

## Thirty days

**Week 1 — a cluster of your own.** k3s or kind, on your machine. Deploy
something simple and break it on purpose. A pod that refuses to start is the
best exercise of the month.

**Week 2 — the objects.** Deployment, Service, Ingress, ConfigMap, Secret.
Write them by hand before using a chart: Helm hides half of what there is to
understand.

**Week 3 — a chart.** Package your application, and check that an upgrade
keeps the data. That is where most charts fail.

**Week 4 — GitOps.** Argo CD or Flux, on your local cluster. Deployment stops
being a gesture and becomes a state written in a repository.

## What comes next

An operator. Kubebuilder is the way in, and an operator that handles one
simple object end to end is a serious portfolio piece.

## What fails a review

Resource limits picked at random. A secret in a manifest. An `apply` that can
only be run once.

## Where people are

`#ops-k8s` on Discord.
$md$,
 50),

-- ═══════════════════════════════════════════════════════════════════
-- Observability Engineer
-- ═══════════════════════════════════════════════════════════════════

('ops-onboarding-observability', 'onboarding', 'ops', 'observability', 'fr',
 'Débuter comme ingénieur observabilité',
 'Trois questions auxquelles il faut pouvoir répondre en deux minutes. Le reste est décoratif.',
$md$
# Débuter comme ingénieur observabilité

Le piège du métier est de confondre « beaucoup de données » et « on sait ce
qui se passe ». Un mur de graphiques n'est pas de l'observabilité ; trois
questions qui trouvent leur réponse en deux minutes, si.

## Trente jours

**Semaine 1 — trois questions.** Choisis un service et écris les trois
questions qu'on se pose pendant un incident. Sans cette liste, tout ce que
tu construiras ensuite sera une décoration.

**Semaine 2 — les métriques.** Prometheus, en local. Instrumente ce qu'il
faut pour répondre à la première question, et rien d'autre.

**Semaine 3 — les traces.** OpenTelemetry, à travers au moins deux services.
La norme d'abord, l'agent propriétaire ensuite si vraiment nécessaire :
instrumenter une fois permet de changer d'outil sans tout réécrire.

**Semaine 4 — les alertes.** Une seule, sur un symptôme utilisateur, qui dit
quoi faire. Une alerte de seuil de ressource sans lien avec un symptôme est
un refus en relecture.

## La facture

La cardinalité coûte. Une étiquette par identifiant utilisateur est une
facture, pas une métrique. Connaître le coût de ce qu'on ingère fait partie
du métier, au même titre que le tableau de bord.

## Premier défi

« Trois questions, deux minutes » : les questions, les tableaux de bord, et
le chronométrage.

## Où sont les gens

`#ops-observability` sur le Discord.
$md$,
 60),

('ops-onboarding-observability', 'onboarding', 'ops', 'observability', 'en',
 'Getting started as an observability engineer',
 'Three questions answerable in two minutes. The rest is decoration.',
$md$
# Getting started as an observability engineer

The trap of this trade is mistaking "a lot of data" for "we know what is
happening". A wall of graphs is not observability; three questions that find
their answer in two minutes is.

## Thirty days

**Week 1 — three questions.** Pick a service and write the three questions
people ask during an incident. Without that list, everything you build after
is decoration.

**Week 2 — metrics.** Prometheus, locally. Instrument what answers the first
question, and nothing else.

**Week 3 — traces.** OpenTelemetry, across at least two services. The
standard first, a vendor agent later if genuinely needed: instrumenting once
is what lets you change tool without rewriting everything.

**Week 4 — alerts.** One alert, on a user-facing symptom, that says what to
do. A resource-threshold alert with no link to a symptom is a refusal at
review.

## The bill

Cardinality costs. One label per user id is an invoice, not a metric. Knowing
what you are paying to ingest is part of the trade, as much as the dashboard
is.

## First challenge

"Three questions, two minutes": the questions, the dashboards, and the timing.

## Where people are

`#ops-observability` on Discord.
$md$,
 60),

-- ═══════════════════════════════════════════════════════════════════
-- Incident Commander
-- ═══════════════════════════════════════════════════════════════════

('ops-onboarding-incident', 'onboarding', 'ops', 'reliability', 'fr',
 'Débuter comme responsable d''incident',
 'Un métier technique dont le livrable est un texte.',
$md$
# Débuter comme responsable d'incident

Conduire un incident n'est pas le réparer. C'est décider qui cherche quoi,
tenir la chronologie, et parler à ceux qui attendent — pendant que d'autres
ont les mains dans le système.

## Trente jours

**Semaine 1 — lire des post-mortems.** Ceux de Cloudflare, GitLab, AWS sont
publics. Repère ce qu'ils disent du système et ce qu'ils ne disent de
personne.

**Semaine 2 — les modèles de communication.** Écris trois messages : interne,
client, public. Fais-les relire par quelqu'un qui n'est pas technique. Si
la personne ne comprend pas, ils sont à refaire.

**Semaine 3 — un exercice.** Provoque une panne en environnement contrôlé,
avec au moins une autre personne. Tiens la chronologie pendant, pas après.

**Semaine 4 — le post-mortem.** Deux cents caractères minimum, aucune
personne nommée, et au moins une action portée et datée. Un post-mortem qui
ne conclut rien à faire n'a pas cherché.

## Pourquoi personne n'est nommé

Ce n'est pas de la politesse. Un post-mortem qui nomme quelqu'un est un
post-mortem que personne n'écrit honnêtement la fois suivante, et c'est la
fois suivante qui compte. Sur Skilluv, aucune colonne ne permet d'enregistrer
qui a causé un incident.

## Premier défi

« Conduire un exercice » : le scénario, la chronologie réelle, et le compte
rendu.

## Où sont les gens

`#ops-incident` et `#ops-incidents-lounge` sur le Discord.
$md$,
 70),

('ops-onboarding-incident', 'onboarding', 'ops', 'reliability', 'en',
 'Getting started as an incident commander',
 'A technical trade whose deliverable is a piece of writing.',
$md$
# Getting started as an incident commander

Running an incident is not fixing it. It is deciding who investigates what,
holding the timeline, and talking to the people waiting — while others have
their hands in the system.

## Thirty days

**Week 1 — read post-mortems.** Cloudflare's, GitLab's and AWS's are public.
Notice what they say about the system and what they say about nobody.

**Week 2 — communication templates.** Write three messages: internal,
customer, public. Have them read by somebody non-technical. If they do not
understand, the messages need rewriting.

**Week 3 — a drill.** Cause a failure in a controlled environment, with at
least one other person. Hold the timeline during, not after.

**Week 4 — the post-mortem.** Two hundred characters minimum, nobody named,
and at least one action with an owner and a date. A post-mortem concluding
nothing to do did not look.

## Why nobody is named

This is not politeness. A post-mortem that names somebody is one nobody
writes honestly the next time, and it is the next time that matters. On
Skilluv there is no column anywhere for who caused an incident.

## First challenge

"Run a drill": the scenario, the real timeline, and the write-up.

## Where people are

`#ops-incident` and `#ops-incidents-lounge` on Discord.
$md$,
 70),

-- ═══════════════════════════════════════════════════════════════════
-- Database Administrator
-- ═══════════════════════════════════════════════════════════════════

('ops-onboarding-database', 'onboarding', 'ops', 'data', 'fr',
 'Débuter comme administrateur de bases de données',
 'Le seul métier ops où une erreur ne se rattrape pas par un redéploiement.',
$md$
# Débuter comme administrateur de bases de données

Partout ailleurs en ops, on peut redéployer. Ici, une migration mal jouée
détruit des données qui n'existent nulle part ailleurs. Le métier
s'apprend dans cet ordre : restaurer d'abord, optimiser ensuite.

## Trente jours

**Semaine 1 — restaurer.** Prends une base, sauvegarde-la, détruis-la,
restaure-la. Chronomètre. Une sauvegarde jamais restaurée n'est pas une
sauvegarde, et c'est la première chose qu'un relecteur demande.

**Semaine 2 — lire un plan.** `EXPLAIN ANALYZE` sur des requêtes réelles,
avec un volume réaliste. Sur mille lignes, tout est rapide et rien ne
s'apprend.

**Semaine 3 — un index.** Ajoute-en un à partir d'un plan, et mesure les
deux côtés : le gain en lecture et le coût en écriture. Un index sans requête
qui l'utilise est un coût permanent.

**Semaine 4 — une migration.** Modifie une table volumineuse sans verrou
long. Écris le volume de la table et la durée du verrou mesurée : sans ces
deux chiffres, personne ne peut relire.

## La réplication

Quand tu la mettras en place, instrumente le décalage avant d'en avoir
besoin. Une bascule se prépare ; sinon elle se découvre pendant.

## Premier défi

« Restaurer pour de vrai » : la procédure, la durée obtenue, et ce qui a
manqué.

## Où sont les gens

`#ops-db` sur le Discord. La documentation PostgreSQL est, à elle seule, une
formation complète.
$md$,
 80),

('ops-onboarding-database', 'onboarding', 'ops', 'data', 'en',
 'Getting started as a database administrator',
 'The one ops trade where a mistake cannot be undone by redeploying.',
$md$
# Getting started as a database administrator

Everywhere else in ops you can redeploy. Here, a migration run badly destroys
data that exists nowhere else. The trade is learnt in that order: restore
first, optimise second.

## Thirty days

**Week 1 — restore.** Take a database, back it up, destroy it, restore it.
Time it. A backup never restored is not a backup, and it is the first thing a
reviewer asks about.

**Week 2 — read a plan.** `EXPLAIN ANALYZE` on real queries, at realistic
volume. On a thousand rows everything is fast and nothing is learnt.

**Week 3 — an index.** Add one from a query plan, and measure both sides: the
read gain and the write cost. An index with no query using it is a permanent
cost.

**Week 4 — a migration.** Alter a large table without a long lock. Write down
the table's size and the measured lock duration: without those two figures
nobody can review it.

## Replication

When you set it up, instrument the lag before you need it. A failover is
prepared; otherwise it is discovered during.

## First challenge

"Restore for real": the procedure, the duration you got, and what was
missing.

## Where people are

`#ops-db` on Discord. The PostgreSQL documentation is, on its own, a complete
course.
$md$,
 80)

ON CONFLICT (slug, locale) DO NOTHING;
