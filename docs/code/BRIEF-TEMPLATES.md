# Modèles de brief — domaine Code

Huit modèles, un par famille de métier. À utiliser pour rédiger l'énoncé d'un
challenge.

Un brief mal écrit produit des livrables incomparables entre eux : chacun
répond à une question différente, et le relecteur arbitre au jugé. La
structure commune ci-dessous existe pour éviter ça.

**Les familles correspondent aux groupes de revue** (`reviewer_group` sur les
orientations), donc à la grille de revue qui sera appliquée. Écrire un brief
dans la mauvaise famille, c'est promettre une grille et en appliquer une
autre.

---

## Structure commune

Tout brief comporte ces six sections, dans cet ordre.

### 1. Problème

Ce qui ne va pas aujourd'hui, en une ou deux phrases, **du point de vue de
quelqu'un que ça gêne**. Pas la solution attendue.

> Mauvais : « Implémenter un cache Redis. »
> Bon : « La page d'accueil interroge la base à chaque visite et met deux
> secondes à répondre aux heures de pointe. »

### 2. Contraintes techniques

Ce qui est imposé et ce qui est libre. Une contrainte non écrite est une
contrainte que le candidat découvrira en revue, ce qui est déloyal.

Préciser : langages ou plateformes imposés, dépendances interdites, version
minimale à supporter, environnement de déploiement.

### 3. Livrables

Toujours les trois : **code**, **tests**, **documentation**. Dire où ils
atterrissent — un dépôt, une contribution en amont, un paquet publié.

### 4. Critères d'acceptation

Vérifiables, au sens où deux relecteurs aboutiraient à la même conclusion.
Chiffrer ce qui peut l'être.

> Mauvais : « Le site doit être rapide. »
> Bon : « LCP sous 2,5 s sur mobile 4G simulé, mesuré avant et après. »

### 5. Licence

Celle du livrable, et celles à respecter en amont. Un travail sans licence
n'est utilisable par personne.

### 6. Hors périmètre

Ce qu'il ne faut **pas** faire. C'est la section la plus souvent omise et
celle qui évite le plus de travail perdu.

---

## 1. Application web — `web`

*frontend, backend, fullstack, performance, web3-frontend*

Préciser en plus :
- navigateurs et tailles d'écran à supporter ;
- contrat d'API si le front et le back sont séparés ;
- exigence d'accessibilité (niveau visé, et comment il sera vérifié) ;
- budget de performance chiffré.

Pour le web3 : réseau visé, comportement attendu si l'utilisateur refuse la
signature, et ce qui se passe en cas de réorganisation de chaîne.

## 2. Application mobile — `mobile`

*iOS, Android, cross-platform*

Préciser en plus :
- versions d'OS minimales ;
- comportement hors-ligne attendu — c'est la question qui distingue un
  travail mobile sérieux ;
- permissions demandées et conduite à tenir en cas de refus définitif ;
- si la publication en magasin fait partie du livrable.

## 3. Bureau et logiciel d'entreprise — `devtools-media`

*desktop, enterprise, lowcode*

Préciser en plus :
- systèmes d'exploitation cibles et mode d'installation ;
- signature et mise à jour : attendues ou hors périmètre ;
- pour l'entreprise : mode d'authentification, cloisonnement des données,
  exigences de traçabilité.

## 4. Systèmes et embarqué — `systems`

*systems-programmer, kernel, firmware, robotique, critique*

Préciser en plus :
- matériel cible, ou simulateur accepté à défaut ;
- contraintes de mémoire et d'énergie, chiffrées ;
- comportement attendu à la défaillance — un système embarqué qui n'a pas de
  mode dégradé défini n'est pas terminé ;
- pour le critique : la norme applicable et le niveau visé.

## 5. Blockchain — `blockchain`

*smart contracts, protocoles*

Préciser en plus :
- réseau de test puis réseau principal, ou test uniquement ;
- budget en gas si pertinent ;
- hypothèses de confiance : qui peut faire quoi, et ce que l'administrateur
  peut faire ;
- **rappel systématique** : un déploiement ne se corrige pas. Le brief doit
  dire ce qui est irréversible.

## 6. Compilation et méthodes formelles — `compilers`

*compilateurs, langages, preuves*

Préciser en plus :
- grammaire ou spécification de référence ;
- cible de génération, ou propriété à démontrer ;
- exigence sur les messages d'erreur : un outil qui dit seulement « non »
  n'aide personne, et cela se juge ;
- jeu de programmes de test fourni ou à construire.

## 7. Données et systèmes distribués — `data`

*moteurs de base, recherche, distribué, flux*

Préciser en plus :
- garanties attendues : cohérence, durabilité, sémantique de livraison ;
- volume et charge de référence pour les mesures ;
- pannes à supporter, et comportement attendu sous chacune ;
- ce qui est mesuré : percentiles hauts, pas moyenne.

## 8. Calcul scientifique et GPU — `scientific`

*scientifique, GPU, quantitatif*

Préciser en plus :
- référence de validation : solution analytique, jeu de données connu ;
- exigence de reproductibilité — graines, environnement figé ;
- matériel de référence pour les mesures de performance ;
- pour le quantitatif : coûts de transaction et biais à éviter dans un
  backtest.

---

## Version anglaise

Les briefs publiés le sont dans la langue du challenge. La structure ci-dessus
se traduit sans adaptation : *Problem, Technical constraints, Deliverables,
Acceptance criteria, Licence, Out of scope*. Les exemples chiffrés, eux, ne se
traduisent pas — ils se recalculent pour le contexte visé.
