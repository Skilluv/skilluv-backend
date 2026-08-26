-- The templates: five things this domain writes, and five briefs it sets.
--
-- ## Why a template rather than a checklist in a guide
--
-- Because the editor loads it. A guide is read once; a template is the thing
-- somebody is typing into, and the headings it does not have are the sections
-- that do not get written. The reproduction section of a finding report is the
-- clearest case: a report without one is refused, and the difference between a
-- reporter who writes one and a reporter who does not is usually that
-- somebody put the heading there.
--
-- ## The two lines that appear in every template here
--
-- What was *not* done, and what is *not* certain. Both are what a reviewer
-- needs and neither is what somebody writing about their own work volunteers.
-- Putting them in the template is the only reliable way to get them.

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

-- ═══════════════════════════════════════════════════════════════════
-- Write-up templates
-- ═══════════════════════════════════════════════════════════════════

('security-template-finding', 'writeup_template', 'security', 'red-team', 'en',
 'Vulnerability report',
 'What a reviewer needs in order to reproduce a finding and rate it.',
$md$
# {one line: what an attacker can do, and where}

## Target and authorisation
| | |
|---|---|
| Target | {host, application or repository} |
| Authorised by | {link to the published scope or the rules of engagement} |
| Tested between | {first and last request} |

> A finding outside the authorisation above is refused however real it is.

## Summary
{two sentences. What the defect is, and what it lets somebody do.}

## Reproduction
1.
2.
3.

> Written for somebody who has never seen this system. Requests in full, with
> the payload — not a screenshot of a payload. If a step assumes something you
> know, it is not reproducible; it looks reproducible.

## Proof
{what the response shows that proves the claim: the data that should not have
been readable, the action that should not have been permitted. An error page is
not proof of anything.}

## Impact
{what an attacker gets, on this system. Not the worst thing this class of
defect has caused elsewhere.}

## Severity
`CVSS:3.1/AV:_/AC:_/PR:_/UI:_/S:_/C:_/I:_/A:_`

{one sentence per metric that is not obvious. The vector is what makes the
score arguable; a bare number is a claim.}

## Root cause
{which check is missing, and where. Not which request returns the wrong thing.}

## Proposed fix
{the concrete change, at the layer that closes the class rather than the
instance.}

## What I did not do
{where you stopped, what you did not touch, what you believed was on the other
side. Data taken and then deleted, if any.}

## What is not certain
{what you could not confirm, and what your conclusion depends on.}
$md$, 10),

('security-template-finding', 'writeup_template', 'security', 'red-team', 'fr',
 'Rapport de vulnérabilité',
 'Ce dont un relecteur a besoin pour reproduire une découverte et la coter.',
$md$
# {une ligne : ce qu'un attaquant peut faire, et où}

## Cible et autorisation
| | |
|---|---|
| Cible | {hôte, application ou dépôt} |
| Autorisé par | {lien vers le périmètre publié ou les règles d'engagement} |
| Testé entre | {première et dernière requête} |

> Une découverte hors de l'autorisation ci-dessus est refusée, aussi réelle
> soit-elle.

## Résumé
{deux phrases. Quel est le défaut, et ce qu'il permet de faire.}

## Reproduction
1.
2.
3.

> Écrit pour quelqu'un qui n'a jamais vu ce système. Les requêtes complètes,
> avec la charge utile — pas une capture d'écran d'une charge utile. Si une
> étape suppose quelque chose que vous savez, ce n'est pas reproductible :
> cela y ressemble.

## Preuve
{ce que la réponse montre et qui établit l'affirmation : la donnée qui ne
devait pas être lisible, l'action qui ne devait pas être permise. Une page
d'erreur ne prouve rien.}

## Impact
{ce que l'attaquant obtient, sur ce système. Pas le pire que cette classe de
défaut ait causé ailleurs.}

## Gravité
`CVSS:3.1/AV:_/AC:_/PR:_/UI:_/S:_/C:_/I:_/A:_`

{une phrase par métrique non évidente. Le vecteur est ce qui rend le score
discutable ; un chiffre seul est une affirmation.}

## Cause racine
{quelle vérification manque, et où. Pas quelle requête renvoie la mauvaise
chose.}

## Correctif proposé
{le changement concret, à la couche qui ferme la classe et non l'instance.}

## Ce que je n'ai pas fait
{où vous vous êtes arrêté, ce que vous n'avez pas touché, ce que vous pensiez
trouver derrière. Les données extraites puis supprimées, s'il y en a.}

## Ce qui n'est pas certain
{ce que vous n'avez pas pu confirmer, et de quoi dépend votre conclusion.}
$md$, 10),

('security-template-disclosure', 'writeup_template', 'security', 'red-team', 'en',
 'Public disclosure write-up',
 'The version a stranger reads after the embargo — written to teach, not to impress.',
$md$
---
finding: {internal reference}
severity: {critical | high | medium | low | informational}
cwe: CWE-{nn}
reported: {date}
fixed: {date}
disclosed: {date}
credit: {name or alias}
---

# {title}

## In two sentences
{what it was and what it allowed. Somebody skimming reads only this.}

## The system
{what the affected component does, and why it is there. A reader who does not
know the product needs this paragraph to follow the rest.}

## The defect
{the technical explanation, with the code or the request before the fix.}

## How it was found
{the honest account, including the wrong turns. This is the part that teaches,
and the part every published write-up leaves out.}

## Impact
{what could have happened. Stated in the past conditional, because it did not.}

## The fix
{the change, with a link to it. What it closes, and what it does not.}

## What would have prevented it earlier
{a lint rule, a review question, a test. The generalisation is the whole
reason to publish.}

## Timeline
| | |
|---|---|
| Reported | |
| Triaged | |
| Confirmed | |
| Fixed | |
| Disclosed | |

## Credit
{who found it, and a sentence about what was good about the report.}
$md$, 20),

('security-template-disclosure', 'writeup_template', 'security', 'red-team', 'fr',
 'Publication d''une divulgation',
 'La version qu''un inconnu lit après l''embargo — écrite pour enseigner, pas pour impressionner.',
$md$
---
finding: {référence interne}
severity: {critical | high | medium | low | informational}
cwe: CWE-{nn}
reported: {date}
fixed: {date}
disclosed: {date}
credit: {nom ou alias}
---

# {titre}

## En deux phrases
{ce que c'était et ce que cela permettait. Celui qui survole ne lit que ça.}

## Le système
{ce que fait le composant concerné, et pourquoi il existe. Un lecteur qui ne
connaît pas le produit a besoin de ce paragraphe pour suivre le reste.}

## Le défaut
{l'explication technique, avec le code ou la requête d'avant le correctif.}

## Comment il a été trouvé
{le récit honnête, fausses pistes comprises. C'est la partie qui enseigne, et
celle que toutes les publications omettent.}

## Impact
{ce qui aurait pu arriver. Au conditionnel passé, parce que ce n'est pas
arrivé.}

## Le correctif
{le changement, avec un lien. Ce qu'il ferme, et ce qu'il ne ferme pas.}

## Ce qui l'aurait évité plus tôt
{une règle de lint, une question de revue, un test. La généralisation est toute
la raison de publier.}

## Chronologie
| | |
|---|---|
| Signalé | |
| Trié | |
| Confirmé | |
| Corrigé | |
| Divulgué | |

## Crédit
{qui l'a trouvé, et une phrase sur ce qui était bon dans le rapport.}
$md$, 20),

('security-template-analysis', 'writeup_template', 'security', 'blue-team', 'en',
 'Defensive analysis',
 'Reading an artefact to a conclusion, with observation kept apart from inference.',
$md$
# {one line: what happened, and to what}

## Material
| | |
|---|---|
| Artefacts | {capture, log set, memory image — with sizes} |
| Source | {where they came from, and their licence if published} |
| Time range | {first to last event, timezone stated once} |
| Tools | {and versions} |

## Timeline
| Time | Source | Event |
|---|---|---|
| | | |

> Every row points at something in the material. Clock skew between sources is
> called out here, not smoothed over.

## What was observed
{facts only, each with where it is visible. "The account authenticated from
203.0.113.9 at 03:14" belongs here.}

## What is inferred
{your reading, and what would disprove it. "That account is compromised"
belongs here, not above.}

## Indicators
{hashes, addresses, domains, patterns — in a form another team can search
their own estate for.}

## Detection
{the rule, in a named format, with the two results: it fires on this material,
and it stays quiet on {period} of ordinary activity. Both halves, or it is a
hypothesis.}

## Recommendations
{what to do now, and what would have caught this earlier. Ranked.}

## What is not certain
{what the material does not contain, and what you could not establish from it.}
$md$, 30),

('security-template-analysis', 'writeup_template', 'security', 'blue-team', 'fr',
 'Analyse défensive',
 'Lire un artefact jusqu''à une conclusion, en séparant l''observation de la déduction.',
$md$
# {une ligne : ce qui s'est passé, et sur quoi}

## Matériel
| | |
|---|---|
| Artefacts | {capture, journaux, image mémoire — avec les tailles} |
| Source | {d'où ils viennent, et leur licence s'ils sont publiés} |
| Plage temporelle | {du premier au dernier événement, fuseau annoncé une fois} |
| Outils | {et versions} |

## Chronologie
| Heure | Source | Événement |
|---|---|---|
| | | |

> Chaque ligne pointe vers quelque chose du matériel. Les décalages d'horloge
> entre sources sont signalés ici, pas lissés.

## Ce qui a été observé
{des faits seulement, chacun avec l'endroit où il est visible. « Le compte
s'est authentifié depuis 203.0.113.9 à 03 h 14 » va ici.}

## Ce qui en est déduit
{votre lecture, et ce qui l'infirmerait. « Ce compte est compromis » va ici,
pas au-dessus.}

## Indicateurs
{empreintes, adresses, domaines, motifs — sous une forme qu'une autre équipe
peut chercher dans son propre parc.}

## Détection
{la règle, dans un format nommé, avec les deux résultats : elle se déclenche
sur ce matériel, et elle reste silencieuse sur {période} d'activité ordinaire.
Les deux moitiés, sinon c'est une hypothèse.}

## Recommandations
{quoi faire maintenant, et ce qui aurait détecté plus tôt. Classé.}

## Ce qui n'est pas certain
{ce que le matériel ne contient pas, et ce que vous n'avez pas pu établir.}
$md$, 30),

('security-template-engagement', 'writeup_template', 'security', 'code-audit', 'en',
 'Engagement report',
 'What a client is handed at the end of a paid engagement: an executive page, then findings somebody can act on.',
$md$
# {client} — {engagement type}, {dates}

## Executive summary
{one page, for somebody who will not read the rest. What was tested, what was
found, what it means for the business, and the three things to do first. No
tool names, no CVSS.}

| | |
|---|---|
| Critical | |
| High | |
| Medium | |
| Low | |
| Informational | |

## Scope
{what was in, what was out, and what was in scope and not reached — with the
reason. An unstated gap reads as coverage.}

## Method
{what was done, in enough detail that another practitioner could repeat the
engagement. Named tools with versions, and what was done by hand.}

## Findings
{one section each, ordered by severity. Each one: title, severity with its
vector, affected component, reproduction, impact, remediation, and effort.}

## What was checked and found sound
{the classes tested that produced nothing. Without this section the reader
cannot tell a clean system from a shallow test.}

## What the tools said and was wrong
{dismissed scanner output, with reasons. Half of what makes a report
believable.}

## Remediation plan
| Finding | Severity | Effort | Owner | By |
|---|---|---|---|---|

## Limitations
{time, access, environment. What a longer engagement would have reached.}
$md$, 40),

('security-template-engagement', 'writeup_template', 'security', 'code-audit', 'fr',
 'Rapport de mission',
 'Ce qu''un client reçoit à la fin d''une mission payée : une page de synthèse, puis des découvertes exploitables.',
$md$
# {client} — {type de mission}, {dates}

## Synthèse
{une page, pour quelqu'un qui ne lira pas le reste. Ce qui a été testé, ce qui
a été trouvé, ce que cela signifie pour l'activité, et les trois premières
choses à faire. Pas de noms d'outils, pas de CVSS.}

| | |
|---|---|
| Critique | |
| Élevé | |
| Moyen | |
| Faible | |
| Informatif | |

## Périmètre
{ce qui était dedans, ce qui était dehors, et ce qui était dedans et n'a pas
été atteint — avec la raison. Un manque non énoncé se lit comme une
couverture.}

## Méthode
{ce qui a été fait, assez précisément pour qu'un autre praticien puisse
refaire la mission. Outils nommés avec versions, et ce qui a été fait à la
main.}

## Découvertes
{une section chacune, par gravité décroissante. Chacune : titre, gravité avec
son vecteur, composant concerné, reproduction, impact, remédiation, effort.}

## Ce qui a été vérifié et jugé sain
{les classes testées qui n'ont rien donné. Sans cette section, le lecteur ne
peut pas distinguer un système propre d'un test superficiel.}

## Ce que les outils ont dit et qui était faux
{les signalements écartés, avec les raisons. La moitié de ce qui rend un
rapport crédible.}

## Plan de remédiation
| Découverte | Gravité | Effort | Responsable | Échéance |
|---|---|---|---|---|

## Limites
{temps, accès, environnement. Ce qu'une mission plus longue aurait atteint.}
$md$, 40),

('security-template-threat-model', 'writeup_template', 'security', 'governance', 'en',
 'Threat model',
 'A system, the threats against it named in a shared vocabulary, and who owns each mitigation.',
$md$
# Threat model — {system}, {date}

## What this covers
{the system as modelled, and its boundary. What is out of scope and why.}

## The picture
{a diagram — Mermaid is fine — showing components, trust boundaries and where
data crosses them. The boundaries are the point.}

## What is worth protecting
| Asset | Why it matters | Where it lives |
|---|---|---|

## Who might attack it
{named, with what they can be assumed to have. "A state actor" and "somebody
with a stolen session cookie" produce different lists, and a model that skips
this produces the wrong one.}

## Threats
| # | Component | Threat | Category | Likelihood | Impact | Rank |
|---|---|---|---|---|---|---|

> One taxonomy, named — STRIDE or another. Consistency matters more than which.

## Mitigations in place
{per threat, what stops it today, and the evidence that it does.}

## Accepted risks
| Threat | Why not fixed | Accepted by | Review on |
|---|---|---|---|

> An unowned acceptance is how a finding survives three audits.

## What this model does not consider
{the assumptions, and what would change if they broke.}
$md$, 50),

('security-template-threat-model', 'writeup_template', 'security', 'governance', 'fr',
 'Modèle de menaces',
 'Un système, les menaces nommées dans un vocabulaire partagé, et un responsable par mesure.',
$md$
# Modèle de menaces — {système}, {date}

## Ce que cela couvre
{le système tel que modélisé, et sa frontière. Ce qui est hors périmètre et
pourquoi.}

## Le schéma
{un diagramme — Mermaid convient — avec les composants, les frontières de
confiance et les endroits où les données les traversent. Les frontières sont
l'essentiel.}

## Ce qui vaut d'être protégé
| Actif | Pourquoi il compte | Où il se trouve |
|---|---|---|

## Qui pourrait attaquer
{nommés, avec ce qu'on leur suppose. « Un acteur étatique » et « quelqu'un
avec un cookie de session volé » produisent des listes différentes, et un
modèle qui saute cette étape produit la mauvaise.}

## Menaces
| # | Composant | Menace | Catégorie | Vraisemblance | Impact | Rang |
|---|---|---|---|---|---|---|

> Une seule taxonomie, nommée — STRIDE ou une autre. La cohérence compte plus
> que le choix.

## Mesures en place
{par menace, ce qui l'empêche aujourd'hui, et la preuve que c'est le cas.}

## Risques acceptés
| Menace | Pourquoi non corrigé | Accepté par | Revue le |
|---|---|---|---|

> Une acceptation sans responsable est la façon dont une non-conformité survit
> à trois audits.

## Ce que ce modèle ne considère pas
{les hypothèses, et ce qui changerait si elles tombaient.}
$md$, 50),

-- ═══════════════════════════════════════════════════════════════════
-- Brief templates — what a curator writes when setting work
-- ═══════════════════════════════════════════════════════════════════

('brief-security-red-team', 'brief_template', 'security', 'red-team', 'en',
 'Brief — offensive work',
 'Setting a hunt or an exercise: the authorisation first, then what counts as done.',
$md$
## What there is to do
{one paragraph. What to attack, and what a result looks like.}

## Where, exactly
| | |
|---|---|
| Target | {host, application, or range} |
| Authorisation | {link to the scope or rules of engagement} |
| Out of scope | {named, not implied} |
| Window | {when testing may happen} |

## What is expected
{the deliverable. A report, a set of findings, a write-up — and the template
to use.}

## What will be looked at
The `red-team` review grid applies and it is public. In short: it replays, the
proof proves the claim, the severity is argued from a vector, and the scope was
respected under pressure.

## What ends this badly
- Anything outside the authorisation, however real.
- Denial of service of any kind, including load testing.
- Data taken beyond what proves the finding.
- A report nobody can follow.
$md$, 60),

('brief-security-red-team', 'brief_template', 'security', 'red-team', 'fr',
 'Brief — travail offensif',
 'Poser une chasse ou un exercice : l''autorisation d''abord, puis ce qui compte comme fait.',
$md$
## Ce qu'il y a à faire
{un paragraphe. Quoi attaquer, et à quoi ressemble un résultat.}

## Où, exactement
| | |
|---|---|
| Cible | {hôte, application ou plateforme d'entraînement} |
| Autorisation | {lien vers le périmètre ou les règles d'engagement} |
| Hors périmètre | {nommé, pas implicite} |
| Fenêtre | {quand les tests peuvent avoir lieu} |

## Ce qui est attendu
{le livrable. Un rapport, un ensemble de découvertes, une publication — et le
modèle à utiliser.}

## Ce qui sera regardé
La grille de relecture `red-team` s'applique et elle est publique. En bref :
ça se rejoue, la preuve établit l'affirmation, la gravité est argumentée
depuis un vecteur, et le périmètre a été respecté sous pression.

## Ce qui termine mal
- Tout ce qui est hors de l'autorisation, aussi réel soit-il.
- Tout déni de service, y compris un test de charge.
- Des données extraites au-delà de ce qui prouve la découverte.
- Un rapport que personne ne peut suivre.
$md$, 60),

('brief-security-blue-team', 'brief_template', 'security', 'blue-team', 'en',
 'Brief — defensive work',
 'Setting an analysis: the artefact, the questions, and the rule that has to fire.',
$md$
## What there is to do
{one paragraph. What the artefact contains and what to establish from it.}

## The material
| | |
|---|---|
| Artefacts | {what, and how large} |
| Source and licence | {who published it} |
| Handling | {isolated environment, if the material warrants it} |

## What is expected
- A timeline, every entry sourced.
- Observation kept apart from inference.
- A detection, with the evidence that it fires on this material **and** stays
  quiet on ordinary activity.

## What will be looked at
The `blue-team` review grid applies and it is public. The line that catches
most submissions: a rule tested only on the positive case is a hypothesis.

## What ends this badly
- A conclusion the artefact does not support.
- Credentials or personal data from the material reproduced in the write-up.
- Executing anything from the material outside an isolated environment.
$md$, 70),

('brief-security-blue-team', 'brief_template', 'security', 'blue-team', 'fr',
 'Brief — travail défensif',
 'Poser une analyse : l''artefact, les questions, et la règle qui doit se déclencher.',
$md$
## Ce qu'il y a à faire
{un paragraphe. Ce que contient l'artefact et ce qu'il faut établir.}

## Le matériel
| | |
|---|---|
| Artefacts | {quoi, et quelle taille} |
| Source et licence | {qui l'a publié} |
| Manipulation | {environnement isolé, si le matériel l'exige} |

## Ce qui est attendu
- Une chronologie, chaque entrée sourcée.
- L'observation séparée de la déduction.
- Une détection, avec la preuve qu'elle se déclenche sur ce matériel **et**
  qu'elle reste silencieuse sur une activité ordinaire.

## Ce qui sera regardé
La grille `blue-team` s'applique et elle est publique. La ligne qui arrête le
plus de rendus : une règle testée seulement sur le cas positif est une
hypothèse.

## Ce qui termine mal
- Une conclusion que l'artefact ne soutient pas.
- Des identifiants ou des données personnelles du matériel reproduits dans le
  rendu.
- Exécuter quoi que ce soit du matériel hors d'un environnement isolé.
$md$, 70),

('brief-security-code-audit', 'brief_template', 'security', 'code-audit', 'en',
 'Brief — code security review',
 'Setting an audit: the code, the commit, and the coverage that has to be stated.',
$md$
## What there is to do
{one paragraph. Which codebase, and what classes of defect to look for.}

## What exactly
| | |
|---|---|
| Repository | {url} |
| Commit | {sha — an audit of "main" is an audit of nothing in particular} |
| In scope | {directories, services} |
| Out of scope | {named} |

## What is expected
- Each finding with its path traced from entry point to sink, file and line.
- Reachability established, not assumed.
- A proposed fix at the layer that closes the class.
- **What you checked and found sound**, and the scanner output you dismissed
  with reasons.

## What will be looked at
The `code-audit` review grid applies and it is public. A finding that names a
sink without a reachable source is a scanner hit with a paragraph attached.

## What ends this badly
- A live credential published rather than reported privately.
- A version table pasted from a dependency scanner, presented as findings.
$md$, 80),

('brief-security-code-audit', 'brief_template', 'security', 'code-audit', 'fr',
 'Brief — revue de sécurité du code',
 'Poser un audit : le code, le commit, et la couverture qui doit être énoncée.',
$md$
## Ce qu'il y a à faire
{un paragraphe. Quelle base de code, et quelles classes de défaut chercher.}

## Quoi exactement
| | |
|---|---|
| Dépôt | {url} |
| Commit | {sha — auditer « main » est auditer rien en particulier} |
| Dans le périmètre | {répertoires, services} |
| Hors périmètre | {nommé} |

## Ce qui est attendu
- Chaque découverte avec son chemin tracé du point d'entrée au point
  d'arrivée, fichier et ligne.
- L'atteignabilité établie, pas supposée.
- Un correctif proposé à la couche qui ferme la classe.
- **Ce que vous avez vérifié et jugé sain**, et les signalements d'outil
  écartés avec leurs raisons.

## Ce qui sera regardé
La grille `code-audit` s'applique et elle est publique. Une découverte qui
nomme un point d'arrivée sans source atteignable est un signalement d'outil
avec un paragraphe autour.

## Ce qui termine mal
- Un identifiant actif publié au lieu d'être signalé en privé.
- Un tableau de versions collé depuis un scanner de dépendances, présenté
  comme des découvertes.
$md$, 80),

('brief-security-governance', 'brief_template', 'security', 'governance', 'en',
 'Brief — governance work',
 'Setting a document: the framework it answers to, and the evidence it will be judged on.',
$md$
## What there is to do
{one paragraph. Which document, for what organisation, against what.}

## Against what
| | |
|---|---|
| Framework | {GDPR articles, ISO 27001 controls, SOC 2 criteria} |
| Organisation | {size, sector, what it actually does} |
| Existing material | {what is already written, and where} |

## What is expected
- Each control or clause mapped to the requirement it answers.
- For each claim: what evidence an auditor would be shown, and whether it
  exists.
- Residual risk written down, with who accepts it and when it is reviewed.

## What will be looked at
The `governance` review grid applies and it is public. The line that fails most
submissions: a control nobody could comply with on an ordinary day.

## What ends this badly
- Real personal data, or a real organisation's confidential material, in the
  submission.
- A policy that cites no requirement — it cannot be audited against anything.
$md$, 90),

('brief-security-governance', 'brief_template', 'security', 'governance', 'fr',
 'Brief — travail de gouvernance',
 'Poser un document : le référentiel auquel il répond, et la preuve sur laquelle il sera jugé.',
$md$
## Ce qu'il y a à faire
{un paragraphe. Quel document, pour quelle organisation, contre quoi.}

## Contre quoi
| | |
|---|---|
| Référentiel | {articles du RGPD, contrôles ISO 27001, critères SOC 2} |
| Organisation | {taille, secteur, ce qu'elle fait réellement} |
| Matériel existant | {ce qui est déjà écrit, et où} |

## Ce qui est attendu
- Chaque contrôle ou clause rattaché à l'exigence auquel il répond.
- Pour chaque affirmation : quelle preuve un auditeur verrait, et si elle
  existe.
- Le risque résiduel écrit, avec qui l'accepte et quand il est revu.

## Ce qui sera regardé
La grille `governance` s'applique et elle est publique. La ligne qui recale le
plus de rendus : un contrôle avec lequel personne ne pourrait se conformer un
jour ordinaire.

## Ce qui termine mal
- De vraies données personnelles, ou du matériel confidentiel d'une vraie
  organisation, dans le rendu.
- Une politique qui ne cite aucune exigence — elle n'est auditable contre
  rien.
$md$, 90),

('brief-security-purple-team', 'brief_template', 'security', 'purple-team', 'en',
 'Brief — purple exercise',
 'Setting an exercise: the environment, the techniques, and the detection that has to exist afterwards.',
$md$
## What there is to do
{one paragraph. Which techniques, against what, and what the exercise is
trying to find out.}

## Where
| | |
|---|---|
| Environment | {a disposable one — nothing anybody depends on} |
| Techniques | {ATT&CK identifiers} |
| Window | {and the stop condition} |
| Cleanup | {who verifies it, and how} |

## What is expected
- Every technique named in a shared taxonomy.
- Each detection **validated by re-running the technique**, with both results
  shown.
- One timeline holding what the attack did and what the defence saw, including
  the steps nothing saw.
- Gaps ranked by what closing each would cost.

## What will be looked at
The `purple-team` review grid applies and it is public. An exercise whose
output is a slide deck has not finished.

## What ends this badly
- Running anything against an environment somebody depends on.
- Cleanup nobody verified — simulation tooling that leaves persistence behind
  has created a real incident.
$md$, 100),

('brief-security-purple-team', 'brief_template', 'security', 'purple-team', 'fr',
 'Brief — exercice purple',
 'Poser un exercice : l''environnement, les techniques, et la détection qui doit exister après.',
$md$
## Ce qu'il y a à faire
{un paragraphe. Quelles techniques, contre quoi, et ce que l'exercice cherche
à savoir.}

## Où
| | |
|---|---|
| Environnement | {jetable — rien dont quiconque dépend} |
| Techniques | {identifiants ATT&CK} |
| Fenêtre | {et la condition d'arrêt} |
| Nettoyage | {qui le vérifie, et comment} |

## Ce qui est attendu
- Chaque technique nommée dans une taxonomie partagée.
- Chaque détection **validée en réexécutant la technique**, avec les deux
  résultats montrés.
- Une seule chronologie portant ce que l'attaque a fait et ce que la défense a
  vu, y compris les étapes que rien n'a vues.
- Les angles morts classés par ce que coûterait de les fermer.

## Ce qui sera regardé
La grille `purple-team` s'applique et elle est publique. Un exercice dont la
sortie est un jeu de diapositives n'est pas terminé.

## Ce qui termine mal
- Exécuter quoi que ce soit contre un environnement dont quelqu'un dépend.
- Un nettoyage que personne n'a vérifié — un outil de simulation qui laisse
  une persistance a créé un vrai incident.
$md$, 100);
