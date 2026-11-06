# Annexe du plan Codex

Résultat de la répétition du 2026-11-06 ([PLAN_codex.md](../PLAN_codex.md) §1.3).
Les correctifs C1, C2a, C2 et C3 ainsi que le diff protégé ont été appliqués,
validés et retirés après C3. Le test protégé a été approuvé puis rescellé par
l’humain. Les commandes d’application du plan restent l’historique d’exécution.
C4 a été clôturée sur acceptation explicite de l’utilisateur le 2026-11-06.
Les quatre scripts ci-dessous restent pour les requalifications.

| Fichier | Étape | Usage |
|---|---|---|
| `isolated_env.sh` | C1 à C4 | crée une installation jetable sous `/tmp` |
| `verify_image.sh` | C1, C2, C4 | contrôle une image Codex : `PASS` ou `FAIL` |
| `verify_launch.py` | C2, C4 | contrôle `zevbox launch codex` à travers le relais : `PASS` ou `FAIL` |
| `cleanup_isolated.sh` | C1 à C4 | supprime seulement les images Codex puis le dossier jetable |

Les quatre correctifs et le diff protégé ont été retirés après C3 (PLAN §6). Les
scripts restent : ils requalifient une nouvelle version de Zed ou de l'adaptateur.

Un état jetable partage le runtime et le builder global. Aucun build ni nettoyage
pendant les tests d’intégration. Avant nettoyage, conserver hors de l’état un
inventaire des ressources créées ; le script ne supprime pas les images Claude
ou Antigravity d’un état mixte. Ne jamais supprimer de ressources étrangères.
