# zevbox — Plan d'implémentation (v2)

> Agents ACP (Claude, Antigravity) isolés dans des micro-VM Apple Container, utilisés depuis Zed.
> Binaire hôte en Rust. Ce document est **le contrat complet** pour l'agent IA qui va construire le projet.
> Il remplace `zevbox_plan.v1.md` (conservé pour l'historique ; ne pas l'utiliser).
> Faits vérifiés le 2026-10-13 sur la machine cible et dans les sources officielles (voir §16).
> **Un harnais exécutable est fourni** : squelette compilable, 167 tests déjà écrits et protégés, faux CLI `container`, faux agent, portes de validation (`make verify STEP=N`). Tous les tests ont été validés contre une implémentation de référence : ils sont réalisables.

---

## 0. Mode d'emploi de ce document (à lire en entier avant de coder)

1. **Ce qui est déjà fourni** (ne pas le recréer) : le squelette Rust complet (`src/`, toutes les signatures publiques, des corps qui renvoient `NotImplemented`), les tests (`tests/`), les faux outils de test (`tests/fixtures/`), les portes (`scripts/verify-step.sh`, `scripts/check-invariants.sh`), le `Makefile`, `AGENTS.md`, `docs/PROGRESS.md`, `docs/DECISIONS.md`, `assets/Containerfile`. Rust 1.99 est installé dans `/opt/homebrew/opt/rustup/bin` (le `Makefile` l'ajoute au PATH ; dans un shell : `export PATH=/opt/homebrew/opt/rustup/bin:$PATH`).
2. **Ton travail : remplacer les corps `NotImplemented` par du vrai code jusqu'à ce que les tests passent**, étape par étape (§10). Les signatures publiques sont le contrat des tests : ne pas les changer. Tu peux ajouter des fonctions privées, des modules privés et des tests supplémentaires.
3. **Suivre les étapes du §10 dans l'ordre.** Une étape est terminée quand `make verify STEP=N` affiche `STEP N: OK`. Alors seulement : marquer ✅ dans `docs/PROGRESS.md` avec la preuve, puis `git commit -m "step N: <titre>"`. La porte refuse une étape si les précédentes ne sont pas ✅.
4. **Fichiers protégés (ne jamais modifier)** : tout `tests/`, tout `scripts/`, `clippy.toml`. Leurs empreintes sont dans `tests/PROTECTED.sha256` et chaque porte les vérifie. **Un test qui échoue signifie que le code est faux, pas le test.** Si tu es convaincu qu'un test protégé est faux : arrête-toi, écris la preuve dans `docs/PROGRESS.md` et demande à l'humain (👤). Seul l'humain lance `make protect-update`.
5. **Recommencer une étape ratée** : `git restore src/ && git clean -fd src/` (jamais ailleurs), puis refaire. **Reprise après perte de contexte** : relire ce document, `AGENTS.md`, `docs/PROGRESS.md`, `docs/DECISIONS.md`, puis reprendre à la première étape non ✅.
6. **Travailler par petits incréments** : implémenter une fonction, lancer le test ciblé (`cargo test --test <fichier> <nom_du_test>`), recommencer. Ne jamais écrire plusieurs centaines de lignes sans compiler.
7. **Fait non vérifié = à vérifier, pas à supposer.** Quand ce plan dit « VÉRIFIER », exécuter la commande indiquée et consigner le résultat dans `docs/DECISIONS.md`. Le `--help` du CLI installé l'emporte sur ce plan.
8. **Étapes HUMAINES (👤)** : connexion navigateur, interface Zed, requête payante, choix du projet. L'agent s'arrête, écrit dans `docs/PROGRESS.md` les instructions exactes pour l'humain, et attend sa réponse.
9. **En cas de blocage** : ne jamais affaiblir une règle de sécurité (§3) pour faire passer un test. Consigner le blocage (symptôme, commandes essayées, hypothèses écartées) dans `docs/PROGRESS.md`.
10. **Simplicité** : ne rien ajouter qui n'est pas demandé ici (pas de daemon, de serveur HTTP, de traduction de chemins, de traits « pour plus tard », de plugin). Les idées vont dans `docs/DECISIONS.md` › « Idées reportées ».

## 1. Objectif

Un binaire unique `zevbox` (Rust, macOS arm64) qui permet de :

- choisir **Claude** ou **Antigravity** dans le panneau Agent de Zed, comme agent ACP personnalisé ;
- exécuter l'agent et **toutes ses commandes** dans une micro-VM Linux Apple Container qui ne voit **que** le projet autorisé et l'état dédié de l'agent ;
- **interposer un relais ACP filtrant** entre Zed et l'agent, pour que l'agent ne puisse pas faire lire, écrire ou exécuter quoi que ce soit sur le Mac par l'intermédiaire de Zed ;
- installer, authentifier, lister, arrêter, nettoyer, mettre à jour (avec tests et retour arrière) et ajouter des outils Linux, **avec des commandes simples**.

### Expérience utilisateur visée (doit être vraie à la fin)

```bash
cd ~/code/zevbox && make install          # compile et installe ~/.local/bin/zevbox
zevbox setup --project ~/code/myproject   # configuration guidée, images, auth, Zed
# → ouvrir le projet dans Zed, choisir « zevbox · Claude » dans le panneau Agent (amendement 2026-10-14, §8.9)
zevbox status                                         # voir ce qui tourne
zevbox stop                                           # tout arrêter (seulement les instances zevbox)
```

Toute erreur affichée à l'utilisateur dit **quoi faire** (commande exacte à lancer).

---

## 2. Faits établis (ne pas re-débattre)

Ces points corrigent le plan v1. Ils sont des exigences.

| # | Fait | Conséquence obligatoire |
|---|---|---|
| F1 | ACP permet à l'agent de demander au **client (Zed)** de lire/écrire des fichiers (`fs/*`), de créer des terminaux (`terminal/*`), de lancer un programme d'authentification « terminal » (`AuthMethod.type = "terminal"` ou `_meta["terminal-auth"]`) et d'ouvrir des URL (`elicitation/create`). Tout cela s'exécute **sur le Mac**. | Relais filtrant obligatoire (§7). |
| F2 | L'adaptateur Claude (`claude-agent-acp` 0.89.x) annonce, dans la réponse à `initialize`, des méthodes d'auth `type:"terminal"` et un `_meta["terminal-auth"]` contenant `command: process.execPath`. Zed exécuterait cette commande **sur le Mac**. Il active ces méthodes si `clientCapabilities.auth.terminal` ou `clientCapabilities._meta["terminal-auth"]` vaut `true`. | Le relais réécrit les capacités client et filtre `authMethods` (§7.3). |
| F3 | L'adaptateur Claude lit `session/new._meta.claudeCode.options` pour configurer le SDK. | Le relais supprime `_meta` des requêtes de session venant de Zed (§7.3). |
| F4 | Transport ACP stdio : **un message JSON par ligne**, délimité par `\n`, sans retour à la ligne interne. stderr = journaux libres. | Codec ligne par ligne (§7.2). Rien d'autre que du JSON-RPC sur stdout. |
| F5 | Sur macOS, Claude Code stocke ses identifiants dans le Trousseau ; sous Linux dans un fichier (`~/.claude/.credentials.json`, VÉRIFIER à l'étape 13). Monter `~/.claude` du Mac ne marche pas et serait dangereux. | État d'auth Linux dédié, monté en `$HOME` du conteneur. |
| F6 | Commande d'auth Claude dans le conteneur : `claude-agent-acp --cli auth login --claudeai` (abonnement) ; l'option `--cli` relaie vers le binaire natif Claude livré en dépendance **optionnelle** du SDK. Repli : `claude-agent-acp --cli` puis `/login`. | `npm ci` **sans** `--omit=optional`. Ne jamais définir `ANTHROPIC_API_KEY` (facturation API au lieu de l'abonnement). |
| F7 | Registre ACP : Claude = `distribution.npx.package` (ex. `@agentclientprotocol/claude-agent-acp@0.89.1`), binaire `claude-agent-acp`, Node ≥ 22. Antigravity = `distribution.binary["linux-aarch64"]` avec `archive` (zip), `cmd` (`./agy_acp_server.par`), `args` (`["--uid="]`). **Aucune empreinte n'est fournie par le registre.** | L'empreinte sha256 est calculée par zevbox au moment du verrouillage et revérifiée au build. Le `.par` est un exécutable natif, pas du Python. |
| F8 | `container` 1.5.0 est installé (`/opt/homebrew/bin/container`) et tourne. Options disponibles : `run -i -t --rm --name --label --mount type=bind,source=,target=[,readonly] -w --uid --gid -e -c/--cpus -m/--memory --init --cap-drop --read-only` ; `stop -t/--time -s` ; `delete -f` ; `list --all --format json` ; `build -f -t --target --build-arg --no-cache` ; `image list --format json`. | Utiliser ces options. Toujours parser `--format json`, jamais le tableau texte. |
| F9 | L'utilisateur a d'autres conteneurs (`buildkit`, forgejo, postgresql…) et d'autres images. | Ne jamais utiliser `--all` avec `stop`/`delete`. Ne cibler qu'un conteneur à la fois, vérifié (§8.3). |
| F10 | Les réglages Zed actifs sont dans `~/.config/zed/settings.json` (JSONC : commentaires, virgules finales ; droits `0600`). Ils contiennent déjà `agent_servers.claude-acp`, qui **ne doit pas être modifié**. Format d'un agent : `{"type":"custom","command":"…","args":[…],"env":{}}`. Journaux ACP de Zed : palette → `dev: open acp logs`. | Édition AST via `jsonc-parser` (feature `cst`). Ne toucher qu'aux clés commençant par `zevbox · `. |
| F11 | Sur la machine : Rust 1.99 installé (Homebrew `rustup`, `/opt/homebrew/opt/rustup/bin`) ; le dossier `~/code/myproject` **n'existe pas** ; UID 501, GID 20 ; macOS 26.7.1 arm64. | Le projet à autoriser est demandé à l'humain (👤), jamais supposé. |
| F14 | `container image list` affiche des noms **entièrement qualifiés** (`docker.io/library/debian:bookworm-slim`) ; une image construite `zevbox/claude:<tag>` apparaît comme `docker.io/zevbox/claude:<tag>`. | Comparer avec `container::image_matches`, jamais avec `==`. |
| F15 | Vérifié sur la vraie VM : chemins avec espace dans `--mount` OK ; `--uid 501 --gid 20` → fichiers 501:20 sur le Mac ; `--cap-drop ALL` + `--init` OK ; montages hôte visibles en `virtiofs` ; progression de `container run` sur stderr ; après `run --rm` le conteneur peut subsister un instant ; un `SIGKILL` de `zevbox launch` fait disparaître la VM (stdin fermé). | Détails dans `docs/DECISIONS.md`. |
| F12 | `--rm` supprime le conteneur après son arrêt, mais archiver un thread Zed ne garantit pas l'arrêt du processus ACP. Un processus ACP peut porter plusieurs sessions. | Ne pas promettre « 1 thread = 1 conteneur ». Fournir `status`, `stop` et `clean`, et un nettoyage automatique des instances orphelines à chaque lancement. |
| F13 | Les anciennes règles `.claude/settings.json` (`denyWrite`, `allowWrite`…) ne sont pas une protection fiable ici. | zevbox n'écrit **jamais** dans le projet de l'utilisateur. |

---

## 3. Invariants de sécurité (numérotés, testés, jamais assouplis)

Chaque invariant est couvert par au moins un test (§9) ou une vérification (`make check`, §6.4).

- **I1 — Montages minimaux.** Un conteneur zevbox ne reçoit que deux montages : le projet autorisé (même chemin absolu, lecture-écriture) et `stateRoot/state/<agent>` (monté en `/home/agent`). Jamais `$HOME`, `/Users`, `~/.claude`, `~/.gemini`, `~/.ssh`, le Trousseau, un socket, le dossier du dépôt zevbox ni `stateRoot` lui-même.
- **I2 — Projets sur liste blanche.** `--project` doit, après canonicalisation (`std::fs::canonicalize`), être **exactement** un chemin de `config.projects`. Refuser `/`, `$HOME`, un parent de `$HOME`, un chemin contenant `stateRoot` ou le dépôt zevbox, ou un chemin contenu dans ceux-ci.
- **I3 — Aucune délégation à l'hôte via ACP.** Requêtes agent→Zed `fs/*`, `terminal/*` et `elicitation/*` refusées par le relais, jamais transmises. Capacités client réécrites. Méthodes d'auth terminal supprimées.
- **I4 — Liste blanche dans les deux sens.** Une méthode absente des tables du §7.3 est refusée (requête) ou ignorée (notification), et le refus est journalisé. Les réponses dont l'`id` ne correspond à aucune requête transmise sont ignorées.
- **I5 — `cwd` verrouillé.** Toute requête portant `cwd` doit avoir `cwd` == racine du projet de l'instance (comparaison après normalisation, sans accès disque). `additionalDirectories` non vide → refus. `mcpServers` → remplacé par `[]`.
- **I6 — stdout sacré.** En mode `launch`, seul le relais écrit sur stdout, et uniquement des messages JSON-RPC. Tous les journaux vont sur stderr ou dans le fichier journal de l'instance.
- **I7 — Pas de shell sur l'hôte.** Aucun `sh -c`, `bash -c` ni concaténation de commande. Uniquement `Command::new(binaire).args([...])`. (Les scripts de test dans `tests/fixtures/` sont la seule exception.)
- **I8 — Ciblage strict des conteneurs.** Arrêt et suppression uniquement par nom exact, et seulement si le conteneur porte le label `zevbox.managed=1` **et** que son nom commence par `zevbox-`. Jamais `--all`.
- **I9 — Pas de secrets dans les journaux.** Ne jamais journaliser le contenu des messages ACP (prompts, fichiers), des tokens ou des variables d'environnement. Journaliser seulement : méthode, direction, id, décision, taille.
- **I10 — Pas de mise à jour pendant un lancement.** `launch` n'effectue aucun accès réseau de zevbox, aucun build et aucune résolution de version : il utilise la release active.
- **I11 — Pas de panic en production.** Interdire `unwrap`, `expect`, `panic!`, `todo!` et `unimplemented!` hors tests (lints Clippy, §6.3).
- **I12 — zevbox n'est pas modifiable par l'agent.** Binaire installé, configuration et releases sont hors des montages (conséquence de I1).
- **I13 — Fichiers du projet exécutés par le Mac** (amendement 2026-10-16, EXP-01, décision P1). Dans le conteneur ACP, `<projet>/.zed` est masqué par un tmpfs et un `<projet>/.git` existant est en lecture seule (options `--tmpfs` / `--read-only-path`, qui ne sont pas des montages hôte : I1 inchangé). Pendant la session, `launch` surveille le projet et arrête l'agent si un `.git` apparaît ou change ailleurs qu'à la racine existante. Motif : Zed exécute sans geste les `lsp.*.binary.path` / `context_servers` du `.zed` racine (projet approuvé) et les filtres git de tout dépôt du projet (même en Restricted Mode).

Limites assumées, à écrire telles quelles dans le README : l'agent peut modifier ou supprimer tout le projet monté ; il a accès au réseau, donc peut exfiltrer le code du projet ; il a accès à ses propres identifiants ; le contenu volontairement envoyé depuis Zed (fichiers joints, texte collé) lui est transmis ; aucune garantie contre une faille du runtime Apple Container ou du relais.

---

## 4. Architecture

```text
Zed (macOS)
  │ lance : ~/.local/bin/zevbox launch claude   (cwd = projet ; `--project` optionnel — amendement 2026-10-14, §8.9)
  ▼
zevbox launch  (processus hôte, Rust, tokio)
  ├─ 1. charge config + release active, vérifie projet (I2), runtime, image, auth
  │     └─ si échec : répond à la 1re requête de Zed par une erreur JSON-RPC lisible, puis sort (§8.5)
  ├─ 2. nettoie les instances orphelines (§8.4)
  ├─ 3. démarre : container run -i --rm --init --name zevbox-claude-<id> --label … IMAGE CMD
  ├─ 4. écrit instances/<id>.json
  ├─ 5. relaie stdin/stdout ⇄ conteneur avec la politique (§7) ; stderr du conteneur → logs/<id>.log
  └─ 6. à la fin (EOF, signal, erreur) : arrêt borné et vérifié du conteneur (§8.3), suppression de l'enregistrement
         │
         ▼
Micro-VM Linux (image zevbox/claude:<release> ou zevbox/antigravity:<release>)
  ├─ adaptateur ACP (utilisateur non root, UID/GID = ceux de l'utilisateur Mac)
  ├─ /chemin/projet  (bind, rw)
  └─ /home/agent     (bind de stateRoot/state/<agent>, rw)
```

**Même chemin pour le projet** dans Linux et dans Zed : ACP transmet des chemins absolus (`cwd`, emplacements des outils). Monter au même chemin évite toute traduction. Ce n'est pas une mesure de sécurité.

**Un seul processus, pas de daemon.** Zed lance `zevbox launch` ; ce processus vit autant que la connexion ACP.

---

## 5. Emplacements et fichiers

### 5.1 Dépôt source (ce dossier)

Racine : `~/code/zevbox`. Crate unique avec une bibliothèque (`src/lib.rs`) et un binaire mince (`src/main.rs`) : les tests de `tests/` ne peuvent appeler que la bibliothèque. 🔒 = protégé (§0.4), ✳ = fourni en squelette à compléter.

```text
zevbox/
├── Cargo.toml / Cargo.lock / rustfmt.toml / .gitignore
├── clippy.toml 🔒                # autorise unwrap seulement dans les tests
├── Makefile                      # check, verify STEP=N, test-integration, install, protect-update (humain)
├── zevbox_plan.md                # ce document
├── AGENTS.md / CLAUDE.md         # guide court pour agents IA (CLAUDE.md importe AGENTS.md)
├── README.md                     # à écrire à l'étape 17 (français)
├── docs/PROGRESS.md              # avancement + preuves (tableau lu par la porte)
├── docs/DECISIONS.md             # faits vérifiés, choix, idées reportées
├── assets/Containerfile ✳        # embarqué via include_str!
├── scripts/ 🔒
│   ├── verify-step.sh            # porte cumulative d'une étape
│   └── check-invariants.sh       # vérifications grep des invariants
├── src/ ✳
│   ├── main.rs  lib.rs  ui.rs  paths.rs (complets)  agents.rs (complet)
│   ├── cli/ (mod.rs complet : clap + dispatch ; un fichier par commande)
│   ├── config.rs  fsutil.rs  tools.rs  instances.rs  container.rs
│   ├── supervisor.rs  release.rs  zed_settings.rs
│   └── relay/ (mod.rs, policy.rs, jsonrpc.rs)
└── tests/ 🔒
    ├── common/mod.rs             # TestEnv : état temporaire, lance le vrai binaire avec le faux container
    ├── config_test.rs  relay_policy_test.rs  relay_transport_test.rs  container_test.rs
    ├── release_test.rs  zed_settings_test.rs  lifecycle_test.rs  cli_test.rs
    ├── integration_test.rs       # #[ignore] : vraie VM Apple Container (make test-integration)
    ├── PROTECTED.sha256
    └── fixtures/
        ├── fake-container        # faux CLI container (bash 3.2 compatible)
        ├── fake-agent.py         # faux agent ACP (modes normal/stubborn/crash/garbage)
        ├── container/  registry/  zed/   # JSON réels ou dérivés des formats réels
```

### 5.2 Données d'exécution

`stateRoot` = `~/Library/Application Support/zevbox` (droits `0700`). Le chemin contient une espace : vérifié le 2026-10-13, `--mount type=bind,source=<chemin avec espace>,…` fonctionne (F15).

```text
stateRoot/
├── config.json                 # configuration utilisateur (§5.3)
├── tools.json                  # paquets apt supplémentaires (§8.8)
├── state/claude/               # = /home/agent du conteneur Claude (identifiants Linux)
├── state/antigravity/          # = /home/agent du conteneur Antigravity
├── instances/<id>.json         # instances en cours
├── logs/<id>.log               # stderr du conteneur + journal du relais (garder les 30 plus récents)
├── backups/zed-settings-<horodatage>-<sha8>.json   # garder les 20 plus récents
├── releases/<release-id>/      # lock.json, contexte de build, résultat des tests
├── releases/active.json        # {"claude": {"current": "<id>", "previous": "<id>|null"}, "antigravity": {...}}
└── inventory-preexisting.json  # conteneurs présents avant zevbox (écrit par `setup` à la 1re exécution) — pour vérifier qu'ils restent intacts
```

Binaire installé : `~/.local/bin/zevbox` (copié par `make install`, hors de tout montage). Zed pointe sur ce chemin absolu, **pas** sur `target/release/` (un `cargo build` pendant une session remplacerait le binaire en cours d'utilisation).

### 5.3 `config.json` (schemaVersion 1)

Créé par `zevbox init`. Champs inconnus → erreur (`#[serde(deny_unknown_fields)]`). Toute erreur de validation dit quel champ corriger.

```json
{
  "schemaVersion": 1,
  "containerBinary": "/opt/homebrew/bin/container",
  "projects": ["/Users/<user>/code/myproject"],
  "resources": { "cpus": 4, "memory": "4G" },
  "relay": { "maxMessageBytes": 33554432 },
  "hostUid": 501,
  "hostGid": 20
}
```

- `containerBinary` : chemin absolu, détecté par `init` (recherche dans le PATH puis `/opt/homebrew/bin/container`, `/usr/local/bin/container`). Zed lance zevbox avec un PATH réduit : ne **jamais** compter sur le PATH au lancement.
- `projects` : chemins canoniques, gérés par `zevbox project add/remove`. Valeur initiale : liste vide.
- `resources` : valeurs de départ par instance ; plusieurs instances s'additionnent.
- `maxMessageBytes` : 32 Mio par défaut (les images collées dans un prompt sont encodées en base64).
- `hostUid/hostGid` : détectés par `init` (`nix::unistd::getuid/getgid`).
- Pas de champ « security » : la politique stricte n'est **pas** configurable dans cette version (moins de risques d'erreur, moins de code).

### 5.4 `agents.rs` — table statique

```rust
pub struct AgentSpec {
    pub id: &'static str,             // "claude" | "antigravity"
    pub display: &'static str,        // "Claude" | "Antigravity"
    pub build_target: &'static str,   // étape du Containerfile : "claude" | "antigravity"
    pub image_repo: &'static str,     // "zevbox/claude" | "zevbox/antigravity"
}
```

La commande de l'agent, la commande d'auth et le fichier témoin d'authentification sont stockés dans le `lock.json` de la release (§8.7), car ils dépendent de la version. Ajouter un agent = ajouter une entrée + une étape dans le Containerfile + une fonction de résolution. Pas de trait.

---

## 6. Règles de code et contrôles automatiques

### 6.1 Dépendances (`Cargo.toml`)

Versions vérifiées sur crates.io le 2026-10-13. N'en ajouter aucune autre sans la justifier dans `DECISIONS.md`.

```toml
[package]
name = "zevbox"
version = "0.1.0"
edition = "2021"
rust-version = "1.85"

[dependencies]
anyhow = "1"
thiserror = "2"
clap = { version = "4.6", features = ["derive"] }
tokio = { version = "1.53", features = ["full"] }
tokio-util = { version = "0.7", features = ["codec", "rt"] }
futures-util = { version = "0.3", features = ["sink"] }
serde = { version = "1", features = ["derive"] }
serde_json = { version = "1", features = ["preserve_order"] }
jsonc-parser = { version = "0.34", features = ["cst", "serde", "serde_json"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tempfile = "3"
uuid = { version = "1", features = ["v4"] }
sha2 = "0.10"
hex = "0.4"
nix = { version = "0.29", features = ["signal", "process", "user"] }
chrono = { version = "0.4", default-features = false, features = ["clock", "std"] }

[dev-dependencies]
tempfile = "3"

[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
todo = "deny"
unimplemented = "deny"
print_stdout = "deny"
print_stderr = "deny"
dbg_macro = "deny"
```

Remarques : `sha2 0.10` et `nix 0.29` sont choisis pour la stabilité de leur API (la 0.11 de sha2 et la 0.31 de nix fonctionnent aussi, mais leurs exemples en ligne sont moins nombreux). Le téléchargement HTTP se fait en lançant `/usr/bin/curl` via `Command` (présent sur macOS) : pas de client HTTP Rust ni de pile TLS à maintenir.

`clippy.toml` :

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
allow-panic-in-tests = true
```

`print_stdout` et `print_stderr` sont interdits partout : `ui.rs` écrit via `std::io::Write` sur `std::io::stdout()`/`stderr()`, et les journaux passent par `tracing` (configuré vers stderr ou vers un fichier).

### 6.2 Règles impératives

1. Erreurs : `thiserror` pour les erreurs typées des modules `relay`, `container` et `config` ; `anyhow` (+ `.context("…")`) dans `cli/`. Chaque erreur montrée à l'utilisateur termine par une action (`→ lancez : zevbox auth claude`).
2. Pas d'I/O bloquante dans la boucle du relais (`tokio::fs`, `tokio::process` seulement). Ailleurs (commandes humaines), `std::fs` est autorisé.
3. Écritures de fichiers d'état : toujours atomiques (`tempfile::NamedTempFile::new_in(même_dossier)` → `write_all` → `sync_all` → `persist`), en conservant les droits d'origine si le fichier existait.
4. Code, identifiants, commentaires et messages CLI en **anglais** ; README en **français**. Commentaires rares, réservés au « pourquoi » (notamment chaque règle de `policy.rs` : quel fait du §2 elle couvre).
5. Pas de `async-trait`, de macros maison, de génériques au-delà de `run_relay` (§7.1), ni de modules de plus de ~400 lignes : au-delà, découper par responsabilité.
6. Chaque fonction qui appelle `container` passe par `container.rs` (un seul endroit pour les arguments, le binaire et le parsing JSON).

### 6.3 Commandes de validation

- `make check` : fmt, clippy `-D warnings`, `cargo test`, invariants. À lancer souvent.
- `make verify STEP=N` : porte de l'étape N (§10). Cumulative : vérifie aussi les fichiers protégés, l'ordre des étapes dans `PROGRESS.md` et les tests de toutes les étapes précédentes.
- `make test-integration` : tests `#[ignore]` sur la vraie VM (image `docker.io/library/debian:bookworm-slim`, déjà présente).

`cargo test` n'utilise ni le réseau, ni le vrai `container`, ni le vrai `stateRoot`, ni les vrais réglages Zed : `paths.rs` lit `ZEVBOX_STATE_ROOT`, `ZEVBOX_ZED_SETTINGS`, `ZEVBOX_INSTALLED_BINARY`, et la config de test pointe `containerBinary` vers `tests/fixtures/fake-container`. Les tests lancent le binaire avec `PATH=/usr/bin:/bin` (comme Zed) : ne jamais dépendre du PATH.

### 6.4 `scripts/check-invariants.sh` (fourni)

Échoue si `src/` contient : un shell hôte (`Command::new("sh"|"bash"|"zsh")`, I7) ; `"--all"`/`"-a"` ailleurs que pour `container list` (I8) ; un accès à stdout hors `ui.rs` et `supervisor.rs` (I6) ; `-t`/`-it`/`--tty` hors `container.rs` ; une option `container` interdite (`--ssh`, `--publish`, `--volume`, `--detach`, `--privileged`, `--virtualization`, `--rosetta`) ; `ANTHROPIC_API_KEY` (F6) ; un dossier personnel comme source de montage (I1) ; un `allow(clippy::unwrap_used|expect_used|panic|print_*|…)` (I11) ; ou si les lints obligatoires ont disparu de `Cargo.toml`.

### 6.5 `AGENTS.md` (mainteneurs IA, ≤ 80 lignes)

Contient : but du projet en 3 lignes, carte des modules (1 ligne chacun), les invariants I1–I13 (renvoi à ce plan), « avant tout commit : `make check` », comment ajouter une méthode ACP à la liste blanche (modifier `policy.rs` + un test + une ligne dans le tableau §7.3 du plan), comment ajouter un agent.

---

## 7. Relais ACP (cœur de sécurité)

### 7.1 Structure

```rust
// relay/mod.rs
pub async fn run_relay<CR, CW, AR, AW>(
    client_in: CR,   // stdin de zevbox (venant de Zed)
    client_out: CW,  // stdout de zevbox (vers Zed)
    agent_out: AR,   // stdout du conteneur
    agent_in: AW,    // stdin du conteneur
    policy: Policy,
    shutdown: tokio_util::sync::CancellationToken, // ou un tokio::sync::watch
) -> Result<RelayEnd, RelayError>
where CR: AsyncRead + Unpin, CW: AsyncWrite + Unpin, AR: AsyncRead + Unpin, AW: AsyncWrite + Unpin;

pub enum RelayEnd { ClientEof, AgentEof, Shutdown, Fatal(String) }
```

- **Une seule tâche** avec `tokio::select!` sur les deux flux entrants et le signal d'arrêt. L'état (requêtes en attente) appartient à cette tâche : pas de `Mutex`, pas de `Arc`.
- Lecture : `FramedRead::new(reader, LinesCodec::new_with_max_length(max))` (tokio-util). Il gère les fragments, les messages collés, l'UTF-8 strict et la taille maximale. Ignorer les lignes vides. `\r` final retiré.
- Écriture : `serde_json::to_string(&value)` + `"\n"`, puis `flush()` après **chaque** message.
- `CancellationToken` vient de `tokio-util` (feature `rt`).
- Le relais est générique : les tests l'alimentent avec `tokio::io::duplex`, sans processus ni conteneur.

### 7.2 Erreurs de transport

| Situation | Action |
|---|---|
| Ligne non UTF-8, JSON invalide, ou JSON qui n'est pas un objet (y compris un tableau de batch) | journaliser (direction + taille, **pas le contenu**) ; terminer `Fatal` |
| Ligne > `maxMessageBytes` | idem `Fatal` |
| `jsonrpc` ≠ `"2.0"` | idem `Fatal` |
| EOF côté Zed | `ClientEof` |
| EOF côté agent | `AgentEof` |

« Terminer » = sortir de la boucle ; le superviseur arrête ensuite le conteneur (§8.3). Aucun panic, quel que soit l'octet reçu.

### 7.3 Politique (`policy.rs`, fonctions pures)

```rust
pub struct Policy { pub project_root: String /* canonique, sans '/' final */ }
pub struct RelayState { pending_from_client: HashSet<IdKey>, pending_from_agent: HashSet<IdKey> }
pub enum Direction { ClientToAgent, AgentToClient }
pub enum Verdict {
    Forward(serde_json::Value),          // envoyer (éventuellement modifié) au destinataire
    ReplyToSender(serde_json::Value),    // répondre à l'expéditeur, ne rien transmettre
    Drop { reason: &'static str },       // ignorer + journaliser
    Fatal { reason: &'static str },      // message invalide (§7.2) : le relais s'arrête
}
pub fn decide(policy: &Policy, state: &mut RelayState, dir: Direction, msg: serde_json::Value) -> Verdict;
```

`IdKey` = représentation canonique de l'`id` (`serde_json::to_string(&id)`), donc `1` et `"1"` restent distincts. Une requête dont l'`id` est déjà en attente dans le même sens → `ReplyToSender(erreur -32600)`.

**Classification** (`jsonrpc::classify`) : `method` + `id` = requête ; `method` sans `id` = notification ; `id` sans `method` avec exactement un de `result`/`error` = réponse ; sinon (y compris `jsonrpc` ≠ `"2.0"`, `id` ni chaîne ni nombre, non-objet) → `Verdict::Fatal`.

**Réponses** : une réponse venant de l'agent est transmise seulement si son `id` est dans `pending_from_client` (on le retire alors), sinon `Drop`. Symétrique pour Zed. Les réponses synthétiques du relais n'entrent jamais dans ces ensembles.

**Requêtes Zed → agent :**

| Méthode | Décision |
|---|---|
| `initialize` | Transmettre avec `params.clientCapabilities` **remplacé** par `{"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false,"auth":{"terminal":false}}` + le champ `session` d'origine s'il existe (capacités d'affichage). Supprimer `elicitation`, `_meta`, tout le reste. Mémoriser l'`id` pour réécrire la réponse. |
| *réponse à `initialize`* | Dans `result.authMethods` : garder uniquement les méthodes sans `type` ou avec `type == "agent"`, et supprimer leur `_meta`. Mémoriser les `id` des méthodes gardées. |
| `authenticate` | Transmettre si `params.methodId` fait partie des méthodes gardées ; sinon `-32602`. |
| `session/new`, `session/load`, `session/resume`, `session/fork` | Vérifier `cwd` (I5) ; `additionalDirectories` absent ou `[]`, sinon `-32602` ; remplacer `mcpServers` par `[]` (ou l'ajouter s'il manque pour `new`/`load`) ; supprimer `params._meta`. |
| `session/prompt`, `session/set_mode`, `session/set_model`, `session/set_config_option`, `session/list`, `session/close`, `session/delete`, `logout` | Transmettre tel quel. |
| toute autre (y compris préfixe `_`) | `ReplyToSender` avec `-32601`. |

**Notifications Zed → agent :** `session/cancel`, `$/cancel_request` → transmettre ; autres → `Drop`.

**Requêtes agent → Zed :**

| Méthode | Décision |
|---|---|
| `session/request_permission` | Transmettre (affiche un choix à l'utilisateur ; aucune action hôte). |
| `fs/read_text_file`, `fs/write_text_file`, `terminal/create`, `terminal/output`, `terminal/wait_for_exit`, `terminal/kill`, `terminal/release`, `elicitation/create` | `ReplyToSender` avec `-32601`. |
| toute autre (y compris préfixe `_`) | `ReplyToSender` avec `-32601`. |

**Notifications agent → Zed :** `session/update`, `$/cancel_request` → transmettre ; `elicitation/complete` et toutes les autres → `Drop`.

**Vérification de `cwd` (I5)**, sans accès disque : chaîne absolue (commence par `/`), aucun composant `.` ou `..`, aucun `//`, `\0` interdit, `/` final retiré ; puis égalité stricte avec `project_root`. Sinon `-32602`.

**Messages d'erreur** (exactement ce format, pour qu'ils soient compréhensibles dans Zed) :

- `-32601` : `zevbox: '<method>' is blocked by zevbox strict mode (the agent runs in an isolated Linux VM).`
- `-32602` cwd : `zevbox: this agent is bound to <root>; this session asked for <cwd>. Open <root> in Zed, or run 'zevbox project add <cwd>' in a terminal.`
- `-32602` additionalDirectories : `zevbox: extra workspace folders are not supported; open a single folder.`

**Indication d'auth :** si l'agent répond par une erreur de code `-32000` (« auth required » en ACP), le relais ajoute au message : ` → run 'zevbox auth <agent>' in a macOS terminal.`

Quand une nouvelle version d'adaptateur utilise une méthode inconnue, elle sera refusée et journalisée (`blocked method=<m> dir=<d>`). `zevbox doctor` affiche les méthodes refusées dans les 30 derniers journaux, pour que le mainteneur décide de les auditer.

---

## 8. Comportement des commandes

### 8.1 Référence CLI (contrat)

Codes de sortie : `0` succès, `1` erreur, `2` usage (clap), `3` prérequis manquant (non configuré, non authentifié, aucune release active). Toute commande sans `config.json` sort en `3` avec `zevbox is not set up. Fix: run 'zevbox setup'` sur stderr.

| Commande | Effet |
|---|---|
| `zevbox setup [--project PATH]` (conforme depuis R3, 2026-10-14) | Assistant idempotent : `init` → écrit `inventory-preexisting.json` s'il manque (noms des conteneurs non-zevbox) → `doctor` → `update` (les deux agents ; l'échec de l'un ne bloque pas l'autre) → `auth claude` → `auth antigravity` (chacune peut être passée en répondant `n`) → `project add PATH` si fourni → résumé final avec l'état de chaque agent et la prochaine action. |
| `zevbox init` | Crée `stateRoot` (0700) et ses sous-dossiers, `config.json` et `tools.json` s'ils manquent. Ne remplace jamais un fichier existant. Si le CLI `container` est introuvable, écrit `/opt/homebrew/bin/container` (et `doctor` le signale). |
| `zevbox doctor [--json]` | Liste de contrôles `OK/WARN/FAIL` + action corrective pour chacun (§8.6). Sortie 1 si au moins un `FAIL`. |
| `zevbox project add PATH [--no-zed]` / `remove PATH` / `list` | Gère `config.projects` (validation I2), puis lance `zed install` sauf `--no-zed`. `remove` arrête d'abord les instances de ce projet. |
| `zevbox zed install [--settings PATH] [--dry-run]` / `zed uninstall` | Synchronise les entrées `zevbox · …` des réglages Zed : ~~`config.projects` ×~~ une par agent ayant une release active (§8.9, amendement 2026-10-14). `--dry-run` affiche le texte avant/après des entrées concernées. |
| `zevbox auth <agent> [--status]` | Session interactive d'authentification dans le conteneur (§8.2). *Amendement 2026-10-14 (R2) : hors `--status`, fait d'abord `init` si `config.json` manque, démarre le runtime s'il est arrêté, et `update --agent <a>` si la release ou son image manque ; idempotent.* `--status` : `authenticated`/`not authenticated`/`unknown`. |
| `zevbox launch <agent> [--project PATH]` | Appelé par Zed. Ne jamais l'appeler à la main sauf pour tester. Sans `--project` (amendement 2026-10-14) : projet = répertoire de travail, que Zed fixe au premier worktree du projet ; toujours contrôlé par I2. |
| `zevbox status [--json]` | **Lecture seule.** Tableau : id, agent, projet, release, état (`running`/`orphan`/`unregistered`). Croise les enregistrements et `container list --all --format json`. `--json` : tableau d'objets `{id, agent, project, release, containerName, state}` (`id`, `project`, `release` à `null` pour `unregistered`) ; `[]` si rien. |
| `zevbox stop [ID \| --project PATH]` | Sans argument : arrête **toutes les instances zevbox enregistrées**, une par une (jamais `--all`). Affiche ce qui a été arrêté. |
| `zevbox clean [--images]` | Supprime les orphelins vérifiés (I8), les enregistrements obsolètes, les journaux au-delà de 30 et les sauvegardes au-delà de 20. `--images` : supprime les images `zevbox/*` qui ne sont ni `current` ni `previous`. Ne touche jamais `state/`, `config.json` ni les projets. |
| `zevbox shell <agent> [--project PATH]` | Shell `bash` interactif (`-it`) dans un conteneur jetable identique à celui de l'agent. `--project` par défaut = dossier courant s'il est autorisé, sinon erreur code 3 (amendement R3 : plus de repli sur le premier projet). Bannière : `zevbox Linux shell — agent=<a> project=<p> release=<r> (changes outside the project are discarded on exit)`. |
| `zevbox logs [ID] [--path]` | Affiche les 200 dernières lignes du journal de l'instance (par défaut la plus récente) ; `--path` affiche seulement le chemin. |
| `zevbox update [--check] [--agent A]` | `--check` : compare la release active aux registres et n'écrit rien. Sans `--check` : candidate → build → smoke test → promotion (§8.7). |
| `zevbox rollback [--agent A]` | Échange `current` et `previous` (vérifie que l'image `previous` existe). |
| `zevbox tools list` / `tools add <paquet-apt>…` / `tools remove <paquet>…` | Modifie `tools.json` puis reconstruit les agents (candidate → tests → promotion). |

`--agent` accepte `claude` ou `antigravity` ; sans option, la commande s'applique aux deux.

### 8.2 Arguments `container run` (construits uniquement par `container.rs`)

Mode ACP (`launch`) — vecteur exact, dans cet ordre :

```text
run -i --rm --init --progress none
  --name zevbox-<agent>-<id8>
  --label zevbox.managed=1 --label zevbox.instance=<id8> --label zevbox.agent=<agent>
  --cpus <n> --memory <m>
  --uid <hostUid> --gid <hostGid>
  --cap-drop ALL
  -e HOME=/home/agent
  --mount type=bind,source=<projet>,target=<projet>
  --mount type=bind,source=<stateRoot>/state/<agent>,target=/home/agent
  -w <projet>
  <image:release>
  <commande agent du lock.json…>
```

- `<id8>` = 8 premiers caractères hexadécimaux d'un UUID v4 ; en cas de collision avec un enregistrement existant, en tirer un autre.
- Jamais `-t`, `-p`, `--publish-socket`, `--ssh`, `--virtualization`, `-v`, ni `-e` sans valeur (qui hériterait d'une variable de l'hôte).
- Une virgule dans un chemin casserait la syntaxe `--mount` : `project add` et `init` refusent les chemins contenant `,`.
- `--cap-drop ALL` : vérifié viable sur la VM (F15) ; reconfirmer avec le vrai agent à l'étape 11 (smoke test).
- Antigravity : la commande est `cmd` + `args` du manifeste, exécutée depuis son dossier d'installation (`/opt/zevbox/antigravity/agy_acp_server.par --uid=`). VÉRIFIER à l'étape 14 la signification de `--uid=` (vide dans le registre) et consigner le résultat.

Mode interactif (`shell`, `auth`) : même vecteur, mais `-it` au lieu de `-i`, nom `zevbox-<agent>-sh-<id8>`, label `zevbox.interactive=1`, commande `bash` (shell) ou la commande d'auth du lock. Pas de relais (le terminal est branché directement : `std::process::Command` avec `stdin/stdout/stderr` hérités, puis `status()`).

### 8.3 Cycle de vie et arrêt (`supervisor.rs`)

Démarrage (`launch`) :

1. Contrôles préalables (§8.5). En cas d'échec : mode « erreur pour Zed ».
2. Nettoyage des orphelins (§8.4), en silence (journal seulement).
3. `tokio::process::Command` avec `stdin(piped)`, `stdout(piped)`, `stderr` → fichier `logs/<id>.log` (ouvert en ajout), `kill_on_drop(true)`.
4. Écrire `instances/<id>.json` : `{id, agent, project, release, image, containerName, relayPid, relayStartedAt (horodatage du processus), createdAt}`.
5. Lancer `run_relay`. Installer les gestionnaires `SIGTERM`, `SIGINT`, `SIGHUP` (tokio::signal::unix), qui déclenchent l'arrêt.

Arrêt (quelle que soit la cause : `ClientEof`, `AgentEof`, signal, `Fatal`), borné à ~12 s :

1. Fermer le stdin du conteneur (drop) ; attendre la fin du processus `container run` jusqu'à **3 s**.
2. S'il tourne encore : `container stop --time 3 <nom>` ; attendre jusqu'à **5 s**.
3. S'il tourne encore : `child.start_kill()` puis attente de 1 s.
4. Lire `container list --all --format json` : si un conteneur `<nom>` existe encore (et passe I8) → `container delete --force <nom>`.
5. Revérifier son absence ; supprimer `instances/<id>.json` ; journaliser la durée et le résultat. Code de sortie 0 si le conteneur a disparu, sinon 1 (et l'enregistrement est conservé pour `clean`).

Si Zed tue zevbox avec `SIGKILL`, rien de tout cela ne s'exécute : le pipe stdin du conteneur se ferme, l'adaptateur reçoit EOF et sort, `--rm` supprime le conteneur ; l'enregistrement orphelin est affiché par `status` et nettoyé au prochain `launch`, `stop` ou `clean`. Vérifié sur la vraie VM (test `real_sigkill_of_zevbox_still_removes_container`, étape 10).

### 8.4 Orphelins

Une instance est **orpheline** si son `relayPid` n'existe plus (`nix::sys::signal::kill(pid, None)` renvoie `ESRCH`), ou si le processus existe mais n'est pas le même (heure de démarrage différente, lue via `ps -o lstart= -p <pid>` lancé avec `Command`). Nettoyage : appliquer les étapes 4–5 de l'arrêt au conteneur nommé dans l'enregistrement (s'il passe I8), puis supprimer l'enregistrement.

Un conteneur `zevbox-*` **avec** label `zevbox.managed=1` mais **sans** enregistrement est signalé par `doctor` et `status` (`unregistered`). Seul `clean` le supprime, après avoir affiché son nom.

### 8.5 Contrôles préalables de `launch` et erreurs visibles dans Zed

Ordre : config présente (sinon `zevbox is not set up. Fix: run 'zevbox setup'`) et lisible → projet autorisé (I2) → agent connu → release active et image présente (`container image list --format json`, comparée avec `image_matches`, F14) → runtime en marche (`container system status` ; s'il est arrêté : `container system start`, puis revérifier une fois) → témoin d'authentification présent.

En cas d'échec, zevbox ne démarre pas de conteneur. Il lit la première ligne de Zed. Si c'est une requête, il répond `{"jsonrpc":"2.0","id":<id>,"error":{"code":-32000,"message":"zevbox: <problème>. Fix: <commande exacte>"}}`, puis il sort avec le code 3. Il écrit le même message sur stderr. Exemples :

- `zevbox: Claude is not authenticated. Fix: run 'zevbox auth claude' in a macOS terminal.`
- `zevbox: /x/y is not an allowed project. Fix: run 'zevbox project add /x/y'.`
- `zevbox: no active Claude image. Fix: run 'zevbox update --agent claude'.`

VÉRIFIER à l'étape 16 (👤) comment Zed affiche ce message, et le consigner dans le README.

### 8.6 `doctor` — contrôles

1. `containerBinary` exécutable ; version ≥ 1.5.0.
2. `container system status` = running (sinon `WARN` + `container system start`).
3. Architecture `arm64`.
4. `config.json` valide ; chaque projet existe et est canonique.
5. Pour chaque agent : release active, image présente, témoin d'auth (`--status`).
6. Droits `0700` sur `stateRoot` et `state/*`.
7. `~/.local/bin/zevbox` existe ; c'est le binaire référencé par les entrées Zed.
8. Les entrées Zed `zevbox · …` correspondent ~~à `config.projects`~~ aux agents ayant une release active (amendement 2026-10-14 ; sinon `WARN` → `zevbox zed install`).
9. Instances orphelines ou non enregistrées.
10. Méthodes ACP refusées récemment (lues dans les journaux).
11. Les conteneurs de `inventory-preexisting.json` existent toujours (sinon `WARN`, information seulement).

### 8.7 Releases, mises à jour, retour arrière (`release.rs`)

Une **release** = un agent + des versions figées + une image construite et testée. `release-id` = `<agent>-<AAAAMMJJ-HHMMSS>`. Image : `zevbox/<agent>:<release-id>`.

`releases/<release-id>/lock.json` :

```json
{
  "schemaVersion": 1,
  "releaseId": "claude-20261013-101500",
  "agent": "claude",
  "createdAt": "2026-10-13T10:15:00Z",
  "registryManifestUrl": "https://raw.githubusercontent.com/agentclientprotocol/registry/main/claude-acp/agent.json",
  "registryManifestSha256": "…",
  "agentVersion": "0.89.1",
  "baseImage": "node:22-bookworm-slim@sha256:…",
  "aptPackages": ["ripgrep"],
  "npmPackage": "@agentclientprotocol/claude-agent-acp@0.89.1",
  "archiveUrl": null,
  "archiveSha256": null,
  "agentCommand": ["/opt/zevbox/claude/node_modules/.bin/claude-agent-acp"],
  "authCommand": ["/opt/zevbox/claude/node_modules/.bin/claude-agent-acp", "--cli", "auth", "login", "--claudeai"],
  "authMarker": ".claude/.credentials.json",
  "image": "zevbox/claude:claude-20261013-101500",
  "smokeTest": { "passed": true, "at": "…", "details": "…" }
}
```

Pipeline `update` (et `tools add/remove`, qui réutilisent les versions actuelles au lieu de résoudre) :

1. **Résoudre** : `curl --fail --silent --show-error --location --max-time 60 -o <fichier> <url>` du manifeste. Valider : `id` attendu (`claude-acp` / `antigravity-acp`), présence de `distribution.npx.package` (Claude) ou de `distribution.binary["linux-aarch64"]` avec `archive` et `cmd` (Antigravity). Champ manquant → erreur explicite `registry schema changed: <champ>` ; l'agent est déclaré indisponible et la release active reste en place. Ne jamais fabriquer une URL à partir d'un numéro de version. Ne jamais utiliser `preview`.
2. **Base** : `baseImage` = `node:22-bookworm-slim` figée par digest. VÉRIFIER à l'étape 11 comment obtenir le digest avec le CLI (`container image pull` puis `container image inspect` / `image list --format json`) et le consigner.
3. **Contexte de build** dans `releases/<id>/context/` : `Containerfile` (depuis `include_str!("../assets/Containerfile")`), `zevbox-release.json` (le lock sans `smokeTest`), et selon l'agent :
   - Claude : `package.json` minimal `{"private":true,"dependencies":{"@agentclientprotocol/claude-agent-acp":"<version exacte>"}}` + `package-lock.json`, généré en lançant `container run --rm --mount type=bind,source=<context>,target=/w -w /w <baseImage> npm install --package-lock-only --ignore-scripts`. Le lock est conservé dans la release.
   - Antigravity : télécharger l'archive dans `releases/<id>/context/agy.zip`, calculer son sha256 et l'écrire dans `archiveSha256`.
4. **Build** : `container build -f <context>/Containerfile --target <agent> --tag zevbox/<agent>:<id> --build-arg BASE_IMAGE=<baseImage> --build-arg ZEVBOX_UID=<uid> --build-arg ZEVBOX_GID=<gid> --build-arg APT_PACKAGES="<paquets validés>" [--build-arg AGY_SHA256=<sha>] <context>`. stdout et stderr du build → `releases/<id>/build.log`.
5. **Smoke test** (sans quota, sans compte) :
   - `container run --rm <image> uname -s` = `Linux` ;
   - pour chaque paquet de `tools.json` : `container run --rm <image> dpkg -s <paquet>` réussit ;
   - `container run --rm <image> cat /etc/zevbox-release.json` contient `releaseId` ;
   - ACP : démarrer le vrai chemin `launch` (même code) avec un projet temporaire de test (`tempfile::tempdir()` dans `$TMPDIR`, canonicalisé, ajouté aux projets autorisés **en mémoire seulement**, supprimé ensuite), envoyer `initialize` (`protocolVersion: 1`), exiger une réponse `result` avec le même `id` sous **60 s**, vérifier que la réponse réécrite ne contient aucune méthode d'auth `terminal`, fermer stdin, vérifier que le conteneur a disparu sous 15 s.
6. **Promotion** seulement si tout passe : `active.json[agent] = {current: <id>, previous: <ancien current>}` (écriture atomique). Sinon : release marquée `failed` dans son lock, image candidate supprimée (`container image delete`, seulement ce tag exact), release active inchangée, message clair + chemin de `build.log`.
7. Après promotion : `zed install` si de nouveaux agents deviennent disponibles.

`rollback` : refuse si `previous` est nul ou si son image n'existe plus ; sinon échange `current` et `previous`. Les instances en cours ne sont pas touchées ; le prochain lancement utilise la release restaurée.

### 8.8 `assets/Containerfile` (squelette à compléter et vérifier)

```dockerfile
ARG BASE_IMAGE
FROM ${BASE_IMAGE} AS base
ARG ZEVBOX_UID
ARG ZEVBOX_GID
ARG APT_PACKAGES=""
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates git curl jq unzip python3 procps less ${APT_PACKAGES} \
 && rm -rf /var/lib/apt/lists/*
RUN (getent group ${ZEVBOX_GID} || groupadd -g ${ZEVBOX_GID} agent) \
 && useradd -u ${ZEVBOX_UID} -g ${ZEVBOX_GID} -M -d /home/agent -s /bin/bash agent \
 && mkdir -p /home/agent && chown ${ZEVBOX_UID}:${ZEVBOX_GID} /home/agent
COPY zevbox-release.json /etc/zevbox-release.json

FROM base AS claude
COPY package.json package-lock.json /opt/zevbox/claude/
RUN cd /opt/zevbox/claude && npm ci \
 && test -x node_modules/.bin/claude-agent-acp

FROM base AS antigravity
ARG AGY_SHA256
COPY agy.zip /tmp/agy.zip
RUN echo "${AGY_SHA256}  /tmp/agy.zip" | sha256sum -c - \
 && mkdir -p /opt/zevbox/antigravity && unzip -q /tmp/agy.zip -d /opt/zevbox/antigravity \
 && rm /tmp/agy.zip && chmod +x /opt/zevbox/antigravity/agy_acp_server.par
```

- `BASE_IMAGE` est passé en `--build-arg BASE_IMAGE=<baseImage du lock>` (§8.7, point 4).
- Rien n'est installé dans `/home/agent` (il est masqué par le montage d'état).
- `APT_PACKAGES` : chaque nom est validé par zevbox avec `^[a-z0-9][a-z0-9+.-]{0,62}$` avant d'être passé. C'est la seule chose qui empêche une injection dans le `RUN`.
- Pas de `npm install …@latest`. Pas de `--omit=optional` (F6).
- VÉRIFIER : la structure réelle du zip Antigravity (le binaire peut être dans un sous-dossier, et d'autres fichiers peuvent être nécessaires) ; adapter le chemin et le consigner.

`tools.json` : `{"schemaVersion":1,"apt":["ripgrep"]}`. `tools add` refuse les doublons et les noms invalides. L'ajout d'un outil ne crée jamais de montage.

### 8.9 Réglages Zed (`zed_settings.rs`)

> **Amendement 2026-10-14 (DECISIONS R1).** `agent_servers` est un réglage global de Zed, sans filtrage par projet (`agent_server_store.rs` lit `settings.get(None)`) : une entrée par (projet, agent) affichait toutes les entrées dans tous les projets. Désormais **une entrée par agent ayant une release active**, indépendante de `config.projects`. Zed lance l'agent avec `current_dir` = premier worktree du projet (`acp/transport.rs::spawn_stdio`), `launch` en déduit le projet et applique I2/I5 comme avant. Ancien texte barré ci-dessous.

Entrée gérée, une par ~~(projet,~~ agent ayant une release active~~)~~ :

```json
"zevbox · Claude": {
  "type": "custom",
  "command": "/Users/<user>/.local/bin/zevbox",
  "args": ["launch", "claude"],
  "env": {}
}
```

- Nom : `zevbox · <Display>`. ~~`zevbox · <Display> · <basename>` ; si deux projets ont le même basename : `zevbox · <Display> · <parent>/<basename>`.~~ Les anciennes entrées `zevbox · <Display> · <projet>` sont gérées (préfixe) donc remplacées par `zed install`.
- **Clés gérées (amendement 2026-10-14, DECISIONS R8)** : dans une entrée `zevbox · …` existante, seules `type`, `command`, `args`, `env` sont comparées et réécrites ; toute autre clé (notamment `default_config_options`, que Zed écrit quand l'utilisateur choisit modèle/mode) est préservée. Une entrée créée n'a que ces quatre clés (la copie des options de l'entrée registre `claude-acp` est abandonnée : elle contredit `install_semantics_match_expected`).
- **Propriété** : une clé est gérée par zevbox si et seulement si elle commence par `zevbox · `. Les autres clés (ex. `claude-acp`) ne sont jamais lues, modifiées, réordonnées ni reformatées.
- Algorithme `install` : lire le texte → sha256 → sauvegarde dans `backups/` → parser avec `jsonc_parser::cst::CstRootNode::parse` → obtenir ou créer l'objet racine puis `agent_servers` → supprimer les clés gérées obsolètes → mettre à jour ou ajouter les clés attendues → texte final → re-parser pour valider → **relire le fichier et comparer son sha256 à celui du début** (s'il a changé : abandonner, avec le message « settings changed during update, retry ») → écriture atomique **en conservant les droits d'origine** (`0600` ici).
- Si le texte final est identique à l'original : ne rien écrire, ne pas créer de sauvegarde, afficher `already up to date`.
- `uninstall` : supprime toutes les clés gérées ; laisse `agent_servers` même s'il devient vide.
- Fichier absent : le créer avec `{ "agent_servers": { … } }`.
- VÉRIFIER l'API exacte de `jsonc-parser` 0.34 (feature `cst`) sur docs.rs avant de coder, en écrivant d'abord un petit test d'essai (insertion d'une clé dans un objet imbriqué avec commentaires). Si l'API ne permet pas l'édition attendue, l'écrire dans `DECISIONS.md` et s'arrêter (👤) : ne pas passer à un éditeur maison ni à `serde_json`.

---

## 9. Tests fournis (protégés)

Ils existent déjà. **Ne pas les écrire, les faire passer.** Lire le test avant d'implémenter : son nom et ses assertions disent exactement ce qui est attendu.

| Fichier | Tests | Ce qui est vérifié | Étape |
|---|---|---|---|
| `config_test.rs` | 22 | config.json (champs inconnus, chemins, ressources), règles de projet I2 (symlinks, `$HOME`, `/`, virgule, stateRoot), noms apt, `tools.json`, `write_atomic` (droits), `ensure_private_dir`, `prune_oldest`, sha256 | 3 |
| `relay_policy_test.rs` | 44 | toute la table §7.3 : blocage fs/terminal/elicitation/extensions, réécriture des capacités, suppression des auth terminal, `cwd` (traversée, préfixe, relatif, `//`, `.`, NUL), `additionalDirectories`, MCP, routage par `id`, doublons, textes exacts | 4 |
| `relay_transport_test.rs` | 18 | fragments, messages collés, CRLF, lignes vides, JSON invalide, non-UTF-8, taille max, batch, EOF des deux côtés, arrêt, aucune fuite de contenu, fuzz 1 000 entrées | 5 |
| `container_test.rs` | 12 | vecteur `container run` exact, mode interactif, options interdites, parsing JSON réel (list/image/system), `image_matches`, règles de nom, ciblage I8, refus sans exécution | 6 |
| `release_test.rs` | 10 | manifestes réels du registre et variantes cassées, format d'id, `promote`/`rollback`, `active.json` | 7 |
| `zed_settings_test.rs` | 23 | noms des entrées, préservation des commentaires et de `claude-acp` à l'octet, idempotence, entrées obsolètes, désinstallation, fichier absent/invalide, modification concurrente, droits 0600/0644, sauvegarde | 8 |
| `lifecycle_test.rs` | 22 | bout en bout avec le vrai binaire : réécritures vues par l'agent, aucune requête hôte reçue par Zed, arguments exacts, enregistrement d'instance, arrêt sur EOF/agent têtu/SIGTERM/crash/sortie invalide, erreurs préalables visibles dans Zed (5 cas), noms distincts, `stop` ne touche que zevbox, orphelins, non-enregistrés, jamais `--all` ni TTY | 9 |
| `cli_test.rs` | 8 | `init` (droits, idempotence), `project add/list/remove` (+ refus dangereux, mise à jour Zed), `zed install --dry-run/install/uninstall`, codes de sortie 2 et 3 | 9 |
| `integration_test.rs` | 8 (`#[ignore]`) | vraie VM : témoin hors projet invisible et intact, écriture projet avec l'UID hôte, symlink non suivi, seulement 2 montages virtiofs, `$HOME` = état dédié, `--cap-drop ALL` viable, EOF et **SIGKILL** de zevbox → VM supprimée, conteneurs préexistants intacts | 10 |

Faux outils : `fake-container` reproduit les formats JSON de `container` 1.5.0 et journalise chaque appel dans `$FAKE_CONTAINER_LOG` ; `run` exécute `fake-agent.py` (le PID du conteneur = celui de l'agent). `fake-agent.py` se comporte comme l'adaptateur Claude (méthodes d'auth terminal, demande `fs/read_text_file`, `terminal/create` et `session/request_permission` pendant un prompt) et journalise tout ce qu'il reçoit dans `$FAKE_AGENT_LOG`.

Tests ajoutés par l'agent : bienvenus (`#[cfg(test)]` dans `src/` ou nouveaux fichiers dans `tests/`), surtout pour `update`, `tools`, `doctor`, `auth`, `shell`, qui ne sont pas couverts par le harnais.

## 10. Étapes de réalisation (ordre strict)

Chaque étape : implémenter → `make verify STEP=N` → `STEP N: OK` → ✅ + preuve dans `docs/PROGRESS.md` → commit.

### Étape 0 — Chaîne d'outils et git
- Rust est déjà installé (F11). `git init`, puis premier commit « harness » de tout le dossier (base de retour arrière).
- Porte : `make verify STEP=0`.

### Étape 1 — 👤 Inventaire non destructif
- Relire `docs/DECISIONS.md` (inventaire déjà fait le 2026-10-13). Revérifier : `container --version`, `container system status --format json`, `container list --all --format json` (noms et états inchangés).
- 👤 Demander à l'humain le chemin exact du premier projet à autoriser (F11) et la version de Zed. Consigner.
- Porte : `make verify STEP=1` (vérifie seulement l'étape 0).

### Étape 2 — Squelette (fourni, déjà ✅)
- Porte : `make verify STEP=2`.

### Étape 3 — `config.rs`, `fsutil.rs`, `tools.rs` (fournie comme exemple, déjà ✅)
- Lire ces trois fichiers avant l'étape 4 : ils fixent le style attendu (erreurs typées, messages actionnables, commentaires citant §/invariants, pas d'unwrap).
- Porte : `make verify STEP=3` (`config_test`).

### Étape 4 — `relay/jsonrpc.rs::classify`, `relay/policy.rs::decide` (fournie comme exemple, déjà ✅)
- Fonctions pures. Commencer par `classify`, puis les cas `Response`, puis `Notification`, puis `Request` par direction. Chaque règle a un commentaire qui cite le fait (§2) ou l'invariant (§3).
- Porte : `make verify STEP=4` (`relay_policy_test`).

### Étape 5 — `relay/mod.rs::run_relay`
- Suivre §7.1 (`FramedRead` + `LinesCodec::new_with_max_length`, une seule tâche, `tokio::select!`, flush après chaque message).
- Porte : `make verify STEP=5` (`relay_transport_test`).

### Étape 6 — `container.rs` (fournie comme exemple, déjà ✅)
- Fonctions pures, puis `ContainerCli` (`tokio::process::Command`, stdin null, sortie capturée, erreur `Failed` avec stderr). `stop`/`delete_force` vérifient le nom **avant** toute exécution.
- Porte : `make verify STEP=6` (`container_test`).

### Étape 7 — `release.rs` (sans réseau)
- Parsing des manifestes, `active.json`, `load_lock`, `active_lock`, `promote`, `rollback`, `new_release_id`.
- Porte : `make verify STEP=7` (`release_test`).

### Étape 8 — `zed_settings.rs` (fournie comme exemple, déjà ✅)
- Utiliser l'API CST listée dans `docs/DECISIONS.md` (vérifiée). Laisser une entrée gérée intacte si sa valeur est déjà identique (idempotence octet pour octet) ; sinon `set_value` ; supprimer les clés gérées absentes ; `append` les nouvelles.
- Porte : `make verify STEP=8` (`zed_settings_test`).

### Étape 9 — `instances.rs`, `supervisor.rs`, CLI `init`, `project`, `zed`, `status`, `stop`, `clean`, `launch`
- Suivre §8.3–§8.5 à la lettre. Erreur préalable : lire la 1re ligne de stdin, répondre à son `id`, sortir en 3. `stop`/`clean` : `stop_and_delete_verified`. Règles de projet : `home` = `paths::home_dir()`, `forbidden` = `[stateRoot, env!("CARGO_MANIFEST_DIR"), dossier de paths::installed_binary()]`.
- Piège tokio : voir §13.11.
- Porte : `make verify STEP=9` (`lifecycle_test`, `cli_test`, puis tout `cargo test`).

### Étape 10 — Tests réels Apple Container
- Rien de nouveau à coder normalement : si un test échoue ici mais pas à l'étape 9, c'est une différence entre le faux et le vrai `container` → consigner dans `DECISIONS.md` et corriger le code (pas le test).
- Porte : `make verify STEP=10` (ajoute `make test-integration`).

### Étape 11 — `update`, `rollback` (réseau, vrai build)
- Implémenter §8.7 (résolution via `/usr/bin/curl`, contexte de build, `container build --tag`, smoke test, promotion). VÉRIFIER le digest de `node:22-bookworm-slim` et le consigner. Ajouter des tests pour la logique pure (validation du contexte, échec → release active inchangée).
- Preuves manuelles : `zevbox update --check` ; `zevbox update --agent claude` → release promue ; `container run --rm <image> uname -s` = Linux ; mise à jour volontairement cassée → release active inchangée ; 2e release puis `zevbox rollback`.
- Porte : `make verify STEP=11` + preuves dans `PROGRESS.md`.

### Étape 12 — `tools list/add/remove`
- Preuves : `zevbox tools add ripgrep` → nouvelle release ; `container run --rm <image> rg --version` ; aucun nouveau montage.

### Étape 13 — 👤 Authentification Claude
- `zevbox auth claude` lance `authCommand` en `-it`. L'humain se connecte. VÉRIFIER le fichier créé sous `state/claude/` → confirmer `authMarker` (F5). Nouveau conteneur → `zevbox auth claude --status` = authenticated. `grep` de fragments du token dans `logs/` et le dépôt → rien. 👤 Requête réelle (quota) seulement avec accord.

### Étape 14 — 👤 Antigravity
- `zevbox update --agent antigravity`. VÉRIFIER zip, exécution Linux arm64, `--uid=`, authentification. Sans méthode compatible : ⛔ avec la raison, pas d'entrée Zed, Claude reste fonctionnel.

### Étape 15 — `doctor`, `setup`, `shell`, `logs`, `auth --status`
- Preuves : `doctor` tout OK (ou WARN expliqués) ; `setup` relancé ne change rien et le dit ; `shell` affiche la bannière ; `logs` affiche la dernière instance.

### Étape 16 — 👤 Intégration Zed réelle
- `make install`, `zevbox zed install` sur le vrai fichier (sauvegarde vérifiée). L'humain teste dans Zed : lecture, modification bénigne, `uname -s; cat /etc/os-release; cat /etc/zevbox-release.json; pwd`. Consigner archivage, changement de thread, 2e thread, fermeture de fenêtre, sortie de Zed, `zevbox stop` (conteneur encore là ?). Autre dossier → message `-32602`. Témoin d'auth renommé → message §8.5 visible ? `dev: open acp logs` : aucun `fs/*`/`terminal/*`.

### Étape 17 — README, acceptation finale
- README (français) : installation en 3 commandes, usage quotidien, référence des commandes réellement implémentées, §11, comportements Zed observés, limites (§3), dépannage par message d'erreur.
- Porte : `make verify STEP=17` (vérifie aussi qu'il ne reste aucun `NotImplemented`). Cocher §12 avec preuves.

## 11. Ce qui persiste et ce qui disparaît

| Élément | Après la fin d'une instance |
|---|---|
| Processus, mémoire, fichiers Linux hors montages | disparaissent |
| Fichiers modifiés dans le projet | persistent sur le Mac |
| Identifiants et état de l'agent (`state/<agent>`) | persistent |
| Outils de l'image (`tools.json`) | disponibles au prochain lancement |
| Outils installés à la main dans un conteneur (`apt install` dans `shell`) | perdus — utiliser `zevbox tools add` |
| Images `current`/`previous`, journaux, sauvegardes | restent jusqu'à `zevbox clean [--images]` |

Limite connue : deux instances du même agent partagent `state/<agent>` (même compte). Un rafraîchissement simultané du token pourrait entrer en conflit ; si ça se produit, relancer `zevbox auth <agent>`.

---

## 12. Critères d'acceptation finaux (tous requis, chacun avec une preuve dans `PROGRESS.md`)

- [ ] `make check` vert (fmt, clippy `-D warnings` avec les lints du §6.1, tests, invariants).
- [ ] `make test-integration` vert.
- [ ] `make verify STEP=17` affiche `STEP 17: OK` ; `tests/PROTECTED.sha256` intact.
- [ ] Claude : opérationnel dans Zed (étape 16), avec authentification persistante (étape 13).
- [ ] Antigravity : opérationnel **ou** bloqué/incompatible, avec la raison et la preuve.
- [ ] Les journaux ACP de Zed ne montrent aucune requête `fs/*` ni `terminal/*` reçue par Zed, et aucune méthode d'auth terminal.
- [ ] Témoins hors projet intacts ; seuls deux montages (étape 10).
- [ ] `SIGKILL`, EOF, `SIGTERM`, plantage de l'agent : aucun conteneur zevbox restant après 15 s ou après le `status` suivant.
- [ ] Mise à jour ratée (release active inchangée), mise à jour réussie, `rollback` : testés.
- [ ] `tools add` : reconstruction testée, aucun nouveau montage.
- [ ] Réglages Zed : sauvegarde présente, `claude-acp` et tous les commentaires intacts, réinstallation idempotente.
- [ ] Conteneurs préexistants de l'utilisateur intacts (comparaison avec `inventory-preexisting.json`).
- [ ] `zevbox setup` sur une installation existante ne change rien et le dit.
- [ ] README et AGENTS.md à jour avec les commandes réellement implémentées et les comportements réellement observés.

---

## 13. Pièges connus (lire avant de coder)

1. **Les tests d'intégration ne voient que la bibliothèque** : toute logique testée doit être dans `src/lib.rs` et ses modules, pas dans `main.rs`.
2. **Filtres `cargo test`** : `cargo test --test relay_policy_test` exécute un fichier de `tests/` ; `cargo test policy` filtre par nom. `cargo test tests::x` ne fonctionne pas pour les fichiers de `tests/`.
3. **Zed a un PATH minimal** : chemins absolus partout (`containerBinary`, `command` des entrées Zed).
4. **`container run` ne transmet pas l'environnement de l'hôte**, sauf `-e CLE` sans valeur (interdit ici).
5. **`/home/agent` est masqué par le montage d'état** : n'y rien installer dans l'image.
6. **`kill_on_drop(true)` ne suffit pas** à arrêter la VM : le processus client `container` peut mourir sans que la VM s'arrête. Toujours suivre la procédure du §8.3 et vérifier avec `list --format json`.
7. **Ne pas lire les messages ACP pour les journaliser** (I9), même en mode debug. Pour diagnostiquer, utiliser `dev: open acp logs` dans Zed.
8. **Un `--help` du CLI installé l'emporte sur ce plan** si les deux divergent : consigner l'écart dans `DECISIONS.md` et adapter le code.
9. **Les réponses ACP se transmettent par `id`, pas par méthode** : sans le suivi des `id` en attente (§7.3), un agent pourrait fabriquer de fausses réponses.
10. **Le registre ne donne pas d'empreinte** : le sha256 calculé à la résolution fait foi pour les builds suivants de cette release (confiance au premier téléchargement — le dire dans le README).

---

11. **tokio et stdin** : `tokio::io::stdin()` lit dans un thread bloquant ; à la fin, la destruction du runtime attendrait ce thread indéfiniment (Zed garde stdin ouvert après un SIGTERM). Dans `cli::run`, terminer par `runtime.shutdown_timeout(Duration::from_millis(200))` après `block_on`.
12. **Noms d'image** : toujours `container::image_matches` (F14).
13. **`--rm` est asynchrone** : après la sortie de `container run`, lister puis `delete --force` si besoin (§8.3, étapes 4–5).
14. **Clippy dans les tests** : `allow-unwrap-in-tests` ne couvre pas les fonctions d'aide des fichiers `tests/` ; les fichiers fournis ont `#![allow(clippy::unwrap_used, …)]` en tête. Dans `src/`, jamais d'`allow` de ces lints (vérifié par `check-invariants.sh`).
15. **bash de macOS = 3.2** : pas de tableaux vides sous `set -u` (concerne uniquement les fixtures, déjà corrigées).
16. **`ps -o lstart=`** donne l'heure de démarrage d'un processus (garde contre la réutilisation de PID, §8.4) ; `nix::sys::signal::kill(pid, None)` teste l'existence.

## 14. Hors périmètre de la v1 (ne pas implémenter)

Traduction de chemins / montage `/workspace` ; plusieurs dossiers par session ; serveurs MCP ; politique configurable ou mode « permissif » ; filtrage réseau ; daemon ou conteneurs persistants ; support Intel ou Linux hôte ; interface graphique ; versions `preview` des agents.

---

## 15. Glossaire

- **ACP** : Agent Client Protocol, JSON-RPC 2.0 sur stdio entre un éditeur (client) et un agent.
- **Client** : Zed. **Agent** : l'adaptateur ACP dans le conteneur.
- **Relais** : `zevbox launch`, intercalé entre les deux, qui applique la politique.
- **Release** : version figée et testée de l'image d'un agent.
- **Instance** : une exécution de `zevbox launch` (un conteneur).
- **Orphelin** : instance enregistrée dont le relais n'existe plus.

---

## 16. Sources (vérifiées le 2026-10-13)

- ACP schéma (méthodes, `ClientCapabilities`, `AuthMethod`, `session/*`) : https://agentclientprotocol.com/protocol/schema
- ACP transport stdio (délimitation `\n`) : https://agentclientprotocol.com/protocol/transports
- ACP système de fichiers client : https://agentclientprotocol.com/protocol/v1/file-system
- Registre Claude : https://github.com/agentclientprotocol/registry/blob/main/claude-acp/agent.json
- Registre Antigravity : https://github.com/agentclientprotocol/registry/blob/main/antigravity-acp/agent.json
- Adaptateur Claude (auth terminal, `--cli`, `_meta.claudeCode`) : https://github.com/agentclientprotocol/claude-agent-acp (`src/acp-agent.ts`, `src/index.ts`, `package.json`)
- Authentification Claude Code : https://code.claude.com/docs/en/authentication
- Zed agents externes : https://zed.dev/docs/ai/external-agents
- Zed JSONC : https://zed.dev/docs/languages/json
- Apple container 1.5.0 : https://github.com/apple/container/blob/1.5.0/docs/command-reference.md et `container <cmd> --help` sur la machine
- jsonc-parser (feature `cst`) : https://docs.rs/jsonc-parser
