# Corpus de fichiers Compose réels

Fichiers Compose publics, copiés tels quels (données de test uniquement), pour
vérifier le critère de succès de la spec 001 (§6) : aucun faux positif de
sévérité `error` après revue manuelle. Le test
`crates/eyeship-cli/tests/corpus.rs` lance `eyeship compose` sur chacun (avec un
environnement vide), vérifie que le chargement réussit et fige la sortie JSON
dans un snapshot `insta`. Tout nouveau constat sur ce corpus doit être relu.

Seul le fichier Compose est copié : les fichiers voisins (configurations,
scripts, `.env`) sont absents, donc les R012 (chemin hôte introuvable) et R013
(variable non définie, `.env` à créer depuis un `.env.example`) sont attendus.
Exception : `sentry-self-hosted/.env`, versionné dans le dépôt d'origine.

Téléchargés le 2026-11-15 depuis `raw.githubusercontent.com`.

| Dossier | Source | Licence du dépôt d'origine |
|---|---|---|
| `awesome-nginx-golang-mysql` | `docker/awesome-compose/master/nginx-golang-mysql/compose.yaml` | CC0-1.0 |
| `awesome-react-express-mongodb` | `docker/awesome-compose/master/react-express-mongodb/compose.yaml` | CC0-1.0 |
| `awesome-wordpress-mysql` | `docker/awesome-compose/master/wordpress-mysql/compose.yaml` | CC0-1.0 |
| `awesome-prometheus-grafana` | `docker/awesome-compose/master/prometheus-grafana/compose.yaml` | CC0-1.0 |
| `awesome-flask-redis` | `docker/awesome-compose/master/flask-redis/compose.yaml` | CC0-1.0 |
| `awesome-elasticsearch-logstash-kibana` | `docker/awesome-compose/master/elasticsearch-logstash-kibana/compose.yaml` | CC0-1.0 |
| `immich` | `immich-app/immich/main/docker/docker-compose.yml` | AGPL-3.0 |
| `sentry-self-hosted` | `getsentry/self-hosted/master/docker-compose.yml` et `.env` | FSL (voir le dépôt) |
| `mastodon` | `mastodon/mastodon/main/docker-compose.yml` | AGPL-3.0 |
| `supabase` | `supabase/supabase/master/docker/docker-compose.yml` | Apache-2.0 |
| `plausible` | `plausible/community-edition/v2.1.4/compose.yml` | AGPL-3.0 |
| `n8n` | `n8n-io/n8n-hosting/main/docker-compose/withPostgres/docker-compose.yml` | voir le dépôt |
| `outline` | `outline/outline/main/docker-compose.yml` | BSL-1.1 |
| `nextcloud` | `nextcloud/docker/master/.examples/docker-compose/with-nginx-proxy/postgres/fpm/compose.yaml` | voir le dépôt (GPL) |
| `paperless-ngx` | `paperless-ngx/paperless-ngx/main/docker/compose/docker-compose.postgres.yml` | GPL-3.0 |
| `authentik` | `goauthentik/authentik/main/lifecycle/container/compose.yml` | MIT (hors dossiers entreprise) |
| `traefik-whoami` | `traefik/traefik/master/docs/content/user-guides/docker-compose/basic-example/docker-compose.yml` | MIT |

URL complète : `https://raw.githubusercontent.com/<source>`.

## Revue des constats `error` (2026-11-15)

- **R013** (immich, n8n, plausible, supabase, authentik) : variables fournies
  par un `.env` absent du dépôt (chaque projet livre un `.env.example`). Vrai
  risque de mauvaise configuration si on déploie sans `.env` : attendu.
- **Faux positifs corrigés** : un plantage du chargeur sur Sentry (heuristique
  anti « alias bomb » du parseur YAML, déclenchée par 134 alias `<<: *defaults`
  pour 10 ancres), et des constats en cascade quand une variable non résolue
  vide une valeur (`${UPLOAD_LOCATION}:/data` signalé R011 chez immich ; même
  mécanisme pour R003, R010, R014 et R008). Seul R013 signale désormais la
  variable.

## Avertissements discutables

- **R004** `timeout` égal à `interval` (supabase, n8n, sentry, awesome-elk) :
  Docker attend `interval` *après la fin* de la sonde précédente, donc ce
  réglage très courant n'est pas incohérent en soi.
- **R008** sur une image locale construite (`build` + `image`, sentry) : le nom
  désigne le résultat du build, pas une image tirée d'un registre.
