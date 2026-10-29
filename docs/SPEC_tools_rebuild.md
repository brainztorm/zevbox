# Spécification — `zevbox tools add/remove` reconstruit les images aux versions actuelles

- **Backlog** : `docs/DECISIONS.md`, point 1 (« R3 — `tools add/remove` sans résolution du registre », revue ARGON 3.1).
- **Statut** : spécification du 2026-10-29, relue contre le code (revue adverse, 18 remarques
  intégrées). Pas encore implémentée.
- **Contrat visé** : `zevbox_plan_V2.md` §8.7, qui dit déjà « Pipeline `update` (et `tools add/remove`,
  qui réutilisent les versions actuelles au lieu de résoudre) ». Le code ne le fait pas : cette
  spécification le met en conformité et précise ce que le plan laisse dans le vague.
- **Lecteur visé** : l'agent qui implémentera, avec seulement `AGENTS.md`, le plan, ce fichier et le
  code. Les références `fichier:ligne` datent du 2026-10-29 (HEAD `3effd7b`, lignes de `release.rs` mises à jour après le point 12) : vérifier avec `grep`
  avant d'éditer.

« Doit » = exigence. Chaque exigence a un identifiant `T-xx`, repris par les preuves (§8).

---

## 1. Problème

`zevbox tools add <paquets>` / `remove <paquets>` modifie `tools.json`, puis appelle
`update::run_update(false, Some(agent))` pour chaque agent actif (`src/cli/tools.rs:42` et `:63`).
Or `run_update` **re-résout le registre ACP** (`fetch_manifest`, `release.rs:427-453`, branche `main`
de `agentclientprotocol/registry`) : ajouter un outil peut faire passer Claude de 0.99.0 à 0.100.0
ou Antigravity de 1.4.4 à 1.5.0 sans que l'utilisateur l'ait demandé, avec de possibles nouvelles
méthodes ACP non auditées (`relay/policy.rs`).

Défauts du même chemin, corrigés dans le même changement :

1. `tools.json` est enregistré **avant** la reconstruction et n'est pas remis en état si elle échoue :
   le fichier et les images divergent (`cli/tools.rs:31-44`).
2. Le premier agent en échec arrête les suivants ; ceux déjà reconstruits sont promus avec la
   nouvelle liste, les autres non.
3. Un `tools.json` illisible est traité comme vide (`unwrap_or_default()` dans `cli/tools.rs` et
   `cli/update.rs:22`), puis **écrasé** par `add`/`remove` : la liste est perdue sans message.
4. Rien ne vérifie que le service Apple Container tourne : le registre est contacté, puis la
   construction échoue avec un message technique.
5. Une fois le fichier et les images divergents, `tools add X` répond `no new tools to add` et ne
   répare rien.

## 2. Objectif, définitions, limites

- **Release active** d'un agent : `releases/active.json` → `<agent>.current` → `releases/<id>/lock.json`.
- **Reconstruction** : construire une **nouvelle release** dont toutes les entrées sont **reprises du
  `lock.json` de la release active**, sauf la liste apt, qui devient celle de `tools.json`. Aucun appel
  au registre ACP.
- **Même version** : même `agentVersion`, même `npmPackage` et mêmes dépendances npm transitives quand
  le verrou npm est disponible (Claude, oMLX), même `archiveUrl` + `archiveSha256` (Antigravity), même
  `baseImage`, mêmes `agentCommand`/`authCommand`/`authMarker`, mêmes `registryManifestUrl`/`Sha256`.

**Limites, à écrire telles quelles dans les docs utilisateur :**

- **Le réseau reste nécessaire.** Seule la résolution du registre ACP disparaît. La construction
  télécharge toujours l'image de base (sauf si elle est en cache), les paquets npm (`npm ci`), les
  paquets Debian (`apt-get`) et, pour Antigravity, l'archive. Le titre du backlog « sans réseau » est
  inexact : il devient « sans résolution du registre ».
- **Les paquets Debian ne sont pas figés** (`apt-get install` prend la version courante de bookworm).
  Déjà le cas aujourd'hui ; hors périmètre.
- **`baseImage` vient du lock, le gabarit `assets/Containerfile` vient du binaire.** L'image de base
  porte Node et le système : la changer peut changer le comportement de l'agent, donc elle reste celle
  de la release active (seul `update` la renouvelle, avec `release::BASE_IMAGE`). Le gabarit, lui,
  porte les protections de zevbox (utilisateur, droits, vérification d'empreinte) : il suit le code de
  zevbox installé, y compris pour une reconstruction.

## 3. Comportement exigé

### 3.1 Lecture de `tools.json` (commune à `tools` et `update`)

**T-01** — Une seule fonction de lecture « tolérante à l'absence » (par exemple
`tools::load_or_default(path) -> Result<ToolsFile, ConfigError>`), utilisée par `tools list|add|remove`
**et** par `update` (`cli/update.rs:22`) :

- `Err(ConfigError::Io { source, .. })` avec `source.kind() == NotFound` → `ToolsFile::default()`.
- Toute autre erreur → l'appelant affiche
  `zevbox: cannot read <chemin de tools.json>: <raison>. Fix: correct or delete that file, then run the command again.`
  et sort en **1**, sans rien modifier ni construire. `<raison>` = l'erreur d'E/S (`source`) pour
  `Io`, la chaîne interne pour `Parse` (pas le `Display` de `ConfigError::Parse`, qui dit
  « config.json is not valid »), `'<nom>' is not a valid apt package name` pour `InvalidAptName`.

C'est un changement de comportement de `update` (aujourd'hui il construit sans outils) : à documenter.

### 3.2 `zevbox tools list`

**T-02** — Inchangé (une ligne par paquet, rien si vide, aucune release ni service nécessaire), sauf
qu'un fichier illisible donne le message de T-01 et le code 1.

### 3.3 `zevbox tools add` / `remove` : déroulé

1. **T-03** — Sans `config.json` : inchangé (`zevbox is not set up. Fix: run 'zevbox setup' or 'zevbox init' in a macOS terminal.`, code 3).
   `config::load` en échec : erreur actuelle (`zevbox: …`, code 1), rien de modifié.
2. **T-04** — Lire `tools.json` (T-01).
3. **T-05** — Calculer la nouvelle liste avec `ToolsFile::add` / `remove`, inchangés :
   - `add` valide tous les noms d'abord ; un nom invalide → `zevbox: '<nom>' is not a valid apt package name`, code 1, rien de modifié ;
   - `remove` ne valide pas : un nom inconnu ou invalide est simplement ignoré.
4. **T-06** — Déterminer les **agents à reconstruire** : chaque entrée de `active.json` connue de zevbox
   (`AgentArg::from_id`), oMLX compris s'il est installé, **dans l'ordre de la table `agents::AGENTS`**
   (claude, antigravity, omlx), pas l'ordre alphabétique de la `BTreeMap`.
5. **T-07** — **Liste inchangée** (aucun paquet ajouté ou retiré) :
   - si **chaque** agent à reconstruire a une release active dont `aptPackages` est égal (même
     ensemble, l'ordre ne compte pas) à la liste de `tools.json` → messages actuels
     `no new tools to add` / `no matching tools to remove`, code 0, rien d'autre ;
   - sinon (divergence laissée par un échec, un `rollback` ou une édition à la main) → afficher
     `the tool list is unchanged, but <Display> was built with another list; rebuilding it.` pour
     chaque agent concerné, puis continuer à l'étape 8 **pour ces agents seulement**. C'est la
     réparation prévue des divergences (§3.5).
6. **T-08** — **Aucun agent à reconstruire** (pas encore de release) : enregistrer `tools.json`,
   afficher `added: <liste>` / `removed: <liste>`, puis
   `no agent is installed yet: the tools will be part of the first 'zevbox update'.`, code 0.
7. **T-09** — Vérifier le service : `ContainerCli::system_status()`. `Err(_)` ou `running == false` →
   `Apple Container system is not running. Fix: run 'container system start' in a macOS terminal, then run the command again.`,
   code **3**, rien de modifié, aucun accès réseau. (On ne démarre pas le service soi-même : `tools`
   est une commande de terminal.)
8. **T-10** — Pour chaque agent à reconstruire, charger `releases/<current>/lock.json`
   (`release::load_lock`) et vérifier `lock.agent == clé de active.json` et
   `lock.release_id == current`. Lock absent, invalide ou incohérent →
   `<Display>: cannot use the lock of the active release <id>: <raison>. Fix: run 'zevbox update --agent <id agent>'.`,
   code 1, rien de modifié. **Tous** les locks sont vérifiés avant toute construction.
9. **T-11** — Afficher `added: <liste>` / `removed: <liste>` (sauf cas T-07 sans ajout ni retrait),
   puis construire une **candidate** par agent selon §4, **une à la fois, dans l'ordre de T-06**.
   **Aucune promotion pendant cette phase.**
10. **T-12** — **Premier échec** (préparation, construction ou test de fumée) : ne pas construire les
    agents suivants ; appliquer §3.6 (annulation).
11. **T-13** — **Toutes les candidates réussissent** : appliquer §3.4 (validation).

### 3.4 Validation (tout a réussi)

Dans cet ordre :

1. **T-14** — Écrire le `lock.json` de chaque candidate (`release::save_lock`).
2. **T-15** — Enregistrer `tools.json` (`tools::save`).
3. **T-16** — Pour chaque candidate, `release::promote` (`current` = candidate, `previous` = ancienne
   `current`), puis écrire `active.json` **une seule fois** (`release::save_active`).
4. **T-17** — Afficher `<Display>: promoted to <id candidate> (same version <agentVersion>)` par agent.
5. **T-18** — `cli::zed::update_zed` comme `update` (erreurs ignorées).
6. **T-19** — Ménage comme `update` : `release::prune_images` (qui arrête et supprime aussi la VM de
   construction), puis `clean::prune_release_files` (messages actuels).
7. Code 0.

**T-20** — Échec d'écriture aux étapes 1 à 3 : `zevbox: cannot save <chemin>: <raison>.`
(`<raison>` sans répéter le chemin), code 1. Pas d'annulation des écritures déjà faites. Les
écritures sont atomiques une à une (`fsutil::write_atomic`), et l'ordre garantit que `active.json`,
écrit en dernier, ne désigne jamais une release sans `lock.json`. **Divergence acceptée** : si
l'étape 3 échoue après l'étape 2, `tools.json` a la nouvelle liste et les releases actives
l'ancienne ; la commande suivante la répare (T-07, cas « liste inchangée »).

### 3.5 Divergences entre `tools.json` et les images

Sources possibles : T-20, `zevbox rollback`, une édition à la main de `tools.json`. Elles ne sont
jamais silencieuses : n'importe quelle commande `tools add|remove`, même sans changement de liste,
reconstruit les agents divergents (T-07). `zevbox update` les répare aussi, mais en changeant de
version.

### 3.6 Annulation (un agent a échoué)

1. **T-21** — Supprimer l'image de **chaque candidate dont `build_image` a réussi** (y compris celle
   dont le test de fumée a échoué), avec `ContainerCli::delete_zevbox_image` ; erreurs ignorées avec
   `tracing::warn!` ; ne pas utiliser `image list` pour décider ; jamais `--all` (I8).
2. **T-22** — Ne **pas** écrire de `lock.json` pour les candidates. Leurs dossiers `releases/<id>/`
   (contexte et `build.log`) restent pour le diagnostic, comme pour un `update` en échec aujourd'hui
   (`prune_release_dirs` ne supprime que les dossiers qui ont un `lock.json` ; défaut existant, §6).
3. **T-23** — Appeler `clean::prune_release_files` (retire l'archive `agy.zip` d'environ 300 Mo de
   chaque dossier, garde les dossiers sans `lock.json`) et `ContainerCli::remove_builder` (erreur
   ignorée). **Ne pas** appeler `prune_images`.
4. **T-24** — `tools.json`, `active.json`, les releases actives et leurs images sont **inchangés**.
5. **T-25** — Message final sur stderr, code 1 :
   `zevbox: rebuilding <Display> failed (see above); nothing was changed: the tool list and the active releases are as before.`
   Les messages détaillés restent ceux de l'étape en échec (`<Display>: prepare context failed: …`,
   `<Display>: build failed: build error: container build failed with exit code …; see <build.log>`,
   `<Display>: smoke test failed: …`).

### 3.7 Messages

Par agent, à la place de `<Display>: preparing release for version X...` :
`<Display>: rebuilding version <agentVersion> (release <id active>, registry not contacted) with tools: <liste séparée par ", " ou "none">...`,
puis les messages actuels `<Display>: building image <image>...` et `<Display>: running smoke test...`.

Progression sur stdout (`ui::out`), erreurs sur stderr (`ui::err`), en anglais, format
`<problème>. Fix: <commande exacte>` quand une action est possible.

## 4. Construction d'une candidate à partir du lock actif

Le travail est dans `src/release.rs` ; `cli/tools.rs` enchaîne seulement.

### 4.1 Lock candidat : fonction pure

`pub fn rebuild_lock(active: &ReleaseLock, apt: &[String], release_id: &str, image: &str, now: &str) -> ReleaseLock`,
testée en unitaire.

| Champ | Valeur |
|---|---|
| `schemaVersion` | `1` |
| `releaseId` | `release::new_release_id(agent, Utc::now())` (`release.rs:422`, `<agent>-AAAAMMJJ-HHMMSS` UTC) |
| `agent` | celui du lock actif |
| `createdAt` | maintenant (RFC 3339) |
| `registryManifestUrl`, `registryManifestSha256` | **copiés** (provenance d'origine) |
| `agentVersion`, `npmPackage`, `archiveUrl`, `archiveSha256` | **copiés** |
| `baseImage` | **copié du lock actif**, pas `release::BASE_IMAGE` (§2) |
| `aptPackages` | la nouvelle liste |
| `agentCommand`, `authCommand`, `authMarker` | **copiés** |
| `image` | `<image_repo de l'agent dans agents::AGENTS>:<releaseId>` |
| `smokeTest` | `None`, rempli après le test de fumée |

**T-26** — Pas de nouveau champ dans `ReleaseLock` : il est `deny_unknown_fields`, et les locks déjà
sur disque comme le JSON des fixtures de test (`tests/common/mod.rs:88-107`) doivent rester lisibles.
La provenance (« reconstruite depuis `<id>` ») apparaît seulement dans les messages (§3.7).

**T-27** — Si `releases/<releaseId>/` existe déjà : attendre jusqu'à la seconde suivante, recalculer
l'identifiant, au plus 2 essais ; sinon échec de préparation `<Display>: release folder <id> already exists.`

### 4.2 Préparation du contexte

`prepare_context` (`release.rs:456-546`) fait aujourd'hui tout d'un bloc, et écrit toujours
`package.json` avant de lancer npm. **Le découper** en trois fonctions, qu'`update` appelle dans le
même ordre qu'aujourd'hui, avec les mêmes messages et le même résultat :

- `write_common_context(ctx, lock)` : dossier `0700`, `Containerfile` (gabarit embarqué),
  `zevbox-release.json` (le lock sérialisé) ;
- `claude_npm_lock(cli, ctx, lock)` : `package.json` + `npm install --package-lock-only --ignore-scripts <npmPackage>`
  dans un `container run` de `baseImage` ;
- `download_archive(ctx, lock)` : `curl … -o context/agy.zip <archiveUrl>`.

Pour une reconstruction, dans `releases/<nouvel id>/context/` :

- **T-28** — `write_common_context` avec le lock candidat. Pour Antigravity, `archiveSha256` est déjà
  rempli dans ce lock (le contenu de `zevbox-release.json` est donc complet, contrairement à
  `update` qui l'écrit avant de connaître l'empreinte).
- **Claude et oMLX** (`build_target == "claude"`) :
  - **T-29** — **Réutiliser le verrou npm** : copier `package.json` et `package-lock.json` depuis
    `releases/<id active>/context/` **si et seulement si** les deux existent, `package-lock.json` se lit
    en JSON, `packages["node_modules/@agentclientprotocol/claude-agent-acp"].version == agentVersion`,
    et la dépendance `@agentclientprotocol/claude-agent-acp` de `package.json` vaut `<agentVersion>` ou
    `^<agentVersion>` (`npm` écrit `^0.99.0` : vérifié sur les releases réelles
    `claude-20261029-163307`, `omlx-20261029-154928`, `claude-20261014-151553`). Le Containerfile fait
    `npm ci` : les dépendances transitives sont alors exactement celles de la release active.
  - **T-30** — Sinon : `claude_npm_lock` avec le **même** `npmPackage`, et l'avertissement
    `<Display>: the npm lock of <id active> is missing or does not match version <agentVersion>; resolving its dependencies again (same agent version).`
    L'agent garde sa version ; seules ses dépendances transitives peuvent bouger.
- **Antigravity** :
  - **T-31** — `archiveUrl` doit commencer par `https://` et `archiveSha256` être présent. Sinon échec :
    `<Display>: the active release <id> has no usable archive (https URL and sha256). Fix: run 'zevbox update --agent antigravity'.`
  - **T-32** — `download_archive`. Échec de `curl` (dont 404) →
    `<Display>: cannot download the archive of version <agentVersion> (<erreur curl>). Fix: run 'zevbox update --agent antigravity' to install the version the registry offers now.`
  - **T-33** — Comparer le SHA-256 de `agy.zip` à `archiveSha256` **avant** de construire. Différent →
    supprimer `agy.zip`, échec
    `<Display>: the archive at <archiveUrl> no longer matches the active release (expected sha256 <12 premiers caractères>…). Fix: run 'zevbox update --agent antigravity' to install the version the registry offers now, then run 'zevbox tools …' again.`
    (`update`, lui, n'a pas de référence et calcule l'empreinte de ce qu'il télécharge.) Le
    Containerfile revérifie l'empreinte (`--build-arg AGY_SHA256`, `sha256sum -c`, `assets/Containerfile:29`).

### 4.3 Construction et test de fumée

- **T-34** — `build_image` (inchangée) avec le lock candidat : `BASE_IMAGE` = `baseImage` du lock,
  `APT_PACKAGES` = nouvelle liste, `ZEVBOX_UID/GID` = `config.host_uid/host_gid`, `AGY_SHA256` =
  `archiveSha256` pour Antigravity, `--target` = `build_target` de l'agent (`agents::AGENTS`).
  `build.log` est écrit par `build_image` comme aujourd'hui (il le vide à l'ouverture : rien n'y est
  ajouté avant).
- **T-35** — `run_smoke_test` existant, inchangé : `uname -s`, `dpkg -s` de chaque paquet de la
  nouvelle liste, `/etc/zevbox-release.json` contient le nouveau `releaseId`.
- **T-36** — Preuve de version, propre à la reconstruction, **dans l'image** :
  - `build_target == "claude"` :
    `container run --rm <image> node -p "require('/opt/zevbox/claude/node_modules/@agentclientprotocol/claude-agent-acp/package.json').version"`
    doit afficher exactement `agentVersion` (après suppression des espaces). Sinon : test de fumée
    en échec, `<Display>: smoke test failed: the image holds adapter version <lu>, expected <agentVersion>.`
  - Antigravity : la preuve est la vérification d'empreinte de l'archive (T-33 et `sha256sum -c`
    pendant la construction) ; pas de contrôle supplémentaire.
  - (`/etc/zevbox-release.json` n'est pas une preuve : zevbox l'écrit lui-même depuis le lock.)
- `tools remove` : pas de test d'absence du paquet retiré (il peut rester présent comme dépendance
  d'un autre).

## 5. Ce qui ne change pas

- **T-37** — `zevbox update` : seul changement, la lecture de `tools.json` (T-01). Il résout toujours
  le registre, prend les dernières versions et `release::BASE_IMAGE`. C'est la seule commande qui
  change de version.
- **T-38** — `launch` ne construit rien et ne touche pas le réseau (I10) : le code de reconstruction
  n'est appelé que depuis `cli/tools.rs`.
- **`zevbox rollback --agent A`** : revient à `previous`. Après un `tools add|remove`, `previous` est la
  release **juste avant** (même version, ancienne liste d'outils) ; la release d'une version plus
  ancienne d'avant (par exemple 0.90 avant 0.99) n'est plus accessible par `rollback`. `rollback` ne
  modifie pas `tools.json` : la divergence qui en résulte est réparée par la commande `tools` suivante
  (T-07) ou par `update`. À documenter.
- Aucun nouveau montage, `run_args` inchangé (I1, I12) ; aucun fichier protégé modifié.

## 6. Hors périmètre

- Verrou entre processus (deux `update`/`tools` simultanés peuvent s'écraser `active.json`).
- Versions des paquets Debian.
- Commande `zevbox tools rebuild` (non demandée ; T-07 couvre la réparation).
- Test ACP `initialize` du plan §8.7 étape 5, marquage « failed » d'un lock, suppression des dossiers
  de releases en échec (sans `lock.json`) : non implémentés aujourd'hui, non traités ici.
- **Point de backlog 12, trouvé pendant l'analyse et corrigé le 2026-10-29** : `release::prune_images` supprimait toute image `zevbox/*` absente de **son** `active.json`, donc, depuis un état isolé, les images de la vraie installation. Depuis la correction (`release::stale_images_here`), une racine d'état ne supprime que les images de ses propres releases : les essais de §8.2 peuvent se faire dans un état isolé sans risque pour la vraie installation. L'essai sur l'état réel reste la preuve finale.

## 7. Fichiers touchés

- `src/release.rs` : `rebuild_lock`, découpage de `prepare_context` (§4.2), réutilisation du verrou npm
  (T-29/T-30), contrôles d'archive (T-31 à T-33), preuve de version (T-36).
- `src/cli/tools.rs` : déroulé §3 (mince).
- `src/tools.rs` : `load_or_default` (T-01), dans le style du fichier.
- `src/cli/update.rs` : lecture de `tools.json` (T-01) et appels aux trois fonctions issues de
  `prepare_context` ; aucun autre changement de comportement ni de message.
- `AGENTS.md` (module map) : `release.rs` « registry, locks, build, promote, rollback, **rebuild from lock** ».
- **Aucun fichier protégé** (`tests/PROTECTED.sha256`). Si l'implémentation en a besoin : s'arrêter et
  demander au 👤 avec le diff.

## 8. Preuves

### 8.1 Tests automatiques (nouveaux fichiers, non protégés)

Faits sur le faux `container` (`tests/fixtures/fake-container`, protégé) :
- il journalise chaque appel dans `FAKE_CONTAINER_LOG` **avant** de répondre ;
- `build` → `fake-container: unsupported command 'build'`, code 64 : toute construction échoue, ce
  qui teste l'annulation et permet de lire les arguments de `build` ;
- `run` sans `--name` → code 65 : le chemin T-30 (npm dans un `container run`) ne peut pas aller
  jusqu'au bout avec le faux ;
- `system status` répond toujours `running`.

`curl` n'est pas appelé à travers le faux : il est lancé directement (`release.rs:431`, `:522`). Pour
prouver « registre non contacté » et rester hors réseau, les tests de bout en bout construisent leur
commande à la main (comme `launch_omlx` dans `tests/omlx_test.rs`) avec
`PATH=<tmp>/fakebin:/usr/bin:/bin`, où `fakebin/curl` est un script qui ajoute ses arguments à
`<tmp>/curl.log` et sort en 7.

**Unitaires** (`#[cfg(test)]` de `release.rs`, `tools.rs`, `cli/tools.rs`) :
- `rebuild_lock` : chaque ligne du tableau §4.1 (T-26) ;
- `load_or_default` : absent → défaut ; invalide → erreur avec la bonne raison (T-01) ;
- ordre des agents (T-06) ; décision « liste inchangée / divergence » (T-07) ;
- réutilisation du verrou npm : `^v` et `v` acceptés, version différente refusée, fichier manquant ou
  JSON invalide → régénération (T-29, T-30) ;
- contrôles d'archive : URL non https, empreinte absente, empreinte différente (T-31, T-33) ;
- lecture de la version de l'adaptateur (T-36, fonction pure sur la sortie de `node -p`).

**Bout en bout** (nouveau fichier `tests/r1_tools_rebuild_test.rs`, `mod common;`, `TestEnv`) :
1. **Sans release** : supprimer `releases/active.json` (que `TestEnv::new` crée,
   `tests/common/mod.rs:83-86`) ; `tools add tree` → `tools.json` = `{"schemaVersion":1,"apt":["tree"]}`,
   message T-08, code 0, aucun `build` dans le journal du faux, `curl.log` vide.
2. **Échec de construction = rien ne change** : écrire dans `releases/claude-test/context/` un
   `package.json` `{"private":true,"dependencies":{"@agentclientprotocol/claude-agent-acp":"^0.0.0-test"}}`
   et un `package-lock.json` dont `packages["node_modules/@agentclientprotocol/claude-agent-acp"].version`
   vaut `"0.0.0-test"` (l'`agentVersion` du lock de test, `tests/common/mod.rs:96`). `tools.json` est
   absent au départ. `tools add tree` → code 1, message T-25 ; `tools.json` **toujours absent** ;
   `active.json` identique octet pour octet ; le journal du faux contient un `build` avec
   `--build-arg BASE_IMAGE=<baseImage du lock de test>` et `--build-arg APT_PACKAGES=tree` ; `curl.log`
   vide ; stderr sans `cannot fetch manifest`, stdout sans `preparing release for version`.
3. **`tools.json` invalide** : `tools list`, `tools add tree` et `update` → message T-01, code 1,
   fichier inchangé, aucun `build`.
4. **Lock actif incohérent** (`agent` modifié) → T-10, code 1, rien de modifié, aucun `build`.
5. **Service arrêté** : `config.json` → `containerBinary` pointe sur un script de `<tmp>` qui répond
   `{"status":"stopped"}` à `system status` → T-09, code 3, `tools.json` inchangé, `curl.log` vide.
6. **Liste inchangée mais divergente** : même contexte npm que le test 2, `tools.json` = `["tree"]`, lock de test `aptPackages: []` ;
   `tools add tree` → message T-07 « rebuilding it », tentative de `build` (qui échoue avec le faux),
   code 1, rien de modifié.

### 8.2 Essai manuel (machine réelle ; preuve finale sur l'état réel, voir §6)

À consigner dans `docs/PROGRESS.md`. Paquet d'essai : **`tree`**, absent de l'image (attention, `jq` est
déjà dans l'image de base, `assets/Containerfile:11` : il ne prouve rien).

1. `zevbox update --check` : noter versions actives et disponibles. Idéalement une version plus récente
   est proposée pour au moins un agent : c'est ce qui prouve qu'elle n'est **pas** prise.
2. `container run --rm <image active> tree --version` → échoue (preuve que `tree` manque).
3. `zevbox tools add tree` → nouvelles releases pour chaque agent installé, messages §3.7, même version.
4. Pour chaque agent : `agentVersion`, `npmPackage` ou `archiveSha256`, `baseImage` du nouveau
   `lock.json` égaux à ceux de l'ancien ; `aptPackages` contient `tree` ; `previous` = l'ancienne ;
   `container run --rm <nouvelle image> tree --version` répond ; pour Claude et oMLX, le `node -p` de
   T-36 affiche la même version ; `package-lock.json` du nouveau contexte identique à l'ancien
   (`shasum`).
5. Un thread Zed par agent : fonctionne.
6. `zevbox tools remove tree` : nouvelles releases, `tree` absent de `aptPackages`.
7. Échec provoqué : couper le réseau, `zevbox tools add tree` → code 1, message T-25, `tools.json` et
   `active.json` identiques à avant (`shasum`), aucune image candidate restante (`container image list`).
8. Divergence : `zevbox rollback --agent claude` (ancienne liste), puis `zevbox tools add tree` sans
   rien changer à la liste → message T-07 « rebuilding it », Claude reconstruit.

## 9. Docs à mettre à jour dans le même commit (règle absolue 1)

- `zevbox_plan_V2.md` : ligne `tools` du tableau des commandes (`:427`) ; §8.7 (`:538` et suivantes :
  chemin « reconstruction depuis le lock » à côté du pipeline `update` ; corriger l'étape 3 qui dit
  « version exacte » pour `package.json`, alors que npm écrit `^<version>`) ; §8.8 `tools.json`
  (`:592`) ; tableau des cycles de vie (`:720-721`) ; liste de contrôle (`:739`).
- `README.md` (`:151`, `:175`) et `docs/fr/README.md` (`:156`, `:180`) : « reconstruit les images **aux
  versions actuelles** ; seul `zevbox update` change de version ».
- `docs/USER_GUIDE.md` (`:359`, `:440`) et `docs/fr/GUIDE_UTILISATEUR.md` (`:337`, `:421`) : idem, plus les
  limites de §2, « un échec ne change rien », l'effet de `rollback` (§5) ; remplacer l'exemple `jq`
  (déjà dans l'image) par un paquet absent, par exemple `tree`.
- `docs/ARCHITECTURE.md` (`:251-265`) et `docs/fr/ARCHITECTURE.md` (`:227-236`) : branche « tools → lock
  actif → contexte → build → test → promotion de toutes les candidates ».
- `docs/DECISIONS.md` : point 1 → fait (date, résumé de 3 lignes) et titre corrigé ; point 12 (§6) —
  **déjà ajouté avec cette spécification**.
- `docs/PROGRESS.md` : ligne de journal avec les preuves de §8.
- `AGENTS.md` : module map (§7).

## 10. Critères d'acceptation

1. `T-01` à `T-38` couvertes chacune par un test (§8.1) ou par l'essai manuel (§8.2).
2. `make check` vert ; `make verify STEP=17` → `STEP 17: OK` ; aucun fichier protégé modifié.
3. Essai manuel §8.2 fait et consigné ; versions identiques prouvées par les locks, par le `node -p` de
   T-36 et par l'égalité des `package-lock.json`.
4. Docs §9 à jour. `grep -rn` des anciennes formulations sans résultat obsolète :
   « rebuilds the active agent images », « reconstruit les images actives », « `tools add/remove` sans
   réseau » (pas `sans réseau` seul : `zevbox_plan_V2.md:672` « Étape 7 — release.rs (sans réseau) » est
   légitime).
5. Un commit par sujet : cette spécification est commitée seule ; l'implémentation fait un commit
   `r1: tools add/remove rebuild the active versions` ; le point 12 est un autre sujet.
