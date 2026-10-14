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
- Digest de `node:22-bookworm-slim` vérifié : `sha256:c3de60bf2f9dd0ac6370e6117950ff62d6e339527e7472301c9c78a017978392` (via `container image list --format json`). Image de base figée : `node:22-bookworm-slim@sha256:c3de60bf2f9dd0ac6370e6117950ff62d6e339527e7472301c9c78a017978392`.

## Faits vérifiés — étape 14 (Antigravity, 2026-10-14)

- Zip `agy-acp-server-1.3.0-linux-arm64.zip` : le binaire `agy_acp_server.par` est à la racine de l'archive ; exécutable natif (flags absl `--[no]debug`, `--[no]notices`, `--helpfull`). Lancé sans argument : le `--uid=` du registre n'est pas un flag reconnu par `--helpfull` et n'est pas passé.
- Fichiers lus par le serveur (chaînes présentes dans le binaire) : `$HOME/.gemini/antigravity-acp/settings.json` (`{"auth":{"type":"oauth-personal"}}`) et `$HOME/.gemini/antigravity-acp/acp_token.json` (format google-auth `authorized_user` : `client_id`, `client_secret`, `refresh_token`, `token_uri`, `scopes`).
- Authentification retenue : OAuth 2.0 PKCE exécuté **sur l'hôte** par `zevbox auth antigravity` (client ID/secret d'application installée extraits du binaire public, listener `127.0.0.1:0`, échange du code via `curl`, écriture `0600`). Rejetés : clé API Gemini (facturation au jeton, pas le forfait AI Pro), réutilisation de `~/.gemini/oauth_creds.json` (client ID gemini-cli ≠ Antigravity → `unauthorized_client`), trousseau macOS (invite de mot de passe anxiogène), OAuth depuis la VM (navigateur hôte ne joint pas `127.0.0.1` de la VM sans `-p`, interdit par I1). Détails : `exploration_solutions_gemini.md`.
- Test réel 👤 (2026-10-14) : `zevbox auth claude` → `Login successful` ; `zevbox auth antigravity` → `Google Antigravity authenticated successfully.` (les deux : le navigateur ne s'est pas ouvert tout seul, l'URL imprimée a été copiée à la main). Le `session/new` depuis un autre projet que `zevbox` est refusé — comportement voulu, voir « Décisions » 2026-10-14 : le dialogue réel avec Gemini n'est donc **pas encore prouvé**.
- Zed (`crates/agent_servers/src/acp/transport.rs`, `spawn_stdio`, lu le 2026-10-14) : pour un projet local, le processus agent custom est lancé avec `current_dir` = premier chemin de `project.default_path_list()` (premier worktree). Le `cwd` de `session/new` vient de la même liste. → zevbox peut déduire le projet de son propre répertoire de travail au lieu de `--project`.
- Zed (`crates/project/src/agent_server_store.rs`, lu le 2026-10-14) : les entrées `agent_servers` sont lues avec `settings.get::<AllAgentServersSettings>(None)`, c'est-à-dire **depuis les réglages globaux uniquement** ; un `.zed/settings.json` local au projet n'est pas consulté et Zed n'offre aucun filtrage par projet. Conséquence : avec une entrée par (agent, projet), **toutes** les entrées `zevbox · …` sont visibles dans tous les projets (constaté le 2026-10-14 dans `demo-app`). Seule une entrée par agent, indépendante du projet, évite cet affichage.
- « Auto mode unavailable: the selected model does not support Auto mode; using Accept edits instead. » (2026-10-14, après R1) : message émis par **claude-agent-acp dans le conteneur** (`dist/session-mode.js`, `AUTO_MODE_FALLBACK_NOTICE`), pas par zevbox ni par le relais. Il apparaît quand la session démarre avec `mode = "auto"` et un modèle dont `supportsAutoMode !== true`. Cause sur cette machine : Zed a enregistré dans `~/.config/zed/settings.json` → `agent_servers."zevbox · Claude".default_config_options = {"model": "haiku", "mode": "auto"}` (Zed persiste les choix faits dans le sélecteur du thread ; `haiku` est aussi le `model` de `~/.claude/settings.json` de l'hôte). Haiku ne supporte pas Auto ; l'entrée `claude-acp` de l'hôte n'a pas le message car elle a `model: "fable"`. Le relais ne modifie pas ces options (`session/set_mode`, `session/set_model`, `session/set_config_option` sont relayés tels quels).
- Même test : `zevbox zed install` / `project add` / `update` / `rollback` (tous appellent `update_zed`) **écrasent** les `default_config_options` ajoutées par Zed aux entrées gérées : `zed_settings::apply_entries` compare l'objet existant à `{type, command, args, env}` et, s'il diffère, remplace l'objet entier (`prop.set_value`). → R8.
- Test réel 👤 (2026-10-14, soir) : depuis `demo-app`, `zevbox · Antigravity · demo-app` et `zevbox · Claude · demo-app` répondent à un prompt. Étapes 14 et 16 prouvées de bout en bout.

## Décisions

- 2026-10-13 — Langue : code, identifiants, messages CLI en anglais ; README en français.
- 2026-10-13 — `status` est en lecture seule (affiche `orphan`) ; `launch`, `stop` et `clean` nettoient.
- 2026-10-13 — `init` écrit `/opt/homebrew/bin/container` si le CLI est introuvable (et `doctor` le signale) plutôt que d'échouer.
- 2026-10-13 — Étape 1 : Zed version 1.23.3 (`/Applications/Zed.app`) ; premier projet autorisé : `~/code/zevbox`.
- 2026-10-14 (matin) — ~~Une entrée Zed par (agent, projet) est conservée.~~ **Révisée le soir même** : l'utilisateur constate que toutes les entrées `zevbox · …` s'affichent dans chaque projet et veut ne voir que celles du projet ouvert. Zed ne filtrant pas par projet (fait vérifié ci-dessus), la seule solution est **une entrée par agent** (`zevbox · Claude`, `zevbox · Antigravity`), le projet étant déduit du répertoire de travail → **R1**. La liste blanche I2 reste stricte (`zevbox project add` par projet) ; ce qui disparaît, c'est le suffixe projet des entrées Zed et le `--project` figé.
- 2026-10-14 — **Quick start cible** (ergonomie demandée) : `make install` (une fois) → `zevbox auth claude` et/ou `zevbox auth antigravity` (dans n'importe quel ordre ; chacune fait `init` + `update --agent <agent>` si nécessaire) → `zevbox project add <projet>` (une ligne par projet) → Zed. `setup` et `update` restent utilisables séparément → **R2**.
- 2026-10-14 — Parcours documenté : `setup` → `update` → `auth claude` + `auth antigravity` → `project add` (répétable). `auth` n'enregistre aucun projet : le projet `zevbox` présent dans `config.json` vient de `zevbox setup ~/code/zevbox` à l'étape 1/16, pas de l'authentification. Ce que l'utilisateur a vu pendant `auth claude` (bannière/conteneur nommés d'après `zevbox`) vient du montage du **premier** projet de la liste dans le conteneur d'auth → R1.
- 2026-10-13 — Étape 11 : Digest vérifié `node:22-bookworm-slim@sha256:c3de60bf2f9dd0ac6370e6117950ff62d6e339527e7472301c9c78a017978392`.

## À vérifier (remplir pendant les étapes indiquées)

- ~~Étape 13 : chemin réel du fichier d'identifiants Claude sous Linux (`authMarker`).~~ → `.claude/.credentials.json` (lock.json).
- ~~Étape 14 : structure du zip Antigravity, sens de `--uid=`, méthode d'authentification.~~ → voir « Faits vérifiés — étape 14 ».
- Étape 16 : affichage dans Zed des erreurs préalables (§8.5) — vérifié le 2026-10-14 : le message `zevbox: this agent is bound to …` est affiché tel quel dans le panneau agent. Comportement à l'archivage/fermeture : encore à consigner.
- Étape 14/16 : premier échange réel avec Gemini et avec Claude depuis Zed (`session/new` + un prompt) — `demo-app` a été ajouté le 2026-10-14 ; reste à sélectionner `zevbox · <Agent> · demo-app` dans Zed et consigner le résultat.

## Reste à faire (décidé le 2026-10-14, non implémenté)

Ordre de priorité. Chaque point est un correctif ou une déviation du contrat (plan §7/§8) ; aucun n'est commencé. R1 et R2 forment le « quick start » demandé le 2026-10-14.

### R1 — Une entrée Zed par agent, projet déduit du répertoire de travail — ✅ FAIT le 2026-10-14

> Implémenté (commit « r1: … ») : `launch --project` optionnel (`src/cli/launch.rs`), `zed_settings::entries_for(agents, command)`, `project add/remove` ne créent plus d'entrées, tests protégés `cli_test.rs`/`zed_settings_test.rs` modifiés avec l'autorisation 👤 et rescellés (`make protect-update`), nouveau `tests/launch_cwd_test.rs` (cwd autorisé / refusé), plan §1/§4/§8.1/§8.6/§8.9 amendés. Non fait : contrôle `doctor` n° 8 (il n'existait pas avant non plus) ; message « multi-root » (I5 garde `this agent is bound to …`). Point 9 (déploiement) : fait par l'agent, voir PROGRESS.

**Problème constaté** : les entrées Zed sont `zevbox · <Agent> · <projet>` avec `--project <chemin>` figé dans `args`. Comme `agent_servers` est un réglage global de Zed (sans filtrage par projet, fait vérifié), **toutes** les entrées apparaissent dans tous les projets (capture du 2026-10-14 : 4 entrées visibles dans `demo-app`, dont les 2 de `zevbox`) ; choisie depuis le mauvais projet, une entrée fait refuser `session/new` (`this agent is bound to …`). L'utilisateur veut ne voir que ce qui s'applique au projet ouvert : avec une entrée par agent, le menu montre exactement `zevbox · Claude` et `zevbox · Antigravity`, valables partout où le projet est autorisé.

**Résultat attendu dans Zed** : menu « External Agents » = `zevbox · Claude`, `zevbox · Antigravity` (2 lignes, quel que soit le nombre de projets). Dans un projet non autorisé : message `<cwd> is not an allowed project. Fix: run 'zevbox project add <cwd>'`.

**Plan** :
1. `zevbox launch <agent>` : rendre `--project` optionnel ; sans lui, projet = `std::env::current_dir()` (Zed lance le processus avec `current_dir` = premier worktree, fait vérifié ci-dessus). Garder `--project` pour les tests et le débogage.
2. Préflight inchangé : canonicalisation, puis contrôle I2 contre `config.projects`. Nouveau message si refusé : `zevbox: <cwd> is not an allowed project. Fix: run 'zevbox project add <cwd>' in a macOS terminal.` (déjà prévu §8.5).
3. I5 inchangé : le `cwd` de `session/new` doit toujours être exactement le projet (même source côté Zed, donc identiques sauf workspace multi-racines → message explicite `multi-root workspaces are not supported`).
4. `zed_settings::entries_for` : une entrée par agent, nommée `zevbox · Claude` / `zevbox · Antigravity`, `args: ["launch", "<agent>"]`, sans suffixe projet. `zed install` migre les anciennes entrées `zevbox · <Agent> · <projet>` (elles sont gérées, préfixe `zevbox · `, donc supprimables sans risque). `project add/remove` ne touche plus Zed sauf si les entrées manquent.
5. `doctor` contrôle 8 (§8.6) : vérifier les deux entrées par agent au lieu d'une par projet.
6. 👤 **Décision prise le 2026-10-14 : option (a)**, liste blanche stricte conservée (`zevbox project add` par projet ; confirmé par l'utilisateur). L'option (b) reste une idée reportée. Pour mémoire, les options étaient : (a) garder la liste blanche stricte (une commande `zevbox project add` par nouveau projet, pas de redémarrage de Zed) ; (b) ajouter des **racines autorisées** (`config.projectRoots`, ex. `~/code`) : tout sous-dossier direct ou indirect d'une racine est accepté, les interdits de `project add` (home, `/`, dossiers système, chemins contenant des fichiers zevbox — I12) restent appliqués au `cwd` reçu. Recommandation : (b), car c'est ce que demande l'usage ; à consigner comme amendement de l'invariant I2 dans le plan §3 et dans `tests/` via le 👤 (fichiers protégés : `cli_test.rs:142-192` et `zed_settings_test.rs:36-210` encodent le nommage `zevbox · <Agent> · <projet>` et `--project` ; ils devront être modifiés par l'humain puis rescellés avec `make protect-update`).
7. Tests : nouveaux cas `launch` sans `--project` (cwd autorisé / refusé / multi-racines), migration des entrées Zed, `doctor`. **Fichiers protégés** : `tests/cli_test.rs` et `tests/zed_settings_test.rs` (et tout autre test qui encode le nommage) sont modifiés par l'agent **avec l'autorisation explicite du 👤 donnée le 2026-10-14**, le diff des tests est présenté avant rescellement, puis `make protect-update` régénère `tests/PROTECTED.sha256`.
8. Docs : README §1-2 et §6, MANUEL (étapes 5 et 7, Test 3, dépannage), plan §7.3/§8.1/§8.5/§8.6 (amendement daté, l'ancien texte barré), PROGRESS (nouvelle ligne de journal « R1 »), DECISIONS (ce point passe en « fait »).
9. Déploiement 👤 après `make install` : `zevbox zed install` migre `~/.config/zed/settings.json` (sauvegarde dans `backups/`), puis vérification dans Zed : menu = 2 lignes, thread depuis `demo-app` et depuis `zevbox`, message `not an allowed project` depuis un dossier non autorisé.
10. Hors périmètre de R1 (inchangé) : `zevbox shell <agent>` (déjà repli sur le cwd), `stop --project`, `status`, le message `this agent is bound to …` du relais (I5, reste pour le cas multi-racines).

### R2 — Quick start : `zevbox auth <agent>` fait `init` + `update` si nécessaire

> 👤 2026-10-14 : **ne pas implémenter maintenant** (R2 et R3 attendront une demande explicite, après R1).

**Objectif** : trois commandes suffisent après `make install` : `zevbox auth claude` et/ou `zevbox auth antigravity` (ordre libre), `zevbox project add <projet>`, puis Zed. `setup`, `update`, `init` restent disponibles séparément.

**Plan** (`src/cli/auth.rs`, sans toucher aux fichiers protégés) :
1. Si `config.json` est absent : exécuter `init` (déjà idempotent) au lieu de sortir en code 3. `--status` garde le comportement actuel (`unknown`).
2. Si aucune release active pour l'agent demandé (ou image absente) : exécuter `update --agent <agent>` (réseau, 1-3 min, progression sur stderr) puis continuer. Si `update` échoue : message `Fix: run 'zevbox update --agent <agent>'`, code 3.
3. Démarrer le runtime Apple Container s'il est arrêté (comme `setup`).
4. Puis l'authentification proprement dite (inchangée). Idempotent : relancer `auth` quand tout est en place ne refait ni `init` ni `update`.
5. Le second agent suit le même chemin : `init` déjà fait → seulement `update --agent <autre>`.
6. `setup` est rendu conforme à §8.1 (voir R3) mais n'est plus requis dans le parcours.
7. Tests : nouveaux cas dans `src/cli/auth.rs` (`#[cfg(test)]`) ou un nouveau fichier `tests/auth_quickstart_test.rs` (fake `container` de `tests/` réutilisable) : auth sans config → init ; auth sans release → update appelé ; auth avec tout en place → aucun build.
8. Docs : README §1 et MANUEL (étapes 3-5 fusionnées en « Quick start », les commandes séparées gardées en référence).

### R3 — `zevbox auth` et `zevbox setup` : écarts avec le plan (priorité haute)

> 👤 2026-10-14 : **ne pas implémenter maintenant** (voir R2).

- **`auth claude` monte un projet utilisateur sans raison** (`src/cli/auth.rs:51-57`) : le conteneur d'auth est construit avec `RunSpec.project = config.projects.first()`, sinon `current_dir()`, sinon `/`. Conséquences : (1) le premier projet de la liste est monté en lecture/écriture dans un conteneur qui n'en a pas besoin ; (2) **sans aucun projet enregistré, `auth claude` lancé depuis `$HOME` monterait `$HOME`, et depuis un dossier illisible monterait `/`** — contraire à « Never mount $HOME » (AGENTS.md) et à I1. Correctif : monter à la place un dossier temporaire vide privé (`tempfile::tempdir()` sous `stateRoot/tmp`, 0700), ou pas de montage projet du tout si `container::run_args` le permet ; même revue pour `zevbox shell` sans `--project` (`src/cli/shell.rs:49-56`, repli sur `current_dir()`, à valider avec `validate_project_path`).
- **`setup` ne suit pas §8.1** : le plan prévoit `init → inventory-preexisting.json → doctor → update (les deux agents) → auth claude → auth antigravity (chacune passable avec `n`) → project add → résumé`. L'implémentation fait `init → [project add] → zed install → doctor`. Donc après `zevbox setup` seul, aucun agent n'est construit ni authentifié : `doctor` l'indique mais l'utilisateur doit lancer `update` et `auth` à la main (c'est ce que documentent README §1 et MANUEL étapes 3-5 en attendant). Correctif : ajouter `update` et les deux `auth` (interactives, avec question `[y/n]`) au flux `setup`, et le résumé final.



### R4 — `zevbox auth` : le navigateur ne s'ouvre pas automatiquement

- Claude : la commande tourne dans le conteneur ; `claude-agent-acp --cli auth login` ne peut pas ouvrir un navigateur hôte. Correctif : zevbox lit le stdout du conteneur, repère l'URL `https://claude.com/cai/oauth/authorize?…` et exécute `open <url>` sur le Mac (le reste du flux — coller le code — reste interactif).
- Antigravity : `open <url>` est appelé (`src/cli/auth.rs`) mais n'a pas ouvert le navigateur ; vérifier le code de retour de `open` et l'afficher au lieu de l'ignorer (`let _ =`), et tester avec `open -u`/`/usr/bin/open` explicite.
- Dans les deux cas l'URL est déjà imprimée : solution de repli fonctionnelle.

### R5 — Durcissement de `zevbox auth antigravity` (`src/cli/auth.rs`)

- Vérifier le paramètre `state` reçu au callback contre celui émis (anti-CSRF) ; refuser sinon.
- Timeout du callback 120 s → 300 s (choix de compte, 2FA).
- `--status` : exiger un JSON valide contenant `refresh_token`, pas seulement l'existence du fichier.
- Si le premier `session/new` échoue côté Gemini : persister aussi `token` (access_token) et `expiry` dans `acp_token.json`.
- Tests `#[cfg(test)]` pour `base64_url_encode` (vecteurs RFC 4648), `url_encode`, `extract_query_param`, `percent_decode`.
- Ne répondre qu'à la requête `GET /?code=…` : ignorer `/favicon.ico` et autres chemins au lieu de renvoyer 400.

### R6 — Mises à jour disponibles

- `zevbox update --check` (2026-10-14) : Claude 0.89.1 → 0.90.0, Antigravity 1.3.0 → 1.4.0. À appliquer avec `zevbox update` une fois le premier dialogue Zed prouvé (re-tester l'auth Antigravity : le format d'`acp_token.json` peut changer entre versions).

### R8 — « Auto mode unavailable » dans `zevbox · Claude` (constaté le 2026-10-14, cause trouvée)

**Faits vérifiés** (agent piloté à la main dans le conteneur avec le même montage d'état, 2026-10-14) :
- Le message est émis par claude-agent-acp (`dist/session-mode.js`), **une fois par session**, dès qu'un passage en mode `auto` est demandé alors que le modèle courant a `supportsAutoMode !== true`. Il arrive dans Zed comme `agent_message_chunk` + `current_mode_update: acceptEdits`.
- **Dans le conteneur, le modèle par défaut à `session/new` est « Default (recommended) = Haiku 4.5 »** (`claude-haiku-4-5-20251001`), car le dossier d'état ne contient aucun `~/.claude/settings.json` (seul `.credentials.json` y est écrit par `auth`). Sur l'hôte, `~/.claude/settings.json` fixe `model: haiku` et l'entrée `claude-acp` envoie `model: fable`.
- Zed applique le **mode avant le modèle** à la création du thread : `set_mode auto` arrive quand le modèle est encore Haiku 4.5 → repli `acceptEdits` + message. Le `set model=fable` qui suit ne rétablit pas Auto (le repli est définitif pour la session). D'où : « le message reste même avec opus/sonnet/fable, même dans un nouveau thread ».
- Séquence inverse testée : `set model=fable` **puis** `set_mode auto` → aucun message, Auto actif. Passer ensuite à `haiku` (5.5) ou `claude-haiku-4-5` → Auto retiré + message. Donc ici : Haiku (4.5 et 5.5) sans Auto ; Fable (et a priori Opus/Sonnet) avec.
- Hors zevbox : ni le relais (il relaie `session/set_mode` / `set_config_option` tels quels, dans l'ordre), ni I1–I12 ne sont en cause ; la seule différence avec l'hôte est l'absence de `settings.json` (modèle par défaut) dans l'état de l'agent.

**Correctif immédiat (👤, sans code, réversible)** : donner à l'agent du conteneur un modèle par défaut qui supporte Auto :
```bash
printf '{"model": "fable"}\n' > "$HOME/Library/Application Support/zevbox/state/claude/.claude/settings.json"
chmod 600 "$HOME/Library/Application Support/zevbox/state/claude/.claude/settings.json"
```
(ou `opus` / `sonnet`). Nouveau thread Zed → plus de message, Auto actif. Vérification : `zevbox shell claude` → `cat ~/.claude/settings.json`.

**Plan zevbox (non implémenté)** :
1. `zevbox auth claude` (ou une commande dédiée `zevbox agent-config claude --model fable`) écrit `state/claude/.claude/settings.json` (0600) avec `{"model": …}` : valeur = `--model` si donné, sinon le `model` lu (lecture seule, jamais monté) dans `~/.claude/settings.json` de l'hôte s'il existe, sinon rien. Ne jamais écraser un fichier existant sans `--force`. `doctor` : `WARN agent:claude — no default model in state (Auto mode falls back to Accept edits); Fix: …`.
2. `zed_settings::apply_entries` : ne gérer que `type`, `command`, `args`, `env` ; **conserver** les autres clés (`default_config_options`) octet pour octet (aujourd'hui tout `update_zed` — `zed install`, `project add/remove`, `update`, `rollback` — remplace l'objet entier dès qu'il diffère).
3. À la **création** d'une entrée gérée seulement : copier `default_config_options` de l'entrée registre homonyme si présente (`claude-acp` → `zevbox · Claude`, `antigravity-acp` → `zevbox · Antigravity`), sinon rien.
4. Tests : fixture avec `default_config_options` → conservées après `apply_entries` (y compris quand `command` change) ; création → copie depuis `claude-acp` ; `uninstall` → entrée entière supprimée ; `auth claude` → `settings.json` créé / non écrasé. `tests/zed_settings_test.rs` et `cli_test.rs` (protégés) à compléter avec autorisation 👤, ou nouveaux fichiers de test.
5. Docs : MANUEL (étape 4a : modèle par défaut ; étape 7 : message Auto mode), README dépannage, plan §8.1/§8.9.

### R7 — Divers

- `exploration_solutions_gemini.md` : déplacer sous `docs/` (ou résumer ici) ; il contient le client secret Google, déjà public dans le binaire, mais à garder à l'esprit si le dépôt devient public.
- `lock.json` d'`antigravity-20261013-074439` a été édité à la main pour `authMarker` (release construite avant le correctif) : le prochain `zevbox update` régénérera un lock cohérent.

## Idées reportées

- Racines autorisées (`config.projectRoots`) à la place de la liste blanche par projet — option (b) de R1, écartée le 2026-10-14.
