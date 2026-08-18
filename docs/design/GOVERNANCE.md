# Gouvernance du domaine Design

*Qui décide quoi, et par quel chemin ça change.*

---

## Pourquoi ce document existe

Une charte dit ce qui est exigé. Elle ne dit pas qui a écrit l'exigence, ni
comment on la conteste. Sans ça, la première fois que quelqu'un est en
désaccord avec une critique, il découvre qu'il n'y a personne à qui parler —
et une communauté où le désaccord n'a pas de chemin devient une communauté qui
se tait puis qui part.

## Les cinq décisions et qui les prend

| Décision | Qui | Comment on la conteste |
| --- | --- | --- |
| Un verdict sur une version | Un relecteur de la famille | Demande d'arbitrage (§3) |
| Le droit de relire une famille | L'équipe, sur dossier | Nouvelle candidature après un travail supplémentaire |
| Le contenu d'une grille de revue | L'équipe, après consultation des relecteurs de la famille | Proposition publique (§4) |
| L'ajout ou l'archivage d'un métier | L'équipe | Proposition publique (§4) |
| La mise en avant hebdomadaire | L'équipe, avec raison écrite | Elle n'est pas contestable : c'est un choix éditorial assumé |

« L'équipe » veut dire les détenteurs de la capacité `admin`. Aujourd'hui,
trois personnes bénévoles. C'est écrit parce qu'une gouvernance qui prétend
être plus large qu'elle ne l'est se fait rattraper au premier conflit.

## 1. Les grilles de revue

Chaque famille a la sienne, plus une grille commune qui pose les mêmes
questions à tout le monde. Elles sont publiées avant qu'un challenge
commence : personne n'est jugé sur un critère qu'on ne lui a pas montré.

**Une grille change rarement, et jamais en cours de challenge.** Un travail est
jugé par la grille qui était affichée quand il a été réclamé. Changer les
critères pendant qu'on rend est le défaut le plus contestable qu'un concours
puisse avoir, et il ne coûte rien de l'interdire.

## 2. Le droit de relire

Accordé par famille (`design_reviewer:{famille}`), sur trois critères : du
métier dans la famille, la capacité à écrire une critique actionnable, et de la
disponibilité. Le détail est dans
[Devenir relecteur](REVIEWER-ONBOARDING.md).

**Il se retire.** Trois motifs, et ils sont écrits :

- approuver sans lire — repérable parce que les tours de critique sont publics ;
- une file laissée à l'abandon plusieurs semaines ;
- une critique qui juge la personne au lieu du travail.

Un retrait est notifié avec sa raison. Il n'annule pas les validations déjà
prononcées : revenir sur des attestations émises punirait les designers pour la
faute du relecteur.

## 3. L'arbitrage d'un désaccord

Un designer qui estime un verdict injuste demande un arbitrage. Le chemin :

1. **Répondre au relecteur d'abord**, dans la note de la version suivante. La
   moitié des désaccords sont un malentendu sur le brief et se règlent là.
2. **Demander un second regard** si ça ne suffit pas. Un autre relecteur de la
   famille lit la version et la critique, et dit s'il aurait rendu le même
   verdict.
3. **Arbitrage de l'équipe** si les deux relecteurs divergent. La décision est
   écrite, publique, et vaut précédent pour la famille.

Ce qui n'est pas un motif d'arbitrage : ne pas être d'accord avec un goût. Un
relecteur qui bloque sur un critère absent de la grille, en revanche, en est un
— et c'est pour ça que la grille est publiée avant.

## 4. Proposer un changement

Une grille, un métier, un motif de blocage, une règle de la charte : la
proposition se fait publiquement, dans `#design-general`, avec trois choses.

1. **Ce qui ne va pas**, avec un cas réel. « La grille brand ne demande pas la
   monochromie » est une proposition ; « la grille brand est faible » n'en est
   pas une.
2. **Ce que vous proposez**, dans les termes exacts qui seraient appliqués.
3. **Ce que ça casse.** Une modification de grille rend les travaux passés
   incomparables aux suivants ; le dire est ce qui distingue une proposition
   d'une envie.

Réponse de l'équipe sous deux semaines, motivée dans les deux cas. Un refus
sans raison écrite est un refus qui reviendra.

## 5. Ajouter un métier

Les vingt-six ne sont pas gravés. Un vingt-septième s'ajoute quand trois
conditions sont réunies :

- **des gens le pratiquent** et sont sur la plateforme ;
- **quelqu'un peut le relire** — un métier sans relecteur est une promesse
  vide ;
- **il produit un artefact opposable** qu'un inconnu peut ouvrir et juger.

La troisième condition est celle qui écarte le plus de candidats, et c'est
voulu : un métier dont le livrable est un avis n'entre pas dans ce cadre, même
s'il est un vrai métier.

**Un métier ne se renomme jamais.** Il s'archive, avec un renvoi vers celui qui
le remplace, parce que des attestations le nomment et qu'une attestation qui
désigne un métier disparu n'est plus vérifiable. C'est ce qu'a fait la
migration `0226` pour les cinq métiers d'avant.

## 6. La mise en avant

Une personne par domaine et par semaine, avec une phrase écrite par qui l'a
choisie, publiée telle quelle.

Trois règles, et elles sont dans le code :

- **une seule par semaine.** Deux personnes mises en avant la même semaine, ça
  veut dire qu'aucune ne l'a été. La rareté est toute la valeur.
- **pas deux fois en treize semaines.** Sinon c'est une rotation entre les
  mêmes quatre personnes.
- **il faut un livrable vérifié dans le domaine.** Mettre en avant un travail
  que personne n'a contrôlé est exactement ce que cette plateforme existe pour
  ne pas faire.

C'est éditorial, et c'est nommé comme tel. Il n'y a pas de formule derrière, et
en inventer une en ferait une version dégradée du score de métier au lieu de ce
que c'est : quelqu'un a choisi de mettre cette personne en avant, et y a mis
son nom.

**Rien n'est publié automatiquement sur les réseaux.** La plateforme compose le
message ; c'est une personne qui l'envoie. Publier le nom et le travail de
quelqu'un sur une plateforme tierce, à heure fixe, sans humain entre la
décision et la publication, n'est pas une fonctionnalité.

## 7. Ce que l'équipe ne peut pas faire

Écrit ici parce qu'une limite qu'on s'impose ne vaut que si elle est publique.

- **Émettre une attestation sans livrable vérifié.** Le schéma le refuse pour
  six bases sur sept, et la septième — la mise en avant — est éditoriale et
  nommée comme telle.
- **Modifier une critique déjà rendue.** Le journal est en ajout seul.
- **Retirer une ligne d'un classement conclu.** Ça réécrirait le classement de
  tous ceux qui étaient derrière.
- **Prélever une commission sur une dotation de concours.** Les entreprises
  payent, les talents non.

---

*Voir aussi : [Charte](CHARTER.md), [Devenir relecteur](REVIEWER-ONBOARDING.md),
[Propriété intellectuelle](IP-AND-COPYRIGHT.md).*
