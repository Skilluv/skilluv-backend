# Faire tourner les tests sans Docker

La suite d'intégration n'a jamais eu besoin de Docker : elle a besoin d'un
PostgreSQL et d'un Redis joignables. Le `docker-compose.yml` est une façon
commode de les fournir, pas une dépendance.

Sur une machine où Docker Desktop coûte cher — mémoire de pagination saturée,
build tué en plein milieu — les services natifs font le même travail pour
beaucoup moins.

---

## Ce qui est réellement requis

| Service | Requis pour | Sans lui |
|---|---|---|
| PostgreSQL | tout | rien ne démarre : `TestApp::spawn` crée une base par test |
| Redis | tout | rien ne démarre : le cache et les compteurs sont câblés au démarrage |
| MinIO | téléversements uniquement | toléré : `StorageService` n'échoue jamais au démarrage |
| Mailpit | tests qui lisent un e-mail | toléré : l'envoi échoue, le test qui n'en lit pas passe |
| Judge0 | tests de bac à sable | toléré de la même façon |

Autrement dit : **Postgres et Redis suffisent** pour la très grande majorité
des suites.

## Postgres natif

Le harnais se connecte à une base `skilluv` avec le rôle `skilluv`, puis crée
et supprime une base par test — il lui faut donc `CREATEDB`.

Une fois, en superutilisateur :

```
psql -U postgres -h 127.0.0.1 \
  -c "CREATE ROLE skilluv LOGIN CREATEDB PASSWORD 'skilluv_secret';" \
  -c "CREATE DATABASE skilluv OWNER skilluv;"
```

Le mot de passe n'a rien de sensible : il ne vaut que sur cette machine et
c'est déjà celui du `docker-compose.yml`. Garder les deux identiques évite
d'avoir à se souvenir duquel on parle.

## Pointer la suite dessus

Le harnais lit `TEST_DATABASE_BASE_URL`, et vaut par défaut le port 5433 du
conteneur. Postgres natif écoute sur 5432 :

```
$env:TEST_DATABASE_BASE_URL = 'postgres://skilluv:skilluv_secret@localhost:5432'
```

Pour ne pas le refaire à chaque session :

```
[Environment]::SetEnvironmentVariable(
  'TEST_DATABASE_BASE_URL',
  'postgres://skilluv:skilluv_secret@localhost:5432',
  'User')
```

La variable existe précisément pour ça — le port par défaut est facile à
masquer, et le harnais fait des `CREATE DATABASE` / `DROP DATABASE` sur ce qui
répond. Le pointer explicitement ne doit pas demander de modifier du code.

## Redis natif

Rien à configurer : le harnais attend `redis://localhost:6379` et s'attribue
une base parmi les seize selon son PID, pour que deux binaires de test en
parallèle ne s'écrasent pas.

## Lancer

Un fichier à la fois. La suite complète en une passe fait tomber la machine
avant de tomber elle-même :

```
cargo test -j 2 --test test_ai_catalogue -- --test-threads=2
```

`-j 2` limite les éditions de liens simultanées, qui sont ce qui consomme la
mémoire. `--test-threads=2` limite les bases créées en même temps.

## Ce à quoi faire attention

**Le collationnement.** Le conteneur crée son cluster en `--locale=C`, une
installation Windows non. Les bases de test héritent de `template1`, donc du
cluster. Un test qui vérifie un ordre alphabétique sur des chaînes accentuées
peut passer ici et échouer en CI, ou l'inverse. Aucun ne le fait aujourd'hui ;
si cela arrive, c'est la première chose à regarder.

**La version.** Le compose épingle PostgreSQL 18.4. Une installation native
plus ancienne peut refuser une migration qui utilise une syntaxe récente. `psql
--version` avant de conclure à un bug.
