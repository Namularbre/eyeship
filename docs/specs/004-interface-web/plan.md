# Plan 004 : Interface web, plan technique

| Champ | Valeur |
|---|---|
| Spec | [`spec.md`](./spec.md) |
| Statut | Brouillon, à valider |

> Ce plan décrit **comment** livrer la spec 004 en respectant la [constitution](../../constitution.md) : Rust idiomatique, sans `unsafe`, fonctionnel, hexagonal, TDD, dépendances légères, un seul binaire.

## 1. Vue d'ensemble

L'interface est un **nouvel adaptateur de sortie**, au même titre que le texte et le JSON. L'historique est un **adaptateur de stockage** : des fichiers SARIF dans un dossier. Le domaine gagne une seule fonction pure, la comparaison de deux rapports.

```
                    ┌──────────────────────────── eyeship-cli ────────────────────────────┐
navigateur ──GET──▶ │ serve: boucle HTTP ──▶ route(request) ──▶ Page ──▶ eyeship-report::html│──▶ réponse HTML
                    │                          │                                          │
                    │                          ├─ audit_deployments(config, fs, env)       │
                    │                          │    load (eyeship-compose) ▶ audit (domain)│
                    │                          └─ history.list / read (eyeship-history)    │
                    └──────────────────────────────────────────────────────────────────────┘
CI ── eyeship compose --deployment prod --save ──▶ .eyeship/reports/prod/2026-11-26T11-47-17.123Z.sarif
```

Le flux reste une suite de fonctions, avec une seule fonction impure au bord (la boucle réseau) :

```
.eyeship.toml ──load_config──▶ Config { deployments } ──audit_deployments──▶ Vec<DeploymentResult>
requête ──parse──▶ Route ──handle(route, results, history)──▶ Response { status, headers, body }
Report ──to_sarif(+ deployment, saved_at, label)──▶ fichier .sarif                    (--save)
fichiers ──parse_sarif──▶ Vec<Record> ──diff(a, b)──▶ { new, resolved, unchanged }   (US8, US9)
```

## 2. Découpage

```
crates/
├── eyeship-domain/     + diff.rs : comparaison pure de deux listes de constats (US9)
├── eyeship-compose/    inchangé
├── eyeship-report/     + sarif.rs : écriture et lecture SARIF 2.1.0 (US10)
│                       + src/html/ : échappement, mise en page, une fonction pure par page
│   └── assets/         style.css (embarqué par include_str!)
├── eyeship-history/    NOUVEAU : dossier d'historique (nommage, écriture atomique, liste) via le trait FileSystem
├── eyeship-web/        NOUVEAU : routeur (pur) + serveur (bord)
│   ├── src/route.rs    chemin + query ──▶ Route (pur)
│   ├── src/handler.rs  Route + données ──▶ Response (pur)
│   └── src/server.rs   boucle tiny_http, en-têtes de sécurité
└── eyeship-cli/        + commande `serve`, + `--deployment`, `--save`, `--label`, + `[deployments]` dans la config
```

Les pages HTML sont un adaptateur de sortie comme le texte et le JSON : elles vivent dans `eyeship-report`, ce qui sert aussi l'export `--format html` (US6). `eyeship-web` dépend de `eyeship-domain` et `eyeship-report` mais pas de `eyeship-compose` : il reçoit des résultats d'audit déjà calculés à travers un port, ce qui permettra de lui donner des résultats Kubernetes plus tard (P7).

```rust
/// Port: what the web adapter needs from the application.
pub trait DeploymentSource {
    fn deployments(&self) -> Vec<DeploymentResult>;
    fn history(&self, deployment: &str) -> Vec<HistoryEntry>;      // résumés, du plus récent au plus ancien
    fn record(&self, deployment: &str, id: &str) -> Option<Record>; // un rapport enregistré complet
}

pub struct DeploymentResult {
    pub name: String,
    pub files: Vec<String>,
    pub fail_on: FailOn,
    pub outcome: Result<Report, String>,   // Err = illisible (US1 §3)
}
```

`eyeship-cli` implémente `DeploymentSource` en relisant `.eyeship.toml` puis en auditant chaque déploiement **à chaque requête** (US2 §4). Un audit de 50 services prend ~6 ms : pas de cache, pas d'observation du disque, pas de fil d'exécution en arrière-plan.

## 3. Configuration (`.eyeship.toml`)

```toml
fail_on = "warning"
ignore = ["R016"]

[deployments.staging]
files = ["compose.yaml", "compose.staging.yaml"]
env_file = ".env.staging"          # optionnel

[deployments.prod]
files = ["compose.yaml", "compose.prod.yaml"]
env_file = ".env.prod"
fail_on = "error"                  # optionnel, remplace le seuil global

[services.api]
ignore = ["R006"]
```

- Chemins relatifs au dossier du `.eyeship.toml`.
- Noms de déploiement : `[a-z0-9][a-z0-9-]*` (ils apparaissent dans les URL). Clé inconnue ou nom invalide : erreur de configuration, comme aujourd'hui (`deny_unknown_fields`).
- Sans section `deployments` : un déploiement implicite `default`, découverte habituelle (US1 §2).
- `run_compose` est refactoré pour partager une fonction pure de construction de `LoadRequest` et d'`AuditConfig` avec `serve` et `--deployment` (US5 §2 : même résultat par construction).

## 4. Historique des rapports (`eyeship-history`)

### 4.1 Emplacement

```
.eyeship/reports/            # à côté du .eyeship.toml ; `reports_dir` dans la config, `--reports-dir` pour serve
├── prod/
│   ├── 2026-11-25T09-12-03.481Z.sarif
│   └── 2026-11-26T11-47-17.123Z.sarif
└── staging/
    └── …
```

- Un sous-dossier par déploiement, un fichier par rapport. Pas de base de données : les fichiers se lisent avec n'importe quel visualiseur SARIF, se versionnent, s'archivent comme artefacts de CI et se suppriment à la main.
- Nom de fichier = date UTC à la milliseconde, sans `:` (compatible Windows) ; il sert d'identifiant dans l'URL et donne l'ordre chronologique par simple tri. En cas de collision, suffixe `-1`, `-2`.
- Écriture atomique : fichier temporaire dans le même dossier puis renommage, pour qu'un serveur qui lit en même temps ne voie jamais un fichier tronqué.

### 4.2 Format : SARIF 2.1.0

Le format machine prévu par la constitution (P4) et la spec 001 (« sortie SARIF juste après le MVP »). Un rapport enregistré est la sortie de `--format sarif` (US10) complétée de trois informations de contexte. Un seul format à maintenir pour la CI, GitHub Code Scanning et l'historique.

```json
{
  "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
  "version": "2.1.0",
  "runs": [{
    "tool": { "driver": {
      "name": "eyeship", "version": "0.1.0", "informationUri": "https://github.com/Namularbre/eyeship",
      "rules": [{
        "id": "R001", "name": "DependsOnWithoutHealthCondition",
        "shortDescription": { "text": "depends_on without a health condition" },
        "fullDescription": { "text": "<scénario de panne>" },
        "help": { "text": "<correction>" },
        "defaultConfiguration": { "level": "warning" }
      }]
    }},
    "automationDetails": { "id": "prod/2026-11-26T11-47-17.123Z" },
    "invocations": [{ "executionSuccessful": true, "endTimeUtc": "2026-11-26T11:47:17.123Z" }],
    "originalUriBaseIds": { "PROJECTROOT": { "uri": "file:///" } },
    "results": [{
      "ruleId": "R001", "ruleIndex": 0, "level": "warning",
      "message": { "text": "depends_on 'db' does not wait for the service to be healthy" },
      "locations": [{
        "physicalLocation": { "artifactLocation": { "uri": "compose.yaml", "uriBaseId": "PROJECTROOT" },
                              "region": { "startLine": 12, "startColumn": 7 } },
        "logicalLocations": [{ "name": "api", "kind": "module" }]
      }],
      "partialFingerprints": { "eyeship/v1": "<hash(règle, service, message)>" },
      "properties": { "fix": "Use `condition: service_healthy` for 'db' and give it a healthcheck." }
    }],
    "properties": { "deployment": "prod", "label": "a1b2c3d", "failOn": "error" }
  }]
}
```

Correspondances retenues :

| eyeship | SARIF |
|---|---|
| Sévérité `error` / `warning` / `info` | `level` `error` / `warning` / `note` |
| Service | `logicalLocations[].name` (`kind: "module"`) |
| Correction d'un constat | `properties.fix` (l'objet SARIF `fixes` exige des remplacements de texte, que `eyeship` ne calcule pas) |
| Constat ignoré (P6) | résultat présent avec `suppressions: [{ "kind": "external", "justification": "ignored by .eyeship.toml" }]` |
| Documentation de règle (`explain`) | `tool.driver.rules[]`, seulement les règles déclenchées |
| Identité d'un constat pour la comparaison | `partialFingerprints["eyeship/v1"]` = empreinte de (règle, service, message) |
| Déploiement, étiquette, seuil | `run.properties` ; `automationDetails.id` = `<déploiement>/<identifiant>` |
| Date d'enregistrement | `invocations[0].endTimeUtc` |

- **Déterminisme** (P4) : `--format sarif` sur la sortie standard n'a ni `invocations`, ni `automationDetails`, ni `run.properties` ; ils ne sont ajoutés que par `--save`. Chemins relatifs au dossier du projet, jamais absolus.
- **Lecture** : `eyeship-report` désérialise le sous-ensemble SARIF qu'il écrit vers une vue possédée (`ReportView`, chaînes `String`), que l'interface affiche qu'il s'agisse d'un rapport en direct ou enregistré. Un fichier qui n'est pas un SARIF 2.1.0 produit par `eyeship` (`tool.driver.name`) est signalé « illisible » et ignoré (US8 §5).
- **Validation** : un test compare la sortie au schéma officiel SARIF 2.1.0, copié dans `tests/fixtures/sarif/` (dépendance de test `jsonschema`, **D8**), et le fichier est envoyé une fois à `github/codeql-action/upload-sarif` sur un dépôt de test (US10 §6).
- **Empreinte** : hachage FNV-1a 64 bits écrit à la main (quelques lignes, stable entre versions de Rust, contrairement à `DefaultHasher`).
- La date vient de `SystemTime` et est formatée en RFC 3339 par une fonction pure (algorithme jours → date civile), sans `chrono`. L'horloge est un petit trait `Clock` pour les tests.

### 4.3 Lecture

À chaque requête, comme pour les rapports en direct : lister le dossier du déploiement, lire les en-têtes et résumés. Pour 1 000 fichiers de quelques Ko, la lecture reste sous ENF8 (300 ms) ; si la mesure (T026) dit le contraire, on ajoutera un cache invalidé par la date de modification du dossier, sans changer le format.

### 4.4 Comparaison (US9)

```rust
pub struct Diff<'a, T> { pub new: Vec<&'a T>, pub resolved: Vec<&'a T>, pub unchanged: Vec<&'a T> }
pub fn diff<'a, T, K: Ord>(before: &'a [T], after: &'a [T], key: impl Fn(&T) -> K) -> Diff<'a, T>
```

Fonction pure et générique du domaine (qui ne connaît pas `ReportView`) ; l'appelant fournit la clé, l'empreinte `eyeship/v1` (règle, service, message). La ligne n'entre pas dans la clé : déplacer un service dans le fichier ne crée ni constat « nouveau » ni « résolu ». Deux constats de même clé sont appariés dans l'ordre (multiensemble).

## 5. Pages et routes

Uniquement `GET` (et `HEAD`). Toute autre méthode : `405`. Route inconnue : `404` avec une page lisible.

| Route | Page | Récit |
|---|---|---|
| `/` | Tableau de bord : un bloc par déploiement (statut, compteurs, fichiers), échecs en premier | US2 |
| `/deployments/{name}?severity=warning&service=api` | Rapport en direct : résumé, constats groupés par service, filtres ; puis historique (liste et courbe) | US3, US8 |
| `/deployments/{name}/reports/{id}` | Rapport enregistré, mêmes filtres, bandeau « enregistré le … » | US8 |
| `/deployments/{name}/compare?from={id}&to={id\|live}` | Constats nouveaux, résolus, inchangés | US9 |
| `/rules` | Catalogue des règles | US4 |
| `/rules/{id}` | Explication d'une règle | US4 |
| `/assets/style.css` | Feuille de style, en cache long avec hash dans l'URL | |
| `/health` | `ok`, pour un `healthcheck` Compose ou un test | |

- **Sans JavaScript** (ENF6) : les filtres sont un formulaire `GET` ; l'URL porte l'état.
- **Rendu serveur** par des fonctions pures `fn page(data) -> String`. Chaque valeur issue d'un fichier analysé passe par `escape()` (`& < > " '`). Un test vérifie qu'un nom de service `<script>` ressort échappé sur chaque page.
- **Style** : une feuille CSS écrite à la main (< 10 Ko), variables CSS, `prefers-color-scheme`, grille fluide pour mobile. Les gravités ont une couleur **et** un libellé (pas de couleur seule).
- **Courbe d'évolution** : SVG généré côté serveur (polylignes par gravité, axes simples), sans bibliothèque ni JavaScript ; un tableau équivalent l'accompagne pour l'accessibilité.
- **Déterminisme** : mêmes données, même HTML ; testé par snapshots `insta`. Les dates sont affichées en UTC (le serveur ne connaît pas le fuseau du lecteur sans JavaScript).

## 6. Serveur

```
eyeship serve [--host 127.0.0.1] [--port 7878] [--open]
```

- Écoute sur `127.0.0.1:7878` par défaut ; `--port 0` choisit un port libre. Affiche `eyeship: serving http://127.0.0.1:7878 (Ctrl+C to stop)` sur stderr.
- `--host` autre qu'une adresse de boucle locale : avertissement sur stderr (ENF4).
- **Protection contre le DNS rebinding** : en écoute locale, l'en-tête `Host` doit être `localhost`, `127.0.0.1` ou `[::1]` (avec le port) ; sinon `421`.
- En-têtes de chaque réponse : `Content-Security-Policy: default-src 'none'; style-src 'self'; img-src 'self' data:; form-action 'self'; base-uri 'none'; frame-ancestors 'none'`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`, `Cache-Control: no-store` pour les pages.
- Un fil d'exécution, requêtes traitées l'une après l'autre : suffisant pour un usage local, pas de `tokio` (plan 001 §7).
- `--open` (P3 de cette spec, optionnel) : ouvrir le navigateur. Ajouté seulement si on le juge utile, sans dépendance (commande système).
- Codes de sortie : `0` à l'arrêt normal, `2` si le port est pris ou la configuration invalide au démarrage.

## 7. Export HTML (US6)

`eyeship compose --format html` réutilise la fonction de la page rapport avec une mise en page « autonome » : CSS en ligne dans `<style>`, aucun lien vers `/rules` (les explications des règles concernées sont incluses en bas de page, dépliables avec `<details>`). La sortie n'a ni date ni chemin absolu, pour rester déterministe (P4).

## 8. Dépendances

| Crate | Usage | Où | Justification |
|---|---|---|---|
| `tiny_http` | Serveur HTTP/1.1 synchrone | eyeship-web | Petit, sans `tokio`, sans TLS par défaut. Analyser HTTP correctement à la main est une source de failles. **D2** |
| *(aucune)* | Gabarits HTML | eyeship-report | Fonctions Rust + `format!` + `escape()`. Les pages sont peu nombreuses. Alternative : `maud` (macros, échappement automatique). **D3** |
| *(aucune)* | Front-end | | HTML + CSS, pas de framework JS ni de WASM, pas de chaîne de build Node. **D3** |

`eyeship-history` et la sortie SARIF n'ajoutent rien : `serde` et `serde_json` sont déjà dans le workspace. `jsonschema` n'est qu'une dépendance de test (D8).

Rien d'autre. Pas d'`axum`/`actix` (tirent `tokio`), pas de `rust-embed` (`include_str!` suffit pour un fichier).

## 9. Stratégie de test (TDD)

1. **Pages** : tests unitaires sur des `Report` construits avec les builders existants ; snapshots `insta` du HTML ; test d'échappement sur chaque page ; test « aucune URL externe » (pas de `http://` ni `https://` dans le HTML et le CSS hors liens de documentation).
2. **Routeur et gestionnaire** : fonctions pures, tables de cas (`/deployments/prod?severity=error` → `Route::Deployment { … }`, chemin inconnu → 404, méthode `POST` → 405, `Host` étranger → 421).
3. **Configuration** : parsing des `[deployments]`, `reports_dir`, noms invalides, chemins relatifs, déploiement implicite.
4. **SARIF** : snapshot de la sortie pour chaque fixture, validation par le schéma officiel, aller-retour écrire/relire, constats ignorés en `suppressions`, déterminisme.
5. **Historique** : aller-retour enregistrer/relire avec `MemoryFileSystem` et une horloge fixe ; formatage RFC 3339 (années bissextiles, fin de mois) ; collisions de noms ; fichier illisible ou schéma inconnu ignoré ; `--save` ne change ni stdout ni le code de sortie.
6. **Comparaison** : constats nouveaux, résolus, inchangés ; ligne déplacée = inchangé ; doublons.
7. **Équivalence CLI / web** : pour chaque fixture de la spec 001, les constats de la page rapport = ceux de `--format json` (critère de succès §6).
8. **Bout en bout** : `eyeship serve --port 0` lancé par `assert_cmd`, port lu sur stderr, requêtes HTTP avec `std::net::TcpStream` (pas de client HTTP en dépendance), arrêt du processus.
9. **Performance** : historique de 1 000 rapports sous 300 ms (ENF8) ; page rapport du fichier de 50 services du benchmark ENF1 en moins de 100 ms ; taille du binaire release mesurée avant et après (≤ +1 Mo).

## 10. Décisions à valider

| # | Décision | Recommandation | Alternative |
|---|---|---|---|
| D1 | Source des déploiements | **Validé le 2026-11-26 : les deux.** Déploiements déclarés dans `.eyeship.toml` audités à chaque requête, et historique de rapports enregistrés | Stacks en cours via Docker (déroge à P3), écarté |
| D6 | Stockage de l'historique | **Retenu le 2026-11-26 : un fichier SARIF 2.1.0 par rapport** dans `.eyeship/reports/<déploiement>/`, enregistré par la CLI (`--save`) | Enveloppe JSON maison autour de `--format json` (second format à maintenir, illisible par les autres outils) ; SQLite (`rusqlite`, C embarqué, binaire plus lourd) |
| D7 | Dates | `SystemTime` + formatage RFC 3339 écrit à la main, affichage UTC | `time` ou `chrono` (dépendance pour une seule fonction) |
| D8 | Validation SARIF | `jsonschema` en dépendance **de test** uniquement, schéma officiel copié dans le dépôt (tests hors ligne) | Validation manuelle seulement |
| D2 | Serveur HTTP | `tiny_http`, synchrone | `axum` + `tokio` (lourd) ; HTTP écrit à la main sur `std::net` (zéro dépendance, risqué) |
| D3 | Front-end | Rendu serveur, HTML + CSS sans JS, gabarits en fonctions Rust | `maud` pour les gabarits ; SPA (Leptos/WASM ou TypeScript) : chaîne de build et poids sans bénéfice ici |
| D4 | Servi par la CLI | Sous-commande `eyeship serve` du binaire unique | Binaire séparé `eyeship-web` |
| D5 | Adresse par défaut | `127.0.0.1:7878` | `0.0.0.0` (exposé au réseau, refusé par ENF4) |

## 11. Risques

| Risque | Parade |
|---|---|
| Faille XSS via un nom de service ou un message | Échappement systématique, test dédié par page, CSP stricte sans script. |
| Lecture de fichiers arbitraires via l'URL | Aucune route ne prend de chemin : seuls les déploiements déclarés sont audités. |
| Site malveillant qui interroge l'interface locale (DNS rebinding) | Vérification de l'en-tête `Host`, `GET` seulement, aucune action à effets de bord. |
| Historique qui grossit sans fin | Un fichier fait quelques Ko ; la purge reste manuelle (spec §7). Mesure en T026. |
| Rapports enregistrés par une version plus récente d'`eyeship` | `version` SARIF et `tool.driver.name` vérifiés ; champs inconnus tolérés ; empreinte versionnée (`eyeship/v1`). |
| SARIF verbeux (rapport plus gros que le JSON maison) | Quelques Ko par rapport ; seules les règles déclenchées sont décrites. |
| Dérive entre CLI et web | Construction partagée de la requête et de la config (§3), test d'équivalence (§8.4). |
| `tiny_http` peu actif (dernière version 0.12, un fork `tiny_http_dh` annonce des correctifs de CVE) | Vérifier les avis RustSec en T002 et choisir entre l'original et le fork ; API réduite utilisée derrière `server.rs`, remplaçable sans toucher aux pages ni au routeur. |
