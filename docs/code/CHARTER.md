# Charte du domaine Code

*Destinée à être publiée sur `skill-uv.com/code/charter`.*

Cette charte dit ce qui est exigé, ce qui est refusé, et sur quoi une
validation se fonde. Elle est contraignante : un livrable qui s'en écarte est
refusé, quelle que soit la qualité du code.

Elle existe parce qu'elle était jusqu'ici implicite, dispersée dans des
documents généraux. Une règle que personne ne peut citer n'est pas une règle.

---

## 1. Ce qu'est un livrable

Un livrable est un **artefact opposable** : quelque chose qu'un inconnu peut
ouvrir, exécuter et juger sans vous croire sur parole.

Sont recevables :

- une contribution acceptée dans un dépôt que vous ne contrôlez pas ;
- un projet publié, avec une adresse où il tourne ;
- une bibliothèque publiée sur un registre public ;
- un outil que d'autres utilisent.

Ne sont pas recevables :

- un exercice de tutoriel, même terminé ;
- un dépôt privé décrit mais non montrable ;
- une capture d'écran sans code derrière ;
- un projet dont vous êtes le seul relecteur possible.

La différence n'est pas la difficulté. C'est la vérifiabilité.

## 2. Trois exigences non négociables

**Des tests.** Ils décrivent le comportement attendu, pas l'implémentation. Un
test qui casse lors d'un remaniement sans changement de comportement est un
mauvais test et ne compte pas.

**De la documentation.** Un lecteur qui découvre le dépôt doit savoir quoi
lancer et pourquoi les choix ont été faits. **Un code sans documentation est
refusé** — c'est la règle la plus souvent sous-estimée et celle qui écarte le
plus de soumissions.

**Une licence.** Un travail sans licence explicite n'est utilisable par
personne. Choisissez-en une et respectez celles des dépendances : MIT, Apache
et GPL n'imposent pas les mêmes obligations, et les ignorer expose le projet
d'accueil autant que vous.

## 3. Éthique de contribution

**Attribution.** Le code repris est cité. Reprendre sans citer est un
plagiat, et le plagiat entraîne la révocation de l'artefact et des
attestations qui en dépendent.

**Respect du temps des mainteneurs.** Une contribution en amont se prépare :
lire les règles du projet, chercher si le sujet a déjà été traité, ouvrir une
discussion avant d'écrire mille lignes. Une proposition non sollicitée et non
préparée coûte du temps à quelqu'un qui n'a rien demandé.

**Hygiène des commits.** Un commit fait une chose et l'explique. Un historique
lisible est une forme de documentation.

## 4. Assistance par IA

L'usage d'un assistant est **accepté et déclaré**.

Accepté : ces outils font partie du métier, et prétendre le contraire
produirait des déclarations fausses plutôt que des pratiques différentes.

Déclaré : la soumission indique le niveau d'assistance — aucun, complétion,
programmation en binôme, généré puis remanié, généré tel quel. Le camoufler
est une faute distincte du fait de l'utiliser, et c'est celle-là qui est
sanctionnée.

Ce qui est jugé reste le résultat et votre capacité à en répondre. La
soutenance en temps réel — expliquer un choix, modifier le code devant un
relecteur — est ce qui départage, pas la déclaration elle-même.

## 5. Validation

Une validation s'appuie sur la **grille de revue de la famille de métier**
concernée, publique et consultable avant de soumettre. Elle porte sur des
critères vérifiables, chacun accompagné de ce qui compte comme satisfait.

Un refus dit lequel des critères n'est pas atteint et ce qui manque. Un refus
sans motif exploitable n'est pas un refus valable.

## 6. Révocation

Un artefact validé peut être révoqué : plagiat découvert, contribution
rétractée en amont, fraude établie.

La révocation retire l'artefact du décompte — rang, badges, attestations qui
s'appuyaient dessus. Elle n'efface pas l'historique : ce qui a été révoqué
reste visible comme tel.

---

*Voir aussi : le [manifeste du domaine](./MANIFESTO.md), qui dit pourquoi ces
règles sont celles-là.*
