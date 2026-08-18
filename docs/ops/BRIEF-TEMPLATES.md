# Modèles de brief — ops

Cinq modèles, un par famille de relecture. Un brief ops qui ne remplit pas ces
champs produit une mission où personne ne sait quand elle est finie.

Le champ le plus important de chacun est le même : **comment on saura que
c'est fait**. Dans ce domaine, « ça marche » est une opinion tant que personne
n'a dit ce que marcher veut dire.

---

## 1. Infra — module, chart, pipeline

```
Titre :
Sous-type : iac_terraform | kubernetes_manifests | cicd_pipeline
Plateformes cibles : aws | gcp | azure | on-prem | multi
Outillage : terraform, helm, argocd, …

Ce qui existe aujourd'hui :
  (l'état actuel, y compris ce qui est fait à la main)

Ce qu'il faut construire :

Contraintes non négociables :
  - (versions imposées, conformité, réseau existant)

Comment on saura que c'est fait :
  - le module s'applique deux fois de suite sans différence
  - il se détruit sans laisser d'orphelin
  - quelqu'un d'autre l'utilise en suivant le README seul

Ce qui reste hors périmètre :
```

## 2. Fiabilité — objectif de service, runbook, résilience

```
Titre :
Service concerné :
Ce que le service fait, en une phrase que son utilisateur reconnaîtrait :

Objectif proposé :
  - cible : ____ %
  - fenêtre : ____ jours
  - mesuré par : (la source, nommée, accessible au relecteur)

Ce qui casse aujourd'hui :
  (les incidents connus, ou l'absence d'historique — dire lequel)

Comment on saura que c'est fait :
  - la cible est mesurée en continu et lisible sans demander à personne
  - une alerte existe avant que l'utilisateur ne s'en aperçoive
  - un runbook couvre les trois modes de panne connus

Astreinte : incluse | non incluse
  (si incluse : plage horaire, délai de réponse, rémunération)
```

## 3. Cloud — conception, coûts, région

```
Titre :
Fournisseurs envisagés :
Régions envisagées, et pourquoi :

La charge attendue :
  - au repos :
  - au pic :
  - la croissance prévue sur douze mois :

Le budget :
  - plafond mensuel :
  - ce qui compte comme dépassement :

Reprise :
  - RTO visé :
  - RPO visé :

Comment on saura que c'est fait :
  - une architecture chiffrée, avec la facture estimée poste par poste
  - les compromis écrits, y compris l'enfermement accepté
  - un test de reprise joué et documenté
```

## 4. Observabilité — instrumentation, alertes, tableaux

```
Titre :
Périmètre : (quels services, quelles couches)
Pile en place : (prometheus, otel, loki, grafana, …)

Les questions auxquelles il faut pouvoir répondre :
  1.
  2.
  3.
  (si la liste est vide, la mission n'est pas prête)

Qui est réveillé, et quand :

Comment on saura que c'est fait :
  - chacune des questions ci-dessus se répond en moins de deux minutes
  - chaque alerte pointe vers ce qu'il faut faire
  - la facture d'ingestion est connue et tenue

Rétention décidée : ____ jours, parce que ____
```

## 5. Données — migration, réglage, réplication

```
Titre :
Moteur et version :
Volume de la table ou de la base concernée :
Fenêtre d'intervention possible :

Ce qui pose problème aujourd'hui :
  (avec la mesure : latence, verrous, taille, décalage de réplication)

Ce qu'il faut obtenir :

Comment on saura que c'est fait :
  - le plan de requête avant et après, sur des volumes réalistes
  - la durée du verrou mesurée, et sous le seuil convenu
  - la restauration testée depuis la sauvegarde prise avant

Retour en arrière :
  - possible : oui | non
  - si non : la sauvegarde vérifiée avant, et par qui
```

---

## Les trois champs que personne ne remplit et qui coûtent le plus

**« Ce qui existe aujourd'hui »** — sauté parce que le client le connaît. Le
contributeur ne le connaît pas, et découvre à mi-mission que la moitié est
faite à la main.

**« Ce qui reste hors périmètre »** — sauté parce qu'il semble négatif. C'est
le champ qui évite la conversation où le périmètre a grandi sans que personne
ne l'ait décidé.

**« Comment on saura que c'est fait »** — sauté parce que ça paraît évident.
Ce n'est jamais évident, et sans lui la fin de mission est une négociation.
