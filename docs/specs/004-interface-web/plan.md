# Plan 004 : Interface web, plan technique

| Champ | Valeur |
|---|---|
| Spec | [`spec.md`](./spec.md) |
| Statut | Brouillon, à valider |

> Ce plan décrit **comment** livrer la spec 004 en respectant la [constitution](../../constitution.md) : Rust idiomatique, sans `unsafe`, fonctionnel, hexagonal, TDD, dépendances légères, un seul binaire.

## 1. Vue d'ensemble

L'interface est un **nouvel adaptateur de sortie**, au même titre que le texte et le JSON. Le domaine ne change pas.

```
                    ┌──────────────────────────── eyeship-cli ────────────────────────────┐
navigateur ──GET──▶ │ serve: boucle HTTP ──▶ route(request) ──▶ Page ──▶ eyeship-report::html│──▶ réponse HTML
                    │                          │                                          │
                    │                          └─ audit_deployments(config, fs, env)       │
                    │                               load (eyeship-compose) ▶ audit (domain)│
                    └──────────────────────────────────────────────────────────────────────┘
```

Le flux reste une suite de fonctions, avec une seule fonction impure au bord (la boucle réseau) :

```
.eyeship.toml ──load_config──▶ Config { deployments } ──audit_deployments──▶ Vec<DeploymentResult>
requête ──parse──▶ Route ──handle(route, results)──▶ Response { status, headers, body }
```

## 2. Découpage

```
crates/
├── eyeship-domain/     inchangé
├── eyeship-compose/    inchangé
├── eyeship-report/     + src/html/ : échappement, mise en page, une fonction pure par page
│   └── assets/         style.css (embarqué par include_str!)
├── eyeship-web/        NOUVEAU : routeur (pur) + serveur (bord)
│   ├── src/route.rs    chemin + query ──▶ Route (pur)
│   ├── src/handler.rs  Route + données ──▶ Response (pur)
│   └── src/server.rs   boucle tiny_http, en-têtes de sécurité
└── eyeship-cli/        + commande `serve`, + `--deployment`, + `[deployments]` dans la config
```

Les pages HTML sont un adaptateur de sortie comme le texte et le JSON : elles vivent dans `eyeship-report`, ce qui sert aussi l'export `--format html` (US6). `eyeship-web` dépend de `eyeship-domain` et `eyeship-report` mais pas de `eyeship-compose` : il reçoit des résultats d'audit déjà calculés à travers un port, ce qui permettra de lui donner des résultats Kubernetes plus tard (P7).

```rust
/// Port: what the web adapter needs from the application.
pub trait DeploymentSource {
    fn deployments(&self) -> Vec<DeploymentResult>;
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

## 4. Pages et routes

Uniquement `GET` (et `HEAD`). Toute autre méthode : `405`. Route inconnue : `404` avec une page lisible.

| Route | Page | Récit |
|---|---|---|
| `/` | Tableau de bord : un bloc par déploiement (statut, compteurs, fichiers), échecs en premier | US2 |
| `/deployments/{name}?severity=warning&service=api` | Rapport : résumé, constats groupés par service, filtres | US3 |
| `/rules` | Catalogue des règles | US4 |
| `/rules/{id}` | Explication d'une règle | US4 |
| `/assets/style.css` | Feuille de style, en cache long avec hash dans l'URL | |
| `/health` | `ok`, pour un `healthcheck` Compose ou un test | |

- **Sans JavaScript** (ENF6) : les filtres sont un formulaire `GET` ; l'URL porte l'état.
- **Rendu serveur** par des fonctions pures `fn page(data) -> String`. Chaque valeur issue d'un fichier analysé passe par `escape()` (`& < > " '`). Un test vérifie qu'un nom de service `<script>` ressort échappé sur chaque page.
- **Style** : une feuille CSS écrite à la main (< 10 Ko), variables CSS, `prefers-color-scheme`, grille fluide pour mobile. Les gravités ont une couleur **et** un libellé (pas de couleur seule).
- **Déterminisme** : mêmes données, même HTML ; testé par snapshots `insta`.

## 5. Serveur

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

## 6. Export HTML (US6)

`eyeship compose --format html` réutilise la fonction de la page rapport avec une mise en page « autonome » : CSS en ligne dans `<style>`, aucun lien vers `/rules` (les explications des règles concernées sont incluses en bas de page, dépliables avec `<details>`). La sortie n'a ni date ni chemin absolu, pour rester déterministe (P4).

## 7. Dépendances

| Crate | Usage | Où | Justification |
|---|---|---|---|
| `tiny_http` | Serveur HTTP/1.1 synchrone | eyeship-web | Petit, sans `tokio`, sans TLS par défaut. Analyser HTTP correctement à la main est une source de failles. **D2** |
| *(aucune)* | Gabarits HTML | eyeship-report | Fonctions Rust + `format!` + `escape()`. Les pages sont peu nombreuses. Alternative : `maud` (macros, échappement automatique). **D3** |
| *(aucune)* | Front-end | | HTML + CSS, pas de framework JS ni de WASM, pas de chaîne de build Node. **D3** |

Rien d'autre. Pas d'`axum`/`actix` (tirent `tokio`), pas de `rust-embed` (`include_str!` suffit pour un fichier).

## 8. Stratégie de test (TDD)

1. **Pages** : tests unitaires sur des `Report` construits avec les builders existants ; snapshots `insta` du HTML ; test d'échappement sur chaque page ; test « aucune URL externe » (pas de `http://` ni `https://` dans le HTML et le CSS hors liens de documentation).
2. **Routeur et gestionnaire** : fonctions pures, tables de cas (`/deployments/prod?severity=error` → `Route::Deployment { … }`, chemin inconnu → 404, méthode `POST` → 405, `Host` étranger → 421).
3. **Configuration** : parsing des `[deployments]`, noms invalides, chemins relatifs, déploiement implicite.
4. **Équivalence CLI / web** : pour chaque fixture de la spec 001, les constats de la page rapport = ceux de `--format json` (critère de succès §6).
5. **Bout en bout** : `eyeship serve --port 0` lancé par `assert_cmd`, port lu sur stderr, requêtes HTTP avec `std::net::TcpStream` (pas de client HTTP en dépendance), arrêt du processus.
6. **Performance** : page rapport du fichier de 50 services du benchmark ENF1 en moins de 100 ms ; taille du binaire release mesurée avant et après (≤ +1 Mo).

## 9. Décisions à valider

| # | Décision | Recommandation | Alternative |
|---|---|---|---|
| D1 | Source des déploiements | Déclarés dans `.eyeship.toml`, audités à chaque requête | Rapports JSON enregistrés (historique) ; stacks en cours via Docker (déroge à P3) |
| D2 | Serveur HTTP | `tiny_http`, synchrone | `axum` + `tokio` (lourd) ; HTTP écrit à la main sur `std::net` (zéro dépendance, risqué) |
| D3 | Front-end | Rendu serveur, HTML + CSS sans JS, gabarits en fonctions Rust | `maud` pour les gabarits ; SPA (Leptos/WASM ou TypeScript) : chaîne de build et poids sans bénéfice ici |
| D4 | Servi par la CLI | Sous-commande `eyeship serve` du binaire unique | Binaire séparé `eyeship-web` |
| D5 | Adresse par défaut | `127.0.0.1:7878` | `0.0.0.0` (exposé au réseau, refusé par ENF4) |

## 10. Risques

| Risque | Parade |
|---|---|
| Faille XSS via un nom de service ou un message | Échappement systématique, test dédié par page, CSP stricte sans script. |
| Lecture de fichiers arbitraires via l'URL | Aucune route ne prend de chemin : seuls les déploiements déclarés sont audités. |
| Site malveillant qui interroge l'interface locale (DNS rebinding) | Vérification de l'en-tête `Host`, `GET` seulement, aucune action à effets de bord. |
| Dérive entre CLI et web | Construction partagée de la requête et de la config (§3), test d'équivalence (§8.4). |
| `tiny_http` peu actif (dernière version 0.12, un fork `tiny_http_dh` annonce des correctifs de CVE) | Vérifier les avis RustSec en T002 et choisir entre l'original et le fork ; API réduite utilisée derrière `server.rs`, remplaçable sans toucher aux pages ni au routeur. |
