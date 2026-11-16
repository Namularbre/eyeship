# eyeship

> *eyeship*, prononcé « I ship » : l'œil qui surveille (*eye*), le navire (*ship*) et « je livre en prod » (*I ship*).

`eyeship` audite vos fichiers Docker Compose et trouve, **avant la mise en production**, ce qui risque de faire tomber votre application : dépendance démarrée avant que la base soit prête, conteneur sans limite mémoire, logs qui remplissent le disque, port publié deux fois, variable non définie…

- Analyse **statique et hors ligne** : aucun conteneur lancé, aucun démon Docker, aucun réseau.
- Chaque constat dit **où** (fichier, service, ligne), **pourquoi** et **comment corriger**.
- Pensé pour la **CI** : codes de sortie stables, sortie JSON, seuil d'échec configurable, résultat déterministe.
- Un seul binaire statique (quelques Mo), rapide : 50 services audités en ~6 ms et ~5 Mo de RAM.

## Installation

**Binaires** : téléchargez l'archive de votre plateforme (Linux musl x86_64/aarch64, macOS x86_64/aarch64, Windows x86_64) depuis les [releases GitHub](https://github.com/Namularbre/eyeship/releases).

**Depuis les sources** (Rust ≥ 1.85) :

```sh
cargo install --git https://github.com/Namularbre/eyeship eyeship-cli
```

**Docker** (image `scratch` de quelques Mo) :

```sh
docker run --rm -v "$PWD:/work" ghcr.io/namularbre/eyeship compose
```

## Utilisation

```sh
eyeship compose                                 # cherche compose.yaml, docker-compose.yml… dans le dossier courant
eyeship compose infra/compose.yaml              # un fichier précis
eyeship compose -f compose.yaml -f compose.prod.yaml   # plusieurs fichiers, fusionnés dans l'ordre
eyeship compose --env-file .env.prod            # variables pour l'interpolation
eyeship compose --format json                   # sortie machine
eyeship compose --fail-on warning               # seuil d'échec : error (défaut), warning, info, none
eyeship compose --ignore R015,R016              # ignorer des règles (les constats ignorés restent comptés)
eyeship rules                                   # liste des règles
eyeship explain R001                            # scénario de panne, correction, exemples
```

### Fichier `.eyeship.toml`

Placé dans le dossier où l'on lance `eyeship`, il rend le silence explicite et versionné :

```toml
fail_on = "warning"          # seuil d'échec
ignore = ["R015", "R016"]    # pour tous les services

[services.api]
ignore = ["R006"]            # pour un seul service
```

### Codes de sortie

| Code | Signification |
|---|---|
| 0 | Aucun constat au niveau du seuil ou au-dessus |
| 1 | Au moins un constat au niveau du seuil ou au-dessus |
| 2 | Erreur d'utilisation ou de lecture (fichier absent, YAML invalide) |

## Exemple

Sur [`examples/compose.yaml`](examples/compose.yaml) :

```text
$ eyeship compose examples/compose.yaml
api
  error   R013 examples/compose.yaml:21:21
          variable 'DB_PASSWORD' is not set and has no default
          fix: define DB_PASSWORD in the environment or in .env, or use ${DB_PASSWORD:-default} or ${DB_PASSWORD:?message}
  warning R008 examples/compose.yaml:15:12
          image 'ghcr.io/acme/api:latest' is not pinned to a version and may change on the next pull
          fix: Pin 'acme/api' to an explicit version tag or a digest (@sha256:...).
  warning R001 examples/compose.yaml:17:9
          depends_on 'db' does not wait for the service to be healthy
          fix: Use `condition: service_healthy` for 'db' and give it a healthcheck.

worker
  error   R009 examples/compose.yaml:28:9
          host port 8080/tcp is already published by service 'api' (8080/tcp)
          fix: Publish a different host port, bind to a distinct host IP, or remove one of the two mappings of service 'api' and 'worker'.
  error   R010 examples/compose.yaml:29:16
          network 'backend' is not declared in the root 'networks' section
          fix: Declare 'backend' under the root 'networks' key (add 'external: true' if it is created outside this project), or fix the network name.

db
  error   R011 examples/compose.yaml:35:9
          named volume 'pgdata' is not declared in the root 'volumes' section
          fix: Declare 'pgdata' under the root 'volumes' key (add 'external: true' if it is created outside this project), or fix the volume name.

4 errors, 2 warnings, 0 info
$ echo $?
1
```

## Intégration GitHub Actions

```yaml
- run: docker run --rm -v "$PWD:/work" ghcr.io/namularbre/eyeship compose --fail-on warning
```

## Règles

| ID | Sévérité | Règle |
|---|---|---|
| R001 | warning | depends_on without health condition |
| R002 | error | depends_on towards a service without healthcheck |
| R003 | error | Invalid dependency |
| R004 | warning | Inconsistent healthcheck |
| R005 | warning | No restart policy |
| R006 | warning | No memory limit |
| R007 | warning | Logs without rotation |
| R008 | warning | Image not pinned |
| R009 | error | Published port conflict |
| R010 | error | Undeclared network |
| R011 | error | Undeclared or malformed volume |
| R012 | warning | Missing host path |
| R013 | error | Undefined environment variable |
| R014 | error | Invalid container name |
| R015 | info | Fixed container name |
| R016 | info | Container name not in kebab-case |
| R017 | info | Obsolete version key |

`eyeship explain <ID>` détaille chacune : le problème, le scénario de panne, la correction, un exemple fautif et un exemple corrigé. Les identifiants sont stables : une règle peut être dépréciée, jamais renumérotée.

## Développement

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --release -p eyeship-cli --test perf -- --ignored   # ENF1 : < 100 ms pour 50 services
scripts/bench.sh                                               # temps et RSS max (GNU time requis)
docker build -t eyeship .
```

La démarche (constitution, spec, plan, tâches) est décrite dans [`docs/`](docs/). Documentation en français, messages de l'outil en anglais.
