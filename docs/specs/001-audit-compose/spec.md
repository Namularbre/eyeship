# Spec 001 : Audit de fiabilité d'un projet Docker Compose

| Champ | Valeur |
|---|---|
| Statut | Validé le 2026-11-15 |
| Date | 2026-11-14 |
| Constitution | [`docs/constitution.md`](../../constitution.md) |

> Cette spec décrit **quoi** et **pourquoi**. Les choix techniques sont dans [`plan.md`](./plan.md).

## 1. Problème

J'ai besoin d'un outil qui audite un projet Docker Compose pour vérifier sa fiabilité.

Une stack Compose peut être syntaxiquement valide (`docker compose config` passe) et pourtant tomber en production pour des raisons prévisibles :

- l'API démarre avant que la base de données soit prête et plante au démarrage ;
- un conteneur qui crashe n'est jamais relancé ;
- un service fuit en mémoire, sature l'hôte et entraîne les autres dans sa chute ;
- les logs remplissent le disque du VPS en quelques semaines ;
- une variable d'environnement manquante donne une chaîne vide au lieu d'une erreur ;
- une image `latest` change sans prévenir entre deux déploiements ;
- deux services veulent le même port et le second ne démarre pas.

Par **fiabilité**, on entend :

- pas d'erreur de configuration (réseaux, volumes, dépendances, variables) ;
- pas de conflit de ports ;
- pas d'image à version flottante ;
- des services qui redémarrent, attendent leurs dépendances et restent dans leurs limites de ressources ;
- les bonnes pratiques Docker Compose respectées ;
- pas de faille de sécurité (optionnel : beaucoup d'outils le font déjà, hors MVP).

## 2. Persona

**Jean** est développeur d'un SaaS qu'il déploie sur un VPS avec Docker Compose. Il n'a pas d'équipe ops. Il veut savoir si son déploiement est fiable et avoir un retour sur les bonnes pratiques, en local avant de pousser, puis automatiquement dans sa CI.

## 3. Récits utilisateur

### US1 (P1) : Auditer un fichier Compose en local
*En tant que Jean, je lance une commande dans mon projet et j'obtiens la liste des risques de plantage, triés par gravité, avec la correction à appliquer.*

Critères d'acceptation :
1. **Étant donné** un dossier contenant `compose.yaml`, `compose.yml`, `docker-compose.yaml` ou `docker-compose.yml`, **quand** je lance `eyeship compose` sans argument, **alors** le fichier est détecté et analysé.
2. **Étant donné** un chemin explicite, **quand** je lance `eyeship compose chemin/vers/fichier.yml`, **alors** ce fichier est analysé.
3. **Quand** des problèmes sont trouvés, **alors** chaque constat affiche : identifiant de règle, sévérité, service concerné, fichier et ligne, explication courte et correction proposée.
4. **Quand** aucun problème n'est trouvé, **alors** l'outil l'indique clairement et sort avec le code `0`.
5. Un résumé final donne le nombre de constats par sévérité.

### US2 (P1) : Bloquer un pipeline CI
*En tant que Jean, je veux que ma CI échoue si des risques au-dessus d'un seuil sont détectés.*

Critères d'acceptation :
1. Par défaut, l'outil sort en code `1` si au moins un constat de sévérité `error` est trouvé.
2. `--fail-on <error|warning|info|none>` change ce seuil.
3. Le code `2` est réservé aux erreurs d'utilisation ou de lecture (fichier absent, YAML invalide), pour les distinguer d'un audit qui trouve des problèmes.
4. `--format json` produit une sortie JSON stable et documentée, sans texte parasite sur la sortie standard.
5. La sortie est déterministe (même ordre à chaque exécution).

### US3 (P1) : Comprendre une règle
*En tant que Jean, je veux savoir pourquoi une règle existe et comment la corriger.*

Critères d'acceptation :
1. `eyeship rules` liste toutes les règles avec identifiant, sévérité par défaut et titre.
2. `eyeship explain <ID>` affiche le scénario de panne, la correction, un exemple fautif et un exemple corrigé.

### US4 (P2) : Ignorer volontairement une règle
*En tant que Jean, je veux désactiver une règle qui ne s'applique pas à mon contexte, de façon traçable.*

Critères d'acceptation :
1. `--ignore R004,R005` désactive des règles pour l'exécution.
2. Un fichier `.eyeship.toml` à la racine permet d'ignorer des règles globalement ou pour un service, et de changer le seuil d'échec.
3. Les constats ignorés sont comptés dans le résumé (« 3 constats ignorés par configuration »), jamais supprimés silencieusement.

### US5 (P2) : Auditer une stack en plusieurs fichiers
*En tant que Jean, j'utilise un fichier de base et une surcharge de production (`compose.yaml` + `compose.prod.yaml`) et je veux auditer le résultat fusionné.*

Critères d'acceptation :
1. `-f` est répétable, comme dans `docker compose`, et les fichiers sont fusionnés selon les règles de Compose.
2. `--env-file` permet de fournir les variables de l'environnement cible (par exemple `.env.prod`).
3. Un constat pointe vers le fichier d'origine de la valeur fautive.

## 4. Règles du MVP

Sévérités : `error` (plantage très probable ou configuration invalide), `warning` (risque réel en production), `info` (bonne pratique ou style).

### Dépendances et démarrage

| ID | Titre | Sév. | Scénario de panne prévenu |
|---|---|---|---|
| R001 | `depends_on` sans condition de santé | warning | Forme courte (liste) ou `condition: service_started` : le service dépendant démarre dès que la dépendance est *lancée*, pas *prête*. Connexion refusée et crash au démarrage. |
| R002 | `depends_on` vers un service sans healthcheck | error | `condition: service_healthy` vers un service sans `healthcheck` (ou avec `disable: true`) : le déploiement échoue. |
| R003 | Dépendance invalide | error | `depends_on` vers un service inexistant, ou cycle de dépendances : la stack ne peut pas démarrer. |
| R004 | Healthcheck incohérent | warning | `timeout` supérieur ou égal à `interval`, ou `retries: 0` : le service est marqué malsain à tort ou jamais détecté malsain. |

### Résilience et ressources

| ID | Titre | Sév. | Scénario de panne prévenu |
|---|---|---|---|
| R005 | Pas de politique de redémarrage | warning | `restart` absent ou `"no"` : un crash isolé laisse le service mort jusqu'à une intervention manuelle. `on-failure`, `always` et `unless-stopped` sont acceptés. Exception : services ponctuels attendus via `service_completed_successfully`. |
| R006 | Pas de limite mémoire | warning | Ni `mem_limit` ni `deploy.resources.limits.memory` : une fuite mémoire sature l'hôte et l'OOM killer peut tuer d'autres services. |
| R007 | Logs sans rotation | warning | Driver `json-file` (par défaut) sans `max-size` : les logs grossissent sans limite jusqu'à remplir le disque. |

### Images

| ID | Titre | Sév. | Scénario de panne prévenu |
|---|---|---|---|
| R008 | Image non épinglée | warning | Image sans tag ou en `:latest` : la version déployée change sans modification du fichier. Ne déclenche pas si le service a un `build` et pas d'`image`. |

### Réseau et ports

| ID | Titre | Sév. | Scénario de panne prévenu |
|---|---|---|---|
| R009 | Conflit de ports publiés | error | Deux services publient le même port hôte (même IP et protocole), plages comprises : le second ne démarre pas. |
| R010 | Réseau non déclaré | error | Un service utilise un réseau absent de la section `networks` racine (hors réseaux par défaut et `external`). |

### Volumes, fichiers et variables

| ID | Titre | Sév. | Scénario de panne prévenu |
|---|---|---|---|
| R011 | Volume non déclaré ou malformé | error | Volume nommé absent de la section `volumes` racine, ou syntaxe invalide (`source:target:mode` incorrecte, mode inconnu). |
| R012 | Chemin hôte introuvable | warning | Bind mount, `env_file`, secret ou config de type fichier pointant vers un chemin relatif absent : Docker crée un dossier vide à la place, ou le démarrage échoue. |
| R013 | Variable d'environnement non définie | error | `${VAR}` sans valeur par défaut, absente de l'environnement et du `.env` : Compose la remplace par une chaîne vide et l'application démarre mal configurée. |

### Conteneurs et bonnes pratiques

| ID | Titre | Sév. | Scénario de panne prévenu |
|---|---|---|---|
| R014 | Nom de conteneur invalide | error | `container_name` qui ne respecte pas le format Docker (`[a-zA-Z0-9][a-zA-Z0-9_.-]*`) : le conteneur ne peut pas être créé. |
| R015 | Nom de conteneur fixe | info | `container_name` empêche de scaler le service et entre en conflit si deux projets tournent sur le même hôte. |
| R016 | Nom de conteneur hors kebab-case | info | Convention de nommage du projet (`mon-service`). Règle de style, désactivable. |
| R017 | Clé `version` obsolète | info | La clé racine `version` est ignorée par Compose v2 et affiche un avertissement : à supprimer. |

Chaque règle a au moins un exemple fautif et un exemple correct dans sa documentation (`eyeship explain`).

## 5. Exigences non fonctionnelles

- **ENF1** : analyse d'un fichier de 50 services en moins de 100 ms et moins de 20 Mo de RAM.
- **ENF2** : aucun accès réseau, aucun démon Docker requis.
- **ENF3** : un seul binaire pour Linux, macOS et Windows, plus une image Docker minimale.
- **ENF4** : messages de l'outil en anglais (cf. constitution), documentation du projet en français.

## 6. Critères de succès du MVP

- Les 17 règles sont implémentées, chacune avec tests positifs et négatifs.
- Sur un corpus d'au moins 10 fichiers Compose publics réels, aucun faux positif de sévérité `error` après revue manuelle.
- L'outil s'intègre dans un pipeline GitHub Actions en une ligne de commande.
- Jean comprend et corrige un constat sans lire de documentation externe.

## 7. Hors périmètre du MVP

| Idée | Pourquoi plus tard |
|---|---|
| Failles de sécurité | Déjà bien couvert (Trivy, Dockle...). À intégrer en agrégeant ces outils. |
| Images inconnues ou peu fiables | Deux pistes. **Hors ligne** : intégrer à la compilation la liste publique des Docker Official Images (dépôt GitHub `docker-library/official-images`, qui liste aussi tags et variantes). Une image officielle n'a pas de préfixe (`python:3.12`) ; une image dont l'éditeur n'est pas reconnu donne un constat `info`. Cette même liste sert à la règle « image non optimisée ». **En ligne** (mode `--online`, constitution P3) : interroger Docker Hub pour les badges Verified Publisher et Sponsored OSS, et vérifier que le tag existe. Étoiles et téléchargements ne sont pas retenus : mauvais indicateurs de confiance. |
| Port ouvert pour rien (aucun processus n'écoute) | Impossible à savoir sans lancer le conteneur. Piste partielle : comparer avec les `EXPOSE` du Dockerfile quand le service a un `build`. |
| Image non optimisée (variante plus légère disponible) | Ne fait pas planter en soi : gain de taille, de temps de pull et de surface d'attaque. Si on l'ajoute : sévérité `info`, liste intégrée d'images officielles connues (python, node, ruby, golang...) pour rester hors ligne, et suggestion `-slim` plutôt qu'`-alpine`, car musl peut casser des applications (paquets Python compilés, DNS). |
| Limite CPU absente | Règle `info` ou `warning` à ajouter après le MVP, sur le modèle de R006 (Q3). |
| Dockerfile, Kubernetes | Specs suivantes. |
| Dashboard web | Après la CLI ; l'architecture hexagonale le prépare. |
| Correction automatique (`--fix`), sortie SARIF, ignore en commentaire YAML | Juste après le MVP. |

## 8. Questions ouvertes

- **Q1** *(résolue)* : « tag non optimisé » = proposer une variante plus légère d'une image (`python` → `python:slim`). Reportée après le MVP, voir section 7.
- **Q2** *(résolue)* : l'outil s'appelle `eyeship` (prononcé « I ship »). Libre sur crates.io et Docker Hub, aucun dépôt GitHub de ce nom au 2026-11-14.
- **Q3** *(résolue)* : R006 ne vérifie que la mémoire. Une règle de limite CPU (`info` ou `warning`) viendra après le MVP.
- **Q4** *(résolue)* : R016 (kebab-case) est activée par défaut en `info`, sans effet sur le code de sortie. L'utilisateur la désactive s'il préfère une autre convention.
