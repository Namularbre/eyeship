# Plan 001 : Audit Compose, plan technique

| Champ | Valeur |
|---|---|
| Spec | [`spec.md`](./spec.md) |
| Statut | Validé le 2026-11-15 |

> Ce plan décrit **comment** livrer la spec 001 en respectant la [constitution](../../constitution.md) : Rust idiomatique, sans `unsafe`, fonctionnel, hexagonal, TDD, dépendances légères.

## 1. Vue d'ensemble

```
             adaptateurs d'entrée                cœur (pur)                 adaptateurs de sortie
┌──────────────┐   ┌──────────────────┐   ┌───────────────────────┐   ┌──────────────────┐
│ eyeship-cli  │──▶│ eyeship-compose  │──▶│    eyeship-domain     │──▶│  eyeship-report  │
│ args, codes  │   │ YAML → modèle    │   │ modèle, règles,       │   │ texte, JSON      │
│ de sortie    │   │ interpolation    │   │ moteur, constats      │   │ (SARIF plus tard)│
└──────────────┘   │ faits du disque  │   └───────────────────────┘   └──────────────────┘
                   └──────────────────┘
```

Le flux est une suite de fonctions :

```
fichiers ──load──▶ Project ──audit(rules, config)──▶ Report ──render(format)──▶ texte/JSON
                                                         └──exit_code(fail_on)──▶ 0 / 1 / 2
```

## 2. Workspace Cargo

```
eyeship/
├── Cargo.toml                 # [workspace]
├── crates/
│   ├── eyeship-domain/        # cœur pur, zéro dépendance
│   ├── eyeship-compose/       # adaptateur d'entrée Compose
│   ├── eyeship-report/        # adaptateurs de sortie (texte, JSON)
│   └── eyeship-cli/           # binaire `eyeship`, câblage
├── tests/fixtures/compose/    # fichiers Compose d'exemple, par règle
└── docs/
```

Le `src/main.rs` actuel migre dans `crates/eyeship-cli`. Chaque crate a `#![forbid(unsafe_code)]`.

## 3. Le domaine (`eyeship-domain`)

Aucune dépendance externe. Ne connaît ni YAML, ni système de fichiers, ni terminal.

### 3.1 Modèle

Un modèle **indépendant de Compose**, pour accueillir Kubernetes plus tard (constitution P7). L'adaptateur le remplit avec des *faits* déjà résolus, pour que les règles restent des fonctions pures.

```rust
pub struct Project {
    pub services: Vec<Service>,
    pub networks: Vec<Declared>,       // réseaux déclarés à la racine
    pub volumes: Vec<Declared>,        // volumes nommés déclarés à la racine
    pub has_version_key: Option<Location>,
    pub unresolved_vars: Vec<UnresolvedVar>,
}

pub struct Service {
    pub name: String,
    pub location: Location,
    pub image: Option<ImageRef>,       // registre, nom, tag, digest
    pub has_build: bool,
    pub container_name: Option<Located<String>>,
    pub restart: Option<Located<RestartPolicy>>,
    pub memory_limit: Option<Located<u64>>,
    pub logging: Option<Logging>,
    pub healthcheck: Option<Healthcheck>,
    pub depends_on: Vec<Dependency>,   // nom, condition, location
    pub ports: Vec<PortMapping>,       // ip hôte, plage hôte, protocole
    pub networks: Vec<Located<String>>,
    pub mounts: Vec<Mount>,            // Named | Bind { host_path: HostPath } | Malformed
    pub files: Vec<HostPath>,          // env_file, secrets, configs de type fichier
}

pub struct HostPath { pub path: String, pub exists: bool, pub location: Location }
pub struct Location { pub file: FileId, pub line: u32, pub column: u32 }
```

Les faits qui demandent le monde extérieur (un chemin existe-t-il, une variable est-elle définie) sont calculés par l'adaptateur et stockés dans le modèle (`HostPath::exists`, `unresolved_vars`). Les règles n'ont donc jamais besoin d'entrées/sorties.

### 3.2 Constats et règles

```rust
pub enum Severity { Info, Warning, Error }   // Ord : Info < Warning < Error

pub struct Finding {
    pub rule: RuleId,          // "R001"
    pub severity: Severity,
    pub service: Option<String>,
    pub location: Location,
    pub message: String,       // anglais, court
    pub fix: String,           // correction proposée
}

pub struct Rule {
    pub id: RuleId,
    pub title: &'static str,
    pub severity: Severity,
    pub doc: RuleDoc,          // scénario, correction, exemples (pour `explain`)
    pub check: fn(&Project) -> Vec<Finding>,
}

pub static RULES: &[Rule] = &[r001::RULE, r002::RULE, /* ... */];
```

Une règle = un module `rules/rNNN.rs` = une fonction pure + ses métadonnées + ses tests unitaires. Pas de trait objet ni d'état : un tableau statique de structures suffit.

### 3.3 Moteur

```rust
pub fn audit(project: &Project, rules: &[Rule], config: &AuditConfig) -> Report
```

- applique les règles actives, sépare les constats retenus et ignorés (constitution P6) ;
- trie de façon déterministe : sévérité décroissante, puis fichier, ligne, colonne, ID de règle ;
- `Report::exit_code(fail_on) -> ExitCode` : `0` ou `1`. Le code `2` (erreur de lecture) est produit par la CLI.

### 3.4 Ports (traits)

```rust
pub trait ProjectLoader {               // implémenté par eyeship-compose, plus tard eyeship-k8s
    fn load(&self, request: &LoadRequest) -> Result<Project, LoadError>;
}
pub trait Renderer {                    // implémenté par eyeship-report
    fn render(&self, report: &Report, out: &mut dyn std::io::Write) -> std::io::Result<()>;
}
```

## 4. L'adaptateur Compose (`eyeship-compose`)

Responsabilités, chacune dans son module et testée séparément :

1. **Découverte** : chercher `compose.yaml`, `compose.yml`, `docker-compose.yaml`, `docker-compose.yml` (ordre de priorité de Docker Compose).
2. **Lecture YAML avec positions** : chaque nœud garde sa ligne et sa colonne.
3. **Interpolation** : `${VAR}`, `${VAR:-défaut}`, `${VAR-défaut}`, `${VAR:?erreur}`, `$$`. Sources : environnement du processus, puis `.env` ou `--env-file`. Une variable non résolue devient un `UnresolvedVar` (R013), jamais une erreur bloquante.
4. **Normalisation** : formes courtes et longues vers le même modèle (`depends_on` liste ou map, `ports` chaîne ou objet, volumes courts ou longs, `mem_limit` ou `deploy.resources.limits.memory`, tailles `512m` / `1g`).
5. **Faits du disque** : existence des chemins relatifs, résolus par rapport au dossier du fichier Compose, via un petit trait `FileSystem` pour rester testable sans disque.
6. **Fusion multi-fichiers** (US5, P2) : règles de fusion de Compose.

L'accès au disque et à l'environnement passe par des traits (`FileSystem`, `Env`), avec une implémentation réelle et une implémentation en mémoire pour les tests.

## 5. Les sorties (`eyeship-report`)

- **Texte** (par défaut) : groupé par service, couleur si la sortie est un terminal et que `NO_COLOR` n'est pas défini, résumé final. Codes ANSI écrits à la main, sans dépendance.
- **JSON** : schéma versionné.

```json
{
  "schema_version": 1,
  "tool": { "name": "eyeship", "version": "0.1.0" },
  "files": ["compose.yaml"],
  "findings": [
    {
      "rule": "R001", "severity": "warning", "service": "api",
      "location": { "file": "compose.yaml", "line": 12, "column": 7 },
      "message": "depends_on 'db' does not wait for the service to be healthy",
      "fix": "Use `condition: service_healthy` and add a healthcheck to 'db'"
    }
  ],
  "ignored": 0,
  "summary": { "error": 0, "warning": 1, "info": 0 }
}
```

## 6. La CLI (`eyeship-cli`)

```
eyeship compose [FICHIER] [-f FICHIER]... [--env-file FICHIER]
              [--format text|json] [--fail-on error|warning|info|none]
              [--ignore R001,R002]
eyeship rules
eyeship explain <ID>
```

| Code | Signification |
|---|---|
| 0 | Aucun constat au-dessus du seuil |
| 1 | Au moins un constat au-dessus du seuil |
| 2 | Erreur d'utilisation ou de lecture (fichier absent, YAML invalide) |

`main` reste minimal : parser les arguments, appeler `load`, `audit`, `render`, convertir en code de sortie. Toute la logique est dans les autres crates.

## 7. Dépendances

| Crate | Usage | Où | Justification |
|---|---|---|---|
| `serde-saphyr` (+ `serde`) | Lecture YAML vers des structures typées | eyeship-compose | Désérialisation serde construite sur le parseur `saphyr`, en Rust pur. Les formes courtes et longues de Compose deviennent des `enum` serde (`#[serde(untagged)]`). `serde_yaml` est archivé depuis 2024. Condition : obtenir la ligne de chaque valeur (spike T003). Repli : `saphyr` seul, en parcourant les nœuds. **D1** |
| `clap` (derive, features réduites) | Arguments CLI | eyeship-cli | Standard, aide et erreurs gratuites. Alternative minimaliste : `lexopt`. **D2** |
| `serde` + `serde_json` | Sortie JSON | eyeship-report | Le format JSON doit être exact ; inutile de le réécrire. |
| `toml` | `.eyeship.toml` (US4, P2) | eyeship-cli | Ajouté seulement à la phase P2. **D4** |
| `insta` (dev) | Tests snapshot | tests | Sorties CLI comparées à des références. |
| `assert_cmd` (dev) | Tests de bout en bout | tests | Lancer le binaire et vérifier code et sortie. |

Pas de `anyhow`, `tokio`, ni framework : erreurs en `enum` écrites à la main, tout est synchrone.

## 8. Stratégie de test (TDD)

1. **Règles** : tests unitaires sur un `Project` construit en Rust (petits *builders* de test), sans YAML. Pour chaque règle : au moins un cas qui déclenche et un cas correct qui ne déclenche pas (constitution P2).
2. **Adaptateur Compose** : tests de normalisation et d'interpolation sur des chaînes YAML en mémoire, avec `FileSystem` et `Env` en mémoire.
3. **Bout en bout** : `tests/fixtures/compose/RNNN/{bad,good}.yaml`, sortie texte et JSON comparées par snapshot `insta`, codes de sortie vérifiés avec `assert_cmd`.
4. **Corpus réel** : une dizaine de fichiers Compose publics (projets open source connus) pour traquer les faux positifs avant la sortie.

CI (GitHub Actions) : `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, sur Linux, macOS et Windows.

## 9. Performance et distribution

- Une seule lecture, une passe par règle sur un modèle en mémoire : linéaire en taille du fichier.
- Profil release : `lto = true`, `codegen-units = 1`, `strip = true`, `panic = "abort"`, `opt-level = 3` (ou `"z"` si la taille du binaire compte plus).
- Binaires : `x86_64-unknown-linux-musl` (statique), `aarch64-unknown-linux-musl`, macOS, Windows.
- Image Docker : `FROM scratch` avec le binaire musl seul, quelques Mo.

## 10. Décisions à valider

| # | Décision | Recommandation | Alternative |
|---|---|---|---|
| D1 | Bibliothèque YAML | **Validé et confirmé par le spike T003 : `serde-saphyr` 2.1** (voir §12) | `saphyr` seul si les positions manquent |
| D2 | Parsing des arguments | **Validé** : `clap` derive, features réduites | `lexopt`, plus léger mais aide écrite à la main |
| D3 | Découpage | **Validé** : 4 crates dans un workspace dès le départ | Un seul crate avec des modules, découpé plus tard |
| D4 | Format du fichier de config | **Validé** : TOML (`.eyeship.toml`) pour le MVP, YAML possible plus tard | YAML (`.eyeship.yml`), cohérent avec Compose |

## 11. Risques

| Risque | Parade |
|---|---|
| Positions YAML imprécises après interpolation ou fusion | Garder la position du nœud source ; spike T003 avant de s'engager. |
| Les formes de Compose sont nombreuses (syntaxe courte et longue) | Normalisation testée forme par forme ; référence : la spécification Compose. |
| Faux positifs sur des usages légitimes | Corpus réel, sévérités prudentes, `--ignore`. |

## 12. Résultat du spike T003 (serde-saphyr)

Conclusion : **`serde-saphyr` 2.1 convient**, pas de repli sur `saphyr`.

Vérifié sur un Compose avec ancre (`x-defaults: &defaults`), fusion `<<: *defaults`, `depends_on` en liste et en map, `ports` court et long :

- `Spanned<T>` donne ligne et colonne (1-indexées) de chaque valeur. Pour une valeur héritée par `<<`, `referenced` pointe sur la ligne du `<<` dans le service et `defined` sur la ligne de l'ancre : les constats utilisent `referenced`, qui désigne le service fautif.
- Les erreurs de syntaxe ou de type portent une position (`Error::location()`), utile pour T017.

Contraintes d'implémentation retenues :

1. **Pas de `#[serde(untagged)]` autour d'une valeur dont on veut la position interne** : serde met la valeur en tampon et perd les positions. On place `Spanned<…>` *autour* de l'`enum` (ex. `Vec<Spanned<PortEntry>>`), ou on écrit un `Visitor` à la main (`visit_seq` / `visit_map`) quand les éléments internes doivent garder leur position (`depends_on`).
2. **Clés de map avec position** : `Spanned<String>` n'implémente pas `Ord`, donc pas de `BTreeMap<Spanned<String>, _>`. Un petit type `Entries<V>(Vec<(Spanned<String>, V)>)` avec un `visit_map` manuel garde l'ordre du fichier et la position de chaque clé (nom de service, de réseau, de volume).
3. **Pas de `#[serde(flatten)]`** (même raison que 1) ; les clés inconnues et les extensions `x-*` sont simplement ignorées par défaut.
4. Les scalaires Compose peuvent être des nombres ou des chaînes (`mem_limit: 512`, `published: 8080`) : un type `Scalar` accepte les deux et produit une chaîne, interpolée ensuite.
