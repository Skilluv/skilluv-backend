-- The toolkit, and the twelve documents a contributor writes.
--
-- ## The toolkit
--
-- One page rather than one per language: somebody arriving needs to see the
-- whole landscape before choosing a corner of it, and eleven pages nobody
-- reads is worse than one page somebody skims.
--
-- Opinionated on purpose. A neutral list of forty editors helps nobody; the
-- useful thing is to say which two to try and why.
--
-- ## The templates
--
-- Twelve documents, each with the same argument behind it: the writing is not
-- paperwork around the work, it is part of the work. A pull request nobody
-- can review is not finished. An outage with no post-mortem happens twice.
--
-- Each template is short on purpose. A template long enough to be intimidating
-- is one people skip.

INSERT INTO content_guides
    (slug, kind, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

-- ═══════════════════════════════════════════════════════════════════
-- Toolkit
-- ═══════════════════════════════════════════════════════════════════

('toolkit-code', 'toolkit', NULL, 'fr',
 'Outillage code',
 'Ce qu''il faut installer, et pourquoi. Volontairement partial : une liste neutre de quarante éditeurs n''aide personne.',
$md$
# Outillage code

Volontairement partial. Une liste neutre de quarante éditeurs n'aide personne ;
ce qui sert, c'est de dire lesquels essayer et pourquoi.

## Langages et leurs gestionnaires de version

Installe toujours par un gestionnaire de version, jamais par le paquet système :
le jour où un projet exige une autre version, tu ne veux pas avoir à choisir.

| Langage | Installer par | Note |
|---|---|---|
| Rust | `rustup` | Le seul gestionnaire, et il fait tout. |
| Go | `goenv` ou l'archive officielle | Peu de versions, peu de douleur. |
| Python | `uv` | Remplace pyenv, pip et virtualenv à lui seul, et il est rapide. |
| TypeScript | `bun`, `node` via `fnm`, ou `deno` | Bun pour démarrer vite, Node pour la compatibilité. |
| Zig | `zigup` | Le langage bouge encore ; épingle une version. |
| Elixir | `asdf` | Gère aussi Erlang, qu'il te faut de toute façon. |
| Java / Kotlin | `sdkman` | Bien plus simple que d'installer un JDK à la main. |
| Swift | Xcode | Pas le choix sur macOS ; `swiftly` sous Linux. |
| C / C++ | clang ou gcc du système | Ici le paquet système est la bonne réponse. |
| Julia | `juliaup` | Même logique que rustup. |

## Éditeurs

Deux à essayer, pas dix.

- **VSCode ou VSCodium** — la version sans télémétrie de Microsoft. Le plus
  grand écosystème d'extensions, et le serveur de langage marche partout.
- **Neovim** avec une configuration prête (LazyVim, kickstart) — plus rapide à
  l'usage, plus lent à apprendre. Ne commence pas par une configuration
  écrite depuis zéro.
- **Zed** si tu veux quelque chose de moderne et rapide sans configurer.
- **JetBrains** reste supérieur sur les gros projets Java et Kotlin, et le
  restera.

## Contrôle de version

Git, évidemment. Et **Jujutsu (`jj`)** vaut une semaine d'essai : il s'installe
par-dessus un dépôt Git existant, ce qui rend l'essai réversible.

## Gestionnaires de paquets

`cargo`, `pnpm` (plus économe que npm), `uv`, `mix`, `maven` ou `gradle`,
`nuget`, `composer`. Choisis celui de ta communauté et arrête d'y penser.

## Systèmes de build

Cargo et `go build` ne demandent rien. Ailleurs : Vite pour le web, `esbuild`
ou Rollup en dessous, Bazel seulement si ton dépôt est vraiment grand — c'est
un outil qui coûte cher avant de rapporter. Nix quand la reproductibilité
compte plus que la vitesse d'apprentissage.

## Conteneurs

Docker ou Podman. Podman n'a pas besoin d'un démon privilégié, ce qui est un
avantage réel. Les DevContainers valent la peine dès qu'on travaille à
plusieurs sur une même machine de développement.

## Assistants IA

Autorisés sur Skilluv, à une condition : le déclarer. Copilot, Claude Code,
Continue, Codeium. Ce qui n'est pas acceptable, ce n'est pas de s'en servir,
c'est de le cacher — une contribution dont on ne sait pas d'où elle vient est
une contribution qu'un mainteneur ne peut pas relire correctement.

## Ce qui manque à cette liste

Ton profileur. Presque personne ne l'installe avant d'en avoir besoin, et
c'est l'outil qui change le plus la façon de coder. `perf`, `samply`,
`py-spy`, `pprof` : un par langage, une heure à apprendre.
$md$,
 200),

('toolkit-code', 'toolkit', NULL, 'en',
 'Code toolkit',
 'What to install, and why. Deliberately opinionated: a neutral list of forty editors helps nobody.',
$md$
# Code toolkit

Deliberately opinionated. A neutral list of forty editors helps nobody; what
helps is saying which ones to try and why.

## Languages and their version managers

Always install through a version manager, never through the system package: the
day a project needs a different version, you do not want to have to choose.

| Language | Install with | Note |
|---|---|---|
| Rust | `rustup` | The only manager, and it does everything. |
| Go | `goenv` or the official archive | Few versions, little pain. |
| Python | `uv` | Replaces pyenv, pip and virtualenv on its own, and it is fast. |
| TypeScript | `bun`, `node` via `fnm`, or `deno` | Bun to start fast, Node for compatibility. |
| Zig | `zigup` | The language still moves; pin a version. |
| Elixir | `asdf` | Also handles Erlang, which you need anyway. |
| Java / Kotlin | `sdkman` | Far simpler than installing a JDK by hand. |
| Swift | Xcode | No choice on macOS; `swiftly` on Linux. |
| C / C++ | system clang or gcc | Here the system package is the right answer. |
| Julia | `juliaup` | Same idea as rustup. |

## Editors

Two to try, not ten.

- **VSCode or VSCodium** — the build without Microsoft's telemetry. The largest
  extension ecosystem, and the language server works everywhere.
- **Neovim** with a ready configuration (LazyVim, kickstart) — faster to use,
  slower to learn. Do not start from a config written from scratch.
- **Zed** if you want something modern and fast with no configuring.
- **JetBrains** is still better on large Java and Kotlin projects, and will
  stay that way.

## Version control

Git, obviously. And **Jujutsu (`jj`)** is worth a week: it installs on top of
an existing Git repository, which makes trying it reversible.

## Package managers

`cargo`, `pnpm` (leaner than npm), `uv`, `mix`, `maven` or `gradle`, `nuget`,
`composer`. Pick your community's and stop thinking about it.

## Build systems

Cargo and `go build` ask nothing of you. Elsewhere: Vite for web, `esbuild` or
Rollup underneath, Bazel only if your repository is genuinely large — it costs
a lot before it pays. Nix when reproducibility matters more than learning
speed.

## Containers

Docker or Podman. Podman needs no privileged daemon, which is a real
advantage. DevContainers earn their keep as soon as several people share a
development machine.

## AI assistants

Allowed on Skilluv, on one condition: say so. Copilot, Claude Code, Continue,
Codeium. What is unacceptable is not using them, it is hiding it — a
contribution whose origin is unknown is one a maintainer cannot review
properly.

## What this list is missing

Your profiler. Almost nobody installs one before needing it, and it is the
tool that changes how you write code the most. `perf`, `samply`, `py-spy`,
`pprof`: one per language, an hour to learn.
$md$,
 200),

-- ═══════════════════════════════════════════════════════════════════
-- The twelve writeup templates
-- ═══════════════════════════════════════════════════════════════════

('template-pr-description', 'writeup_template', NULL, 'fr',
 'Description de pull request',
 'Ce qu''un mainteneur a besoin de lire avant de regarder une seule ligne.',
$md$
## Ce que fait ce changement

Une phrase. Si tu ne peux pas la faire tenir en une phrase, la PR fait
probablement deux choses et devrait en être deux.

## Pourquoi

Le problème, pas la solution. Lie l'issue s'il y en a une.

## Comment vérifier

Les commandes exactes. Un relecteur qui doit deviner comment tester ne teste
pas.

```
# à lancer
```

## Ce que ce changement ne fait pas

Les limites connues, ce qui reste ouvert. Le dire soi-même vaut mieux que de
se le faire dire.

## Assistance IA

Déclarée si elle a servi, et à quel point : autocomplétion, écrit puis
retravaillé, généré tel quel.
$md$,
 300),

('template-pr-description', 'writeup_template', NULL, 'en',
 'Pull request description',
 'What a maintainer needs to read before looking at a single line.',
$md$
## What this changes

One sentence. If it will not fit in one, the PR is probably doing two things
and should be two.

## Why

The problem, not the solution. Link the issue if there is one.

## How to check it

The exact commands. A reviewer who has to guess how to test it does not test
it.

```
# to run
```

## What this does not do

Known limits, what is left open. Saying it yourself beats being told.

## AI assistance

Declared if used, and how much: autocomplete, written then reworked,
generated as is.
$md$,
 300),

('template-rfc', 'writeup_template', NULL, 'fr',
 'RFC / proposition de conception',
 'Une proposition se juge sur les alternatives qu''elle a écartées.',
$md$
# RFC : titre

## Résumé

Trois phrases maximum.

## Motivation

Quel problème, pour qui, à quelle fréquence. Un problème que personne n'a
rencontré ne mérite pas de RFC.

## Conception détaillée

Le cœur. Assez précis pour que quelqu'un d'autre puisse l'implémenter sans te
demander.

## Alternatives envisagées

La section qui décide de l'accueil d'une RFC. Chaque alternative, et la raison
précise de son rejet. Une RFC sans alternatives se lit comme une décision déjà
prise.

## Inconvénients

Ce que cette proposition coûte. Il y a toujours quelque chose.

## Questions ouvertes

Ce que tu ne sais pas encore. L'écrire attire l'aide ; le cacher attire le
rejet.
$md$,
 310),

('template-rfc', 'writeup_template', NULL, 'en',
 'RFC / design proposal',
 'A proposal is judged on the alternatives it ruled out.',
$md$
# RFC: title

## Summary

Three sentences at most.

## Motivation

What problem, for whom, how often. A problem nobody has hit does not deserve
an RFC.

## Detailed design

The core. Precise enough that somebody else could implement it without asking
you.

## Alternatives considered

The section that decides how an RFC is received. Each alternative, and the
precise reason it was rejected. An RFC with no alternatives reads like a
decision already taken.

## Drawbacks

What this costs. There is always something.

## Open questions

What you do not know yet. Writing it attracts help; hiding it attracts
rejection.
$md$,
 310),

('template-readme-oss', 'writeup_template', NULL, 'fr',
 'README de projet open source',
 'Trente secondes pour dire ce que c''est, et une commande pour l''essayer.',
$md$
# Nom du projet

Une phrase qui dit ce que c'est. Pas ce que ça pourrait devenir.

## Installer

```
une commande
```

## Utiliser

L'exemple le plus court qui fonctionne. Copiable tel quel.

## Pourquoi celui-ci plutôt qu'un autre

Nomme les alternatives et dis en quoi tu diffères. Ne pas le faire oblige le
lecteur à le chercher lui-même, et il ne le fera pas.

## État

Expérimental, utilisable, maintenu, en recherche de mainteneur. Le dire
franchement fait gagner du temps à tout le monde.

## Contribuer

Ce que tu acceptes, ce que tu n'acceptes pas, et comment lancer les tests.

## Licence

Une ligne, et le fichier qui va avec.
$md$,
 320),

('template-readme-oss', 'writeup_template', NULL, 'en',
 'Open source project README',
 'Thirty seconds to say what it is, and one command to try it.',
$md$
# Project name

One sentence saying what it is. Not what it could become.

## Install

```
one command
```

## Use

The shortest example that works. Copyable as is.

## Why this rather than something else

Name the alternatives and say how you differ. Not doing so makes the reader
find out for themselves, and they will not.

## Status

Experimental, usable, maintained, looking for a maintainer. Saying it plainly
saves everybody time.

## Contributing

What you accept, what you do not, and how to run the tests.

## Licence

One line, and the file to go with it.
$md$,
 320),

('template-changelog', 'writeup_template', NULL, 'fr',
 'Changelog et versionnage',
 'Écrit pour la personne qui met à jour, pas pour celle qui a codé.',
$md$
# Changelog

Écrit pour la personne qui met à jour. Elle veut savoir ce qui va casser, pas
ce que tu as trouvé élégant.

## [1.2.0] — AAAA-MM-JJ

### Ruptures
Ce qui casse, et comment migrer. En premier, toujours.

### Ajouts
Ce qui est nouveau.

### Corrections
Ce qui était cassé et ne l'est plus.

### Obsolète
Ce qui partira, et à quelle version.

---

Version majeure : une rupture. Mineure : un ajout compatible. Correctif :
seulement des corrections. Un projet qui ne respecte pas cette règle rend son
changelog inutile.
$md$,
 330),

('template-changelog', 'writeup_template', NULL, 'en',
 'Changelog and versioning',
 'Written for the person upgrading, not for the person who wrote the code.',
$md$
# Changelog

Written for the person upgrading. They want to know what will break, not what
you found elegant.

## [1.2.0] — YYYY-MM-DD

### Breaking
What breaks, and how to migrate. First, always.

### Added
What is new.

### Fixed
What was broken and is not any more.

### Deprecated
What is going away, and in which version.

---

Major: a break. Minor: a compatible addition. Patch: fixes only. A project
that does not follow this makes its own changelog useless.
$md$,
 330),

('template-adr', 'writeup_template', NULL, 'fr',
 'Décision d''architecture (ADR)',
 'Une décision non écrite est une décision que quelqu''un défera dans six mois.',
$md$
# ADR-000 : titre de la décision

**Statut** : proposée / acceptée / remplacée par ADR-00X
**Date** : AAAA-MM-JJ

## Contexte

Ce qui est vrai au moment de décider. Contraintes, ce qui existe déjà, ce
qu'on ne peut pas changer.

## Décision

Ce qui a été décidé, à l'indicatif. « Nous utilisons X. » Pas « nous
pourrions ».

## Conséquences

Ce que cela rend facile, ce que cela rend difficile, ce que cela rend
impossible. La troisième colonne est celle qu'on oublie et celle qui coûte.

## Alternatives écartées

Et pourquoi. Sans cela, quelqu'un les reproposera dans six mois et personne ne
saura répondre.
$md$,
 340),

('template-adr', 'writeup_template', NULL, 'en',
 'Architecture decision record (ADR)',
 'An unwritten decision is one somebody will undo in six months.',
$md$
# ADR-000: decision title

**Status**: proposed / accepted / superseded by ADR-00X
**Date**: YYYY-MM-DD

## Context

What is true at the moment of deciding. Constraints, what already exists, what
cannot be changed.

## Decision

What was decided, in the present tense. "We use X." Not "we could".

## Consequences

What this makes easy, what it makes hard, what it makes impossible. The third
is the one people forget and the one that costs.

## Alternatives rejected

And why. Without this, somebody proposes them again in six months and nobody
can answer.
$md$,
 340),

('template-benchmark', 'writeup_template', NULL, 'fr',
 'Rapport de banc d''essai',
 'Un chiffre sans méthode n''est pas un résultat, c''est une affirmation.',
$md$
# Banc d'essai : ce qui est mesuré

## Question

Ce que tu cherches à savoir. Une question, pas un chiffre à atteindre.

## Référence

Ce à quoi tu compares. Sans référence il n'y a pas de mesure, seulement un
nombre.

## Méthode

Machine, système, versions, jeu de données, nombre de répétitions, ce qui
tournait d'autre. Quelqu'un doit pouvoir refaire ça.

## Résultats

| Cas | Référence | Après | Écart |
|---|---|---|---|

Donne la dispersion, pas seulement la moyenne. Une médiane sans écart cache
les régressions les plus gênantes.

## Ce que cela ne dit pas

Les limites. Un banc d'essai mesure toujours moins que ce que le lecteur croit.

## Reproduire

Le dépôt, la commande, le commit exact.
$md$,
 350),

('template-benchmark', 'writeup_template', NULL, 'en',
 'Benchmark report',
 'A number with no method is not a result, it is a claim.',
$md$
# Benchmark: what is measured

## Question

What you are trying to find out. A question, not a number to reach.

## Baseline

What you compare against. Without a baseline there is no measurement, only a
number.

## Method

Machine, operating system, versions, dataset, repetitions, what else was
running. Somebody must be able to redo this.

## Results

| Case | Baseline | After | Delta |
|---|---|---|---|

Give the spread, not only the mean. A median with no spread hides the most
awkward regressions.

## What this does not say

The limits. A benchmark always measures less than the reader assumes.

## Reproducing

The repository, the command, the exact commit.
$md$,
 350),

('template-security-audit', 'writeup_template', NULL, 'fr',
 'Rapport d''audit de sécurité',
 'La sévérité se justifie, elle ne se décrète pas.',
$md$
# Audit : périmètre

## Périmètre et limites

Ce qui a été examiné, et surtout ce qui ne l'a pas été. Un audit dont le
périmètre est flou sert à rassurer, pas à protéger.

## Constats

### [Sévérité] Titre du constat

**Où** : fichier, ligne, commit.
**Impact** : ce qu'un attaquant obtient. Concrètement.
**Reproduire** : les étapes exactes.
**Correction proposée** : ce que tu ferais.

Sévérité justifiée, jamais décrétée : dis quel accès elle suppose et ce
qu'elle donne.

## Ce qui a été vérifié et trouvé correct

Aussi important que les constats. Sinon le lecteur ne sait pas ce que ton
silence signifie.

## Divulgation

Prévenue le … , délai accordé … , publication prévue le … .
$md$,
 360),

('template-security-audit', 'writeup_template', NULL, 'en',
 'Security audit report',
 'Severity is argued, never declared.',
$md$
# Audit: scope

## Scope and limits

What was examined, and above all what was not. An audit with vague scope
reassures rather than protects.

## Findings

### [Severity] Finding title

**Where**: file, line, commit.
**Impact**: what an attacker gets. Concretely.
**Reproducing**: the exact steps.
**Proposed fix**: what you would do.

Severity argued, never declared: say what access it assumes and what it yields.

## What was checked and found sound

As important as the findings. Otherwise the reader does not know what your
silence means.

## Disclosure

Notified on …, embargo …, publication planned for … .
$md$,
 360),

('template-release-notes', 'writeup_template', NULL, 'fr',
 'Notes de version de bibliothèque',
 'Le changelog dit ce qui a changé ; les notes disent ce qu''il faut faire.',
$md$
# Version X.Y.Z

## Ce qu'il faut savoir

Deux phrases pour quelqu'un qui décide s'il met à jour aujourd'hui ou dans un
mois.

## Migration

Uniquement s'il y a une rupture. Avant / après, en code.

```diff
- ancien
+ nouveau
```

## Nouveautés

Avec un exemple pour chacune. Une fonctionnalité sans exemple n'est pas
découverte.

## Corrections

Lien vers les issues.

## Remerciements

Les personnes qui ont contribué, nommées. C'est gratuit et c'est ce qui fait
revenir les gens.
$md$,
 370),

('template-release-notes', 'writeup_template', NULL, 'en',
 'Library release notes',
 'The changelog says what changed; the notes say what to do about it.',
$md$
# Version X.Y.Z

## What you need to know

Two sentences for somebody deciding whether to upgrade today or in a month.

## Migration

Only if something breaks. Before and after, in code.

```diff
- old
+ new
```

## New

With an example for each. A feature with no example is not discovered.

## Fixes

Link the issues.

## Thanks

The people who contributed, by name. It costs nothing and it is what makes
people come back.
$md$,
 370),

('template-blog-post', 'writeup_template', NULL, 'fr',
 'Article technique',
 'Commence par le problème. Personne ne lit une solution avant d''avoir le problème.',
$md$
# Titre : le problème, pas la technologie

## Le problème

Concret, vécu, avec les chiffres si tu en as. Commence ici. Un article qui
commence par « nous avons choisi X » perd le lecteur au deuxième paragraphe.

## Ce qu'on a essayé d'abord

Y compris ce qui n'a pas marché. C'est la partie qui rend un article crédible
et c'est celle que la plupart des gens coupent.

## Ce qui a marché

Avec assez de code pour être reproduit.

## Les chiffres

Avant, après, méthode.

## Ce qu'on ferait autrement

Écrit six mois plus tard, cette section vaut le reste de l'article.
$md$,
 380),

('template-blog-post', 'writeup_template', NULL, 'en',
 'Technical blog post',
 'Start with the problem. Nobody reads a solution before they have the problem.',
$md$
# Title: the problem, not the technology

## The problem

Concrete, lived, with numbers if you have them. Start here. A post that opens
with "we chose X" loses the reader by the second paragraph.

## What we tried first

Including what did not work. It is the part that makes a post credible and the
part most people cut.

## What worked

With enough code to be reproduced.

## The numbers

Before, after, method.

## What we would do differently

Written six months later, this section is worth the rest of the post.
$md$,
 380),

('template-contributing', 'writeup_template', NULL, 'fr',
 'Guide de contribution',
 'Dire non clairement fait gagner plus de temps que dire oui vaguement.',
$md$
# Contribuer

## Avant de commencer

Ouvre une issue pour tout ce qui dépasse une correction évidente. Une grosse
PR non annoncée finit souvent refusée, et c'est du temps perdu pour tout le
monde.

## Ce que nous acceptons

Sois précis. Corrections, documentation, tests, portages.

## Ce que nous n'acceptons pas

Encore plus important. Reformatages massifs, changements de dépendances,
réécritures. Dire non clairement ici évite de dire non durement plus tard.

## Lancer les tests

```
une commande
```

## Style

Un formateur automatique, et rien d'autre. Les débats de style se règlent par
un outil, pas en revue.

## Assistance IA

Notre position : acceptée, déclarée. Dis dans la PR ce qui a été assisté.

## Licence des contributions

Sous quelle licence tu places ce que tu envoies.
$md$,
 390),

('template-contributing', 'writeup_template', NULL, 'en',
 'Contribution guide',
 'Saying no clearly saves more time than saying yes vaguely.',
$md$
# Contributing

## Before you start

Open an issue for anything beyond an obvious fix. A large unannounced PR often
ends up refused, and that is everybody's time wasted.

## What we accept

Be specific. Fixes, documentation, tests, ports.

## What we do not accept

More important still. Mass reformatting, dependency changes, rewrites. Saying
no clearly here avoids saying no harshly later.

## Running the tests

```
one command
```

## Style

An automatic formatter, and nothing else. Style arguments are settled by a
tool, not in review.

## AI assistance

Our position: accepted, declared. Say in the PR what was assisted.

## Licence of contributions

Under which licence you place what you send.
$md$,
 390),

('template-code-review', 'writeup_template', NULL, 'fr',
 'Grille de relecture',
 'Une revue qui ne dit que « LGTM » n''est pas une revue.',
$md$
# Relire une contribution

## Avant de commenter

Fais tourner le code. Une revue faite sans exécuter est une lecture.

## Ce qu'on regarde, dans cet ordre

1. **Est-ce que cela résout le problème annoncé ?** Sinon, rien d'autre ne
   compte.
2. **Les bords.** Entrée vide, valeur limite, erreur réseau, concurrence.
3. **Les tests.** Décrivent-ils le comportement ou l'implémentation ? Un test
   qui casse à chaque refactorisation est un mauvais test.
4. **La lisibilité.** Les noms disent-ils l'intention ?
5. **La documentation.** Un lecteur qui arrive comprend-il quoi lancer ?

## Comment écrire un commentaire

Sépare ce qui bloque de ce qui est une préférence. Dis lequel c'est. Un
mainteneur qui mélange les deux est un mainteneur qu'on n'ose plus solliciter.

**Bloquant** : … parce que … .
**Suggestion** : … , à toi de voir.

## Ce qu'on ne fait pas

Reformater le code de quelqu'un d'autre en revue. Demander une réécriture
complète sans l'avoir dit avant la PR. Laisser une PR sans réponse plus d'une
semaine sans dire pourquoi.
$md$,
 400),

('template-code-review', 'writeup_template', NULL, 'en',
 'Code review checklist',
 'A review that only says "LGTM" is not a review.',
$md$
# Reviewing a contribution

## Before commenting

Run the code. A review done without executing it is a reading.

## What to look at, in this order

1. **Does it solve the stated problem?** If not, nothing else matters.
2. **The edges.** Empty input, boundary value, network error, concurrency.
3. **The tests.** Do they describe behaviour or implementation? A test that
   breaks on every refactor is a bad test.
4. **Readability.** Do the names say the intent?
5. **Documentation.** Can somebody arriving tell what to run?

## How to write a comment

Separate what blocks from what is a preference. Say which. A maintainer who
mixes the two is one people stop approaching.

**Blocking**: … because … .
**Suggestion**: … , your call.

## What we do not do

Reformat somebody else's code in review. Ask for a full rewrite without having
said so before the PR. Leave a PR unanswered for more than a week without
saying why.
$md$,
 400),

('template-post-mortem', 'writeup_template', NULL, 'fr',
 'Post-mortem technique',
 'Sans cause humaine nommée. Une panne qu''on impute à quelqu''un se reproduit en silence.',
$md$
# Post-mortem : ce qui s'est passé

**Durée** : de … à … . **Impact** : qui, combien, quoi.

## Chronologie

Heures exactes. Ce qui a été observé, ce qui a été fait, ce que cela a donné.
Y compris les fausses pistes : elles expliquent la durée mieux que tout le
reste.

## Cause immédiate

Le changement, la charge, la panne matérielle.

## Causes profondes

Pourquoi c'était possible. Pourquoi cela n'a pas été détecté plus tôt.
Pourquoi la remise en route a pris ce temps-là. Trois questions distinctes.

## Ce qui a bien fonctionné

L'alerte a-t-elle sonné ? La documentation était-elle à jour ? Le dire évite de
casser ce qui marchait.

## Actions

| Action | Responsable | Échéance |
|---|---|---|

Concrètes et datées. « Être plus vigilant » n'est pas une action.

## Sans nommer de coupable

Personne n'est nommé comme cause. Une organisation où une panne se solde par un
nom est une organisation où la panne suivante ne sera pas signalée.
$md$,
 410),

('template-post-mortem', 'writeup_template', NULL, 'en',
 'Technical post-mortem',
 'No human named as a cause. An outage blamed on somebody happens again in silence.',
$md$
# Post-mortem: what happened

**Duration**: from … to … . **Impact**: who, how many, what.

## Timeline

Exact times. What was observed, what was done, what it gave. Including the
false trails: they explain the duration better than anything else.

## Immediate cause

The change, the load, the hardware failure.

## Root causes

Why it was possible. Why it was not caught earlier. Why recovery took as long
as it did. Three separate questions.

## What went well

Did the alert fire? Was the documentation current? Saying so avoids breaking
what worked.

## Actions

| Action | Owner | Due |
|---|---|---|

Concrete and dated. "Be more careful" is not an action.

## No culprit named

Nobody is named as a cause. An organisation where an outage ends in a name is
one where the next outage is not reported.
$md$,
 410);
