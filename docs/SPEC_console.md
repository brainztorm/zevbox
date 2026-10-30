# Spécification — `zevbox console` : interface plein écran au clavier

- **Statut** : implémentée, fusionnée dans `main` en **version 0.2.0** (2026-10-30). Conception validée
  par 👤 le 2026-10-30 ; réponses par défaut du §1 acceptées après essai (« c'est très bien »),
  puis retours intégrés (explications, cadre bleu du panneau actif, trois couleurs).
- **But** : ne plus avoir à retenir les commandes et leurs options. On lance `zevbox console` :
  à gauche les commandes, à droite ce qu'elles font ; Entrée exécute, Échap revient, les flèches
  naviguent ; un « Êtes-vous sûr ? » Oui/Non protège les actions qui ont un impact.

## 1. Choix (réponses par défaut aux questions ouvertes)

| Question | Choix pour la v1 |
|---|---|
| Nom | sous-commande **`zevbox console`** du binaire unique (règle du projet : un seul binaire). Pas de binaire `zevbox_console` séparé. |
| Langue | interface **en anglais ou en français** (2026-10-30, après la 0.2.0 qui était en français seulement) : `zevbox console --lang en\|fr`, sinon la première valeur non vide de `LC_ALL`, `LC_MESSAGES`, `LANG` ; une valeur qui commence par `fr` donne le français, tout le reste (ou rien) l'anglais ; `--lang` invalide → erreur clap, code 2. Tous les textes visibles sont dans `src/console/i18n.rs` (une clé `Msg` par texte, les deux langues, test exhaustif). La sortie des commandes reste en anglais (règle du projet). |
| Bandeau d'état | **oui**, une ligne en haut : versions actives et nombre d'agents en cours. |
| Confirmations | la liste du §6. |
| Commande équivalente | **oui**, affichée dans le panneau de description. |

## 2. Principe d'architecture

- **La console ne réimplémente aucune commande.** Une action = lancer le binaire courant
  (`std::env::current_exe()`) avec des arguments, comme on le ferait au terminal. La ligne de
  commande reste la seule source de vérité ; la console n'est qu'un menu au-dessus.
- **Pas de shell** (I7) : `std::process::Command` sur le binaire, arguments un par un.
- **Secrets** : la clé oMLX est passée sur l'**entrée standard** de `zevbox auth omlx`, jamais en
  argument ni en variable d'environnement ; elle n'est ni affichée (saisie masquée) ni journalisée.
- **Lecture directe autorisée, écriture interdite** : pour le bandeau et les listes de choix, la
  console peut lire l'état avec les fonctions de la bibliothèque (`release::load_active`,
  `instances::list`, `tools::load`, `agents::AGENTS`, `Paths`). Toute modification passe par une
  commande lancée.
- `launch` n'est jamais concerné : la console n'est jamais lancée par Zed (I6 inchangé).
- **Framework** : `ratatui` (rendu) + `crossterm` (terminal, clavier). Tests avec le
  `TestBackend` de ratatui.
- **Code** : module `src/console/` (modèle du menu pur et testable, état de l'application,
  rendu, exécution des commandes), branché dans `src/cli/mod.rs` comme la commande `console`.
  Règles du projet : pas de `unwrap`/`expect`/`panic` (I11), erreurs propagées, style des fichiers
  `WORKED EXAMPLE`. Le terminal est **toujours restauré** (mode brut, écran alterné, curseur),
  y compris en cas d'erreur.

## 3. Écran

```
 zevbox ─ Claude 0.99.1 · Antigravity 1.4.5 · oMLX non installé ─ 2 en cours
┌ zevbox › Mises à jour ───────────────┐┌ Tout mettre à jour ──────────────────┐
│  Vérifier s'il y a des mises à jour  ││Construit une image neuve pour Claude,│
│▸ Tout mettre à jour                  ││Antigravity et oMLX s'il est installé…│
│  Mettre à jour un agent              ││                                      │
│  Revenir à la version précédente     ││Durée : 1 à 3 min par agent · réseau  │
│                                      ││Commande : zevbox update              │
└──────────────────────────────────────┘└──────────────────────────────────────┘
 Menu : ↑↓ choisir · Entrée ouvrir · PgUp/PgDn texte · Échap retour · q quitter
```

Le croquis est en français ; en anglais, mêmes zones avec les libellés anglais (§5) : « Menu: ↑↓
choose · Enter open · PgUp/PgDn text · Esc back · q quit », « Command: zevbox update », « N agents
running » (« N running » abrégé), « not installed ».

- **Bandeau** (haut) : version de la release active de chaque agent installé (`agentVersion`
  du lock), ou « non installé » ; nombre d'instances enregistrées (« N agents en cours », abrégé
  en « N en cours » si la ligne ne tient pas). Rafraîchi au retour d'une commande.
- **Gauche** (40 colonnes) : la liste du niveau courant ; un sous-menu est marqué `›`. Titre =
  chemin (`zevbox › Mises à jour`), coupé par la gauche avec « … » s'il est trop long.
- **Droite** : titre = l'entrée surlignée ; une explication en clair pour un non-spécialiste
  (ce que ça fait, quand s'en servir, ce que ça change et ne change pas, les conséquences), l'état
  de connexion pour les entrées de connexion, durée et réseau si utiles, puis la ligne
  `Commande : zevbox …` (avec les paramètres choisis quand ils sont connus). Une explication plus
  longue que le panneau se lit avec PgUp/PgDn (la bordure du bas l'indique). Quand une action
  demande un choix, la liste s'affiche ici, avec l'explication et la commande.
- **Focus** : le panneau qui reçoit les touches a sa bordure et son titre en bleu, l'autre est
  gris : à gauche pendant la navigation dans les menus, à droite dès qu'une action est ouverte
  (liste, saisie, sortie, question).
- **Bas** : où vont les touches (« Menu: » / « Menu : », « List on the right: » / « Liste à
  droite : », « Input: » / « Saisie : », « Output on the right: » / « Sortie à droite : »,
  « Question: » / « Question : »), puis les touches valables à cet endroit.
- **Couleurs** : trois, une signification chacune, prises dans la palette du terminal (elles
  suivent son thème) : **bleu** = là où l'on est (bordure et titre du panneau actif, ligne
  sélectionnée en blanc gras sur bleu, bouton choisi) ; **vert** = tout va bien (`✓ terminé`,
  versions installées, « connecté ») ; **rouge** = attention (`✗ échec …`, cadre et titre de
  « Êtes-vous sûr ? », « non installé », « non connecté »). Le reste est dans la couleur par
  défaut, ou en gris pour le secondaire (durée, réseau, `Commande :`, lignes `$ zevbox …`). Les
  lignes écrites sur stderr ne sont pas colorées : elles commencent par `!`.
- Terminal trop petit (moins de 80 × 20) : message « Enlarge the window (80 × 20 minimum) » /
  « Agrandissez la fenêtre (80 × 20 minimum) ».
- Pas de terminal (stdin ou stdout non TTY) : `zevbox console` refuse avec
  `zevbox console needs an interactive terminal.` et le code 2.

## 4. Clavier

| Touche | Effet |
|---|---|
| ↑ ↓ | se déplacer (la liste boucle) |
| Entrée ou → | ouvrir le sous-menu, ou lancer l'action |
| Échap ou ← | revenir au niveau précédent ; au premier niveau, quitter |
| q | quitter (sauf pendant une saisie de texte) |
| ← → ou Tab | dans « Êtes-vous sûr ? » : passer de Oui à Non |
| PgUp / PgDn | dans le menu : faire défiler une explication plus longue que le panneau |
| PgUp / PgDn, ↑ ↓ | faire défiler la sortie d'une commande |
| Ctrl-C | quitter proprement (comme `q`) |

Les touches sont les mêmes dans les deux langues ; seuls les libellés de la ligne du bas changent
(« Esc back » / « Échap retour », « Enter confirm » / « Entrée valider »…).

## 5. Menus (v1)

Chaque feuille = une commande `zevbox …`. Libellés en anglais, le français entre parenthèses.

- **Running agents** (Agents en cours)
  - Show running agents (Voir les agents en cours) — `status`
  - Stop an agent (Arrêter un agent) — choix dans la liste des instances → `stop <id>` (confirmation)
  - Stop all agents (Arrêter tous les agents) — `stop` (confirmation)
  - Read a session log (Lire le journal d'une session) — choix de l'instance, ou « The most recent » (« La plus récente ») → `logs [<id>]`
- **Updates** (Mises à jour)
  - Check for updates (Vérifier s'il y a des mises à jour) — `update --check`
  - Update everything (Tout mettre à jour) — `update` (confirmation)
  - Update one agent (Mettre à jour un agent) — choix de l'agent → `update --agent <a>` (confirmation)
  - Go back to the previous version (Revenir à la version précédente) — choix de l'agent → `rollback --agent <a>` (confirmation)
- **Agent sign-in** (Connexion des agents)
  - Sign-in status (État des connexions) — `auth <a> --status` pour chaque agent, l'un après l'autre
  - Sign in to Claude (Se connecter à Claude) — `auth claude` (interactif, §7 ; confirmation si déjà connecté)
  - Sign in to Antigravity (Se connecter à Antigravity) — `auth antigravity` (interactif ; confirmation si déjà connecté)
  - Connect oMLX (Connecter oMLX) — saisie de l'adresse (pré-remplie avec l'actuelle), de la clé (masquée ;
    vide = garder l'actuelle) et du modèle (optionnel) → `auth omlx --url <u> [--model <m>]`,
    clé sur stdin (confirmation si déjà connecté)
- **Linux tools** (Outils Linux)
  - Show added tools (Voir les outils ajoutés) — `tools list`
  - Add a tool (Ajouter un outil) — saisie d'un ou plusieurs noms séparés par des espaces → `tools add …` (confirmation)
  - Remove a tool (Retirer un outil) — choix dans la liste → `tools remove <p>` (confirmation)
- **Diagnosis and cleanup** (Diagnostic et ménage)
  - Full diagnosis (Diagnostic complet) — `doctor`
  - Clean up (Faire le ménage) — `clean`
  - Clean up, old images too (Ménage avec les anciennes images) — `clean --images` (confirmation)
- **Zed**
  - Preview what would be written (Voir ce qui serait écrit) — `zed install --dry-run`
  - Install the zevbox entries (Installer les entrées zevbox) — `zed install`
  - Remove the zevbox entries (Retirer les entrées zevbox) — `zed uninstall` (confirmation)
- **Shell in a VM** (Shell dans une VM) — choix de l'agent, puis saisie du projet (pré-rempli avec le dossier
  courant) → `shell <a> --project <p>` (interactif, §7)
- **First-time setup** (Première installation) — `setup` (interactif, §7)

Non proposés : `launch` (réservé à Zed), `init` (fait par `setup`), `logs --path`, `zed install --settings`.

## 6. « Are you sure? » (« Êtes-vous sûr ? »)

- Boîte centrée : la question, puis **la conséquence concrète** (par exemple « The threads open in
  Zed will be cut. » / « Les threads ouverts dans Zed seront coupés. »), puis `[ Yes ]   [ No ]` /
  `[ Oui ]   [ Non ]`.
- **No / Non est sélectionné par défaut** ; Échap = Non ; Entrée valide le bouton sélectionné.
- Demandent une confirmation : arrêter un agent ou tous, tout mettre à jour, mettre à jour un agent,
  revenir en arrière, ajouter ou retirer un outil (reconstruit les images et met les agents à jour),
  ménage avec les anciennes images, retirer les entrées Zed, refaire la connexion d'un agent déjà
  connecté, interrompre une commande en cours.
- N'en demandent pas : tout ce qui ne fait que lire (état, `doctor`, journaux, vérification des mises
  à jour, liste des outils, aperçu Zed, état des connexions), `clean`, `zed install`, `setup`.

## 7. Exécution

- **Commande non interactive** : stdout et stderr lus en direct et affichés dans le panneau de
  droite (les deux mélangés dans l'ordre d'arrivée, stderr marqué par `!`, sans couleur), avec
  « running… » / « en cours… ». À la fin : `✓ done` / `✓ terminé`, `✗ failed (code N)` /
  `✗ échec (code N)`, `✗ failed (interrupted)` / `✗ échec (interrompue)` ou `✗ failed (could not
  start the command)` / `✗ échec (impossible de lancer la commande)`. La sortie défile (PgUp/PgDn, ↑ ↓).
  Échap revient au menu (une fois la commande terminée).
- **Pendant une commande** : Échap ouvre « Interrupt the command? » / « Interrompre la commande ? » ; Oui envoie SIGTERM au
  processus, puis SIGKILL après 5 s s'il tourne encore.
- **Commande interactive** (`shell`, `auth claude`, `auth antigravity`, `setup`) : la console
  quitte l'écran alterné et le mode brut, lance la commande sur le vrai terminal (stdin/stdout/stderr
  hérités), attend sa fin, affiche « Press Enter to return to the console » / « Appuyez sur Entrée pour revenir à la console », puis se
  redessine.
- **Bandeau et listes** relus après chaque commande.

## 8. Tests (v1)

- Modèle du menu (pur) : arborescence, navigation, retour, commande générée pour chaque feuille,
  liste des actions confirmées (§6) — tests unitaires.
- Boîte « Êtes-vous sûr ? » : Non par défaut, Tab/←/→, Échap = Non.
- Rendu : `TestBackend` 80 × 24, présence du bandeau, des deux panneaux, de la ligne des touches ;
  message « Enlarge the window » / « Agrandissez la fenêtre » sous 80 × 20 ; libellés anglais
  avec `Lang::En`, français avec `Lang::Fr`.
- Textes : chaque clé de `i18n.rs` a ses deux langues, non vides, avec les mêmes paramètres ;
  détection de la langue (`fr_FR.UTF-8` → fr ; `en_US`, `C`, vide → en ; `LC_ALL` l'emporte sur
  `LANG`) ; `--lang` (en, fr, valeur invalide refusée).
- Construction de la commande `auth omlx` : la clé n'apparaît jamais dans les arguments.
- Aucun test protégé modifié. Pas de test de bout en bout du terminal réel (essai manuel 👤).

## 9. Docs (même commit que le code, règle absolue 1)

`README.md` (+ `docs/fr/README.md`) : ligne `zevbox console` dans le tableau des commandes et une
phrase dans l'usage quotidien ; `docs/USER_GUIDE.md` (+ `docs/fr/GUIDE_UTILISATEUR.md`) : courte
section « La console » ; `zevbox_plan_V2.md` §8.1 : ligne `console` ; `docs/ARCHITECTURE.md` (+ fr) :
une phrase (la console lance les commandes, ne les réimplémente pas) ; `AGENTS.md` : module map
(`console/`) ; `docs/DECISIONS.md` : décision du 2026-10-30 ; les compteurs de commandes (« 14
commands ») mis à jour.
