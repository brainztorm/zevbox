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
| 14 | 👤 Antigravity | ⛔ | Antigravity 1.3.0 téléchargé et construit dans Apple Container (binaire arm64 `/opt/zevbox/antigravity/agy_acp_server.par`, groupe `nobody` ajouté dans `assets/Containerfile`). `initialize` répond via JSON-RPC sur stdout, mais `authenticate` (`oauth-personal`) lance un serveur HTTP local sur `127.0.0.1:<port>` dans la VM micro-VM pour le callback OAuth. Sans port mapping (`--publish` étant strictement interdit par l'invariant I1 et §8.2) et sans méthode terminale ou code out-of-band, l'authentification est bloquée. Aucune entrée Zed générée pour Antigravity, Claude reste intact (conforme §10). (2026-10-13). |
| 15 | doctor, setup, shell, logs, auth --status | ✅ | Implémenté : `src/cli/{doctor, setup, shell, logs}.rs`. Preuves : `doctor` rapporte 7/7 contrôles [OK] (config, container 1.5.0 arm64, runtime running, architecture, projet canonique, releases, permissions 0700) ; `setup` idempotent ; `shell` affiche la bannière exacte ; `logs` implémente `--path` et les 200 dernières lignes. `make verify STEP=15` OK (2026-10-13). |
| 16 | 👤 Intégration Zed réelle | ⬜ | |
| 17 | README, AGENTS.md, acceptation | ⬜ | |

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
