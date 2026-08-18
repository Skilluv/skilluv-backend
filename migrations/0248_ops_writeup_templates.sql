-- Ten ops write-up templates, in two languages.
--
-- In this domain the document often is the artefact. A runbook nobody can
-- follow at three in the morning has not been written, however good the
-- system behind it is, and a post-mortem without owned actions is an
-- anecdote. These are the skeletons, with the fields people skip marked as
-- what they are.
--
-- ## The field every one of them shares
--
-- What would have to be true for this to be wrong. It appears under different
-- names — the lock duration, the measurement source, the rollback — and it is
-- always the field that separates a document somebody can check from one they
-- have to believe.

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary,
     body_md, sort_order)
VALUES

('ops-template-runbook', 'writeup_template', 'ops', 'reliability', 'fr',
 'Runbook de réponse à incident',
 'Écrit pour quelqu''un qui n''a pas construit le système, à trois heures du matin.',
$md$
# Runbook — {nom du service}

**Pour qui** : quelqu'un d'astreinte qui n'a pas construit ce système.
**Dernière relecture** : {date} — un runbook non relu depuis six mois est
suspect.

## Ce que fait ce service, en une phrase

## Comment savoir qu'il va mal

- l'alerte qui se déclenche :
- le tableau de bord à ouvrir en premier :
- ce que voit l'utilisateur :

## Les trois modes de panne connus

### 1. {symptôme}
- **Vérifier** : (la commande exacte, copiable)
- **Si c'est ça** : (les étapes, dans l'ordre)
- **Si ça ne suffit pas** : (à qui parler, et à partir de quand)

### 2. {symptôme}

### 3. {symptôme}

## Ce qu'il ne faut surtout pas faire

(La commande qui aggrave, et pourquoi. Cette section évite plus d'incidents
que les précédentes.)

## Escalade

- après {n} minutes sans amélioration :
- qui décide d'une communication publique :

## Ce que ce runbook ne couvre pas

(Dire ce qu'on ne sait pas encore vaut mieux que laisser croire que tout est
couvert.)
$md$,
 210),

('ops-template-runbook', 'writeup_template', 'ops', 'reliability', 'en',
 'Incident response runbook',
 'Written for somebody who did not build the system, at three in the morning.',
$md$
# Runbook — {service name}

**Written for**: somebody on call who did not build this system.
**Last reviewed**: {date} — a runbook unreviewed for six months is suspect.

## What this service does, in one sentence

## How you know it is unwell

- the alert that fires:
- the first dashboard to open:
- what the user sees:

## The three known failure modes

### 1. {symptom}
- **Check**: (the exact command, copy-pasteable)
- **If it is this**: (the steps, in order)
- **If that is not enough**: (who to talk to, and from when)

### 2. {symptom}

### 3. {symptom}

## What not to do

(The command that makes it worse, and why. This section prevents more
incidents than the ones above it.)

## Escalation

- after {n} minutes with no improvement:
- who decides on a public statement:

## What this runbook does not cover

(Saying what is not known beats letting somebody believe everything is
covered.)
$md$,
 210),

('ops-template-postmortem', 'writeup_template', 'ops', 'reliability', 'fr',
 'Post-mortem sans blâme',
 'Ce que le système a permis. Aucun nom de personne, et des actions portées.',
$md$
# Post-mortem — {titre court}

**Gravité** : sev1 / sev2 / sev3 / sev4
**Détecté après** : {n} minutes · **Résolu après** : {n} minutes
**Utilisateurs concernés** : {estimation, avec la méthode}

## Ce qui s'est passé

(Deux cents caractères minimum. Un post-mortem plus court est un titre.)

## Chronologie

| Heure | Ce qui s'est passé | Comment on l'a su |
|---|---|---|
| | | |

## Ce que le système a permis

(La question centrale. Pas « qui a lancé la commande » mais « pourquoi la
commande était lançable ».)

## Ce qui a bien marché

(À écrire vraiment. Ce qui a permis de détecter en dix minutes plutôt qu'en
deux heures mérite d'être renforcé, pas seulement ce qui a échoué.)

## Actions

| Action | Porteur | Échéance | État |
|---|---|---|---|
| | | | |

Au moins une. Un post-mortem qui ne conclut rien à faire a soit trouvé un
système qui ne peut plus tomber, soit pas cherché.

## Ce qu'on ne sait toujours pas
$md$,
 220),

('ops-template-postmortem', 'writeup_template', 'ops', 'reliability', 'en',
 'Blameless post-mortem',
 'What the system allowed. No names, and actions with owners.',
$md$
# Post-mortem — {short title}

**Severity**: sev1 / sev2 / sev3 / sev4
**Detected after**: {n} minutes · **Resolved after**: {n} minutes
**Users affected**: {estimate, with the method}

## What happened

(Two hundred characters minimum. Anything shorter is a title.)

## Timeline

| Time | What happened | How we knew |
|---|---|---|
| | | |

## What the system allowed

(The central question. Not "who ran the command" but "why the command was
runnable".)

## What went well

(Write this properly. Whatever let it be detected in ten minutes rather than
two hours deserves reinforcing, not only what failed.)

## Actions

| Action | Owner | Due | State |
|---|---|---|---|
| | | | |

At least one. A post-mortem concluding nothing to do either found a system
that cannot fall over, or did not look.

## What we still do not know
$md$,
 220),

('ops-template-iac-readme', 'writeup_template', 'ops', 'infra', 'fr',
 'README de module d''infrastructure',
 'Le test : quelqu''un d''autre l''utilise sans lire le code.',
$md$
# {nom du module}

## Ce que ça crée

(La liste des ressources, en français. Pas la sortie du plan.)

## Utilisation minimale

```hcl
module "exemple" {
  source = "..."
  # les variables obligatoires, et rien d'autre
}
```

## Variables

| Nom | Type | Défaut | Ce que ça change |
|---|---|---|---|
| | | | |

Les valeurs par défaut sont sûres : rien d'ouvert au monde, rien de
permissif « en attendant ».

## Sorties

## Ce qu'il faut savoir avant d'appliquer

- ce qui est créé et qui coûte de l'argent :
- ce qui est détruit à la destruction, et ce qui survit :
- les versions épinglées, et pourquoi :

## Appliquer deux fois

La deuxième exécution ne montre aucune différence. Trace jointe ci-dessous —
c'est ce qu'un relecteur vérifie en premier.

## Limites connues
$md$,
 230),

('ops-template-iac-readme', 'writeup_template', 'ops', 'infra', 'en',
 'Infrastructure module README',
 'The test: somebody else uses it without reading the code.',
$md$
# {module name}

## What it creates

(The list of resources, in prose. Not the plan output.)

## Minimal usage

```hcl
module "example" {
  source = "..."
  # the required variables, and nothing else
}
```

## Variables

| Name | Type | Default | What it changes |
|---|---|---|---|
| | | | |

Defaults are safe: nothing open to the world, nothing permissive "for now".

## Outputs

## What to know before applying

- what gets created that costs money:
- what gets destroyed on destroy, and what survives:
- pinned versions, and why:

## Applying twice

The second run shows no difference. Trace below — it is the first thing a
reviewer checks.

## Known limitations
$md$,
 230),

('ops-template-slo', 'writeup_template', 'ops', 'reliability', 'fr',
 'Document d''objectif de service',
 'Une cible, une fenêtre, une source. Et ce qui se passe quand le budget est épuisé.',
$md$
# Objectif de service — {service}

## Ce que le service fait, en une phrase que son utilisateur reconnaîtrait

## L'indicateur

- **ce qu'on mesure** : (une requête réussie, c'est quoi exactement)
- **ce qui compte comme échec** : (les codes, les délais dépassés)
- **où c'est mesuré** : (côté client ou côté serveur — la différence est
  énorme et se dit)

## La cible

- **{n} %** sur **{n} jours**
- **source** : (lien, accessible à qui relit)

Pourquoi cette valeur et pas une autre : (une cible sans justification est
une cible que personne ne défendra en réunion.)

## Le budget d'erreur

{n} minutes d'indisponibilité par fenêtre.

**Quand il est épuisé** :
- ce qu'on arrête :
- ce qui passe en priorité :
- qui décide de reprendre :

## Ce qui n'est pas couvert

(Les dépendances hors de notre contrôle, et ce qu'on fait quand elles
tombent.)
$md$,
 240),

('ops-template-slo', 'writeup_template', 'ops', 'reliability', 'en',
 'Service objective document',
 'A target, a window, a source. And what happens when the budget runs out.',
$md$
# Service objective — {service}

## What the service does, in one sentence its user would recognise

## The indicator

- **what is measured**: (what exactly counts as a successful request)
- **what counts as failure**: (codes, exceeded latencies)
- **where it is measured**: (client side or server side — the difference is
  large and gets stated)

## The target

- **{n}%** over **{n} days**
- **source**: (link, openable by whoever reviews this)

Why this value and not another: (a target with no justification is a target
nobody will defend in a meeting.)

## The error budget

{n} minutes of unavailability per window.

**When it is spent**:
- what stops:
- what takes priority:
- who decides to resume:

## What is not covered

(Dependencies outside our control, and what we do when they fall over.)
$md$,
 240),

('ops-template-chaos', 'writeup_template', 'ops', 'reliability', 'fr',
 'Rapport d''expérience de chaos',
 'Une hypothèse, une panne provoquée, et l''écart entre les deux.',
$md$
# Expérience — {ce qu'on casse}

## Hypothèse

« Si {panne}, alors {comportement attendu}, et l'utilisateur {voit / ne voit
rien}. »

Une expérience sans hypothèse écrite avant est une panne.

## Périmètre et garde-fous

- **environnement** :
- **fenêtre** :
- **qui est prévenu** :
- **critère d'arrêt** : (à quel moment on annule, décidé avant de commencer)

## Ce qui a été fait

## Ce qui s'est réellement passé

| Mesure | Attendu | Obtenu |
|---|---|---|
| Détection | | |
| Reprise | | |
| Impact utilisateur | | |

## L'écart

(La partie utile. Si tout s'est passé comme prévu, l'expérience était trop
facile.)

## Actions

| Action | Porteur | Échéance |
|---|---|---|
| | | |
$md$,
 250),

('ops-template-chaos', 'writeup_template', 'ops', 'reliability', 'en',
 'Chaos experiment report',
 'A hypothesis, a deliberate failure, and the gap between them.',
$md$
# Experiment — {what we break}

## Hypothesis

"If {failure}, then {expected behaviour}, and the user {sees / sees nothing}."

An experiment with no hypothesis written beforehand is an outage.

## Scope and guardrails

- **environment**:
- **window**:
- **who is told**:
- **abort criterion**: (when we call it off, decided before starting)

## What was done

## What actually happened

| Measure | Expected | Observed |
|---|---|---|
| Detection | | |
| Recovery | | |
| User impact | | |

## The gap

(The useful part. If everything went as predicted, the experiment was too
easy.)

## Actions

| Action | Owner | Due |
|---|---|---|
| | | |
$md$,
 250),

('ops-template-cost', 'writeup_template', 'ops', 'cloud', 'fr',
 'Rapport de réduction de coûts',
 'Les deux montants, ce qui a changé, et la preuve que le service tient toujours.',
$md$
# Réduction de coûts — {périmètre}

## Avant

- **facture mensuelle** : {montant} {devise}
- **les trois postes les plus chers** :
- **période de référence** : (un mois représentatif, pas le plus cher)

## Ce qui a été changé

(Poste par poste. « Optimisation générale » n'est pas une méthode.)

## Après

- **facture mensuelle** : {montant} {devise}
- **économie annuelle** : {montant}
- **réduction** : {n} %

## Le service tient toujours

C'est la moitié qui manque à la plupart de ces rapports. Une réduction qui a
cassé le service est une panne avec un tableur.

- **objectif de service avant / après** :
- **latence avant / après** :
- **qui l'a vérifié, et quand** :

## Preuve

(Facture anonymisée ou capture du tableau de bord de coûts, avant et après.)

## Ce qu'on n'a pas touché, et pourquoi
$md$,
 260),

('ops-template-cost', 'writeup_template', 'ops', 'cloud', 'en',
 'Cost reduction report',
 'Both figures, what changed, and the proof the service still stands.',
$md$
# Cost reduction — {scope}

## Before

- **monthly bill**: {amount} {currency}
- **the three most expensive line items**:
- **reference period**: (a representative month, not the worst one)

## What was changed

(Line item by line item. "General optimisation" is not a method.)

## After

- **monthly bill**: {amount} {currency}
- **annual saving**: {amount}
- **reduction**: {n}%

## The service still stands

This is the half most of these reports are missing. A reduction that broke
the service is an outage with a spreadsheet.

- **service objective before / after**:
- **latency before / after**:
- **who verified it, and when**:

## Evidence

(Anonymised invoice or cost dashboard screenshot, before and after.)

## What was left alone, and why
$md$,
 260),

('ops-template-migration', 'writeup_template', 'ops', 'data', 'fr',
 'Plan de migration',
 'Le volume, la durée du verrou, et le chemin de retour — avant de commencer.',
$md$
# Plan de migration — {ce qui bouge}

## D'où, vers où

- **source** : (version, volume, charge)
- **cible** :
- **ce qui ne bouge pas** :

## Le volume

| Table / jeu de données | Lignes | Taille | Écritures par seconde |
|---|---|---|---|
| | | | |

Sans ces chiffres, personne ne peut relire ce plan.

## La procédure

1.
2.
3.

**Durée du verrou attendue** : {n} — mesurée sur une copie, pas estimée.
**Fenêtre d'intervention** :

## Retour en arrière

- **possible** : oui / non
- **si oui** : la procédure, et sa durée
- **si non** : la sauvegarde vérifiée avant, par qui, et restaurée pour de
  vrai à quelle date

Une migration irréversible sans sauvegarde restaurée est refusée en
relecture.

## Réconciliation

Comment on saura que rien n'a été perdu ni dupliqué : (le compte, la somme de
contrôle, la comparaison ligne à ligne.)

## Qui est prévenu, et quand
$md$,
 270),

('ops-template-migration', 'writeup_template', 'ops', 'data', 'en',
 'Migration plan',
 'The volume, the lock duration, and the way back — before starting.',
$md$
# Migration plan — {what moves}

## From where, to where

- **source**: (version, volume, load)
- **target**:
- **what does not move**:

## The volume

| Table / dataset | Rows | Size | Writes per second |
|---|---|---|---|
| | | | |

Without these figures nobody can review this plan.

## The procedure

1.
2.
3.

**Expected lock duration**: {n} — measured on a copy, not estimated.
**Intervention window**:

## Rolling back

- **possible**: yes / no
- **if yes**: the procedure, and how long it takes
- **if no**: the backup verified beforehand, by whom, and actually restored
  on what date

An irreversible migration with no restored backup is refused at review.

## Reconciliation

How we will know nothing was lost or duplicated: (the count, the checksum,
the row-by-row comparison.)

## Who is told, and when
$md$,
 270),

('ops-template-oncall', 'writeup_template', 'ops', 'reliability', 'fr',
 'Guide de rotation d''astreinte',
 'Être joignable est du travail. Ce document dit lequel, et ce qu''il paye.',
$md$
# Astreinte — {équipe ou service}

## La rotation

- **qui** :
- **rythme** : (une semaine sur {n})
- **plage horaire** :
- **remplaçant** : (il y en a un, sinon ce n'est pas une rotation)

## Ce qui est attendu

- **délai de réponse** : {n} minutes
- **ce qu'on doit pouvoir faire** : (répondre, diagnostiquer, escalader —
  pas nécessairement réparer)
- **ce qu'on ne doit pas faire seul** :

## Ce qui est payé

Le montant, et pour quoi : la disponibilité elle-même, pas seulement les
nuits où quelque chose tombe.

- **forfait de disponibilité** :
- **intervention de nuit** :
- **récupération** : (le lendemain d'une nuit d'intervention)

## Ce qu'il faut avant de prendre son premier tour

- accès obtenus et testés (pas seulement demandés) :
- runbooks lus :
- un exercice joué avec quelqu'un d'expérimenté :

## Passation

Ce qui se transmet à la fin d'un tour : les alertes en cours, ce qui a été
contourné plutôt que réparé, ce qui va probablement se réveiller.
$md$,
 280),

('ops-template-oncall', 'writeup_template', 'ops', 'reliability', 'en',
 'On-call rotation guide',
 'Being reachable is work. This document says which work, and what it pays.',
$md$
# On-call — {team or service}

## The rotation

- **who**:
- **cadence**: (one week in {n})
- **hours**:
- **backup**: (there is one, or it is not a rotation)

## What is expected

- **response time**: {n} minutes
- **what you must be able to do**: (answer, diagnose, escalate — not
  necessarily fix)
- **what you must not do alone**:

## What is paid

The amount, and for what: availability itself, not only the nights something
falls over.

- **availability retainer**:
- **night intervention**:
- **time back**: (the day after a night call)

## What you need before your first shift

- access obtained and tested (not merely requested):
- runbooks read:
- one drill run with somebody experienced:

## Handover

What passes at the end of a shift: open alerts, what was worked around rather
than fixed, what is likely to wake up next.
$md$,
 280),

('ops-template-capacity', 'writeup_template', 'ops', 'cloud', 'fr',
 'Rapport de planification de capacité',
 'Ce qui casse en premier, à quel niveau de charge, et ce qu''on fait avant.',
$md$
# Capacité — {système}

## La charge actuelle

- **au repos** :
- **au pic** : (quand, et pourquoi)
- **croissance observée sur douze mois** :

## Ce qui casse en premier

(La ressource qui sature avant les autres. Si la réponse est « je ne sais
pas », le reste du document est de la spéculation.)

| Ressource | Utilisation actuelle | Seuil | Marge |
|---|---|---|---|
| | | | |

## La projection

À {n} % de croissance, le premier seuil est atteint le {date}.

**Méthode** : (comment la projection a été faite — linéaire, saisonnière,
sur quelle mesure.)

## Ce qu'on fait avant

- **à court terme** :
- **le vrai changement** : (celui qui coûte du temps, à démarrer maintenant)
- **le coût** :

## Ce que ce rapport suppose

(Les hypothèses qui, si elles sont fausses, invalident tout ce qui précède.)
$md$,
 290),

('ops-template-capacity', 'writeup_template', 'ops', 'cloud', 'en',
 'Capacity planning report',
 'What breaks first, at what load, and what we do before then.',
$md$
# Capacity — {system}

## Current load

- **at rest**:
- **at peak**: (when, and why)
- **observed growth over twelve months**:

## What breaks first

(The resource that saturates before the others. If the answer is "I do not
know", the rest of this document is speculation.)

| Resource | Current use | Threshold | Headroom |
|---|---|---|---|
| | | | |

## The projection

At {n}% growth, the first threshold is reached on {date}.

**Method**: (how the projection was made — linear, seasonal, on which
measurement.)

## What we do before then

- **short term**:
- **the real change**: (the one that costs time, to start now)
- **the cost**:

## What this report assumes

(The assumptions which, if wrong, invalidate everything above.)
$md$,
 290),

('ops-template-observability', 'writeup_template', 'ops', 'observability', 'fr',
 'Conception d''une pile d''observabilité',
 'Les questions d''abord. Les outils sont ce qui reste une fois qu''elles sont posées.',
$md$
# Observabilité — {périmètre}

## Les questions auxquelles il faut répondre

1.
2.
3.

Si cette liste est vide, la conception n'est pas prête. Les outils viennent
après, et jamais l'inverse.

## Ce qui est instrumenté

| Signal | Source | Ce que ça permet de répondre |
|---|---|---|
| Métriques | | |
| Journaux | | |
| Traces | | |

## Les alertes

| Alerte | Symptôme utilisateur | Action | Runbook |
|---|---|---|---|
| | | | |

Chaque alerte réveille quelqu'un. Une alerte sans action possible détruit
l'astreinte plus sûrement qu'un incident.

## La rétention

- **métriques** : {n} jours, parce que
- **journaux** : {n} jours, parce que
- **traces** : {n} jours (ou échantillonnage à {n} %), parce que

## Le coût

- **ingestion mensuelle estimée** :
- **l'étiquette la plus chère, et sa justification** :

## Ce qu'on ne verra toujours pas
$md$,
 300),

('ops-template-observability', 'writeup_template', 'ops', 'observability', 'en',
 'Observability stack design',
 'The questions first. The tools are what is left once they are written down.',
$md$
# Observability — {scope}

## The questions that must be answerable

1.
2.
3.

If this list is empty, the design is not ready. Tools come after, never the
other way round.

## What is instrumented

| Signal | Source | What it answers |
|---|---|---|
| Metrics | | |
| Logs | | |
| Traces | | |

## Alerts

| Alert | User symptom | Action | Runbook |
|---|---|---|---|
| | | | |

Every alert wakes somebody. An alert with no possible action destroys on-call
more reliably than an incident does.

## Retention

- **metrics**: {n} days, because
- **logs**: {n} days, because
- **traces**: {n} days (or {n}% sampling), because

## Cost

- **estimated monthly ingestion**:
- **the most expensive label, and its justification**:

## What we still will not see
$md$,
 300)

ON CONFLICT (slug, locale) DO NOTHING;
