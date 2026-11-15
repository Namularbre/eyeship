# Constitution

> Ce document fixe les principes non négociables du projet. Toute spec, tout plan et toute tâche doivent les respecter. Une dérogation se discute ici, par une modification explicite de la constitution, jamais en silence dans le code.

Nom de l'outil : **`eyeship`**, prononcé « I ship ». Triple sens : l'œil qui surveille (*eye*), le navire (*ship*), et « je livre en prod » (*I ship*).

## 1. Mission

Créer un outil en ligne de commande qui fiabilise les déploiements d'applications sur Docker, Docker Compose et Kubernetes, en trouvant **avant la mise en production** les causes probables de plantage.

L'outil répond à une seule question : *« Qu'est-ce qui, dans cette configuration, risque de faire tomber mon application en production ? »*

## 2. Où l'utiliser

- Sur son PC, en local, pour tester avant de déployer.
- Dans un pipeline CI/CD, pour bloquer automatiquement une configuration risquée.
- Plus tard, sur le web via une interface (dashboard). Hors MVP : l'architecture hexagonale doit simplement le rendre possible sans réécriture.

## 3. Positionnement

- **Fiabilité au sens large** : stabilité **et** sécurité. Une faille exploitée ou un conteneur tué par l'OOM killer sont deux façons de tomber.
- **La valeur propre est la stabilité.** Les outils existants (Hadolint, Dockle, Trivy, dclint, kube-score, kube-linter, Polaris, Popeye, Checkov, KICS) couvrent surtout la sécurité et le style. On ne les réécrit pas : on pourra les appeler et agréger leurs résultats plus tard.
- **Docker Compose d'abord.** C'est le terrain le moins couvert, et celui de beaucoup d'équipes qui déploient en production sans Kubernetes (un SaaS sur un VPS, par exemple).

## 4. Principes produit

### P1. Chaque règle explique un plantage réel
Une règle n'existe que si l'on peut décrire le scénario de production qu'elle prévient (« le conteneur redémarre en boucle », « l'API démarre avant la base et crashe »). Chaque règle documente : le problème, le scénario de panne, la correction, un exemple fautif et un exemple corrigé. Les règles de pur style sont en sévérité `info` au plus.

### P2. Zéro faux positif plutôt que l'exhaustivité
Un outil qui crie au loup est désactivé au bout d'une semaine. En cas de doute, une règle se tait ou baisse sa sévérité. Chaque règle a des tests sur des cas négatifs (configurations correctes qui ne doivent **pas** déclencher).

### P3. Analyse statique, hors ligne, sans effet de bord
L'audit ne lance aucun conteneur, n'appelle aucun démon Docker et n'a pas besoin du réseau pour les règles de base. Il ne modifie jamais les fichiers analysés. Les vérifications qui demandent le réseau ou un conteneur en marche seront des modes séparés et explicites.

### P4. Utilisable en CI dès le premier jour
- Codes de sortie stables et documentés.
- Sortie lisible par un humain **et** par une machine (JSON, puis SARIF).
- Seuil de sévérité configurable pour faire échouer un pipeline.
- Résultat déterministe : même entrée, même sortie, dans le même ordre.

### P5. Messages actionnables
Chaque constat indique **où** (fichier, service, ligne), **pourquoi** c'est risqué et **comment** corriger.

### P6. Silence explicite, jamais implicite
L'utilisateur peut ignorer une règle, mais volontairement et de façon traçable. Les constats ignorés restent comptés.

### P7. Extensible par sources
Le cœur (règles, constats, rendu) ne dépend pas du format analysé. Ajouter Kubernetes ou un Dockerfile, c'est ajouter un adaptateur et des règles, pas réécrire le moteur.

## 5. Langage et style de code

- **Rust**, en étant le plus idiomatique possible.
- **Pas de `unsafe`** (`#![forbid(unsafe_code)]` dans chaque crate).
- **Paradigme fonctionnel au maximum** : fonctions pures, données immuables, itérateurs plutôt que boucles mutables, erreurs portées par `Result` (pas de `panic!` ni de `unwrap()` hors tests).
- **On code en anglais** (identifiants, commentaires, messages de l'outil). La documentation du projet est en français.
- **Petits modules**, quitte à en faire des crates séparés (workspace Cargo).
- **Architecture hexagonale** : le domaine (modèle, règles) ne connaît ni le YAML, ni le système de fichiers, ni le terminal. Les entrées (lecture de fichiers Compose) et les sorties (texte, JSON) sont des adaptateurs.

## 6. Tests

- **Le TDD est obligatoire** : un test rouge avant chaque comportement.
- Pas de couverture abusive, mais **toute la logique métier est testée** : chaque règle a des tests positifs **et** négatifs.
- Les tests de bout en bout comparent la sortie de la CLI à des fichiers de référence (*snapshots*).
- Les identifiants de règles sont stables : une règle peut être dépréciée, jamais renumérotée.

## 7. Contraintes

- Pas de framework ni de bibliothèque lourde. Chaque dépendance se justifie dans le plan.
- Code performant et lisible : l'outil doit tourner sur de très petites configurations (petit VPS, runner CI minimal).
- Distribution simple : un seul binaire statique, plus une image Docker minimale.

## 8. Démarche : Spec-Driven Development

1. **Spec** (`spec.md`) : le *quoi* et le *pourquoi*, du point de vue de l'utilisateur. Aucun choix technique.
2. **Plan** (`plan.md`) : le *comment*, avec les choix techniques justifiés.
3. **Tâches** (`tasks.md`) : découpage en petites tâches ordonnées et testables.
4. **Implémentation** : une tâche à la fois, en TDD.

Chaque spec est numérotée et vit dans son dossier : `docs/specs/NNN-nom/{spec,plan,tasks}.md`. Toute évolution de comportement commence par une mise à jour de la spec.

## 9. Hors périmètre permanent

- Corriger automatiquement les fichiers sans demande explicite (un futur `--fix` sera opt-in).
- Surveiller une production en cours d'exécution (c'est le rôle de l'observabilité).
- Remplacer un scanner de vulnérabilités d'images (on délègue à Trivy ou équivalent).
