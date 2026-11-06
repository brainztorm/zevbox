# Plan d'implémentation de Codex en ACP

Statut au 2026-11-06 : plan éprouvé par une répétition complète dans des copies
jetables (§1.3). C0 à C3 sont validées ; reprise à C4. Rien ici ne vaut
acceptation C4. Résultat attendu : [SPEC_codex.md](SPEC_codex.md).

L'humain demande la même expérience et les mêmes fonctions de gestion que pour
Claude et Antigravity, avec Codex standard et une connexion native fiable. Le
code d'appareil ChatGPT est le parcours guidé retenu, sans exclusivité imposée.
Ce plan complète le contrat V2 pour cette extension ; il n'en retire aucun
invariant. Il ne demande ni sous-agents ni délégation.

**Mode d'emploi.** Ce plan se suit à la lettre, dans l'ordre. Quand il donne une
commande, lancer exactement cette commande. Quand il donne un résultat attendu,
le comparer au résultat obtenu. Au moindre écart, appliquer le §1.5 au lieu
d'improviser. Les décisions de conception sont déjà prises : ne pas les rouvrir.

La comparaison attendue avec Claude et Antigravity figure dans la SPEC §9.
L'acceptation ne doit pas promettre des interfaces de modèles, des politiques de
permissions ou des performances identiques. Dans toute la doc, le nom du projet
est **Zevbox** ; les commandes, chemins, noms d'images et le préfixe géré
`zevbox · ` gardent leur orthographe technique.

## 1. Instructions de reprise

### 1.1 Lecture

Lire entièrement `AGENTS.md`, `zevbox_plan_V2.md`, `docs/PROGRESS.md`,
`docs/DECISIONS.md`, `docs/SPEC_codex.md`, puis ce plan. Les étapes historiques
0–17 restent terminées ; ne pas les recommencer. Reprendre à la première étape
Codex non validée dans le tableau « Extension Codex » de `docs/PROGRESS.md`.

### 1.2 Avant de commencer

```bash
git status --short
make check
shasum -a 256 -c tests/PROTECTED.sha256 --quiet
```

Résultats attendus : `git status --short` n'affiche rien ; `make check` se termine
par `invariants: OK` ; `shasum` n'affiche rien.

Si `git status --short` affiche quelque chose, **ne pas commencer** et demander à
l'humain : « Le dépôt contient des modifications non commitées : <liste>. Les
commitez-vous, ou puis-je en faire un commit dédié ? ». Le travail de chaque étape
doit rester séparé de ce qui existait déjà. Si `make check` échoue, appliquer le
§1.4 (échec de délai) puis, si l'échec persiste, le §1.5 : c'est une panne de base,
pas une régression Codex.

Base vérifiée le 2026-11-06 : `make check` vert (324 tests réussis, 8 ignorés).
Un bac à sable d'agent peut empêcher un test de socket locale et un test de
surveillance de fichiers. Dans ce cas seulement, relancer la même commande hors
restriction ; ne pas assouplir ces tests.

### 1.3 Annexe : ce qui est déjà prouvé

Les correctifs de `docs/codex_annex/` ont été appliqués puis retirés après C3.
Les quatre scripts restent pour C4 et les requalifications. Les noms de correctifs
et commandes d’application ci-dessous décrivent la procédure exécutée ; ils ne
sont plus des fichiers présents ni des actions de reprise.

Le dossier `docs/codex_annex/` conserve les scripts issus de la répétition du
2026-11-06. Chaque correctif a été appliqué dans l'ordre sur un dépôt propre, avec
`make check` complet après chaque étape. Le code complet a aussi été construit et
exécuté dans une vraie VM (SPEC §8.1).

| Fichier | Rôle |
|---|---|
| `C1_image_release.patch` | Étape C1. `make check` vert : 329 tests. |
| `C2a_relay_version.patch` | Étape C2a. `make check` vert : 332 tests. |
| `C2_activation_auth.patch` | Étape C2, sans le fichier protégé. Deux échecs attendus (§5.3). |
| `protected_zed_settings.diff` | Diff protégé de C2, à faire approuver (§5.6). |
| `C3_console.patch` | Étape C3. `make check` vert : 343 tests. |
| `isolated_env.sh` | Crée une installation jetable sous `/tmp`. |
| `verify_image.sh` | Contrôle une image Codex avec les options de `launch` : `PASS` ou `FAIL`. |
| `verify_launch.py` | Contrôle `zevbox launch codex` à travers le vrai relais : `PASS` ou `FAIL`. |
| `cleanup_isolated.sh` | Supprime les images et le dossier d'une installation jetable. |

Appliquer un correctif toujours ainsi :

```bash
git apply --check docs/codex_annex/<fichier>.patch
git apply docs/codex_annex/<fichier>.patch
```

Si `--check` échoue, le dépôt a changé depuis la répétition. Ouvrir le correctif et
reporter ses changements à la main, fichier par fichier, à l'identique. Ne pas
concevoir autre chose. Si un changement ne peut pas être reporté tel quel, §1.5.

Le code de ces correctifs ne contient aucune version de Codex en dur : il lit la
version stable dans le registre à chaque `update`.

### 1.4 Règles de travail

- **Un sujet par commit.** Avant chaque commit : `cargo fmt`, `make check`,
  `git diff --check`, documents à jour (§8). Ajouter les fichiers par leur nom :
  jamais `git add -A` ni `git add .`.
- **Interdits Git** : `git stash`, `git reset --hard`, `git checkout -- <fichier>`,
  `git clean`, tag, push. Les modifications présentes appartiennent à l'humain.
- **Fichiers protégés** : `tests/PROTECTED.sha256` liste ceux qu'il ne faut pas
  toucher. Seul le diff du §5.6 est prévu, et seulement après accord explicite.
  Ne jamais lancer `make protect-update` : il scellerait aussi des tests hors manifeste.
- **Ne pas modifier `tests/fixtures/fake-container`** (protégé). Il ne sait ni
  `build` ni `image pull`, et `fetch_manifest` utilise le vrai réseau : le pipeline
  `update` ne se teste donc pas avec `common::TestEnv`. Il se prouve en vrai dans
  une installation jetable (§5.4 et §7.1). Ne pas ajouter de variable d'environnement
  ni de chemin de code réservé aux tests pour contourner cela.
- **`HOME`** : ne jamais l'exporter dans le shell de travail, `cargo` en dépend.
  Dans une installation jetable, toujours utiliser un sous-shell :
  `( . "$W/env.sh"; "$W/zevbox-bin" <commande> )`.
- **Machine de l'humain** : d'autres conteneurs y tournent. Ne jamais lancer
  `container image prune`, `container system stop`, ni `stop`/`delete` sur un
  conteneur qui n'est pas le vôtre. Ne pas lancer `make install`, ne pas toucher à
  `~/Library/Application Support/zevbox` ni au vrai Zed avant C4 avec l'humain.
- **Secrets** : ne jamais afficher, copier ni consigner le contenu d'un `auth.json`,
  un code d'appareil ou un jeton. Seules des valeurs synthétiques sont permises.
- **Échec de délai** : `zevbox launch did not exit within 5s`, dans un test de
  `lifecycle_test` sans rapport avec l'étape, vient de la charge de la machine
  (vu une fois pendant la répétition, sur un test protégé). Lancer
  `cargo test --test lifecycle_test` trois fois. Trois réussites : relancer
  `make check` et continuer, ne rien modifier. Sinon : §1.5.
- **Lecture de `make check`** : il s'arrête au premier fichier de tests en échec,
  donc ses totaux sont alors partiels. Pour la liste complète :
  `cargo test --no-fail-fast 2>&1 | grep -E '^test .* FAILED$'`.
- **Recherche** : utiliser `grep -rn`. `rg` n'est pas garanti dans tous les shells.
- **Rust** : `make` trouve `cargo` seul. Pour lancer `cargo` directement, commencer la
  commande par `export PATH=/opt/homebrew/opt/rustup/bin:$PATH;`.
- **Un bloc, une commande** : lancer chaque bloc de commandes de ce plan en un seul
  appel. Avec un outil qui ouvre un nouveau shell à chaque appel, les variables
  (`W`, `IMAGE`, `T`) sont perdues entre deux appels. Si un bloc doit être coupé,
  noter la valeur de `W` affichée par `isolated_env.sh` et la redéfinir ensuite.

### 1.5 Quand s'arrêter et demander à l'humain

S'arrêter sans contourner dans chacun de ces cas :

1. Le dépôt n'est pas propre au départ (§1.2).
2. `make check` échoue autrement que ce que ce plan annonce.
3. `verify_image.sh` ou `verify_launch.py` affiche `FAIL` après une relance.
   Ne jamais passer en Full access, ajouter une capacité, ni ouvrir
   `elicitation/*` pour faire passer un contrôle.
4. Zed ne demande plus la version 1 d'ACP (§3).
5. Il faudrait modifier un fichier protégé autre que celui du §5.6, ou affaiblir
   un invariant I1–I13.
6. L'étape demande un compte, du quota ou l'interface de Zed (§7.2 et §7.3).

Rapporter : l'étape, la commande exacte, sa sortie, ce qui a été essayé. Une étape
en attente de l'humain reste `👤` dans PROGRESS. Les vérifications indépendantes
peuvent continuer, sauf celles qui en dépendent.

## 2. Ordre des étapes

| Étape | Livrable | Commit |
|---|---|---|
| C0 | Vérifier que les versions n'ont pas bougé | `docs: qualify Codex ACP integration` |
| C1 | Image et composants de release, sans activation publique | `feat: prepare Codex ACP image and release components` |
| C2a | Le relais refuse les versions ACP qu'il ne filtre pas | `relay: refuse ACP versions it does not filter` |
| C2 | Activation standard, auth native, diagnostics, dans un seul commit | `feat: enable standard Codex agent with native authentication` |
| C3 | Connexion Codex dans la console | `feat: expose Codex throughout the console` |
| C4 | Qualification réelle, acceptation humaine, revue documentaire | `docs: record Codex ACP acceptance` |

Chaque étape livre ses documents dans son propre commit (§8) ; ne pas les reporter.
La porte finale est `make verify STEP=17`. Elle réutilise les gates historiques et
ne remplace pas les preuves Codex. Ne pas créer de gate STEP=18.

La séparation C1/C2 est une condition de cohérence : `auth` traite Antigravity et
oMLX explicitement, puis utilise par défaut les réglages et le constructeur Claude.
Ajouter `AgentArg::Codex` sans sa branche dédiée monterait `state/claude` pour une
commande Codex. Aucun commit ne doit exposer cet état, et Codex ne passe jamais par
`opt_in: true` à titre temporaire.

## 3. C0 Vérifier que les versions n'ont pas bougé

La qualification de fond est faite (SPEC §8.1). C0 vérifie seulement qu'elle vaut
toujours le jour de l'exécution.

```bash
curl -fsSL https://raw.githubusercontent.com/agentclientprotocol/registry/main/codex-acp/agent.json \
  | python3 -c 'import json,sys;d=json.load(sys.stdin);print(d["version"],d["distribution"]["npx"]["package"])'
defaults read /Applications/Zed.app/Contents/Info.plist CFBundleShortVersionString
container --version
```

Résultats attendus le 2026-11-06 : `2.5.0 @agentclientprotocol/codex-acp@2.5.0`,
`1.25.3`, `container CLI version 1.5.0`.

| Constat | Décision |
|---|---|
| Tout est identique | Continuer. |
| L'adaptateur stable est plus récent | Continuer : le code lit la version dans le registre, et `verify_image.sh` puis `verify_launch.py` la requalifieront. Noter la version dans la SPEC §8 et PROGRESS. |
| Zed a une autre version | Lancer la commande ci-dessous. S'il affiche `ProtocolVersion::V1`, continuer et noter la version. Sinon : §1.5. |
| `container` a une autre version | Continuer, noter la version. Tout `FAIL` ultérieur se traite par le §1.5. |

```bash
curl -fsSL "https://raw.githubusercontent.com/zed-industries/zed/v<version>/crates/agent_servers/src/acp.rs" \
  | grep -n 'InitializeRequest::new(ProtocolVersion'
```

**Documents** : dans `docs/PROGRESS.md`, passer C0 à ✅ avec les versions et la date.
Si une version a changé, la noter aussi dans la SPEC §8 et `docs/DECISIONS.md`.

Commit : `docs: qualify Codex ACP integration`.

## 4. C1 Image et composants de release

Rien ne devient visible pour l'utilisateur : pas d'entrée `AGENTS`, pas
d'`AgentArg::Codex`, pas de release active.

1. Appliquer `C1_image_release.patch` (§1.3). Il contient :
   - `src/release.rs` : `CodexComponent`, `parse_codex_manifest`, et `prepare_context`
     qui écrit le `package.json` du bon adaptateur (helper `write_package_json`) ;
   - `assets/Containerfile` : l'étape `FROM base AS codex`, avec `NO_BROWSER=1` et la
     suppression de `/usr/bin/bwrap` (SPEC §6 explique pourquoi elle est indispensable) ;
   - `tests/codex_release_test.rs` et `tests/fixtures/registry/codex-acp.json`.
2. Lancer `make check`. Attendu : vert, 329 tests réussis.
3. Construire une image d'essai et la contrôler. Si C0 a relevé une autre version,
   la remplacer dans `package.json`.

```bash
T=$(mktemp -d /tmp/zevbox-c1.XXXXXX)
cp assets/Containerfile "$T/Containerfile"
printf '{"private":true,"dependencies":{"@agentclientprotocol/codex-acp":"2.5.0"}}\n' > "$T/package.json"
printf '{}\n' > "$T/zevbox-release.json"
container image pull --progress none --platform linux/arm64 docker.io/library/ubuntu:rolling
container build -f "$T/Containerfile" --target codex --tag local/zevbox-c1-probe:1 \
  --build-arg BASE_IMAGE=docker.io/library/ubuntu:rolling \
  --build-arg ZEVBOX_UID="$(id -u)" --build-arg ZEVBOX_GID="$(id -g)" \
  --build-arg APT_PACKAGES= "$T" > "$T/build.log" 2>&1
echo "build rc=$?"
bash docs/codex_annex/verify_image.sh local/zevbox-c1-probe:1
container image delete local/zevbox-c1-probe:1
container builder stop; container builder delete
rm -rf "$T"
```

Attendu : `build rc=0`, puis `verify_image: ALL OK`. La construction dure 2 à
5 minutes. Le message `Codex could not find bubblewrap on PATH` dans les journaux de
Codex est normal avec cette image.

**Documents, dans ce commit** :

- `zevbox_plan_V2.md` §8.8 (« contenu réel ») : recopier l'étape `codex` et la ligne de
  commentaire `--target` exactement comme dans `assets/Containerfile`.
- `docs/ARCHITECTURE.md` §6 : « The image also has a `codex` stage, prepared for the
  Codex agent. Nothing exposes it to users yet. »
- `docs/fr/ARCHITECTURE.md` §6 : « L'image comporte aussi une étape `codex`, préparée
  pour l'agent Codex. Rien ne l'expose encore aux utilisateurs. »
- `docs/PROGRESS.md` : C1 à ✅ avec le résultat de `verify_image.sh`.

Commit : `feat: prepare Codex ACP image and release components`.

## 5. C2a et C2 Relais, activation, authentification

### 5.1 C2a Version ACP (commit séparé, avant C2)

Avant C2a, `src/` ne contrôlait `protocolVersion` nulle part : une v2 passerait, avec
ses capacités dans un autre champ. Ce contrôle est obligatoire (SPEC §6), et il
s'applique à tous les agents puisque le relais est commun.

1. Appliquer `C2a_relay_version.patch`. Il ajoute le refus dans
   `src/relay/policy.rs` et `tests/codex_relay_test.rs`.
2. Lancer `make check`. Attendu : vert, 332 tests réussis.

**Documents, dans ce commit** (le texte du message doit rester exact) :

- `zevbox_plan_V2.md` §7.3, ligne `initialize` : commencer la cellule par
  « Refuser par `-32602` si `params.protocolVersion` n'est pas `1`, sans rien
  transmettre (`zevbox: ACP protocol version <v> is not filtered by this zevbox (only
  version 1). Fix: update zevbox, or use a Zed that speaks ACP version 1.`). Sinon : ».
- `zevbox_plan_V2.md` §7.3, ligne *réponse à `initialize`* : commencer par « Si
  `result.protocolVersion` n'est pas `1`, remplacer la réponse par l'erreur `-32600`
  de même texte. Sinon : ».
- `docs/ARCHITECTURE.md` §4, tableau des verdicts, ligne `initialize` : remplacer
  **Rewritten** par « **Rewritten**, or refused with error -32602 unless
  `protocolVersion` is 1 » et ajouter à la raison « Another ACP version would move
  capabilities to fields the relay does not filter. ».
- `docs/fr/ARCHITECTURE.md`, même ligne : « **Réécrit**, ou refusé par l'erreur -32602
  si `protocolVersion` n'est pas 1 » et « Une autre version d'ACP déplacerait les
  capacités dans des champs que le relais ne filtre pas. ».
- `README.md`, « Troubleshooting by error message » : « **`zevbox: ACP protocol version
  <v> is not filtered by this zevbox`** — Zed speaks an ACP version the relay does not
  filter yet: update Zevbox. ».
- `docs/fr/README.md`, « Dépannage par message d'erreur » : même entrée, « Zed parle une
  version d'ACP que le relais ne filtre pas encore : mettez Zevbox à jour. ».
- `docs/PROGRESS.md` : C2a à ✅.

Commit : `relay: refuse ACP versions it does not filter`.

### 5.2 C2 Appliquer l'activation

Appliquer `C2_activation_auth.patch`. Il contient, et ne doit être ni découpé ni
réordonné, car Codex ne doit jamais exister sans son auth :

| Fichier | Changement |
|---|---|
| `src/agents.rs` | Quatrième entrée, après oMLX, `opt_in: false`. |
| `src/cli/mod.rs` | `AgentArg::Codex` et texte d'aide. |
| `src/paths.rs` | `state/codex` dans `all_dirs`. |
| `src/cli/update.rs` | Branche Codex ; création de `state/<agent>` avant `promote`, pour les installations antérieures à l'agent. |
| `src/cli/auth.rs` | `auth_codex`, `CODEX_HOME` séparé, `keep_codex_login`, `has_codex_login`, helper `login_run_spec`. `auth_run_spec` garde sa signature. |
| `src/relay/policy.rs` | `api-key` masqué pour `codex` seulement (SPEC §5). |
| `src/console/{i18n,menu,app,ui}.rs` | Trois descriptions et les tests qui suivent `AGENTS`. |
| `tests/r3_mounts_test.rs` | Release Codex synthétique, pour que `setup` n'aille pas sur le réseau. |
| `tests/codex_auth_test.rs`, `tests/codex_relay_test.rs` | Nouveaux tests. |
| `Cargo.toml` | Description du paquet. |

Le test `screen_has_banner_two_panels_and_keys` contrôle désormais le bandeau abrégé à
95 colonnes au lieu de 80. C'est une décision de l'humain, pas un assouplissement :
avec quatre agents le bandeau peut dépasser 80 colonnes, et il suffit d'agrandir le
terminal. Ne pas refaire le bandeau.

### 5.3 C2 Contrôle des tests

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --no-fail-fast 2>&1 | grep -E '^test .* FAILED$'
```

Attendu : la première commande n'affiche rien ; la deuxième se termine par `Finished`,
sans ligne `warning` ni `error` ; la troisième affiche **exactement** ces deux lignes
(les tests protégés comptent encore trois agents) :

```text
test entries_for_builds_names_and_args ... FAILED
test every_entry_name_has_managed_prefix ... FAILED
```

Tout autre échec : §1.4 (échec de délai) puis §1.5.

### 5.4 C2 Preuve réelle en installation jetable

Ne pas attendre l'accord protégé pour cette étape : elle n'en dépend pas.

```bash
cargo build
W=$(bash docs/codex_annex/isolated_env.sh target/debug/zevbox)
( . "$W/env.sh"; "$W/zevbox-bin" init; "$W/zevbox-bin" update --agent codex )
IMAGE=$(python3 -c 'import json,sys,glob;print(json.load(open(glob.glob(sys.argv[1]+"/state/releases/codex-*/lock.json")[0]))["image"])' "$W")
bash docs/codex_annex/verify_image.sh "$IMAGE"
( . "$W/env.sh"; python3 docs/codex_annex/verify_launch.py "$W" )
( . "$W/env.sh"; "$W/zevbox-bin" doctor )
python3 -c 'import json,sys;print(list(json.load(open(sys.argv[1]))["agent_servers"]))' "$W/zed/settings.json"
bash docs/codex_annex/cleanup_isolated.sh "$W"
```

Attendu :

- `update` affiche `Codex: promoted to codex-<date>` (1 à 3 minutes) ;
- `verify_image: ALL OK` puis `verify_launch: ALL OK` ;
- `doctor` : `[WARN] agent:codex` « not authenticated », `[OK  ] permissions`.
  Les `[WARN]` de Claude et Antigravity sont normaux dans cette installation jetable ;
- la dernière commande Python affiche `['zevbox · Codex']`.

`verify_launch.py` envoie une clé synthétique à OpenAI, qui répond 401 : c'est voulu,
cela prouve le renvoi vers `zevbox auth codex`. Il ne consomme aucun quota. Si un
contrôle échoue, ne pas lancer le nettoyage avant d'avoir rapporté (§1.5).

### 5.5 C2 Documents

Mettre à jour tous les documents selon le §8, dans ce commit. Annoncer la connexion par
le terminal ; la connexion depuis la console reste annoncée comme arrivant avec C3.

### 5.6 C2 Diff protégé, seule étape qui attend l'humain

L'ajout en fin de table étend deux attentes de `tests/zed_settings_test.rs` (aucune
assertion de sécurité retirée). Ce diff est dans
`docs/codex_annex/protected_zed_settings.diff`. Il est vérifié : `git apply --check`
réussit sur le fichier actuel, et ses 23 tests passent une fois C2 et ce diff appliqués.

```diff
diff --git a/tests/zed_settings_test.rs b/tests/zed_settings_test.rs
--- a/tests/zed_settings_test.rs
+++ b/tests/zed_settings_test.rs
@@ -67,5 +67,6 @@ fn entries_for_builds_names_and_args() {
     let got = entries_for(&AGENTS, BIN);
     let mut expected = my_entries();
     expected.push(entry("zevbox · oMLX", "omlx"));
+    expected.push(entry("zevbox · Codex", "codex"));
     assert_eq!(got, expected);
 }
@@ -84,6 +85,6 @@ fn one_entry_per_agent_without_project_suffix() {
 #[test]
 fn every_entry_name_has_managed_prefix() {
     let got = entries_for(&AGENTS, BIN);
-    assert_eq!(got.len(), 3);
+    assert_eq!(got.len(), 4);
     assert!(got.iter().all(|e| e.name.starts_with(MANAGED_PREFIX)));
 }
```

Quand tout le reste de C2 est prêt, envoyer à l'humain ce message, avec le diff, et
**attendre sa réponse** :

> C2 est prêt : tests, preuve réelle et documents sont faits. Il reste une seule
> modification protégée à approuver : `tests/zed_settings_test.rs` passe de 3 à
> 4 agents, sans retirer d'assertion de sécurité. Voici le diff. Puis-je l'appliquer ?
> Il faudra ensuite resceller la ligne du manifeste avec la commande ci-dessous.

Après accord explicite seulement :

```bash
git apply docs/codex_annex/protected_zed_settings.diff
```

Puis l'humain lance le rescellement, limité à cette ligne :

```bash
perl -i -pe 'BEGIN{chomp($h=`shasum -a 256 tests/zed_settings_test.rs`)} $_="$h\n" if m{  tests/zed_settings_test\.rs$}' tests/PROTECTED.sha256
shasum -a 256 -c tests/PROTECTED.sha256 --quiet && echo "manifest OK"
```

Ensuite : `make check` doit être vert, avec 343 tests réussis, et le manifeste conforme.
`docs/PROGRESS.md` : C2 à ✅, avec la mention que l'auth réelle reste à faire en C4.

Commit : `feat: enable standard Codex agent with native authentication`. Il contient le
code, les documents, le test protégé et son manifeste.

## 6. C3 Console et parité de gestion

1. Appliquer `C3_console.patch`. Il ajoute `Action::AuthCodex` dans
   `src/console/menu.rs` et ses textes EN/FR dans `src/console/i18n.rs`, et complète
   `MenuAuthDesc`. La feuille Codex est placée **avant** oMLX : les tests clavier
   arrivent à oMLX avec `Up` depuis le premier élément, ne pas la déplacer.
2. Lancer `make check`. Attendu : vert, 343 tests réussis. Les tests de la console
   rendent l'écran dans un terminal simulé : ne pas lancer la console plein écran sans
   terminal interactif. Le contrôle à l'œil se fait avec l'humain en C4 (§7.3).

**Documents, dans ce commit** : voir §8.2, ligne C3. Compteur : 23 → 24 actions.

Commit : `feat: expose Codex throughout the console`.

Puis, dans un commit séparé, retirer les quatre correctifs appliqués et le diff
protégé, devenus inutiles. Garder les quatre scripts : ils requalifient une nouvelle
version de Zed ou de l'adaptateur.

```bash
git rm docs/codex_annex/C1_image_release.patch docs/codex_annex/C2a_relay_version.patch \
  docs/codex_annex/C2_activation_auth.patch docs/codex_annex/C3_console.patch \
  docs/codex_annex/protected_zed_settings.diff
```

Mettre à jour en conséquence le §1.3 de ce plan, `docs/codex_annex/README.md` et les
autres mentions des correctifs : `AGENTS.md` (paragraphe Codex), la note d'en-tête de
`zevbox_plan_V2.md`, `docs/DECISIONS.md` (backlog 15) et `docs/PROGRESS.md`. Vérifier avec
`grep -rn 'codex_annex' README.md AGENTS.md zevbox_plan_V2.md docs`.
Commit : `docs: remove applied Codex patches`.

## 7. C4 Qualification réelle et acceptation humaine

Les constructions et diagnostics sans modèle se font par l'exécutant, en installation
jetable. La connexion au compte, l'interface Zed et les requêtes consommant du quota
nécessitent l'humain (V2 §0.8). Préparer scénarios et commandes avant de le solliciter.

### 7.1 État isolé et preuves sans quota

Créer l'installation avec `isolated_env.sh` (§5.4). Elle fixe `HOME`, le projet
`$W/home/code/proj` dans une zone autorisée (I2), et un `settings.json` de Zed : sans ce
fichier, `update` ne synchronise pas Zed. Ne jamais y désigner le dépôt Zevbox.

- [ ] `verify_image.sh` et `verify_launch.py` : `ALL OK`. Versions de Zed, adaptateur,
  moteur et protocole consignées.
- [ ] Installation neuve : `( . "$W/env.sh"; "$W/zevbox-bin" setup )` construit Claude,
  Antigravity et Codex, puis affiche `zevbox setup complete`. Sans terminal, il affiche
  `skipping authentication (not a terminal)`.
- [ ] Installation existante : dans une autre installation jetable, construire Claude et
  Antigravity avec `update --agent`, puis lancer `setup`. Constater la mise à jour globale
  des trois agents. Relancer `setup` : `releases: up to date`, aucune construction.
- [ ] Échec de construction : `( . "$W/env.sh"; "$W/zevbox-bin" tools add zevbox-no-such-package )`
  échoue pendant `apt-get`. Attendu : code 1, `releases/active.json` inchangé,
  `build.log` utile et sans secret. Puis `( . "$W/env.sh"; "$W/zevbox-bin" tools remove zevbox-no-such-package )` : il reconstruit l'image (code 0) et vide la liste `apt` de `tools.json`.
- [ ] Deux releases : relancer `update --agent codex`, puis `rollback --agent codex`
  deux fois. L'entrée `current` de `active.json` alterne entre les deux releases.
- [ ] Ajout d'un outil : `tools add sl`, puis vérifier avec
  `container run --rm <image> dpkg -s sl`.
- [ ] Ciblage : `update --agent codex` ne change pas les releases actives des autres agents.
- [ ] `make check`, `make test-integration` et `make verify STEP=17` réussis ; empreintes
  conformes. Les tests réels historiques ne prouvent pas à eux seuls Codex.

### 7.2 Connexion et modèle réel avec l'humain

Préparer d'abord le déploiement avec l'humain : `make install`, puis `zevbox setup`. Sur
cette machine, `setup` reconstruit aussi Claude, Antigravity et oMLX (§8.4). Pour ne
construire que Codex : `zevbox update --agent codex`.

- [ ] `zevbox auth codex` : le terminal affiche `https://auth.openai.com/codex/device` et un
  code. L'humain valide dans le navigateur. Fin attendue : `codex: signed in.`
  Pas de code ni de jeton dans PROGRESS.
- [ ] Si le navigateur refuse le code : l'humain active la connexion par code d'appareil
  dans les réglages de sécurité de ChatGPT (workspace : un administrateur), puis relance.
- [ ] `zevbox auth codex --status` : `authenticated`. Droits : `state/codex` en 0700,
  `.codex` en 0700, `auth.json` en 0600. Une nouvelle VM relit le même état.
- [ ] Reconnexion : relancer `zevbox auth codex`, annuler avec Ctrl-C. Attendu : message
  `the Codex sign-in did not finish`, compte actuel toujours utilisable dans Zed.
- [ ] Compte révoqué à distance, si l'humain l'accepte : Zed affiche
  `Authentication required → run 'zevbox auth codex' in a macOS terminal.`, et
  `zevbox auth codex` reconnecte.
- [ ] Clé API, seulement si l'humain le souhaite et a une clé : parcours SPEC §5.
  Aucune obligation d'acquérir une clé.
- [ ] Prompt de test sur fichiers synthétiques : lecture d'un fichier connu, édition
  bénigne vérifiée, commande `uname -s` = `Linux` et cwd correct.
- [ ] Demande de précision par texte et mode de planification natif si proposé : absence
  de questionnaires structurés expliquée, dialogue utilisable, `elicitation/*` non ouvert.
- [ ] Permission acceptée puis refusée : le refus empêche l'action. Annulation en cours de
  travail suivie d'un nouveau prompt fonctionnel.
- [ ] Commande demandant du réseau en mode Auto review (`curl https://example.com`) :
  noter le comportement réel (refus, demande d'autorisation ou revue).
- [ ] Modèle et effort sélectionnables selon le compte ; mode natif utilisable, changement
  de mode dans la VM ; aucun passage forcé en Full access.
- [ ] Texte, fichier joint et image synthétique si supportée ; aucun témoin hors projet
  accessible. `.zed` masqué et `.git` racine non modifiable.
- [ ] Marqueurs synthétiques de prompt absents de `logs/*.log`, en succès comme sur erreur.
  L'historique privé sous `state/codex/.codex/sessions` contient le prompt : c'est prévu.
- [ ] Le message `Codex could not find bubblewrap on PATH` n'apparaît pas dans Zed. S'il
  apparaît, le noter dans les limites.

### 7.3 Essai Zed et cycle de vie

- [ ] Installer les entrées avec l'humain ; sauvegarde Zed vérifiée, entrée `zevbox · Codex`
  unique, entrées non gérées inchangées, réinstallation idempotente. Ne jamais pointer Zed
  sur `target/`.
- [ ] Refaire les opérations courantes depuis Zed ; sélecteurs modèle/mode/effort réellement
  visibles ; choix préservés par `zed install` et mise à jour.
- [ ] Reprendre une session après relance dans le même projet ; ouvrir deux threads et
  observer archivage, fermeture de fenêtre et arrêt explicite. Deux VM sur le même état : un
  essai court est passé le 2026-11-06 (bases SQLite intègres) ; confirmer sur un vrai usage.
- [ ] EOF, SIGTERM, crash et SIGKILL du relais : aucune VM Codex abandonnée après le délai du
  contrat ou son nettoyage prévu ; autres agents toujours présents.
- [ ] Console en français et en anglais : connexion interactive, retour au menu, état
  rafraîchi, listes, confirmation de reconnexion.
- [ ] Non-régression réelle de Claude et Antigravity, et oMLX s'il est installé, après les
  adaptations partagées ; conserver leurs comptes et réglages.
- [ ] Comparaison SPEC §9 relue avec les observations réelles. Les fonctions communes sont
  prouvées ; les nuances ne sont pas cachées derrière « même expérience ».
- [ ] Revue documentaire finale §8.5 terminée.

Noter commandes, versions, résultats, durée d'arrêt et observations, sans prompts, fichiers
utilisateur, codes de connexion ni secrets. Une fonctionnalité non vérifiée reste en
attente. Ne pas cocher C4 sur la seule foi d'un test simulé ou d'une réponse `initialize`.

Commit : `docs: record Codex ACP acceptance`, avec des commits séparés pour tout correctif
révélé par les essais.

## 8. Documentation obligatoire et remise à l'humain

La documentation fait partie de chaque étape. Il faut mettre à jour **tous les documents
concernés du projet** dans le même commit que le comportement. Une étape avec des documents
périmés n'est pas terminée. Les guides d'utilisation n'annoncent Codex disponible qu'à C2.

### 8.1 Documents à examiner et actualiser

Cette liste est un minimum : §8.3 donne la recherche à lancer dans tout le dépôt.

| Document | Sections à actualiser à C2 |
|---|---|
| `zevbox_plan_V2.md` | Note d'en-tête, §1, §2 (nouveau fait), §5.2, §5.4 (amendement), §8.1 lignes `auth`/`update`, §8.5, §8.6, §8.7, §11, §12. |
| `README.md`, `docs/fr/README.md` | Introduction, Quick start (bloc de commandes et schéma), Everyday use, Command reference (`auth`, `doctor`, `update`), What persists, Threat model, Troubleshooting. Quatre agents, dont trois standards. |
| `docs/USER_GUIDE.md`, `docs/fr/GUIDE_UTILISATEUR.md` | §1, Step 3, nouvelle section **4d. Codex** (ne pas renuméroter 4c), Check 3, §5, §6, Problem 1 et ses limites. |
| `docs/ARCHITECTURE.md`, `docs/fr/ARCHITECTURE.md` | §3, §4 (`api-key`), §6, nouvelle section **7c. Codex** après 7b, invariants et persistance. |
| `docs/SPEC_console.md` | Quatre agents et statut générique dès C2 ; croquis du §3 ; préciser que le bandeau peut dépasser 80 colonnes. Connexion Codex et 24 actions à C3. |
| `AGENTS.md` | Introduction, carte des modules, `Never` (`~/.codex`), renvois de reprise ; retirer « not implemented yet » seulement à C2. |
| `docs/DECISIONS.md`, `docs/PROGRESS.md` | Choix datés, versions, preuves par étape, attentes humaines. Backlog 15 clos à C4. |
| `docs/SPEC_codex.md`, ce plan | Statut de livraison, faits observés ; cocher seulement les étapes validées. |

`zevbox_plan_V1.md` et les journaux datés restent des références historiques : ne pas
réécrire leurs anciens résultats comme si Codex existait à l'époque.

### 8.2 Livrable documentaire de chaque étape

| Étape | Documentation à livrer avec le travail |
|---|---|
| C0 | Versions dans PROGRESS ; SPEC §8 et DECISIONS si elles ont changé. |
| C1 | V2 §8.8, phrase d'ARCHITECTURE EN/FR, PROGRESS (§4). Commandes utilisateur inchangées. |
| C2a | V2 §7.3, tableau d'ARCHITECTURE EN/FR, dépannage README EN/FR, PROGRESS (§5.1). |
| C2 | Toutes les mentions actives d'agents, commandes, auth, état, diagnostic et installation (§8.1). Connexion de console annoncée comme arrivant avec C3. |
| C3 | `docs/SPEC_console.md`, guides EN/FR (section console), compteur 24, AGENTS si la carte change. |
| C4 | Résultats réels, versions qualifiées, limites et dépannage ; rapprochement final. |

### 8.3 Recherche à lancer avant chaque commit de C2 et C3

```bash
grep -n -E 'Claude,? (and|et|/|,|ou|or) Antigravity|Claude / Antigravity|Claude/Antigravity|Antigravity (and|et|ou|or) oMLX|Claude, Antigravity|zevbox · oMLX|trois agents|three agents|3 agents|deux agents|two agents|both agents|les deux agents|<claude\|antigravity|claude\|antigravity|state/omlx|state/antigravity|zevbox/antigravity|zevbox auth antigravity' \
  README.md docs/fr/README.md docs/USER_GUIDE.md docs/fr/GUIDE_UTILISATEUR.md \
  docs/ARCHITECTURE.md docs/fr/ARCHITECTURE.md zevbox_plan_V2.md AGENTS.md \
  docs/SPEC_console.md docs/exploration_solutions_gemini.md
```

Avant C2, cette recherche trouve : README 10, README FR 9, guide 14, guide FR 14,
architecture 5, architecture FR 5, V2 13, AGENTS 3, SPEC console 1, exploration 8.
Examiner **chaque** ligne : soit elle énumère les agents et Codex s'y ajoute, soit elle
décrit un agent précis et reste telle quelle. Une recherche de « Codex » seule ne repère
pas une liste ancienne. Traiter les deux langues ensemble. Ne pas modifier les sorties CLI
citées pour les rendre plus jolies.

Compteurs attendus : 15 commandes principales inchangées ; 3 → 4 agents à C2, dont 3
standards et oMLX seul optionnel ; 23 → 24 actions de console à C3 ; 13 invariants
inchangés. Les entrées Zed suivent les releases actives.

### 8.4 Faits à écrire

Reprendre ces formulations pour rester exact. La dernière ligne ne s'écrit qu'une fois
observée en C4.

| Sujet | Anglais | Français |
|---|---|---|
| Connexion | `zevbox auth codex` runs the login in a throw-away VM. It prints an address and a one-time code: open the address in the Mac's browser, sign in with your ChatGPT account and enter the code. | `zevbox auth codex` lance la connexion dans une VM jetable. Elle affiche une adresse et un code à usage unique : ouvrez l'adresse dans le navigateur du Mac, connectez-vous avec votre compte ChatGPT et saisissez le code. |
| Activation | Device code sign-in is a beta feature of OpenAI. It may have to be enabled first: in your ChatGPT security settings for a personal account, by an admin for a workspace. | La connexion par code d'appareil est une fonction bêta d'OpenAI. Il faut parfois l'activer d'abord : dans les réglages de sécurité de ChatGPT pour un compte personnel, par un administrateur pour un workspace. |
| Reconnexion | Running `zevbox auth codex` again replaces the sign-in only once the new one is complete: a failed or cancelled attempt keeps the current account. | Relancer `zevbox auth codex` ne remplace la connexion qu'une fois la nouvelle terminée : un échec ou une annulation conserve le compte actuel. |
| Clé API | A valid API-key login stored by Codex is accepted, but Zevbox has no guided API-key prompt and Zed cannot send a key through the relay. | Une connexion par clé API valide enregistrée par Codex est acceptée, mais Zevbox n'a pas d'assistant de saisie de clé et Zed ne peut pas transmettre de clé à travers le relais. |
| Installation existante | On an existing installation, `setup` adds Codex through the global update: it also rebuilds the other standard agents, and oMLX if installed. `zevbox update --agent codex` builds Codex alone. | Sur une installation existante, `setup` ajoute Codex par la mise à jour globale : il reconstruit aussi les autres agents standards, et oMLX s'il est installé. `zevbox update --agent codex` construit Codex seul. |
| Sandbox | Codex's own sandbox runs inside the VM, in addition to Zevbox's isolation. It has no network by default, and `ps` does not work inside it. | Le sandbox de Codex tourne dans la VM, en plus de l'isolation de Zevbox. Il n'a pas de réseau par défaut, et `ps` n'y fonctionne pas. |
| État privé | Codex keeps its history and databases in `state/codex/.codex`. Prompts are stored there, never in Zevbox's journals. | Codex garde son historique et ses bases dans `state/codex/.codex`. Les prompts y sont conservés, jamais dans les journaux de Zevbox. |
| Bandeau | With four agents the console banner may exceed 80 columns: widen the terminal to read the end of the line. | Avec quatre agents, le bandeau de la console peut dépasser 80 colonnes : agrandissez le terminal pour lire la fin de la ligne. |
| Questionnaires (après C4) | Codex's structured questions are not displayed in Zed. | Les questionnaires structurés de Codex ne s'affichent pas dans Zed. |

### 8.5 Revue finale et remise

- [ ] Inventaire des documents examinés, liste des fichiers modifiés et justification des
  documents de la table laissés inchangés, s'il y en a.
- [ ] Comportement, aide CLI, sorties et guides cohérents ; exemples utilisables, liens locaux
  valides, paires EN/FR alignées, schémas et compteurs vérifiés.
- [ ] Recherche finale des anciennes règles : Codex optionnel, exclusivité ChatGPT, rejet
  systématique des caches API, v1 figée, activation avant auth, promesse d'un `setup` limité à
  Codex, sauvegarde et restauration de cache. Ne les conserver que pour expliquer une décision.
- [ ] `git diff --check`, gates requises et empreintes protégées conformes ; aucune donnée de
  compte ou de projet utilisateur dans les docs ; statuts cohérents partout.

La remise finale comporte les commits, résultats de gates, preuves Codex, revue documentaire,
état du diff protégé et limites restantes. Ne pas annoncer « parité complète » si un essai Zed
ou une opération courante manque.

## 9. Message de transmission à l'agent exécutant

> Implémente Codex en ACP dans Zevbox en suivant `docs/PLAN_codex.md` à la lettre, dans
> l'ordre : C0, C1, C2a, C2, C3, C4. Lis d'abord entièrement `AGENTS.md`,
> `zevbox_plan_V2.md`, `docs/PROGRESS.md`, `docs/DECISIONS.md` et `docs/SPEC_codex.md`.
>
> Le code est déjà écrit et testé : `docs/codex_annex/` contient un correctif par étape, et
> des scripts qui répondent `PASS` ou `FAIL`. Ton travail : appliquer chaque correctif,
> vérifier les résultats attendus, écrire les documents EN/FR et préparer les étapes de
> l'humain. Les décisions de conception sont prises : ne les rouvre pas et ne réécris pas
> ce code autrement.
>
> Codex est un agent standard (`opt_in: false`), comme Claude et Antigravity : `setup` construit
> sa release manquante et propose sa connexion ; `update` sans `--agent` l'inclut dès que Zevbox
> est initialisé, même sans release. oMLX reste seul optionnel.
>
> Vérifie d'abord que `git status --short` est vide (§1.2). Un sujet par commit. Ajoute les
> fichiers par leur nom. Fais `make check` avant chaque commit. Ne publie et ne pousse rien.
> Ne lance pas de sous-agents. Le nom du projet dans la prose est Zevbox.
>
> Arrête-toi et demande à l'humain dans les cas du §1.5. En particulier : ne modifie aucun
> fichier protégé sans son accord explicite (§5.6), ne passe jamais en Full access et n'ouvre
> jamais `elicitation/*` pour faire passer un contrôle, ne contourne pas un `FAIL`.
>
> La documentation est un livrable obligatoire : applique le §8 à chaque étape, dans le même
> commit que le comportement, avec les versions EN/FR. Termine par la revue du §8.5 et indique
> les fichiers examinés et modifiés. Distingue tests simulés et preuves réelles. Termine avec
> les preuves et les nuances observées, ou un blocage documenté.
