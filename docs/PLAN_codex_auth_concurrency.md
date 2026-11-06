# Plan de correction des connexions Codex simultanées

Statut au 2026-11-06 : **prêt pour une mission d’implémentation ultérieure**.
La demande actuelle porte uniquement sur la conception et ce plan ; aucun code,
test ou fichier protégé n’est modifié. Contrat de cette évolution :
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
