-- The briefs somebody fills in before commissioning leadership or QA work.
--
-- ## What was missing
--
-- Backlog tickets leadership/F-05 and quality/F-05 ask for brief templates.
-- Both were written as documents — `docs/leadership/BRIEF-TEMPLATES.md` and
-- `docs/quality/BRIEF-TEMPLATES.md` — and neither was seeded, so the platform
-- could serve the write-up templates a contributor hands back and not the
-- briefs a client fills in. Communication and education did the opposite:
-- rows seeded, no document. Both halves exist now for all four.
--
-- ## Why a brief is a template at all
--
-- The failure these prevent is the one every commissioned engagement has: a
-- client who knows what they want and has never written it down, and a
-- contributor who starts anyway because refusing feels rude. Two months later
-- the disagreement is about what was asked for, and neither side is lying.
--
-- So each brief is a form with a refusal condition attached: the fields that
-- must be answered before work starts are named, and the contributor is told
-- in the guide itself to decline rather than begin without them. A template
-- that only collects answers when the client happens to have them is a
-- template that helps in exactly the cases that never needed it.
--
-- ## Both locales
--
-- French and English, like every other guide in this table. A brief is filled
-- in by the client, who is the least likely person in the exchange to be
-- reading in their second language.

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

-- ═══════════════════════════════════════════════════════════════════
-- Quality — one per trade
-- ═══════════════════════════════════════════════════════════════════

('brief-qa-code', 'brief_template', 'quality', 'automation', 'en',
 'Brief — test plan or suite',
 'Filled in before commissioning automated testing. Points 1 to 3 are refusal conditions.',
$md$
# Brief — test plan or suite

## The system
- Which system, and which build or version:
- Where it runs, and how the contributor reaches it:
- Who can answer a question during the work:

## The one question
- What is this suite meant to tell you that you cannot tell today?

*One question. A brief with three gets three shallow answers.*

## Scope
- In scope:
- Explicitly out of scope:

## What already exists
- [ ] A suite · [ ] A partial suite · [ ] Nothing
- Does it currently pass? yes / no

*Taking over a red suite is a different job. Price it as one.*

## Where it has to run
- Pipeline:
- Run-time budget:
- Parallelism available: yes / no
- Database available in CI: yes / no

## After the handover
- Who owns and maintains this suite afterwards:

*A suite handed to a team that has not agreed to own it is deleted within six
months. Name the team or accept that outcome.*

## Coverage
- Paths that matter most:
- Coverage target, if any:

*A percentage target produces tests that satisfy the percentage. Naming the
paths is better and takes five more minutes.*

## Terms
- Deliverable format (from the quality formats list):
- Deadline, and what happens to the fee if the system is not ready on the
  start date:
- NDA: yes / no
- May the contributor state publicly that the engagement happened: yes / no

## Not in this brief
Which framework. That is the contributor's call unless your pipeline forces
one, and forcing one for preference costs the engagement its best option.
$md$, 400),

('brief-qa-code', 'brief_template', 'quality', 'automation', 'fr',
 'Brief — plan ou suite de tests',
 'À remplir avant de commander du test automatisé. Les points 1 à 3 sont des conditions de refus.',
$md$
# Brief — plan ou suite de tests

## Le système
- Quel système, et quelle version ou build :
- Où il tourne, et comment le contributeur y accède :
- Qui répond aux questions pendant le travail :

## La question unique
- Que doit vous dire cette suite que vous ne savez pas aujourd'hui ?

*Une seule question. Un brief qui en pose trois obtient trois réponses
superficielles.*

## Périmètre
- Dans le périmètre :
- Explicitement hors périmètre :

## L'existant
- [ ] Une suite · [ ] Une suite partielle · [ ] Rien
- Passe-t-elle actuellement ? oui / non

*Reprendre une suite au rouge est un autre métier. Facturez-le comme tel.*

## Où elle doit tourner
- Pipeline :
- Budget de temps d'exécution :
- Parallélisme disponible : oui / non
- Base de données disponible en CI : oui / non

## Après la livraison
- Qui possède et maintient cette suite ensuite :

*Une suite confiée à une équipe qui n'a pas accepté de la porter est supprimée
en six mois. Nommez l'équipe ou assumez ce résultat.*

## Couverture
- Les chemins qui comptent le plus :
- Objectif de couverture, s'il y en a un :

*Un objectif en pourcentage produit des tests qui satisfont le pourcentage.
Nommer les chemins vaut mieux et prend cinq minutes de plus.*

## Conditions
- Format de livrable (dans la liste des formats quality) :
- Échéance, et ce qu'il advient des honoraires si le système n'est pas prêt à
  la date de début :
- NDA : oui / non
- Le contributeur peut-il dire publiquement que la mission a eu lieu : oui / non

## Pas dans ce brief
Le framework. C'est le choix du contributeur, sauf si votre pipeline en impose
un — et en imposer un par préférence prive la mission de sa meilleure option.
$md$, 400),

('brief-qa-cyber', 'brief_template', 'quality', 'intrusion', 'en',
 'Brief — scoped security testing',
 'Filled in before commissioning security testing. Nothing starts without signed rules of engagement.',
$md$
# Brief — scoped security testing

## Authorisation

**Rules of engagement, signed, before anything is touched.** Not "we will sort
it out" — signed, naming the systems, the window, and the person who
authorised it. A contributor who starts without this is exposed personally,
and no fee covers that.

- Systems in scope, by hostname or identifier:
- Explicitly out of scope:
- Testing window (dates and hours):
- Who authorised this, and do they own the systems:
- If a third party hosts any of it, is their authorisation attached:

## The one question
- What are you trying to find out?

## Constraints
- Production or a copy:
- Rate limits, or a ban on automated scanning:
- What must never be attempted (data destruction, denial of service, social
  engineering of staff):
- Emergency contact during the window, reachable in minutes:

## Disclosure
- Fix window before anything may be described publicly:
- Is a redacted write-up permitted afterwards: yes / no
- If a finding affects a third-party component, who reports it upstream:

## Terms
- Deliverable format:
- Deadline:
- NDA: yes / no

## Refusal condition
Unsigned rules of engagement, unclear ownership of a system, or an
out-of-scope list that does not exist. Decline and say why. This is the one
brief where starting anyway is not a professional risk but a legal one.
$md$, 410),

('brief-qa-cyber', 'brief_template', 'quality', 'intrusion', 'fr',
 'Brief — test de sécurité cadré',
 'À remplir avant de commander un test de sécurité. Rien ne commence sans règles d''engagement signées.',
$md$
# Brief — test de sécurité cadré

## Autorisation

**Règles d'engagement signées avant qu'on ne touche à quoi que ce soit.** Pas
« on verra » : signées, nommant les systèmes, la fenêtre, et la personne qui
autorise. Un contributeur qui commence sans cela s'expose personnellement, et
aucun honoraire ne couvre ça.

- Systèmes dans le périmètre, par nom d'hôte ou identifiant :
- Explicitement hors périmètre :
- Fenêtre de test (dates et heures) :
- Qui autorise, et cette personne possède-t-elle les systèmes :
- Si un tiers en héberge une partie, son autorisation est-elle jointe :

## La question unique
- Que cherchez-vous à savoir ?

## Contraintes
- Production ou copie :
- Limites de débit, ou interdiction de scan automatisé :
- Ce qui ne doit jamais être tenté (destruction de données, déni de service,
  ingénierie sociale du personnel) :
- Contact d'urgence pendant la fenêtre, joignable en quelques minutes :

## Divulgation
- Délai de correction avant toute description publique :
- Un compte rendu expurgé est-il autorisé ensuite : oui / non
- Si une faille touche un composant tiers, qui la remonte en amont :

## Conditions
- Format de livrable :
- Échéance :
- NDA : oui / non

## Condition de refus
Règles d'engagement non signées, propriété d'un système floue, ou liste
hors-périmètre inexistante. Refusez en disant pourquoi. C'est le seul brief où
commencer quand même n'est pas un risque professionnel mais juridique.
$md$, 410),

('brief-qa-design', 'brief_template', 'quality', 'usability', 'en',
 'Brief — usability study or accessibility audit',
 'Filled in before commissioning research or an audit. Participants are people, and the brief says so.',
$md$
# Brief — usability study or accessibility audit

## Which of the two
- [ ] Usability study (people use it and we watch)
- [ ] Accessibility audit (it is measured against a standard)

*They are different jobs. An audit finds what a checker finds; a study finds
what a checker cannot. Asking for both in one engagement gets a shallow
version of each.*

## The system
- What, and which version:
- The specific flows in scope:
- Explicitly out of scope:

## The one question
- What decision will this change?

*A study whose findings change nothing was research theatre. Name the decision
that is waiting on it.*

## For a study — participants
- Who are they, and does the client recruit them or the contributor:
- How many, and is that number honest for the claim you want to make:
- Compensation, and who pays it:
- Consent and recording: what is recorded, who sees it, when it is deleted:
- Accessibility of the session itself (assistive technology, language,
  time zone):

## For an audit — the standard
- Which standard and level (for example WCAG 2.2 AA):
- Assistive technologies to be covered:
- Is a re-test after fixes included:

## Terms
- Deliverable format:
- Deadline:
- NDA: yes / no

## Refusal condition
No consent process for a study involving recordings, or unpaid participants
where the client's own staff would be paid for the same hour.
$md$, 420),

('brief-qa-design', 'brief_template', 'quality', 'usability', 'fr',
 'Brief — étude d''utilisabilité ou audit d''accessibilité',
 'À remplir avant de commander une étude ou un audit. Les participants sont des personnes, et le brief le dit.',
$md$
# Brief — étude d'utilisabilité ou audit d'accessibilité

## Lequel des deux
- [ ] Étude d'utilisabilité (des gens l'utilisent et on observe)
- [ ] Audit d'accessibilité (on mesure contre une norme)

*Ce sont deux métiers. Un audit trouve ce qu'un vérificateur trouve ; une
étude trouve ce qu'un vérificateur ne peut pas voir. Demander les deux en une
mission donne une version superficielle de chacun.*

## Le système
- Quoi, et quelle version :
- Les parcours précis dans le périmètre :
- Explicitement hors périmètre :

## La question unique
- Quelle décision cela va-t-il changer ?

*Une étude dont les résultats ne changent rien était du théâtre. Nommez la
décision qui l'attend.*

## Pour une étude — les participants
- Qui sont-ils, et qui recrute, le client ou le contributeur :
- Combien, et ce nombre est-il honnête pour ce que vous voudrez affirmer :
- Indemnisation, et qui la paie :
- Consentement et enregistrement : ce qui est enregistré, qui le voit, quand
  c'est supprimé :
- Accessibilité de la séance elle-même (technologie d'assistance, langue,
  fuseau horaire) :

## Pour un audit — la norme
- Quelle norme et quel niveau (par exemple WCAG 2.2 AA) :
- Technologies d'assistance à couvrir :
- Un nouveau test après corrections est-il inclus :

## Conditions
- Format de livrable :
- Échéance :
- NDA : oui / non

## Condition de refus
Aucun processus de consentement pour une étude avec enregistrement, ou des
participants non indemnisés là où le personnel du client serait payé pour la
même heure.
$md$, 420),

('brief-qa-game', 'brief_template', 'quality', 'playtest', 'en',
 'Brief — playtest facilitation',
 'Filled in before commissioning a playtest. What you want to learn decides everything else.',
$md$
# Brief — playtest facilitation

## The build
- Which build, and what is knowingly broken in it:
- Platform and hardware the session runs on:
- Session length, and where it starts (a fresh save, a specific level):

## The one question
- What are you trying to learn?

*"Is it fun" is not a question a playtest can answer. "Do players understand
what to do in the first ninety seconds" is.*

## Players
- Who: existing players, newcomers, or a mix, and why that mix:
- How many sessions:
- Recruited by whom:
- Compensation, and who pays it:
- Consent: what is recorded, who watches it, when it is deleted:

## What is not in scope
- Balance, difficulty tuning, or bug hunting — say which of these the session
  is *not* about, because a session that tries to do all four does none:

## Handling what comes back
- Who receives the findings:
- Is the facilitator expected to prioritise them, or only to report:

*Facilitation and prioritisation are separate skills and separate fees. A
facilitator asked to prioritise at the end usually does it badly and for
free.*

## Terms
- Deliverable format:
- Deadline:
- NDA: yes / no
- May players speak publicly about the build: yes / no

## Refusal condition
No consent process, or a build so broken that the session measures the bugs
rather than the design.
$md$, 430),

('brief-qa-game', 'brief_template', 'quality', 'playtest', 'fr',
 'Brief — animation de playtest',
 'À remplir avant de commander un playtest. Ce que vous voulez apprendre détermine tout le reste.',
$md$
# Brief — animation de playtest

## La build
- Quelle build, et ce qui y est cassé en connaissance de cause :
- Plateforme et matériel de la séance :
- Durée de la séance, et point de départ (nouvelle partie, niveau précis) :

## La question unique
- Qu'essayez-vous d'apprendre ?

*« Est-ce que c'est amusant » n'est pas une question à laquelle un playtest
répond. « Les joueurs comprennent-ils quoi faire dans les quatre-vingt-dix
premières secondes » en est une.*

## Les joueurs
- Qui : joueurs existants, nouveaux venus, ou mélange, et pourquoi ce mélange :
- Combien de séances :
- Recrutés par qui :
- Indemnisation, et qui la paie :
- Consentement : ce qui est enregistré, qui le regarde, quand c'est supprimé :

## Ce qui est hors périmètre
- Équilibrage, réglage de difficulté ou chasse aux bugs — dites lesquels la
  séance ne traite **pas**, car une séance qui vise les quatre n'en fait aucun :

## Ce qu'on fait des retours
- Qui reçoit les constats :
- L'animateur doit-il les prioriser, ou seulement les rapporter :

*Animer et prioriser sont deux compétences et deux honoraires. Un animateur à
qui on demande de prioriser à la fin le fait mal et gratuitement.*

## Conditions
- Format de livrable :
- Échéance :
- NDA : oui / non
- Les joueurs peuvent-ils parler publiquement de la build : oui / non

## Condition de refus
Aucun processus de consentement, ou une build si cassée que la séance mesure
les bugs plutôt que le design.
$md$, 430),

('brief-qa-lead', 'brief_template', 'quality', 'strategy', 'en',
 'Brief — quality strategy or initiative',
 'Filled in before commissioning strategy work. The honest version names what is already known.',
$md$
# Brief — quality strategy or initiative

## The situation
- What is happening that made you ask for this:
- What has already been tried, and what happened:
- What you already believe the answer is:

*The last one matters. A strategy engagement where the client has a conclusion
and does not say it produces a document arguing for something nobody will
do.*

## The one question
- What decision is waiting on this?

## Constraints that are not negotiable
- Budget:
- Headcount, current and possible:
- Tooling the organisation will not change:
- Deadlines outside your control:

*A strategy that ignores a constraint is a strategy that gets shelved. Name
them now rather than in the review.*

## Access
- Who may the contributor talk to, and for how long each:
- What data is available (defect history, incident records, pipeline metrics):
- Who must approve the result, and have they agreed to read it:

## Scope of the deliverable
- [ ] Assessment only · [ ] Assessment plus a plan · [ ] Plan plus help
  starting it
- If a plan: over what horizon:

## Terms
- Deliverable format:
- Deadline:
- NDA: yes / no
- May the contributor describe the engagement in the abstract: yes / no

## Refusal condition
No access to the people doing the work, or an approver who has not agreed to
read the result.
$md$, 440),

('brief-qa-lead', 'brief_template', 'quality', 'strategy', 'fr',
 'Brief — stratégie ou initiative qualité',
 'À remplir avant de commander un travail de stratégie. La version honnête dit ce qui est déjà su.',
$md$
# Brief — stratégie ou initiative qualité

## La situation
- Ce qui se passe et qui vous fait demander ça :
- Ce qui a déjà été tenté, et ce que ça a donné :
- Ce que vous pensez déjà être la réponse :

*Le dernier point compte. Une mission de stratégie où le client a une
conclusion et ne la dit pas produit un document qui plaide pour quelque chose
que personne ne fera.*

## La question unique
- Quelle décision attend ce travail ?

## Contraintes non négociables
- Budget :
- Effectif, actuel et possible :
- Outillage que l'organisation ne changera pas :
- Échéances hors de votre contrôle :

*Une stratégie qui ignore une contrainte est une stratégie qu'on range dans un
tiroir. Nommez-les maintenant plutôt qu'en réunion de restitution.*

## Accès
- À qui le contributeur peut-il parler, et combien de temps à chacun :
- Quelles données sont disponibles (historique de défauts, incidents, métriques
  de pipeline) :
- Qui doit valider le résultat, et cette personne a-t-elle accepté de le lire :

## Périmètre du livrable
- [ ] Diagnostic seul · [ ] Diagnostic et plan · [ ] Plan et aide au démarrage
- Si plan : sur quel horizon :

## Conditions
- Format de livrable :
- Échéance :
- NDA : oui / non
- Le contributeur peut-il décrire la mission de façon abstraite : oui / non

## Condition de refus
Aucun accès aux personnes qui font le travail, ou un validateur qui n'a pas
accepté de lire le résultat.
$md$, 440),

-- ═══════════════════════════════════════════════════════════════════
-- Leadership — one per trade
-- ═══════════════════════════════════════════════════════════════════

('brief-roadmap-quarterly', 'brief_template', 'leadership', 'delivery', 'en',
 'Brief — product or delivery direction',
 'Filled in before commissioning roadmap work. Every roadmap is a list of refusals.',
$md$
# Brief — product or delivery direction

## The horizon
- Over what period:
- What is already committed and cannot move:

## The one question
- What are you deciding, and by when:

## What you are choosing between
- The options as you see them today:
- What each would cost:

*A roadmap is a list of refusals. If nothing is being refused, the exercise is
a schedule, not a direction, and should be priced as one.*

## Constraints
- Team size and shape:
- Budget:
- Dependencies on other teams:
- Contractual or regulatory dates:

## Who has to agree
- Who signs off:
- Who has a veto, formally or in practice:
- Have they agreed to be interviewed:

*The second one is where these engagements die. Name the person who can quietly
refuse to implement it.*

## What is already known
- What has failed before:
- What you believe the answer is:

## Terms
- Deliverable format:
- Deadline:
- Confidentiality: may the artefact be shown publicly, anonymised, or not at
  all:
$md$, 400),

('brief-roadmap-quarterly', 'brief_template', 'leadership', 'delivery', 'fr',
 'Brief — direction produit ou delivery',
 'À remplir avant de commander un travail de roadmap. Toute roadmap est une liste de refus.',
$md$
# Brief — direction produit ou delivery

## L'horizon
- Sur quelle période :
- Ce qui est déjà engagé et ne peut pas bouger :

## La question unique
- Que décidez-vous, et pour quand :

## Entre quoi vous choisissez
- Les options telles que vous les voyez aujourd'hui :
- Ce que chacune coûterait :

*Une roadmap est une liste de refus. Si rien n'est refusé, l'exercice est un
planning et non une direction, et doit être facturé comme tel.*

## Contraintes
- Taille et forme de l'équipe :
- Budget :
- Dépendances vers d'autres équipes :
- Dates contractuelles ou réglementaires :

## Qui doit être d'accord
- Qui valide :
- Qui a un veto, formel ou de fait :
- Ces personnes ont-elles accepté d'être interrogées :

*Le deuxième point est là où ces missions meurent. Nommez la personne qui peut
discrètement refuser de mettre en œuvre.*

## Ce qui est déjà su
- Ce qui a échoué avant :
- Ce que vous croyez être la réponse :

## Conditions
- Format de livrable :
- Échéance :
- Confidentialité : l'artefact peut-il être montré publiquement, anonymisé, ou
  pas du tout :
$md$, 400),

('brief-tech-rfc', 'brief_template', 'leadership', 'technical', 'en',
 'Brief — a technical decision',
 'Filled in before commissioning an RFC or architecture decision.',
$md$
# Brief — a technical decision

## The decision
- What is being decided:
- What happens if it is not decided:
- The deadline, and what forces it:

## Constraints
- What cannot change (a language, a vendor, a data location):
- Team's existing skills:
- Operational budget:

## Options already on the table
- Named, with who is advocating each:

*Naming the advocates is not politics, it is the fastest route to the strongest
version of each option.*

## What would settle it
- What evidence would change your mind:
- Is a prototype in scope, or is this document-only:

## Who has to agree
- Who approves:
- Who implements, and have they been asked:

## Reversibility
- If this turns out wrong in a year, what does undoing it cost:

*A cheap-to-reverse decision deserves a short document and a fast answer. Say
which kind this is.*

## Terms
- Deliverable format:
- Deadline:
- Confidentiality:
$md$, 410),

('brief-tech-rfc', 'brief_template', 'leadership', 'technical', 'fr',
 'Brief — une décision technique',
 'À remplir avant de commander une RFC ou une décision d''architecture.',
$md$
# Brief — une décision technique

## La décision
- Ce qui est décidé :
- Ce qui se passe si ce n'est pas décidé :
- L'échéance, et ce qui l'impose :

## Contraintes
- Ce qui ne peut pas changer (un langage, un fournisseur, une localisation de
  données) :
- Compétences existantes de l'équipe :
- Budget d'exploitation :

## Options déjà sur la table
- Nommées, avec qui défend chacune :

*Nommer les défenseurs n'est pas de la politique, c'est le chemin le plus court
vers la meilleure version de chaque option.*

## Ce qui trancherait
- Quelle preuve vous ferait changer d'avis :
- Un prototype est-il dans le périmètre, ou est-ce documentaire uniquement :

## Qui doit être d'accord
- Qui valide :
- Qui met en œuvre, et cette personne a-t-elle été consultée :

## Réversibilité
- Si c'est une erreur dans un an, que coûte le retour en arrière :

*Une décision facile à défaire mérite un document court et une réponse rapide.
Dites de quel type il s'agit.*

## Conditions
- Format de livrable :
- Échéance :
- Confidentialité :
$md$, 410),

('brief-project-delivery-plan', 'brief_template', 'leadership', 'delivery', 'en',
 'Brief — delivery, usually recovery',
 'Filled in before commissioning delivery work. Most of these are recoveries, and saying so helps.',
$md$
# Brief — delivery, usually recovery

## The state of it
- What was supposed to happen, and by when:
- Where it actually is:
- When you knew it was late:

*The gap between the last two is the most useful number in this brief.*

## The one question
- Are you asking for a plan to finish, or a judgement about whether to
  continue:

*These are different engagements. Asking for the first while wanting the second
wastes everybody's month.*

## Constraints
- Fixed date, and what makes it fixed:
- Team, and whether it can change:
- Budget remaining:
- What has already been promised externally:

## Access
- Who may the contributor talk to, individually and without a manager present:
- What is in the tracker, and how much of the real state is not:

## Authority
- What may the contributor change directly:
- What must they recommend and wait for:

## What has been tried
- Previous attempts to recover this, and why they did not work:

## Terms
- Deliverable format:
- Deadline:
- Confidentiality, and specifically whether the team knows this engagement
  exists:

*If they do not, say so here. A contributor who finds out from a team member is
already compromised.*
$md$, 420),

('brief-project-delivery-plan', 'brief_template', 'leadership', 'delivery', 'fr',
 'Brief — delivery, le plus souvent un redressement',
 'À remplir avant de commander un travail de delivery. La plupart sont des redressements, et le dire aide.',
$md$
# Brief — delivery, le plus souvent un redressement

## L'état des lieux
- Ce qui devait arriver, et pour quand :
- Où c'en est réellement :
- Quand vous avez su que c'était en retard :

*L'écart entre ces deux derniers points est le chiffre le plus utile de ce
brief.*

## La question unique
- Demandez-vous un plan pour finir, ou un jugement sur l'opportunité de
  continuer :

*Ce sont deux missions différentes. Demander la première en voulant la seconde
gâche le mois de tout le monde.*

## Contraintes
- Date ferme, et ce qui la rend ferme :
- Équipe, et si elle peut changer :
- Budget restant :
- Ce qui a déjà été promis à l'extérieur :

## Accès
- À qui le contributeur peut-il parler, individuellement et sans manager
  présent :
- Ce qui est dans l'outil de suivi, et quelle part de l'état réel n'y est pas :

## Autorité
- Ce que le contributeur peut changer directement :
- Ce qu'il doit recommander en attendant une décision :

## Ce qui a été tenté
- Tentatives précédentes de redressement, et pourquoi elles ont échoué :

## Conditions
- Format de livrable :
- Échéance :
- Confidentialité, et précisément : l'équipe sait-elle que cette mission
  existe :

*Si elle ne le sait pas, dites-le ici. Un contributeur qui l'apprend par un
membre de l'équipe est déjà grillé.*
$md$, 420),

('brief-team-health-audit', 'brief_template', 'leadership', 'people', 'en',
 'Brief — people',
 'Filled in before commissioning work on a team. Confidentiality here is not a formality.',
$md$
# Brief — people

## What prompted this
- What you have observed:
- What you have already been told, and by whom:

## The one question
- What are you trying to understand:

## Confidentiality — read this before filling anything else in

People will say things to an outsider that they will not say to you. That only
happens if they believe it will not come back to them, and it only stays true
if it does not.

- Will the contributor's notes be shared with you: yes / no
- Will findings be attributed to individuals: yes / no
- What happens to the raw material after the engagement:
- Who else in the organisation will see the result:

*If findings are attributed, say so up front and expect a shallower
engagement. That is a legitimate choice; discovering it afterwards is not.*

## Scope
- Which team or teams, and how many people:
- Interviews, observation, survey, or a combination:
- Is anybody excluded, and why:

## Constraints
- What you already know you will not change (a manager, a structure, a
  location policy):

*A finding about something that cannot change wastes the engagement and the
trust of whoever raised it.*

## Terms
- Deliverable format:
- Deadline:
- May the existence of the engagement be public inside the organisation:
$md$, 430),

('brief-team-health-audit', 'brief_template', 'leadership', 'people', 'fr',
 'Brief — humain',
 'À remplir avant de commander un travail sur une équipe. La confidentialité n''est pas une formalité ici.',
$md$
# Brief — humain

## Ce qui déclenche la demande
- Ce que vous avez observé :
- Ce qu'on vous a déjà dit, et qui :

## La question unique
- Que cherchez-vous à comprendre :

## Confidentialité — à lire avant de remplir le reste

Les gens disent à un intervenant extérieur ce qu'ils ne vous diront pas. Cela
n'arrive que s'ils croient que ça ne leur reviendra pas dessus, et ça ne reste
vrai que si ça ne leur revient pas dessus.

- Les notes du contributeur vous seront-elles transmises : oui / non
- Les constats seront-ils attribués à des personnes : oui / non
- Que devient la matière brute après la mission :
- Qui d'autre dans l'organisation verra le résultat :

*Si les constats sont attribués, dites-le d'emblée et attendez-vous à une
mission plus superficielle. C'est un choix légitime ; le découvrir après ne
l'est pas.*

## Périmètre
- Quelle équipe ou équipes, et combien de personnes :
- Entretiens, observation, questionnaire, ou combinaison :
- Quelqu'un est-il exclu, et pourquoi :

## Contraintes
- Ce que vous savez déjà ne pas vouloir changer (un manager, une structure, une
  politique de présence) :

*Un constat portant sur ce qui ne peut pas changer gâche la mission et la
confiance de celui qui l'a soulevé.*

## Conditions
- Format de livrable :
- Échéance :
- L'existence de la mission peut-elle être publique en interne :
$md$, 430),

('brief-community-strategy', 'brief_template', 'leadership', 'community', 'en',
 'Brief — community',
 'Filled in before commissioning community work. Growth is not a goal, it is a side effect.',
$md$
# Brief — community

## What exists today
- Where the community is (platform, size, activity):
- Who runs it now, and how much of their time it takes:
- What has been tried:

## The one question
- What do you want to be true in a year that is not true now:

*"More members" is a metric, not an answer. A community that doubles and
answers nobody's questions got worse.*

## Who it is for
- Who belongs here, and who does not:

*The second half is the one that gets skipped and the one that decides
everything: a space for everybody is a space nobody recognises.*

## Constraints
- Budget, including whether anybody is paid:
- Moderation capacity available:
- What the organisation will not allow to be discussed publicly:

## Health, measured honestly
- What would tell you it is working:
- What would tell you it is failing:

*Name both. A brief with only the first produces a report with only the
first.*

## Terms
- Deliverable format:
- Deadline:
- May the contributor speak publicly about the work: yes / no
$md$, 440),

('brief-community-strategy', 'brief_template', 'leadership', 'community', 'fr',
 'Brief — communauté',
 'À remplir avant de commander un travail de communauté. La croissance n''est pas un but, c''est un effet.',
$md$
# Brief — communauté

## L'existant
- Où est la communauté (plateforme, taille, activité) :
- Qui l'anime aujourd'hui, et combien de temps ça lui prend :
- Ce qui a été tenté :

## La question unique
- Que voulez-vous voir vrai dans un an et qui ne l'est pas aujourd'hui :

*« Plus de membres » est une métrique, pas une réponse. Une communauté qui
double et ne répond à personne s'est dégradée.*

## Pour qui
- Qui a sa place ici, et qui n'en a pas :

*La seconde moitié est celle qu'on saute et celle qui décide de tout : un
espace pour tout le monde est un espace où personne ne se reconnaît.*

## Contraintes
- Budget, y compris si quelqu'un est rémunéré :
- Capacité de modération disponible :
- Ce que l'organisation n'autorise pas à discuter publiquement :

## Santé, mesurée honnêtement
- Ce qui vous dirait que ça marche :
- Ce qui vous dirait que ça échoue :

*Nommez les deux. Un brief qui n'a que le premier produit un rapport qui n'a
que le premier.*

## Conditions
- Format de livrable :
- Échéance :
- Le contributeur peut-il parler publiquement du travail : oui / non
$md$, 440),

('brief-mentoring-cohort', 'brief_template', 'leadership', 'teaching', 'en',
 'Brief — mentoring and curriculum',
 'Filled in before commissioning a cohort. Who is not ready is part of the brief.',
$md$
# Brief — mentoring and curriculum

## The cohort
- How many people, and how were they chosen:
- Where they are starting from, honestly:
- Is participation voluntary:

*If it is not, say so. A mandatory cohort is a different job and the first
session is about that, whether or not anybody planned it.*

## The one question
- What should they be able to do at the end that they cannot do now:

## Shape
- Over what period:
- How many hours of theirs, per week, and has their manager agreed:
- Group sessions, one-to-one, or both:

## What counts as finished
- How will you know it worked:
- Is there an assessment, and who sees the result:

*If the result reaches a manager, everybody in the cohort knows it and behaves
accordingly. That is a legitimate design and it has to be declared.*

## Who is not ready
- Is anybody in the cohort who should not be:

*The most useful thing a client can say in this brief. A person placed in a
cohort they are not ready for spends it hiding, and the cohort spends it
waiting.*

## Constraints
- Budget:
- What materials already exist and may be reused:
- Licence of what the contributor produces:

## Terms
- Deliverable format:
- Deadline:
- Confidentiality:
$md$, 450),

('brief-mentoring-cohort', 'brief_template', 'leadership', 'teaching', 'fr',
 'Brief — mentorat et parcours',
 'À remplir avant de commander une cohorte. Qui n''est pas prêt fait partie du brief.',
$md$
# Brief — mentorat et parcours

## La cohorte
- Combien de personnes, et comment ont-elles été choisies :
- D'où elles partent, honnêtement :
- La participation est-elle volontaire :

*Si elle ne l'est pas, dites-le. Une cohorte obligatoire est un autre métier et
la première séance porte là-dessus, que ce soit prévu ou non.*

## La question unique
- Que devront-ils savoir faire à la fin qu'ils ne savent pas faire maintenant :

## Forme
- Sur quelle période :
- Combien d'heures de leur temps par semaine, et leur manager est-il d'accord :
- Séances collectives, individuelles, ou les deux :

## Ce qui compte comme terminé
- Comment saurez-vous que ça a marché :
- Y a-t-il une évaluation, et qui en voit le résultat :

*Si le résultat remonte à un manager, toute la cohorte le sait et se comporte
en conséquence. C'est un choix légitime et il doit être déclaré.*

## Qui n'est pas prêt
- Y a-t-il dans la cohorte quelqu'un qui ne devrait pas y être :

*La chose la plus utile qu'un client puisse dire dans ce brief. Une personne
placée dans une cohorte pour laquelle elle n'est pas prête la passe à se
cacher, et la cohorte la passe à attendre.*

## Contraintes
- Budget :
- Quels supports existent déjà et peuvent être réutilisés :
- Licence de ce que produit le contributeur :

## Conditions
- Format de livrable :
- Échéance :
- Confidentialité :
$md$, 450);
