# Spec 004 : Interface web des déploiements et des rapports

| Champ | Valeur |
|---|---|
| Statut | Brouillon, à valider |
| Date | 2026-11-26 |
| Constitution | [`docs/constitution.md`](../../constitution.md) (amendement §2 proposé, voir §8) |

> Cette spec décrit **quoi** et **pourquoi**. Les choix techniques sont dans [`plan.md`](./plan.md).

## 1. Problème

`eyeship compose` répond bien à la question « qu'est-ce qui va faire tomber ma stack ? » pour **un** projet, dans un terminal. Trois limites apparaissent dès qu'on a plusieurs environnements :

- Jean a une stack de **staging** et une de **production** (fichiers de surcharge et `.env` différents). Il doit retenir et retaper les bonnes combinaisons `-f … --env-file …` pour chacune.
- Il n'a **aucune vue d'ensemble** : quel déploiement est sain, lequel a des erreurs, combien de constats par gravité.
- Un rapport texte de 40 constats se lit mal : on ne peut ni filtrer par gravité ou par service, ni passer d'un constat à l'explication de sa règle d'un clic.

La constitution (§2) prévoit une interface web « plus tard ». Ce « plus tard » est maintenant.

## 2. Vocabulaire

- **Déploiement** : un environnement cible nommé (`staging`, `prod`…) défini par la liste de ses fichiers Compose et, éventuellement, son fichier de variables. C'est exactement ce que décrit une ligne `eyeship compose -f … --env-file …`, mais nommé et versionné.
- **Rapport** : le résultat d'un audit d'un déploiement (constats retenus, constats ignorés, résumé par gravité, statut au regard du seuil).

## 3. Personas

- **Jean** (spec 001) : développeur d'un SaaS sur un VPS, sans équipe ops. Il veut voir d'un coup d'œil si ses déploiements sont prêts.
- **Léa** : collègue de Jean, moins à l'aise avec la ligne de commande. Elle lit les rapports et corrige les constats, sans connaître les options de la CLI.

## 4. Récits utilisateur

### US1 (P1) : Déclarer ses déploiements
*En tant que Jean, je décris mes déploiements une fois, dans le dépôt, pour ne plus retaper les options.*

Critères d'acceptation :
1. **Étant donné** un `.eyeship.toml` contenant une section par déploiement (nom, fichiers Compose, fichier de variables optionnel, seuil optionnel), **quand** je lance l'interface, **alors** chaque déploiement déclaré y apparaît.
2. **Étant donné** un `.eyeship.toml` sans déploiement, **alors** l'interface montre un déploiement implicite `default` : le fichier Compose détecté dans le dossier, comme `eyeship compose` sans argument.
3. Un déploiement mal déclaré (fichier absent, YAML invalide) apparaît **en erreur**, avec le message de lecture, sans empêcher l'affichage des autres.
4. Les règles ignorées du `.eyeship.toml` (globales et par service) s'appliquent à tous les déploiements, comme dans la CLI.

### US2 (P1) : Voir l'état de tous les déploiements
*En tant que Jean, j'ouvre une page et je vois lesquels de mes déploiements sont prêts pour la production.*

Critères d'acceptation :
1. `eyeship serve` lance l'interface et affiche l'adresse à ouvrir.
2. La page d'accueil liste les déploiements, chacun avec : son statut (**passe** ou **échoue** au regard de son seuil, ou **illisible**), le nombre de constats par gravité, le nombre de constats ignorés et la liste de ses fichiers.
3. Les déploiements en échec apparaissent en premier ; l'ordre est ensuite alphabétique et stable.
4. Après une modification d'un fichier Compose, recharger la page montre le nouveau résultat, sans relancer `eyeship serve`.

### US3 (P1) : Lire le rapport d'un déploiement
*En tant que Léa, je lis les constats d'un déploiement, je les filtre et je comprends comment corriger chacun.*

Critères d'acceptation :
1. La page d'un déploiement montre les mêmes constats que `eyeship compose` avec les mêmes options, dans le même ordre (constitution P4).
2. Chaque constat affiche : gravité, identifiant de règle, service, fichier et ligne, message et correction (constitution P5).
3. On peut filtrer par gravité et par service ; le filtre est dans l'adresse de la page, pour pouvoir la partager.
4. Le résumé rappelle le nombre de constats ignorés par configuration (constitution P6).
5. Un clic sur l'identifiant d'une règle ouvre sa page d'explication.

### US4 (P1) : Comprendre une règle
*En tant que Léa, je veux le scénario de panne et la correction d'une règle, sans ouvrir de terminal.*

Critères d'acceptation :
1. Une page liste toutes les règles (identifiant, gravité par défaut, titre), comme `eyeship rules`.
2. La page d'une règle montre le même contenu que `eyeship explain <ID>` : scénario de panne, correction, exemple fautif, exemple corrigé.

### US5 (P2) : Auditer un déploiement depuis la CLI par son nom
*En tant que Jean, je veux lancer en CI le même audit que celui de l'interface.*

Critères d'acceptation :
1. `eyeship compose --deployment prod` audite le déploiement `prod` déclaré dans `.eyeship.toml`.
2. Le résultat (constats, code de sortie) est identique à celui affiché par l'interface pour ce déploiement.
3. `--deployment` combiné à un fichier, `-f` ou `--env-file` est une erreur d'utilisation (code 2).

### US6 (P2) : Exporter un rapport en HTML
*En tant que Jean, je veux joindre un rapport lisible aux artefacts de ma CI, pour que Léa le consulte sans installer `eyeship`.*

Critères d'acceptation :
1. `eyeship compose --format html` écrit sur la sortie standard une page HTML autonome (aucune ressource externe) avec le même contenu que la page d'un déploiement.
2. La sortie est déterministe : même entrée, même page, octet pour octet.

## 5. Exigences non fonctionnelles

- **ENF1** : l'interface est servie par le binaire `eyeship` lui-même. Rien d'autre à installer (pas de Node, pas de base de données, pas de second binaire). Constitution §7.
- **ENF2** : hors ligne. Les pages n'appellent aucune ressource externe (police, CDN, statistiques). Aucun démon Docker requis. Constitution P3.
- **ENF3** : **lecture seule**. L'interface ne modifie jamais un fichier analysé ni `.eyeship.toml`.
- **ENF4** : **sûre par défaut**. Elle n'écoute que sur la machine locale ; s'ouvrir au réseau demande une option explicite et affiche un avertissement. Tout contenu issu des fichiers analysés est échappé.
- **ENF5** : légère. Une page de 50 services s'affiche en moins de 100 ms ; le serveur au repos consomme moins de 20 Mo de RAM. Le binaire ne grossit pas de plus de 1 Mo.
- **ENF6** : utilisable sans JavaScript ; lisible sur mobile ; thème clair et sombre suivant le système ; accessible au clavier.
- **ENF7** : textes de l'interface en anglais, comme les messages de l'outil (constitution §5).

## 6. Critères de succès

- Jean déclare `staging` et `prod`, lance `eyeship serve` et voit leurs statuts en moins d'une minute, sans lire la documentation.
- Léa corrige un constat en partant de la page d'accueil, sans terminal.
- Pour chaque fixture de la spec 001, la page d'un déploiement liste exactement les constats de `eyeship compose --format json`.

## 7. Hors périmètre de cette spec

| Idée | Pourquoi plus tard |
|---|---|
| Historique des rapports (évolution dans le temps, comparaison entre deux audits) | Demande de stocker des rapports. Spec suivante, une fois l'interface en place. |
| Lire les stacks **en cours d'exécution** via le démon Docker | Contraire à P3 dans le mode par défaut ; ce serait un mode séparé et explicite. |
| Modifier les fichiers ou `.eyeship.toml` depuis l'interface, `--fix` | ENF3 ; constitution §9. |
| Authentification, multi-utilisateur, hébergement partagé | L'interface est locale. Pour la partager : l'export HTML (US6). |
| Kubernetes | Viendra avec l'adaptateur Kubernetes ; l'interface parlera du modèle commun (P7). |
| Lancer un audit depuis un formulaire (fichiers arbitraires) | Les déploiements déclarés couvrent le besoin et évitent de lire n'importe quel chemin. |

## 8. Amendement de la constitution proposé

§2, troisième point, remplacé par :

> - Sur le web, via une interface **locale** servie par le binaire (`eyeship serve`) : vue des déploiements et lecture des rapports. Lecture seule, hors ligne, sans démon Docker.

Aucun autre principe ne change.

## 9. Questions ouvertes

- **Q1** : source des déploiements. Proposé : déclarés dans `.eyeship.toml` et audités à chaque affichage (§4 US1). Alternatives : afficher des rapports JSON enregistrés (historique), ou les stacks en cours via Docker.
- **Q2** : nom de la commande : `eyeship serve` (proposé) ou `eyeship ui` ?
- **Q3** : US5 et US6 en P2 dans cette spec, ou dans une spec séparée ?
