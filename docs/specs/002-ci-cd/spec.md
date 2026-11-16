# Spec 002 : Pipelines CI/CD du projet

| Champ | Valeur |
|---|---|
| Statut | Rétro-documentation de l'existant, 2026-11-16 |
| Date | 2026-11-16 |
| Constitution | [`docs/constitution.md`](../../constitution.md) |
| Origine | Tâches T002 et T047 de la [spec 001](../001-audit-compose/tasks.md) |

> Cette spec décrit **quoi** et **pourquoi** pour les pipelines qui construisent, vérifient et livrent `eyeship` lui-même. Elle a été écrite **après** leur mise en place, pour décrire l'existant. Les choix techniques sont dans [`plan.md`](./plan.md).
>
> À ne pas confondre avec l'usage d'`eyeship` **dans la CI des utilisateurs** (US2 de la spec 001, section « GitHub Actions integration » du README).

## 1. Problème

`eyeship` promet d'être fiable, déterministe et multiplateforme (constitution P4, §5, §6, §7). Ces promesses ne tiennent que si elles sont vérifiées à chaque changement, sur chaque système visé, et si la livraison est reproductible :

- un `unwrap()` ou un avertissement oublié passe en revue sans qu'on le voie ;
- un test passe sous Linux mais casse sous Windows (séparateurs de chemin, fins de ligne) ;
- un snapshot de sortie change sans que personne ne l'ait voulu ;
- une release faite à la main oublie une plateforme, ou publie un binaire différent de l'image Docker.

## 2. Personas

- **Le mainteneur** veut savoir, avant de fusionner une PR, que le code respecte la constitution (formatage, lint strict, tests) sur les trois systèmes.
- **Jean** (persona de la spec 001) veut installer `eyeship` sans Rust : un binaire pour sa plateforme, ou une image Docker à lancer dans sa CI.

## 3. Récits utilisateur

### US1 (P1) : Vérifier chaque changement
*En tant que mainteneur, je veux que chaque push sur la branche principale et chaque PR soient vérifiés automatiquement.*

Critères d'acceptation :
- Le formatage est contrôlé (`cargo fmt --check`) : un code mal formaté fait échouer la CI.
- Le lint est strict : tout avertissement (compilateur ou clippy), y compris dans les tests, fait échouer la CI.
- Toute la suite de tests (unitaires, bout en bout, snapshots, corpus réel) tourne sous **Linux, macOS et Windows**.
- L'échec sur un système n'empêche pas de voir le résultat des autres.

### US2 (P1) : Publier une release en poussant un tag
*En tant que mainteneur, je veux qu'un tag `vX.Y.Z` suffise à publier une release complète.*

Critères d'acceptation :
- Une release GitHub est créée pour le tag.
- Un binaire statique est attaché pour chaque plateforme de la constitution §7 : Linux musl x86_64 et aarch64, macOS x86_64 et aarch64, Windows x86_64.
- Chaque archive a sa somme de contrôle SHA-256.
- Aucun secret à configurer : seul le jeton fourni par GitHub est utilisé.

### US3 (P1) : Publier une image Docker minimale
*En tant que Jean, je veux une image Docker de quelques Mo pour lancer `eyeship` dans ma CI sans rien installer.*

Critères d'acceptation :
- L'image est publiée sur `ghcr.io/namularbre/eyeship` à chaque release.
- Elle est multi-architecture (`linux/amd64`, `linux/arm64`).
- Elle contient le binaire statique seul (`FROM scratch`), avec `/work` comme dossier de travail.
- Tags : version complète (`0.1.0`), version mineure (`0.1`) et `latest`.

## 4. Exigences non fonctionnelles

| # | Exigence |
|---|---|
| ENF-CI1 | La CI n'a besoin d'aucun secret : elle tourne de la même façon sur un fork. |
| ENF-CI2 | Les commandes de la CI sont celles de la section « Development » du README : un développeur reproduit la CI en local. |
| ENF-CI3 | La compilation est mise en cache entre deux exécutions pour garder la CI rapide. |

## 5. Hors périmètre (état actuel)

Ces points ne sont **pas** couverts par les pipelines existants. Ils sont listés pour être explicites, pas comme engagement (voir [`plan.md`](./plan.md) §6) :

- Benchmark ENF1 (`tests/perf.rs`, `scripts/bench.sh`) : lancé à la main uniquement.
- Version minimale de Rust (README : ≥ 1.85) : non vérifiée.
- Publication sur crates.io : le workspace est en `publish = false`.
- Signature des binaires et des images, SBOM, scan de vulnérabilités.
- Déploiement : `eyeship` est un outil en ligne de commande, il n'y a rien à déployer. Le « CD » se limite à la publication des artefacts.
