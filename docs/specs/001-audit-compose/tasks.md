# Tâches 001 : Audit Compose

| Champ | Valeur |
|---|---|
| Spec | [`spec.md`](./spec.md) |
| Plan | [`plan.md`](./plan.md) |

Conventions :
- `[P]` : tâche parallélisable (fichiers indépendants, pas de dépendance avec les tâches voisines).
- `[USn]` : récit utilisateur servi.
- **TDD** : chaque tâche commence par écrire les tests qui échouent, puis le code minimal qui les fait passer, puis le refactoring. Une tâche est terminée quand `cargo fmt`, `cargo clippy -- -D warnings` et `cargo test` passent.

## Phase 0 : Mise en place

- [ ] **T001** Transformer le projet en workspace Cargo avec les 4 crates (`eyeship-domain`, `eyeship-compose`, `eyeship-report`, `eyeship-cli`), `#![forbid(unsafe_code)]` partout, profil release du plan. Renommer le package `audit` en `eyeship` et migrer `src/main.rs` dans `eyeship-cli`.
- [x] **T002** [P] CI GitHub Actions : fmt, clippy, tests sur Linux, macOS, Windows.
- [x] **T003** [P] Spike `serde-saphyr` (décision D1) : désérialiser un Compose avec ancres et `<<`, vérifier qu'on obtient la ligne et la colonne de chaque valeur utile aux règles. Si ce n'est pas possible, repli sur `saphyr` seul. Conclure dans `plan.md`.

## Phase 1 : Fondations (bloque toutes les règles)

### Domaine
- [x] **T004** `Severity`, `Location`, `Located<T>`, `RuleId`, `Finding`. Tests : ordre des sévérités, affichage des IDs.
- [x] **T005** Modèle `Project` / `Service` et sous-types du plan (§3.1). Builders de test dans un module `test_support`.
- [x] **T006** Structure `Rule`, registre `RULES`, moteur `audit()` avec tri déterministe. Tests : ordre stable, règle désactivée, constats ignorés comptés.
- [x] **T007** `Report::exit_code(fail_on)`. Tests : chaque seuil (`error`, `warning`, `info`, `none`).

### Adaptateur Compose
- [x] **T008** Traits `FileSystem` et `Env`, implémentations réelle et en mémoire.
- [x] **T009** Découverte du fichier Compose par défaut (ordre de priorité Docker). [US1]
- [x] **T010** Lecture YAML et mapping des champs simples : `services`, `image` (parsing registre/nom/tag/digest), `build`, `container_name`, `restart`, clé `version`, avec positions.
- [x] **T011** [P] Normalisation `depends_on` (liste et map, conditions).
- [x] **T012** [P] Normalisation `ports` (syntaxe courte avec IP, plages, protocole ; syntaxe longue).
- [x] **T013** [P] Normalisation volumes et mounts (court, long, nommé, bind, malformé) et `HostPath::exists`.
- [x] **T014** [P] Normalisation `mem_limit` / `deploy.resources.limits.memory` avec unités, `logging`, `healthcheck` (durées Compose : `1m30s`).
- [x] **T015** [P] Réseaux, `env_file`, secrets et configs (déclarés et référencés).
- [x] **T016** Interpolation des variables (`${VAR}`, `:-`, `-`, `:?`, `$$`), lecture de `.env`, production des `UnresolvedVar`.
- [x] **T017** Erreurs de chargement explicites (fichier absent, YAML invalide) avec position.

### Sortie et CLI : première tranche de bout en bout
- [x] **T018** Rendu texte : constats groupés par service, résumé, couleurs désactivées si pas de TTY ou `NO_COLOR`. Snapshot `insta`. [US1]
- [x] **T019** Commande `eyeship compose [FICHIER]`, câblage `load → audit → render`, codes 0/1/2. [US1]
- [x] **T020** Tranche verticale avec **R008** (image non épinglée), la règle la plus simple : fixture `bad`/`good`, test de bout en bout `assert_cmd`. **Jalon : l'outil est utilisable.**

## Phase 2 : Règles du MVP

Une tâche par règle : module `rules/rNNN.rs`, tests unitaires positifs et négatifs, `RuleDoc` (scénario, correction, exemples), fixtures `tests/fixtures/compose/RNNN/{bad,good}.yaml` et snapshot. Toutes parallélisables une fois la phase 1 terminée.

- [x] **T021** [P] R001 `depends_on` sans condition de santé
- [x] **T022** [P] R002 `depends_on` vers un service sans healthcheck
- [x] **T023** [P] R003 Dépendance invalide (service inexistant, cycle)
- [x] **T024** [P] R004 Healthcheck incohérent
- [x] **T025** [P] R005 Pas de politique de redémarrage (exception `service_completed_successfully`)
- [x] **T026** [P] R006 Pas de limite mémoire
- [x] **T027** [P] R007 Logs sans rotation
- [x] **T028** [P] R009 Conflit de ports publiés (IP, protocole, plages)
- [x] **T029** [P] R010 Réseau non déclaré
- [x] **T030** [P] R011 Volume non déclaré ou malformé
- [x] **T031** [P] R012 Chemin hôte introuvable
- [x] **T032** [P] R013 Variable d'environnement non définie
- [x] **T033** [P] R014 Nom de conteneur invalide
- [x] **T034** [P] R015 Nom de conteneur fixe
- [x] **T035** [P] R016 Nom de conteneur hors kebab-case
- [x] **T036** [P] R017 Clé `version` obsolète

## Phase 3 : CI et documentation des règles (P1)

- [x] **T037** Rendu JSON versionné (plan §5), aucun texte parasite sur stdout. Snapshot. [US2]
- [x] **T038** Option `--fail-on`. Tests de bout en bout pour chaque seuil. [US2]
- [x] **T039** [P] Commande `eyeship rules`. [US3]
- [x] **T040** [P] Commande `eyeship explain <ID>`, erreur claire si l'ID est inconnu. [US3]

## Phase 4 : Confort (P2)

- [x] **T041** Option `--ignore`, constats ignorés comptés dans le résumé. [US4]
- [x] **T042** Fichier `.eyeship.toml` (ignore global et par service, seuil). Selon décision D4. [US4]
- [ ] **T043** `-f` répétable et fusion selon les règles de Compose, positions conservées par fichier d'origine. [US5]
- [x] **T044** Option `--env-file`. [US5]

## Phase 5 : Sortie du MVP

- [ ] **T045** Corpus de 10 fichiers Compose publics réels, revue des constats, correction des faux positifs. Critère : aucun faux positif `error`.
- [ ] **T046** [P] Benchmark : 50 services en moins de 100 ms et moins de 20 Mo de RAM (ENF1).
- [ ] **T047** [P] Binaires de release (Linux musl x86_64 et aarch64, macOS, Windows) et image Docker `FROM scratch`.
- [ ] **T048** [P] README : installation, exemple de sortie, intégration GitHub Actions en une ligne, liste des règles.

## Dépendances entre phases

```
Phase 0 ──▶ Phase 1 (T004 → T007, T008 → T017, puis T018 → T020)
                         │
                         ├──▶ Phase 2 (toutes en parallèle)
                         └──▶ Phase 3
Phase 2 + Phase 3 ──▶ Phase 4 ──▶ Phase 5
```

Le jalon T020 est le premier moment où Jean peut lancer l'outil sur son projet. Tout ce qui suit ajoute des règles ou du confort sans changer l'architecture.
