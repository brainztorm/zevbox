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
| 5 | Transport du relais | ⬜ | |
| 6 | container.rs | ✅ | **Exemple fourni** (2026-10-13) : `container_test` 12/12, clippy OK, invariants OK. |
| 7 | Releases (parsing, active.json) | ⬜ | |
| 8 | Réglages Zed (JSONC) | ✅ | **Exemple fourni** (2026-10-13) : `zed_settings_test` 23/23 ; essai sur une copie du vrai `~/.config/zed/settings.json` : 0 ligne supprimée, désinstallation = original à l'octet. |
| 9 | launch, status, stop, clean, init, project, zed | ⬜ | |
| 10 | Tests réels Apple Container | ⬜ | |
| 11 | update / build / smoke test / rollback | ⬜ | |
| 12 | tools | ⬜ | |
| 13 | 👤 Authentification Claude | ⬜ | |
| 14 | 👤 Antigravity | ⬜ | |
| 15 | doctor, setup, shell, logs, auth --status | ⬜ | |
| 16 | 👤 Intégration Zed réelle | ⬜ | |
| 17 | README, AGENTS.md, acceptation | ⬜ | |

## Journal

- 2026-10-13 — Harnais fourni : squelette, 167 tests (159 + 8 réels), faux
  `container`, faux agent, gates. Tous les tests ont été validés contre une
  implémentation de référence jetable (non fournie) : ils sont réalisables.
- 2026-10-13 — Étape 0 validée : `make verify STEP=0` OK, git initialisé et commit « harness ».
- 2026-10-13 — Étape 1 validée : inventaire vérifié, Zed 1.23.3, projet confirmé (`~/code/zevbox`), `make verify STEP=1` OK.
