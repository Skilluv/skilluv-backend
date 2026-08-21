# Charte du domaine Audio

*Destinée à être publiée sur `skill-uv.com/audio/charter`.*

Cette charte dit ce qui est exigé, ce qui est refusé, et sur quoi une
validation se fonde. Elle est contraignante : une livraison qui s'en écarte est
refusée, quelle qu'en soit la qualité musicale.

---

## 1. Ce qu'est une livraison audio

Une livraison est un **artefact opposable** : quelque chose qu'un inconnu peut
écouter, utiliser et juger sans vous croire sur parole.

Sont recevables :

- une composition livrée avec ses stems et ses licences déclarées ;
- un pack sonore cohérent, nommé selon une convention, avec sa feuille
  d'usage ;
- une bande démo de comédien voix, jugée exploitable par un relecteur du
  métier ;
- un système musical adaptatif intégré et vérifié **dans une build jouable** ;
- une fonctionnalité audio livrée dans un moteur ou une bibliothèque ;
- un crédit sur une œuvre publiée.

Ne sont pas recevables :

- un extrait de trente secondes présenté comme un morceau ;
- une exportation MP3 sans master ni stems ;
- une capture d'écran d'une session de DAW ;
- un projet FMOD qui n'a jamais tourné dans un jeu ;
- une démo de voix dont on ne sait pas si c'est votre voix.

La différence n'est pas la difficulté. C'est la vérifiabilité.

## 2. Quatre exigences non négociables

**La provenance déclarée.** Chaque échantillon, boucle ou banque utilisé est
déclaré avec sa licence — ou tout est original et c'est écrit. **C'est
l'exigence la plus stricte du domaine, et la seule qui rende une livraison
inutilisable à elle seule** : une source non tracée expose le client à un
retrait, et l'auteur à une réclamation, des mois après la livraison.

**Le niveau mesuré.** Le loudness intégré (LUFS) et la crête vraie sont
mesurés, pas estimés, et adaptés à la destination. « Ça sonne fort » n'est pas
une mesure. La plateforme mesure les fichiers qu'elle reçoit ; l'écart entre le
niveau visé et le niveau atteint est ce qu'un relecteur regarde.

**Les stems, quand il y a une commande.** Sans pistes séparées, un client ne
peut rien ajuster sans revenir vers l'auteur. Une composition livrée sans stems
est incomplète, pas protégée.

**Le service du propos.** Le son sert ce à quoi il est attaché — le jeu,
l'image, l'interface, le récit. Un travail qu'on remarque au détriment de ce
qu'il accompagne a raté sa cible, même bien fait.

## 3. Ce que chaque métier garde

Les cinq métiers du domaine ne cèdent pas la même chose, et le contrat le dit
avant l'enregistrement, pas après.

| Métier | Ce qui se cède d'ordinaire | Ce qui se garde |
|---|---|---|
| Compositeur | une licence d'usage sur l'œuvre | la propriété de l'œuvre, sauf cession explicite et payée |
| Designer sonore | les fichiers livrés, pour l'usage convenu | les techniques, les enregistrements bruts, le droit de refaire |
| Comédien voix | un usage borné de l'enregistrement | la voix elle-même, toujours |
| Intégrateur | le projet middleware et son intégration | les outils et gabarits réutilisables |
| Programmeur audio | le code livré, sous la licence convenue | les briques génériques, sauf accord contraire |

**Le portfolio est le cas par défaut.** Un créateur qui ne peut pas montrer ce
qu'il a fait ne peut pas prouver qu'il l'a fait, et c'est la seule monnaie de
cette plateforme. Une clause qui l'interdit existe — `buyout` — et elle est
visible, séparée, et se paie.

## 4. Quatre étendues de licence, et pourquoi la question est posée

Propriété et licence sont deux questions différentes, et en musique elles ont
presque toujours deux réponses différentes. Toute mission audio doit dire
laquelle s'applique :

- **synchronisation seule** — usage à l'image dans l'œuvre nommée ;
- **commercial limité** — un support, un territoire, une durée ;
- **commercial mondial** — sans limite de territoire ni de durée ;
- **exclusif** — le client est seul à pouvoir utiliser l'œuvre, et cela se
  paie.

Une commande sans étendue déclarée est le premier motif de litige du métier :
le client suppose « mondial » et le compositeur suppose « ce jeu-là ».

## 5. L'IA générative

**Apprentissage : autorisé, à déclarer.** Utiliser Suno, Udio, MusicGen ou un
outil de séparation de sources pour apprendre, prototyper ou dépanner est
accepté. C'est déclaré, comme partout ailleurs sur la plateforme. Le camoufler
ne l'est pas.

**Mission payée : interdit sans accord écrit du client.** Un client qui commande
une musique originale achète une intention humaine et une chaîne de droits
qu'il peut défendre. Le statut juridique des sorties génératives est instable
dans la plupart des juridictions, et le lui faire porter sans qu'il le sache est
une faute.

**Voix de synthèse : interdit sans le consentement écrit de la personne.**
Sans exception, sans clause implicite, et sans « c'était pour tester ». Une voix
est un attribut de quelqu'un ; l'entraîner sans son accord est ce que cette
plateforme refuse le plus fermement. Voir
[VOICE-RIGHTS.md](./VOICE-RIGHTS.md).

## 6. Ce sur quoi une validation se fonde

La grille de revue de la famille, publique et lisible avant de soumettre :
composition, design sonore, voix, intégration. Elle est appliquée par un
relecteur qui a la capability correspondante — `audio_reviewer:composition`,
`audio_reviewer:sound-design`, `audio_reviewer:voice`,
`audio_reviewer:implementation`.

Une attestation de composition ou de pack n'est délivrée qu'une fois la
déclaration de sources complétée. Ce n'est pas une formalité : c'est la moitié
de ce que l'attestation affirme.

## 7. Les documents qui vont avec

- [LICENSING.md](./LICENSING.md) — échantillons, dépôt, synchronisation.
- [VOICE-RIGHTS.md](./VOICE-RIGHTS.md) — droits de la voix, non-concurrence,
  clonage.
- Les modèles de brief et de rapport sont servis par l'API
  (`/api/guides?domain=audio`), pas par ce dépôt : ils sont traduits et édités
  par des gens qui ne déploient pas.
