# Plan 002 : Pipelines CI/CD, plan technique

| Champ | Valeur |
|---|---|
| Spec | [`spec.md`](./spec.md) |
| Statut | Rétro-documentation de l'existant, 2026-11-16 |

> Ce plan décrit **comment** les pipelines sont réalisés aujourd'hui. Toute modification d'un workflow commence par une mise à jour de ce plan (constitution §8).

## 1. Vue d'ensemble

Deux workflows GitHub Actions, dans `.github/workflows/` :

| Workflow | Fichier | Déclencheur | Rôle |
|---|---|---|---|
| CI | [`ci.yml`](../../../.github/workflows/ci.yml) | push sur `main` ou `master`, toute PR | Vérifier (US1) |
| Release | [`release.yml`](../../../.github/workflows/release.yml) | push d'un tag `v*` | Livrer binaires et image (US2, US3) |

```
PR / push main ──▶ CI ──┬─ fmt
                        ├─ clippy
                        └─ test (ubuntu, macos, windows)

tag v* ──▶ Release ── create-release ──┬─ binaries (5 cibles) ──▶ release GitHub
                                       └─ docker (amd64, arm64) ──▶ ghcr.io
```

## 2. Workflow CI (`ci.yml`)

Variables globales :
- `CARGO_TERM_COLOR: always` : sortie lisible dans les logs.
- `RUSTFLAGS: -D warnings` : tout avertissement du compilateur est une erreur, dans **tous** les jobs.

| Job | Machine | Étapes | Cache |
|---|---|---|---|
| `fmt` | ubuntu-latest | toolchain stable + `rustfmt`, `cargo fmt --all --check` | non (inutile, pas de compilation) |
| `clippy` | ubuntu-latest | toolchain stable + `clippy`, `cargo clippy --workspace --all-targets -- -D warnings` | `Swatinem/rust-cache@v2` |
| `test` | matrice ubuntu / macos / windows | toolchain stable, `cargo test --workspace` | `Swatinem/rust-cache@v2` |

Choix :
- **Trois jobs indépendants**, sans `needs` : ils tournent en parallèle, un problème de formatage ne masque pas un test cassé.
- **`--all-targets`** pour clippy : les tests et les benchs sont lintés aussi.
- **`fail-fast: false`** sur la matrice de test : un échec Windows n'annule pas macOS (US1). C'est ce qui a permis de repérer les problèmes de séparateurs de chemin.
- **Toolchain `stable`** via `dtolnay/rust-toolchain`, pas de version figée : la CI suit la dernière stable.
- Les tests snapshot (`insta`) échouent en CI sans réécrire les références, car `CI=true` est défini par GitHub Actions.
- Le test de performance `tests/perf.rs` est marqué `#[ignore]` : il ne tourne pas dans `cargo test --workspace`.

## 3. Workflow Release (`release.yml`)

Permissions par défaut : `contents: write` (création de la release, envoi des archives). Seul `secrets.GITHUB_TOKEN` est utilisé.

### 3.1 `create-release`

`taiki-e/create-gh-release-action@v1` crée la release GitHub associée au tag. Les deux jobs suivants en dépendent (`needs: create-release`).

### 3.2 `binaries`

Matrice `fail-fast: false` :

| Cible | Machine | Archive |
|---|---|---|
| `x86_64-unknown-linux-musl` | ubuntu-latest | `.tar.gz` |
| `aarch64-unknown-linux-musl` | ubuntu-latest (compilation croisée) | `.tar.gz` |
| `x86_64-apple-darwin` | macos-latest (compilation croisée depuis arm64) | `.tar.gz` |
| `aarch64-apple-darwin` | macos-latest | `.tar.gz` |
| `x86_64-pc-windows-msvc` | windows-latest | `.zip` |

Étapes :
1. Toolchain stable (`dtolnay/rust-toolchain`).
2. Sur Linux uniquement : `taiki-e/setup-cross-toolchain-action@v1` installe la chaîne musl et, pour aarch64, la compilation croisée.
3. `taiki-e/upload-rust-binary-action@v1` compile le binaire `eyeship` en release (profil du workspace : `lto`, `codegen-units = 1`, `strip`, `panic = "abort"`), l'archive sous le nom `eyeship-<tag>-<cible>`, calcule le SHA-256 et envoie le tout sur la release.

### 3.3 `docker`

Permissions réduites au job : `contents: read`, `packages: write`.

1. `docker/setup-qemu-action@v3` et `docker/setup-buildx-action@v3` : construction multi-architecture.
2. `docker/login-action@v3` sur `ghcr.io` avec `GITHUB_TOKEN`.
3. `docker/metadata-action@v5` : tags `{{version}}`, `{{major}}.{{minor}}` et `latest`, image `ghcr.io/<owner>/<repo>` (mise en minuscules par l'action).
4. `docker/build-push-action@v6` : construit le [`Dockerfile`](../../../Dockerfile) pour `linux/amd64` et `linux/arm64`, pousse l'image, cache de couches `type=gha`.

Le `Dockerfile` est en deux étapes :
- `rust:1-alpine` + `musl-dev` : `cargo build --release --locked -p eyeship-cli` (le `--locked` garantit le respect de `Cargo.lock`) ;
- `FROM scratch` : uniquement `/eyeship`, `WORKDIR /work`, `ENTRYPOINT ["/eyeship"]`, `CMD ["--help"]`.

Le `.dockerignore` exclut tout sauf `Cargo.toml`, `Cargo.lock` et `crates/` : le contexte de build est minimal.

Note : le binaire de l'image est compilé par le `Dockerfile`, indépendamment du job `binaries`. Les deux partent du même commit et du même `Cargo.lock`, mais ce ne sont pas les mêmes octets.

## 4. Procédure de release

1. Mettre à jour `version` dans `[workspace.package]` du `Cargo.toml` racine.
2. Fusionner sur `main` et attendre que la CI soit verte.
3. `git tag vX.Y.Z && git push origin vX.Y.Z`.
4. Vérifier la release GitHub (5 archives + 5 sommes) et l'image `ghcr.io/namularbre/eyeship:X.Y.Z`.

Au 2026-11-16, aucun tag n'a encore été poussé : le workflow Release n'a jamais tourné.

## 5. Actions tierces utilisées

| Action | Version | Usage |
|---|---|---|
| `actions/checkout` | v4 | Tous les jobs |
| `dtolnay/rust-toolchain` | `@stable` | Installation de Rust |
| `Swatinem/rust-cache` | v2 | Cache Cargo (CI) |
| `taiki-e/create-gh-release-action` | v1 | Création de la release |
| `taiki-e/setup-cross-toolchain-action` | v1 | musl et compilation croisée Linux |
| `taiki-e/upload-rust-binary-action` | v1 | Compilation, archive, checksum, envoi |
| `docker/setup-qemu-action` | v3 | Émulation arm64 |
| `docker/setup-buildx-action` | v3 | Buildx |
| `docker/login-action` | v3 | Connexion à ghcr.io |
| `docker/metadata-action` | v5 | Calcul des tags |
| `docker/build-push-action` | v6 | Construction et envoi de l'image |

Toutes sont référencées par un tag majeur, pas par un SHA de commit.

## 6. Écarts constatés

Points relevés en documentant l'existant. Leur correction est décrite dans la [spec 003](../003-ci-cd-durcissement/spec.md).

| # | Constat | Conséquence possible |
|---|---|---|
| E1 | La release ne dépend pas de la CI : un tag poussé sur un commit rouge publie quand même. | Release cassée publiée. |
| E2 | Aucun test du binaire ou de l'image produits (pas de *smoke test* `eyeship --version`). | Archive ou image inutilisable non détectée. |
| E3 | Le tag `latest` est poussé pour tout tag `v*`, y compris une pré-release (`v1.0.0-rc.1`). | `latest` pointe sur une version instable. |
| E4 | Actions tierces référencées par tag majeur, pas par SHA. Ironie : c'est l'équivalent de la règle R008 (image non épinglée). | Changement silencieux d'une action. |
| E5 | MSRV (Rust ≥ 1.85 dans le README) non vérifiée, `rust-version` absent du `Cargo.toml`. | Promesse d'installation non tenue. |
| E6 | Benchmark ENF1 hors CI. | Régression de performance non détectée. |
| E7 | Pas de `concurrency` dans `ci.yml` : plusieurs pushes rapides sur une PR lancent plusieurs exécutions complètes. | Minutes CI consommées pour rien. |
| E8 | Pas de mise à jour automatique des dépendances (Dependabot, Renovate). | Dépendances et actions qui vieillissent. |
