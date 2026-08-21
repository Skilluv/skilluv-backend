# Jetons, chaînes et attestations vérifiables — la décision

Ticket 16-06 demande une analyse et une décision. La voici, avec le
raisonnement, parce qu'une décision sans raisonnement se rouvre tous les six
mois.

**Décision : pas de jeton. Pas de chaîne. Les attestations restent vérifiables
par une URL et une signature.**

---

## 1. Ce qui était proposé

Trois idées circulent dès qu'une plateforme de preuves existe :

1. **un jeton** — récompenser les contributions, financer la communauté,
   aligner les intérêts ;
2. **des attestations sur chaîne** — pour qu'une preuve survive à la
   plateforme qui l'a émise ;
3. **une identité décentralisée** — pour que la personne possède son profil
   plutôt que nous.

Les trois répondent à de vraies inquiétudes. C'est ce qui les rend difficiles
à refuser d'un revers de main.

## 2. Le jeton : non

**Ce qu'il résoudrait** : rémunérer une contribution avant qu'il y ait des
revenus, et donner à la communauté une part de ce qu'elle construit.

**Ce qu'il produirait** :

- **une valeur mobilière dans plusieurs juridictions**. Un jeton distribué
  contre une contribution, dont la valeur dépend du succès de l'émetteur,
  ressemble à un titre financier partout où nous opérerions. La conformité
  coûterait plus que le premier million de revenu ;
- **une spéculation sur le travail des gens**. Le prix bougerait sans rapport
  avec l'effort fourni. Un contributeur payé en jeton un mois où il baisse de
  40 % a été payé 40 % de moins pour la même semaine de travail, et personne
  ne peut le lui expliquer ;
- **une communauté d'investisseurs**. Elle remplacerait la communauté de
  praticiens en douze mois. La question posée en public cesserait d'être « ce
  travail est-il bon » pour devenir « quand est-ce que ça monte ».

**Le point décisif** : Skilluv veut être une réponse à « qui sait faire quoi ».
Un jeton ajoute une seconde question — « combien ça vaut aujourd'hui » — qui
étouffe la première. Aucun des problèmes que le jeton résoudrait n'est plus
grave que celui qu'il créerait.

La rémunération sans revenus se traite autrement, et l'est déjà : le
bénévolat est reconnu et récompensé en commission de placement
(`mentor_referral_commissions`), pas en promesse de valeur future.

## 3. Les attestations sur chaîne : non, et voici ce qui les remplace

**L'inquiétude est réelle et bonne** : une attestation qui disparaît avec la
plateforme n'est pas une preuve, c'est une location. Quelqu'un qui a travaillé
trois ans doit pouvoir montrer ce travail si nous fermons.

**Ce qu'une chaîne apporterait** : la persistance et l'infalsifiabilité de la
signature.

**Ce qu'elle n'apporterait pas** :

- **la vérité du contenu**. Une attestation fausse ancrée sur une chaîne est
  une attestation fausse, éternelle et impossible à corriger. Le problème
  difficile n'est pas de prouver que nous avons signé — c'est de garantir que
  ce que nous avons signé était vrai, et aucune chaîne ne le fait ;
- **la révocabilité**. Une attestation retirée doit cesser d'être vérifiable.
  Une chaîne rend cela structurellement difficile, et les schémas de
  révocation qui existent réintroduisent un serveur — c'est-à-dire nous ;
- **la lisibilité**. Un recruteur à Cotonou vérifie une attestation en
  cliquant sur un lien, pas en installant un portefeuille.

**Ce que nous faisons à la place**, et qui répond à la même inquiétude :

- chaque attestation porte un **code de vérification** à 50 bits d'entropie et
  une page publique consultable sans compte ;
- chaque attestation nomme **ce sur quoi elle repose** (`basis`) et pointe
  vers l'artefact — une pull request fusionnée, un paquet publié, une place en
  finale. La preuve principale est l'artefact, pas notre signature : il reste
  vérifiable si nous disparaissons ;
- un **export complet** est déjà accessible à chacun, et
  [CHARTER.md](CHARTER.md) engage à le maintenir.

C'est plus faible que l'ancrage cryptographique sur un point — la
persistance — et plus fort sur les trois qui comptent davantage. Si un jour un
standard d'attestation vérifiable s'impose auprès des employeurs que nos
membres visent, nous signerons dans ce format en plus, sans jeton et sans
chaîne propre.

## 4. L'identité décentralisée : pas maintenant, et sans regret

L'idée est bonne et l'écosystème n'est pas prêt. Aucun employeur d'Afrique de
l'Ouest ne demande un identifiant décentralisé, aucun ATS n'en lit, et
l'adopter aujourd'hui reviendrait à ajouter une couche que personne n'utilise
au parcours d'inscription d'un public qui, pour partie, découvre Git.

À revoir quand un standard sera lisible par un outil que nos membres
rencontrent déjà.

## 5. Ce que cette décision engage

- **aucun jeton**, aucune levée en jeton, aucun partenariat qui en suppose un ;
- **aucune chaîne** dans l'infrastructure ;
- **l'engagement de portabilité reste contractuel**, dans la charte, et c'est
  ce qui doit être tenu à sa place.

Réexaminable si un employeur significatif de nos marchés exige un format
vérifiable sur chaîne. Pas réexaminable pour des raisons de financement : ce
serait exactement le raisonnement que cette page refuse.
