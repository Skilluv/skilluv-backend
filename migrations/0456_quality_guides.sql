-- Five quality onboarding guides, one per trade, in two languages.
--
-- Rows and not files, for the reason migration 0199 gave: they have to be
-- translated and edited by somebody who is not deploying.
--
-- ## Why both locales, when the rest of this domain is English only
--
-- The seeded vocabulary of this domain is English because that is the
-- repository's default. These are not vocabulary — `content_guides` is
-- keyed by locale and every other domain ships both — and a francophone
-- arriving in the quality domain would otherwise be the only person on the
-- platform who gets no guide in their language.
--
-- ## What each one has to answer
--
-- Where to practise on something real, without permission being the blocker.
-- In this domain that is not a footnote: three of the five trades need
-- somebody else's system, and two of them need somebody else's *people*. A
-- first month that depends on an employer providing a product and five
-- participants is a first month that does not happen. Every guide names a way
-- in that does not require either.

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary,
     body_md, sort_order)
VALUES

-- ═══════════════════════════════════════════════════════════════════
-- qa-code — Software Test Engineer
-- ═══════════════════════════════════════════════════════════════════

('quality-onboarding-qa-code', 'onboarding', 'quality', 'automation', 'en',
 'Getting started as a software test engineer',
 'Your first month, on somebody else''s codebase, without asking anybody''s permission.',
$md$
# Software test engineer — the first month

## What this trade actually is

Not "writing tests". Deciding **what is worth putting to the test**, at which
level, at what cost — and then being able to say what a green suite does not
prove. The second half is the part that gets people hired.

## Where to practise without permission

Every open-source project with a test suite has gaps in it, and most of them
accept a contribution that closes one. You do not need an employer, a product
or an account. You need a repository you can clone and run.

Three starting points, in increasing order of difficulty:

1. A project whose test suite already runs locally in under five minutes.
2. A project whose suite is slow, and which has said so in an issue.
3. A project with an intermittent test somebody has already complained about.

The third one is where you learn the most and where nobody wants to go.

## Days 1–7 — read before you write

Run the existing suite. Time it. Then answer three questions in writing:

- What does this suite cover that matters?
- What does it not cover that matters more?
- Which of its tests would still pass if the code were wrong?

That last question is the whole trade in one line. The way to answer it is to
break the code on purpose and see what fails. If nothing does, you have found
something worth reporting before you have written a single test.

## Days 8–20 — the first contribution

Close one gap. One. A pull request that adds forty tests gets read by nobody;
one that adds four, with a paragraph saying which risk they cover, gets
merged.

Write the paragraph first. If you cannot say what risk the test covers, the
test is decoration.

## Days 21–30 — the artefact

Turn what you have learned into a **test plan** for a feature that does not
exist yet — one that is on the project's roadmap, or one you invent. State
what you would cover, at which level, and what you would deliberately not
cover and why.

That document is your first Skilluv quality artefact. The omissions are what
a reviewer reads first.

## What gets a submission refused here

- A test that would pass with the code broken.
- A suite that only passes in the order it was written in.
- A coverage percentage with no report behind it.
- A plan that claims to cover everything.

## Where to ask

`#quality-code` on Discord for the trade, `#quality-help` when you are stuck
on something specific. Bring the repository link and the command you ran.
$md$, 10),

('quality-onboarding-qa-code', 'onboarding', 'quality', 'automation', 'fr',
 'Débuter comme ingénieur de test logiciel',
 'Ton premier mois, sur le code de quelqu''un d''autre, sans demander la permission à personne.',
$md$
# Ingénieur de test logiciel — le premier mois

## Ce qu''est vraiment ce métier

Pas « écrire des tests ». Décider **ce qui vaut la peine d''être éprouvé**, à
quel niveau, à quel coût — puis savoir dire ce qu''une suite verte ne prouve
pas. C''est la seconde moitié qui fait recruter.

## Où pratiquer sans permission

Tout projet libre avec une suite de tests a des trous dedans, et la plupart
acceptent une contribution qui en comble un. Tu n''as besoin ni d''employeur,
ni de produit, ni de compte. Tu as besoin d''un dépôt que tu peux cloner et
faire tourner.

Trois points de départ, du plus simple au plus dur :

1. Un projet dont la suite tourne en local en moins de cinq minutes.
2. Un projet dont la suite est lente, et qui l''a dit dans un ticket.
3. Un projet avec un test instable dont quelqu''un s''est déjà plaint.

Le troisième est celui où on apprend le plus et où personne ne veut aller.

## Jours 1 à 7 — lire avant d''écrire

Fais tourner la suite existante. Chronomètre-la. Puis réponds par écrit à
trois questions :

- Qu''est-ce que cette suite couvre qui compte ?
- Qu''est-ce qu''elle ne couvre pas qui compte davantage ?
- Lesquels de ses tests passeraient encore si le code était faux ?

La dernière question, c''est tout le métier en une ligne. Pour y répondre, on
casse le code exprès et on regarde ce qui échoue. Si rien n''échoue, tu as
trouvé quelque chose à signaler avant même d''avoir écrit un test.

## Jours 8 à 20 — la première contribution

Comble un trou. Un seul. Une contribution qui ajoute quarante tests n''est lue
par personne ; celle qui en ajoute quatre, avec un paragraphe disant quel
risque ils couvrent, est fusionnée.

Écris le paragraphe d''abord. Si tu n''arrives pas à dire quel risque le test
couvre, le test est décoratif.

## Jours 21 à 30 — l''artefact

Transforme ce que tu as appris en **plan de test** pour une fonctionnalité qui
n''existe pas encore — une de la feuille de route du projet, ou une que tu
inventes. Dis ce que tu couvrirais, à quel niveau, et ce que tu ne couvrirais
délibérément pas, avec la raison.

Ce document est ton premier artefact qualité Skilluv. Ce sont les
renoncements qu''un relecteur lit en premier.

## Ce qui fait refuser une soumission ici

- Un test qui passerait avec le code cassé.
- Une suite qui ne passe que dans l''ordre où elle a été écrite.
- Un pourcentage de couverture sans son rapport.
- Un plan qui prétend tout couvrir.

## Où demander

`#quality-code` sur Discord pour le métier, `#quality-help` quand tu bloques
sur quelque chose de précis. Viens avec le lien du dépôt et la commande que tu
as lancée.
$md$, 10),

-- ═══════════════════════════════════════════════════════════════════
-- qa-cyber — Disciplined Penetration Tester
-- ═══════════════════════════════════════════════════════════════════

('quality-onboarding-qa-cyber', 'onboarding', 'quality', 'intrusion', 'en',
 'Getting started as a disciplined penetration tester',
 'Method before tooling, and a scope before anything at all.',
$md$
# Disciplined penetration tester — the first month

## The line that defines this trade

**Nothing outside the written scope.** Not "I only looked", not "it was
obviously in scope". A test without signed rules of engagement is not a test,
it is an intrusion, and on this platform it is refused before anybody reads
the findings.

That is not a legal formality bolted on. It is the discipline the whole trade
rests on: somebody who cannot bound their own scope cannot be trusted with
anybody's system.

## Where to practise legally

Targets built to be attacked, which need no permission because permission is
their whole purpose:

- **OWASP Juice Shop** — the standard first target. Runs in one container.
- **DVWA**, **WebGoat** — older, and still the clearest for injection classes.
- **HackTheBox**, **TryHackMe** — structured, and free tiers are enough.
- **Public bug bounty programmes** — read the scope page twice before starting.

Never a site because it "looked vulnerable". Never a former employer's.

## Days 1–10 — pick a method and follow it

Instinct does not transfer and cannot be reviewed. Pick one:

- **OWASP Testing Guide** — the most complete for web.
- **PTES** — broader, better for an engagement with phases.

Then run a full pass on Juice Shop *following it*, including the parts that
find nothing. The parts that find nothing are what makes the report a report.

## Days 11–20 — the write-up

For every finding: the request, the payload, the response. A reviewer has to
reproduce it without asking you anything.

Then the part people skip: **the false positives**. What the tool flagged that
was not real, and why. An untriaged scanner output is not a report — it is a
file, and handing it over is how this trade gets its bad reputation.

## Days 21–30 — the artefact

A complete report on a training target: scope, named method, findings with
their reproductions, false positives dismissed with reasons, and severities
argued in terms of what an attacker actually gets.

Not a tool score copied across. "CVSS 7.5" is not an argument; "an
unauthenticated user reads any other user's invoices" is.

## What gets a submission refused here

- No written scope. Refused outright, whatever was found.
- A finding nobody else can reproduce.
- A severity that is a tool score with no reasoning.
- Anything published before the agreed disclosure date.

## Where to ask

`#quality-cyber` on Discord. If you are unsure whether something is in scope,
ask **before**, not after.
$md$, 20),

('quality-onboarding-qa-cyber', 'onboarding', 'quality', 'intrusion', 'fr',
 'Débuter comme testeur d''intrusion discipliné',
 'La méthode avant l''outil, et un périmètre avant tout le reste.',
$md$
# Testeur d''intrusion discipliné — le premier mois

## La ligne qui définit ce métier

**Rien en dehors du périmètre écrit.** Pas « j''ai seulement regardé », pas
« c''était évidemment dedans ». Un test sans règles d''engagement signées n''est
pas un test, c''est une intrusion, et sur cette plateforme il est refusé avant
même qu''on lise les constats.

Ce n''est pas une formalité juridique ajoutée après coup. C''est la discipline
sur laquelle tout le métier repose : qui ne sait pas borner son propre
périmètre ne peut pas se voir confier le système de quelqu''un d''autre.

## Où pratiquer légalement

Des cibles faites pour être attaquées, qui ne demandent aucune permission
puisque c''est leur raison d''être :

- **OWASP Juice Shop** — la première cible standard. Tourne en un conteneur.
- **DVWA**, **WebGoat** — plus anciennes, et toujours les plus claires sur les
  familles d''injection.
- **HackTheBox**, **TryHackMe** — structurées, et les offres gratuites
  suffisent.
- **Programmes de prime au bug publics** — lis la page de périmètre deux fois
  avant de commencer.

Jamais un site parce qu''« il avait l''air vulnérable ». Jamais celui d''un
ancien employeur.

## Jours 1 à 10 — choisir une méthode et la suivre

L''intuition ne se transmet pas et ne se relit pas. Choisis-en une :

- **OWASP Testing Guide** — la plus complète pour le web.
- **PTES** — plus large, mieux adaptée à une mission en phases.

Puis mène une passe complète sur Juice Shop *en la suivant*, y compris les
parties qui ne trouvent rien. Ce sont ces parties-là qui font d''un rapport un
rapport.

## Jours 11 à 20 — la rédaction

Pour chaque constat : la requête, la charge, la réponse. Un relecteur doit
pouvoir le reproduire sans rien te demander.

Puis la partie que tout le monde saute : **les faux positifs**. Ce que l''outil
a signalé et qui n''en était pas, avec la raison. Une sortie de scanner non
triée n''est pas un rapport — c''est un fichier, et le remettre tel quel est ce
qui donne sa mauvaise réputation au métier.

## Jours 21 à 30 — l''artefact

Un rapport complet sur une cible d''entraînement : périmètre, méthode nommée,
constats avec leurs reproductions, faux positifs écartés avec leurs raisons,
et gravités argumentées en fonction de ce qu''un attaquant obtient réellement.

Pas un score d''outil recopié. « CVSS 7.5 » n''est pas un argument ; « un
utilisateur non authentifié lit les factures de n''importe qui » en est un.

## Ce qui fait refuser une soumission ici

- Pas de périmètre écrit. Refus immédiat, quoi qu''on ait trouvé.
- Un constat que personne d''autre ne peut reproduire.
- Une gravité qui est un score d''outil sans raisonnement.
- Toute publication avant la date de divulgation convenue.

## Où demander

`#quality-cyber` sur Discord. Si tu doutes qu''une chose soit dans le
périmètre, demande **avant**, pas après.
$md$, 20),

-- ═══════════════════════════════════════════════════════════════════
-- qa-design — Usability and Accessibility Researcher
-- ═══════════════════════════════════════════════════════════════════

('quality-onboarding-qa-design', 'onboarding', 'quality', 'usability', 'en',
 'Getting started as a usability and accessibility researcher',
 'Five participants you do not have yet, and the audit you can start today.',
$md$
# Usability and accessibility researcher — the first month

## The two halves, and which one to start with

**Accessibility auditing** needs a page and a standard. You can start this
afternoon, alone.

**Usability research** needs people, and recruiting five of them is the part
that stops most beginners. Start with the audit; recruit while you do it.

## Days 1–10 — an audit, done properly

Pick a real page. Not a demo — something with a form, a table and a modal.

Audit it against **WCAG 2.2 level AA**, and name the level in the report.
"Not accessible" is not a finding; "1.4.3 Contrast (Minimum): the secondary
button is 3.1:1 against its background, AA requires 4.5:1" is.

Use a tool — axe, WAVE, Lighthouse — and then do the part the tool cannot:

- Navigate the whole page with the keyboard only. Where does focus go?
- Turn on a screen reader for twenty minutes. What does the table announce?
- Zoom to 400%. Does anything become unreachable?

Automated tools find roughly a third of real issues. The other two thirds are
the reason this is a trade.

Every defect gets a **proposed fix and its estimated cost**. An audit with no
way out becomes a list nobody opens.

## Days 11–25 — the study

Recruiting five people is a real task, so treat it as one:

- Not five colleagues. Five colleagues are five people who already know the
  product exists.
- Say who you need and why, in one sentence, before you look.
- Communities, user groups, and the product's own users if the team will ask.

Then: **written consent**, always, before anything is recorded. Anonymise in
the report. This is not optional here — a session run without consent is
refused whatever it found.

During the session, the single rule: **do not help**. The moment the person
gets stuck is the data. Giving away the answer destroys the only thing you
came for.

## Days 26–30 — the write-up

Two sections that must not blur into each other:

- **What was observed.** "Four of five participants clicked the logo to go
  back."
- **What is inferred.** "The back affordance is not discoverable."

The first is data. The second is your reading of it, and a reader is entitled
to disagree with it while keeping the first.

Raw quotes, not paraphrases. Rephrasing comes after, flagged as such.

## What gets a submission refused here

- A recording with no consent. Refused outright.
- Five colleagues presented as five users.
- A protocol that cannot support what it concludes.
- An audit that names no standard and no level.

## Where to ask

`#quality-design` on Discord. Recruitment questions are welcome there — it is
the hard part and everybody has hit it.
$md$, 30),

('quality-onboarding-qa-design', 'onboarding', 'quality', 'usability', 'fr',
 'Débuter comme chercheur en utilisabilité et accessibilité',
 'Cinq participants que tu n''as pas encore, et l''audit que tu peux commencer aujourd''hui.',
$md$
# Chercheur en utilisabilité et accessibilité — le premier mois

## Les deux moitiés, et par laquelle commencer

L''**audit d''accessibilité** demande une page et une norme. Tu peux commencer
cet après-midi, seul.

L''**étude d''utilisabilité** demande des gens, et en recruter cinq est ce qui
arrête la plupart des débutants. Commence par l''audit ; recrute pendant.

## Jours 1 à 10 — un audit, fait correctement

Prends une vraie page. Pas une démo — quelque chose avec un formulaire, un
tableau et une fenêtre modale.

Audite-la contre **WCAG 2.2 niveau AA**, et nomme le niveau dans le rapport.
« Pas accessible » n''est pas un constat ; « 1.4.3 Contraste minimum : le
bouton secondaire est à 3,1:1 sur son fond, AA demande 4,5:1 » en est un.

Utilise un outil — axe, WAVE, Lighthouse — puis fais la partie que l''outil ne
peut pas faire :

- Parcours toute la page au clavier seul. Où va le focus ?
- Allume un lecteur d''écran pendant vingt minutes. Qu''annonce le tableau ?
- Zoome à 400 %. Est-ce que quelque chose devient inatteignable ?

Les outils automatiques trouvent environ un tiers des vrais problèmes. Les
deux autres tiers sont la raison pour laquelle c''est un métier.

Chaque défaut vient avec **un correctif proposé et son coût estimé**. Un audit
sans chemin de sortie devient une liste que personne n''ouvre.

## Jours 11 à 25 — l''étude

Recruter cinq personnes est une vraie tâche : traite-la comme telle.

- Pas cinq collègues. Cinq collègues, ce sont cinq personnes qui savent déjà
  que le produit existe.
- Dis qui il te faut et pourquoi, en une phrase, avant de chercher.
- Communautés, groupes d''utilisateurs, et les utilisateurs du produit si
  l''équipe accepte de demander.

Ensuite : **consentement écrit**, toujours, avant tout enregistrement.
Anonymise dans le rapport. Ce n''est pas facultatif ici — une séance menée sans
consentement est refusée quoi qu''elle ait trouvé.

Pendant la séance, la seule règle : **ne pas aider**. Le moment où la personne
bloque est la donnée. Souffler la réponse détruit la seule chose pour laquelle
tu es venu.

## Jours 26 à 30 — la restitution

Deux sections qui ne doivent pas se mélanger :

- **Ce qui a été observé.** « Quatre participants sur cinq ont cliqué sur le
  logo pour revenir en arrière. »
- **Ce qu''on en déduit.** « Le retour n''est pas repérable. »

La première est une donnée. La seconde est ta lecture, et un lecteur a le
droit de ne pas être d''accord avec elle tout en gardant la première.

Des verbatims bruts, pas des reformulations. La reformulation vient après,
signalée.

## Ce qui fait refuser une soumission ici

- Un enregistrement sans consentement. Refus immédiat.
- Cinq collègues présentés comme cinq utilisateurs.
- Un protocole qui ne permet pas de conclure ce qu''il conclut.
- Un audit qui ne nomme ni norme ni niveau.

## Où demander

`#quality-design` sur Discord. Les questions de recrutement y sont les
bienvenues — c''est la partie dure et tout le monde s''y est cogné.
$md$, 30),

-- ═══════════════════════════════════════════════════════════════════
-- qa-game — Playtest Facilitator
-- ═══════════════════════════════════════════════════════════════════

('quality-onboarding-qa-game', 'onboarding', 'quality', 'playtest', 'en',
 'Getting started as a playtest facilitator',
 'Game jams are full of games nobody has watched anybody play.',
$md$
# Playtest facilitator — the first month

## What you are actually measuring

Not whether the game is good. **What the player did**, and where the gap is
between that and what the designer expected.

"They found it confusing" is an opinion, and a weak one. "They re-read the
tutorial three times, then quit on level two" is a measurement, and a designer
can act on it.

## Where to find games, this week

Game jams end every weekend, and they end with dozens of games that nobody
has ever watched a stranger play. Their authors will almost always say yes.

- **itch.io** jam pages, sorted by newest.
- **Ludum Dare**, **GMTK Jam** archives.
- `#game-*` channels on this platform's Discord.

Ask the author. One message: what you want to do, how many sessions, what
they get back. The answer is yes more often than beginners expect.

## Days 1–7 — the protocol

Write it before the first session, and do not change it between sessions.
Sessions run under different protocols do not add up, and a synthesis over
them compares different things.

The minimum:

- What the player is asked to do, and what they are told beforehand (as
  little as possible).
- How long.
- What you record, and the consent for it.
- The three questions you ask at the end, in the same words every time.

## Days 8–22 — five sessions

Five is the number where patterns start being visible and one person is still
enough to run it.

During: **you do not help**. Not a hint, not a "you could try". Write down the
moment you wanted to intervene — that moment is usually the finding.

Note the player's profile: familiar with the genre or not. It changes how
every observation is read, and leaving it out makes the report unusable.

## Days 23–30 — findings into decisions

A report that stops at findings gets thanked and shelved. Each finding should
propose a **possible trade-off**, and leave the decision to the team:

> Four of five players missed the second ability. Either the tutorial
> introduces it (costs a screen), or level two forces its use (costs a
> redesign), or it moves to level three (costs nothing, delays the depth).

That is what a facilitator hands over. The team decides.

## If your subject is balance

Then a **win rate comes with its number of matches**. Without the volume it is
not a measurement, and a reviewer will say so before reading anything else.

## What gets a submission refused here

- Sessions run under different protocols, summed as if they were one.
- A facilitator who helped.
- Balance data with no match count.
- Recordings with no consent.

## Where to ask

`#quality-game` on Discord, and `Playtest Sessions` voice when somebody is
running one live.
$md$, 40),

('quality-onboarding-qa-game', 'onboarding', 'quality', 'playtest', 'fr',
 'Débuter comme animateur de playtests',
 'Les game jams sont pleines de jeux que personne n''a jamais regardé quelqu''un jouer.',
$md$
# Animateur de playtests — le premier mois

## Ce que tu mesures réellement

Pas si le jeu est bon. **Ce que le joueur a fait**, et l''écart entre ça et ce
que le concepteur attendait.

« Il a trouvé ça confus » est un avis, et un avis faible. « Il a relu le
tutoriel trois fois, puis a arrêté au niveau deux » est une mesure, et un
concepteur peut agir dessus.

## Où trouver des jeux, cette semaine

Des game jams se terminent tous les week-ends, et elles se terminent avec des
dizaines de jeux que personne n''a jamais vu un inconnu jouer. Leurs auteurs
disent presque toujours oui.

- Les pages de jam sur **itch.io**, triées par nouveauté.
- Les archives de **Ludum Dare**, **GMTK Jam**.
- Les canaux `#game-*` du Discord de la plateforme.

Demande à l''auteur. Un message : ce que tu veux faire, combien de séances, ce
qu''il récupère. La réponse est oui plus souvent que les débutants ne le
croient.

## Jours 1 à 7 — le protocole

Écris-le avant la première séance, et ne le change pas entre les séances. Des
séances menées sous des protocoles différents ne s''additionnent pas, et une
synthèse qui les mélange compare des choses différentes.

Le minimum :

- Ce qu''on demande au joueur, et ce qu''on lui dit avant (le moins possible).
- La durée.
- Ce que tu enregistres, et le consentement correspondant.
- Les trois questions posées à la fin, dans les mêmes mots à chaque fois.

## Jours 8 à 22 — cinq séances

Cinq, c''est le nombre où les motifs commencent à se voir et où une seule
personne suffit encore à animer.

Pendant : **tu n''aides pas**. Pas un indice, pas un « tu pourrais essayer ».
Note le moment où tu as eu envie d''intervenir — ce moment-là est en général le
constat.

Note le profil du joueur : habitué du genre ou non. Ça change la lecture de
chaque observation, et l''omettre rend le rapport inutilisable.

## Jours 23 à 30 — des constats aux décisions

Un rapport qui s''arrête aux constats est remercié puis rangé. Chaque constat
devrait proposer un **arbitrage possible**, et laisser la décision à l''équipe :

> Quatre joueurs sur cinq sont passés à côté de la seconde capacité. Soit le
> tutoriel l''introduit (coûte un écran), soit le niveau deux force son usage
> (coûte une refonte), soit elle passe au niveau trois (ne coûte rien, retarde
> la profondeur).

C''est ça que remet un animateur. L''équipe tranche.

## Si ton sujet est l''équilibrage

Alors un **taux de victoire vient avec son nombre de parties**. Sans le
volume, ce n''est pas une mesure, et un relecteur le dira avant de lire quoi
que ce soit d''autre.

## Ce qui fait refuser une soumission ici

- Des séances menées sous des protocoles différents, additionnées comme si
  elles n''en faisaient qu''un.
- Un animateur qui a aidé.
- Des données d''équilibrage sans nombre de parties.
- Des enregistrements sans consentement.

## Où demander

`#quality-game` sur Discord, et le salon vocal `Playtest Sessions` quand
quelqu''un en anime une en direct.
$md$, 40),

-- ═══════════════════════════════════════════════════════════════════
-- qa-lead — Test Strategy Lead
-- ═══════════════════════════════════════════════════════════════════

('quality-onboarding-qa-lead', 'onboarding', 'quality', 'strategy', 'en',
 'Getting started as a test strategy lead',
 'The trade where the deliverable is a list of things you decided not to do.',
$md$
# Test strategy lead — the first month

## Read this first if you are here early

This trade is usually reached after several years in one of the other four.
That is not a rule and nobody checks — but a strategy written by somebody who
has never maintained a flaky suite tends to be a diagram, and reviewers here
notice.

If you are arriving early, the honest path is to write a strategy for a team
you are actually on, however small, rather than for a hypothetical team of
ten.

## What the deliverable actually is

**A list of owned omissions.** A strategy claiming to cover everything has
decided nothing, and that is the first thing a reviewer looks for.

The document says:

- What is put to the test, at which level, and by whom.
- What is **not**, and what risk that corresponds to, and who accepted it.
- What it costs — machine time, human time, time waiting on a merge.
- The indicator that will say whether it worked.

## Days 1–10 — measure before deciding

You cannot write a strategy for a system you have not measured. Three numbers,
before any opinion:

- How long the suite takes, end to end.
- How many of its failures in the last month were real.
- How long a change waits between "ready" and "merged".

The second number is the one nobody has. It is also the one that decides
whether the team trusts its own tests, and a team that does not trust its
tests has no strategy, it has a ritual.

## Days 11–20 — write the omissions

Start from what you will not do. It is faster, and it is where the argument
is:

> We do not test the admin panel end-to-end. It has four internal users, an
> outage there costs an hour of one person's day, and the suite would cost
> ninety seconds on every merge. Accepted by <role>, revisit if it goes
> external.

Three of those are worth more than a page of pyramid diagrams.

## Days 21–30 — ownership and survival

Two questions that separate a strategy from a document:

- **Who owns each level?** "The team" is not an answer. Who writes, who
  maintains, who is allowed to delete.
- **Does it survive your departure?** If it only works while you are there to
  enforce it, it is a personal practice, not a strategy.

Then the culture half: what is being asked of people, and what makes the new
path *easier* than the old one. An imposed ritual empties out in six months;
the only ones that hold are the ones that removed friction somewhere else.

## What gets a submission refused here

- A strategy with no stated omissions.
- A test pyramid with no cost attached.
- Ownership assigned to "the team".
- A success indicator that cannot move downwards.

## Where to ask

`#quality-lead` on Discord. Bring the three numbers — the conversation is much
shorter with them.
$md$, 50),

('quality-onboarding-qa-lead', 'onboarding', 'quality', 'strategy', 'fr',
 'Débuter comme responsable de la stratégie de test',
 'Le métier où le livrable est une liste de choses qu''on a décidé de ne pas faire.',
$md$
# Responsable de la stratégie de test — le premier mois

## À lire d''abord si tu arrives tôt

On atteint généralement ce métier après plusieurs années dans l''un des quatre
autres. Ce n''est pas une règle et personne ne vérifie — mais une stratégie
écrite par quelqu''un qui n''a jamais maintenu une suite instable a tendance à
être un schéma, et les relecteurs d''ici le remarquent.

Si tu arrives tôt, le chemin honnête est d''écrire une stratégie pour une
équipe dont tu fais réellement partie, aussi petite soit-elle, plutôt que pour
une équipe hypothétique de dix personnes.

## Ce qu''est vraiment le livrable

**Une liste de renoncements assumés.** Une stratégie qui prétend tout couvrir
n''a rien décidé, et c''est la première chose qu''un relecteur cherche.

Le document dit :

- Ce qui est éprouvé, à quel niveau, et par qui.
- Ce qui ne l''est **pas**, à quel risque ça correspond, et qui l''a accepté.
- Ce que ça coûte — temps machine, temps humain, temps d''attente sur une
  fusion.
- L''indicateur qui dira si ça a marché.

## Jours 1 à 10 — mesurer avant de décider

On n''écrit pas de stratégie pour un système qu''on n''a pas mesuré. Trois
chiffres, avant tout avis :

- Combien de temps la suite prend, de bout en bout.
- Combien de ses échecs du mois dernier étaient réels.
- Combien de temps un changement attend entre « prêt » et « fusionné ».

Le deuxième chiffre est celui que personne n''a. C''est aussi celui qui décide
si l''équipe fait confiance à ses propres tests, et une équipe qui ne leur fait
pas confiance n''a pas de stratégie, elle a un rituel.

## Jours 11 à 20 — écrire les renoncements

Commence par ce que tu ne feras pas. C''est plus rapide, et c''est là qu''est
l''argument :

> On ne teste pas le panneau d''administration de bout en bout. Il a quatre
> utilisateurs internes, une panne y coûte une heure de la journée d''une
> personne, et la suite coûterait quatre-vingt-dix secondes à chaque fusion.
> Accepté par <rôle>, à revoir s''il s''ouvre à l''extérieur.

Trois paragraphes comme celui-là valent mieux qu''une page de pyramides.

## Jours 21 à 30 — responsabilité et survie

Deux questions qui séparent une stratégie d''un document :

- **Qui porte chaque niveau ?** « L''équipe » n''est pas une réponse. Qui
  écrit, qui maintient, qui a le droit de supprimer.
- **Est-ce que ça survit à ton départ ?** Si ça ne marche que tant que tu es
  là pour le faire appliquer, c''est une pratique personnelle, pas une
  stratégie.

Puis la moitié culturelle : ce qu''on demande aux gens, et ce qui rend le
nouveau chemin *plus facile* que l''ancien. Un rituel imposé se vide en six
mois ; les seuls qui tiennent sont ceux qui ont retiré de la friction
ailleurs.

## Ce qui fait refuser une soumission ici

- Une stratégie sans renoncements énoncés.
- Une pyramide de tests sans coût attaché.
- Une responsabilité attribuée à « l''équipe ».
- Un indicateur de succès qui ne peut pas baisser.

## Où demander

`#quality-lead` sur Discord. Viens avec les trois chiffres — la conversation
est beaucoup plus courte avec eux.
$md$, 50);
