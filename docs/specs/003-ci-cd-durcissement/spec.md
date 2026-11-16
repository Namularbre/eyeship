# Spec 003 : Durcir les pipelines CI/CD

| Champ | Valeur |
|---|---|
| Statut | Brouillon, à valider |
| Date | 2026-11-16 |
| Constitution | [`docs/constitution.md`](../../constitution.md) |
| Origine | Écarts E1 à E8 du [plan 002 §6](../002-ci-cd/plan.md#6-écarts-constatés) |

> Cette spec décrit **quoi** et **pourquoi**. Les choix techniques sont dans [`plan.md`](./plan.md). L'existant est décrit dans la [spec 002](../002-ci-cd/spec.md).

## 1. Problème

`eyeship` existe pour trouver ce qui fera tomber une application avant la production. Ses propres pipelines ont aujourd'hui les défauts qu'il reproche aux fichiers Compose :

- on peut publier une release depuis un commit dont la CI est rouge (E1) ;
- ce qui est publié n'est jamais lancé avant d'être distribué (E2) ;
- `latest` peut désigner une pré-release (E3) ;
- les actions tierces ne sont pas épinglées, l'équivalent de notre règle R008 (E4) ;
- la version minimale de Rust annoncée dans le README n'est pas vérifiée (E5) ;
- le budget de performance ENF1 n'est vérifié qu'à la main (E6) ;
- des exécutions de CI obsolètes tournent pour rien (E7) ;
- les dépendances vieillissent sans que personne ne le voie (E8).

Un outil de fiabilité qui livre de façon non fiable perd sa crédibilité.

## 2. Personas

- **Le mainteneur** pousse un tag et veut être sûr que ce qui part est ce qui a été testé, et que ça fonctionne.
- **Jean** (persona de la spec 001) installe `eyeship` via un binaire, `cargo install` ou `docker run`, et veut que ça marche du premier coup avec la version qu'il croit avoir.

## 3. Récits utilisateur

### US1 (P1) : Ne jamais publier une release non vérifiée [E1]
*En tant que mainteneur, je veux qu'une release ne puisse être publiée que si toute la CI passe sur le commit tagué.*

Critères d'acceptation :
- Le workflow de release lance les mêmes vérifications que la CI (fmt, clippy, tests sur les trois systèmes) avant de créer la release.
- Si une vérification échoue, aucune release GitHub n'est créée, aucun binaire ni aucune image n'est publié.
- Si le tag (`v1.2.3`) ne correspond pas à la version du `Cargo.toml`, la release échoue avec un message clair.

### US2 (P1) : Ne publier que des artefacts qui fonctionnent [E2]
*En tant que Jean, je veux que le binaire et l'image que je télécharge démarrent et auditent réellement un fichier.*

Critères d'acceptation :
- Chaque binaire est lancé avant d'être envoyé : `eyeship --version` affiche la version du tag, et `eyeship compose examples/compose.yaml` produit des constats et sort avec le code 1.
- L'image Docker subit les mêmes vérifications, sur chaque architecture, avant d'être poussée.
- Un artefact qui échoue n'est pas publié.

### US3 (P1) : `latest` désigne toujours une version stable [E3]
*En tant que Jean, je veux que `ghcr.io/namularbre/eyeship:latest` et la dernière release GitHub soient des versions stables.*

Critères d'acceptation :
- Un tag avec un suffixe de pré-release (`v1.0.0-rc.1`) publie l'image sous son numéro exact uniquement, sans toucher `latest` ni le tag `major.minor`.
- La release GitHub correspondante est marquée « pre-release ».

### US4 (P2) : Des pipelines reproductibles [E4, E8]
*En tant que mainteneur, je veux que les pipelines n'exécutent que du code tiers que j'ai explicitement accepté, et être prévenu quand une mise à jour existe.*

Critères d'acceptation :
- Chaque action tierce est référencée par un SHA de commit complet, avec la version lisible en commentaire.
- Une PR de mise à jour est proposée automatiquement, au plus une fois par semaine, pour les actions GitHub et pour les dépendances Cargo.
- Les mises à jour mineures et correctives sont groupées pour limiter le bruit.

### US5 (P2) : Tenir les promesses du README [E5, E6]
*En tant que Jean, je veux que la version minimale de Rust et les performances annoncées soient vraies.*

Critères d'acceptation :
- La version minimale de Rust est déclarée dans le `Cargo.toml` et vérifiée par la CI : si une dépendance ou du code l'augmente, la CI échoue.
- Le test de performance ENF1 (50 services en moins de 100 ms) tourne dans la CI, sur un build optimisé.

### US6 (P3) : Une CI économe [E7]
*En tant que mainteneur, je veux qu'un nouveau push sur une PR annule l'exécution précédente devenue inutile.*

Critères d'acceptation :
- Sur une PR, une exécution en cours est annulée quand un nouveau commit arrive.
- Sur `main`, aucune exécution n'est annulée : chaque commit garde son verdict.

## 4. Exigences non fonctionnelles

| # | Exigence |
|---|---|
| ENF-CI1 | Toujours aucun secret à configurer, hors `GITHUB_TOKEN` (spec 002). |
| ENF-CI2 | Une seule définition des vérifications : la CI et la release ne peuvent pas diverger. |
| ENF-CI3 | La durée de la CI d'une PR n'augmente pas de plus de quelques minutes. |

## 5. Critères de succès

- Un tag poussé sur un commit qui ne compile pas ne publie rien.
- La première release `v0.1.0` est faite avec ces pipelines, et ses 5 binaires et son image passent les vérifications.
- Aucune action n'est référencée par un tag mobile.

## 6. Hors périmètre

- Signature des binaires et des images (cosign, attestations de provenance), SBOM : utile, mais demande une spec dédiée.
- Scan de vulnérabilités (constitution §9 : on délègue).
- Publication sur crates.io, Homebrew, Scoop.
- Utiliser `eyeship` pour auditer ses propres fichiers : le dépôt ne contient pas de Compose de production.

## 7. Questions ouvertes

- ~~Q0~~ : la MSRV annoncée (1.85) était fausse, la vraie est 1.89 (voir plan §6).
- Q1 : Le test de performance sur un runner GitHub partagé est-il assez stable ? Mesure actuelle : ~6 ms pour un budget de 100 ms, la marge semble suffisante (voir plan, décision D4).
