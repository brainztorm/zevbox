# Annexe du plan Codex

Résultat de la répétition du 2026-11-06 ([PLAN_codex.md](../PLAN_codex.md) §1.3).
Ces fichiers ne modifient rien par eux-mêmes : l'exécutant les applique ou les
lance aux étapes indiquées par le plan.

| Fichier | Étape | Usage |
|---|---|---|
| `C1_image_release.patch` | C1 | `git apply` (PLAN §4) |
| `C2a_relay_version.patch` | C2a | `git apply` (PLAN §5.1) |
| `C2_activation_auth.patch` | C2 | `git apply` (PLAN §5.2) |
| `protected_zed_settings.diff` | C2 | seulement après accord explicite de l'humain (PLAN §5.6) |
| `C3_console.patch` | C3 | `git apply` (PLAN §6) |
| `isolated_env.sh` | C1 à C4 | crée une installation jetable sous `/tmp` |
| `verify_image.sh` | C1, C2, C4 | contrôle une image Codex : `PASS` ou `FAIL` |
| `verify_launch.py` | C2, C4 | contrôle `zevbox launch codex` à travers le relais : `PASS` ou `FAIL` |
| `cleanup_isolated.sh` | C1 à C4 | supprime les images et le dossier d'une installation jetable |

Les quatre correctifs et le diff protégé sont retirés après C3 (PLAN §6). Les
scripts restent : ils requalifient une nouvelle version de Zed ou de l'adaptateur.
