# Faits vérifiés et décisions

Format : date — fait ou décision — comment c'est vérifié.

## Faits vérifiés sur la machine cible (2026-10-13)

- macOS 26.7.1 arm64, UID 501, GID 20. `container` 1.5.0 à `/opt/homebrew/bin/container`, système `running`.
- Conteneurs préexistants (ne jamais toucher) : `buildkit`, `kiac-forgejo-agentic-worker-1`, `kiac-forgejo-agentic-control-plane`.
- `container list --all --format json` : `[{configuration:{id, labels, ...}, id, status:{state, ...}}]`.
- `container image list --format json` : `[{configuration:{name: "docker.io/library/debian:bookworm-slim", ...}, id}]` — **noms entièrement qualifiés** : une image `zevbox/claude:<tag>` apparaît comme `docker.io/zevbox/claude:<tag>` → comparer avec `container::image_matches`.
- `container system status --format json` : `{status: "running", client:{version}, host:{architecture}}`.
- `--mount type=bind,source=<chemin avec espace>,target=...` fonctionne → `stateRoot = ~/Library/Application Support/zevbox` est conservé.
- `--uid 501 --gid 20` : les fichiers créés dans le projet appartiennent à 501:20 sur le Mac. `--cap-drop ALL` + `--init` : fonctionnement normal.
- Dans la VM, `/proc/mounts` montre les montages hôte en `virtiofs` (espace encodé `\040`).
- Les lignes de progression de `container run` vont sur **stderr** (stdout propre) ; zevbox passe quand même `--progress none`.
- Après `container run --rm`, le conteneur peut exister encore un court instant : toujours vérifier avec `list` puis `delete --force` (procédure §8.3).
- `SIGKILL` du processus `zevbox launch` : le stdin du conteneur se ferme, l'agent sort, le conteneur disparaît (< 15 s). Vérifié avec l'implémentation de référence (`tests/integration_test.rs`).
- Réglages Zed : `~/.config/zed/settings.json` (0600) contient déjà `agent_servers.claude-acp` (ne pas toucher).
- `jsonc-parser` 0.34 (features `cst`, `serde`) : l'édition CST préserve commentaires et virgules finales (API utilisée : `CstRootNode::parse`, `root.value()`, `root.object_value()`, `root.object_value_or_set()`, `obj.get(name)`, `prop.object_value()`, `obj.object_value_or_set(name)`, `obj.properties()`, `prop.decoded_name()`, `prop.value()?.to_serde_value()`, `prop.set_value(CstInputValue)`, `prop.remove()`, `obj.append(name, CstInputValue)`, `root.to_string()`). Rendu vérifié : nouvelles entrées indentées comme le fichier, idempotence octet pour octet.

## Décisions

- 2026-10-13 — Langue : code, identifiants, messages CLI en anglais ; README en français.
- 2026-10-13 — `status` est en lecture seule (affiche `orphan`) ; `launch`, `stop` et `clean` nettoient.
- 2026-10-13 — `init` écrit `/opt/homebrew/bin/container` si le CLI est introuvable (et `doctor` le signale) plutôt que d'échouer.

## À vérifier (remplir pendant les étapes indiquées)

- Étape 11 : digest de `node:22-bookworm-slim` (commande exacte).
- Étape 13 : chemin réel du fichier d'identifiants Claude sous Linux (`authMarker`).
- Étape 14 : structure du zip Antigravity, sens de `--uid=`, méthode d'authentification.
- Étape 16 : affichage dans Zed des erreurs préalables (§8.5) et comportement à l'archivage/fermeture.

## Idées reportées

(rien pour l'instant)
