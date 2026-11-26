# Tâches 004 : Interface web

| Champ | Valeur |
|---|---|
| Spec | [`spec.md`](./spec.md) |
| Plan | [`plan.md`](./plan.md) |

Conventions : celles de la [spec 001](../001-audit-compose/tasks.md). `[P]` = parallélisable. Chaque tâche commence par un test rouge.

## Phase 0 : Préalables

- [ ] **T001** Amender la constitution §2 (spec §8) une fois la spec validée.
- [ ] **T002** Créer les crates `eyeship-history` et `eyeship-web` (`#![forbid(unsafe_code)]`, lints du workspace), ajouter `tiny_http` (D2) après vérification des avis RustSec. Mesurer la taille du binaire release avant ajout, pour ENF5.

## Phase 1 : Déploiements déclarés (US1, US5)

- [ ] **T003** Configuration : section `[deployments.<name>]` (`files`, `env_file`, `fail_on`) et clé `reports_dir`, noms validés, chemins relatifs au `.eyeship.toml`, déploiement implicite `default`. Tests : cas nominal, nom invalide, clé inconnue, absence de section. [US1]
- [ ] **T004** Extraire de `run_compose` une fonction pure qui construit `LoadRequest`, `AuditConfig` et le seuil à partir des arguments **ou** d'un déploiement. Tests existants inchangés et verts. [US1, US5]
- [ ] **T005** `audit_deployments(config, fs, env) -> Vec<DeploymentResult>` : un déploiement illisible donne `Err(message)` sans bloquer les autres ; tri échecs d'abord puis par nom. Tests avec `MemoryFileSystem`. [US1, US2]
- [ ] **T006** [P] Option `eyeship compose --deployment <name>`, incompatible avec `FILE`, `-f`, `--env-file` (code 2). Tests de bout en bout. **Jalon : les déploiements sont utilisables en CI.** [US5]

## Phase 2 : SARIF et historique (US10, US7)

- [ ] **T007** [P] Formatage RFC 3339 pur depuis `SystemTime` (D7) et trait `Clock`. Tests : époque, années bissextiles, fin de mois, millisecondes, nom de fichier sans `:`.
- [ ] **T008** `--format sarif` (plan §4.2) : règles déclenchées, résultats, niveaux, service, correction, empreinte FNV-1a, constats ignorés en `suppressions`. Snapshots sur chaque fixture, validation par le schéma officiel (D8), déterminisme. Envoi manuel à `upload-sarif` sur un dépôt de test. **Jalon : sortie SARIF utilisable en CI.** [US10]
- [ ] **T009** Lecture SARIF vers `ReportView` (sous-ensemble écrit par `eyeship`), conversion `Report → ReportView`, ajout du contexte d'enregistrement (`invocations`, `automationDetails`, `run.properties`) ; fichier étranger ou invalide → illisible. Test aller-retour sur chaque fixture.
- [ ] **T010** Écriture atomique dans `<reports_dir>/<déploiement>/`, création du dossier, collisions de noms ; liste et lecture d'un historique trié. Tests avec `MemoryFileSystem` (étendre le trait `FileSystem` si besoin : écrire, renommer, lister).
- [ ] **T011** Options `--save` et `--label` sur `eyeship compose` : stdout et code de sortie inchangés, code 2 si l'écriture échoue. Tests de bout en bout dans un dossier temporaire. **Jalon : la CI peut construire un historique.** [US7]
- [ ] **T012** [P] `diff` générique dans le domaine (plan §4.4) : nouveaux, résolus, inchangés, ligne déplacée, doublons. [US9]

## Phase 3 : Pages HTML (US2, US3, US4, US8)

- [ ] **T013** `escape()` et mise en page commune (`<head>`, navigation, lien vers la feuille de style). Test : caractères spéciaux échappés.
- [ ] **T014** Feuille `style.css` : thèmes clair et sombre, gravités en couleur **et** libellé, mise en page mobile. Test : aucune URL externe.
- [ ] **T015** [P] Page tableau de bord : statut, compteurs, ignorés, fichiers ; déploiement illisible. Snapshots. [US2]
- [ ] **T016** [P] Page rapport : résumé, constats groupés par service, lien vers la règle, filtres gravité et service (formulaire GET). Snapshots, test d'échappement. [US3]
- [ ] **T017** [P] Pages historique : liste des rapports enregistrés et courbe SVG avec tableau équivalent sur la page déploiement ; page d'un rapport enregistré (bandeau, filtres) ; date et tendance sur le tableau de bord ; entrées illisibles signalées. Snapshots. [US8]
- [ ] **T018** [P] Page de comparaison : nouveaux, résolus, inchangés, entre deux rapports enregistrés ou avec le direct. Snapshots. [US9]
- [ ] **T019** [P] Pages catalogue et explication de règle, mêmes contenus que `rules` et `explain`. Snapshots. [US4]
- [ ] **T020** Test d'équivalence : pour chaque fixture de la spec 001, constats de la page rapport = constats de `--format json`. [US3]

## Phase 4 : Serveur (US2, US8)

- [ ] **T021** Routeur pur : chemin + query → `Route` ; 404, 405, filtres invalides ignorés. Tables de cas.
- [ ] **T022** Gestionnaire pur : `Route` + `DeploymentSource` → `Response` ; en-têtes de sécurité (CSP, `nosniff`, `no-referrer`, `no-store`) ; vérification de `Host` (421). Tests.
- [ ] **T023** Boucle `tiny_http` dans `server.rs`, sous-commande `eyeship serve [--host] [--port]`, message d'adresse sur stderr, avertissement hors boucle locale, code 2 si le port est pris. [US2]
- [ ] **T024** Bout en bout : `serve --port 0`, requêtes par `TcpStream` sur `/`, une page rapport, un rapport enregistré, une comparaison, `/rules/R001`, `/health`, une 404 ; modification d'un fichier puis rechargement (US2 §4). **Jalon : l'interface est utilisable.**

## Phase 5 : Export HTML (US6)

- [ ] **T025** `eyeship compose --format html` : page autonome, CSS en ligne, explications des règles concernées incluses. Snapshot, test de déterminisme (deux exécutions, même octets). [US6]

## Phase 6 : Clôture

- [ ] **T026** [P] Performance : page rapport de 50 services < 100 ms, serveur au repos < 20 Mo, binaire ≤ +1 Mo (ENF5) ; historique de 1 000 rapports < 300 ms (ENF8).
- [ ] **T027** [P] Accessibilité : navigation clavier, contrastes, `lang="en"`, titres hiérarchisés (ENF6). Vérification manuelle avec capture d'écran clair et sombre.
- [ ] **T028** [P] README : section « Web interface » (déclarer des déploiements, `eyeship serve`, `--format sarif` et exemple GitHub Code Scanning, `--save` en CI et historique, `--format html`, conseils de sécurité). Exemple `examples/.eyeship.toml`.
- [ ] **T029** Passer spec et plan au statut « Validé », cocher les tâches.
