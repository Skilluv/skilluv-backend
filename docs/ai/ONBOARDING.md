# Premiers pas — domaine IA

Dix métiers, dix entrées. Chacune dit ce qu'il faut avoir, par quoi commencer,
et ce qu'on peut viser en trente jours.

Un principe commun : **le calcul n'est pas le verrou**. Colab et Kaggle
donnent un GPU gratuitement, DuckDB tourne sur un portable, et un modèle de
sept milliards de paramètres quantifié tient dans huit gigaoctets de mémoire
vive. Ce qui bloque n'est presque jamais la machine — c'est de ne pas savoir
par où commencer. D'où ce document.

Les outils cités sont dans le [toolkit](/api/ai/toolkit), avec pour chacun ce
qu'il coûte réellement d'y accéder.

---

## Data Engineer

**Prérequis** : SQL sérieux, Python courant. Pas besoin de cloud.

**Commencer par** : monter un pipeline batch complet en local — une source,
une transformation, une table — avec Dagster et DuckDB. Le rattrapage
d'historique est ce qui sépare un script d'un pipeline.

**En trente jours** : le pipeline batch de bout en bout, avec des contrôles de
qualité qui l'arrêtent quand la source ment.

**Communauté** : dbt Slack, r/dataengineering.

## Data Analyst

**Prérequis** : SQL. C'est tout, et c'est beaucoup.

**Commencer par** : une analyse de cohortes sur un jeu public. La difficulté
n'est pas la requête, c'est d'écrire une définition de métrique que deux
personnes calculeraient pareil.

**En trente jours** : un tableau de bord de six indicateurs, chacun avec sa
définition, et un rapport qui va jusqu'à la recommandation.

**Communauté** : Locally Optimistic, r/analytics.

## ML Engineer

**Prérequis** : Python, un peu d'algèbre linéaire. Colab suffit.

**Commencer par** : le cours fast.ai, qui commence par entraîner un modèle et
explique ensuite. Puis battre une régression logistique bien réglée sur un jeu
tabulaire — c'est plus dur qu'il n'y paraît, et la découverte est le vrai
premier acquis.

**En trente jours** : un modèle de classification servi derrière une API, avec
sa référence de comparaison et un entraînement reproductible.

**Communauté** : r/MachineLearning, PyTorch Forums.

## Prompt Engineer

**Prérequis** : savoir lire du Python. Un accès API, ou llama.cpp en local.

**Commencer par** : construire un jeu d'évaluation **avant** d'écrire les
invites. C'est l'inversion qui fait tout le métier : sans mesure, on ne
compare que des impressions.

**En trente jours** : vingt invites versionnées, chacune avec ses évaluations
et l'historique de ce qui a changé.

**Communauté** : Discord DSPy, r/LocalLLaMA.

## LLM Engineer

**Prérequis** : PyTorch, et le métier de prompt engineer derrière soi. Un GPU
loué à l'heure suffit pour un LoRA.

**Commencer par** : un affinage LoRA d'un petit modèle ouvert sur une tâche
étroite, publié sur HuggingFace avec sa fiche. Petit et publié vaut mieux que
grand et local.

**En trente jours** : ce LoRA, plus une recherche hybride avec ablation — ce
que chaque étage apporte, mesuré.

**Communauté** : EleutherAI Discord, HuggingFace forums.

## MLOps Engineer

**Prérequis** : conteneurs, CI/CD, un peu de Kubernetes. Le métier ops
appliqué à des artefacts qui vieillissent.

**Commencer par** : servir un modèle existant avec vLLM et mesurer la latence
de queue sous charge. La moyenne ment ; les percentiles hauts non.

**En trente jours** : une chaîne de réentraînement qui s'exécute seule, avec
sa procédure de retour arrière.

**Communauté** : MLOps Community Slack.

## Computer Vision Engineer

**Prérequis** : PyTorch, patience avec les jeux d'images. Colab tient pour
l'affinage.

**Commencer par** : affiner un détecteur sur un petit jeu annoté soi-même.
Annoter deux cents images enseigne plus sur le domaine que lire dix articles :
on découvre que le jeu décide plus que l'architecture.

**En trente jours** : le détecteur déployé, avec sa performance par
sous-population — pas seulement la moyenne.

**Communauté** : r/computervision, Ultralytics Discord.

## NLP Engineer

**Prérequis** : Python, et de la curiosité pour une langue précise.

**Commencer par** : un système de reconnaissance d'entités sur un corpus
qu'on connaît. Puis regarder ce qu'il rate — c'est là qu'on apprend ce que la
tokenisation fait vraiment.

**En trente jours** : le système avec son jeu d'évaluation annoté et un taux
d'erreur par type d'entité.

**Communauté** : **Masakhane** — TAL pour les langues africaines, mené depuis
le continent, ouvert sans affiliation académique. C'est l'entrée la plus
directe entre ce métier et un travail qui compte ici.

## AI Safety Researcher

**Prérequis** : rigueur méthodologique avant technique. Savoir écrire un
protocole compte plus que savoir entraîner.

**Commencer par** : lire les AI Safety Fundamentals, puis rejouer un
red-team publié sur un modèle ouvert. Rejouer avant de trouver : on apprend
ce qui rend un résultat rejouable en essayant de rejouer celui d'un autre.

**En trente jours** : une évaluation de biais avec protocole écrit, écarts
mesurés et recommandations — divulguée selon la
[politique](./SAFETY-DISCLOSURE.md).

**Communauté** : Alignment Forum, EleutherAI.

## Generative AI Artist

*Métier marqué expérimental : sa frontière avec le design n'est pas stabilisée,
et il évolue vite.*

**Prérequis** : un œil, et un GPU ou RunPod à l'heure.

**Commencer par** : reproduire délibérément une image. Même graine, mêmes
paramètres, même résultat. Tant qu'on relance au hasard jusqu'à ce que ça
tombe bien, il n'y a pas de métier.

**En trente jours** : une série de dix pièces qui tiennent ensemble, avec la
note d'intention qui les tient — c'est la cohérence qui se juge, pas l'image.

**Communauté** : r/StableDiffusion, Discord ComfyUI.

---

*Voir aussi : la [charte du domaine](./CHARTER.md) et les [modèles de
brief](./BRIEF-TEMPLATES.md).*
