# Skilluv Qualité — charte

Traduction de `CHARTER.md`, qui fait foi. Le dépôt écrit désormais en anglais
par défaut ; cette version existe parce que la communauté à qui la charte
s'adresse lit d'abord en français.

Publiée sur `skill-uv.com/quality/charter`.

---

## 1. La qualité est un métier, pas une étape

Ce qu'on dit le plus souvent du test, c'est qu'il vient *après*. Après la
fonctionnalité, après le design, après la construction — un portail par lequel
on passe en allant livrer.

Cette description ne produit qu'une seule sorte de travail de qualité :
s'apercevoir trop tard. Toutes les décisions qu'il aurait fallu influencer sont
déjà prises, et il ne reste qu'à objecter.

Sur Skilluv, la qualité est un métier avec ses artefacts, sa relecture, son
échelle et ses preuves. Un plan de test écrit avant que la fonctionnalité
existe est un livrable qualité. Une stratégie qui dit ce qu'une organisation
n'éprouvera pas est un livrable qualité. Un rapport d'anomalie assez précis
pour qu'un inconnu le reproduise est un livrable qualité. Aucun n'est une étape
dans le processus de quelqu'un d'autre.

## 2. Ce que nous acceptons comme preuve

La même règle que dans tous les autres domaines, appliquée à ce que celui-ci
produit : **un artefact dont quelqu'un d'autre peut se servir sans son auteur
dans la pièce.**

Concrètement, l'un de ceux-ci :

- Un **plan** ou une **stratégie de test** qui dit ce qui est couvert, ce qui
  ne l'est pas, et quel risque cela accepte.
- Une **suite de tests** qu'une autre équipe fait tourner dans sa propre
  chaîne.
- Un **rapport d'anomalie** dont un inconnu peut suivre la reproduction.
- Une **étude d'utilisabilité** ou un **audit d'accessibilité** avec un
  protocole, des séances réellement tenues ou une norme nommée, et des constats
  séparés des déductions.
- Un **compte rendu de playtest** qui transforme des séances en décisions
  qu'une équipe de jeu pouvait prendre.
- Une **analyse de couverture** qui classe les trous par risque et non par
  taille.

Ce que nous n'acceptons pas : un pourcentage de couverture sans son rapport,
une sortie de scanner sans tri, « je l'ai testé », ou une certification sans
travail derrière.

## 3. Le rapport d'anomalie est la signature du domaine, et il n'est pas fini
quand il est écrit

Un rapport d'anomalie devient une preuve Skilluv quand trois choses sont
vraies :

1. Quelqu'un d'autre que l'auteur l'a reproduit, ou aurait pu.
2. Un correctif a été livré.
3. **La personne qui l'a trouvé est retournée vérifier.**

La troisième est celle que personne ne fait, et c'est celle que nous
enregistrons. Une contribution fusionnée est l'affirmation de quelqu'un d'autre
que le problème a disparu. Retourner voir est la seule chose qui en fasse un
fait, et c'est la seule attestation de cette plateforme dont la condition ne
peut pas être remplie en travaillant plus dur tout seul.

## 4. La gravité s'argumente, elle ne s'affirme pas

Une gravité est une affirmation sur ce qu'un utilisateur perd. Ce n'est pas un
score d'outil, et ce n'est pas une impression sur le niveau d'agacement.

Celui qui signale annonce une gravité. Le relecteur peut en annoncer une autre,
et dans ce cas **les deux sont conservées**. Nous n'écrasons pas le chiffre de
l'auteur, parce qu'une tendance à systématiquement surévaluer est une
information qu'un mentor doit pouvoir voir — et parce qu'une échelle sur
laquelle personne ne peut se tromper n'est pas une échelle.

Le score d'artisanat lit le chiffre relu, et rien d'autre.

## 5. Le consentement n'est pas de la paperasse

Deux des cinq métiers qualité travaillent avec des personnes plutôt qu'avec des
systèmes.

- **Aucun enregistrement sans consentement écrit.** Pas « elle a dit que
  c'était bon ». Pas « c'est seulement pour l'interne ».
- **Aucun participant identifié dans un rapport.** Anonymiser, toujours.
- **Les enregistrements vont au client, jamais dans un portfolio.** La personne
  a consenti à un usage. Une bande démo n'est pas cet usage.

Une étude menée sans consentement est refusée quoi qu'elle ait trouvé. C'est le
seul refus de ce domaine qu'aucune qualité par ailleurs ne compense.

## 6. Éprouver le système d'un tiers demande son accord

Pour le test de sécurité, le périmètre est écrit et signé **avant** qu'on
touche à quoi que ce soit. Les règles d'engagement ne sont pas une formalité
juridique ajoutée au métier ; elles sont la discipline sur laquelle il repose.
Qui ne sait pas borner son propre périmètre ne peut pas se voir confier le
système de quelqu'un.

Pour le test exploratoire et le playtest, on demande à l'auteur. « C'était
public » n'est pas une permission, et « je n'ai rien cassé » non plus.

Un constat qui s'avère exploitable cesse d'être un travail de qualité et
devient une divulgation. Il passe par l'adresse de `SECURITY.md`, sur le délai
convenu avec la partie concernée, et pas dans un compte rendu public.

## 7. Ce qui n'a pas été éprouvé fait partie du rapport

Chaque artefact de ce domaine énonce ses trous.

Un rapport qui laisse un lecteur supposer une couverture totale est plus
dangereux que pas de rapport : il convertit une absence de preuve en un
sentiment de preuve, et quelqu'un livre là-dessus. Écrire « non vérifié » coûte
une ligne et sauve une mise en production.

C'est un critère de toutes les grilles de relecture, et le premier motif de
retour d'une soumission.

## 8. Attribution

Chaque rapport d'anomalie, étude et audit a un auteur nommé, et ce nom voyage
avec lui. Un constat utilisé par une équipe est crédité comme l'est une
contribution fusionnée.

Quand les termes d'une mission interdisent de nommer le client, l'attestation
dit quel type de système, ce qui a été trouvé et à quelle échelle — pas qui. La
compétence reste démontrable sans trahir l'engagement.

## 9. L'IA

Déclarée, et autorisée.

Se servir d'un assistant pour rédiger un plan de test, produire des cas ou
résumer des séances est un travail normal. Le cacher ne l'est pas. La
déclaration est un champ de la soumission, et le relecteur la lit comme un
contexte, pas comme un reproche.

Ce qu'un assistant ne peut pas faire, c'est précisément ce pour quoi ce domaine
existe : il ne peut pas retourner vérifier que le correctif fonctionne, il ne
peut pas s'asseoir avec un participant, et il ne peut pas décider ce qu'une
organisation accepte de ne pas éprouver. Ce sont ces parties-là qui portent
l'attestation.

## 10. Cinq métiers, un domaine

`qa-code`, `qa-cyber`, `qa-design`, `qa-game`, `qa-lead`.

Ils partagent une charte et presque rien d'autre. Qui sait juger une suite
Playwright ne sait pas juger un protocole d'utilisabilité, et prétendre le
contraire reviendrait à envoyer du travail à des relecteurs incapables de le
lire. Le droit de relecture est accordé par famille, et un droit dans une
famille n'en atteint aucune autre.

Ce qui tient le domaine ensemble n'est pas la technique. C'est la question :
**qu'est-ce qui devrait être vrai pour que ceci soit faux, et est-ce que
quelqu'un a vérifié ?**
