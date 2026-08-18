# Licences musicales — ce qu'il faut savoir avant de livrer

*Destiné à être publié sur `skill-uv.com/audio/licensing`.*

> **Ceci n'est pas un avis juridique.** C'est ce que la plateforme exige et ce
> qu'elle a compris de la pratique du métier. Une mission au-delà de quelques
> milliers d'euros, une exclusivité, ou un différend justifient un juriste
> spécialisé. La revue juriste de ce document est ouverte et sera mutualisée
> avec les autres domaines.

---

## 1. La règle qui gouverne tout le reste

**Une source non tracée rend la livraison inutilisable.**

Pas « moins bonne » : inutilisable. Un client qui découvre qu'une boucle de
votre morceau vient d'un pack dont vous n'avez pas la licence a trois options,
et les trois lui coûtent : retirer l'œuvre, la refaire, ou négocier
rétroactivement. Il n'en choisit aucune sans vous le faire payer.

C'est pourquoi la plateforme demande une **déclaration de sources** avant de
délivrer une attestation de composition ou de pack. La déclaration est une
phrase que vous signez : *la liste est complète et exacte*. Une liste vide
accompagnée de cette phrase est parfaitement valable — cela veut dire « tout est
original ». Une liste vide sans la phrase veut dire « personne n'a rempli le
formulaire », et ce n'est pas la même chose.

## 2. Les six natures de source

| Nature | Ce qu'elle exige |
|---|---|
| `original` | rien à attribuer. À dire quand même. |
| `public_domain` | domaine public ou CC0. Sans condition. |
| `creative_commons` | **la mention de crédit, verbatim**. Certaines variantes interdisent l'usage commercial. |
| `royalty_free` | acheté ou souscrit. Garder la preuve d'achat et le numéro de licence. |
| `licensed_commercial` | licence négociée pour un usage nommé. |
| `third_party_work` | l'œuvre ou l'interprétation de quelqu'un d'autre. |

### Creative Commons : gratuit ne veut pas dire libre

- **BY** — crédit obligatoire, sous la forme exacte demandée par l'auteur.
  Utiliser le son sans le crédit n'est pas un oubli, c'est une contrefaçon.
- **BY-SA** — le crédit, **et** l'œuvre dérivée sous la même licence. Cela
  contamine votre morceau : à éviter dans une commande client, sauf si le
  client sait et accepte.
- **BY-NC** — pas d'usage commercial. **Une mission payée est un usage
  commercial**, y compris quand le jeu est gratuit et financé autrement.

Freesound et OpenGameArt mélangent les trois. La licence est indiquée par
fichier, pas par pack.

### Les banques payantes

Splice, Kontakt, EastWest et les autres vendent d'ordinaire une licence
d'usage personnelle, non transférable, sur les rendus. Deux conséquences que
les gens découvrent tard :

1. **Vous ne pouvez pas livrer les échantillons bruts** au client, seulement le
   rendu qui les contient.
2. **La licence ne se transfère pas** avec l'œuvre. Si le client veut refaire
   le mixage, il lui faut sa propre licence de la banque.

Déclarez la banque une fois, avec son numéro de licence si elle en a un.

## 3. Le dépôt : SACEM, ASCAP, BMI

Déposer une œuvre auprès d'une société de gestion collective sert à percevoir
des droits de diffusion — radio, télévision, plateformes, lieux publics. Cela
**ne** remplace **pas** le contrat avec le client, et cela peut entrer en
conflit avec lui.

Trois choses à savoir avant de déposer :

- **Le dépôt ne crée pas le droit d'auteur.** Vous l'avez dès la création. Le
  dépôt sert à être payé quand l'œuvre est diffusée.
- **Une œuvre déposée ne peut plus être cédée librement.** Si vous êtes membre
  de la SACEM, vous lui avez apporté vos droits de diffusion : un contrat de
  rachat total (`buyout`) devient incompatible avec votre statut. Dites-le au
  client avant, pas au moment de signer.
- **Le jeu vidéo est un cas mal couvert.** Les droits de diffusion sur une
  musique jouée dans un jeu se perçoivent mal ou pas du tout selon les
  territoires. Ne comptez pas dessus pour compenser un tarif bas.

Adhérer n'est pas obligatoire et n'est pas toujours souhaitable en début de
carrière. C'est une décision à prendre en connaissance de cause, pas un passage
obligé.

## 4. Synchronisation, mécanique, exécution

Trois droits distincts, souvent confondus :

- **Synchronisation** — associer une musique à une image ou à un logiciel.
  C'est celui qui compte dans presque toute mission de cette plateforme.
- **Mécanique** — reproduire l'œuvre sur un support. Concerne surtout les
  reprises.
- **Exécution publique** — jouer l'œuvre en public ou la diffuser. C'est ce que
  perçoit une société de gestion.

Une mission Skilluv accorde d'ordinaire une **licence de synchronisation**,
d'étendue déclarée. Le champ `licensing_scope` de la mission est ce champ-là.

### Les reprises

Une reprise exige une licence mécanique du titulaire, et une reprise dans un
jeu exige en plus une licence de synchronisation — que le titulaire peut
refuser sans motif. En pratique : **n'acceptez pas une commande de reprise sans
que le client ait obtenu les autorisations par écrit.**

## 5. La musique générée

Le statut juridique des sorties de Suno, Udio ou MusicGen n'est pas stable. En
2026, selon les juridictions, elles peuvent n'être protégées par aucun droit
d'auteur — donc être réutilisées par n'importe qui, y compris un concurrent du
client — et faire l'objet de réclamations sur les données d'entraînement.

Position de la plateforme :

- **apprentissage et prototypage** : autorisés, déclarés ;
- **mission payée** : interdits sans accord écrit du client, qui doit savoir ce
  qu'il achète ;
- **déclaration** : une piste générée se déclare comme une source, au même
  titre qu'un échantillon.

## 6. Le contrat minimal

Cinq lignes, avant l'enregistrement :

1. **Ce qui est livré** — formats, stems, délais.
2. **Ce qui est cédé** — propriété, ou licence et son étendue.
3. **Où et combien de temps** — territoire et durée.
4. **Exclusif ou non.**
5. **Portfolio** — oui par défaut ; si non, pourquoi et à quel prix.

Un modèle par étendue est servi avec les modèles de brief
(`/api/guides?domain=audio&kind=brief_template`).
