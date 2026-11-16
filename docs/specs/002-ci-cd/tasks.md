# Tâches 002 : Pipelines CI/CD

| Champ | Valeur |
|---|---|
| Spec | [`spec.md`](./spec.md) |
| Plan | [`plan.md`](./plan.md) |

> Rétro-documentation : les tâches ci-dessous décrivent ce qui a été réalisé dans le cadre de la spec 001 (tâches d'origine entre parenthèses). Les écarts du [plan §6](./plan.md#6-écarts-constatés) sont traités par la [spec 003](../003-ci-cd-durcissement/tasks.md).

## Phase 1 : Intégration continue [US1]

- [x] **T001** Workflow `ci.yml` déclenché sur push `main`/`master` et sur PR, `RUSTFLAGS=-D warnings`. (001-T002)
- [x] **T002** [P] Job `fmt` : `cargo fmt --all --check`. (001-T002)
- [x] **T003** [P] Job `clippy` : `--workspace --all-targets -- -D warnings`, cache Cargo. (001-T002)
- [x] **T004** [P] Job `test` : `cargo test --workspace` sur Linux, macOS, Windows, `fail-fast: false`, cache Cargo. (001-T002)

## Phase 2 : Livraison [US2, US3]

- [x] **T005** Workflow `release.yml` déclenché par un tag `v*`, job `create-release`. (001-T047)
- [x] **T006** [P] Job `binaries` : 5 cibles, archives `tar.gz`/`zip`, SHA-256. [US2] (001-T047)
- [x] **T007** [P] `Dockerfile` deux étapes (`rust:1-alpine` puis `scratch`) et `.dockerignore` minimal. [US3] (001-T047)
- [x] **T008** Job `docker` : image multi-architecture sur `ghcr.io`, tags semver et `latest`, cache `gha`. [US3] (001-T047)
- [x] **T009** README : installation (binaires, Docker), section « Development » alignée sur la CI. (001-T048)

## Phase 3 : Première release

- [ ] **T010** Pousser le premier tag `v0.1.0` et vérifier les artefacts selon la procédure du [plan §4](./plan.md#4-procédure-de-release).
