# Plan 003 : Durcir les pipelines CI/CD, plan technique

| Champ | Valeur |
|---|---|
| Spec | [`spec.md`](./spec.md) |
| Statut | Brouillon, à valider |

> Ce plan décrit **comment** corriger les écarts E1 à E8 du [plan 002](../002-ci-cd/plan.md#6-écarts-constatés). Une fois livré, le plan 002 est mis à jour pour décrire le nouvel existant.

## 1. Vue d'ensemble cible

```
PR / push main ──▶ ci.yml ──┬─ fmt
                            ├─ clippy
                            ├─ msrv (Rust 1.85)                 [E5]
                            ├─ test (ubuntu, macos, windows)
                            └─ perf (ubuntu, --release)         [E6]
                  concurrency: annule l'exécution précédente sur une PR [E7]

tag v* ──▶ release.yml ── check-version ── ci (appel de ci.yml) [E1]
                                              │
                                       create-release (pré-release si suffixe) [E3]
                                     ┌────────┴────────┐
                       binaries: build ▶ smoke ▶ upload   docker: build ▶ smoke ▶ push [E2]
                                                              (latest seulement si stable) [E3]

.github/dependabot.yml ──▶ PR hebdomadaires (actions, cargo) [E8]
toutes les actions épinglées par SHA [E4]
```

## 2. Une seule définition des vérifications [E1]

`ci.yml` reçoit un déclencheur supplémentaire `workflow_call`. `release.yml` l'appelle comme un job :

```yaml
# release.yml
jobs:
  check-version:
    # compare ${GITHUB_REF_NAME#v} à la version de [workspace.package]
  ci:
    needs: check-version
    uses: ./.github/workflows/ci.yml
  create-release:
    needs: ci
```

Le job `check-version` lit la version avec `cargo metadata --no-deps --format-version 1` et `jq` (déjà présents sur les runners), sans dépendance nouvelle.

Choix : appeler le workflow plutôt que d'exiger un statut vert sur le commit. Le statut d'un commit dépend de l'historique (une CI relancée, annulée, ou jamais lancée si le tag vise un commit hors `main`) ; l'appel rejoue les vérifications sur exactement le code tagué (ENF-CI2).

## 3. Vérifier les artefacts avant publication [E2]

### 3.1 Binaires

`taiki-e/upload-rust-binary-action` compile et envoie dans la même étape. On sépare :

1. `cargo build --release --locked --target <cible> -p eyeship-cli` ;
2. script `scripts/smoke.sh <binaire> <version>` :
   - `eyeship --version` contient la version attendue ;
   - `eyeship compose examples/compose.yaml` sort avec le code 1 et sa sortie contient `R013` ;
   - `eyeship compose --format json examples/compose.yaml` produit du JSON valide (`jq -e .`) ;
3. archive, SHA-256 et envoi avec `gh release upload` (même nommage qu'aujourd'hui : `eyeship-<tag>-<cible>`). Si `upload-rust-binary-action` permet de réutiliser un build existant, on la garde ; à vérifier dans sa documentation lors de l'implémentation.

Exécution des cibles croisées : `setup-cross-toolchain-action` configure déjà QEMU pour `aarch64-unknown-linux-musl` (variable `CARGO_TARGET_..._RUNNER`). Le script utilise ce *runner* quand il est défini. `x86_64-apple-darwin` tourne via Rosetta sur `macos-latest` (arm64). Windows lance le script avec `bash` (Git Bash, présent sur le runner).

### 3.2 Image Docker

Avant la poussée, pour chaque plateforme : `docker buildx build --platform <p> --load`, puis `docker run --rm --platform <p> -v "$PWD/examples:/work" <image> compose` avec les mêmes attentes que le script (QEMU est déjà installé dans le job). La poussée réutilise le cache `gha`, donc la reconstruction est quasi gratuite.

## 4. Pré-releases et `latest` [E3]

```yaml
# docker/metadata-action
flavor: |
  latest=false
tags: |
  type=semver,pattern={{version}}
  type=semver,pattern={{major}}.{{minor}}
  type=raw,value=latest,enable=${{ !contains(github.ref_name, '-') }}
```

`type=semver` ignore déjà les pré-releases pour `{{major}}.{{minor}}`. Côté GitHub, `create-gh-release-action` marque une version avec suffixe comme pré-release (option `prerelease` à vérifier dans la documentation de l'action lors de l'implémentation, sinon la passer explicitement avec la même condition).

## 5. Épinglage et mises à jour [E4, E8]

Chaque `uses:` devient `owner/action@<sha40> # vX.Y.Z`. Exception : `dtolnay/rust-toolchain`, dont la version est le nom de branche (`@stable`, `@1.85`). On l'épingle aussi par SHA et on passe la toolchain en entrée `toolchain:`.

`.github/dependabot.yml` :

| Écosystème | Fréquence | Groupement |
|---|---|---|
| `github-actions` | hebdomadaire | toutes les mises à jour mineures et correctives en une PR |
| `cargo` | hebdomadaire | idem ; les majeures en PR séparées |
| `docker` (`Dockerfile`) | hebdomadaire | `rust:1-alpine` épinglé par digest, mis à jour par Dependabot |

Dependabot comprend le format `@sha # vX.Y.Z` et met à jour les deux. Ses PR passent par la CI comme les autres.

## 6. MSRV [E5]

- `rust-version = "1.85"` dans `[workspace.package]`, hérité par chaque crate (`rust-version.workspace = true`). 1.85 est le minimum imposé par l'édition 2024.
- Job `msrv` : toolchain `1.85`, `cargo check --workspace --all-targets --locked`.
- `--locked` : on vérifie le `Cargo.lock` livré. Avec `resolver = "3"`, Cargo choisit déjà des versions compatibles avec `rust-version` quand il résout.

## 7. Performance [E6]

Job `perf` sur `ubuntu-latest` : `cargo test --release -p eyeship-cli --test perf -- --ignored`. Le test garde son budget de 100 ms (spec 001, ENF1). La mémoire (`scripts/bench.sh`, < 20 Mo) est ajoutée au même job : le script échoue si le pic RSS dépasse le budget (modification du script : aujourd'hui il ne fait qu'afficher).

## 8. Concurrence [E7]

```yaml
concurrency:
  group: ci-${{ github.workflow }}-${{ github.event.pull_request.number || github.sha }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}
```

Sur `main`, le groupe est le SHA : rien n'est annulé. Le bloc est ignoré quand `ci.yml` est appelé par la release (seul le `concurrency` de l'appelant compte).

## 9. Décisions à valider

| # | Décision | Recommandation | Alternative |
|---|---|---|---|
| D1 | Garde de la release | **Appeler `ci.yml` (`workflow_call`)** | Exiger un statut vert sur le commit via l'API |
| D2 | Smoke test des cibles croisées | **QEMU / Rosetta sur le runner** | Ne tester que les cibles natives (3 sur 5) |
| D3 | Outil de mise à jour | **Dependabot** (natif, sans installation) | Renovate (plus souple, application à installer) |
| D4 | Perf en CI | **Bloquant**, marge actuelle ×15 | Informatif (`continue-on-error`) le temps d'observer la stabilité |
| D5 | Image de build Docker | **`rust:1-alpine` épinglée par digest** | Laisser `rust:1-alpine` flottante |

## 10. Risques

| Risque | Parade |
|---|---|
| Un SHA épinglé pointe vers un commit supprimé ou réécrit | Dependabot signale ; la CI échoue de façon visible, pas silencieuse. |
| Le smoke test sous QEMU est lent ou instable | Un seul audit d'un petit fichier ; repli D2 si besoin. |
| Les PR Dependabot font du bruit | Groupement et fréquence hebdomadaire. |
| Le test perf échoue sur un runner chargé | Le test prend déjà le meilleur de 5 exécutions ; repli D4. |
