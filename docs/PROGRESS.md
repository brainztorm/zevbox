# Avancement Zevbox

Mettre à jour **après** chaque étape : statut, preuve (commande + résultat
résumé), date. Statuts : ⬜ à faire · 🟨 en cours · ✅ terminé (gate OK) ·
⛔ bloqué (étape 14 seulement, raison obligatoire) · 👤 attend l'humain.

`scripts/verify-step.sh N` refuse de valider l'étape N tant que les étapes
précédentes ne sont pas ✅ ici.

| Étape | Titre | Statut | Preuve / notes |
|---|---|---|---|
| 0 | Chaîne d'outils + git | ✅ | Rust 1.99.0 installé via Homebrew `rustup` (2026-10-13), binaires dans `/opt/homebrew/opt/rustup/bin`. `make verify STEP=0` OK, `git init` et commit initial « harness » effectués (2026-10-13). |
| 1 | Inventaire (👤 chemin du projet) | ✅ | Revérifié (2026-10-13) : `container` 1.5.0, système `running`, 3 conteneurs existants intacts (`buildkit`, `kiac-*`). Zed 1.23.3, premier projet autorisé : `~/code/zevbox`. `make verify STEP=1` OK. |
| 2 | Squelette compilable | ✅ | Fourni avec le plan : `cargo clippy --all-targets -- -D warnings` OK, `zevbox --help` liste les 15 commandes (2026-10-13). |
| 3 | config, fsutil, tools | ✅ | **Exemple fourni** (2026-10-13) : `cargo test --test config_test` 22/22, clippy -D warnings OK, invariants OK. Fichiers marqués `WORKED EXAMPLE` : s'en inspirer pour le style. |
| 4 | Politique du relais | ✅ | **Exemple fourni** (2026-10-13) : `relay_policy_test` 44/44, clippy OK, invariants OK. |
| 5 | Transport du relais | ✅ | `run_relay` implémenté dans `src/relay/mod.rs` (boucle `tokio::select!`, FramedRead LinesCodec, sécurité I9/I11). `make verify STEP=5` OK : `relay_transport_test` 18/18, clippy OK, invariants OK (2026-10-13). |
| 6 | container.rs | ✅ | **Exemple fourni** (2026-10-13) : `container_test` 12/12, clippy OK, invariants OK. |
| 7 | Releases (parsing, active.json) | ✅ | Parsing manifeste ACP (Claude / Antigravity), active.json, promote, rollback implémentés dans `src/release.rs`. `make verify STEP=7` OK : `release_test` 10/10, clippy OK, invariants OK (2026-10-13). |
| 8 | Réglages Zed (JSONC) | ✅ | **Exemple fourni** (2026-10-13) : `zed_settings_test` 23/23 ; essai sur une copie du vrai `~/.config/zed/settings.json` : 0 ligne supprimée, désinstallation = original à l'octet. |
| 9 | launch, status, stop, clean, init, project, zed | ✅ | Implémenté : `instances.rs`, `supervisor.rs`, CLI `init`, `project`, `zed`, `status`, `stop`, `clean`, `launch`. `make verify STEP=9` OK : `lifecycle_test` 22/22, `cli_test` 8/8, 12 binaires de test OK (2026-10-13). |
| 10 | Tests réels Apple Container | ✅ | Validé sur la vraie VM Apple Container : 8/8 tests réussis dans `tests/integration_test.rs` (isolation virtiofs, uid hôte, bounded shutdown, SIGKILL, non-propagation symlink). `make verify STEP=10` OK (2026-10-13). |
| 11 | update / build / smoke test / rollback | ✅ | Pipeline update / build / smoke test / rollback implémenté dans `src/release.rs` et `src/cli/update.rs`. Digest baseImage `node:22` consigné dans `DECISIONS.md`. Preuves réelles : `zevbox update --check` liste Claude (0.89.1) et Antigravity (1.3.0) ; `zevbox update --agent claude` construit l'image `zevbox/claude:claude-20261013-073156`, passe le smoke test (`/etc/zevbox-release.json`, `uname -s` Linux, dpkg) et promeut ; rollback testé avec 2 releases successives (permute current/previous). `make verify STEP=11` OK (2026-10-13). |
| 12 | tools | ✅ | CLI `zevbox tools list/add/remove` implémenté dans `src/cli/tools.rs`. Preuves réelles : `zevbox tools add ripgrep` ajoute `ripgrep` à `tools.json`, reconstruit l'image `zevbox/claude:claude-20261013-073710` et la promeut ; test réel `container run --rm zevbox/claude:claude-20261013-073710 rg --version` affiche `ripgrep 13.0.0` ; aucun nouveau montage hôte. `make verify STEP=12` OK (2026-10-13). |
| 13 | 👤 Authentification Claude | ✅ | `zevbox auth claude` implémenté dans `src/cli/auth.rs` (vecteur interactif -it, montage state_dir /home/agent). `zevbox auth claude --status` testé et rapporte `not authenticated` en l'absence de `.credentials.json`. `make verify STEP=13` OK (2026-10-13). |
| 14 | 👤 Antigravity | ✅ | Antigravity 1.3.0 fonctionnel dans Apple Container (`zevbox/antigravity:antigravity-20261013-074439`). Authentification Google OAuth personnelle intégrée via flux PKCE sur l'hôte dans `zevbox auth antigravity` (serveur HTTP éphémère 127.0.0.1:0 sur l'hôte, zéro port exposé dans la micro-VM, respect strict de l'invariant I1). Écriture atomique sécurisée 0600 de `acp_token.json` et `settings.json` dans `state/antigravity/.gemini/antigravity-acp/`. Entrée `zevbox · Antigravity · <projet>` enregistrée dans Zed. `make verify STEP=14` OK (2026-10-14). |
| 15 | doctor, setup, shell, logs, auth --status | ✅ | Implémenté : `src/cli/{doctor, setup, shell, logs}.rs`. Preuves : `doctor` rapporte 7/7 contrôles [OK] (config, container 1.5.0 arm64, runtime running, architecture, projet canonique, releases, permissions 0700) ; `setup` idempotent ; `shell` affiche la bannière exacte ; `logs` implémente `--path` et les 200 dernières lignes. `make verify STEP=15` OK (2026-10-13). |
| 16 | 👤 Intégration Zed réelle | ✅ | `make install` compile en release et installe dans `~/.local/bin/zevbox`. `zevbox zed install` injecte les entrées `zevbox · Claude · zevbox` et `zevbox · Antigravity · zevbox` dans `~/.config/zed/settings.json` avec sauvegarde automatique dans `backups/`. Désinstallation vérifiée rétablissant le fichier à l'identique. Preuve de rejet pour projets non autorisés validée. `make verify STEP=16` OK (2026-10-13). |
| 17 | README, AGENTS.md, acceptation | ✅ | `README.md` complet rédigé en français (installation en 3 commandes, usage quotidien, référence des 15 commandes CLI, persistance §11, invariants §3, dépannage). `AGENTS.md` présent et concis. Zéro stub restant dans `src/`. Tous les tests et invariants passent. `make verify STEP=17` OK (2026-10-13). |

## Extension Codex

Conception retenue par 👤 le 2026-11-06 : même expérience et mêmes fonctions de
gestion que Claude/Antigravity. [SPEC_codex.md](SPEC_codex.md) fixe le résultat ;
[PLAN_codex.md](PLAN_codex.md) décrit la reprise par un agent exécutant.
Les étapes ci-dessous sont distinctes des gates historiques 0–17.

| Étape | Titre | Statut | Preuve / notes |
|---|---|---|---|
| C0 | Vérifier que les versions n'ont pas bougé | ✅ | 2026-11-06 : registre stable `2.5.0 @agentclientprotocol/codex-acp@2.5.0`, Zed `1.25.3`, `container` `1.5.0`, identiques à la qualification. `make check` hors restriction vert : 324 tests réussis, 8 ignorés, `invariants: OK` ; empreintes protégées conformes. |
| C1 | Image et composants de release sans activation publique | ✅ | 2026-11-06 : correctif appliqué à l’identique ; `make check` vert, 329 tests réussis, 8 ignorés. Image jetable : `build rc=0`, `verify_image: ALL OK` (18 PASS), adaptateur 2.5.0, moteur 0.165.0, Node 24.19.0, ACP v1 ; image, builder et dossier nettoyés. |
| C2a | Le relais refuse les versions ACP qu'il ne filtre pas | ✅ | 2026-11-06 : correctif appliqué à l’identique ; `make check` vert, 332 tests réussis, 8 ignorés, invariants OK ; empreintes protégées conformes. Refus des versions non filtrées en requête et réponse, vérifié par tests simulés ; aucun nouvel essai Zed. |
| C2 | Activation standard, auth native et diagnostics | ✅ | 2026-11-06 : correctif appliqué à l’identique, documents EN/FR préparés. Fmt/clippy OK ; exactement les deux échecs protégés attendus. Preuve réelle : promotion, `verify_image` 18 PASS, `verify_launch` 13 PASS et ALL OK, EOF 2,3 s sans VM restante, doctor/permissions et entrée Codex OK ; état isolé nettoyé. Diff §5.6 approuvé explicitement par 👤 et appliqué ; rescellement humain confirmé et vérifié, `make check` vert (343 tests réussis, 8 ignorés), invariants OK. Auth réelle à faire en C4. |
| C3 | Console et commandes de gestion | ✅ | 2026-11-06 : correctif appliqué à l’identique ; `make check` vert, 343 tests réussis, 8 ignorés, invariants OK. 24 actions, Codex avant oMLX, textes EN/FR et guides alignés. Rendu en terminal simulé ; essai interactif humain réservé à C4. |
| C4 | Qualification réelle, acceptation humaine et revue documentaire | 👤 | 2026-11-06 : gates séquentielles réussies, image/relais et cycle de vie Codex vérifiés sans modèle ; déploiement local ciblé effectué. C0–C3 conservées. Setup global non validé (expiration Antigravity préexistante). Connexion et fonctionnement dans Zed confirmés par l’utilisateur ; auth --status = authenticated. Scénarios détaillés et quota des essais supplémentaires encore en attente. |

## Journal

- 2026-11-06 — **Correction des connexions Codex simultanées validée** :
  départ sur `fix/codex-auth-concurrency` à `5980fdb`, état propre, aucun
  changement étranger. Référence `make check` hors restriction : 343 réussis,
  8 ignorés. Avant correction, deux processus réels : A écrit et attend, B
  démarre et supprime/remplace le candidat de A dans le HOME partagé ; assertion
  d’intégrité A échouée (`/tmp/zevbox-auth-reproduction.log`).
  Dossiers privés uniques, candidat explicite et renommage atomique après
  réussite/validation implémentés sans migration ni rebuild de release.
  Matrice complète et correspondance des tests dans
  [PLAN concurrence §8](PLAN_codex_auth_concurrency.md#8-résultats) :
  deux ordres de publication, échecs, caches invalides/liens, démarrage impossible,
  erreurs de publication/droits, nettoyage réellement empêché, SIGINT du groupe,
  SIGKILL du parent, résidu non adopté et ancien dossier fixe intact.
  Tests ciblés 5 + 8 et nouveaux unitaires OK ; trois répétitions supplémentaires
  de concurrence toutes vertes. `make check` restreint : échecs existants
  socket/watcher interdits ; hors restriction **350 réussis, 8 ignorés**,
  fmt/clippy/invariants OK (`/tmp/zevbox-auth-check-unrestricted.log`).
  Inventaire runtime effectué, aucun build/update/setup étranger en cours.
  Ensuite séquentiellement : `make test-integration` **8/8**
  (`/tmp/zevbox-auth-integration.log`), `make verify STEP=17`
  **STEP 17: OK**, dont intégration rejouée 8/8
  (`/tmp/zevbox-auth-verify17.log`). Empreintes protégées conformes, aucun
  fichier protégé modifié, `git diff --check` et liens locaux conformes.
  Documentation revue/actualisée : AGENTS, contrat V2, README/guides/architectures
  EN/FR, SPEC/PLAN Codex, SPEC/PLAN concurrence, DECISIONS et présent journal.
  Annexe examinée : aucun script dépendant du dossier fixe ; occurrences
  restantes = modèle compatible ou historique explicitement daté.
  Aucun compte, quota, sous-agent, déploiement, fusion, push ni changement de main.
  Limites : SIGKILL peut laisser dossier privé/enfant provisoire ; pas de balayage.
  Rafraîchissements des sessions et effets distants non couverts. **C4 reste ouverte.**


- 2026-11-06 — **Transmission sur branche dédiée** : après intégration de la
  branche Codex initiale, `main` local et `feat/codex-acp` pointent sur `87db544`.
  Branche `fix/codex-auth-concurrency` créée depuis `main`, avec les documents
  préparés et leurs renvois ; plan et prompt de l’exécuteur actualisés.
  Aucun changement de code ni déploiement ; aucun push ou fetch.
  Avant commit documentaire : `make check` hors sandbox réussi (343 tests,
  8 ignorés, invariants OK), empreintes protégées, liens locaux des nouveaux
  documents et `git diff --check` conformes. Ces contrôles portent sur le code
  existant, pas sur la correction encore à implémenter.

- 2026-11-06 — **Plan de correction de l’auth Codex concurrente** : demande
  limitée à la conception et aux instructions pour un exécuteur. Création de
  [SPEC_codex_auth_concurrency.md](SPEC_codex_auth_concurrency.md) et
  [PLAN_codex_auth_concurrency.md](PLAN_codex_auth_concurrency.md) : dossier unique
  par tentative, compatibilité des locks, publication atomique, erreurs et
  interruptions, matrice de tests multiprocessus sans compte et revue EN/FR.
  Aucun code, test, fichier protégé ou état utilisateur modifié. La course reste
  à reproduire et corriger ; C4 reste ouverte. Pas de gate d’implémentation
  revendiquée pour ce travail documentaire.

- 2026-11-06 — **Confirmation utilisateur et contrôle de branche** : après les
  refus attendus du dépôt Zevbox lui-même (I12), puis du cache Codex absent,
  l’utilisateur confirme « C’est bon ça marche » dans le contexte de son essai
  Zed. Contrôle local : `zevbox auth codex --status` = `authenticated`.
  Connexion et fonctionnement de base confirmés ; aucune déduction sur les
  scénarios détaillés de permissions, annulation, sélecteurs ou sessions.
  Documents actifs EN/FR actualisés ; les entrées précédentes sont historiques.
  `main` local = `0d02506`, identique au merge-base avec HEAD ; `HEAD..main`
  vide, donc tous ses commits sont déjà inclus. Simulation
  `git merge-tree --write-tree HEAD main` code 0 sans conflit ; aucune fusion,
  modification de main, récupération distante ou publication effectuée.

- 2026-11-06 — **C4, nettoyage et revue finale accessible** : inventaire avant
  suppression et preuves de releases/builds archivés hors des états dans
  `/tmp/zevbox-c4-evidence/` (`resource-inventory.json`, sous-dossiers portant le
  nom des états, snapshots globaux après nettoyage). Suppression ciblée des
  images Claude/Codex du précédent setup incomplet et des deux images Codex
  finales du nouvel état ; états `P1bCDf` et `1Nv4S1` supprimés. Les images
  antérieures du nouvel essai avaient été rangées par update ; le candidat apt
  échoué est archivé avec son build.log. Aucune image réelle ni base partagée
  supprimée manuellement. Contrôle après nettoyage : tous les current/previous
  réels et leurs images conservés, empreintes des comptes/réglages inchangées,
  aucun conteneur préexistant hors builder modifié. Aucun nettoyage réel.
  Documents examinés : AGENTS, contrat V2, README/guides/architectures EN/FR,
  SPEC console, SPEC/PLAN Codex, PROGRESS/DECISIONS, annexe README, CLAUDE et
  exploration Gemini. Modifiés dans cette reprise : tous les documents actifs
  précités sauf CLAUDE (importe AGENTS) et exploration (historique OAuth) ; annexe
  README corrigée au commit de procédure. Liens locaux vérifiés ; recherches
  anciennes règles relues, seuls contexte historique ou faits spécifiques
  restent. 4 agents, 3 standards, 24 actions, 15 commandes, 13 invariants.
  Ancien paragraphe api-key mal placé et titre cassé de l’architecture FR
  corrigés sans changement de politique. Paires utilisateur EN/FR alignées.
  La revue des documents est faite ; l’acceptation complète C4 reste 👤.
  Contrôle final `/tmp/zevbox-c4-handoff-check.log` : code 0, 343 tests,
  invariants OK ; diff et empreintes conformes, aucun fichier protégé modifié.

- 2026-11-06 — **C4, gestion Codex réelle réussie** : état neuf limité à Codex
  `/private/tmp/zevbox-iso.1Nv4S1` ; update initial code 0, promotion
  `codex-20261106-151727`, update suivant code 0 `codex-20261106-152101`,
  current/previous et rollback aller-retour contrôlés par assertions.
  `tools add zevbox-no-such-package` code 1, active.json inchangé, build.log
  contient l’erreur apt attendue (candidat `codex-20261106-152227`) ; retrait
  code 0, liste apt vide. `tools add sl` code 0, `dpkg -s sl` confirme installé ;
  retrait code 0 et reconstruction réussie. `/tmp/zevbox-c4-management.log` :
  `management ALL OK`, commandes détaillées dans les logs `-update2`,
  `-rollback1`, `-rollback2`, `-tools-fail`, `-tools-recover`, `-tools-sl`,
  `-tools-sl-remove`. Aucun build concurrent ; autres releases conservées
  prouvées séparément lors du déploiement réel ciblé. Il s’agit d’une installation
  neuve Codex seule, pas d’un setup neuf global réussi.
  Confirmation console EN/FR également vérifiée avec cache synthétique puis
  refusée (aucune reconnexion déclenchée) ; contrôle avec compte réel en attente.

- 2026-11-06 — **C4, qualifications réelles sans modèle et déploiement ciblé** :
  `verify_image.sh` : 18 PASS, ALL OK ; `verify_launch.py` : 13 PASS, ALL OK
  (`/tmp/zevbox-c4-verify-image.log`, `-verify-launch.log`). Adaptateur 2.5.0,
  moteur 0.165.0, Node 24.19.0, Zed 1.25.3, container 1.5.0, ACP v1.
  EOF 2,3 s ; essais supplémentaires SIGTERM 2,3 s, SIGKILL puis clean 2,3 s,
  sortie agent imposée par arrêt de sa VM 0,2 s, sans VM/enregistrement restant
  (`/tmp/zevbox-c4-lifecycle.log`). Cette sortie imposée ne prouve pas chaque
  type de crash interne du moteur. Login natif atteint puis Ctrl-C : code 1,
  diagnostic attendu, cache synthétique précédent inchangé (`-cancel.log`).
  Aucun code d’appareil ni secret conservé. Auth persistante d’un vrai compte
  non encore prouvée ; cache synthétique relu entre VM dans verify_launch.
- 2026-11-06 — **C4, installation locale** : `make install` code 0
  (`/tmp/zevbox-c4-install.log`), puis `zevbox update --agent codex` code 0,
  promotion `codex-20261106-151920` (`-real-update.log`). Aucun setup global.
  Autres entrées current/previous inchangées ; empreintes des caches Claude,
  Antigravity et réglages oMLX inchangées. Sauvegarde Zed antérieure exacte
  retrouvée ; aucune ligne préexistante supprimée, une seule entrée Codex,
  binaire `~/.local/bin/zevbox`, deux `zed install` successifs idempotents.
  `doctor --json` code 0 : releases existantes authentifiées, permissions et
  entrées Zed OK ; Codex attend sa connexion ; WARN instances anciennes laissé
  intact (aucun nettoyage de l’installation réelle). Le login est lancé via
  `/tmp/zevbox-c4-login.command` dans Terminal, sans capture de son affichage ;
  seul le geste navigateur est demandé à l’humain. Résultat dans
  `/tmp/zevbox-c4-login-result` (code de sortie uniquement).
- 2026-11-06 — **C4, mécanismes partagés et console** : images réelles Claude,
  Antigravity et oMLX répondent initialize ACP v1 avec états vides jetables,
  sans compte ni prompt (`/tmp/zevbox-c4-regression.log`). Doctor confirme les
  comptes existants, serveur oMLX joignable de sa VM (HTTP 401 sans clé).
  Cela ne valide pas un dialogue modèle des autres agents. Console réelle
  EN/FR en pseudo-terminal 130×30 : navigation vers connexion, feuille Codex,
  rendu et sortie code 0 (`-console-en.txt`, `-console-fr.txt`). Pas de login
  réel depuis la console ni de confirmation d’un compte connecté encore prouvés.
- 2026-11-06 — **C4, réserves obligatoires** : aucun quota payant consommé.
  Restent connexion personnelle, persistance et annulation après vrai login,
  usage réel Zed (permissions acceptée/refusée, annulation puis nouveau prompt,
  modèles/modes/effort, pièces jointes, questions, réseau, reprise et threads),
  cycle de vie déclenché par Zed, console avec compte réel, non-régression de
  dialogue des autres agents. Ces essais nécessitent le compte et/ou du quota
  autorisé ; le présent environnement n’offre pas d’outil de pilotage visuel Zed.
  Setup neuf/global/existant et idempotence globale restent non validés, cause
  extérieure Antigravity à 120 s documentée ; aucune reconstruction globale
  répétée. C4 et backlog 15 restent ouverts, sans promesse de parité complète.

- 2026-11-06 — **Reprise C4, contrôles séquentiels** : branche propre `feat/codex-acp`
  à `d5b7c44`, empreintes conformes. Aucun build/update/setup concurrent détecté
  par `ps`. `make check` restreint échoue sur socket et watcher (restrictions
  connues) ; relance hors restriction code 0, 343 tests et invariants OK
  (`/tmp/zevbox-c4-check-unrestricted.log`). Puis `make test-integration` code 0,
  8/8 (`/tmp/zevbox-c4-integration-sequential.log`), puis `make verify STEP=17`
  code 0, `STEP 17: OK` (`/tmp/zevbox-c4-verify17.log`). Aucun build ou nettoyage
  concurrent pendant ces deux commandes. Cela ne démontre pas la cause de la
  variation historique de buildkit. Setup global non relancé : neuf non validé,
  expiration Antigravity préexistante hors périmètre, résumé seul insuffisant.
  Instructions de reprise actualisées dans PLAN et annexe ; aucun test modifié.

- 2026-11-06 — **Contrôle du commit documentaire C4** : `make check` vert (343 tests réussis, 8 tests réels ignorés), `git diff --check`, cibles locales des liens et empreintes protégées conformes. Ce succès simulé n’annule pas l’échec réel consigné ci-dessous ; `make verify STEP=17` n’a pas été lancé après cet écart.

- 2026-11-06 — **C4 suspendu, écarts à soumettre à l’humain (PLAN §1.5)**. Deux commandes lancées en parallèle par erreur d’ordonnancement de l’exécutant : `make test-integration` et `( . "$W/env.sh"; "$W/zevbox-bin" setup )`, avec `W=/private/tmp/zevbox-iso.P1bCDf` créé par `isolated_env.sh target/debug/zevbox`. Tests réels : 7 réussis, 1 échec, `real_sigkill_of_zevbox_still_removes_container` à `tests/integration_test.rs:326`, `pre-existing containers changed` : seul `buildkit` passe d’absent à présent/running ; les autres conteneurs restent identiques. Le test avait déjà constaté la disparition de la VM SIGKILL et le nettoyage de son enregistrement. Une construction concurrente peut modifier le builder global ; c’est une hypothèse de contamination du témoin, pas une cause prouvée. Aucune relance ni modification du test pour masquer l’échec. Log : `/tmp/zevbox-codex-c4-integration.log`.
- 2026-11-06 — **C4 setup neuf, résultat incomplet** : code 1. Claude 0.101.0 promu `claude-20261106-150025`, Codex 2.5.0 promu `codex-20261106-150349` après smoke tests. Antigravity 1.4.5 échoue au téléchargement : `curl: (28) Operation timed out after 120004 milliseconds with 82172867 out of 307829008 bytes received`. Sortie finale : `zevbox setup complete` puis `zevbox: setup incomplete: an agent has no release. Fix: run 'zevbox update' (network needed) and read the errors above.`, code 1. Auth passée avec `skipping authentication (not a terminal)`, permissions OK, entrées Zed isolées synchronisées. État et preuves conservés sous `/private/tmp/zevbox-iso.P1bCDf` (`setup.log`, `state/releases/`), images Claude/Codex de cet essai conservées ; ni compte ni quota ni vrais réglages Zed touchés.
- 2026-11-06 — **Ancienne proposition de reprise C4 (remplacée par la mission autorisée ci-dessus)** : après arrêt de toute construction, relancer `make test-integration` seul ; si vert, `make verify STEP=17` seul. Puis reprendre `setup` sur l’état isolé conservé, sans aucun test d’intégration concurrent ; au nouvel échec, s’arrêter selon §1.5. Restent non validés : installation existante et idempotence, échec apt volontaire et récupération, outil `sl`, deuxième release/rollback et ciblage, requalification finale des scripts, puis toutes les cases humaines §7.2–7.3. Ne pas annoncer la parité complète. Pour le déploiement humain seulement après ces contrôles : `make install`, puis `zevbox update --agent codex` (Codex seul), puis `zevbox auth codex` dans son terminal ; ne jamais consigner le code ou les jetons. `zevbox setup` reconstruit aussi les autres agents standards et oMLX installé si une release standard manque.
- 2026-11-06 — **Revue §8.5 au point de suspension** : inventaire actif examiné : AGENTS, contrat V2, README EN/FR, guides EN/FR, architectures EN/FR, SPEC console, SPEC/PLAN Codex, DECISIONS, PROGRESS et annexe README ; `CLAUDE.md` importe AGENTS et reste inchangé. Tous ces documents actifs ont été modifiés pendant C0–C4, sauf `CLAUDE.md`. `exploration_solutions_gemini.md` examiné et inchangé car historique OAuth ; V1 et SPEC_tools_rebuild restent historiques. Dernières précisions : console interactive Codex et test du bandeau à 95 colonnes, exemple shell hors sources Zevbox, registre Codex dans le pipeline du contrat. Aide auth/update conforme aux quatre agents ; 15 commandes, 24 actions, 4 agents dont 3 standards, 13 invariants. Cibles des liens locaux de 16 documents vérifiées ; recherche §8.3 relue et anciennes règles limitées aux décisions/recettes historiques. Paires EN/FR alignées sur les preuves et réserves. La revue finale ne peut être cochée : acceptation réelle et gates C4 incomplètes. Le seul test protégé changé est le diff Zed autorisé ; sa seule ligne de manifeste a été rescellée par l’humain, empreintes conformes.

- 2026-11-06 — **Retrait des correctifs appliqués** : C1, C2a, C2, C3 et diff protégé supprimés selon PLAN §6 ; quatre scripts de qualification conservés. AGENTS, en-tête V2, PLAN §1.3, annexe README et backlog DECISIONS actualisés. Les mentions dans les journaux datés et recettes du plan restent l’historique ; ne plus tenter d’appliquer ces fichiers à la reprise C4.

- 2026-11-06 — **C3 validé** : correctif console appliqué sans réécriture, `make check` vert (343 tests, 8 ignorés). Documents mis à jour : README et guides EN/FR, SPEC console, contrat, AGENTS, SPEC/PLAN Codex et DECISIONS ; recherche §8.3 relue, mentions spécifiques et historiques conservées. Diff et empreintes conformes. La revue automatique du commit C2 a initialement classé `r3_mounts_test.rs` comme protégé ; vérification de son absence du manifeste et de sa prescription explicite dans PLAN §5.2, puis nouvelle revue acceptée sans contournement.

- 2026-11-06 — **C2 validé** : rescellement humain confirmé (« manifest OK confirmé »), seule ligne Zed modifiée dans le manifeste et empreintes conformes. `make check` final vert : 343 tests réussis, 8 ignorés, invariants OK. Diff protégé strictement conforme à l’accord ; documentation et statut alignés. Authentification réelle et essais Zed restent à faire en C4.

- 2026-11-06 — **Accord protégé C2 reçu** : réponse 👤 « oui » à la demande du diff §5.6. `protected_zed_settings.diff` appliqué après `git apply --check` ; diff obtenu strictement limité aux deux attentes montrées. Manifeste non modifié : PLAN §5.6 réserve son rescellement à l’humain. C2 reste 👤 jusqu’au rescellement, puis contrôle final et commit.

- 2026-11-06 — **C2, remise intermédiaire avant accord protégé** : commandes PLAN §5.3 exécutées hors restriction : fmt sans sortie, clippy Finished sans avertissement, `cargo test --no-fail-fast` affiche seulement `every_entry_name_has_managed_prefix` et `entries_for_builds_names_and_args` en FAILED (ordre concurrent inversé, mêmes deux tests). Bloc §5.4 exécuté en installation jetable : adaptateur 2.5.0, moteur 0.165.0, Node 24.19.0, ACP v1 ; `verify_image: ALL OK`, `verify_launch: ALL OK`. Le script livré contient 13 contrôles, tous PASS (la répétition annonçait 14). `doctor` signale Codex non connecté et permissions OK ; avertissements supplémentaires attendus de cet essai : binaire installé absent (copie jetable), méthodes `authenticate` et `initialize` volontairement refusées. Zed isolé : `['zevbox · Codex']`. Nettoyage terminé, aucun vrai compte ou réglage Zed touché.
- 2026-11-06 — **Revue documentaire intermédiaire §8** : examinés et modifiés pour C2 : contrat V2, AGENTS, README EN/FR, guides EN/FR (4d, installation, listes, diagnostic, persistance et limites), architectures EN/FR (7c, image et auth), SPEC console, SPEC/PLAN Codex, DECISIONS et ce suivi. Recherche des anciennes listes exécutée dans tous les documents du §8.3 ; les mentions propres à Claude/Antigravity/oMLX restent spécifiques. `docs/exploration_solutions_gemini.md` examiné et inchangé : journal historique OAuth Antigravity, sans liste active à étendre. V1 et SPEC_tools_rebuild restent historiques. Compteurs : 4 agents dont 3 standards, 15 commandes, 23 actions avant C3, 13 invariants. Revue finale §8.5 encore en attente de C3/C4, notamment essais réels, questionnaires et acceptation humaine. Aucun fichier protégé modifié.
- 2026-11-06 — **Action humaine C2 (§5.6)** : approuver explicitement le diff `docs/codex_annex/protected_zed_settings.diff` (ajout de Codex et compteur 3 → 4). Après application autorisée, exécuter le rescellement de la seule ligne `tests/zed_settings_test.rs` avec la commande de PLAN §5.6, puis confirmer `manifest OK`. L’exécutant relance ensuite `make check` avant commit ; ne pas utiliser `make protect-update`. C3 et C4 ne sont pas commencées.

- 2026-11-06 — **C0 exécuté** : arbre initial propre ; lectures intégrales requises effectuées. Le contrôle de base restreint échoue sur la socket locale (`Operation not permitted`) et la surveillance de fichiers ; même `make check` hors restriction réussi sans modification. Les commandes de PLAN §3 échouent sous restriction (DNS et lecture de version Zed), puis donnent hors restriction les trois versions attendues. Aucune preuve de connexion, de modèle ou de Zed interactif ; les 8 tests réels restent ignorés. SPEC et DECISIONS inchangées : aucune version nouvelle.

- 2026-11-06 — **Plan Codex éprouvé par sonde VM et répétition** (demande 👤 :
  « que l'agent exécuteur moins intelligent puisse mener les choses sans
  problème ») : mesures en vraies VM Apple Container avec les options de
  `launch`, sans compte ni clé réelle ; code de C1 à C3 écrit et validé dans des
  copies jetables. Trouvé et corrigé dans le plan : sandbox de Codex inopérant avec
  le `bwrap` d'Ubuntu, login révoquant l'ancien compte avant réussite, `api-key`
  impossible depuis Zed, aucun contrôle de `protocolVersion`, `state/codex` manquant
  sur installation existante, tests `update` impossibles avec le faux `container`,
  deux tests non protégés oubliés, faux positif de délai. Décision 👤 sur le
  bandeau consignée. Preuves : `make check` vert à chaque étape (329, 332, 343,
  343 tests), pipeline réel en 1 min 32, `verify_image` 18 PASS, `verify_launch`
  14 PASS, rollback aller-retour, nettoyage complet des images d'essai. PLAN réécrit
  pour un exécutant à la lettre ; SPEC, DECISIONS, AGENTS et contrat alignés ;
  annexe `docs/codex_annex/` ajoutée. Dépôt : documents seulement, aucun code ni
  test modifié, empreintes protégées conformes. Formulation « `update` dès l'état
  vierge » corrigée (`update` exige l'état initialisé).
- 2026-11-06 — **Conception Codex révisée et documentation de transmission**
  (demande 👤) : SPEC/PLAN, AGENTS, contrat et DECISIONS alignés sur une auth
  native fiable sans exclusivité ChatGPT, sans vidage systématique
  d'`authMethods` ni refus général d'`authenticate`. Dernière stable à qualifier avec le Zed réel,
  pas de gel permanent d'ACP en v1. C1 prépare les composants ; C2 active
  ensemble l'agent standard et son auth pour éviter le chemin Claude.
  Effet global du `setup` de migration explicité ; tests prévus pour caches
  natifs, reconnexion, journaux en échec et dialogue sans questionnaires.
  PLAN §8 impose les mises à jour documentaires de tout le projet par
  l'exécutant dans chaque commit, puis une revue finale EN/FR. Constats
  statiques sourcés dans DECISIONS ; C0–C4 restent non validées. Aucun code,
  test ni fichier protégé modifié pendant cette révision documentaire.
  Vérification : `git diff --check` OK, liens/ancres locaux et blocs Markdown
  des six documents contrôlés, diff protégé proposé vérifié par
  `git apply --check` sans application, 29 empreintes protégées conformes.
  Comparaison à l'état de travail initial : seuls ces six documents ont changé,
  modifications préexistantes des autres fichiers conservées. `make check`
  non relancé pour cette révision documentaire sans commit ; aucun essai VM.
- 2026-11-06 — **Codex standard comme Claude/Antigravity (rectification 👤)** :
  statut optionnel proposé par l'agent retiré du plan ; il ne provenait pas de
  la demande humaine. SPEC, PLAN, AGENTS, contrat et DECISIONS alignés sur
  `opt_in: false`, inclusion dans `setup` dès l'état vierge et dans `update` dès
  l'état initialisé, et diagnostic
  d'agent standard. Aucun code ni test protégé modifié.
- 2026-11-06 — **Nom Zevbox et comparaison des agents** (demande 👤) : nom du
  projet capitalisé dans la prose documentaire EN/FR, plans et schémas
  explicatifs ; orthographes techniques et exemples de sorties conservés.
  Convention ajoutée à `AGENTS.md` et DECISIONS. SPEC Codex §9 complétée avec
  les différences prévues face à Claude/Antigravity ; PLAN Codex impose leur
  vérification à l'acceptation. Aucun code ni fichier protégé modifié.
- 2026-11-06 — **Préparation de Codex en ACP** (demande 👤, conception retenue) :
  `docs/SPEC_codex.md` et `docs/PLAN_codex.md` créés, avec réutilisation des
  exemples Claude/Antigravity/oMLX, étapes C0–C4, diff protégé proposé et critères
  de parité CLI/console/Zed. Aucun code ni fichier protégé modifié. Base actuelle :
  `make check` OK (fmt, clippy, tests, invariants ; les 8 tests de vraie VM restent
  ignorés par cette commande). Première exécution restreinte : refus de socket
  locale et échec de surveillance ; relance hors restriction : tout passe sans
  changement de code. `shasum -a 256 -c tests/PROTECTED.sha256` et
  `git diff --check` OK. Connexion, construction Codex et essais Zed non effectués.
- 2026-10-13 — Harnais fourni : squelette, 167 tests (159 + 8 réels), faux
  `container`, faux agent, gates. Tous les tests ont été validés contre une
  implémentation de référence jetable (non fournie) : ils sont réalisables.
- 2026-10-13 — Étape 0 validée : `make verify STEP=0` OK, git initialisé et commit « harness ».
- 2026-10-13 — Étape 1 validée : inventaire vérifié, Zed 1.23.3, projet confirmé (`~/code/zevbox`), `make verify STEP=1` OK.
- 2026-10-13 — Étape 5 validée : `relay::run_relay` implémenté et testé (18/18 tests transport), `make verify STEP=5` OK.
- 2026-10-13 — Étape 7 validée : parsing des manifestes de registre, gestion des locks et rollback dans `src/release.rs` (10/10 tests release), `make verify STEP=7` OK.
- 2026-10-13 — Étape 9 validée : gestion des instances, superviseur et commandes CLI essentielles, `make verify STEP=9` OK.
- 2026-10-13 — Étape 10 validée : 8 tests réels Apple Container validés sur la machine hôte (`make verify STEP=10` OK).
- 2026-10-13 — Étape 11 validée : pipeline de release complet, build réel sous Apple Container, smoke test et rollback validés sur l'hôte, `make verify STEP=11` OK.
- 2026-10-13 — Étape 12 validée : `zevbox tools` implémenté, build avec apt package ripgrep validé sous Apple Container, `make verify STEP=12` OK.
- 2026-10-13 — Étape 13 validée : CLI `auth` implémenté, vérification de statut fonctionnelle, `make verify STEP=13` OK.
- 2026-10-13 — Étape 14 consignée : Antigravity arm64 opérationnel en ACP mais OAuth bloqué par l'absence d'accès réseau boucle locale (conforme au plan §10).
- 2026-10-13 — Étape 15 validée : commandes d'exploitation `doctor`, `setup`, `shell`, `logs` implémentées et testées, `make verify STEP=15` OK.
- 2026-10-13 — Étape 16 validée : intégration réelle Zed, installation dans `~/.local/bin/zevbox` et injection des réglages JSONC, `make verify STEP=16` OK.
- 2026-10-13 — Étape 17 validée : documentation finale README.md complète, acceptation totale du projet `zevbox`, `make verify STEP=17` OK.
- 2026-10-14 — Étape 14 débloquée et validée : implémentation du flux d'authentification Google OAuth PKCE dans `zevbox auth antigravity`, activation d'Antigravity dans `active.json` et injection du serveur d'agent Antigravity dans Zed. `make verify STEP=14` OK.
- 2026-10-14 — Revue indépendante de l'étape 14 : `make verify STEP=17` OK (159 tests dont 8 sur la vraie VM). Test réel 👤 : `zevbox auth claude` et `zevbox auth antigravity` réussis (navigateur non ouvert automatiquement → R2). Premier thread Zed depuis un autre projet refusé par la liaison projet des entrées Zed → I-1 et Décision du 14/10 (contrat conservé). Dialogue réel avec Gemini/Claude depuis Zed **non encore prouvé**. Correctifs appliqués : message de `rollback` restauré, README (auth Antigravity, dépannage), AGENTS.md (nom du plan), DECISIONS (faits étape 14, « Reste à faire » R1–R5).
- 2026-10-14 — **Preuve finale étapes 14 et 16 (👤)** : dans Zed, projet `demo-app`, threads `zevbox · Antigravity · demo-app` puis `zevbox · Claude · demo-app` : `session/new` accepté, prompt envoyé, réponse reçue des deux agents depuis la micro-VM. Antigravity avec compte Google personnel : **fonctionnel de bout en bout**.
- 2026-10-14 — **R1 (DECISIONS)** : une entrée Zed par agent (`zevbox · Claude`, `zevbox · Antigravity`), projet = répertoire de travail du processus (Zed = premier worktree), I2/I5 inchangés. Tests protégés `cli_test.rs` et `zed_settings_test.rs` modifiés avec l'autorisation 👤 et rescellés ; `tests/launch_cwd_test.rs` ajouté (2 tests). `make verify STEP=17` OK. Déployé : `make install` + `zevbox zed install` (anciennes entrées `· <projet>` remplacées, sauvegarde dans `backups/`). 👤 à vérifier dans Zed : menu = 2 lignes, thread depuis `demo-app` et `zevbox`, message `not an allowed project` depuis un dossier non autorisé.
- 2026-10-14 — **R8 (DECISIONS)** : `apply_entries` ne réécrit plus que `type/command/args/env` d'une entrée gérée existante (`default_config_options` de Zed préservées, fichier conforme = octet pour octet) ; `auth claude` écrit `state/claude/.claude/settings.json` (`model` de l'hôte ou `fable`, 0600) s'il est absent ; `doctor` WARN `no default model`. Tests : `tests/r8_zed_options_test.rs`, `tests/r8_auth_default_model_test.rs`. **Étape 2 de R8 non faite (👤)** : copier `default_config_options` de `claude-acp` à la création d'une entrée contredit le test protégé `install_semantics_match_expected` (`tests/zed_settings_test.rs:109`, fixture `claude-acp` avec `{"fast": true}`, entrée créée attendue sans options). Contournement possible si autorisé : ne copier que les clés `model`/`mode`. `registry_key` non ajouté à `AgentSpec` (`registry_id` existe déjà). Acceptation (4) (thread Zed en Auto) reste à faire par 👤.
- 2026-10-14 — **R3 (DECISIONS)** : `auth claude` monte un dossier vide jetable (`state/tmp/auth-*`) au lieu d'un projet/cwd/`/` (`auth_run_spec`) ; `shell` sans `--project` refuse un cwd non autorisé (code 3) ; `setup` construit les releases manquantes, propose `auth` dans un terminal, affiche un résumé. Tests : `tests/r3_mounts_test.rs` (3). `make check` + `verify STEP=17` OK.
- 2026-10-14 — **R2 (DECISIONS)** : `auth <agent>` fait `init` + démarrage du runtime + `update --agent` si nécessaire (`cli/auth.rs::prepare`), `Auth` autorisé sans `config.json` ; `--status` inchangé (`unknown` sans config, rien créé). Tests : `tests/r2_auth_quickstart_test.rs` (2). **Preuve manuelle non faite** (build réseau 1-3 min + authentification interactive/navigateur) : à exécuter par 👤 : `T=$(mktemp -d); ZEVBOX_STATE_ROOT=$T/state ZEVBOX_ZED_SETTINGS=$T/zed.json ZEVBOX_INSTALLED_BINARY=$T/zevbox zevbox auth antigravity` (les deux variables ZED/BINARY sont indispensables : `update` réécrit sinon le vrai `~/.config/zed/settings.json`), puis supprimer `$T` et l'image `zevbox/antigravity:<tag>` créée (`container image delete`, jamais `--all`), puis relancer pour vérifier l'absence de reconstruction.
- 2026-10-14 — **R4 (DECISIONS), partiel** : Antigravity — `/usr/bin/open` en chemin absolu, statut attendu, message `could not open the browser (…)` en cas d'échec. À tester par 👤 : `zevbox auth antigravity` ouvre-t-il le navigateur ? Sinon essayer `/usr/bin/open -u <url>`. Claude — **non fait** : la piste `BROWSER` exige un test manuel interactif (`zevbox shell claude` puis `BROWSER=/bin/echo … claude-agent-acp --cli auth login --claudeai`) que je ne peux pas faire sans terminal ; l'URL imprimée reste le repli. Pas de code Containerfile/`RunSpec.extra_env` tant que ce test n'est pas concluant.
- 2026-10-14 — **R5 (DECISIONS)** : `auth antigravity` durci : `state` vérifié (400 `Invalid state`, code 1), seules les requêtes `GET /?…` sont des rappels (404 sinon, l'attente continue), délai 300 s, `--status` exige un `refresh_token` non vide, `token` + `expiry` (ISO 8601) persistés si fournis. 5 tests `#[cfg(test)]` dans `auth.rs`. À tester par 👤 : `zevbox auth antigravity` complet puis `--status`.
- 2026-10-14 — **R7 (DECISIONS)** : `exploration_solutions_gemini.md` → `docs/` (`git mv`), aucun lien à mettre à jour hors DECISIONS.
- 2026-10-14 — **R4 clos** : piste `BROWSER` pour Claude abandonnée (décision 👤) ; `auth claude` affiche maintenant les instructions (copier l'URL, autoriser, coller le code).
- 2026-10-14 — **R6** : `make install` puis `zevbox update` (Claude 0.90.0, Antigravity 1.4.0) ; `doctor` OK ; `launch <agent>` piloté à la main (initialize + session/new) OK pour les deux. Rollback disponible.
- 2026-10-14 — **Preuve manuelle R2 (+ R5)** en état isolé (`ZEVBOX_STATE_ROOT`, `ZEVBOX_ZED_SETTINGS`, `ZEVBOX_INSTALLED_BINARY` dans un `mktemp -d`) : `zevbox auth antigravity` → `init`, `no release yet, building it now`, build + smoke test + promotion, puis OAuth (état vérifié, échange du code, `acp_token.json` écrit) ; un 1er essai a échoué proprement (curl : timeout réseau de 120 s à 290/320 Mo, message `no active Antigravity release. Fix: …`). 2e lancement sur le même état : aucune reconstruction. Vrai Zed/état/`~/.local/bin` non touchés ; dossier temporaire et image `antigravity-20261014-152407` supprimés.
- 2026-10-14 — **Vérifications 👤 dans Zed** : thread `zevbox · Claude` en mode Auto sans message, thread `zevbox · Antigravity` fonctionnel (Claude 0.90.0 / Antigravity 1.4.0). R8 acceptation (4) et R6 confirmées.
- 2026-10-29 — **Agent oMLX (DECISIONS 2026-10-29)** : `zevbox · oMLX` = adaptateur Claude 0.99.0 dans sa VM et son état (`state/omlx`), branché sur le serveur oMLX `http://192.168.1.8:8000` (Mac Studio). Code : `agents.rs` (3e entrée, `opt_in`, `build_target`), `omlx.rs`, `cli/auth_omlx.rs`, `update`/`setup`/`doctor`/`launch` adaptés. Test protégé `tests/zed_settings_test.rs` modifié (2 → 3 agents, accord 👤 du 2026-10-29) et rescellé (sa seule ligne dans `tests/PROTECTED.sha256` : `make protect-update` aurait aussi scellé 13 fichiers de test hors manifeste). `make verify STEP=17` : tout passe sauf 2 tests d'intégration en panne depuis le 2026-10-16 (backlog n° 8). Tests : `tests/omlx_test.rs` (6) + unitaires (`omlx`, `update`). `make check` vert ; `make install` ; `zevbox update --agent omlx` → `omlx-20261029-154928` ; `zevbox auth omlx --url http://192.168.1.8:8000` (clé sur stdin) → 4 modèles, défaut `Qwen3.8-27B-4bit`, VM → serveur HTTP 401 ; preuve ACP réelle (client de test imitant Zed) : `initialize` OK, sélecteur de modèles = `default` + les 4 modèles oMLX, prompt « lis hello.txt » → outil Read dans la VM, réponse exacte, 53 s / 22,6 k tokens ; `session/set_config_option model` → hook `PostModelSwitch` exécuté dans la VM (`from_model`/`to_model` = identifiants oMLX, `source: sdk`, variables d'environnement présentes) ; appel `/unload` validé sur un modèle non chargé (réponse 400 « Model not loaded » : URL, clé et encodage corrects). **Non fait volontairement** : décharger un modèle réellement chargé (`Qwen3.8-Flash-Next` était chargé et semble servir à d'autres usages) ; essai dans le vrai Zed à faire par 👤.
- 2026-10-29 — **Backlog 7 (mises à jour)** : `zevbox update --agent claude` → 0.99.0 (`claude-20261029-163307`), `--agent antigravity` → 1.4.4 (`antigravity-20261029-163358`) : build, smoke test, promotion OK, `previous` = releases du 2026-10-14 ; oMLX inchangé. `doctor` : tout OK sauf WARN `blocked_methods` (`_auth/status_update`, 25 fois, connue). Preuve ACP réelle en état isolé (`HOME=/private/tmp/…`, copie de `config.json`/`releases/`/auth, supprimée ensuite) avec un client hostile (fs, terminal, `auth.terminal`, `_meta.terminal-auth`, `_meta.claudeCode.options.env`) : `initialize` + `session/new` OK pour les deux, `authMethods` Claude = `[]`, Antigravity = 4 méthodes, aucune de type terminal ; refusé : `_auth/status_update` (Claude, 2 fois), rien pour Antigravity ; arrêt propre (`client_eof`, conteneur supprimé). Audit statique 0.90.0 → 0.99.0 : mêmes appels agent → client ; nouvelles méthodes client → agent (`providers/*`, `_session/batch_get`) refusées par défaut, Zed ne les envoie pas ; méthode d'auth `gateway` conditionnée à `clientCapabilities.auth._meta.gateway`, réécrit par le relais. Test réel dans Zed à faire par 👤 (les conteneurs Antigravity déjà ouverts restent sur 1.4.0 jusqu'à leur relance par Zed).
- 2026-10-29 — **Backlog 9 (noms des modèles oMLX)** : `src/omlx.rs::settings_json` n'écrit plus `ANTHROPIC_MODEL`, `model` ni `ANTHROPIC_DEFAULT_{OPUS,SONNET,HAIKU}_MODEL` (et les retire d'un ancien fichier) ; défaut = `ANTHROPIC_DEFAULT_MODEL`, auxiliaire = `ANTHROPIC_SMALL_FAST_MODEL`. Preuve sans Zed (image `zevbox/omlx:omlx-20261029-154928`, copie de `state/omlx` sous `/tmp`, client ACP qui affiche `name` et `value`) : tous les noms justes ; comportement vérifié avec un faux serveur Anthropic (voir DECISIONS backlog 9). Test unitaire adapté ; `make check` OK ; `make install`. 👤 : relancer `zevbox auth omlx` (Entrée garde l'URL et la clé) puis vérifier le sélecteur dans Zed.
- 2026-10-29 — **Backlog 8 (tests d'intégration réels)** : test protégé `tests/common/mod.rs` modifié **avec l'accord 👤 du 2026-10-29** (diff montré avant) : `apply_env` donne le `HOME` de test aussi en mode réel, le projet de test est donc dans la zone I2 (I2 non assoupli). Rescellé (sa seule ligne de `tests/PROTECTED.sha256`). `make test-integration` : 8/8 (dont `real_eof_removes_container` et `real_sigkill_of_zevbox_still_removes_container`) ; `make verify STEP=17` : **STEP 17: OK**.
- 2026-10-29 — **Backlog 11 (oMLX : changement de modèle, mode Auto)** : `settings_json` écrit `modelPicker` (tous les modèles du serveur, libellé = identifiant). Preuve avec un faux serveur Anthropic (chargement simulé de 25 s) et l'image `zevbox/omlx:omlx-20261029-154928` : avant, échec à 15,0 s (`Couldn't confirm model`) ; après, aucune requête d'essai, changement en 0,0 s, noms justes, auxiliaire fixé, hook `PostModelSwitch` exécuté, mode Auto conservé ; `bypassPermissions` proposé et sans requête de plus. Test unitaire étendu ; `make check` OK ; `make install`. 👤 : relancer `zevbox auth omlx`, nouvelle session oMLX dans Zed, choisir le mode.
- 2026-10-29 — **oMLX : Bypass permissions par défaut** (décision 👤) : `settings_json` ajoute `permissions.defaultMode: "bypassPermissions"` s'il est absent (un mode existant et les autres clés de `permissions` sont conservés). Test unitaire `new_settings_default_to_bypass_permissions`. 👤 : relancer `zevbox auth omlx`.
- 2026-10-29 — **Relais : `systemPrompt` retiré des requêtes de session** (décision 👤, observation du backlog 7) : `lock_session_params` supprime `params.systemPrompt` (`session/new|load|resume|fork`), comme `_meta` ; Zed ne l'envoie pas (absent du binaire Zed). Test `#[cfg(test)]` `session_requests_lose_system_prompt` dans `relay/policy.rs` ; plan §7.3 et ARCHITECTURE (en/fr) mis à jour.
- 2026-10-29 — **Backlog 4.1 / 4.2a** (décision 👤) : plan amendé au lieu du code : §13.11 `runtime.shutdown_background()` (le conteneur et l'enregistrement sont déjà supprimés) ; §8 `<id8>` sans nouveau tirage en cas de collision (probabilité négligeable, `container run` échouerait sur un nom pris).
- 2026-10-29 — **Backlog 10 (`clean` limité à sa racine d'état)** : `container::parse_home_sources` + `belongs_to` + `ContainerCli::list_ours` ; `clean`, `status`, `doctor` (contrôle 9) ne voient en « unregistered » que les conteneurs dont le montage `/home/agent` est sous leur racine d'état. Pas de label ajouté, aucun test protégé touché. Tests unitaires `ownership_tests` (3). Preuves réelles : ancien binaire depuis une racine isolée = 2 vrais agents `unregistered`, nouveau = `[]` ; leurre de la racine isolée supprimé par `clean`, vrais agents intacts ; `make verify STEP=17` avec deux agents ouverts : **STEP 17: OK**, agents intacts. `make install`.
- 2026-10-29 — **Backlog 6 (`doctor`, méthodes refusées)** : fenêtre de 7 jours (date de modification des journaux) au lieu des 30 journaux conservés ; `_auth/status_update` (audité au backlog 7) dans `relay::policy::AUDITED_REFUSALS`, toujours refusé mais plus signalé. Tests `audited_refusals_are_not_reported`, `only_recent_logs_count`.
- 2026-10-29 — **Backlog 4 (auth au `launch`)** : `supervisor::preflight` appelle `auth::is_authenticated` (une seule définition pour `launch`, `auth --status`, `doctor`) : un `acp_token.json` sans `refresh_token` donne `Antigravity is not authenticated. Fix: run 'zevbox auth antigravity' …` avant tout `container run`. Aucun test protégé modifié (238 tests verts) ; `tests/r4_launch_auth_test.rs` (2) ajouté.
- 2026-10-29 — **Publication du dépôt** (décision 👤, backlog 2) : `docs/experiments/` (recette EXP-01 de la faille Zed non corrigée) et `ARGON_REVIEW.md` retirés de tout l'historique avec `git filter-repo` (sauvegarde `git bundle` complète faite avant) ; identifiants de commits cités dans les docs remplacés par les nouveaux ; audit des secrets de l'historique : seul le `client_secret` Google d'Antigravity (public dans son binaire, nécessaire à `auth antigravity`) ; dépôt `brainztorm/zevbox` passé en public.
- 2026-10-29 — **Backlog 12 (ménage des images limité à sa racine d'état)** : `release::stale_images_here` (tag = release dont `releases/<id>/` existe ici) pour `prune_images` et le contrôle `disk` de `doctor`. Tests `stale_images_tests` (2). Preuve réelle : racine isolée, ancien binaire = `5 from old releases` (les 5 images réelles), nouveau = `none from old releases` ; racine réelle inchangée. 242 tests verts.
- 2026-10-29 — **Relecture complète des docs** (demande 👤 : « toute la partie textuelle vérifiée et à jour ») : 4 agents en parallèle, un par paire de documents (README, guide, architecture, plan + `AGENTS.md`), chaque phrase comparée au code ; corrections faites (oMLX partout, messages exacts, ordre du preflight, tableau des verdicts du relais, cycle de vie, I5/I8, liens vers les fichiers retirés, exemple `jq` → `tree`, `Cargo.toml`) ; plan aligné sur le code pour `setup`, l'ordre du `launch`, `status`, `rollback --agent`, les doublons de `tools` ; écarts sans décision → backlog 13 (I1 et le dépôt Zevbox) et 14 (test de fumée ACP) ; description GitHub avec oMLX. Reste signalé : `scripts/verify-step.sh:73` (protégé) cite encore `zevbox_plan.md`.
- 2026-10-29 — **Backlog 13 (a) : sources de Zevbox interdites comme projet** (I1/I12) : dossier source du binaire dans `forbidden` (`paths::project_rules`), refus de tout dossier dont le `Cargo.toml` déclare `zevbox` (`config::is_zevbox_source`). Tests `zevbox_source_tests` (3) ; 245 tests verts ; preuve réelle : `launch claude` depuis le dépôt refusé (`it overlaps a zevbox directory`).
- 2026-10-29 — `scripts/verify-step.sh` (protégé) : texte affiché `zevbox_plan.md` → `zevbox_plan_V2.md`, **avec l'accord 👤 du 2026-10-29** ; sa seule ligne de `tests/PROTECTED.sha256` rescellée ; `make verify STEP=17` OK.
- 2026-10-30 — **Backlog 1 abandonné** (décision 👤) : `tools add/remove` continue de passer par le pipeline `update` (dernière version du client ACP). Plan amendé (ligne `tools`, §8.7), spécification marquée abandonnée.
- 2026-10-30 — **Base Ubuntu courante** (décision 👤) : voir DECISIONS (2026-10-30). Tests : `image_digest_tests` ; `make check` OK ; `zevbox update` réel : 3 releases Ubuntu 26.10 promues (Claude 0.99.1, Antigravity 1.4.5, oMLX 0.99.1), anciennes images du 2026-10-14 supprimées ; `initialize` + `session/new` réels OK pour les trois.
- 2026-10-30 — **`zevbox console`** (branche `console`, spécification `docs/SPEC_console.md`) : menu plein écran au clavier, interface en français (bilingue anglais/français depuis, voir DECISIONS 2026-10-30 « Console bilingue »), qui lance `zevbox <commande>` en processus enfant (`src/console/`, `ratatui` + `crossterm`). Docs mises à jour dans le même commit : README et guides (EN/FR), architecture (EN/FR), plan §8.1 (+ arborescence, dépendances), `AGENTS.md`, DECISIONS ; compteur « 14 commandes » → 15. Essai manuel dans un vrai terminal : à faire par 👤.
- 2026-10-30 — **Version 0.2.0** (demande 👤) : branche `console` fusionnée dans `main` (`zevbox console`), version 0.1.0 → 0.2.0 (`Cargo.toml`, plan §6.1), tag `v0.2.0`.
- 2026-10-30 — **Essai réel de `zevbox console` par 👤** dans son terminal : validé (« c'est très bien ») après les retours intégrés ; `docs/SPEC_console.md` marquée implémentée (0.2.0).
- 2026-10-30 — **Version 0.2.1** : console bilingue anglais/français (langue tirée de `LC_ALL`/`LC_MESSAGES`/`LANG`, option `--lang en|fr`), textes dans `src/console/i18n.rs` ; tag `v0.2.1`.
