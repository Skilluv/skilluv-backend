# Discord — les variables du déploiement

Ce que `scripts/discord-setup.py` a lu sur le serveur Skilluv, à reporter dans
la configuration du backend. Régénérable à tout moment :

```
python3 scripts/discord-setup.py --env
```

## Pourquoi ces identifiants sont dans le dépôt alors que la migration 0257 les refuse

0257 refuse de seeder `discord_channels`, et elle a raison : une migration
s'applique toute seule contre la base vers laquelle on la pointe, donc une
migration qui transporte ces valeurs poste la suite de tests de la plateforme
dans le vrai Discord de quelqu'un.

Un document ne s'exécute pas. Et un identifiant de salon n'est pas un secret :
il apparaît dans chaque lien de message que n'importe quel membre peut
partager (`discord.com/channels/<serveur>/<salon>/<message>`). Quiconque est
sur le serveur les a déjà.

**Le secret, c'est `DISCORD_BOT_TOKEN`, et il n'est pas ici.** Il vit dans le
`.env`, qui est gitignoré, et le script ne l'imprime jamais.

## Sur le backend

| Variable | Rôle |
|---|---|
| `DISCORD_GUILD_ID` | le serveur |
| `DISCORD_ANNONCES_CHANNEL_ID` | salon de repli quand un domaine n'a pas le sien |
| `DISCORD_PROMOTIONS_CHANNEL_ID` | idem pour les promotions |
| `SKILLUV_DISCORD_CHANNELS` | le routage complet, 39 salons sur 5 destinations |

Le bot refuse de démarrer sans les deux salons de repli.

```
DISCORD_GUILD_ID=1530972791837294782
DISCORD_ANNONCES_CHANNEL_ID=1536351909294776402
DISCORD_PROMOTIONS_CHANNEL_ID=1536351993151492137
```

### `SKILLUV_DISCORD_CHANNELS`

Une seule ligne, de `[` à `]`, sans guillemets autour et sans retour à la
ligne. C'est la forme durable : `services::seed` la relit à chaque démarrage,
donc une base supprimée reconstruit son propre routage. Les logs disent alors
`39 rooms routed`.

```
SKILLUV_DISCORD_CHANNELS=[{"purpose":"contests","domain":"ai","channel_id":"1543041827702046910","label":"#ai-competitions"},{"purpose":"contests","domain":"audio","channel_id":"1543041872769982495","label":"#audio-battles"},{"purpose":"contests","domain":"code","channel_id":"1543041952709214248","label":"#code-contests"},{"purpose":"contests","domain":"design","channel_id":"1543042107659264011","label":"#design-concours"},{"purpose":"contests","domain":"game","channel_id":"1543042199359332548","label":"#game-jams"},{"purpose":"contests","domain":"quality","channel_id":"1543042372848328785","label":"#qa-bug-bashes"},{"purpose":"contests","domain":"security","channel_id":"1543042424455303320","label":"#security-competitions"},{"purpose":"general","domain":null,"channel_id":"1536351909294776402","label":"#annonces"},{"purpose":"general","domain":"ai","channel_id":"1543041805631623242","label":"#ai-general"},{"purpose":"general","domain":"audio","channel_id":"1543041847931445339","label":"#audio-general"},{"purpose":"general","domain":"code","channel_id":"1543041898803892294","label":"#code-general"},{"purpose":"general","domain":"communication","channel_id":"1543042011072827506","label":"#comm-general"},{"purpose":"general","domain":"design","channel_id":"1543042052936302622","label":"#design-general"},{"purpose":"general","domain":"education","channel_id":"1543042122033266738","label":"#edu-general"},{"purpose":"general","domain":"game","channel_id":"1543042164659851317","label":"#game-general"},{"purpose":"general","domain":"leadership","channel_id":"1543042239326855289","label":"#leadership-general"},{"purpose":"general","domain":"ops","channel_id":"1543042283845197826","label":"#ops-general"},{"purpose":"general","domain":"quality","channel_id":"1543042345082163243","label":"#qa-general"},{"purpose":"general","domain":"security","channel_id":"1543042391945125948","label":"#security-general"},{"purpose":"general","domain":"soft_skills","channel_id":"1543044023932162130","label":"#oss-maintainers"},{"purpose":"missions","domain":"ai","channel_id":"1543041830776733786","label":"#ai-missions"},{"purpose":"missions","domain":"audio","channel_id":"1543041878700589197","label":"#audio-missions"},{"purpose":"missions","domain":"code","channel_id":"1543041959306862655","label":"#code-missions"},{"purpose":"missions","domain":"communication","channel_id":"1543042038491250841","label":"#comm-missions"},{"purpose":"missions","domain":"design","channel_id":"1543044014675071016","label":"#design-missions"},{"purpose":"missions","domain":"education","channel_id":"1543042149644374046","label":"#edu-missions"},{"purpose":"missions","domain":"game","channel_id":"1543042208733601833","label":"#game-missions"},{"purpose":"missions","domain":"leadership","channel_id":"1543042265126010963","label":"#leadership-missions"},{"purpose":"missions","domain":"ops","channel_id":"1543042321820549202","label":"#ops-missions"},{"purpose":"missions","domain":"quality","channel_id":"1543042375809499178","label":"#qa-missions"},{"purpose":"missions","domain":"security","channel_id":"1543044017552498760","label":"#security-missions"},{"purpose":"promotions","domain":null,"channel_id":"1536351993151492137","label":"#promotions"},{"purpose":"winners","domain":"ai","channel_id":"1543041827702046910","label":"#ai-competitions"},{"purpose":"winners","domain":"audio","channel_id":"1543041872769982495","label":"#audio-battles"},{"purpose":"winners","domain":"code","channel_id":"1543041952709214248","label":"#code-contests"},{"purpose":"winners","domain":"design","channel_id":"1543042107659264011","label":"#design-concours"},{"purpose":"winners","domain":"game","channel_id":"1543042205696917506","label":"#game-winners"},{"purpose":"winners","domain":"quality","channel_id":"1543042372848328785","label":"#qa-bug-bashes"},{"purpose":"winners","domain":"security","channel_id":"1543042424455303320","label":"#security-competitions"}]
```

## Sur l'hôte du bot

`DISCORD_BOT_TOKEN`, et rien d'autre. Lancer `skilluv-discord-bot`, **pas**
`skilluv-discord-notifier` : le notifier est le repli webhook v1 et ne sait
pas router les cinq destinations.

## État du serveur au 29/08/2026

213 objets déclarés, 213 présents. Community mode activé, et les 22 salons
créés en texte ordinaire avant cette activation ont été convertis en salons
d'annonces **sur place** — mêmes identifiants, historique conservé, donc les
valeurs ci-dessus restent valides.

Ce qui reste manuel : l'attribution des rôles. Rien ne remplit
`users.discord_user_id` (migration 0138 crée la colonne, le code ne fait que
la lire et l'effacer au titre du RGPD), donc la plateforme ne peut pas savoir
quel membre Discord est quel compte. Voir le ticket sur le lien OAuth.
