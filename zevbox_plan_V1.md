# plan_zevbox.md — Agents ACP isolés pour Zed avec Apple Container (Implémentation Rust)

## 1. Objectif et conditions de réussite

Construire un outil `zevbox` en **Rust** (compilé en un binaire natif autonome sur macOS) permettant d’utiliser **Claude et Antigravity dans Zed, via ACP**, avec leurs processus et leurs commandes exécutés dans une micro-VM Linux Apple Container. Zed reste sur le Mac ; le projet est partagé avec la micro-VM par un montage explicitement limité. [Source](https://zed.dev/docs/ai/external-agents) [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

L’outil doit proposer : installation, authentification, lancement automatique depuis Zed, état des instances, arrêt explicite, nettoyage ciblé, mise à jour testée, retour arrière et ajout d’outils Linux. **Ne considérer aucun composant comme opérationnel avant les vérifications décrites dans ce document.** [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

**Important : ce document remplace les plans précédents. Plusieurs affirmations précédentes étaient trop catégoriques ou incorrectes, notamment sur l’authentification, les fichiers de configuration, les accès via ACP et la destruction à la fermeture d’un thread.** Les corrections ci-dessous font partie des exigences de réalisation. [Source](https://agentclientprotocol.com/protocol/v1/file-system) [Source](https://code.claude.com/docs/en/authentication) [Source](https://zed.dev/docs/languages/json)

## 2. Corrections indispensables par rapport au plan précédent

### 2.1. Conteneuriser le processus ne suffit pas si ACP donne accès au Mac

ACP v1 permet à un agent de demander au **client**, donc à Zed, de lire ou d’écrire des fichiers avec `fs/read_text_file` et `fs/write_text_file`. Il permet également de demander au client de créer un terminal. Un agent dans un conteneur pourrait donc bénéficier d’un accès à l’hôte **par l’intermédiaire de Zed**, même sans montage correspondant. [Source](https://agentclientprotocol.com/protocol/v1/file-system) [Source](https://agentclientprotocol.com/protocol/draft/schema)

**Modification obligatoire : ajouter un relais ACP contrôlé entre Zed et le conteneur.** Ce relais doit désactiver et refuser les opérations permettant à l’agent de faire exécuter une commande ou un accès fichier par Zed. Si un adaptateur ne fonctionne pas sans ces opérations, déclarer cet adaptateur incompatible avec le mode strict ; ne pas rouvrir l’accès automatiquement. [Source](https://agentclientprotocol.com/protocol/v1/file-system) [Source](https://agentclientprotocol.com/protocol/draft/schema)

### 2.2. Monter `~/.claude` ne garantit pas l’authentification Claude

Sur macOS, Claude Code conserve normalement ses identifiants dans le Trousseau. Linux utilise un fichier de credentials. Monter le dossier macOS `~/.claude` dans Linux ne transfère donc pas nécessairement une connexion valide. [Source](https://code.claude.com/docs/en/authentication)

**Modification obligatoire : utiliser un état d’authentification Linux dédié à zevbox, séparé de la configuration Claude du Mac.** Ne jamais exporter automatiquement le Trousseau, copier tous les réglages personnels ou afficher des tokens dans les journaux. [Source](https://code.claude.com/docs/en/authentication)

### 2.3. L’existence de `~/.gemini` ne prouve rien pour Antigravity ACP

La découverte de `~/.gemini` ne démontre pas que l’adaptateur Antigravity ACP y stocke son authentification. Le manifeste officiel permet d’identifier son téléchargement et sa commande, **pas de déduire le dossier de credentials à partir de son nom**. L’authentification doit être vérifiée dans l’environnement Linux de zevbox. [Source](https://github.com/agentclientprotocol/registry/blob/main/antigravity-acp/agent.json)

### 2.4. Le paquet Claude doit être résolu depuis le registre actuel

Ne pas reprendre aveuglément `@zed-industries/claude-code-acp`. Le dépôt actuel indique le paquet `@agentclientprotocol/claude-agent-acp`, le binaire `claude-agent-acp` et une exigence Node.js ≥ 22 **dans l'image Linux du conteneur**. Sur l'hôte macOS, l'outil `zevbox` est un binaire natif compilé en Rust qui n'a pas besoin de runtime Node.js sur le Mac. À l’installation et aux mises à jour, résoudre le paquet et sa version depuis le manifeste officiel, puis les verrouiller. [Source](https://github.com/agentclientprotocol/claude-agent-acp/blob/main/package.json) [Source](https://github.com/agentclientprotocol/registry/blob/main/claude-acp/agent.json)

### 2.5. Le chemin des settings Zed précédent était incorrect

Le chemin documenté des réglages utilisateur sur macOS est `~/.config/zed/settings.json`. Le dossier `~/Library/Application Support/Zed` contient notamment des données de l’application, mais ne doit pas être supposé être l’emplacement des réglages. Utiliser `zed: open settings` pour identifier le fichier réellement actif. [Source](https://github.com/zed-industries/zed/blob/main/docs/.doc-examples/configuration.md)

Les réglages Zed acceptent des commentaires `//` et `/* */` ainsi que des virgules finales. **`python3 json.load()` et `jq` ne doivent pas être utilisés pour valider ou modifier le fichier utilisateur.** Utiliser un éditeur JSONC basé sur l'AST (`jsonc-parser` en Rust) conservant strictement les commentaires et les autres réglages. [Source](https://zed.dev/docs/languages/json)

### 2.6. Archivage et destruction ne sont pas équivalents

`--rm` supprime le conteneur **après son arrêt**. Cela ne prouve pas que l’archivage d’une conversation dans Zed arrête le processus ACP. ACP distingue les connexions et les sessions ; un processus peut gérer plusieurs sessions. **Ne pas promettre « un thread = un conteneur » ni « archiver = détruire ».** [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md) [Source](https://agentclientprotocol.com/protocol/v1/session-setup.md)

**Modification obligatoire : fournir `zevbox status`, `zevbox stop` et `zevbox clean`.** L’arrêt explicite doit être fiable, indépendamment du comportement observé de Zed. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

### 2.7. Ne pas ajouter les anciennes règles sandbox Claude sans vérification

Supprimer du plan l’écriture automatique de `.claude/settings.json` contenant `denyWrite: ["~/"]`, `allowWrite: ["."]` et des permissions supposées. Elle risque d’écraser une configuration existante, de créer des conflits et de donner une fausse assurance. Le confinement principal de ce projet repose sur **la micro-VM et le relais ACP**, pas sur ces anciennes propositions. [Source](https://agentclientprotocol.com/protocol/v1/file-system) [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

## 3. Périmètre et limites de sécurité

Le dispositif doit empêcher les processus agents d’accéder directement aux fichiers du Mac non montés et de déléguer leurs commandes ou accès fichiers à Zed. Il ne doit pas être présenté comme une garantie absolue contre toutes les vulnérabilités possibles du runtime ou du relais. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md) [Source](https://agentclientprotocol.com/protocol/v1/file-system)

Le projet monté reste accessible en lecture-écriture : l’agent peut y modifier ou supprimer des fichiers. Les identifiants de son compte restent accessibles à son runtime. Le réseau nécessaire aux services IA reste disponible ; **le conteneur n’empêche pas, à lui seul, l’exfiltration du code monté ni l’accès à des services réseau joignables**. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

Le dossier contenant le binaire zevbox, sa configuration, sa politique de sécurité et ses versions actives **ne doit pas être monté dans le conteneur**. Autrement, l’agent pourrait modifier les règles qui gouvernent son prochain lancement. Cette exigence complète le périmètre défini par les montages. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

Les contenus volontairement transmis depuis Zed restent transmis à l’agent : pièces jointes, texte collé, fichiers mentionnés ou contexte de conversation. **Ne pas joindre de fichier sensible extérieur au projet sous prétexte que le processus est conteneurisé.** [Source](https://github.com/agentclientprotocol/claude-agent-acp)

## 4. Architecture retenue

```text
Zed sur macOS
    │
    │ lance un agent custom
    ▼
zevbox launch <agent> --project <racine-autorisée> (Binaire natif Rust)
    │
    ├──── vérifie la configuration, l’image et les chemins
    ├──── attribue un identifiant d’instance
    ├──── démarre le conteneur Linux (tokio::process::Command)
    └──── relaie ACP avec une politique restrictive (Tokio async I/O)
            │
            ▼
Micro-VM Linux Apple Container
    ├──── adaptateur ACP Claude OU Antigravity
    ├──── outils Linux installés dans l’image
    ├──── projet explicitement monté en lecture-écriture
    └──── état Linux dédié à cet agent
```

Zed sait lancer un agent ACP personnalisé avec `type: "custom"`, `command` et `args`. Le binaire compilé `zevbox` devient cette commande ; il démarre le conteneur à la demande et assure le relais bidirectionnel immédiat (< 2 ms de latence). Aucun serveur permanent n’est nécessaire. [Source](https://zed.dev/docs/ai/external-agents)

### Pourquoi conserver le même chemin du projet ?

ACP transmet notamment un `cwd` absolu lors de la création d’une session. Sans traduction des chemins, le moyen le plus simple d’avoir un fonctionnement cohérent est de monter le projet au même chemin absolu dans Linux que dans Zed. [Source](https://agentclientprotocol.com/protocol/v1/session-setup.md)

Ce n’est **pas une obligation intrinsèque du conteneur ni une mesure de sécurité** : un montage `/workspace` est possible avec une couche de traduction complète. Pour la première version, ne pas développer cette complexité ; conserver les chemins identiques. [Source](https://agentclientprotocol.com/protocol/v1/session-setup.md) [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

**Choix prudent pour la première configuration : utiliser `~/code/myproject`, le chemin de la commande initiale de l’utilisateur.** Ne pas élargir automatiquement à `~/code/projects/me`. Si le projet réel exige une autre racine, cette différence doit être explicitement présentée avant son activation. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

## 5. Livrables à créer (Projet Rust)

Créer l’arborescence suivante **hors du projet monté**. Elle constitue le contrat d’implémentation ; les commandes `zevbox` sont fournies par un binaire compilé unique en Rust. [Source](https://zed.dev/docs/ai/external-agents)

```text
~/code/agentbox/
├── Cargo.toml                  # Dépendances Rust verrouillées
├── Cargo.lock                  # Lockfile Rust
├── src/
│   ├── main.rs                 # Point d'entrée CLI (routage clap)
│   ├── cli/                    # Sous-commandes CLI
│   │   ├── mod.rs
│   │   ├── launch.rs           # zevbox launch
│   │   ├── auth.rs             # zevbox auth
│   │   ├── stop.rs             # zevbox stop
│   │   ├── status.rs           # zevbox status
│   │   ├── doctor.rs           # zevbox doctor
│   │   ├── tools.rs            # zevbox tools
│   │   └── zed.rs              # zevbox zed install
│   ├── config.rs               # Parsing & validation de zevbox.json & lockfile
│   ├── container/              # Supervision du processus Apple Container
│   │   ├── mod.rs
│   │   ├── runner.rs           # tokio::process::Command (arguments stricts, sans shell)
│   │   └── supervisor.rs       # Gestion PID, signaux (SIGTERM/SIGINT) et extinction
│   ├── relay/                  # Cœur du relais ACP filtrant
│   │   ├── mod.rs
│   │   ├── codec.rs            # Framing JSON-RPC sur async stdin/stdout
│   │   ├── policy.rs           # Règles Fail-Closed (refus fs/*, terminal/*, cwd strict)
│   │   └── types.rs            # Structures typées ACP (serde)
│   ├── zed/                    # Gestion des réglages Zed
│   │   ├── mod.rs
│   │   └── jsonc.rs            # Édition AST préservant commentaires et formatage
│   └── state.rs                # Gestion du stockage local (~/Library/Application Support/zevbox)
├── tests/
│   ├── fake_agent.rs           # Mock ACP agent pour les tests d'intégration
│   ├── relay_security_test.rs  # Tests de sécurité des règles Fail-Closed
│   ├── zed_settings_test.rs    # Tests de préservation JSONC (Golden tests)
│   └── lifecycle_test.rs       # Tests de cycle de vie et de signaux
├── config/
│   ├── zevbox.json             # Configuration par défaut
│   └── tools.json              # Déclaration des outils Linux
├── images/
│   └── Containerfile           # Image de base Linux ARM64
├── components.lock.json        # Versions et checksums des agents verrouillés
├── README.md
└── plan_zevbox.md

~/Library/Application Support/zevbox/
├── state/
│   ├── claude/                 # Credentials et état Linux Claude (exclus de Git)
│   └── antigravity/            # Credentials et état Linux Antigravity
├── instances/                  # Fichiers JSON d'instances en cours (PID, container name)
├── backups/                    # Sauvegardes horodatées de settings.json Zed
├── logs/                       # Journaux d'exécution et diagnostics (stderr)
├── downloads/                  # Téléchargements vérifiés par checksum
└── releases/                   # Métadonnées de releases actives
```

### Dépendances Rust imposées (`Cargo.toml`)
Pour garantir la cohérence, l’IA exécutante doit utiliser les crates éprouvés suivants :
* `clap = { version = "4", features = ["derive"] }` : Analyse des arguments CLI.
* `tokio = { version = "1", features = ["full", "process", "io-util", "signal"] }` : Runtime asynchrone, pipes et signaux.
* `serde = { version = "1", features = ["derive"] }` & `serde_json = "1"` : Sérialisation JSON.
* `jsonc-parser = "0.26"` : Manipulation chirurgicale de JSONC avec préservation absolue des commentaires `//` et `/* */`.
* `tracing = "0.1"` & `tracing-subscriber = "0.3"` : Journalisation structurée (**uniquement vers `stderr` ou fichier**).
* `tempfile = "3"` : Écritures atomiques de fichiers.
* `thiserror = "2"` & `anyhow = "1"` : Gestion propre et typée des erreurs sans panic.

## 6. Règles impératives pour l’IA exécutante

* [ ] **Interdiction absolue de `unwrap()` et `expect()` dans le runtime du relais :** Tout déballage non sécurisé peut faire crasher le processus en cas de payload inattendu. Utiliser l'opérateur `?` avec des types d'erreur typés (`thiserror`/`anyhow`).
* [ ] **Sanctuarisation totale de `stdout` :** `stdout` est exclusivement réservé au flux JSON-RPC échangé avec Zed. **Aucun `println!`, `dbg!` ou macro de log ne doit écrire sur `stdout`** dans le code du relais ou du runner. Tous les diagnostics, bannières et traces doivent impérativement sortir sur `stderr` ou dans les fichiers de logs.
* [ ] **Aucune exécution via un interpréteur shell (`sh -c`) :** Ne jamais concaténer de commande shell. Utiliser exclusivement `tokio::process::Command::new(binary).args(&[...])` avec des arguments stricts sous forme de vecteurs pour interdire toute injection d'argument.
* [ ] **Pas d'opérations bloquantes dans l'exécuteur Tokio :** Ne jamais utiliser `std::fs` ou `std::process` dans les boucles asynchrones du relais ; utiliser les versions asynchrones de Tokio (`tokio::fs`, `tokio::process`).
* [ ] **Stratégie Fail-Closed stricte :** Si une méthode ACP entrante ou sortante n'est pas explicitement reconnue dans la liste blanche d'opérations auditées, elle doit être **bloquée par défaut** avec un code d'erreur JSON-RPC standard, jamais transmise en aveugle.
* [ ] **Contrôle et validation continue du code :** Exécuter `cargo check`, `cargo test`, `cargo clippy -- -D warnings` et `cargo fmt --check` avant de déclarer chaque phase terminée.
* [ ] **Ne jamais élargir un montage pour faire passer un test :** Un échec doit produire un diagnostic, pas une réduction silencieuse de la sécurité. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] **Ne jamais monter `$HOME`, `/Users`, le Trousseau, les dossiers personnels `.claude` ou `.gemini`, un socket Docker ou un service de contrôle du runtime :** Utiliser uniquement le projet et l’état dédié de l’agent concerné. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] **Ne jamais utiliser `--all` pour arrêter, supprimer ou nettoyer les conteneurs :** L’utilisateur possède déjà des conteneurs actifs. Toutes les opérations doivent cibler des identifiants `zevbox-*` enregistrés et vérifiés. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] **Ne jamais passer `-t` au conteneur transportant ACP :** Réserver `-it` au shell interactif ou à une authentification manuelle ; le lancement ACP utilise `-i`. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] **Ne jamais enregistrer de tokens, d’en-têtes d’authentification, de contenu complet des prompts ou de fichiers dans les journaux par défaut.** [Source](https://code.claude.com/docs/en/authentication)
* [ ] **Ne jamais annoncer « installation réussie » si seuls le build ou `initialize` ont réussi :** L’authentification, les opérations projet, le relais et l’arrêt doivent aussi être testés. [Source](https://agentclientprotocol.com/protocol/v1/session-setup.md)

## 7. Phase A — Inventaire non destructif

### Actions

Exécuter les commandes suivantes sur le Mac. Ne pas arrêter les conteneurs existants et ne pas masquer stderr. L’aide du CLI installé prime sur les exemples provenant d’une autre version. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

```bash
command -v container
container --version
container system status
container list

container run --help
container stop --help
container delete --help
container build --help
container exec --help

command -v cargo
cargo --version
command -v rustc
rustc --version

test -d ~/code/myproject
```

Ouvrir les réglages avec la palette Zed → `zed: open settings`. Relever le chemin exact du fichier actif et la version de Zed. Ne pas modifier les réglages à ce stade. [Source](https://github.com/zed-industries/zed/blob/main/docs/.doc-examples/configuration.md)

### Vérifications et correction

* [ ] Le runtime `container` est opérationnel et l’architecture est `arm64`. Si le système est arrêté, lancer `container system start`, puis revérifier ; ne pas ignorer un échec. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] Le CLI fournit les options nécessaires : `--name`, `--mount`, `--rm`, `-i`, `--cpus`, `--memory`, ainsi que l’arrêt et la suppression ciblés. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] L'environnement de compilation Rust (`cargo`, `rustc`) est fonctionnel sur le Mac.
* [ ] Le projet existe et son chemin canonique est enregistré. Si `myproject` n’est pas la bonne racine, suspendre l’activation et présenter le changement nécessaire. [Source](https://agentclientprotocol.com/protocol/v1/session-setup.md)
* [ ] Conserver un inventaire des conteneurs préexistants pour vérifier qu’ils restent intacts après tous les tests. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

## 8. Phase B — Configuration validée et versions verrouillées

### Configuration

Créer `config/zevbox.json` avec, au minimum, les paramètres suivants. Les chemins absolus sont validés au chargement ; ils ne sont jamais interpolés dans une commande shell. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

```json
{
    "schemaVersion": 1,
    "projectRoot": "~/code/myproject",
    "containerBinary": "/opt/homebrew/bin/container",
    "stateRoot": "~/Library/Application Support/zevbox",
    "resources": {
        "cpus": 4,
        "memory": "4G"
    },
    "security": {
        "clientFilesystem": false,
        "clientTerminal": false,
        "mcpServers": false,
        "failClosed": true
    }
}
```

Remplacer `containerBinary` par le résultat réel de `command -v container`. Les ressources ci-dessus sont des valeurs de départ configurables, pas une recommandation universelle ; plusieurs agents peuvent consommer ces ressources simultanément. [Source](https://github.com/apple/container/blob/main/docs/resource-usage.md)

### Résolution des composants

Télécharger les manifestes officiels Claude et Antigravity. Lire le bloc de distribution correspondant à Linux ARM64 ; pour Antigravity, la clé de plateforme du registre est `linux-aarch64`. **Ne pas construire une URL en concaténant un numéro de version.** [Source](https://github.com/agentclientprotocol/registry/blob/main/antigravity-acp/agent.json) [Source](https://agentclientprotocol.com/rfds/acp-agent-registry)

```bash
curl --fail --show-error --location \
  https://raw.githubusercontent.com/agentclientprotocol/registry/main/antigravity-acp/agent.json \
  --output antigravity-agent.json

curl --fail --show-error --location \
  https://raw.githubusercontent.com/agentclientprotocol/registry/main/claude-acp/agent.json \
  --output claude-agent.json
```

Créer `components.lock.json` contenant : manifeste utilisé, date de résolution, version exacte, paquet npm exact (pour l'image Linux), commande, arguments, URL de téléchargement exacte, empreinte du fichier téléchargé et référence de l’image de base. Installer uniquement les versions verrouillées lors d’un build normal. [Source](https://agentclientprotocol.com/rfds/acp-agent-registry) [Source](https://github.com/agentclientprotocol/claude-agent-acp/blob/main/package.json)

### Vérifications

* [ ] Les manifestes sont du JSON valide et leurs identifiants correspondent aux agents attendus. Une modification du schéma doit produire une erreur explicite, pas une valeur par défaut hasardeuse. [Source](https://agentclientprotocol.com/rfds/acp-agent-registry)
* [ ] La distribution Linux ARM64 existe ; sinon déclarer l’agent indisponible, sans réutiliser le binaire macOS. [Source](https://github.com/agentclientprotocol/registry/blob/main/antigravity-acp/agent.json)
* [ ] Le paquet npm Claude et son champ `bin` sont vérifiés avant de produire les commandes de lancement dans le conteneur. [Source](https://github.com/agentclientprotocol/claude-agent-acp/blob/main/package.json)
* [ ] Toutes les dépendances de l’outil hôte sont verrouillées dans `Cargo.lock`. Un lancement quotidien ne doit pas télécharger de nouvelle dépendance.

## 9. Phase C — Construire les images Linux

### Conception

Préférer une image par agent, construite sur une base commune. Cela évite qu’un problème de mise à jour Antigravity bloque Claude et permet de distinguer les versions actives. Le script doit utiliser explicitement le Containerfile choisi avec les options prises en charge par le CLI installé. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

Installer initialement : Git, certificats CA, `curl`, `jq`, `unzip`, Python 3 et les outils de diagnostic utiles. Installer Node.js dans la version requise pour Claude (≥ 22). Les versions des adaptateurs viennent du fichier verrouillé ; aucun `npm install ...@latest` ne doit apparaître dans le build normal. [Source](https://github.com/agentclientprotocol/claude-agent-acp/blob/main/package.json)

Pour Antigravity, télécharger l’archive exacte du manifeste, vérifier son contenu et installer ses exécutables Linux ensemble. Ne pas supposer que le suffixe `.par` signifie « Python portable » : le fichier macOS observé était un Mach-O. [Source](https://github.com/agentclientprotocol/registry/blob/main/antigravity-acp/agent.json)

Créer un utilisateur Linux non privilégié pour l’exécution des agents. Tester ses droits sur le montage projet ; si les droits posent problème, diagnostiquer la correspondance UID/GID et les permissions, **sans `chmod -R 777` ni `chown -R` sur le projet hôte**. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

Ajouter un marqueur d’image sans secret : `/etc/zevbox-release.json`, contenant l’identifiant de release, le type d’agent, les versions et la date de build. Ce marqueur facilite les diagnostics, mais ne constitue pas à lui seul une preuve de confinement. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

### Vérifications

* [ ] L’image démarre et `uname -s` retourne `Linux`. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] Chaque outil déclaré répond à son contrôle de présence ou de version. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] L’adaptateur répond à un `initialize` ACP valide dans un délai borné. Conserver stderr ; ne pas utiliser `head -1` comme preuve de réussite. [Source](https://agentclientprotocol.com/protocol/draft/schema)
* [ ] La réponse JSON correspond à l’identifiant de la requête, contient un résultat valide et ne contient pas d’erreur. [Source](https://agentclientprotocol.com/protocol/draft/schema)
* [ ] Un échec ou un crash bloque la promotion de l’image. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

## 10. Phase D — Relais ACP restrictif en Rust

### Exigence centrale

Le relais est le composant cœur de sécurité implémenté dans le module `relay/`. Il transporte les messages ACP entre Zed et le processus enfant `container run`, inspecte et filtre les requêtes dans les deux directions et supervise le conteneur. **Ce composant est une frontière de sécurité ; ses tests sont obligatoires avant l’intégration Zed.** [Source](https://agentclientprotocol.com/protocol/v1/file-system) [Source](https://agentclientprotocol.com/protocol/draft/schema)

### Politique minimale (Fail-Closed)

* [ ] Dans `initialize`, annoncer à l’agent `fs.readTextFile: false`, `fs.writeTextFile: false` et `terminal: false`. Ne pas confondre cette annonce avec une sandbox : elle doit être doublée de refus effectifs. [Source](https://agentclientprotocol.com/protocol/v1/file-system) [Source](https://agentclientprotocol.com/protocol/draft/schema)
* [ ] Refuser les requêtes agent → Zed `fs/read_text_file`, `fs/write_text_file` et toutes les requêtes `terminal/*`. Répondre immédiatement avec une erreur JSON-RPC explicite et l’identifiant correct ; ne pas transmettre la requête à Zed. [Source](https://agentclientprotocol.com/protocol/draft/schema)
* [ ] Identifier et désactiver les extensions pouvant appliquer des patches sur l’hôte, exécuter une commande ou ouvrir un flux d’authentification terminal côté client. Une extension inconnue n’est pas automatiquement autorisée. [Source](https://github.com/agentclientprotocol/claude-agent-acp) [Source](https://agentclientprotocol.com/rfds/v2/client-filesystem-terminal-capabilities)
* [ ] Autoriser uniquement les requêtes agent → Zed expressément auditées, notamment les demandes de permission compatibles avec cette politique. Les autres requêtes sont refusées par défaut. [Source](https://agentclientprotocol.com/protocol/draft/schema)
* [ ] Conserver les notifications nécessaires à l’affichage des réponses, outils et diffs seulement après vérification qu’elles ne déclenchent pas une action hôte. Distinguer affichage et exécution. [Source](https://github.com/agentclientprotocol/claude-agent-acp)
* [ ] À `session/new`, `session/load` et toute méthode équivalente prise en charge, vérifier que `cwd` correspond exactement à la racine autorisée. Refuser un autre projet ou un répertoire supplémentaire (`..`, chemin absolu non autorisé). [Source](https://agentclientprotocol.com/protocol/v1/session-setup.md)
* [ ] En mode strict initial, transmettre une liste `mcpServers` vide et bloquer les mécanismes équivalents non audités. Ne pas transmettre silencieusement des serveurs MCP personnels. [Source](https://agentclientprotocol.com/protocol/v1/session-setup.md)
* [ ] Vérifier qu’aucune configuration native de l’agent ne charge un serveur MCP, un hook ou un outil hôte inattendu. Utiliser un état Linux dédié plutôt que copier les réglages personnels. [Source](https://zed.dev/docs/ai/external-agents)

### Robustesse du transport asynchrone

Le parseur / décodeur de trames (`relay/codec.rs`) doit gérer :
* Messages fragmentés entre plusieurs lectures de buffer.
* Plusieurs messages JSON-RPC reçus consécutivement dans le même bloc.
* Encodage UTF-8 strict.
* Limites configurables sur la taille maximale d'un message pour éviter les attaques par épuisement de mémoire.
* Fermeture propre du canal lors de l'arrêt de `stdin` (EOF) ou arrêt du processus enfant.
* **Aucun panic sur payload malformé :** si une ligne reçue n'est pas du JSON valide, consigner une erreur sur `stderr` et clore la session proprement. [Source](https://agentclientprotocol.com/protocol/draft/schema)

## 10bis. Checkpoints de compilation et jalons obligatoires

L'IA exécutante **doit impérativement valider chaque jalon de compilation avant de passer à la suite**. Aucun développement en bloc de plusieurs centaines de lignes sans compilation préalable n'est autorisé.

| Jalon | Fichiers concernés | Commande de validation | Condition de passage |
|---|---|---|---|
| **Checkpoint 0 : Dépendances et squelette** | `Cargo.toml`, `src/main.rs`, structure de modules | `cargo check` | Succès 0 erreur de dépendances ou de module |
| **Checkpoint 1 : Modèles et règles de sécurité** | `src/relay/types.rs`, `src/relay/policy.rs` | `cargo test lib::relay::policy` | 100% des règles Fail-Closed compilent et passent |
| **Checkpoint 2 : Superviseur de conteneurs** | `src/container/runner.rs`, `src/container/supervisor.rs` | `cargo check && cargo clippy -- -D warnings` | Zero warning Clippy, pas d'utilisation de shell |
| **Checkpoint 3 : Édition JSONC Zed** | `src/zed/jsonc.rs`, `tests/zed_settings_test.rs` | `cargo test tests::zed_settings_test` | Commentaires préservés, tests golden files validés |
| **Checkpoint 4 : Relais de sécurité complet** | `src/relay/codec.rs`, `tests/relay_security_test.rs` | `cargo test tests::relay_security_test` | Rejet de toutes les tentatives d'évasion |
| **Checkpoint Final : Binaire de release** | Ensemble du crate `zevbox` | `cargo build --release && cargo test` | Binaire généré dans `target/release/zevbox`, 0 warning |

## 10ter. Suite de tests automatisés obligatoire

La suite de tests dans `tests/` est exécutable sans dépendance à un modèle d'IA payant et sans appel réseau.

### 1. Tests de sécurité du relais (`tests/relay_security_test.rs`)
* [ ] **Tentative `fs/read_text_file` :** Le mock agent envoie une requête de lecture hôte → Le relais intercepte la requête, renvoie un code d'erreur JSON-RPC à l'agent, et ne transmet aucun message vers Zed.
* [ ] **Tentative `fs/write_text_file` :** Idem pour les écritures de fichier hôte.
* [ ] **Tentative `terminal/create` :** Idem pour la création de terminal hôte.
* [ ] **Tentative de traversée de répertoire (`cwd traversal`) :** Une requête `session/new` avec un `cwd` contenant `../` ou pointant hors de `projectRoot` est immédiatement rejetée.
* [ ] **Méthodes inconnues :** Toute méthode inconnue non auditée est rejetée en mode Fail-Closed.
* [ ] **Suppression des serveurs MCP :** Le client Zed envoie un `initialize` ou `session/new` avec des `mcpServers` → Le relais les filtre avant transmission à l'agent.

### 2. Tests de robustesse et fragmentation du transport (`tests/relay_security_test.rs`)
* [ ] **Message fragmenté :** Un message JSON découpé en 3 morceaux distincts sur le flux est correctement réassemblé et traité.
* [ ] **Messages coalescés :** Deux messages JSON envoyés dans le même paquet sont tous les deux décodés et traités séquentiellement.
* [ ] **Payload invalide / non-UTF-8 :** Une chaîne aléatoire ou malformée n'entraîne aucun panic et déclenche une erreur propre.

### 3. Tests de préservation JSONC (`tests/zed_settings_test.rs`)
* [ ] **Golden file avec commentaires :** Un fichier exemple `settings.json` contenant des commentaires sur une ligne `// ...`, des blocs `/* ... */`, et des virgules finales est modifié pour ajouter les agents `zevbox`. Un diff textuel vérifie que 100% des commentaires existants et le formatage sont préservés.
* [ ] **Idempotence :** Exécuter deux fois l'installation ne crée aucun doublon dans `agent_servers`.

### 4. Tests de cycle de vie et signaux (`tests/lifecycle_test.rs`)
* [ ] **Extinction sur EOF :** Fermer `stdin` du relais provoque l'envoi d'un `SIGTERM` au processus conteneur, un arrêt sous 3 secondes et le nettoyage de l'instance enregistrée.
* [ ] **Ciblage strict des conteneurs :** Vérifier que les fonctions d'arrêt et de nettoyage refusent formellement de toucher à un conteneur dont le nom ne correspond pas au préfixe `zevbox-`.

## 11. Phase E — Authentification séparée

### Commandes à développer

```bash
~/code/agentbox/target/release/zevbox auth claude
~/code/agentbox/target/release/zevbox auth antigravity
```

Ces commandes lancent un environnement interactif Linux avec l’état dédié à l’agent concerné. Un TTY (`-it`) est permis pour ce flux manuel, contrairement au transport ACP. L’utilisateur réalise lui-même l’authentification via le mécanisme officiel disponible. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md) [Source](https://code.claude.com/docs/en/authentication)

Pour Claude, vérifier le mécanisme supporté par la version installée et l’utilisation du forfait souhaité. Ne pas injecter `ANTHROPIC_API_KEY` : une clé approuvée peut faire utiliser la facturation API au lieu de l’abonnement. [Source](https://code.claude.com/docs/en/troubleshoot-install)

Pour Antigravity, déterminer le mécanisme d’authentification du build Linux installé. Si son flux exige une capacité client bloquée, chercher une méthode officielle d’authentification manuelle dans le conteneur. **En l’absence de méthode compatible vérifiée, marquer Antigravity comme bloqué et conserver Claude fonctionnel ; ne pas revenir à un agent hôte non confiné.** [Source](https://github.com/agentclientprotocol/registry/blob/main/antigravity-acp/agent.json)

### Vérifications

* [ ] Connexion réalisée dans un premier conteneur ; premier conteneur arrêté ; connexion encore valide dans un nouveau conteneur utilisant le même état dédié. [Source](https://code.claude.com/docs/en/authentication)
* [ ] Aucun dossier personnel `.claude` ou `.gemini` du Mac n’est monté. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] Aucun token n’apparaît dans les logs, le rapport ou Git. [Source](https://code.claude.com/docs/en/authentication)
* [ ] Effectuer une petite requête réelle pour valider le service et le compte. Noter que ce test peut consommer du quota ; `initialize` seul ne valide pas l’accès au modèle. [Source](https://zed.dev/docs/ai/external-agents)

## 12. Phase F — Lancement, identification et arrêt

### Lancement

Développer les interfaces suivantes dans le CLI Rust. Au lancement, vérifier l’image active, le projet, l’état d’authentification et le runtime ; si nécessaire, démarrer le système Container puis vérifier son état. **Ne pas effectuer de mise à jour ni de build pendant le lancement d’un agent.** [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

```bash
zevbox launch claude --project ~/code/myproject
zevbox launch antigravity --project ~/code/myproject

zevbox status
zevbox doctor
zevbox shell claude --project ~/code/myproject
```

Chaque lancement reçoit un nom unique, par exemple `zevbox-claude-<uuid>`, et un enregistrement d’instance dans `~/Library/Application Support/zevbox/instances/` : nom du conteneur, PID du relais, agent, projet, image, heure de création et état. Le projet est monté au même chemin ; seul l’état de cet agent est ajouté. Ne pas publier de port réseau par défaut. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

Exemple de commande produite par le runner Rust via `tokio::process::Command` :

```bash
container run -i --rm \
  --name zevbox-claude-IDENTIFIANT \
  --cpus 4 \
  --memory 4G \
  --mount "type=bind,source=~/code/myproject,target=~/code/myproject" \
  --mount "type=bind,source=~/Library/Application Support/zevbox/state/claude,target=/home/agent" \
  -w ~/code/myproject \
  IMAGE_CLAUDE_VERROUILLEE \
  BINAIRE_CLAUDE_VERIFIE
```

`zevbox shell` utilise `-it` pour un shell interactif humain. Il doit annoncer clairement **« shell Linux zevbox »** et afficher le projet et la release. Il ne doit pas être confondu avec le terminal ordinaire de Zed. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

### Arrêt et nettoyage

Développer les commandes suivantes ; `stop --project` vise uniquement les instances enregistrées pour ce projet. `clean` supprime les résidus zevbox arrêtés, pas les projets, états d’authentification ou images conservées pour rollback. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

```bash
zevbox stop IDENTIFIANT
zevbox stop --project ~/code/myproject
zevbox clean
```

Sur fin de stdin, signal d’arrêt (`SIGINT`, `SIGTERM`) ou erreur fatale, le superviseur Rust doit tenter un arrêt ciblé avec timeout borné, et vérifier la disparition du conteneur. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

`doctor` doit détecter les instances orphelines en croisant les enregistrements locaux et l’état du runtime `container list` ; il ne doit pas faire confiance au seul préfixe d’un nom pour supprimer un conteneur. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

### Vérifications

* [ ] Deux lancements simultanés ne provoquent pas de collision de noms. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] `status` affiche réellement l’agent, le projet, la release et l’état, sans secret. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] Arrêt normal, fin de stdin et erreur fatale : conteneur arrêté puis supprimé. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] Après arrêt brutal simulé sur une instance de test, `doctor` détecte le résidu et `clean` peut le supprimer de manière ciblée. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] Les conteneurs préexistants de l’utilisateur restent intacts. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

## 13. Phase G — Intégration Zed sans perte de réglages

### Modification JSONC

Développer `zevbox zed install --settings CHEMIN`. Sauvegarder le fichier avec un horodatage précis et son empreinte dans `~/Library/Application Support/zevbox/backups/` ; calculer les modifications, vérifier qu’il n’a pas changé entre-temps, appliquer une édition JSONC minimale via `jsonc-parser` et écrire atomiquement. Ne modifier que les entrées zevbox gérées par l’outil. [Source](https://zed.dev/docs/languages/json) [Source](https://zed.dev/docs/ai/external-agents)

Exemple du bloc attendu pour le projet `myproject` : [Source](https://zed.dev/docs/ai/external-agents)

```json
{
    "agent_servers": {
        "zevbox · Claude · myproject": {
            "type": "custom",
            "command": "~/code/zevbox/target/release/zevbox",
            "args": [
                "launch",
                "claude",
                "--project",
                "~/code/myproject"
            ]
        },
        "zevbox · Antigravity · myproject": {
            "type": "custom",
            "command": "~/code/zevbox/target/release/zevbox",
            "args": [
                "launch",
                "antigravity",
                "--project",
                "~/code/myproject"
            ]
        }
    }
}
```

### Vérifications

* [ ] Le fichier JSONC reste valide et 100% de ses commentaires sont conservés. [Source](https://zed.dev/docs/languages/json)
* [ ] Les réglages sans rapport n’ont pas changé. [Source](https://zed.dev/docs/languages/json)
* [ ] Réexécuter l’installation ne duplique pas les entrées. [Source](https://zed.dev/docs/ai/external-agents)
* [ ] Zed affiche les agents `zevbox` et démarre le binaire compilé attendu. [Source](https://zed.dev/docs/ai/external-agents)
* [ ] Un lancement depuis un autre projet est refusé par le relais si son `cwd` ne correspond pas à la racine autorisée. [Source](https://agentclientprotocol.com/protocol/v1/session-setup.md)

## 14. Phase H — Tests de confinement et cycle de vie réel

### Tests sans modèle

Créer un dossier témoin temporaire sur le Mac, **extérieur au projet et non monté**, avec une valeur aléatoire. Le test doit vérifier son contenu et son intégrité depuis l’hôte, avant et après les opérations. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

* [ ] Le conteneur ne peut pas lire le témoin hôte à son chemin original. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] Une écriture au même chemin dans Linux, même si elle réussit dans son propre système de fichiers, ne modifie pas le témoin macOS. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] Une création dans le projet monté apparaît sur le Mac ; supprimer ensuite uniquement le fichier de test. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] Un lien symbolique de test dans le projet pointant vers le témoin extérieur ne révèle pas celui-ci au conteneur. [Source](https://agentclientprotocol.com/protocol/v1/file-system)
* [ ] Les montages effectifs ne contiennent que le projet et l’état dédié autorisé. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

### Tests dans Zed

Pour chaque agent, démarrer une session `zevbox`, demander une lecture et une modification bénigne du projet, puis demander une commande de diagnostic. Vérifier les résultats depuis `zevbox status`, le runtime et les fichiers hôte. [Source](https://zed.dev/docs/ai/external-agents) [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

```bash
uname -s
cat /etc/os-release
cat /etc/zevbox-release.json
pwd
```

Tester et consigner séparément les événements suivants : archivage du thread, changement de thread, création d’un second thread, fermeture de fenêtre, fermeture normale de Zed et arrêt explicite `zevbox stop`. [Source](https://agentclientprotocol.com/protocol/v1/session-setup.md) [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

## 15. Phase I — Mises à jour transactionnelles et retour arrière

### Interfaces à développer

```bash
zevbox update --check
zevbox update --apply
zevbox rollback
```

`update --check` consulte les sources officielles et affiche les changements proposés, sans modifier la release active. `update --apply` crée une candidate, télécharge ses composants, construit les images et lance les tests. La release active n’est remplacée **qu’après réussite des tests requis**. [Source](https://agentclientprotocol.com/rfds/acp-agent-registry) [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

Conserver des tags ou identifiants de release immuables. Garder au moins la release précédente, son verrouillage et sa configuration compatible. Une image candidate défectueuse ne doit pas casser les lancements existants. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

## 16. Phase J — Ajout d’outils dans la micro-VM

### Interfaces à développer

```bash
zevbox tools list
zevbox tools add apt ripgrep
zevbox tools add apt make
zevbox tools add apt build-essential
zevbox tools rebuild
```

`tools add` modifie une liste déclarative validée (`config/tools.json`). `tools rebuild` suit le même circuit candidate → tests → promotion qu’une mise à jour. L’ajout d’un outil ne doit pas créer un nouveau montage hôte. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)

## 17. Mode d’emploi quotidien

### Première utilisation

Après compilation et installation validées :

```bash
~/code/agentbox/target/release/zevbox doctor
~/code/agentbox/target/release/zevbox auth claude
~/code/agentbox/target/release/zevbox auth antigravity
~/code/agentbox/target/release/zevbox zed install
```

Ouvrir `~/code/myproject` dans Zed, puis sélectionner **`zevbox · Claude · myproject`** ou **`zevbox · Antigravity · myproject`** dans le panneau Agent. Zed lance `zevbox`, et `zevbox` démarre le conteneur en relayant ACP de façon sécurisée. [Source](https://zed.dev/docs/ai/external-agents)

### Pendant le travail

Pour vérifier l’exécution, utiliser `zevbox status` depuis un terminal macOS :

```bash
~/code/agentbox/target/release/zevbox status
~/code/agentbox/target/release/zevbox doctor
```

Pour un shell humain isolé dans la micro-VM :

```bash
~/code/agentbox/target/release/zevbox shell claude \
  --project ~/code/myproject
```

### À la fin du travail : arrêt certain

Pour libérer les ressources et arrêter formellement les micro-VMs associées au projet :

```bash
~/code/agentbox/target/release/zevbox stop \
  --project ~/code/myproject

~/code/agentbox/target/release/zevbox status
```

### Ce qui disparaît et ce qui persiste

| Élément | Après suppression du conteneur |
|---|---|
| Processus, mémoire et système Linux modifié à l’exécution | Disparaissent |
| Fichiers écrits dans le projet monté | Persistent sur le Mac |
| Authentification et état dans le dossier zevbox dédié | Persistent |
| Outils inclus dans l’image | Disponibles au prochain lancement |
| Outils installés seulement dans le conteneur jetable | Ne sont pas conservés |
| Images et sauvegardes de release | Restent jusqu’à un nettoyage explicite |

## 18. Critères finaux d’acceptation

* [ ] Le code de `zevbox` est compilé en Rust avec `cargo build --release` et `cargo clippy -- -D warnings` passe sans aucun avertissement.
* [ ] Aucun `unwrap()` ou `expect()` n'est utilisé dans le code de production du relais ou du superviseur.
* [ ] Aucun octet de log ou de diagnostic n'est émis sur `stdout` (réservé exclusivement à ACP).
* [ ] La suite de tests `cargo test` passe à 100% (sécurité du relais, robustesse du décodeur, préservation JSONC, signaux).
* [ ] Les deux agents ont un statut individuel : opérationnel, bloqué ou incompatible, avec preuve de test. [Source](https://zed.dev/docs/ai/external-agents)
* [ ] Le relais refuse les fichiers, terminaux et extensions hôte non autorisés, avec tests automatisés. [Source](https://agentclientprotocol.com/protocol/v1/file-system) [Source](https://agentclientprotocol.com/protocol/draft/schema)
* [ ] Les credentials Linux sont dédiés et persistants ; aucun dossier personnel complet n’est monté. [Source](https://code.claude.com/docs/en/authentication)
* [ ] Les réglages JSONC Zed sont sauvegardés et modifiés sans perte de commentaires ni de formatage. [Source](https://zed.dev/docs/languages/json)
* [ ] Les modifications du projet fonctionnent et les témoins extérieurs restent inchangés. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] L’arrêt explicite, le nettoyage ciblé et la détection des orphelins fonctionnent. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] Mise à jour échouée, mise à jour réussie et rollback ont été testés. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] L’ajout d’un outil passe par une reconstruction testée, sans nouveau montage. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] Les conteneurs préexistants de l’utilisateur n’ont pas été modifiés. [Source](https://github.com/apple/container/blob/1.5.0/docs/command-reference.md)
* [ ] Le README contient les commandes réellement implémentées, les comportements Zed réellement observés et les limites restantes. [Source](https://zed.dev/docs/ai/external-agents)
