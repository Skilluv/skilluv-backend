# Charte du domaine IA

*Destinée à être publiée sur `skill-uv.com/ai/charter`.*

Cette charte dit ce qui est exigé, ce qui est refusé, et sur quoi une
validation se fonde. Elle est contraignante : un livrable qui s'en écarte est
refusé, quel que soit le score annoncé.

---

## 1. Ce qu'est un livrable IA

Un livrable est un **artefact opposable** : quelque chose qu'un inconnu peut
ouvrir, exécuter et juger sans vous croire sur parole.

Sont recevables :

- un modèle publié à une adresse où on peut le télécharger et l'exécuter ;
- un jeu de données publié avec sa fiche ;
- un système d'agents en service, avec ses évaluations ;
- un article paru — préprint ou conférence — avec le code qui le soutient ;
- un résultat de banc public qu'un tiers a rejoué ;
- une trouvaille de sûreté reproduite et divulguée dans les règles.

Ne sont pas recevables :

- un carnet Jupyter qui reproduit un tutoriel ;
- un score annoncé sans jeu de test séparé ;
- une capture d'écran d'une courbe ;
- un modèle décrit mais non téléchargeable ;
- une invite « qui marche bien » sans évaluation derrière.

La différence n'est pas la difficulté. C'est la vérifiabilité.

## 2. Quatre exigences non négociables

**Une évaluation honnête.** Le jeu de test est séparé de l'entraînement et
ressemble à ce que le modèle verra vraiment. Le score annoncé est celui du jeu
de test, pas celui du meilleur essai. **La fuite de données est l'erreur la
plus fréquente du domaine et la moins visible** : c'est elle qui est cherchée
en premier.

**La reproductibilité.** Graines, versions de bibliothèques et données figées.
Un lecteur relance et retrouve les mêmes chiffres — ou l'écart attendu est
écrit avant qu'on le lui demande.

**La provenance des données.** D'où elles viennent, sous quelle licence, avec
quel consentement. Un jeu aspiré sans droit rend tout le reste inutilisable,
y compris pour l'entreprise qui reprendrait le travail.

**Les limites énoncées.** Ce sur quoi le travail échoue est écrit par son
auteur. Un travail qui ne connaît pas ses limites n'a pas été évalué : il a
été montré.

## 3. Une référence de comparaison

Un modèle sans témoin ne prouve rien. Chaque résultat se compare à quelque
chose : une prévision naïve, une régression logistique, le modèle de la
semaine dernière, un résultat publié.

C'est une exigence et une protection. La majorité des modèles complexes
n'améliorent pas la référence simple, et le découvrir soi-même vaut mieux que
l'apprendre en soutenance.

## 4. Éthique

**Attribution.** Le code, les poids et les données repris sont cités. Une
chaîne de licences se respecte entièrement : affiner un modèle sous licence
communautaire n'efface pas cette licence.

**Données personnelles.** Aucun jeu de données publié ne contient de données
personnelles sans base légale et sans consentement. Un modèle entraîné dessus
ne s'oublie pas : c'est pourquoi la question se pose avant, pas après.

**Sûreté avant publication.** Une capacité dangereuse ne se publie pas parce
qu'elle est impressionnante. Voir la [politique de
divulgation](./SAFETY-DISCLOSURE.md).

## 5. Assistance par IA

Dans un domaine dont l'objet *est* l'IA, la question se pose autrement : ce
qui est jugé n'est pas qui a tapé le code, mais si vous répondez de ce que le
système fait.

L'usage d'un assistant est **accepté et déclaré**. La soumission indique le
niveau d'assistance. Le camoufler est une faute distincte du fait de
l'utiliser, et c'est celle-là qui est sanctionnée.

La soutenance en temps réel — expliquer pourquoi ce taux d'apprentissage,
montrer ce qui casse quand on change une hypothèse — est ce qui départage.

## 6. Validation

Une validation s'appuie sur la **grille de revue de la famille de métier**
concernée, publique et consultable avant de soumettre.

Un refus nomme son motif parmi ceux que la plateforme sait dire : évaluation
insuffisante, reproductibilité manquante, provenance des données floue,
problème de sûreté — ou l'un des motifs communs à tous les domaines. Un refus
sans motif exploitable n'est pas un refus valable.

Cinq passages maximum. Au-delà, ce n'est plus le travail qui est en cause :
c'est le périmètre ou l'attribution, et quelqu'un les reprend avec vous.

## 7. Reproduction

Un banc et une trouvaille de sûreté ne comptent qu'une fois **rejoués par
quelqu'un d'autre**. Confirmer sa propre mesure est précisément ce que la
reproduction sert à écarter : la plateforme le refuse, quels que soient vos
droits.

## 8. Révocation

Un artefact validé peut être révoqué : fuite de données découverte, jeu de
données retiré pour licence, résultat non reproductible, plagiat.

La révocation retire l'artefact du décompte — rang, badges, attestations qui
s'appuyaient dessus. Elle n'efface pas l'historique : ce qui a été révoqué
reste visible comme tel.

---

*Voir aussi : les [modèles de brief](./BRIEF-TEMPLATES.md), les [modèles de
compte rendu](./WRITEUP-TEMPLATES.md) et la [politique de
divulgation](./SAFETY-DISCLOSURE.md).*
