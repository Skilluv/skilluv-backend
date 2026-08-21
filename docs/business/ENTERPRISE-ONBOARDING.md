# Accueil d'une entreprise — déroulé

Écrit pour une équipe commerciale qui n'existe pas encore. C'est délibéré :
un processus rédigé après coup décrit ce qu'on a fait, pas ce qu'on voulait
faire.

**Aujourd'hui il n'y a ni équipe ni client.** Ce document est donc une
hypothèse sur la bonne façon de s'y prendre, à corriger au premier vrai
contact.

---

## 1. Le premier appel

Un seul objectif : **savoir quel problème la personne a**, pas placer un
produit.

Quatre questions, dans cet ordre :

1. **Qu'est-ce qui vous a fait chercher quelqu'un aujourd'hui ?** Un départ,
   une croissance, un projet précis qui bloque. La réponse oriente tout le
   reste.
2. **Comment recrutez-vous en ce moment ?** Ce qui a marché, ce qui a échoué,
   combien de temps ça prend.
3. **Qu'est-ce qui vous a fait douter d'un candidat la dernière fois ?**
   C'est la question qui compte. La réponse est presque toujours « je n'avais
   aucun moyen de vérifier ce qu'il disait savoir faire », et c'est exactement
   le problème que Skilluv résout.
4. **Quel est le délai, et quel est le budget ?** Posées ensemble, parce que
   l'une sans l'autre ne veut rien dire.

**Ce qu'on ne fait pas au premier appel :** la démonstration. Une démonstration
avant d'avoir compris le besoin est un catalogue, et un catalogue ne convainc
personne.

---

## 2. Recommander un produit

| Ce qu'ils disent | Ce qu'il leur faut |
|---|---|
| « On cherche un profil précis, pour un poste » | Recherche + crédits |
| « On recrute plusieurs personnes cette année » | Abonnement pipeline |
| « On a un chantier ponctuel bien délimité » | Prime ou mission |
| « On a un chantier gros et flou » | Studios |
| « On veut être connu des développeurs » | Sponsoring, événement |
| « On veut comprendre le marché » | Rapport, licence de données |
| « On ne sait pas par où commencer » | Conseil, deux jours |

**La recommandation honnête est parfois « rien ».** Une entreprise qui cherche
trente développeurs Java seniors en deux semaines ne les trouvera pas ici, et
le lui dire coûte un client aujourd'hui et en gagne trois plus tard.

---

## 3. La démonstration

Une seule chose à montrer, et ce n'est pas l'interface : **un profil réel avec
ses preuves**, et le lien de vérification cliqué en direct.

Le moment qui compte est celui où l'on ouvre `/verify/{code}` et où la page
affiche une contribution fusionnée dans un dépôt qu'ils connaissent. Ce que ça
dit, sans le dire : ce n'est pas nous qui affirmons, c'est vérifiable.

Ce qu'il ne faut pas faire :

- montrer la liste des fonctionnalités ;
- montrer un profil inventé pour la démonstration — c'est précisément le mode
  de défaillance que le produit corrige ;
- montrer une page vide en promettant qu'elle se remplira.

Si aucun profil réel n'a assez de preuves pour être montré, **il est trop tôt
pour la démonstration.** C'est un signal, pas un obstacle à contourner.

---

## 4. Vérification et mise en place

**Vérification de l'entreprise (KYC).** Existence légale, personne autorisée à
engager, moyen de paiement. Avant tout accès aux profils : une plateforme qui
laisse un compte non vérifié contacter ses membres est une plateforme dont les
membres partent.

**Second facteur obligatoire** pour tout compte entreprise. Non négociable :
un accès recruteur compromis, c'est un carnet d'adresses entier.

**Mise en place technique**, selon les cas :

- authentification déléguée (SSO) — pour les entreprises qui la demandent ;
- provisionnement des comptes (SCIM) — au-delà d'une dizaine de recruteurs ;
- API et jetons — pour l'intégration à un outil existant ;
- webhooks — pour recevoir les événements plutôt que d'interroger.

Compter une demi-journée avec un interlocuteur technique. Sans interlocuteur
technique, ne pas proposer d'intégration : elle ne sera pas finie et laissera
le souvenir d'un produit compliqué.

---

## 5. La première valeur

**Un objectif : que quelque chose de réel se produise dans les sept jours.**

Une première recherche accompagnée, un premier contact envoyé, ou une première
prime posée. Peu importe laquelle — ce qui compte est qu'ils aient fait une
action complète, seuls, et qu'elle ait abouti.

Une entreprise qui n'a rien fait dans les sept premiers jours ne fera rien.
C'est l'indicateur le plus fiable qu'il y ait, et il ne s'améliore jamais tout
seul.

---

## 6. Trente, soixante, quatre-vingt-dix jours

**Trente jours.** Ont-ils utilisé le produit sans nous ? Sinon, le problème
est la mise en place, pas l'intérêt. Reprendre au §5.

**Soixante jours.** Y a-t-il eu un résultat — un entretien, une livraison, une
prime versée ? Sinon, comprendre où ça bloque : les profils, la démarche, le
produit choisi.

**Quatre-vingt-dix jours.** Renouvelleraient-ils ? Poser la question
directement plutôt que d'attendre l'échéance. Une réponse tiède à quatre-vingt-dix
jours est un non à douze mois, et il reste trois mois pour le corriger.

---

## 7. Élargir

Une seule règle : **on n'élargit qu'après un résultat.** Proposer un programme
annuel à une entreprise qui n'a encore rien obtenu, c'est vendre une promesse
sur une promesse.

Les chemins qui ont du sens :

- crédits → abonnement pipeline, quand le volume le justifie ;
- prime → mission, quand le chantier grossit ;
- mission → Studios, quand il faut une équipe ;
- n'importe quoi qui marche → programme annuel, à la première échéance.

---

## 8. Modèles d'e-mails

Quatre, courts. Un e-mail commercial long n'est pas lu.

**Après le premier appel** — reformuler leur problème en une phrase, proposer
une chose, donner une date.

**Après la démonstration** — le lien de vérification qui a été montré, rien
d'autre. C'est la pièce qui travaille toute seule.

**Sept jours sans action** — une question, pas une relance : « qu'est-ce qui
vous a arrêté ? » On apprend plus de la réponse que de dix rendez-vous.

**Avant une échéance** — trente jours avant, avec ce qui s'est passé pendant
la période. Un renouvellement demandé sans bilan est un renouvellement qu'on
n'a pas mérité.

---

## 9. Ce qu'on ne fait pas

- **Pas de relance après deux non.** Un troisième message transforme un « pas
  maintenant » en « plus jamais ».
- **Pas de promesse sur des profils qui n'existent pas.** Si le vivier n'a pas
  ce qu'ils cherchent, le dire.
- **Pas de remise pour signer vite.** Un prix qui baisse sous la pression est
  un prix qui n'était pas juste.
- **Pas d'engagement au-delà de douze mois** avant qu'un premier cycle
  complet n'ait été livré.
