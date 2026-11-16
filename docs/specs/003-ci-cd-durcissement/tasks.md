# Tâches 003 : Durcir les pipelines CI/CD

| Champ | Valeur |
|---|---|
| Spec | [`spec.md`](./spec.md) |
| Plan | [`plan.md`](./plan.md) |

Conventions : celles de la [spec 001](../001-audit-compose/tasks.md). Pour un workflow, le « test rouge » est de provoquer le cas d'échec (sur une branche ou un fork) et de vérifier que le pipeline le bloque, puis le cas nominal.

## Phase 1 : Rapide et sans risque (P2, P3)

- [x] **T001** [P] `concurrency` dans `ci.yml`. Vérifier : deux pushes rapides sur une PR, la première exécution est annulée ; sur `main`, aucune annulation. [US6] (E7)
- [x] **T002** [P] `rust-version = "1.89"` dans le workspace, job `msrv`, README corrigé (annonçait 1.85, voir plan §6). Vérifié en local : `cargo +1.85 check --locked` échoue (`serde-saphyr` exige 1.89), `cargo +1.89 check --locked` passe. [US5] (E5)
- [x] **T003** [P] Job `perf` : test ENF1 en release ; `scripts/bench.sh` échoue au-delà de 20 Mo. [US5] (E6)
- [x] **T004** [P] `.github/dependabot.yml` (actions, cargo, docker), groupé, hebdomadaire. [US4] (E8)

## Phase 2 : Garde de release (P1)

- [ ] **T005** `ci.yml` appelable (`workflow_call`) ; `release.yml` : jobs `check-version` puis `ci` avant `create-release`. Vérifier sur un fork : tag sur un commit rouge, rien n'est publié ; tag ≠ version du `Cargo.toml`, échec explicite. [US1] (E1)
- [ ] **T006** `scripts/smoke.sh` : version, audit de `examples/compose.yaml` (code 1, `R013`), JSON valide. Testé en local sur le binaire de dev. [US2] (E2)
- [ ] **T007** Job `binaries` : build séparé, smoke test (natif, QEMU, Rosetta), puis envoi. [US2] (E2)
- [ ] **T008** Job `docker` : build `--load` par plateforme, smoke test, puis poussée. [US2] (E2)
- [ ] **T009** `latest` et `major.minor` désactivés pour une pré-release ; release GitHub marquée pré-release. Vérifier avec un tag `v0.0.0-rc.1` sur un fork. [US3] (E3)

## Phase 3 : Épinglage (P2)

- [ ] **T010** Toutes les actions par SHA avec la version en commentaire ; `rust:1-alpine` par digest. Vérifier : `grep -E 'uses: [^@]+@v' .github/workflows/` ne renvoie rien. [US4] (E4)

## Phase 4 : Clôture

- [ ] **T011** Mettre à jour le [plan 002](../002-ci-cd/plan.md) pour décrire le nouvel existant et retirer les écarts corrigés.
- [ ] **T012** Première release `v0.1.0` avec les nouveaux pipelines (remplace 002-T010).

## Dépendances

```
Phase 1 (indépendante) ─┐
                        ├──▶ Phase 3 ──▶ Phase 4
Phase 2 (T005 → T006 → T007, T008, T009) ─┘
```

T010 vient après les phases 1 et 2 pour épingler en une fois toutes les actions ajoutées.
