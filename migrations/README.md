# Numéros de migration entre branches parallèles

Trois branches avancent en même temps sur ce dépôt — `code`, `design`, `ai` —
et chacune ajoute des migrations. Trois fois de suite, `feat/ai-orientations`
et `feat/code-orientations` ont écrit sur les mêmes numéros, et trois fois il a
fallu renuméroter à la fusion.

## La règle

**Une branche non fusionnée numérote dans son propre bloc.**

| Bloc | À qui |
|---|---|
| suite courante | `master`, et la branche qui fusionnera en premier |
| `0300–0399` | `feat/ai-orientations` |

`sqlx` applique les migrations dans l'ordre des versions et ne se soucie pas
des trous. Un bloc réservé ne coûte donc rien, et supprime la collision au lieu
de la reporter à la fusion suivante.

Une branche qui prend un bloc **s'applique après tout ce qui existe** au moment
où elle est écrite. C'est ce qu'on veut ici de toute façon : les migrations IA
lisent des tables que le domaine code crée — `craft_score_weights`, `missions`,
`content_guides` — donc elles doivent passer après.

## Ce que la renumérotation ne règle pas

Le numéro est la moitié visible du problème. L'autre moitié est plus coûteuse :

> **Un `CHECK` ne s'étend pas, il se remplace.**

Deux branches qui ajoutent chacune une valeur à la même contrainte écrivent
chacune la liste entière, et celle qui s'applique en dernier supprime
silencieusement ce que l'autre avait ajouté. C'est arrivé sur
`tournaments_kind_check` (voir 0223 et le correctif 0228).

Les contraintes partagées à ce jour :

- `tournaments_kind_check`
- `user_capabilities_capability_check`
- `attestations_basis_check` et `attestations_attestation_type_check`
- `project_slices_slice_type_check`
- `missions_deliverable_format_check`
- `slice_validation_decisions_blocking_reason_check`

Avant d'en réécrire une : lire la version la plus récente sur **toutes** les
branches vivantes, pas seulement la sienne.

Deux gardes existent pour la contrainte la plus exposée :
`every_derivable_reviewer_capability_is_grantable` échoue en CI si un métier
existe sans capability correspondante, et les commentaires portés par les
contraintes elles-mêmes disent ce qu'il en coûte de les réécrire.

## Corriger une migration déjà appliquée

Ne pas l'éditer. `sqlx` enregistre une empreinte par migration et refuse une
base dont l'empreinte a changé — c'est le souvenir que garde la branche
`hotfix/restore-migration-0068-checksum`. Ajouter une migration qui corrige la
précédente, comme 0228 le fait pour 0223.
