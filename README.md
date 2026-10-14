# zevbox — Sandbox micro-VM Apple Container pour agents ACP sous Zed

`zevbox` permet d'exécuter des agents ACP (comme Claude Code) pour l'éditeur **Zed** à l'intérieur de micro-VM Linux **Apple Container** légères et isolées, interposant un relais ACP filtrant pour garantir que l'agent ne peut ni lire, ni modifier, ni exécuter quoi que ce soit sur votre Mac hôte via l'éditeur.

Binaire Rust autonome pour macOS Apple Silicon (arm64).

---

## 1. Installation en 4 commandes

```bash
# 1. Compiler et installer le binaire de production dans ~/.local/bin/zevbox
make install

# 2. Initialiser zevbox (dossiers d'état 0700, config.json, images des agents)
zevbox setup
zevbox update            # construit les images Claude et Antigravity (une fois, puis à chaque mise à jour)

# 3. Authentifier les agents, une seule fois (chaque commande imprime l'URL de connexion)
zevbox auth claude        # compte Claude (Pro/Max) — s'exécute dans un conteneur -it
zevbox auth antigravity   # compte Google (AI Pro / One AI Premium) — OAuth PKCE sur le Mac

# 4. Autoriser un projet (à répéter pour chaque projet que vous voulez ouvrir avec un agent)
zevbox project add /chemin/vers/votre/projet
```

Zed affiche deux entrées, **`zevbox · Claude`** et **`zevbox · Antigravity`**, valables dans tout projet autorisé par `zevbox project add` (invariant I2 : seuls les projets explicitement autorisés sont montés dans une micro-VM ; dans un autre dossier, l'agent répond `… is not an allowed project`). L'ordre des étapes 3 et 4 est libre ; `zevbox setup /chemin` fait les étapes 2 (sans `update`) et 4 d'un coup.

> Les deux commandes `auth` impriment l'URL de connexion : si le navigateur ne s'ouvre pas tout seul, copiez-la dans votre navigateur.

---

## 2. Usage quotidien

- **Dans Zed** : Ouvrez un projet autorisé. Dans le panneau de discussion / agent, sélectionnez **zevbox · Claude** ou **zevbox · Antigravity**. Le projet est celui ouvert dans Zed (son premier dossier racine). Pour un dossier pas encore autorisé, `zevbox project add /ce/dossier` suffit, sans redémarrer Zed. L'agent s'exécute dans une VM Linux temporaire dédiée, avec votre code source monté avec les mêmes droits UID/GID que sur votre Mac.
- **Ajouter un projet supplémentaire** :
  ```bash
  zevbox project add /autre/projet
  ```
- **Lancer un shell interactif dans la VM de l'agent** :
  ```bash
  zevbox shell claude
  ```
- **Ajouter des outils Linux (paquets apt) dans les VM** :
  ```bash
  zevbox tools add ripgrep clang
  ```
- **Vérifier l'état et diagnostiquer l'environnement** :
  ```bash
  zevbox status
  zevbox doctor
  ```
- **Arrêter toutes les instances en cours** :
  ```bash
  zevbox stop
  ```

---

## 3. Référence des commandes CLI

| Commande | Description |
|---|---|
| `zevbox setup [PROJET]` | Initialisation complète guidée et idempotente : installe la configuration, enregistre le projet, construit les releases manquantes (`update`), propose `auth` pour chaque agent non authentifié (seulement dans un terminal), configure Zed, lance `doctor` et affiche un résumé par agent. |
| `zevbox init` | Crée les répertoires d'état (`~/.local/share/zevbox` ou Application Support) en droits `0700`, ainsi que `config.json` et `tools.json`. |
| `zevbox doctor [--json]` | Contrôle complet du système : binaire `container` ≥ 1.5.0, runtime en cours, architecture arm64, validité des projets, état des releases, droits `0700`. |
| `zevbox project add <chemin>` | Valide canoniquement le chemin (interdit la racine, le home direct ou les dossiers système) et l'ajoute à la liste des projets autorisés (Invariant I2), puis synchronise Zed. |
| `zevbox project remove <chemin>` | Retire un projet de la liste autorisée et met à jour Zed. |
| `zevbox project list` | Affiche la liste des projets autorisés. |
| `zevbox zed install [--dry-run]` | Injecte de façon non destructive les entrées gérées (`zevbox · ...`) dans `~/.config/zed/settings.json` en préservant commentaires et formatage CST. Sauvegarde automatique dans `backups/`. |
| `zevbox zed uninstall` | Retire toutes les entrées gérées de Zed et restaure les réglages à l'octet près. |
| `zevbox auth <agent> [--status]` | Claude : lance la commande d'authentification interactive dans un conteneur `-it`. Antigravity : exécute le flux Google OAuth 2.0 PKCE **sur le Mac** (serveur HTTP éphémère `127.0.0.1:<port aléatoire>`, aucun port publié sur la micro-VM, aucun accès au trousseau) et écrit `state/antigravity/.gemini/antigravity-acp/{settings.json,acp_token.json}` en `0600`. Avec `--status`, affiche `authenticated`, `not authenticated` ou `unknown`. |
| `zevbox launch <agent> [--project <chemin>]` | Commande appelée par Zed pour démarrer la micro-VM et relayer le protocole ACP sur stdin/stdout (avec filtrage de sécurité strict). Sans `--project`, le projet est le répertoire courant (Zed lance l'agent dans le premier dossier racine du projet). |
| `zevbox status [--json]` | Affiche les instances actives, orphelines ou non enregistrées. |
| `zevbox stop [ID \| --project <chemin>]` | Arrête proprement les instances spécifiées ou toutes les instances actives de zevbox (jamais avec l'option globale `--all`). |
| `zevbox clean [--images]` | Nettoie les conteneurs orphelins vérifiés, les enregistrements obsolètes, et purge les journaux anciens (>30) et sauvegardes (>20). Avec `--images`, supprime les anciennes images inutilisées. |
| `zevbox shell <agent> [--project <chemin>]` | Ouvre un shell Bash interactif dans un conteneur micro-VM jetable identique à celui de l'agent. Sans `--project`, le dossier courant doit être un projet autorisé (sinon erreur, code 3). `auth claude` ne monte, lui, aucun projet (dossier vide jetable + état de l'agent). |
| `zevbox logs [ID] [--path]` | Affiche les 200 dernières lignes du journal de la session la plus récente ou de l'ID spécifié. Avec `--path`, retourne le chemin du fichier journal. |
| `zevbox update [--check] [--agent <agent>]` | Vérifie ou construit une nouvelle version de l'agent depuis le registre ACP officiel, exécute le smoke test et assure la promotion. |
| `zevbox rollback [--agent <agent>]` | Permute la version active (`current`) avec la version précédente (`previous`). |
| `zevbox tools list` / `add <paquets...>` / `remove <paquets...>` | Gère les paquets Debian apt additionnels (`tools.json`) et reconstruit automatiquement les images d'agents actives. |

---

## 4. Persistance et cycle de vie (§11)

| Donnée | Comportement à la fin d'une instance |
|---|---|
| **Code du projet** | Persiste sur le Mac (modifications visibles immédiatement, même UID/GID). |
| **Identifiants de l'agent** | Persistent dans `state/<agent>` (monté sur `/home/agent` dans la VM). |
| **Fichiers temporaires hors projet** | Supprimés à l'arrêt du conteneur (`--rm`, micro-VM jetable). |
| **Journaux de session** | Persistent dans `logs/<instance_id>.log` (rotation à 30 fichiers). |
| **Enregistrement d'instance** | Supprimé de `instances/<instance_id>.json` à l'arrêt propre. |

---

## 5. Invariants de sécurité stricts (§3)

- **I1 — Montages stricts** : Exactement deux montages hôtes dans la VM : le répertoire du projet autorisé et le sous-dossier d'état de l'agent. Aucun montage de `$HOME`, `~/.ssh`, `~/.claude`, etc.
- **I2 — Projets autorisés** : Seuls les répertoires explicitement validés et canonisés dans `config.json` peuvent être lancés.
- **I3 — Blocage des requêtes dangereuses** : Tout appel ACP vers Zed demandant des opérations de système de fichiers (`fs/*`), terminal (`terminal/*`) ou authentification terminale sur l'hôte Mac est intercepté et rejeté.
- **I4 — Filtrage bidirectionnel** : Seules les méthodes ACP autorisées et sûres transitent entre Zed et l'agent.
- **I5 — CWD confiné** : Le répertoire de travail est toujours strictement fixé à la racine du projet autorisé.
- **I6 — Flux JSON-RPC pur** : Sur stdout en mode `launch`, seuls des messages JSON-RPC stricts sont émis, garantissant la stabilité de Zed.
- **I7 — Aucun shell hôte** : Aucun processus shell (`sh`, `bash`, `zsh`) n'est jamais exécuté sur le Mac hôte.
- **I8 — Ciblage vérifié des conteneurs** : `container stop` et `container delete` n'utilisent jamais `--all` et ne ciblent que des conteneurs validés avec préfixe et labels `zevbox`.
- **I9 — Confidentialité des journaux** : Aucun jeton, identifiant ou contenu de message n'est écrit dans les logs ou affiché en cas d'erreur.
- **I10 — Absence de réseau au lancement** : La commande `launch` ne fait aucun build ni appel réseau externe bloquant.
- **I11 — Robustesse Rust** : Zéro `unwrap()`, `expect()` ou `panic!()` dans le code source de production (`src/`).
- **I12 — Isolation de zevbox** : Les binaires et métadonnées internes de zevbox ne sont jamais montés dans les conteneurs.

---

## 6. Dépannage par message d'erreur

- **`zevbox is not set up. Fix: run 'zevbox setup' or 'zevbox init'`** : Exécutez `zevbox setup` dans votre terminal pour initialiser l'environnement.
- **`<chemin> is not an allowed project. Fix: run 'zevbox project add <chemin>'`** : Le dossier ouvert n'est pas dans la liste blanche. Ajoutez-le avec `zevbox project add`.
- **`no active Claude release. Fix: run 'zevbox update --agent claude'`** : L'image de l'agent n'a pas encore été construite. Lancez `zevbox update --agent claude`.
- **`Claude is not authenticated. Fix: run 'zevbox auth claude'`** : Lancez `zevbox auth claude` dans votre terminal pour vous connecter à votre compte Claude.
- **`Antigravity is not authenticated. Fix: run 'zevbox auth antigravity'`** : Lancez `zevbox auth antigravity` et validez votre compte Google dans le navigateur.
- **`zevbox: this agent is bound to <A>; this session asked for <B>. Open <A> in Zed, or run 'zevbox project add <B>'`** : la session Zed demande un dossier différent de celui dans lequel l'agent a été lancé — typiquement un espace de travail Zed à plusieurs dossiers racines. Ouvrez un seul dossier racine par fenêtre Zed.
- **`Auto mode unavailable: the selected model does not support Auto mode`** : le modèle par défaut de Claude dans la VM est Haiku 4.5. `zevbox doctor` signale `no default model` ; lancez `zevbox auth claude` (écrit `state/claude/.claude/settings.json`) ou écrivez-y `{"model": "fable"}`, puis ouvrez un nouveau thread Zed.
- **`Apple Container system is not running. Fix: run 'container system start'`** : Le service de virtualisation Apple Container est arrêté. Démarrez-le avec `container system start`.
