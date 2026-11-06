# Spécification de Codex en ACP dans Zevbox

Statut : conception retenue et précisée par l'humain le 2026-11-06 ; faits
techniques mesurés en VM le même jour (§8.1) ; implémentation à appliquer. La
connexion doit être simple et fiable, sans exclusivité ChatGPT imposée ; ACP doit
rester à jour et compatible avec le Zed utilisé. Le plan d'exécution associé est
[PLAN_codex.md](PLAN_codex.md).

Codex devient un quatrième agent, avec le même parcours et les mêmes commandes
de gestion que Claude et Antigravity. Il utilise l'adaptateur ACP existant dans
une micro-VM Apple Container, derrière le relais strict de Zevbox. Les invariants
I1–I13 de [zevbox_plan_V2.md](../zevbox_plan_V2.md) s'appliquent intégralement.

## 1. Choix retenus et périmètre

| Sujet | Décision |
|---|---|
| Agent | Identifiant `codex`, nom affiché `Codex`, entrée `zevbox · Codex`. |
| Adaptateur | `@agentclientprotocol/codex-acp`, version stable exacte résolue dans le registre ACP ; aucune version `preview`. |
| Exécution | L'adaptateur et son Codex App Server tournent dans la même VM Linux arm64. |
| Installation | Agent standard (`opt_in: false`), comme Claude et Antigravity. `setup` construit sa release manquante et propose sa connexion ; `update` sans `--agent` le traite même s'il n'a jamais été installé. Le quick start `auth codex` et le ciblage `update --agent codex` restent disponibles. |
| Connexion principale | `zevbox auth codex` utilise le CLI Codex embarqué, avec connexion ChatGPT par code d'appareil dans une VM jetable et stockage Linux dédié. Le login tourne dans un `CODEX_HOME` séparé : il ne remplace le compte actuel qu'après réussite (§5). |
| Autres modes natifs | Pas d'exclusivité ChatGPT : un cache natif valide par clé API est reconnu. Pas d'assistant de saisie de clé dans ce chantier, et Zed ne peut pas transmettre de clé à travers le relais (§5). Pas de bascule silencieuse. L'accès aux modèles et la facturation dépendent du mode choisi. |
| Version ACP | Dernière version stable de l'adaptateur, protocole négocié compatible avec le Zed utilisé et couvert par le relais. La v1 observée aujourd'hui n'est pas un plafond permanent ; voir §6. |
| Interface | CLI, entrée Zed et console EN/FR, avec les mêmes mécanismes de confirmation et de diagnostic que les agents existants. |
| Modèles et permissions | Sélecteurs natifs de l'adaptateur utilisables depuis Zed lorsqu'ils sont pris en charge ; aucune liste de modèles codée dans Zevbox. |

Codex participe à `setup`, `update`, `tools`, `doctor`, aux listes de la console
et à la gestion des instances selon les mêmes règles que Claude et Antigravity.
Une release Codex absente déclenche sa construction dans le parcours `setup` ;
si elle manque toujours à la fin, `setup` est incomplet avec le même code
d'erreur que pour les deux autres agents standards. L'échec d'un agent ne
bloque pas le traitement des autres ; la proposition de connexion peut être
passée selon le parcours existant. oMLX conserve seul son statut optionnel.

Le comportement partagé de `setup` est conservé : dès qu'une release standard
manque, il appelle la mise à jour globale. Ajouter Codex à une installation
existante par `setup` reconstruit donc aussi les autres agents standards et
oMLX s'il est déjà installé. Il ne construit pas seulement l'agent manquant.
Sans release standard manquante, il ne relance pas cette construction.
`update` sans sélection inclut Codex dès l'état initialisé sans releases ;
`setup` et `auth codex` assurent eux-mêmes l'initialisation si nécessaire.

L'équivalence demandée porte sur les fonctions déjà offertes par Zevbox et
l'usage ACP courant : dialogue, contexte transmis par Zed, lecture et édition
dans le projet, commandes Linux, permissions, modèles, annulation et sessions
persistées lorsque l'adaptateur les prend en charge. Elle ne promet pas toutes
les extensions de l'application Codex : nouvelles méthodes privées, MCP fournis
par Zed, dossiers supplémentaires et délégations à l'hôte restent soumis au
contrat strict actuel. Un écart sur l'usage courant bloque l'acceptation et doit
être expliqué, pas masqué comme une réussite.

## 2. Réutilisation des implémentations existantes

| Exemple du dépôt | Réutilisation pour Codex |
|---|---|
| `src/agents.rs`, entrées Claude/Antigravity | Quatrième entrée, ajoutée après oMLX pour conserver l'ordre existant ; `opt_in: false` comme les deux agents standards. |
| `assets/Containerfile`, étape Claude | Nouvelle étape npm `codex` depuis la même base Ubuntu ; installation dans `/opt/zevbox/codex`, hors `/home/agent` ; `NO_BROWSER=1` et suppression du `bwrap` d'Ubuntu (§6). |
| `src/release.rs`, parseur Claude et contexte npm | Résolution du manifeste Codex et génération de son `package.json` exact, sans détourner le parseur Claude ni le chemin Antigravity. |
| `src/cli/update.rs` | Même pipeline candidat, build, smoke test, promotion, conservation de `previous`, ménage et synchronisation Zed. |
| `src/cli/auth.rs`, `prepare` et auth Claude | Même préparation automatique et VM d'auth sur dossier vide ; adapter les champs actuellement propres à Claude. |
| `src/cli/auth.rs`, auth Antigravity | Même principe d'un statut d'auth spécifique partagé entre `auth --status`, `launch`, `setup` et `doctor`. Son OAuth sur le Mac n'est pas à recopier. |
| `src/zed_settings.rs` | Édition CST existante, une entrée par release active ; conservation des réglages de modèle et mode de Zed. |
| `src/supervisor.rs`, `src/container.rs`, `src/guard.rs`, `src/relay/` | Chemin de lancement, montages, protections et arrêt existants. Aucun relais Codex parallèle. |
| `src/console/menu.rs`, actions de connexion | Action Codex interactive et confirmation si déjà connecté ; mêmes sélecteurs et même exécuteur de commandes. |
| `tests/omlx_test.rs`, `tests/r3_mounts_test.rs` | Modèles pour de nouveaux tests Codex utilisant `common::TestEnv` et le faux agent, sans modifier les fixtures protégées. |

Les signatures publiques utilisées par les tests sont conservées. Une extraction
privée et limitée pour partager du code npm ou construire la VM d'auth est
possible ; pas de nouveau framework d'agents, de dépendance Rust ni de refonte
des autres agents pour ce sujet.

## 3. Parcours utilisateur attendu

Première utilisation, depuis n'importe quel dossier :

```bash
zevbox auth codex
zevbox auth codex --status
# Ouvrir un projet autorisé dans Zed, puis choisir « zevbox · Codex » avec +.
```

`auth codex` réutilise `prepare` : `init` si nécessaire, démarrage du runtime,
construction si la release ou son image manque. Le code d'appareil et son URL
s'affichent dans le terminal ; l'utilisateur valide la connexion dans le
navigateur du Mac. Aucun callback réseau vers la VM n'est nécessaire. Un nouvel
appel ne reconstruit pas une image déjà disponible. Une connexion déjà présente
est réutilisée par les sessions suivantes. Un appel explicite à `auth codex`
permet aussi de se reconnecter : un cache local bien formé mais révoqué à
distance ne doit pas empêcher le lancement du login natif.

Après connexion, la commande vérifie le résultat et indique la prochaine action
dans Zed. Une erreur ou une annulation ne détruit pas un ancien état valide. La
connexion par code d'appareil est une fonction bêta d'OpenAI : son activation dans
les réglages de sécurité du compte, ou par un administrateur de workspace, est
expliquée avec une action corrective, sans repli silencieux sur une clé API ou une
ouverture de port. Mesuré le 2026-11-06 : la VM affiche
`https://auth.openai.com/codex/device` et un code valable 15 minutes.

Usage quotidien :

```bash
zevbox status
zevbox logs                    # ou logs <ID>, logs --path
zevbox stop <ID>               # ou stop, stop --project <PATH>
zevbox update --check --agent codex
zevbox update --agent codex
zevbox rollback --agent codex
zevbox shell codex --project <PATH>
zevbox doctor                  # ou doctor --json
zevbox console --lang fr        # ou --lang en
```

`zed install`, son aperçu et sa désinstallation, `clean [--images]` et
`tools list/add/remove` prennent en compte Codex par les mécanismes existants.
Une mise à jour ou un rollback ne remplace pas les sessions déjà en cours.
Les protections `.git` restent applicables : l'agent peut lire l'historique,
mais ne peut pas committer dans le `.git` racine protégé.

## 4. Image et release

| Champ | Valeur attendue |
|---|---|
| `registry_id` | `codex-acp` |
| `build_target` | `codex` |
| `image_repo` | `zevbox/codex` |
| `npmPackage` | `@agentclientprotocol/codex-acp@<version stable exacte>` |
| `agentCommand` | `/opt/zevbox/codex/node_modules/.bin/codex-acp` |
| `authCommand` | `env`, `CODEX_HOME=/home/agent/.codex-login`, puis le même exécutable, `cli`, `-c`, `cli_auth_credentials_store="file"`, `login`, `--device-auth`. Syntaxe et démarrage du flux vérifiés en VM. |
| `authMarker` | `.codex/auth.json` (vérifié en VM). |
| Image | `ENV NO_BROWSER=1` ; `/usr/bin/bwrap` supprimé, indispensable au sandbox de Codex (§6). |
| État hôte | `stateRoot/state/codex`, monté en `/home/agent`. |

Le parseur Codex exige l'identité `codex-acp`, une version stable non vide et
le nom de paquet exact associé à cette version. `preview` est ignoré. Un schéma
incompatible produit une erreur actionnable et laisse la release active intacte.

La construction suit le modèle Claude : version exacte de l'adaptateur dans
`package.json`, résolution des dépendances npm pendant le build, dépendances
optionnelles conservées pour le binaire Linux arm64. Le Codex livré avec le
paquet est utilisé ; pas de résolution indépendante de `@openai/codex@latest`,
ni de téléchargement npm à `launch`. Relever aussi la version réellement
installée de Codex et conserver la preuve avec la release : la version exacte de
l'adaptateur ne fige pas à elle seule toutes ses dépendances transitives.
Mesurer le moteur en exécutant directement son binaire embarqué dans la VM :
dans le paquet 2.5.0 étudié, `codex-acp cli --version` est intercepté par
l'adaptateur et affiche sa propre version, pas celle de Codex. Mesuré :
`node_modules/.bin/codex --version` affiche `codex-cli 0.165.0`.
Le schéma des anciens locks reste lisible ; aucun nouveau champ obligatoire.

Les smoke tests existants restent ceux du contrat V2. La qualification Codex
ajoute des vérifications explicites des exécutables et du protocole, sans
réintroduire un smoke test ACP général pour tous les agents (décision du
2026-10-29). Le plan distingue ces vérifications des tests avec le faux agent.

## 5. Authentification et confidentialité

Le flux visé utilise le passage vers le CLI embarqué : `codex-acp cli … login
--device-auth`. La commande distincte `codex-acp login` du tag étudié utilise un
flux navigateur avec callback local ; elle ne convient pas à ce parcours VM.

L'auth tourne en `-it`, avec le même UID/GID que les autres agents. Elle ne
monte que le dossier vide jetable et `state/codex`. Généraliser le constructeur
d'auth derrière sa signature publique actuelle, ou ajouter un constructeur
Codex partageant l'assemblage, pour éviter les noms, labels et chemins Claude
codés en dur. Ni configuration de modèle Claude ni instructions Claude ne sont
appliquées à Codex.

Le login tourne avec `CODEX_HOME=/home/agent/.codex-login`. En effet,
`codex login` commence par révoquer à distance puis supprimer la connexion qu'il
trouve dans son `CODEX_HOME` (`clear_existing_auth_before_login`, Codex 0.165.0).
Lancé sur le vrai `.codex`, un échec ou une annulation ferait perdre un compte
valide, et une sauvegarde suivie d'une restauration rendrait des jetons déjà
révoqués. Zevbox crée donc un dossier de login vide, vérifie le `auth.json` obtenu,
le renomme sur `state/codex/.codex/auth.json` seulement s'il est complet, puis
supprime le dossier de login. Mesuré le 2026-11-06 : Ctrl-C pendant l'attente du
code laisse le cache intact, sans dossier de login ni VM restants.

Le HOME Linux dédié place les données sous `/home/agent/.codex`. Le stockage
fichier est le mode par défaut de Codex 0.165.0 ; sa relecture par une nouvelle VM
est mesurée. `state/codex` est en `0700`. Codex crée `.codex` en `0755` et
`auth.json` en `0600` ; Zevbox remet `.codex` en `0700` quand il y place une
nouvelle connexion. Ne lire ni copier ni monter le `~/.codex` du Mac et ne toucher
à aucun Trousseau. Pas de token dans argv, `-e`, les captures, les journaux ou les
preuves.

`is_authenticated` valide un format natif d'authentification reconnu, pas la
seule existence du fichier : `auth_mode` vaut `apikey` avec `OPENAI_API_KEY` non
vide, ou `chatgpt` avec `tokens.id_token`, `tokens.access_token` et
`tokens.refresh_token` non vides (structure `AuthDotJson` de Codex 0.165.0 ; le
format API est mesuré). Un cache vide, cassé, incomplet ou d'un autre mode est
refusé ; un cache API valide ne l'est pas au seul motif qu'il utilise une clé.
Ce contrôle est local ; il ne garantit ni la validité distante des identifiants
ni le quota disponible. Le rafraîchissement appartient à Codex. Aucun appel
réseau de validation n'est ajouté dans `launch` ou `auth --status`.

Comme les autres agents, deux instances Codex partagent l'état du même agent.
Le risque de rafraîchissement concurrent reste la limite décrite par V2 §11 ;
cette extension n'ajoute pas de gestion de comptes multiples ou de daemon.

Le parcours garanti par Zevbox est la commande terminal `zevbox auth codex`,
également proposée par `setup` et la console, comme pour les autres agents.
Le paquet 2.5.0 masque son login navigateur avec `NO_BROWSER=1`, mais conserve
`api-key` dans `authMethods`. Cette liste décrit les logins que le client peut
déclencher, pas les comptes déjà disponibles dans le cache lu par App Server.

Qualification de `api-key`, mesurée le 2026-11-06 : Zed 1.25.3 envoie
`authenticate` avec le seul `methodId`. L'adaptateur cherche alors la clé dans
`CODEX_API_KEY` ou `OPENAI_API_KEY`, que Zevbox ne passe jamais à la VM. Ce
parcours ne peut donc pas aboutir. Le relais masque `api-key` pour Codex seulement,
et une demande `authenticate` pour cette méthode reçoit le renvoi existant vers
`zevbox auth codex`. La liste vide qui en résulte aujourd'hui découle de cette
qualification, pas d'un principe : les autres méthodes passent toujours par le
filtrage commun, et le masquage est à requalifier si Zed transmet un jour une clé.
Il n'y a pas de refus global d'`authenticate`, et un cache API valide reste accepté.

Un parcours clé API non guidé existe, à qualifier en C4 seulement si l'humain le
souhaite : `zevbox shell codex --project <PATH>`, puis
`/opt/zevbox/codex/node_modules/.bin/codex login --with-api-key`, la clé étant lue
sur l'entrée standard. Mesuré avec une clé synthétique : cache écrit en mode
`apikey`, relu par une nouvelle VM. Le login par code d'appareil de l'adaptateur
ACP demande l'elicitation URL ; ne pas annoncer cette capacité interdite par I3.

L'adaptateur possède une journalisation du trafic App Server. Dans le paquet
2.5.0, `APP_SERVER_LOGS` absent ou vide la désactive ; `Logger.error` peut
néanmoins écrire des erreurs brutes sur stderr, donc dans le journal Zevbox.
Mesuré sur un échec provoqué (clé synthétique refusée) : le journal reçoit la pile
d'erreur de l'adaptateur avec l'identifiant de session, sans le texte du prompt ni
la clé. Vérifier aussi les éventuels exports de télémétrie en C4 ; ne pas supprimer
tout stderr pour dissimuler une fuite. L'historique privé que Codex persiste dans
son propre état (`.codex/sessions`, bases SQLite) contient les prompts ; il est
distinct des journaux Zevbox et se documente comme l'état des autres agents.

## 6. ACP et permissions

```text
Zed sur le Mac
  → zevbox launch codex
    → relais ACP strict existant
      → VM Linux arm64
        → codex-acp
          → Codex App Server
            → outils et commandes dans la VM
```

La version du paquet adaptateur, celle de son moteur Codex, celle de Zed et la
version du protocole ACP sont quatre informations distinctes. Au relevé du
2026-11-06, Zed installé est en 1.25.3 ; les connexions applicatives de ce tag
envoient `ProtocolVersion::V1`, malgré la présence de code préparatoire v2.
L'adaptateur Codex ACP 2.5.0 sait traiter v1 et v2. La migration ACP v2 est
encore décrite comme un brouillon par les sources consultées (§8). Le premier
parcours à prouver est donc v1 avec l'adaptateur stable courant, sans en déduire
que Codex ou Zevbox doivent rester sur d'anciennes versions.

La négociation réelle à travers le relais est mesurée : v1 dans les deux sens. La
requalifier lors d'une évolution de Zed, de l'adaptateur ou du protocole. Le relais
actuel réécrit `clientCapabilities` de v1, mais ne contrôle `protocolVersion` nulle
part ; v2 change notamment le schéma des capacités, l'authentification et le cycle
des prompts. Sa prise en charge ne se résume pas à autoriser le nombre `2`. Une
version non couverte doit produire un diagnostic actionnable, jamais un passage non
filtré ni une réécriture silencieuse de la version : l'étape C2a du plan ajoute ce
refus, commun à tous les agents, en requête comme en réponse. Si la pile retenue requiert une évolution du
protocole, adapter ensemble politique, tables du contrat et tests avant
l'activation de Codex, comme sujet distinct respectant I1–I13 et les agents
existants. Aucune migration spéculative n'est requise pour un brouillon.

Les appels client `fs/*`, `terminal/*`, `elicitation/*`, l'auth terminal, les
extensions inconnues, les dossiers supplémentaires et les MCP transmis par Zed
restent bloqués ou neutralisés par les règles actuelles. Les sorties et diffs
passent par les mises à jour de session ; les décisions utilisateur passent par
`session/request_permission`. La présence d'un événement « terminal » affiché
dans Zed ne doit pas être confondue avec une exécution `terminal/create` sur le Mac.

Sans elicitation de formulaire, le gestionnaire `handleUserInput` du tag 2.5.0
renvoie des réponses vides : ses questionnaires structurés ne sont pas affichés.
Qualifier l'effet sur le dialogue et le mode de planification natif, et vérifier
qu'une précision peut être demandée par texte. Cette limite doit être expliquée
dans les guides ; un blocage du dialogue courant empêche l'acceptation. Elle
ne justifie pas une ouverture globale d'`elicitation/*`.

Conserver le mode initial natif de la version qualifiée et vérifier son
fonctionnement avec `--cap-drop ALL` et les protections I13. Le mode `agent`
du tag 2.5.0 est « Auto review » : sandbox `workspaceWrite`, sans réseau. Ne pas
imposer `agent-full-access` pour contourner un problème de sandbox. Les choix de
modèle, effort de raisonnement et permissions disponibles sont présentés par
l'adaptateur, avec conservation des `default_config_options` par l'éditeur CST
existant. Mesuré : `session/new` ouvre en mode `agent` et propose `read-only`,
`workspace-write`, `agent` et `agent-full-access`, ainsi que les réglages `mode`,
`collaboration_mode`, `model`, `reasoning_effort` et `fast-mode`.

**Sandbox de Codex dans la VM, mesuré le 2026-11-06.** Apple Container masque des
sous-chemins de `/proc` ; `bwrap` ne peut donc pas y monter un nouveau `/proc`.
Codex sait se replier sans nouveau `/proc`, mais il ne reconnaît l'échec que sous
la forme écrite par son propre `bwrap` (`/newroot/proc`). Le `bubblewrap` 0.12.0
d'Ubuntu, tiré par `glycin-loaders`, écrit `/proc` : le repli ne se déclenche pas,
et toute commande échoue, en lecture seule comme en écriture. Sans `/usr/bin/bwrap`,
Codex utilise le `bwrap` qu'il embarque et le repli fonctionne : écriture limitée au
projet, `/opt` et `/home/agent` protégés, réseau coupé, node, python, git, npm, rg,
gcc, make et venv opérationnels. Seul `ps` échoue dans ce sandbox. L'image Codex
supprime donc `/usr/bin/bwrap`. Au démarrage, Codex consigne
`Codex could not find bubblewrap on PATH` : ce message est attendu.

## 7. Critères d'acceptation

| Domaine | Résultat requis |
|---|---|
| Installation | Codex standard dès l'activation publique ; `setup` et `update` l'incluent sans sélection explicite ; quick start `auth codex` ; migration d'une installation existante et relance sans release manquante vérifiées. |
| Connexion | Parcours ChatGPT par code d'appareil, réutilisation dans une nouvelle VM et reconnexion d'un cache révoqué ; échec/annulation compréhensibles ; reconnaissance des modes natifs qualifiés sans exclusivité imposée. |
| ACP | Versions et négociation relevées ; protocole traité par le relais ; méthodes d'auth réellement proposées utilisables ou renvoi explicite vers le terminal ; aucun rejet global des modes natifs. |
| Zed | Une entrée Codex seulement si release active ; choix conservés ; dialogue, précisions textuelles, fichier joint, image si supportée, lecture, édition, commande Linux, permission et annulation. Limites des questionnaires documentées. |
| Sessions | Reprise d'une session persistée dans le même projet après relance ; deux threads et archivage observés sans promettre un conteneur par thread. |
| Isolation | Deux montages, cwd verrouillé, `.zed` masqué, `.git` protégé, aucun outil hôte via ACP ; témoins hors projet intacts ; sandbox natif de Codex fonctionnel (`verify_image.sh`). |
| Cycle de vie | EOF, signal, crash et `SIGKILL` suivis d'une disparition ou d'un nettoyage borné du conteneur selon le contrat ; autres agents intacts. |
| Releases | Échec de build sans changement actif, mise à jour ciblée puis rollback prouvés ; autres releases inchangées par le ciblage ; outils supplémentaires utilisables. |
| Gestion | Statut, journaux, arrêt ciblé, ménage limité à sa racine, shell, diagnostics texte/JSON et setup idempotent. |
| Console | Connexion Codex interactive en EN/FR, état rafraîchi, confirmation de reconnexion, présence dans les sélecteurs et restauration du terminal. |
| Livraison | Tests et invariants verts, fichiers protégés approuvés et rescellés uniquement par l'humain ; tous les documents concernés, dont les guides EN/FR, à jour dans chaque commit de comportement ; revue documentaire finale du dépôt selon PLAN §8. |

## 8. Sources et faits à qualifier

Sources consultées le 2026-11-06. Revalider la version stable au début de
l'exécution (PLAN C0) ; les scripts de `docs/codex_annex/` requalifient une
nouvelle version.

### 8.1 Mesures du 2026-11-06

Mesures faites dans de vraies VM Apple Container, avec les options de
`zevbox launch` (`--cap-drop ALL`, état monté en `virtiofs`), sans compte ni clé
réelle. Environnement : Zed 1.25.3, `container` 1.5.0, Ubuntu 26.10, Node 24.19,
adaptateur 2.5.0, moteur `codex-cli` 0.165.0.

- Installation npm en quelques secondes ; `.bin/codex-acp` et `.bin/codex` présents.
- Sans compte : `initialize` v1, `authMethods` limité à `api-key`, puis `session/new`
  refusé par `-32000 Authentication required`.
- Cache API synthétique : `{"auth_mode":"apikey","OPENAI_API_KEY":…}`, relu par une
  nouvelle VM ; la session s'ouvre ; un prompt reçoit le renvoi vers `zevbox auth codex`.
- Sandbox : en panne avec le `bwrap` d'Ubuntu, opérationnel sans lui (§6).
- Deux VM simultanées sur le même état : sessions ouvertes, bases SQLite intègres
  (essai court, à confirmer sur un vrai usage).
- Login : démarrage du code d'appareil dans la VM ; Ctrl-C conserve le cache (§5).
- Relais : v2 refusée, `api-key` masqué, arrêt sur EOF en 2,3 s sans VM restante,
  marqueur et clé absents des journaux.
- Pipeline complet en installation jetable : construction et promotion en 1 min 32,
  deuxième release, rollback aller-retour.

### 8.2 Sources

- [Registre Codex ACP](https://raw.githubusercontent.com/agentclientprotocol/registry/main/codex-acp/agent.json) : stable 2.5.0 lors de la consultation, distribution npm.
- [Ancien dépôt Zed](https://github.com/zed-industries/codex-acp) : développement déplacé vers `agentclientprotocol/codex-acp`.
- [Paquet du tag 2.5.0](https://github.com/agentclientprotocol/codex-acp/blob/v2.5.0/package.json) et [métadonnées npm publiées](https://registry.npmjs.org/@agentclientprotocol/codex-acp/2.5.0) : dépendance `@openai/codex` déclarée `^0.165.0` ; archive publiée inspectée sans installation, intégrité sha512 conforme aux métadonnées.
- [Entrée du tag 2.5.0](https://github.com/agentclientprotocol/codex-acp/blob/v2.5.0/src/index.ts) et [passage CLI](https://github.com/agentclientprotocol/codex-acp/blob/v2.5.0/src/CodexCli.ts) : sous-commande `cli` relayée vers le Codex embarqué, mais `--version` intercepté avant ce passage.
- [Login du tag 2.5.0](https://github.com/agentclientprotocol/codex-acp/blob/v2.5.0/src/login.ts) : flux navigateur de `codex-acp login`.
- [Méthodes d'auth du tag 2.5.0](https://github.com/agentclientprotocol/codex-acp/blob/v2.5.0/src/CodexAuthMethod.ts) et [client App Server](https://github.com/agentclientprotocol/codex-acp/blob/v2.5.0/src/CodexAcpClient.ts) : `NO_BROWSER` masque le login navigateur et laisse `api-key` ; device login ACP conditionné à l'elicitation URL ; relecture du compte distincte de la liste des logins.
- [Connexion App Server du tag 2.5.0](https://github.com/agentclientprotocol/codex-acp/blob/v2.5.0/src/CodexJsonRpcConnection.ts) et [logger](https://github.com/agentclientprotocol/codex-acp/blob/v2.5.0/src/Logger.ts) : trafic complet conditionné par `APP_SERVER_LOGS`, erreurs stderr à qualifier séparément.
- [Routeur ACP 2.5.0](https://github.com/agentclientprotocol/codex-acp/blob/v2.5.0/src/AcpAgentRouter.ts), [connexion Zed 1.25.3](https://github.com/zed-industries/zed/blob/v1.25.3/crates/agent_servers/src/acp.rs#L1026) et [migration ACP v2](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/protocol/v2/migration.mdx) : support v1/v2 côté adaptateur, v1 demandée par ce Zed, v2 encore en brouillon lors de la revue. Revalider ce dernier document évolutif.
- [Elicitation du tag 2.5.0](https://github.com/agentclientprotocol/codex-acp/blob/v2.5.0/src/CodexElicitationHandler.ts) : `handleUserInput` rend des réponses vides sans capacité de formulaire.
- [Modes du tag 2.5.0](https://github.com/agentclientprotocol/codex-acp/blob/v2.5.0/src/AgentMode.ts) : mode initial natif et options de permissions.
- [Authentification OpenAI](https://learn.chatgpt.com/docs/auth#login-on-headless-devices) : code d'appareil pour environnement sans navigateur, activation selon compte/workspace, cache fichier sous `CODEX_HOME` et rafraîchissement des jetons.

- [Codex 0.165.0, login](https://github.com/openai/codex/blob/rust-v0.165.0/codex-rs/cli/src/login.rs), [révocation](https://github.com/openai/codex/blob/rust-v0.165.0/codex-rs/login/src/auth/manager.rs) et [stockage](https://github.com/openai/codex/blob/rust-v0.165.0/codex-rs/login/src/auth/storage.rs) : `clear_existing_auth_before_login` avant tout login, structure `AuthDotJson`, fichier en `0600`, stockage fichier par défaut.
- [Codex 0.165.0, sandbox Linux](https://github.com/openai/codex/blob/rust-v0.165.0/codex-rs/linux-sandbox/src/linux_run_main.rs) : repli sans nouveau `/proc` reconnu seulement sur le message `/newroot/proc` ; préférence pour le `bwrap` du système.
- [Zed 1.25.3, `authenticate`](https://github.com/zed-industries/zed/blob/v1.25.3/crates/agent_servers/src/acp.rs#L2054) : requête envoyée avec le seul `methodId`.

Les mesures du §8.1 couvrent l'installation Linux arm64, les droits du cache, le
comportement sans capacités hôte, le sandbox et la journalisation. La connexion
ChatGPT réelle, l'usage d'un modèle et le comportement dans l'interface de Zed
restent à prouver en C4 ; la lecture des sources ne les remplace pas.

## 9. Parité et différences avec Claude et Antigravity

La parité porte sur les services rendus par Zevbox. Elle n'impose pas aux trois
moteurs les mêmes modèles, algorithmes, commandes internes ou formats d'état.
Les éléments ci-dessous distinguent les différences prévues des compatibilités
qui restent à prouver en C0–C4.

| Domaine | Claude actuel | Antigravity actuel | Codex selon ce plan |
|---|---|---|---|
| Installation initiale | Agent standard, traité par `setup`/`update`. | Agent standard, traité par `setup`/`update`. | Même règle : agent standard, construit par `setup` si manquant et traité par `update` sans sélection explicite. |
| Connexion | CLI dans une VM jetable ; URL ouverte manuellement sur le Mac puis code du navigateur collé dans le terminal. | OAuth PKCE géré par Zevbox sur le Mac ; navigateur et callback HTTP local éphémère. | CLI dans une VM jetable, dans un `CODEX_HOME` séparé ; code affiché dans le terminal et saisi dans la page du navigateur. Fonction bêta, à activer parfois côté compte ou workspace. |
| Authentification locale | Existence de `.claude/.credentials.json`. | Jeton de rafraîchissement non vide dans `.gemini/antigravity-acp/acp_token.json`. | Cache natif `.codex/auth.json` en mode `chatgpt` ou `apikey` complet ; pas de simple contrôle d'existence ni d'exclusivité imposée. Aucun de ces contrôles ne prouve le quota ou la validité distante du compte. |
| Moteur dans la VM | Adaptateur ACP Claude et SDK/moteur Claude Code. | Serveur ACP natif distribué en archive `.par`. | Adaptateur ACP Node et Codex App Server enfant. La communication entre ces composants reste interne à la même VM. |
| Construction | Version exacte d'adaptateur npm ; dépendances résolues au build. | Archive Linux arm64 téléchargée et vérifiée par sha256. | Version exacte d'adaptateur npm, avec sa dépendance Codex compatible ; relever aussi la version Codex résolue. Même modèle de build que Claude, pas le modèle d'archive Antigravity. |
| Permissions internes | Modes Claude, notamment le mode Auto observé dans Zed. | Choix offerts par son serveur ACP. | Modes natifs de Codex, dont Auto review dans la version étudiée ; même nom « Auto » ne signifie pas même politique ni même fréquence d'interruption. |
| Modèles et réglages | Catalogue et options Claude. | Catalogue et options du serveur Antigravity. | Catalogue du compte Codex et effort de raisonnement si exposé par l'adaptateur et Zed. Aucun catalogue commun imposé par Zevbox. |
| Sessions et instructions | État, historique et conventions Claude dans son dossier dédié et le projet. | État, historique et conventions propres à Antigravity. | État, historique et conventions Codex, dont `AGENTS.md` et les réglages natifs pris en charge. Pas de conversion des anciens threads Claude/Antigravity vers Codex. |
| Documentation livrée | Parcours détaillé EN/FR dans les documents communs. | Même couverture, avec sa section OAuth. | Même couverture et même niveau de preuve, avec les gestes du code d'appareil et les nuances Codex. Aucune doc utilisateur à laisser « pour plus tard ». |

Le traitement des instructions, des skills, des hooks et des outils reste celui
de chaque moteur. Le plan ne construit pas de couche traduisant automatiquement
une configuration Claude ou Antigravity en configuration Codex. Les options
proposées dans Zed, commandes slash, messages d'erreur de fournisseur, limites
de contexte, compactage et précision des diffs peuvent donc différer. Les
fonctions ACP courantes de la SPEC §1 doivent fonctionner et être vérifiées ;
les extensions propres à un moteur ne deviennent pas des fonctions garanties
par le seul ajout d'une entrée dans la table des agents.

La frontière de protection du Mac reste celle de Zevbox : deux montages, mêmes
zones de projets, relais strict, `.zed` masqué, `.git` protégé, utilisateur non
root et arrêt surveillé. Le sandbox et les autorisations internes de Codex
constituent une couche dans la VM. Son choix Full access ne donne aucun montage
supplémentaire ni droit ACP vers le Mac ; il réduit les restrictions internes
du moteur. Ne pas présenter cette couche comme une preuve de sécurité
supérieure ou équivalente sans qualification de son fonctionnement réel.

Le risque fournisseur n'est pas uniformisé : politiques de données, comptes,
quota, disponibilité et comportement des outils restent propres aux services.
Les trois agents peuvent accéder au projet monté, à leurs propres identifiants
et au réseau dans la limite de leurs restrictions internes. Le risque accepté
sur les fichiers de projet exécutés par Zed (I13, décision P1) ne disparaît pas
avec Codex. L'extension ajoute aussi ses propres dépendances et modes de panne :
leur audit et les essais de journaux, d'arrêt et de permissions sont des
conditions d'acceptation, pas une garantie déjà acquise.

Les ressources VM gardent les réglages communs de Zevbox. Cela ne garantit
aucune égalité de temps de réponse, consommation mémoire, taille d'image ou
coût d'usage. Ces propriétés et la reprise des sessions doivent être observées
sur la version qualifiée. L'acceptation C4 doit rapporter les écarts pratiques
au lieu de conclure à une identité complète des trois agents.
