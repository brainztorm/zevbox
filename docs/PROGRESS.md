# Avancement zevbox

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

## Journal

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
