-- The AI guides and templates, as rows.
--
-- ## Why these moved out of the repository
--
-- They were two Markdown files, written before migration 0199 existed. That
-- migration's argument holds and applies here: guides have to be translated
-- and edited by somebody who is not deploying, and a link that rots should
-- not need a pull request. The files are deleted in the same commit — two
-- copies of a guide is how the two start disagreeing.
--
-- ## Grouped by reviewer family, not by trade
--
-- Ten trades would mean ten guides, five of which would repeat each other.
-- The families are how the trades are already grouped everywhere else — by
-- who is competent to review them — and following that keeps a guide, a
-- reviewer group and a set of orientations pointing at each other.
--
-- ## What every AI guide says
--
-- The same five things, because they are the five a person arriving actually
-- asks: what the family is, what it takes to start, thirty days, the trap
-- specific to it, and where the people are.
--
-- The trap is the section that earns its place. Every one of these families
-- has a way of producing work that looks finished and proves nothing, and it
-- is different in each: a leaking split, an unmeasured prompt, a model that
-- only ever saw clean images.

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('onboarding-ai-data', 'onboarding', 'ai', 'data', 'fr',
 'Débuter dans la donnée',
 'Ingénierie et analyse. Le métier IA qui demande le moins de calcul et le plus de rigueur sur ce que les chiffres veulent dire.',
$md$
# Débuter dans la donnée

Deux métiers : faire arriver la donnée là où elle est utile, et en tirer une
décision. C'est la famille où l'on peut aller le plus loin sans GPU — DuckDB
tourne sur un portable — et celle où le travail se juge le plus vite, parce
qu'un chiffre faux se voit.

## Ce qu'il faut

SQL sérieux. Python pour l'ingénierie. Aucun matériel particulier : le
meilleur rapport apprentissage/coût de tout le domaine.

## Trente jours

**Semaine 1 — une question.** Prends un jeu de données public et une question
précise. Écris la définition de la métrique avant la requête. C'est
l'exercice, pas la requête.

**Semaine 2 — un pipeline qui échoue bien.** Orchestre un chargement avec
Dagster, en local. Fais-le tomber au milieu volontairement, et regarde ce que
la table contient après.

**Semaine 3 — les contrôles.** Ajoute des contrôles de qualité qui arrêtent
le pipeline. Un pipeline qui continue avec des données fausses coûte plus
cher qu'un pipeline arrêté.

**Semaine 4 — le rattrapage.** Rejoue sept jours d'historique sans rien
dupliquer. C'est ce qui sépare un script d'un pipeline, et c'est ce qu'un
relecteur regarde en premier.

## Le piège de cette famille

Un tableau de bord magnifique dont deux personnes calculent l'indicateur
différemment. La définition écrite vaut plus que le graphique.

## Où sont les gens

dbt Slack, r/dataengineering, Locally Optimistic.
$md$, 10),

('onboarding-ai-ml', 'onboarding', 'ai', 'ml', 'fr',
 'Débuter dans les modèles',
 'Entraîner, et garder en vie. Deux métiers que le marché confond et qui ne se ressemblent pas.',
$md$
# Débuter dans les modèles

Entraîner un modèle et le maintenir en service sont deux métiers. Le premier
finit quand la courbe est bonne ; le second commence là.

## Ce qu'il faut

Python, un peu d'algèbre linéaire, et Colab suffit pour commencer. Pour la
partie exploitation : conteneurs et CI/CD, c'est-à-dire le métier ops
appliqué à des artefacts qui vieillissent.

## Trente jours

**Semaine 1 — fast.ai.** Le cours commence par entraîner un modèle et
explique ensuite. C'est le bon ordre.

**Semaine 2 — battre la référence.** Prends un jeu tabulaire et essaie de
battre une régression logistique bien réglée. Découvrir que c'est difficile
est le premier vrai acquis du métier.

**Semaine 3 — servir.** Mets le modèle derrière une API et mesure la latence
sous charge. Regarde les percentiles hauts : la moyenne ment.

**Semaine 4 — surveiller.** Décide ce que tu surveillerais en production, et
quel seuil déclencherait quoi. Avant l'incident, pas après.

## Le piège de cette famille

La fuite de données. Un découpage aléatoire sur des données temporelles
fabrique un score que rien ne reproduira. C'est l'erreur la plus fréquente du
domaine et la moins visible — c'est celle qu'un relecteur cherche en premier.

## Où sont les gens

r/MachineLearning, PyTorch Forums, MLOps Community Slack.
$md$, 20),

('onboarding-ai-llm-nlp', 'onboarding', 'ai', 'llm-nlp', 'fr',
 'Débuter dans le langage',
 'Invites, agents et TAL. La famille où tout le monde arrive et où presque personne ne mesure.',
$md$
# Débuter dans le langage

Trois métiers : calibrer des invites, construire des systèmes qui utilisent
un modèle, et traiter le langage comme une structure. C'est la porte d'entrée
la plus fréquentée du domaine, et celle où la différence entre quelqu'un qui
sait et quelqu'un qui bricole se voit le plus vite.

## Ce qu'il faut

Savoir lire du Python. Un accès API, ou llama.cpp en local — un modèle de
sept milliards de paramètres quantifié tient dans huit gigaoctets de mémoire
vive.

## Trente jours

**Semaine 1 — le jeu d'évaluation d'abord.** Écris vingt cas, dont cinq où
le système doit refuser ou dire qu'il ne sait pas. Avant d'écrire la moindre
invite. Cette inversion est tout le métier.

**Semaine 2 — les invites.** Calibre-les contre ce jeu. Versionne-les. Une
modification se justifie par une mesure, pas par une impression.

**Semaine 3 — un RAG.** Sur un corpus que tu connais, pour pouvoir juger les
réponses. Mesure ce que chaque étage apporte : sans ablation, personne ne
sait si le reclasseur sert.

**Semaine 4 — l'attaque.** Essaie de faire sortir ton propre système de son
rôle. Note le taux de réussite. C'est le chiffre qui intéresse un relecteur.

## Le piège de cette famille

« Ça marche bien. » Sans jeu d'évaluation, il n'y a pas de travail, il y a
une impression — et trois exemples réussis n'en sont pas un.

## Où sont les gens

Discord DSPy, r/LocalLLaMA, EleutherAI, et **Masakhane** pour le TAL des
langues africaines : le terrain le plus proche d'ici, ouvert sans affiliation
académique.
$md$, 30),

('onboarding-ai-cv', 'onboarding', 'ai', 'cv', 'fr',
 'Débuter dans l''image',
 'Vision et génératif. Là où le jeu de données décide plus que l''architecture.',
$md$
# Débuter dans l'image

Détecter, segmenter, ou produire. Les deux moitiés de cette famille — la
vision qui juge et la diffusion qui fabrique — partagent une chose : le
résultat dépend des données bien plus que du modèle.

## Ce qu'il faut

PyTorch, de la patience avec les jeux d'images, et Colab pour l'affinage. Un
GPU loué à l'heure pour le génératif.

## Trente jours

**Semaine 1 — annoter.** Annote deux cents images toi-même. C'est fastidieux
et c'est le seul moyen de comprendre pourquoi le modèle se trompe.

**Semaine 2 — affiner.** Un détecteur sur ce jeu. Regarde ce qu'il rate, pas
seulement le mAP.

**Semaine 3 — dégrader.** Teste sur du flou, du contre-jour, un angle
inhabituel. Les images propres ne prouvent rien.

**Semaine 4 — la sous-population.** Mesure la performance par groupe. Un
modèle de visages qui n'a pas été testé par teint n'a pas été testé.

Pour le génératif, la même semaine 4 se lit autrement : reproduis
délibérément une image. Même graine, mêmes paramètres, même résultat. Tant
qu'on relance au hasard jusqu'à ce que ça tombe bien, il n'y a pas de métier.

## Le piège de cette famille

Une moyenne qui cache le groupe sur lequel le modèle échoue.

## Où sont les gens

r/computervision, Discord Ultralytics, r/StableDiffusion, Discord ComfyUI.
$md$, 40),

('onboarding-ai-safety', 'onboarding', 'ai', 'safety', 'fr',
 'Débuter dans la sûreté',
 'Chercher activement l''échec. La famille où la méthode compte plus que la technique.',
$md$
# Débuter dans la sûreté

Trouver ce qu'un modèle fait quand on essaie de le faire échouer, et le dire
correctement. C'est le seul métier du domaine où savoir écrire un protocole
compte davantage que savoir entraîner.

## Ce qu'il faut

De la rigueur avant de la technique. Un accès à un modèle ouvert. Aucun
matériel particulier.

## Trente jours

**Semaine 1 — lire.** AI Safety Fundamentals, et deux rapports de red-team
publiés. Regarde leur forme autant que leur contenu.

**Semaine 2 — rejouer.** Reproduis un red-team publié sur un modèle ouvert.
Rejouer avant de trouver : on apprend ce qui rend un résultat rejouable en
essayant de rejouer celui d'un autre.

**Semaine 3 — mesurer.** Cinquante tentatives, un taux de réussite. Une
capture d'écran n'est pas une trouvaille.

**Semaine 4 — divulguer.** Écris l'atténuation, préviens l'éditeur, note la
date. La [politique de divulgation](/ai/disclosure) dit l'ordre, et il est
contraignant.

## Le piège de cette famille

Publier vite. Entre le moment où une attaque est écrite publiquement et celui
où elle est corrigée, elle est utilisable par n'importe qui.

## Où sont les gens

Alignment Forum, EleutherAI, Deep Learning Indaba pour le continent.
$md$, 50);

-- ═══════════════════════════════════════════════════════════════════
-- The ten documents an AI contributor writes
-- ═══════════════════════════════════════════════════════════════════
--
-- Each is short on purpose. A template long enough to be intimidating is one
-- people skip, and a skipped template teaches nothing.
--
-- A section that cannot be filled is information that is missing, not a
-- section to delete: writing "non mesuré" is a result, omitting it is not.

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('template-model-card', 'writeup_template', 'ai', NULL, 'fr',
 'Fiche de modèle',
 'Compatible avec le format HuggingFace, pour que la même fiche serve ici et là-bas.',
$md$
# Fiche de modèle

- **Ce que fait le modèle**, en une phrase, et pour qui.
- **Usage prévu**, et **usages explicitement déconseillés**.
- **Données d'entraînement** : source, volume, période, licence, prétraitement.
- **Procédure d'entraînement** : matériel, durée, hyperparamètres, graine.
- **Évaluation** : jeu de test, métriques, référence de comparaison.
- **Performance par sous-population**, quand la notion s'applique.
- **Limites connues** : ce sur quoi il échoue, écrit par toi.
- **Empreinte** : coût de calcul de l'entraînement, coût d'une inférence.
- **Licence** du modèle et licences amont respectées.
- **Comment citer**.
$md$, 110),

('template-dataset-card', 'writeup_template', 'ai', NULL, 'fr',
 'Fiche de jeu de données',
 'Ce qu''il contient, d''où il vient, et ce qu''on a le droit d''en faire.',
$md$
# Fiche de jeu de données

- **Ce que contient le jeu**, et ce qu'il ne contient pas.
- **Provenance** : d'où viennent les données, comment elles ont été obtenues.
- **Consentement et données personnelles** : base légale, anonymisation.
- **Composition** : taille, distribution des classes, langues, période.
- **Protocole d'annotation** : consignes, nombre d'annotateurs, accord.
- **Découpages fournis** et selon quel axe.
- **Biais connus** : ce qui est sur-représenté et sous-représenté.
- **Licence** et conditions de réutilisation.
- **Maintenance** : qui corrige une erreur signalée, et où la signaler.
$md$, 120),

('template-experiment-report', 'writeup_template', 'ai', NULL, 'fr',
 'Compte rendu d''expérience',
 'La question posée avant de lancer, et ce qui n''a pas marché.',
$md$
# Compte rendu d'expérience

- **Question** : ce que tu cherchais à savoir, écrit avant de lancer.
- **Hypothèse** et ce qui l'aurait réfutée.
- **Protocole** : variantes comparées, ce qui varie et ce qui est figé.
- **Résultats** : le tableau, avec les écarts-types sur plusieurs graines.
- **Interprétation** : ce que les chiffres permettent de conclure, et ce
  qu'ils ne permettent pas.
- **Ce qui n'a pas marché.** La section la plus utile aux suivants, et la
  première à disparaître quand on écrit après coup.
$md$, 130),

('template-benchmark-report', 'writeup_template', 'ai', NULL, 'fr',
 'Rapport de banc d''essai',
 'Ce qui rend un résultat rejouable par un tiers.',
$md$
# Rapport de banc d'essai

- **Banc** : nom, version, jeu de données et découpage exact.
- **Métrique**, unité, et sens de l'amélioration.
- **Références de comparaison** avec leurs sources.
- **Harnais** utilisé et sa version — `lm-evaluation-harness`, `criterion`.
- **Matériel** : machine, carte, quantité de mémoire.
- **Méthode** : chauffe, nombre d'itérations, ce qui est chronométré.
- **Commande exacte** à relancer.
- **Écart attendu** entre deux exécutions.
$md$, 140),

('template-paper-abstract', 'writeup_template', 'ai', NULL, 'fr',
 'Résumé d''article',
 'Ce que ce travail ajoute à ce qui est déjà publié.',
$md$
# Résumé d'article

- **Contexte** en deux phrases.
- **Ce que ce travail ajoute** à ce qui est déjà publié.
- **Méthode**, assez pour comprendre sans lire le code.
- **Résultat principal**, chiffré.
- **Limites**, avant qu'un relecteur ne les trouve.
- **Code et données** : les adresses.
$md$, 150),

('template-rag-design', 'writeup_template', 'ai', NULL, 'fr',
 'Conception d''un système RAG',
 'Le corpus, le découpage, et l''ablation qui dit ce que chaque étage apporte.',
$md$
# Conception d'un système RAG

- **Corpus** : quoi, combien, à quelle fréquence il change.
- **Découpage** : taille des fragments, chevauchement, et pourquoi.
- **Récupération** : lexicale, dense, hybride ; le reclasseur s'il y en a un.
- **Ablation** : ce que chaque étage apporte, mesuré. Sans ça, personne ne
  sait si le reclasseur sert à quelque chose.
- **Jeu d'évaluation** : les questions, et les cas d'échec choisis exprès.
- **Comportement quand rien n'est trouvé.**
- **Coût et latence** par requête.
$md$, 160),

('template-agent-design', 'writeup_template', 'ai', NULL, 'fr',
 'Conception d''un système d''agents',
 'Ce que l''agent peut faire, ce qu''il ne peut pas, et quand il s''arrête.',
$md$
# Conception d'un système d'agents

- **Tâche** et condition d'arrêt.
- **Outils** exposés, et leurs limites de permission.
- **État partagé** : ce qui circule entre les agents.
- **Boucles** : ce qui empêche une exécution infinie.
- **Bac à sable** : ce que l'agent ne peut pas atteindre.
- **Évaluation** : taux de réussite, coût moyen, trace d'une exécution
  complète.
- **Reprise après échec** : ce qui se passe quand un outil renvoie une erreur.
$md$, 170),

('template-red-team-report', 'writeup_template', 'ai', 'safety', 'fr',
 'Rapport de red-team',
 'Le format qu''attend la politique de divulgation.',
$md$
# Rapport de red-team

- **Cible** : modèle, version ou date d'instantané, mode d'accès.
- **Type d'attaque** et pourquoi ce choix.
- **Reproduction** : la procédure, assez précise pour un tiers.
- **Sortie observée**, verbatim.
- **Taux** : réussites sur essais.
- **Gravité** et le raisonnement qui y mène.
- **Atténuation proposée.**
- **Chronologie de divulgation** : notification, accusé de réception, délai
  convenu, publication.
- **Double usage** : ce qui est retenu, et pourquoi.
$md$, 180),

('template-deployment-runbook', 'writeup_template', 'ai', NULL, 'fr',
 'Manuel de mise en service',
 'Ce qu''il faut savoir à trois heures du matin.',
$md$
# Manuel de mise en service

- **Ce qui est déployé** : version du modèle, empreinte, date.
- **Mise en service** : commandes, dépendances, secrets nécessaires.
- **Retour arrière** : la commande, et le temps qu'elle prend.
- **Surveillance** : métriques suivies, seuils, qui est alerté.
- **Dérive** : ce qui la déclenche, et l'action associée.
- **Capacité** : ce que la machine actuelle peut servir.
- **Pannes connues** et leur remède.
$md$, 190),

('template-ai-post-mortem', 'writeup_template', 'ai', NULL, 'fr',
 'Post-mortem d''incident',
 'Sans nommer de coupable : un post-mortem qui en cherche un ne produit plus d''information la fois suivante.',
$md$
# Post-mortem d'incident

- **Ce qui s'est passé**, et ce que les utilisateurs ont vu.
- **Chronologie** : du premier symptôme au rétablissement.
- **Cause** : technique, et ce qui l'a rendue possible.
- **Détection** : comment on l'a su, et combien de temps après.
- **Ce qui a limité les dégâts.**
- **Ce qui les a aggravés.**
- **Actions** : chacune avec un porteur et une échéance.
$md$, 200);
