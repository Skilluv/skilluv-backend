-- Guides, toolkits and templates, as rows.
--
-- ## Why not files in the repository
--
-- Because they have to be served, translated, and edited by somebody who is
-- not deploying. Files would mean a Dockerfile change to ship them, a second
-- mechanism for translations next to `orientation_translations`, and a pull
-- request every time a link rots.
--
-- Rows give the same content one storage, one translation mechanism, and one
-- admin surface — the same choice already made for orientations, review
-- grids, badge rules and craft-score weights.
--
-- ## Three kinds, one table
--
-- An onboarding guide, a toolkit page and a writeup template are the same
-- shape: a slug, a locale, a title and a body in Markdown. Three tables would
-- differ only in the word above them.

CREATE TABLE content_guides (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(80) NOT NULL,
    kind VARCHAR(30) NOT NULL CHECK (kind IN (
        -- One per reviewer group: the eight families of code trades.
        'onboarding',
        -- Languages, editors, build systems. What to install.
        'toolkit',
        -- The documents a contributor writes: PR descriptions, ADRs,
        -- post-mortems.
        'writeup_template'
    )),
    skill_domain VARCHAR(30) NOT NULL DEFAULT 'code',
    -- Which family this belongs to, for the onboarding guides. NULL for the
    -- ones that apply to everybody.
    reviewer_group VARCHAR(30),
    locale VARCHAR(10) NOT NULL DEFAULT 'fr',
    title VARCHAR(200) NOT NULL,
    -- One line, for a listing. Full body only when somebody opens it.
    summary TEXT NOT NULL,
    body_md TEXT NOT NULL CHECK (btrim(body_md) <> ''),
    sort_order SMALLINT NOT NULL DEFAULT 100,
    is_published BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (slug, locale)
);

COMMENT ON TABLE content_guides IS
    'Onboarding guides, toolkits and writeup templates. Rows rather than '
    'files: they have to be translated and edited by somebody who is not '
    'deploying.';

CREATE INDEX idx_content_guides_listing
    ON content_guides (kind, locale, sort_order)
    WHERE is_published = TRUE;

CREATE INDEX idx_content_guides_group
    ON content_guides (reviewer_group, locale)
    WHERE reviewer_group IS NOT NULL AND is_published = TRUE;

CREATE TRIGGER trg_content_guides_updated_at
    BEFORE UPDATE ON content_guides
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- The eight onboarding guides
-- ═══════════════════════════════════════════════════════════════════
--
-- One per reviewer group, because that is how the trades are already grouped
-- everywhere else — by who is competent to review them. The backlog proposed
-- a slightly different grouping; following the one in the database keeps a
-- guide, a reviewer group and a set of orientations pointing at each other.
--
-- Each guide says the same five things, because those are the five a person
-- arriving actually asks: what this family is, what to do in thirty days,
-- what to install, what to attempt first, and where the people are.

INSERT INTO content_guides
    (slug, kind, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('onboarding-web', 'onboarding', 'web', 'fr',
 'Débuter dans le web',
 'Cinq métiers, le plus grand nombre de projets ouverts, et le chemin le plus court vers une première contribution fusionnée.',
$md$
# Débuter dans le web

Cinq métiers : frontend, backend, fullstack, performance, et frontend web3.
C'est la famille où il y a le plus de projets ouverts aux contributions, et
donc celle où une première pull request fusionnée arrive le plus vite. C'est
aussi celle où il est le plus facile de rester en surface — beaucoup de code
écrit, peu de choses comprises.

## Trente jours

**Semaine 1 — lire.** Choisis un projet du fil des premières issues et lis-le
sans rien écrire. Fais tourner ses tests en local. Si tu n'y arrives pas, ta
première contribution est déjà trouvée : la documentation d'installation.

**Semaine 2 — une issue étiquetée.** Prends une issue `good first issue`.
Ouvre la pull request même si tu doutes. Une revue est un cours particulier
gratuit ; l'attente de la perfection est ce qui empêche la plupart des gens
de contribuer.

**Semaine 3 — répondre à la revue.** C'est la semaine qui compte. Un
mainteneur qui demande des changements te dit exactement ce qu'il attend.
Applique, discute quand tu n'es pas d'accord, argumente.

**Semaine 4 — recommencer, ailleurs.** Un deuxième projet, une deuxième
culture de revue. Deux projets suffisent à voir ce qui est universel et ce
qui est une habitude locale.

## Outils

Node ou Bun, TypeScript, un éditeur avec le serveur de langage configuré.
Pour le backend : une base de données en local dans un conteneur, jamais une
base partagée. Voir le guide outillage.

## Premier défi

Une issue `good first issue` sur un projet du catalogue. Pas un projet
personnel : personne ne relit un projet personnel, et une contribution sans
relecture n'apprend rien.

## Où sont les gens

`#code-web` sur le Discord, et le canal du projet lui-même. Les mainteneurs
répondent plus volontiers dans leur propre espace qu'en message privé.
$md$,
 10),

('onboarding-web', 'onboarding', 'web', 'en',
 'Getting started in web',
 'Five trades, the largest number of open projects, and the shortest path to a first merged contribution.',
$md$
# Getting started in web

Five trades: frontend, backend, fullstack, performance, and web3 frontend.
This is the family with the most projects open to contributions, so a first
merged pull request arrives fastest here. It is also the family where it is
easiest to stay shallow — much code written, little understood.

## Thirty days

**Week 1 — read.** Pick a project from the first-issues feed and read it
without writing anything. Get its tests running locally. If you cannot, your
first contribution has already found you: the setup documentation.

**Week 2 — one labelled issue.** Take a `good first issue`. Open the pull
request even if you doubt it. A review is a free private lesson; waiting to
be perfect is what stops most people contributing at all.

**Week 3 — answer the review.** This is the week that counts. A maintainer
asking for changes is telling you exactly what they expect. Apply them, argue
where you disagree.

**Week 4 — again, elsewhere.** A second project, a second review culture. Two
is enough to see what is universal and what is a local habit.

## Tools

Node or Bun, TypeScript, an editor with the language server configured. For
backend work: a database in a local container, never a shared one. See the
toolkit guide.

## First challenge

A `good first issue` on a catalogue project. Not a personal project: nobody
reviews a personal project, and an unreviewed contribution teaches nothing.

## Where the people are

`#code-web` on Discord, and the project's own channel. Maintainers answer more
readily in their own space than in private messages.
$md$,
 10),

('onboarding-mobile', 'onboarding', 'mobile', 'fr',
 'Débuter dans le mobile',
 'iOS, Android, multiplateforme. Le seul domaine où le matériel décide de ce que tu peux faire.',
$md$
# Débuter dans le mobile

Trois métiers : iOS, Android, multiplateforme. C'est le seul domaine où le
matériel décide : sans machine Apple, iOS est fermé, et il vaut mieux le
savoir maintenant que dans trois semaines.

## Trente jours

**Semaine 1 — publier quelque chose.** Une application minuscule, sur ton
propre téléphone. Le but n'est pas l'application, c'est de traverser la chaîne
d'outils : signature, provisioning, déploiement. C'est là que tout le monde
bloque.

**Semaine 2 — lire un projet ouvert.** Les projets mobiles open source sont
plus rares et souvent plus accueillants. Fais-le compiler.

**Semaine 3 — une contribution.** Souvent de l'accessibilité, de la
traduction ou une correction d'interface. Ces contributions sont sous-estimées
et immédiatement utiles.

**Semaine 4 — un écran complet.** Assez pour comprendre le cycle de vie,
l'état, et ce qui se passe quand l'application passe en arrière-plan.

## Outils

Android : Android Studio, un émulateur, et un appareil réel dès que possible —
l'émulateur ment sur les performances. iOS : Xcode, donc un Mac. En
multiplateforme : Flutter ou React Native, et Kotlin Multiplatform si tu viens
d'Android.

## Premier défi

Une correction d'accessibilité sur une application open source. Concret,
vérifiable, et cela t'apprend l'API la plus mal documentée de chaque
plateforme.

## Où sont les gens

`#code-mobile`, `#lang-kotlin`, `#lang-swift`. Les éditions africaines de
droidcon (Lagos, Nairobi) sont parmi les rares conférences qui viennent sur le
continent plutôt que d'attendre qu'on en sorte.
$md$,
 20),

('onboarding-mobile', 'onboarding', 'mobile', 'en',
 'Getting started in mobile',
 'iOS, Android, cross-platform. The one domain where hardware decides what you can do.',
$md$
# Getting started in mobile

Three trades: iOS, Android, cross-platform. This is the one domain where
hardware decides: without an Apple machine, iOS is closed, and it is better to
know that now than in three weeks.

## Thirty days

**Week 1 — ship something.** A tiny application, on your own phone. The point
is not the app, it is getting through the toolchain: signing, provisioning,
deployment. That is where everybody gets stuck.

**Week 2 — read an open project.** Open source mobile projects are rarer and
often more welcoming. Get it building.

**Week 3 — one contribution.** Often accessibility, translation or a UI fix.
These are undervalued and immediately useful.

**Week 4 — a whole screen.** Enough to understand lifecycle, state, and what
happens when the app is backgrounded.

## Tools

Android: Android Studio, an emulator, and a real device as soon as possible —
the emulator lies about performance. iOS: Xcode, so a Mac. Cross-platform:
Flutter or React Native, and Kotlin Multiplatform if you come from Android.

## First challenge

An accessibility fix on an open source application. Concrete, checkable, and
it teaches you the worst-documented API on either platform.

## Where the people are

`#code-mobile`, `#lang-kotlin`, `#lang-swift`. The African droidcon editions
(Lagos, Nairobi) are among the few conference series that come to the
continent rather than expecting you to leave it.
$md$,
 20),

('onboarding-systems', 'onboarding', 'systems', 'fr',
 'Débuter dans les systèmes',
 'Noyau, pilotes, embarqué, robotique, critique. Long à apprendre, et personne ne peut le simuler.',
$md$
# Débuter dans les systèmes

Cinq métiers : programmation système, noyau et pilotes, firmware embarqué,
robotique, logiciel critique. C'est la famille la plus longue à apprendre et
la plus difficile à feindre — c'est exactement pour cela qu'une contribution
ici vaut ce qu'elle vaut.

## Trente jours

**Semaine 1 — compiler un noyau.** Le tien, sur ta machine, et démarrer
dessus. Cela ne sert à rien en soi et cela change tout : tu cesses de croire
que le système est une boîte noire.

**Semaine 2 — lire un pilote.** Un pilote simple, entier, du haut vers le
bas. Le code du noyau Linux est mieux commenté que sa réputation.

**Semaine 3 — un patch trivial.** Une faute de frappe dans un commentaire, un
`checkpatch` qui râle. L'intérêt n'est pas le patch : c'est de traverser le
processus par liste de diffusion, qui arrête neuf personnes sur dix.

**Semaine 4 — quelque chose de réel.** Petit, dans un sous-système peu
fréquenté. Les pilotes de matériel ancien acceptent des contributions que les
sous-systèmes centraux refuseraient.

## Outils

Un compilateur C récent, `gdb`, `perf`, QEMU pour ne pas redémarrer ta machine
à chaque essai. `git send-email` configuré — oui, vraiment. Rust est de plus
en plus accepté côté noyau et pilotes.

## Premier défi

Un patch envoyé sur une liste de diffusion et relu. Fusionné ou non : c'est
d'avoir traversé le processus qui compte.

## Où sont les gens

`#code-systems`, `#lang-rust`, `#lang-cpp`. kernelnewbies.org existe pour
exactement ce chemin, et lore.kernel.org archive toutes les discussions.
$md$,
 30),

('onboarding-systems', 'onboarding', 'systems', 'en',
 'Getting started in systems',
 'Kernel, drivers, embedded, robotics, safety-critical. Slow to learn, and impossible to fake.',
$md$
# Getting started in systems

Five trades: systems programming, kernel and drivers, embedded firmware,
robotics, safety-critical software. This is the slowest family to learn and
the hardest to fake — which is exactly why a contribution here is worth what
it is worth.

## Thirty days

**Week 1 — build a kernel.** Yours, on your machine, and boot it. Useless in
itself and it changes everything: you stop believing the system is a black
box.

**Week 2 — read a driver.** A simple one, whole, top to bottom. Linux kernel
code is better commented than its reputation.

**Week 3 — a trivial patch.** A typo in a comment, a `checkpatch` complaint.
The patch is not the point: getting through the mailing-list process is, and
it stops nine people in ten.

**Week 4 — something real.** Small, in a quiet subsystem. Drivers for old
hardware accept contributions that core subsystems would refuse.

## Tools

A recent C compiler, `gdb`, `perf`, QEMU so you are not rebooting your machine
for every attempt. `git send-email` configured — yes, really. Rust is
increasingly accepted for kernel and driver work.

## First challenge

A patch sent to a mailing list and reviewed. Merged or not: getting through
the process is what counts.

## Where the people are

`#code-systems`, `#lang-rust`, `#lang-cpp`. kernelnewbies.org exists for
exactly this path, and lore.kernel.org archives every discussion.
$md$,
 30),

('onboarding-blockchain', 'onboarding', 'blockchain', 'fr',
 'Débuter dans la blockchain',
 'Smart contracts et protocoles. Le seul domaine où un bug se solde immédiatement en argent.',
$md$
# Débuter dans la blockchain

Deux métiers : smart contract et protocole. C'est le seul domaine où une
erreur se traduit immédiatement en perte d'argent réel et irrécupérable. La
culture de revue y est plus dure qu'ailleurs, pour de bonnes raisons.

## Trente jours

**Semaine 1 — un contrat sur testnet.** Trivial, déployé, appelé depuis une
autre adresse. Comprendre le gaz avant d'écrire quoi que ce soit de sérieux.

**Semaine 2 — casser le tien.** Reentrancy, dépassement, contrôle d'accès
absent. Écris l'exploit toi-même. On n'apprend pas la sécurité en lisant des
listes.

**Semaine 3 — lire un audit.** Les rapports d'audit publics sont la meilleure
littérature technique du domaine, et ils sont gratuits.

**Semaine 4 — une contribution.** Souvent des tests ou de la documentation sur
une bibliothèque établie. Personne ne confie un contrat en production à
quelqu'un dont c'est le premier mois.

## Outils

Foundry — plus rapide que ce qui l'a précédé et le standard de fait. Slither
et Echidna pour l'analyse. Un nœud local plutôt qu'un service distant.

## Premier défi

Un test manquant sur une bibliothèque de contrats connue. Utile, relu
sérieusement, et sans risque.

## Où sont les gens

`#code-blockchain`. Attention : c'est aussi le domaine où le bruit
commercial est le plus fort. Suis les auditeurs, pas les comptes qui
annoncent des rendements.
$md$,
 40),

('onboarding-blockchain', 'onboarding', 'blockchain', 'en',
 'Getting started in blockchain',
 'Smart contracts and protocols. The one domain where a bug settles immediately, in money.',
$md$
# Getting started in blockchain

Two trades: smart contracts and protocols. It is the one domain where a
mistake translates immediately into real, unrecoverable money. The review
culture is harsher than elsewhere, for good reasons.

## Thirty days

**Week 1 — a contract on a testnet.** Trivial, deployed, called from another
address. Understand gas before writing anything serious.

**Week 2 — break your own.** Reentrancy, overflow, missing access control.
Write the exploit yourself. Nobody learns security from reading lists.

**Week 3 — read an audit.** Public audit reports are the best technical
writing in the field, and they are free.

**Week 4 — one contribution.** Usually tests or documentation on an
established library. Nobody hands a production contract to somebody in their
first month.

## Tools

Foundry — faster than what came before and the de facto standard. Slither and
Echidna for analysis. A local node rather than a hosted service.

## First challenge

A missing test on a well-known contract library. Useful, seriously reviewed,
and risk-free.

## Where the people are

`#code-blockchain`. Be warned: this is also where the commercial noise is
loudest. Follow the auditors, not the accounts announcing yields.
$md$,
 40),

('onboarding-compilers', 'onboarding', 'compilers', 'fr',
 'Débuter dans les compilateurs',
 'Compilateurs, langages, méthodes formelles. Le domaine le plus intimidant et le plus documenté.',
$md$
# Débuter dans les compilateurs

Deux métiers : compilateur et langage, méthodes formelles. Réputation
d'inaccessibilité largement usurpée : c'est le domaine le mieux documenté de
tous, parce que les gens qui l'occupent écrivent beaucoup.

## Trente jours

**Semaine 1 — un interpréteur.** Un langage jouet, en un après-midi. Lexer,
parseur, évaluateur. Tout le reste est une élaboration de ces trois pièces.

**Semaine 2 — lire un vrai front-end.** Celui de Rust ou de TypeScript. Ne
cherche pas à tout comprendre : suis un seul message d'erreur depuis son
émission jusqu'à son affichage.

**Semaine 3 — améliorer un message d'erreur.** C'est la contribution
d'entrée la plus courante et la plus utile. Les projets de langages ont
souvent une étiquette dédiée.

**Semaine 4 — un cas de test.** Un programme qui devrait compiler et ne
compile pas, réduit au minimum. Réduire un cas est une compétence à part
entière.

## Outils

Rust ou OCaml selon la communauté. Coq, Lean ou TLA+ côté méthodes formelles.
Un débogueur qui fonctionne sur ton compilateur : cela prend une journée à
configurer et fait gagner des semaines.

## Premier défi

Un message de diagnostic amélioré, avec le test qui va avec.

## Où sont les gens

`#code-compilers-formal`, `#lang-rust`. Le Zulip de Rust est ouvert et les
discussions de conception y sont publiques : c'est une salle de cours dont
personne ne se sert assez.
$md$,
 50),

('onboarding-compilers', 'onboarding', 'compilers', 'en',
 'Getting started in compilers',
 'Compilers, languages, formal methods. The most intimidating field, and the best documented.',
$md$
# Getting started in compilers

Two trades: compilers and languages, formal methods. The reputation for
inaccessibility is largely undeserved: this is the best-documented field of
all, because the people in it write a great deal.

## Thirty days

**Week 1 — an interpreter.** A toy language, in an afternoon. Lexer, parser,
evaluator. Everything else is an elaboration of those three pieces.

**Week 2 — read a real front end.** Rust's or TypeScript's. Do not try to
understand all of it: follow one error message from where it is raised to
where it is printed.

**Week 3 — improve an error message.** The most common and most useful entry
contribution. Language projects often have a label for it.

**Week 4 — a test case.** A program that should compile and does not, reduced
to the minimum. Reducing a case is a skill in its own right.

## Tools

Rust or OCaml depending on the community. Coq, Lean or TLA+ for formal
methods. A debugger that actually works on your compiler: a day to set up, and
it saves weeks.

## First challenge

An improved diagnostic message, with the test that goes with it.

## Where the people are

`#code-compilers-formal`, `#lang-rust`. The Rust Zulip is open and its design
discussions are public: it is a classroom nobody uses enough.
$md$,
 50),

('onboarding-data', 'onboarding', 'data', 'fr',
 'Débuter dans les données distribuées',
 'Moteurs de bases, recherche, systèmes distribués, traitement de flux. Là où les hypothèses meurent.',
$md$
# Débuter dans les données distribuées

Quatre métiers : moteur de base de données, moteur de recherche, systèmes
distribués, traitement de flux. C'est la famille où les intuitions sont le
plus souvent fausses, et où les mesures remplacent les opinions.

## Trente jours

**Semaine 1 — un moteur de stockage.** Clé-valeur, sur disque, en un fichier.
Puis fais-lui perdre des données en coupant le processus au mauvais moment.
La durabilité cesse d'être un mot.

**Semaine 2 — lire un article.** Raft, Dynamo, ou le papier LSM-tree. Puis
lire le code qui l'implémente. L'écart entre les deux est l'essentiel du
métier.

**Semaine 3 — un banc d'essai.** Mesure quelque chose, change une chose,
mesure à nouveau. Publie la méthode avec le résultat : un chiffre sans
méthode ne vaut rien.

**Semaine 4 — une contribution.** Souvent un test de régression ou une
correction dans un chemin peu emprunté. Les projets de bases de données sont
prudents et le disent clairement.

## Outils

PostgreSQL en local, `EXPLAIN ANALYZE` comme réflexe. Un outil de trace
distribué. Jepsen pour lire — et éventuellement pour écrire, plus tard.

## Premier défi

Un banc d'essai reproductible sur une requête réelle, avec la méthode
publiée.

## Où sont les gens

`#code-data-distributed`. La liste pgsql-hackers est publique et archivée :
c'est l'endroit où l'on voit comment une base de données est réellement
conçue.
$md$,
 60),

('onboarding-data', 'onboarding', 'data', 'en',
 'Getting started in distributed data',
 'Database engines, search, distributed systems, stream processing. Where assumptions go to die.',
$md$
# Getting started in distributed data

Four trades: database engines, search engines, distributed systems, stream
processing. This is the family where intuition is most often wrong, and where
measurements replace opinions.

## Thirty days

**Week 1 — a storage engine.** Key-value, on disk, in one file. Then make it
lose data by killing the process at the wrong moment. Durability stops being
a word.

**Week 2 — read a paper.** Raft, Dynamo, or the LSM-tree paper. Then read the
code implementing it. The gap between the two is most of the job.

**Week 3 — a benchmark.** Measure something, change one thing, measure again.
Publish the method with the result: a number with no method is worth nothing.

**Week 4 — one contribution.** Usually a regression test or a fix in a rarely
travelled path. Database projects are cautious and say so plainly.

## Tools

PostgreSQL locally, `EXPLAIN ANALYZE` as a reflex. A distributed tracing tool.
Jepsen to read — and maybe to write, later.

## First challenge

A reproducible benchmark on a real query, with the method published.

## Where the people are

`#code-data-distributed`. The pgsql-hackers list is public and archived: it is
where you see how a database is actually designed.
$md$,
 60),

('onboarding-scientific', 'onboarding', 'scientific', 'fr',
 'Débuter dans le calcul scientifique',
 'Calcul scientifique, GPU, finance haute fréquence. Là où la performance est la fonctionnalité.',
$md$
# Débuter dans le calcul scientifique

Trois métiers : calcul scientifique, calcul GPU, finance haute fréquence.
Point commun : la performance n'est pas une optimisation tardive, c'est la
fonctionnalité elle-même.

## Trente jours

**Semaine 1 — mesurer avant de croire.** Prends un calcul que tu écris
naïvement, mesure-le, puis regarde où passe réellement le temps. La réponse
est presque toujours ailleurs que là où tu pensais.

**Semaine 2 — un noyau GPU.** Une multiplication de matrices en CUDA ou
WebGPU. Compare à la version CPU. Comprendre pourquoi le transfert mémoire
domine est la leçon centrale.

**Semaine 3 — reproduire un résultat.** Prends un article avec du code publié
et refais tourner. La moitié ne se reproduit pas, et le dire est déjà une
contribution.

**Semaine 4 — une correction numérique.** Stabilité, précision, cas limite.
Les bibliothèques scientifiques accueillent bien ces contributions parce
qu'elles sont vérifiables.

## Outils

Python avec NumPy pour prototyper, Rust, C++ ou Julia pour ce qui doit être
rapide. Un profileur, toujours. `perf` sous Linux, Nsight côté NVIDIA.

## Premier défi

Reproduire le résultat d'un article, publier ce qui a marché et ce qui n'a pas
marché.

## Où sont les gens

`#code-scientific`, `#lang-julia`, `#lang-python`. Beaucoup de ces
communautés vivent sur des listes académiques plutôt que sur Discord ; c'est
inhabituel et cela vaut le détour.
$md$,
 70),

('onboarding-scientific', 'onboarding', 'scientific', 'en',
 'Getting started in scientific computing',
 'Scientific computing, GPU, high-frequency trading. Where performance is the feature.',
$md$
# Getting started in scientific computing

Three trades: scientific computing, GPU compute, high-frequency trading. What
they share: performance is not a late optimisation, it is the feature.

## Thirty days

**Week 1 — measure before believing.** Take a computation you write naively,
measure it, then look at where the time actually goes. The answer is almost
always somewhere other than where you thought.

**Week 2 — a GPU kernel.** A matrix multiplication in CUDA or WebGPU. Compare
against the CPU version. Understanding why memory transfer dominates is the
central lesson.

**Week 3 — reproduce a result.** Take a paper with published code and run it.
Half of them do not reproduce, and saying so is already a contribution.

**Week 4 — a numerical fix.** Stability, precision, an edge case. Scientific
libraries welcome these because they are checkable.

## Tools

Python with NumPy to prototype, Rust, C++ or Julia for what has to be fast. A
profiler, always. `perf` on Linux, Nsight on NVIDIA.

## First challenge

Reproduce a paper's result, and publish what worked and what did not.

## Where the people are

`#code-scientific`, `#lang-julia`, `#lang-python`. Many of these communities
live on academic mailing lists rather than Discord; that is unusual and worth
the detour.
$md$,
 70),

('onboarding-devtools-media', 'onboarding', 'devtools-media', 'fr',
 'Débuter dans l''outillage et les plateformes',
 'CLI, extensions, systèmes de build, média, réseau, logiciel d''entreprise. Neuf métiers, un même réflexe.',
$md$
# Débuter dans l'outillage et les plateformes

Neuf métiers : CLI, extension d'éditeur, système de build, applications
desktop, logiciel d'entreprise, plateformes low-code, traitement média,
protocoles réseau, applications de plateforme. Le point commun n'est pas la
technologie : c'est que tes utilisateurs sont d'autres développeurs, et qu'ils
sont exigeants.

## Trente jours

**Semaine 1 — automatiser ton propre agacement.** La chose que tu fais à la
main trois fois par semaine. C'est le seul outil que tu maintiendras.

**Semaine 2 — le donner à quelqu'un.** Une seule personne. Regarde-la s'en
servir sans intervenir. Chaque hésitation est un défaut de conception.

**Semaine 3 — le publier.** Sur un registre, avec un README qui dit quoi
installer et pourquoi. Un outil sans documentation n'a pas d'utilisateurs.

**Semaine 4 — contribuer ailleurs.** Une extension d'éditeur ou un plugin de
build existant. Tu verras la différence entre ton outil et un outil maintenu.

## Outils

Le langage compte moins que la distribution : un binaire unique, sans
dépendance d'exécution, est adopté ; un script qui exige une machine virtuelle
ne l'est pas. Rust et Go dominent pour cette raison.

## Premier défi

Publier un outil sur un registre et obtenir un utilisateur qui n'est pas toi.

## Où sont les gens

`#code-devtools`, `#code-showcase`. C'est la famille où montrer son travail
compte le plus : un outil que personne ne voit n'existe pas.
$md$,
 80),

('onboarding-devtools-media', 'onboarding', 'devtools-media', 'en',
 'Getting started in devtools and platforms',
 'CLIs, extensions, build systems, media, networking, enterprise software. Nine trades, one reflex.',
$md$
# Getting started in devtools and platforms

Nine trades: CLIs, editor extensions, build systems, desktop applications,
enterprise software, low-code platforms, media processing, network protocols,
platform applications. What they share is not technology: it is that your
users are other developers, and they are demanding.

## Thirty days

**Week 1 — automate your own annoyance.** The thing you do by hand three
times a week. It is the only tool you will keep maintaining.

**Week 2 — give it to somebody.** One person. Watch them use it without
intervening. Every hesitation is a design flaw.

**Week 3 — publish it.** To a registry, with a README saying what to install
and why. A tool with no documentation has no users.

**Week 4 — contribute elsewhere.** An existing editor extension or build
plugin. You will see the difference between your tool and a maintained one.

## Tools

The language matters less than the distribution: a single binary with no
runtime dependency gets adopted; a script that requires a virtual machine does
not. Rust and Go dominate here for that reason.

## First challenge

Publish a tool to a registry and get one user who is not you.

## Where the people are

`#code-devtools`, `#code-showcase`. This is the family where showing your work
matters most: a tool nobody sees does not exist.
$md$,
 80);
