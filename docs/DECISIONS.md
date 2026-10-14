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

## Fait depuis l'acceptation (étape 17)

- **R1 — Une entrée Zed par agent, projet déduit du répertoire de travail — ✅ 2026-10-14** (commit `9128163`). Motif : `agent_servers` est un réglage global de Zed sans filtrage par projet ; une entrée par (agent, projet) affichait toutes les entrées partout. Résultat : menu Zed = `zevbox · Claude` et `zevbox · Antigravity` ; `zevbox launch <agent>` sans `--project` prend `std::env::current_dir()` (Zed lance l'agent dans le premier worktree), I2/I5 inchangés ; `project add/remove` ne touchent plus aux entrées ; tests protégés `cli_test.rs`/`zed_settings_test.rs` modifiés **avec l'autorisation 👤 du 2026-10-14** et rescellés ; `tests/launch_cwd_test.rs` ajouté. Décision associée : liste blanche I2 stricte conservée (option « racines autorisées » → Idées reportées). Non fait, volontairement : contrôle `doctor` n° 8 (n'existait pas) ; message « multi-root » (le relais garde `this agent is bound to …`).

## Reste à faire — mode d'emploi pour l'agent implémenteur

Ce qui suit est écrit pour être exécuté sans autre contexte que `AGENTS.md`, `zevbox_plan_V2.md` et ce fichier. Lire aussi les « Faits vérifiés » plus haut.

**Règles communes (obligatoires)**
1. **Ordre d'exécution : R8 → R3 → R2 → R4 → R5 → R6 → R7.** Un R à la fois. Ne pas commencer le suivant avant que le précédent soit commité.
2. Avant de coder un R : lire les fichiers listés dans sa rubrique « Fichiers ». Copier le style des fichiers `WORKED EXAMPLE` (en-tête de fichier). Messages CLI en anglais, format `<problème>. Fix: <commande exacte>`.
3. Interdits inchangés : `unwrap/expect/panic` dans `src/` (clippy les refuse), shell hôte (`sh -c`), montage de `$HOME`, `--all`, écriture dans un projet utilisateur, `-t` sur le conteneur ACP. Invariants I1–I12 du plan §3.
4. **Fichiers protégés** (`tests/**` listés dans `tests/PROTECTED.sha256`, `scripts/**`, `clippy.toml`) : **ne pas les modifier**. Les nouveaux tests vont dans de **nouveaux fichiers** `tests/r<N>_<sujet>_test.rs` (ils peuvent faire `mod common;` et utiliser `common::TestEnv`, voir `tests/launch_cwd_test.rs` comme modèle) ou dans `#[cfg(test)]` du module. Si un R ne peut pas être fait sans toucher un test protégé : s'arrêter, écrire pourquoi dans `docs/PROGRESS.md`, demander au 👤.
5. Le faux `container` (`tests/fixtures/fake-container`) ne sait pas faire `build` ni accéder au réseau : tout ce qui construit une image ou appelle le registre se teste **à la main** (preuve à consigner dans `docs/PROGRESS.md`), pas en test automatique.
6. Fin de chaque R : `make check` (fmt + clippy -D warnings + tests + invariants) puis `make verify STEP=17` doit afficher `STEP 17: OK` ; `make install` ; test manuel indiqué dans « Acceptation » ; ligne de journal dans `docs/PROGRESS.md` (`- <date> — R<N> : …`) ; déplacer le R dans « Fait depuis l'acceptation » ci-dessus (3 lignes max) ; `git commit -m "r<N>: <titre>"`.
7. Les lignes de code citées (`fichier:ligne`) datent du 2026-10-14 : vérifier avec `grep` avant d'éditer.

### R8 — « Auto mode unavailable » dans `zevbox · Claude` : modèle par défaut + `default_config_options` préservées

**Faits vérifiés** (agent piloté à la main dans le conteneur avec le même montage d'état, 2026-10-14) :
- Le message `**Auto mode unavailable:** the selected model does not support Auto mode; using Accept edits instead.` est émis par claude-agent-acp (`dist/session-mode.js`), une fois par session, dès qu'un passage en mode `auto` est demandé alors que le modèle courant n'a pas `supportsAutoMode`. Il arrive dans Zed comme `agent_message_chunk` + `current_mode_update: acceptEdits`.
- Dans le conteneur, le modèle par défaut à `session/new` est « Default (recommended) = Haiku 4.5 » (`claude-haiku-4-5-20251001`) parce que `state/claude/.claude/settings.json` n'existe pas (`auth` n'écrit que `.credentials.json`). Sur l'hôte, `~/.claude/settings.json` contient `{"model": "haiku", …}` et l'entrée `claude-acp` de Zed envoie `model: fable`.
- Zed applique le **mode avant le modèle** à l'ouverture d'un thread : `set_mode auto` arrive quand le modèle est encore Haiku 4.5 → repli `acceptEdits` + message ; le `model=fable` qui suit ne rétablit pas Auto (repli définitif pour la session). D'où le message avec tous les modèles et à chaque nouveau thread.
- Séquence inverse testée : `model=fable` **puis** `mode=auto` → pas de message, Auto actif. Passer ensuite à `haiku` (5.5) ou Haiku 4.5 → Auto retiré + message. Ici : Haiku (4.5 et 5.5) sans Auto ; Fable (et a priori Opus/Sonnet) avec.
- Zed **écrit lui-même** `default_config_options` dans l'entrée gérée quand l'utilisateur choisit modèle/mode (vu : `"zevbox · Claude": {"default_config_options": {"model": "haiku", "mode": "auto"}, …}`). Or `zed_settings::apply_entries` (`src/zed_settings.rs:130-140`) compare l'objet existant à `entry_json` = `{type, command, args, env}` et, s'il diffère, remplace **l'objet entier** (`prop.set_value(entry_input(entry))`) → les choix de l'utilisateur sont effacés par `zevbox zed install`, `project add/remove`, `update`, `rollback` (tous appellent `cli::project::update_zed`).
- Ni le relais ni I1–I12 ne sont en cause.

**Correctif immédiat (👤, sans code, réversible)** — à appliquer tout de suite si ce n'est pas fait :
```bash
printf '{"model": "fable"}\n' > "$HOME/Library/Application Support/zevbox/state/claude/.claude/settings.json"
chmod 600 "$HOME/Library/Application Support/zevbox/state/claude/.claude/settings.json"
```
Nouveau thread Zed → plus de message, Auto actif. Vérification : `zevbox shell claude` → `cat ~/.claude/settings.json`.

**Fichiers** : `src/zed_settings.rs` (WORKED EXAMPLE), `src/cli/auth.rs`, `src/cli/doctor.rs`, `src/fsutil.rs` (`write_atomic`, `ensure_private_dir`), `src/paths.rs` (`home_dir()`, `state_dir(agent)`), `tests/zed_settings_test.rs` (lecture seule : montre l'usage de `apply_entries`/`install`), `tests/fixtures/zed/with_comments.json`.

**Étapes**
1. `zed_settings::apply_entries` : pour une clé gérée existante, ne plus remplacer l'objet. À la place, obtenir l'objet (`prop.object_value()`), et pour chacune des quatre clés `type`, `command`, `args`, `env` : si `obj.get(k)` existe et que `to_serde_value()` est déjà égal → rien ; sinon `set_value` ; si absente → `obj.append(k, …)`. Toute autre clé (`default_config_options`, etc.) est laissée telle quelle. Si la valeur gérée existante n'est pas un objet → la remplacer entièrement (comportement actuel). Idempotence : un fichier déjà conforme ressort octet pour octet (test `install_is_idempotent_bytes` protégé doit continuer à passer).
2. À la **création** d'une entrée gérée (clé absente) : si `agent_servers` contient une entrée `claude-acp` (pour l'agent `claude`) ou `antigravity-acp` (pour `antigravity`) avec un objet `default_config_options`, copier cet objet (`to_serde_value()` → `CstInputValue`) dans la nouvelle entrée, en première position ; sinon ne rien ajouter. Table agent → clé registre : ajouter un champ `registry_key: &'static str` dans `agents::AgentSpec` (`"claude-acp"`, `"antigravity-acp"`) plutôt qu'une constante locale. Attention : `entry_json()` sert aussi à « déjà à jour » — il ne doit comparer que les quatre clés gérées.
3. `src/cli/auth.rs`, branche Claude, **avant** de lancer le conteneur : si `state/claude/.claude/settings.json` n'existe pas, écrire `{"model": "<m>"}` en 0600 (`write_atomic`) où `<m>` = valeur de la clé `model` de `~/.claude/settings.json` de l'hôte si ce fichier existe, est du JSON valide et contient une chaîne `model` (lecture seule, jamais monté) ; sinon `"fable"`. Si le fichier existe déjà : ne rien faire (même si `model` manque). Afficher `claude: default model set to <m> (state/claude/.claude/settings.json)` seulement quand on l'écrit. Ne jamais copier d'autre clé que `model`.
4. `src/cli/doctor.rs`, contrôle `agent:claude` : si `state/claude/.claude/settings.json` est absent ou sans clé `model` → statut `WARN`, message `no default model in state/claude/.claude/settings.json (Auto mode falls back to Accept edits in Zed). Fix: run 'zevbox auth claude' or write {"model": "fable"} in that file`. Ne pas changer le code de sortie (WARN ≠ FAIL).
5. Tests, nouveau fichier `tests/r8_zed_options_test.rs` (fonctions pures, pas de binaire) : (a) `apply_entries` sur un JSONC contenant `"zevbox · Claude": {"default_config_options": {"model": "haiku", "mode": "auto"}, "type": "custom", "command": "/old", "args": ["launch","claude"], "env": {}}` avec une entrée dont `command` = `/new` → le résultat contient toujours `default_config_options` à l'identique et `command` = `/new` ; (b) même entrée déjà conforme → sortie identique à l'entrée octet pour octet ; (c) création de `zevbox · Claude` dans un fichier qui a `"claude-acp": {"default_config_options": {"mode": "auto", "model": "fable"}, "type": "registry"}` → la nouvelle entrée contient ces options ; sans `claude-acp` → pas de `default_config_options` ; (d) `apply_entries(…, &[])` supprime l'entrée entière, options comprises. Nouveau fichier `tests/r8_auth_default_model_test.rs` avec `common::TestEnv` : `env.run(&["auth","claude","--status"])` ne crée pas le fichier ; le lancement non interactif de `auth claude` n'est pas testable (conteneur `-it`) → la création du fichier est testée via une fonction publique `cli::auth::ensure_claude_default_model(paths, host_settings: &Path) -> io::Result<Option<String>>` appelée directement.
6. Docs : MANUEL étape 4a (le modèle par défaut est écrit par `auth`, comment le changer), étape 7 (que faire si le message Auto apparaît), README §6 (entrée dépannage), plan §8.9 (clés gérées = `type, command, args, env` ; autres clés préservées).

**Acceptation** : (1) tests ci-dessus verts + `make verify STEP=17` ; (2) à la main : renommer `state/claude/.claude/settings.json` en `.bak`, `zevbox auth claude --status` ne le recrée pas, `zevbox doctor` affiche le WARN ; remettre le fichier ; (3) dans `~/.config/zed/settings.json`, laisser `default_config_options` sur `zevbox · Claude`, lancer `zevbox zed install` → `already up to date`-équivalent (aucune sauvegarde créée dans `backups/`, options intactes) ; (4) 👤 : nouveau thread `zevbox · Claude` en mode Auto → aucun message.

### R3 — `zevbox auth` et `zevbox shell` : ne jamais monter un dossier non autorisé ; `setup` conforme à §8.1

**Problème** (`src/cli/auth.rs:51-57`) : le conteneur d'authentification Claude est construit avec `RunSpec.project = config.projects.first()`, sinon `std::env::current_dir()`, sinon `/`. Donc (1) le premier projet est monté en lecture/écriture dans un conteneur qui n'en a pas besoin ; (2) sans projet enregistré, lancé depuis `$HOME` il monterait `$HOME`, et `/` si le cwd est illisible — contraire à I1 et à « never mount $HOME ». `zevbox shell` (`src/cli/shell.rs:48-60`) se rabat aussi sur `config.projects.first()`.

**Fichiers** : `src/cli/auth.rs`, `src/cli/shell.rs`, `src/container.rs` (`RunSpec`, `run_args` — WORKED EXAMPLE, ne pas changer la signature), `src/paths.rs`, `src/cli/setup.rs`, `src/cli/update.rs` (`run_update(check, agent)`), `tests/lifecycle_test.rs` et `tests/cli_test.rs` (lecture seule).

**Étapes**
1. `auth.rs` (branche Claude) : remplacer le choix du projet par un dossier temporaire vide : `paths.state_root.join("tmp")` (créer 0700 avec `ensure_private_dir`), puis `tempfile::Builder::new().prefix("auth-").tempdir_in(...)` ; passer son chemin canonique dans `RunSpec.project` ; le `TempDir` est supprimé à la sortie (drop). Ainsi I1 est respecté (deux montages : ce dossier vide + `state/claude`). Ne plus lire `config.projects` ni `current_dir()` dans `auth`.
2. `shell.rs` sans `--project` : si le cwd canonique est dans `config.projects` → l'utiliser ; sinon **erreur** code 3 : `<cwd> is not an allowed project. Fix: run 'zevbox project add <cwd>' or pass --project <path>`. Supprimer le repli sur `config.projects.first()`.
3. `setup.rs` : après `init` et avant `zed install`, ajouter : `update` des deux agents via `crate::cli::update::run_update(false, None).await` **si** au moins un agent n'a pas de release active (`release::active_lock` → `Err(NoActiveRelease)`), sinon afficher `releases: up to date`. Puis, pour chaque agent sans témoin d'auth (`paths.state_dir(id).join(&lock.auth_marker)` absent) et seulement si stdin est un terminal (`std::io::IsTerminal`), poser `authenticate <Display> now? [y/N] ` ; `y` → `crate::cli::auth::run(agent, false).await`. Hors terminal : afficher `skipping authentication (not a terminal). Fix: run 'zevbox auth <id>'`. Terminer par le résumé : pour chaque agent `  <Display>: release <id|none>, <authenticated|not authenticated>` puis `next: zevbox project add <path>` si `config.projects` est vide.
4. Tests, nouveau fichier `tests/r3_mounts_test.rs` avec `common::TestEnv` et le faux `container` (il journalise chaque invocation dans `env.container_calls()`, les montages sont dans les arguments `--mount type=bind,source=…`) : (a) `shell claude` lancé avec `current_dir` = `env.home` (pas un projet) → code 3, aucune ligne `run ` dans le journal ; (b) `auth claude` ne peut pas tourner en test (`-it`), donc extraire la construction du `RunSpec` dans une fonction `cli::auth::auth_run_spec(&config, &paths, &lock, tmp: &Path) -> RunSpec` et tester que `run_args(&spec)` ne contient aucun `source=` autre que `tmp` et `state/claude` ; (c) `setup` sans terminal (stdin null, c'est le cas de `env.run`) ne lance aucun conteneur `-it` et affiche `skipping authentication`.
5. Docs : README §3 (`setup`, `auth`, `shell`), MANUEL étapes 3-4, plan §8.1 (ligne `setup` : marquer « conforme depuis R3 »).

**Acceptation** : tests verts + `make verify STEP=17` ; à la main : `cd ~ && zevbox shell claude` → message d'erreur, code 3 ; `zevbox auth claude --status` inchangé ; `zevbox setup` (tout déjà en place) affiche le résumé sans rien reconstruire.

### R2 — Quick start : `zevbox auth <agent>` fait `init` + `update` si nécessaire

**Objectif** : après `make install`, trois commandes suffisent : `zevbox auth claude` et/ou `zevbox auth antigravity` (ordre libre), `zevbox project add <projet>`, Zed. `setup`, `update`, `init` restent disponibles séparément.

**Fichiers** : `src/cli/mod.rs` (garde « not set up » ligne ~205 : `Command::Init | Command::Setup { .. } | Command::Launch { .. }`), `src/cli/auth.rs`, `src/cli/init.rs` (`run()`), `src/cli/update.rs` (`run_update(false, Some(agent))`), `src/release.rs` (`active_lock`), `src/container.rs` (`ContainerCli::{image_list, system_status, system_start}`, `image_matches`).

**Étapes**
1. `cli/mod.rs` : ajouter `Command::Auth { .. }` à la liste des commandes autorisées sans `config.json`.
2. `auth.rs`, au début de `run` (sauf si `status == true`, qui garde le comportement actuel : `unknown` sans config) : (a) si `paths.config_file()` est absent → `crate::cli::init::run().await?` ; si code ≠ 0, le retourner. (b) `ContainerCli::system_status` ; si non `running` → `system_start` puis revérifier ; échec → `Apple Container system is not running. Fix: run 'container system start'`, code 3. (c) si `release::active_lock(&paths, agent.id())` est `Err(NoActiveRelease)` **ou** si l'image du lock n'est pas dans `image_list()` (`image_matches`) → afficher `<Display>: no release yet, building it now (network, 1-3 min)…` puis `crate::cli::update::run_update(false, Some(agent)).await?` ; si ensuite toujours pas de release → `no active <Display> release. Fix: run 'zevbox update --agent <id>'`, code 3. (d) continuer avec l'authentification existante. Idempotent : tout en place → ni `init` ni `update`.
3. Tests, nouveau fichier `tests/r2_auth_quickstart_test.rs` : (a) `TestEnv::bare()` + `auth claude --status` → `unknown`, et `config.json` **n'est pas** créé ; (b) `TestEnv::new()` (release active, image présente dans le faux `container`) + `auth claude --status` → `authenticated` et le journal du faux `container` ne contient pas `build`. Le chemin « init puis update » **n'est pas testable** avec le faux `container` (pas de `build`, pas de réseau) : preuve manuelle obligatoire, voir Acceptation.
4. Docs : README §1 (quick start = `make install`, `auth …`, `project add …`, Zed ; les commandes séparées restent dans le tableau §3), MANUEL (étapes 3 et 4 fusionnées : « Étape 3 : Authentifier (construit l'image au premier passage) »), plan §8.1 (ligne `auth` : amendement daté).

**Acceptation** : tests verts + `make verify STEP=17` ; à la main, dans un état vierge (`ZEVBOX_STATE_ROOT=$(mktemp -d) zevbox auth antigravity`) : la commande initialise, construit l'image Antigravity, puis ouvre l'authentification ; relancée, elle ne reconstruit rien. Consigner la sortie dans PROGRESS.

### R4 — `zevbox auth` : ouvrir le navigateur du Mac

**Constat** (2026-10-14) : pour Claude comme pour Antigravity, l'URL s'affiche mais le navigateur ne s'ouvre pas. L'URL imprimée est un repli acceptable ; ne pas y passer plus d'une demi-journée.

- **Antigravity** (`src/cli/auth.rs`, `auth_antigravity`) : `std::process::Command::new("open").arg(&auth_url).spawn()` est appelé avec `let _ =`. Étapes : utiliser `/usr/bin/open` en chemin absolu (le `PATH` peut être minimal), attendre le statut (`.status()`), et si échec afficher `could not open the browser (<erreur>): open the URL above manually`. Tester à la main : si ça marche désormais, fini. Sinon, essayer `/usr/bin/open -u <url>` et consigner le résultat.
- **Claude** : la commande tourne dans le conteneur ; elle ne peut pas ouvrir un navigateur hôte. Piste à **vérifier d'abord à la main** (10 min) : Claude Code respecte la variable `BROWSER` (il exécute `$BROWSER <url>`). Test : `zevbox shell claude` puis `BROWSER=/bin/echo /opt/zevbox/claude/node_modules/.bin/claude-agent-acp --cli auth login --claudeai` ; si l'URL est bien passée à `echo`, implémenter : dans `assets/Containerfile` (étape `claude`), ajouter un script `/opt/zevbox/open-url` (`#!/bin/sh` ; `printf '%s\n' "$1" > "$HOME/.zevbox-open-url"`) ; dans `auth.rs`, ajouter `-e BROWSER=/opt/zevbox/open-url` aux arguments (via un champ `extra_env: Vec<(String,String)>` dans `RunSpec`, vide par défaut, écrit par `run_args`), et pendant que le conteneur tourne, une tâche tokio relit `state/claude/.zevbox-open-url` toutes les 200 ms, lance `/usr/bin/open <url>` et supprime le fichier. Si `BROWSER` n'est pas honoré : consigner « non faisable, URL à copier » dans ce fichier et fermer R4 côté Claude.
- Tests : `#[cfg(test)]` sur la fonction qui lit le fichier d'URL (n'ouvre rien si le fichier est absent ou si l'URL ne commence pas par `https://claude.com/` ou `https://claude.ai/`). Docs : MANUEL étape 4 (retirer « le navigateur ne s'ouvre pas » si corrigé).

### R5 — Durcissement de `zevbox auth antigravity` (`src/cli/auth.rs`)

Étapes, toutes dans `auth_antigravity` et ses helpers :
1. Vérifier le paramètre `state` reçu dans la requête de rappel (`extract_query_param(&req, "state")`) contre celui généré ; différent ou absent → réponse HTTP 400 `Invalid state`, message `Google authentication failed: state mismatch`, code 1. Ne jamais poursuivre l'échange.
2. Ne traiter que `GET /?…` : si le chemin de la première ligne ne commence pas par `/?`, répondre `404` et **continuer à attendre** la bonne requête (boucle `accept`), au lieu d'échouer (les navigateurs demandent parfois `/favicon.ico`).
3. Timeout d'attente 120 s → 300 s.
4. `--status` (branche commune, `marker_path.exists()`) : pour Antigravity, exiger que le fichier soit du JSON contenant une chaîne non vide `refresh_token` ; sinon `not authenticated`.
5. Persister aussi `token` (= `access_token`) et `expiry` (ISO 8601, `now + expires_in`) dans `acp_token.json` ; ne rien casser si absents de la réponse.
6. Tests `#[cfg(test)]` dans `auth.rs` : `base64_url_encode` (vecteurs RFC 4648 §10 : `""`→`""`, `"f"`→`"Zg"`, `"fo"`→`"Zm8"`, `"foo"`→`"Zm9v"`, `"foob"`→`"Zm9vYg"`), `url_encode` (`"a b/c"`→`"a%20b%2Fc"`), `extract_query_param` (`GET /?code=x%2By&state=s HTTP/1.1` → `code` = `x+y`), `percent_decode`, et la vérification de `state` (fonction pure `callback_is_valid(req, expected_state) -> Result<String, String>`).
7. Docs : rien (comportement visible inchangé), sauf MANUEL 4b : délai 5 min.

**Acceptation** : tests verts ; à la main : `zevbox auth antigravity` complet réussit encore ; `zevbox auth antigravity --status` → `authenticated`.

### R6 — Mises à jour disponibles

`zevbox update --check` (2026-10-14) : Claude 0.89.1 → 0.90.0, Antigravity 1.3.0 → 1.4.0. Après R8/R3/R2 : `zevbox update` (les deux), puis re-tester à la main un thread Zed par agent (le format d'`acp_token.json` ou du modèle par défaut peut changer entre versions). Si Antigravity 1.4.0 échoue à l'auth : `zevbox rollback --agent antigravity` et consigner l'erreur ici. Pas de code.

### R7 — Divers (sans code, 15 min)

- Déplacer `exploration_solutions_gemini.md` sous `docs/` (`git mv`), mettre à jour les liens (README, PROGRESS, ce fichier). Il contient le client secret Google (public dans le binaire Antigravity, mais à savoir si le dépôt devient public).
- `lock.json` d'`antigravity-20261013-074439` a été édité à la main pour `authMarker` ; le prochain `zevbox update` (R6) régénère un lock cohérent — rien à faire d'autre.

## Idées reportées

- Racines autorisées (`config.projectRoots`) à la place de la liste blanche par projet — option (b) de R1, écartée le 2026-10-14.
