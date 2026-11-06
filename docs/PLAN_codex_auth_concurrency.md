# Plan de correction des connexions Codex simultanées

Statut au 2026-11-06 : **implémenté et validé sans compte** ; résultats en §8.
Après ces contrôles : déploiement local et retour utilisateur positif dans Zed
(§9), sans acceptation globale C4 à ce stade. C4 a ensuite été clôturée sur
demande explicite de l’utilisateur le 2026-11-06 ; les résultats ci-dessous
restent historiques et n’ajoutent aucune preuve fournisseur.
Correction intégrée à `main` pour la version 0.2.2 ; la branche dédiée est
désormais historique. Les instructions ci-dessous conservent le déroulé de la mission exécutée.
Aucun fichier protégé modifié. Contrat de cette évolution :
[SPEC_codex_auth_concurrency.md](SPEC_codex_auth_concurrency.md).

## 1. Mission et préparation

Travailler sur `fix/codex-auth-concurrency`, créée depuis `main` à `87db544`
après intégration de la branche Codex initiale. Les documents de conception sont
livrés sur cette branche ; ne pas repartir de `feat/codex-acp` ni recréer la branche.
Préserver les changements de l’utilisateur.
Lire AGENTS, contrat V2, PROGRESS, DECISIONS, SPEC/PLAN Codex et la nouvelle SPEC.
Les anciennes instructions d’application des correctifs C0–C3 sont historiques.
Cette correction est un sujet distinct ; ne pas reprendre tout C4.

Lire particulièrement :

- `src/cli/auth.rs` : `run_with`, `login_run_spec`, `codex_login_command`,
  `auth_codex`, `keep_codex_login`, `has_codex_login` et leur test unitaire ;
- `src/cli/update.rs` : enregistrement de la commande dans le lock ;
- `src/container.rs` : arguments du mode interactif ;
- `src/fsutil.rs` : droits, erreurs, écritures atomiques ;
- `tests/codex_auth_test.rs`, `tests/common/mod.rs` (lecture seule) et
  `tests/PROTECTED.sha256`.

Établir la référence avec `git status --short --branch`, contrôle des empreintes
et `make check`. Base de revue : `87db544`, 343 tests réussis, 8 ignorés.
Sous sandbox, les tests socket/watcher peuvent être interdits ; demander la
relance hors restriction au mécanisme d’approbation, jamais affaiblir les tests.
Ne pas utiliser de sous-agents, fusionner, pousser ou déployer pendant ce sujet.

## 2. Reproduire la course sans compte

Ajouter un nouveau test de régression non protégé, par exemple
`tests/codex_auth_concurrency_test.rs`. Utiliser un état jetable commun à deux
processus du vrai binaire, avec config, release Codex et image simulées ; tous
les chemins utilisateur sont redirigés comme dans `TestEnv`.

Créer un double de container dédié dans un nouveau fichier de test non inscrit
au manifeste, ou le générer dans le dossier temporaire du test. Il répond aux
seuls appels nécessaires à `prepare` et simule `run`. Il relève la source du
montage `/home/agent` et l’argument `CODEX_HOME`, puis traduit ce dernier vers
l’état jetable. Il écrit des caches API **synthétiques**, par exemple `candidate-A`
et `candidate-B`, sans utiliser le réseau ni un adaptateur réel. Ne pas modifier
`tests/common/mod.rs`, `fake-container` ou `fake-agent.py`, et ne pas ajouter de
branche de production réservée aux tests.

Piloter l’ordre par des fichiers de rendez-vous distincts par invocation ou des
pipes : « démarré », « candidat écrit », « sortie autorisée ». Attentes bornées
avec diagnostics ; aucun ordre déduit d’un simple délai `sleep`. Nettoyer les
enfants du harnais même si une assertion échoue.

Reproduction attendue sur le code initial : A écrit son candidat et reste en
attente ; B démarre ; constater que B réutilise le même HOME et supprime le
candidat de A. Conserver la preuve résumée dans PROGRESS. Ce test doit devenir
vert après correction ; aucun échec de test protégé n’est attendu.

## 3. Implémentation ciblée

1. Créer une tentative par `tempfile::Builder` avec préfixe `.codex-login-`,
   dans `paths.state_dir("codex")`. Assurer `0700`. Garder son propriétaire
   jusqu’à la fin du traitement ; ne plus supprimer le dossier fixe à l’entrée.
2. Dériver le chemin VM de son seul nom final, sous `/home/agent`. Un petit
   helper privé valide et adapte une copie de `lock.auth_command` selon SPEC §3.
   Tester explicitement une release au format livré avant cette correction.
   Ne pas changer le lock sur disque, son schéma, l’image ou la commande de launch.
3. Construire le `RunSpec` habituel puis lui affecter cette commande privée.
   Garder la signature publique `auth_run_spec` utilisée pour Claude. Aucun
   montage ni option container supplémentaire.
4. Faire recevoir à `keep_codex_login` le chemin explicite du candidat de cette
   tentative. Interdire toute recherche de « dernier dossier » ou relecture du
   chemin fixe. Vérifier fichier régulier, format natif et droits, puis renommer
   directement sur `.codex/auth.json`. Adapter le test unitaire existant.
5. Nettoyer uniquement la tentative possédée. Distinguer publication et nettoyage
   pour respecter SPEC §4, notamment l’avertissement après publication réussie.
   Préserver le diagnostic principal sur erreur, sans afficher de cache.
6. Ajuster le message d’échec du login pour parler de cette tentative, sans
   promettre que le cache global n’a pas changé pendant une connexion concurrente.

Réutiliser `tempfile` et les outils existants. Pas de mutex en mémoire (inefficace
entre processus), de fichier de verrou, de récupération par PID, de balayage
des anciens dossiers ni de nouveau gestionnaire de comptes. Si le module devient
trop volumineux, extraction privée limitée à cette responsabilité ; pas de refonte
générale de l’auth. Toute divergence nécessaire de la SPEC doit être expliquée
avant de remplacer la conception.

## 4. Matrice de tests obligatoires

| Scénario | Assertions attendues |
|---|---|
| Deux tentatives ouvertes | Deux HOME différents ; A intact après démarrage de B ; droits privés ; deux montages chacun. |
| A et B réussissent | Candidats distincts ; publier A puis B donne B actif ; inverser donne A. À chaque étape le cache est complet. |
| A échoue, B attend puis réussit | Ancien cache intact jusqu’à B ; candidat et dossier B intacts ; B devient actif. |
| A réussit, B échoue | L’échec de B ne restaure pas l’ancien cache et ne supprime pas celui publié par A. |
| Cache candidat invalide | Fichier vide, JSON cassé, format incomplet et lien refusés ; cache actif inchangé. |
| Démarrage enfant impossible | Code d’échec ; cache actif intact ; tentative nettoyée ; autre tentative intacte. |
| Nettoyage impossible | Après publication : code 0 et avertissement ; avant publication : échec conservé. Provoquer le cas dans un test unitaire ciblé, sans drapeau de production. |
| SIGINT pendant l’attente | Interrompre le groupe de processus A comme dans un terminal ; B reste utilisable et termine ; aucun cache de A publié. Ne pas confondre avec un signal au seul parent. |
| SIGKILL du parent A | B termine ; C démarre avec un nouveau HOME ; aucun résidu A adopté ou supprimé par B/C. Le harnais termine ensuite l’enfant A survivant, s’il existe. |
| Ancien dossier fixe présent | Sentinelle `.codex-login` intacte avant et après une nouvelle connexion. |
| Compatibilité | Lock existant inchangé à l’octet, argument fixe remplacé uniquement dans la commande de cette invocation ; autres arguments conservés. |
| Commande incompatible | Affectation absente, répétée ou mal placée et exécutable inattendu refusés avant `run` ; message actionnable, code 1. |
| Autres comportements | Auth Claude inchangée, API/ChatGPT reconnus, auth status en lecture seule, aucun cache synthétique dans stdout/stderr/journaux. |

Les tests de concurrence et d’interruption doivent lancer des **processus
distincts**, pas seulement appeler deux helpers à la suite. Les tests unitaires
complètent ces preuves pour la validation, les erreurs et les droits. Répéter
le test de concurrence quelques fois pour vérifier sa détermination ; ne pas
masquer une course par des relances jusqu’au succès.

## 5. Documents à livrer avec le code

Le comportement actuel reste décrit au présent tant que la correction n’est
pas implémentée. Lors du commit de comportement, traiter ensemble :

| Documents | Mise à jour attendue |
|---|---|
| `zevbox_plan_V2.md` §8.1, §8.7 | Dossier unique, commande persistée comme modèle, substitution à l’exécution, publication et résidus. |
| README EN/FR | Reconnexion concurrente : dernière publication réussie active ; un échec ne publie rien ; limite des interruptions brutales. |
| Guides EN/FR, section Codex | Nouveau chemin temporaire, comportement des erreurs et reconnexions ; ne plus recommander le nettoyage du dossier partagé. |
| Architectures EN/FR §7c | Isolation des tentatives, renommage atomique, compatibilité des locks et cycle de vie. |
| `docs/SPEC_codex.md` §3–5 et `docs/PLAN_codex.md` | Remplacer les prescriptions actives de dossier fixe ; conserver les mesures historiques datées comme telles ; lien vers cette correction. |
| `AGENTS.md` | Règle de maintenance de l’auth Codex : ne jamais partager ni balayer les dossiers des tentatives. |
| DECISIONS et PROGRESS | Décision datée, preuve initiale, résultats réels des tests et limites ; C4 inchangée. |
| Cette SPEC et ce plan | Statut livré et résultats, uniquement après validation. |

Chercher dans `README.md`, `docs/`, `zevbox_plan_V2.md`, `AGENTS.md` et `src/` :
`.codex-login`, `previous sign-in`, `unchanged`, `compte actuel`,
`current account`, `cancelled`, `annulation`. Examiner chaque occurrence,
y compris les scripts de l’annexe et les exemples de commandes ; ne pas réécrire
un journal historique comme une preuve du nouveau comportement.

Si un script de requalification non protégé exige l’ancien chemin, l’adapter
et documenter ce changement. Ne toucher à aucun fichier protégé sans montrer le
diff et obtenir un nouvel accord 👤 ; ne jamais resceller de sa propre initiative.
Les guides ne doivent pas promettre que le compte actif reste identique lorsqu’une
autre tentative réussit simultanément.

## 6. Validation et commit

Exécuter les tests ciblés, puis `make check`. Ensuite, sans build, update, setup
ou nettoyage d’images concurrent, lancer séquentiellement `make test-integration`
et `make verify STEP=17`. Inventorier les opérations du runtime avant les gates ;
attendre les travaux étrangers sans les interrompre. Ces gates historiques
complètent les nouveaux tests, elles ne prouvent pas à elles seules la correction.

Vérifier `git diff --check`, les empreintes protégées, les liens documentaires et
la portée du diff. Un seul commit de comportement avec ses tests et documents :
`fix: isolate concurrent Codex sign-in attempts`. Ajouter les fichiers par leur
nom. Aucun changement protégé, nouvelle dépendance, migration de release ou
rebuild d’image ne devrait être nécessaire. Ne pas exécuter `make install`,
`update`, de login personnel ou de push dans cette mission sans demande distincte.

La remise indique : commit, scénario initial reproduit, matrice de tests et gates,
documents mis à jour, absence de modification protégée, limites après SIGKILL.
Ne pas annoncer C4 terminée ni une validation fournisseur avec les caches simulés.

## 7. Message prêt pour l’exécuteur

> Implémente la correction décrite dans `docs/SPEC_codex_auth_concurrency.md`
> en suivant `docs/PLAN_codex_auth_concurrency.md`, sur la branche existante
> `fix/codex-auth-concurrency`. La branche Codex initiale est intégrée à main ;
> les documents de conception sont commités sur cette nouvelle branche.
> Lis d’abord AGENTS et les documents obligatoires. Vérifie la branche et l’état
> de travail, préserve toute modification étrangère. Commence par reproduire la
> collision avec deux processus et des caches synthétiques. Utilise un dossier
> unique par tentative, conserve les anciennes releases sans les modifier, et
> publie le cache complet par renommage atomique : dernière publication réussie
> active. Chaque tentative ne nettoie que son propre dossier. Traite les erreurs
> et interruptions selon la SPEC, sans balayage ni verrou global. Tests,
> documentation EN/FR et comportement doivent être livrés ensemble. Préserve les
> fichiers protégés, les comptes, les images, les autres agents et les invariants.
> Aucun sous-agent, déploiement, fusion ou push. Termine les gates séquentielles
> et rapporte les preuves ; cette mission ne clôt pas C4.

## 8. Résultats

2026-11-06, branche existante à `5980fdb`, état initial propre. Référence
hors sandbox : `make check`, 343 tests réussis, 8 ignorés, invariants OK.
Reproduction avant correction : `two_attempts_and_both_publication_orders`
échoue, le candidat A est remplacé par B au même chemin pendant l’attente de A.
Preuve locale : `/tmp/zevbox-auth-reproduction.log`.

La matrice §4 est couverte par le nouveau fichier non protégé
`tests/codex_auth_concurrency_test.rs` et les tests unitaires d’auth :

| Preuve | Couverture |
|---|---|
| `two_attempts_and_both_publication_orders` | Deux processus ouverts, droits privés, deux montages, candidat A intact, publications A/B et B/A, cache complet à chaque étape, lock inchangé, adaptateur conservé, ancien dossier fixe intact, nettoyage limité. |
| `failures_do_not_publish_or_damage_other_attempts` | Échec puis succès, succès puis échec ; cache vide, JSON cassé, incomplet ou lien refusés. |
| `spawn_failure_cleans_only_its_attempt` | Exécutable disparu après prepare : erreur, cache actif et B intacts, seul dossier B restant. |
| `signals_isolate_attempts_and_leave_residues_unadopted` | SIGINT du groupe A et SIGKILL du seul parent A, B puis C réussissent ; nouveau HOME C, résidu A préservé, enfant survivant terminé par le harnais. |
| `incompatible_commands_are_rejected_before_run` | Affectation absente, répétée ou mal placée, exécutable inattendu : code 1 actionnable, aucun run. |
| `cleanup_errors_preserve_publication_and_primary_errors` | Suppression réellement empêchée par parent non inscriptible ; avertissement, succès préservé, échec et diagnostic principal conservés. |
| `failed_publication_preserves_active_destination_and_candidate` | Renommage impossible et préparation des droits impossible : destination et candidat conservés. |
| Tests Codex et suite existante | API/ChatGPT, statut en lecture seule, non-régression Claude ; aucune donnée synthétique dans stdout/stderr des connexions terminées. |

Tests ciblés : cinq tests multiprocessus et huit tests Codex réussis ; nouveaux
tests unitaires réussis. Trois répétitions supplémentaires des cinq tests
multiprocessus : toutes vertes. `make check` hors sandbox : **350 réussis,
8 ignorés**, invariants OK. Les essais restreints échouent uniquement sur les
tests socket/watcher existants ; relance hors restriction réussie, aucun
contournement. Puis, après inventaire du runtime sans build/update/setup
concurrent : `make test-integration` **8/8**, puis `make verify STEP=17`
**STEP 17: OK**, séquentiellement (la gate rejoue aussi les huit tests réels).
Logs : `/tmp/zevbox-auth-{targeted,check-unrestricted,integration,verify17}.log`.

Revue documentaire EN/FR et recherche §5 effectuées : occurrences restantes
du dossier fixe = modèle de release, compatibilité ou historique daté.
Aucun script de l’annexe n’exige ce chemin. Empreintes protégées et
`git diff --check` conformes ; liens locaux modifiés vérifiés.
Aucun rebuild de release, changement de lock installé, compte personnel,
quota, déploiement, fusion ou push. Les gates emploient leurs images de test
existantes. C4 reste ouverte ; rafraîchissements de sessions concurrentes et
effets distants fournisseur hors portée. SIGKILL peut laisser un dossier privé
et un enfant provisoire ; aucune collecte automatique ni garantie de nettoyage
après toute interruption.


## 9. Déploiement ultérieur et retour humain

Après le commit `0d91211`, l’utilisateur a explicitement demandé le
déploiement : `make install` réussi, dont `make check` (350 réussis,
8 ignorés, invariants OK) et compilation release. `cmp` confirme l’identité
entre `target/release/zevbox` et `~/.local/bin/zevbox` ; version `0.2.1`.
Log local : `/tmp/zevbox-auth-install.log`. Aucun update ni rebuild d’image.

L’utilisateur confirme ensuite « c’est bon de mon côté » dans le contexte de
son essai Zed. Aucun détail supplémentaire sur deux connexions fournisseur
simultanées, les permissions ou les sessions n’est déduit. C4 reste ouverte.
La fusion envisagée a été suspendue à sa demande ; aucune fusion ni push
effectué lors de cette mise à jour documentaire.
