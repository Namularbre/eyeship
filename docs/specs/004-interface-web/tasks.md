# Tâches 004 : Interface web

| Champ | Valeur |
|---|---|
| Spec | [`spec.md`](./spec.md) |
| Plan | [`plan.md`](./plan.md) |

Conventions : celles de la [spec 001](../001-audit-compose/tasks.md). `[P]` = parallélisable. Chaque tâche commence par un test rouge.

## Phase 0 : Préalables

- [ ] **T001** Amender la constitution §2 (spec §8) une fois la spec validée.
- [ ] **T002** Créer le crate `eyeship-web` (`#![forbid(unsafe_code)]`, lints du workspace), ajouter `tiny_http` (D2). Mesurer la taille du binaire release avant ajout, pour ENF5.

## Phase 1 : Déploiements déclarés (US1, US5)

- [ ] **T003** Configuration : section `[deployments.<name>]` (`files`, `env_file`, `fail_on`), noms validés, chemins relatifs au `.eyeship.toml`, déploiement implicite `default`. Tests : cas nominal, nom invalide, clé inconnue, absence de section. [US1]
- [ ] **T004** Extraire de `run_compose` une fonction pure qui construit `LoadRequest`, `AuditConfig` et le seuil à partir des arguments **ou** d'un déploiement. Tests existants inchangés et verts. [US1, US5]
- [ ] **T005** `audit_deployments(config, fs, env) -> Vec<DeploymentResult>` : un déploiement illisible donne `Err(message)` sans bloquer les autres ; tri échecs d'abord puis par nom. Tests avec `MemoryFileSystem`. [US1, US2]
- [ ] **T006** [P] Option `eyeship compose --deployment <name>`, incompatible avec `FILE`, `-f`, `--env-file` (code 2). Tests de bout en bout. **Jalon : les déploiements sont utilisables en CI.** [US5]

## Phase 2 : Pages HTML (US2, US3, US4)

- [ ] **T007** `escape()` et mise en page commune (`<head>`, navigation, lien vers la feuille de style). Test : caractères spéciaux échappés.
- [ ] **T008** Feuille `style.css` : thèmes clair et sombre, gravités en couleur **et** libellé, mise en page mobile. Test : aucune URL externe.
- [ ] **T009** [P] Page tableau de bord : statut, compteurs, ignorés, fichiers ; déploiement illisible. Snapshots. [US2]
- [ ] **T010** [P] Page rapport : résumé, constats groupés par service, lien vers la règle, filtres gravité et service (formulaire GET). Snapshots, test d'échappement. [US3]
- [ ] **T011** [P] Pages catalogue et explication de règle, mêmes contenus que `rules` et `explain`. Snapshots. [US4]
- [ ] **T012** Test d'équivalence : pour chaque fixture de la spec 001, constats de la page rapport = constats de `--format json`. [US3]

## Phase 3 : Serveur (US2)

- [ ] **T013** Routeur pur : chemin + query → `Route` ; 404, 405, filtres invalides ignorés. Tables de cas.
- [ ] **T014** Gestionnaire pur : `Route` + `DeploymentSource` → `Response` ; en-têtes de sécurité (CSP, `nosniff`, `no-referrer`, `no-store`) ; vérification de `Host` (421). Tests.
- [ ] **T015** Boucle `tiny_http` dans `server.rs`, sous-commande `eyeship serve [--host] [--port]`, message d'adresse sur stderr, avertissement hors boucle locale, code 2 si le port est pris. [US2]
- [ ] **T016** Bout en bout : `serve --port 0`, requêtes par `TcpStream` sur `/`, une page rapport, `/rules/R001`, `/health`, une 404 ; modification d'un fichier puis rechargement (US2 §4). **Jalon : l'interface est utilisable.**

## Phase 4 : Export HTML (US6)

- [ ] **T017** `eyeship compose --format html` : page autonome, CSS en ligne, explications des règles concernées incluses. Snapshot, test de déterminisme (deux exécutions, même octets). [US6]

## Phase 5 : Clôture

- [ ] **T018** [P] Performance : page rapport de 50 services < 100 ms, serveur au repos < 20 Mo, binaire ≤ +1 Mo (ENF5).
- [ ] **T019** [P] Accessibilité : navigation clavier, contrastes, `lang="en"`, titres hiérarchisés (ENF6). Vérification manuelle avec capture d'écran clair et sombre.
- [ ] **T020** [P] README : section « Web interface » (déclarer des déploiements, `eyeship serve`, `--format html`, conseils de sécurité). Exemple `examples/.eyeship.toml`.
- [ ] **T021** Passer spec et plan au statut « Validé », cocher les tâches.
