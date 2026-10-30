//! Every string `zevbox console` shows, in English and in French (DECISIONS
//! 2026-10-30). One table, one key per string: the rest of the console asks
//! `lang.t(Msg::…)` and never compares languages itself. The output of the
//! commands it runs stays as they print it (English).
//!
//! Language: `zevbox console --lang en|fr`, else the first non-empty of
//! `LC_ALL`, `LC_MESSAGES`, `LANG` (the POSIX order); a value starting with
//! `fr` gives French, anything else (or nothing) English.

/// Interface language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum Lang {
    #[default]
    En,
    Fr,
}

impl Lang {
    /// Language of a locale setting: the first non-empty value wins, as in POSIX
    /// (`LC_ALL` overrides `LC_MESSAGES`, which overrides `LANG`).
    pub fn from_locale(
        lc_all: Option<&str>,
        lc_messages: Option<&str>,
        lang: Option<&str>,
    ) -> Self {
        let locale = [lc_all, lc_messages, lang]
            .into_iter()
            .flatten()
            .map(str::trim)
            .find(|v| !v.is_empty());
        match locale {
            Some(l) if l.to_ascii_lowercase().starts_with("fr") => Lang::Fr,
            _ => Lang::En,
        }
    }

    /// [`Lang::from_locale`] on the environment of this process.
    pub fn from_env() -> Self {
        let var = |name: &str| std::env::var(name).ok();
        Self::from_locale(
            var("LC_ALL").as_deref(),
            var("LC_MESSAGES").as_deref(),
            var("LANG").as_deref(),
        )
    }

    /// The string of `msg` in this language.
    pub fn t(self, msg: Msg) -> &'static str {
        let (en, fr) = msg.pair();
        match self {
            Lang::En => en,
            Lang::Fr => fr,
        }
    }

    /// [`Lang::t`] with `{0}`, `{1}`… replaced by `args`, in order.
    pub fn f(self, msg: Msg, args: &[&str]) -> String {
        fill(self.t(msg), args)
    }
}

/// Replaces `{0}`, `{1}`… in `template` by `args`.
pub fn fill(template: &str, args: &[&str]) -> String {
    let mut out = template.to_string();
    for (i, arg) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), arg);
    }
    out
}

/// Declares [`Msg`], its list [`Msg::ALL`] (for the exhaustive test) and the
/// English/French pair of each key.
macro_rules! messages {
    ($($key:ident => $en:expr, $fr:expr;)*) => {
        /// One user-visible string of the console.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Msg { $($key),* }

        impl Msg {
            pub const ALL: &'static [Msg] = &[$(Msg::$key),*];

            /// (English, French).
            pub fn pair(self) -> (&'static str, &'static str) {
                match self { $(Msg::$key => ($en, $fr)),* }
            }
        }
    };
}

messages! {
    // ── Banner ────────────────────────────────────────────────────────────
    BannerNotInstalled => "zevbox is not installed: run First-time setup",
        "zevbox n'est pas installé : lancez Première installation";
    NotInstalled => "not installed", "non installé";
    RunningNone => "no agent running", "aucun agent en cours";
    RunningOne => "1 agent running", "1 agent en cours";
    RunningMany => "{0} agents running", "{0} agents en cours";
    RunningShort => "{0} running", "{0} en cours";
    TooSmall => "Enlarge the window (80 × 20 minimum)", "Agrandissez la fenêtre (80 × 20 minimum)";

    // ── Menus ─────────────────────────────────────────────────────────────
    Root => "zevbox", "zevbox";
    MenuAgents => "Running agents", "Agents en cours";
    MenuAgentsDesc => "Each agent Zed starts runs in its own Linux VM, which only sees the open project. Here: see the running agents, stop one or all of them (the thread open in Zed is then cut), and read a session log to understand a problem.",
        "Chaque agent que Zed démarre tourne dans sa propre VM Linux, qui ne voit que le projet ouvert. Ici : voir les agents en cours, en arrêter un ou tous (le thread ouvert dans Zed est alors coupé), et lire le journal d'une session pour comprendre un problème.";
    MenuUpdates => "Updates", "Mises à jour";
    MenuUpdatesDesc => "An update builds a new image per agent: latest Ubuntu, latest agent version, your added tools. It is tested before being activated, and the previous version is kept so you can go back to it. Nothing updates by itself: only when you ask for it here.",
        "Une mise à jour construit une image neuve par agent : dernière Ubuntu, dernière version de l'agent, vos outils ajoutés. Elle est testée avant d'être activée, et la version précédente est gardée pour y revenir. Rien ne se met à jour tout seul : seulement quand vous le demandez ici.";
    MenuAuth => "Agent sign-in", "Connexion des agents";
    MenuAuthDesc => "Each agent signs in once: Claude with your Claude subscription, Antigravity with your Google account, oMLX with your own model server. The tokens stay in zevbox's state folder: neither your ~/.claude nor the macOS keychain is used.",
        "Chaque agent se connecte une fois : Claude à votre abonnement Claude, Antigravity à votre compte Google, oMLX à votre propre serveur de modèles. Les jetons restent dans le dossier d'état de zevbox : ni votre ~/.claude ni le trousseau macOS ne sont utilisés.";
    MenuTools => "Linux tools", "Outils Linux";
    MenuToolsDesc => "The agents' VMs already have Node, Python, git, ripgrep, jq, curl and the build tools. If another Ubuntu package (apt) is needed, add it here: it is built into the images and present in every session. What the agent installs by itself is gone at the next session.",
        "Les VM des agents ont déjà Node, Python, git, ripgrep, jq, curl et les outils de compilation. S'il faut un autre paquet Ubuntu (apt), ajoutez-le ici : il est intégré aux images et présent à chaque session. Ce que l'agent installe lui-même disparaît à la session suivante.";
    MenuCare => "Diagnosis and cleanup", "Diagnostic et ménage";
    MenuCareDesc => "Check that everything is in order (the diagnosis changes nothing) and free some space: leftovers of stopped agents, old logs and, if you ask for it, old images.",
        "Vérifier que tout est en ordre (le diagnostic ne change rien) et libérer de la place : restes d'agents arrêtés, anciens journaux et, si vous le demandez, anciennes images.";
    MenuZed => "Zed", "Zed";
    MenuZedDesc => "zevbox adds its agents (\"zevbox · Claude\", …) to Zed's agent menu by writing to ~/.config/zed/settings.json, with a backup before each change. Here: see what would be written, install these entries or remove them.",
        "zevbox ajoute ses agents (« zevbox · Claude », …) au menu des agents de Zed en écrivant dans ~/.config/zed/settings.json, avec une sauvegarde avant chaque changement. Ici : voir ce qui serait écrit, installer ces entrées ou les retirer.";
    Contains => "Contains:", "Contient :";

    // ── Actions: labels ───────────────────────────────────────────────────
    StatusLabel => "Show running agents", "Voir les agents en cours";
    StopOneLabel => "Stop an agent", "Arrêter un agent";
    StopAllLabel => "Stop all agents", "Arrêter tous les agents";
    LogsLabel => "Read a session log", "Lire le journal d'une session";
    UpdateCheckLabel => "Check for updates", "Vérifier s'il y a des mises à jour";
    UpdateAllLabel => "Update everything", "Tout mettre à jour";
    UpdateAgentLabel => "Update one agent", "Mettre à jour un agent";
    RollbackLabel => "Go back to the previous version", "Revenir à la version précédente";
    AuthStatusLabel => "Sign-in status", "État des connexions";
    AuthClaudeLabel => "Sign in to Claude", "Se connecter à Claude";
    AuthAntigravityLabel => "Sign in to Antigravity", "Se connecter à Antigravity";
    AuthOmlxLabel => "Connect oMLX", "Connecter oMLX";
    ToolsListLabel => "Show added tools", "Voir les outils ajoutés";
    ToolsAddLabel => "Add a tool", "Ajouter un outil";
    ToolsRemoveLabel => "Remove a tool", "Retirer un outil";
    DoctorLabel => "Full diagnosis", "Diagnostic complet";
    CleanLabel => "Clean up", "Faire le ménage";
    CleanImagesLabel => "Clean up, old images too", "Ménage avec les anciennes images";
    ZedDryRunLabel => "Preview what would be written", "Voir ce qui serait écrit";
    ZedInstallLabel => "Install the zevbox entries", "Installer les entrées zevbox";
    ZedUninstallLabel => "Remove the zevbox entries", "Retirer les entrées zevbox";
    ShellLabel => "Shell in a VM", "Shell dans une VM";
    SetupLabel => "First-time setup", "Première installation";

    // ── Actions: explanations (what it does, what it changes, what it leaves) ─
    StatusDesc => "Lists the running agents: their id (8 characters), their state, the agent, the name of its VM and the open project. An \"orphan\" agent is a leftover that no longer runs: \"Clean up\" removes it. Changes nothing.",
        "Liste les agents en cours : leur identifiant (8 caractères), leur état, l'agent, le nom de sa VM et le projet ouvert. Un agent « orphan » est un reste qui ne tourne plus : « Faire le ménage » le supprime. Ne change rien.";
    StopOneDesc => "Stops the chosen agent: its VM is stopped then deleted. The thread open in Zed with this agent is cut. Your project is not touched: what the agent already wrote there stays. Open a new thread in Zed to start again with a fresh VM.",
        "Arrête l'agent choisi : sa VM est arrêtée puis supprimée. Le thread ouvert dans Zed avec cet agent est coupé. Votre projet n'est pas touché : ce que l'agent y a déjà écrit reste. Ouvrez un nouveau thread dans Zed pour repartir avec une VM neuve.";
    StopAllDesc => "Stops all the zevbox agents of this installation and deletes their VMs. Every thread open in Zed with a zevbox agent is cut; your projects are not touched. Handy at the end of the day to free memory and CPU. Your other containers are never stopped.",
        "Arrête tous les agents zevbox de cette installation et supprime leurs VM. Tous les threads ouverts dans Zed avec un agent zevbox sont coupés ; vos projets ne sont pas touchés. Utile en fin de journée pour libérer mémoire et processeur. Vos autres conteneurs ne sont jamais arrêtés.";
    LogsDesc => "Shows the last 200 lines of a session log: start, stop, methods refused by the filter, errors. The log never holds your messages or your tokens: only names, paths, ids and sizes. \"The most recent\" = the last session. Changes nothing.",
        "Affiche les 200 dernières lignes du journal d'une session : démarrage, arrêt, méthodes refusées par le filtre, erreurs. Le journal ne contient jamais vos messages ni vos jetons : seulement des noms, chemins, identifiants et tailles. « La plus récente » = la dernière session. Ne change rien.";
    UpdateCheckDesc => "Asks the ACP registry for the latest version of each agent and compares it with the installed one. Downloads nothing and changes nothing: run it now and then to know whether \"Update everything\" is worth it.",
        "Demande au registre ACP la dernière version de chaque agent et la compare à la version installée. Ne télécharge rien et ne change rien : à faire de temps en temps pour savoir s'il vaut la peine de « Tout mettre à jour ».";
    UpdateAllDesc => "Builds a new image for Claude, Antigravity and oMLX if installed: latest Ubuntu, latest agent version, your added tools. Each image is tested before being activated; the old one becomes the \"previous\" version (you can go back). Threads already open in Zed keep the old image until they are closed.",
        "Construit une image neuve pour Claude, Antigravity et oMLX s'il est installé : dernière Ubuntu, dernière version de l'agent, vos outils ajoutés. Chaque image est testée avant d'être activée ; l'ancienne devient la version « précédente » (retour possible). Les threads déjà ouverts dans Zed gardent l'ancienne image jusqu'à leur fermeture.";
    UpdateAgentDesc => "Like \"Update everything\", for one agent: new image (latest Ubuntu, latest agent version, your tools), tested then activated; the old one stays as the \"previous\" version. Also installs an agent that is not installed yet. Threads already open keep the old image.",
        "Comme « Tout mettre à jour », pour un seul agent : image neuve (dernière Ubuntu, dernière version de l'agent, vos outils), testée puis activée ; l'ancienne reste la version « précédente ». Installe aussi un agent qui ne l'est pas encore. Les threads déjà ouverts gardent l'ancienne image.";
    RollbackDesc => "Reactivates the agent's \"previous\" version, the one before the last update. Instant: nothing is downloaded or rebuilt. One step only: the two versions are swapped, so doing \"Go back\" again brings back the newer one. The next threads opened in Zed use the chosen version.",
        "Réactive la version « précédente » de l'agent, celle d'avant la dernière mise à jour. Instantané : rien n'est téléchargé ni reconstruit. Un seul cran : les deux versions sont échangées, donc refaire « Revenir » ramène la plus récente. Les prochains threads ouverts dans Zed utilisent la version choisie.";
    AuthStatusDesc => "Tells for Claude, Antigravity and oMLX: \"authenticated\" (connected), \"not authenticated\" (to connect) or \"unknown\" (agent not installed yet). Changes nothing.",
        "Indique pour Claude, Antigravity et oMLX : « authenticated » (connecté), « not authenticated » (à connecter) ou « unknown » (agent pas encore installé). Ne change rien.";
    AuthClaudeDesc => "The sign-in happens in a throw-away VM that sees none of your folders. An https://claude.com/… address is printed: open it in the Mac's browser (it does not open by itself), sign in with your Claude subscription, then paste the code shown. The token is stored in zevbox's state folder, never in your ~/.claude.",
        "La connexion se fait dans une VM jetable qui ne voit aucun de vos dossiers. Une adresse https://claude.com/… s'affiche : ouvrez-la dans le navigateur du Mac (il ne s'ouvre pas tout seul), connectez-vous avec votre abonnement Claude, puis collez le code affiché. Le jeton est rangé dans le dossier d'état de zevbox, jamais dans votre ~/.claude.";
    AuthAntigravityDesc => "A small server opens on your Mac (127.0.0.1 only) and the browser shows the Google sign-in: choose your Google AI Pro account and allow (5 minutes at most). The token is stored in zevbox's state folder; the macOS keychain is never touched.",
        "Un petit serveur s'ouvre sur votre Mac (127.0.0.1 seulement) et le navigateur affiche la connexion Google : choisissez votre compte Google AI Pro et autorisez (5 minutes au plus). Le jeton est rangé dans le dossier d'état de zevbox ; le trousseau macOS n'est jamais touché.";
    AuthOmlxDesc => "Links the oMLX agent to your model server (for example a Mac Studio): its address, its API key (hidden, never on the command line) and the default model. The Mac checks the address and the key and lists the models, then zevbox writes the settings and checks that a VM reaches the server. Your Claude subscription is not used.",
        "Relie l'agent oMLX à votre serveur de modèles (par exemple un Mac Studio) : son adresse, sa clé d'API (masquée, jamais sur la ligne de commande) et le modèle par défaut. Le Mac vérifie l'adresse et la clé et liste les modèles, puis zevbox écrit les réglages et vérifie qu'une VM atteint le serveur. Votre abonnement Claude n'est pas utilisé.";
    ToolsListDesc => "Lists the Ubuntu packages (apt) you added to the agents' images. The base tools (Node, Python, git, ripgrep, jq, curl, compilers) are not listed: they are always there. Changes nothing.",
        "Liste les paquets Ubuntu (apt) que vous avez ajoutés aux images des agents. Les outils de base (Node, Python, git, ripgrep, jq, curl, compilateurs) n'y figurent pas : ils sont toujours là. Ne change rien.";
    ToolsAddDesc => "Adds one or more Ubuntu packages (apt names, for example \"tree clang\"), then rebuilds the image of each installed agent, which also moves it to its latest version, like \"Update everything\". The packages are then there in every session. Threads already open keep the old image.",
        "Ajoute un ou plusieurs paquets Ubuntu (noms apt, par exemple « tree clang »), puis reconstruit l'image de chaque agent installé, ce qui le met aussi à sa dernière version, comme « Tout mettre à jour ». Les paquets sont ensuite là à chaque session. Les threads déjà ouverts gardent l'ancienne image.";
    ToolsRemoveDesc => "Removes a package from the list, then rebuilds the image of each installed agent, which also moves to its latest version. Threads already open in Zed keep the old image until they are closed.",
        "Retire un paquet de la liste, puis reconstruit l'image de chaque agent installé, qui passe aussi à sa dernière version. Les threads déjà ouverts dans Zed gardent l'ancienne image jusqu'à leur fermeture.";
    DoctorDesc => "Checks the whole installation: configuration, Apple Container (present and started), image and sign-in of each agent, space used by the images, folder permissions, installed binary, entries in Zed, orphan agents, methods refused by the filter in the last 7 days. Each problem comes with the command that fixes it. Changes nothing.",
        "Vérifie toute l'installation : configuration, Apple Container (présent et démarré), image et connexion de chaque agent, place prise par les images, droits des dossiers, binaire installé, entrées dans Zed, agents orphelins, méthodes refusées par le filtre ces 7 derniers jours. Chaque problème vient avec la commande qui le corrige. Ne change rien.";
    CleanDesc => "Removes leftovers: orphan agents and their VMs, zevbox VMs that are no longer registered; keeps the last 30 logs and the last 20 backups of Zed's settings. Running agents, images and your projects are not touched.",
        "Supprime les restes : agents orphelins et leurs VM, VM zevbox qui ne sont plus enregistrées ; garde les 30 derniers journaux et les 20 dernières sauvegardes des réglages de Zed. Les agents en cours, les images et vos projets ne sont pas touchés.";
    CleanImagesDesc => "Like \"Clean up\", and also deletes the images of versions older than the previous one, the image build VM (its disk grows over time) and the old release folders. The current and the previous version of each agent are kept: going back is still possible.",
        "Comme « Faire le ménage », et en plus supprime les images des versions plus anciennes que la précédente, la VM de construction des images (son disque grossit avec le temps) et les anciens dossiers de version. La version actuelle et la précédente de chaque agent sont gardées : le retour en arrière reste possible.";
    ZedDryRunDesc => "Shows the \"zevbox · …\" entries that would be written to Zed's settings, one per installed agent. Writes nothing.",
        "Montre les entrées « zevbox · … » qui seraient écrites dans les réglages de Zed, une par agent installé. N'écrit rien.";
    ZedInstallDesc => "Writes or updates the \"zevbox · …\" entries in ~/.config/zed/settings.json, one per installed agent. The rest of the file, comments included, is not touched, and a backup is made first. Useful if the zevbox agents are missing in Zed.",
        "Écrit ou met à jour les entrées « zevbox · … » dans ~/.config/zed/settings.json, une par agent installé. Le reste du fichier, commentaires compris, n'est pas touché, et une sauvegarde est faite avant. Utile si les agents zevbox manquent dans Zed.";
    ZedUninstallDesc => "Removes the \"zevbox · …\" entries from Zed's settings (a backup is made first): the zevbox agents disappear from Zed. Nothing else is deleted: images, sign-ins and zevbox settings stay; \"Install the zevbox entries\" puts them back.",
        "Retire les entrées « zevbox · … » des réglages de Zed (une sauvegarde est faite avant) : les agents zevbox disparaissent de Zed. Rien d'autre n'est supprimé : images, connexions et réglages zevbox restent ; « Installer les entrées zevbox » les remet.";
    ShellDesc => "Opens a Linux terminal in a throw-away VM identical to the agent's, on the chosen project: you see exactly what the agent sees. The project is mounted (your changes stay in it), .git is read-only and .zed hidden. Everything else is gone when you leave (exit). The project must be in your home folder or on an external disk.",
        "Ouvre un terminal Linux dans une VM jetable identique à celle de l'agent, sur le projet choisi : vous voyez exactement ce que voit l'agent. Le projet est monté (vos modifications y restent), .git est en lecture seule et .zed masqué. Tout le reste disparaît à la sortie (exit). Le projet doit être dans votre dossier personnel ou sur un disque externe.";
    SetupDesc => "Guided setup, to run once at the start (safe to run again): creates the state folder and the configuration, builds the Claude and Antigravity images, offers to sign the agents in, adds the entries in Zed, starts Apple Container if needed, then runs the diagnosis. oMLX, optional, is connected separately.",
        "Installation guidée, à faire une fois au début (sans risque si on la relance) : crée le dossier d'état et la configuration, construit les images de Claude et Antigravity, propose de connecter les agents, ajoute les entrées dans Zed, démarre Apple Container si besoin, puis lance le diagnostic. oMLX, optionnel, se connecte à part.";

    // ── Duration · network ────────────────────────────────────────────────
    CostStop => "Duration: a few seconds per agent", "Durée : quelques secondes par agent";
    CostCheck => "Duration: a few seconds · network needed", "Durée : quelques secondes · réseau nécessaire";
    CostUpdateAll => "Duration: 1 to 3 min per agent · network needed", "Durée : 1 à 3 min par agent · réseau nécessaire";
    CostUpdateAgent => "Duration: 1 to 3 min · network needed", "Durée : 1 à 3 min · réseau nécessaire";
    CostSignIn => "Interactive · network needed", "Interactif · réseau nécessaire";
    CostOmlx => "Duration: a few seconds · local network needed", "Durée : quelques secondes · réseau local nécessaire";
    CostTools => "Duration: 1 to 3 min per installed agent · network needed", "Durée : 1 à 3 min par agent installé · réseau nécessaire";
    CostSeconds => "Duration: a few seconds", "Durée : quelques secondes";
    CostShell => "Interactive · starts a VM (a few seconds)", "Interactif · démarre une VM (quelques secondes)";
    CostSetup => "Interactive · Duration: a few minutes · network needed", "Interactif · Durée : quelques minutes · réseau nécessaire";
    FirstSignInBuilds => "Not installed yet: the first sign-in builds its image first (a few minutes, network needed)",
        "Pas encore installé : la première connexion construit d'abord son image (quelques minutes, réseau nécessaire)";
    Connected => "Status: connected", "État : connecté";
    NotConnected => "Status: not connected", "État : non connecté";
    CommandLine => "Command: {0}", "Commande : {0}";
    ForEachAgent => "(for each agent)", "(pour chaque agent)";
    KeyOnStdin => "(key on standard input)", "(clé sur l'entrée standard)";
    PhAddress => "<address>", "<adresse>";
    PhModel => "<model>", "<modèle>";
    PhPackage => "<package>", "<paquet>";
    PhProject => "<project>", "<projet>";

    // ── Text fields ───────────────────────────────────────────────────────
    PromptPackages => "Tools to add (names separated by spaces)", "Outils à ajouter (noms séparés par des espaces)";
    PromptUrl => "oMLX server address (e.g. http://192.168.1.8:8000)", "Adresse du serveur oMLX (ex. http://192.168.1.8:8000)";
    PromptKey => "API key (hidden)", "Clé d'API (masquée)";
    PromptModel => "Default model (optional, Enter = the smallest)", "Modèle par défaut (optionnel, Entrée = le plus petit)";
    PromptProject => "Project folder", "Dossier du projet";
    HintKeepKey => "Enter with nothing typed: keep the current key.", "Entrée sans rien taper : garder la clé actuelle.";
    HintKeyHidden => "The key is never shown or logged.", "La clé n'est ni affichée ni journalisée.";
    HintModel => "Enter with nothing typed: the server's smallest model.", "Entrée sans rien taper : le plus petit modèle du serveur.";
    HintPackages => "Example: ripgrep jq", "Exemple : ripgrep jq";
    DashRefused => "A value cannot start with \"-\".", "Une valeur ne peut pas commencer par « - ».";

    // ── Pickers ───────────────────────────────────────────────────────────
    PickStop => "Choose the agent to stop", "Choisir l'agent à arrêter";
    NoAgentRunning => "No agent running.", "Aucun agent en cours.";
    Latest => "The most recent", "La plus récente";
    PickSession => "Choose the session", "Choisir la session";
    PickAgent => "Choose the agent", "Choisir l'agent";
    NoAgentInstalled => "No agent installed: run First-time setup.", "Aucun agent installé : lancez Première installation.";
    NoPrevious => "No agent has a previous version.", "Aucun agent n'a de version précédente.";
    PickTool => "Choose the tool to remove", "Choisir l'outil à retirer";
    NoTool => "No tool added.", "Aucun outil ajouté.";
    AgentNotInstalled => "{0} (not installed)", "{0} (non installé)";

    // ── « Are you sure? » ─────────────────────────────────────────────────
    AreYouSure => "Are you sure?", "Êtes-vous sûr ?";
    Yes => "[ Yes ]", "[ Oui ]";
    No => "[ No ]", "[ Non ]";
    ReauthQ => "{0} is already connected. Sign in again?", "{0} est déjà connecté. Refaire la connexion ?";
    ReauthC => "The current connection will be replaced.", "La connexion actuelle sera remplacée.";
    StopOneQ => "Stop agent {0}?", "Arrêter l'agent {0} ?";
    StopOneC => "The threads open in Zed with this agent will be cut.", "Les threads ouverts dans Zed avec cet agent seront coupés.";
    StopAllQ => "Stop all agents?", "Arrêter tous les agents ?";
    StopAllC => "The threads open in Zed will be cut.", "Les threads ouverts dans Zed seront coupés.";
    UpdateAllQ => "Update everything?", "Tout mettre à jour ?";
    UpdateAllC => "The images are rebuilt (network, several minutes); the new versions are used from the next launch from Zed.",
        "Les images sont reconstruites (réseau, plusieurs minutes) ; les nouvelles versions servent au prochain lancement depuis Zed.";
    UpdateAgentQ => "Update {0}?", "Mettre à jour {0} ?";
    UpdateAgentC => "Its image is rebuilt (network, a few minutes); the new version is used from the next launch from Zed.",
        "Son image est reconstruite (réseau, quelques minutes) ; la nouvelle version sert au prochain lancement depuis Zed.";
    RollbackQ => "Go back to the previous version of {0}?", "Revenir à la version précédente de {0} ?";
    RollbackC => "The next launch from Zed will use the previous version.", "Le prochain lancement depuis Zed utilisera la version précédente.";
    ToolsAddQ => "Add {0}?", "Ajouter {0} ?";
    ToolsRemoveQ => "Remove {0}?", "Retirer {0} ?";
    ToolsC => "The images are rebuilt and the installed agents updated (network, several minutes).",
        "Les images sont reconstruites et les agents installés mis à jour (réseau, plusieurs minutes).";
    CleanImagesQ => "Delete the old images?", "Supprimer les anciennes images ?";
    CleanImagesC => "Images older than the previous version and the build VM are deleted; the next update recreates that VM.",
        "Les images plus anciennes que la version précédente et la VM de construction sont effacées ; la prochaine mise à jour recrée cette VM.";
    ZedUninstallQ => "Remove the zevbox entries from Zed?", "Retirer les entrées zevbox de Zed ?";
    ZedUninstallC => "The zevbox agents disappear from Zed (a backup of the settings is kept).",
        "Les agents zevbox disparaissent de Zed (une sauvegarde des réglages est gardée).";
    InterruptQ => "Interrupt the command?", "Interrompre la commande ?";
    InterruptC => "It gets SIGTERM, then SIGKILL after 5 s if it is still running.", "Elle reçoit SIGTERM, puis SIGKILL après 5 s si elle tourne encore.";
    InterruptQuitC => "It gets SIGTERM (SIGKILL after 5 s), then the console closes.", "Elle reçoit SIGTERM (SIGKILL après 5 s), puis la console se ferme.";

    // ── Running a command ─────────────────────────────────────────────────
    Running => "running…", "en cours…";
    Interrupting => "interrupting…", "interruption…";
    ScrolledUp => "   (↑ {0} lines)", "   (↑ {0} lignes)";
    Done => "✓ done", "✓ terminé";
    FailedCode => "✗ failed (code {0})", "✗ échec (code {0})";
    FailedSignal => "✗ failed (stopped by a signal)", "✗ échec (arrêtée par un signal)";
    Interrupted => "✗ failed (interrupted)", "✗ échec (interrompue)";
    CannotStart => "✗ failed (could not start the command)", "✗ échec (impossible de lancer la commande)";
    PressEnter => "Press Enter to return to the console ", "Appuyez sur Entrée pour revenir à la console ";
    MoreBelow => " ↓ PgDn: more ", " ↓ PgDn : suite ";
    MoreAbove => " ↑ PgUp ", " ↑ PgUp ";
    MoreBoth => " ↑ PgUp · ↓ PgDn ", " ↑ PgUp · ↓ PgDn ";

    // ── Key line (where the keys go, then the keys) ───────────────────────
    KeysConfirm => "Question: ←→ Tab choose Yes/No · Enter confirm · Esc = No",
        "Question : ←→ Tab choisir Oui/Non · Entrée valider · Échap = Non";
    KeysMenuRoot => "Menu: ↑↓ choose · Enter open · PgUp/PgDn text · Esc/q quit",
        "Menu : ↑↓ choisir · Entrée ouvrir · PgUp/PgDn texte · Échap/q quitter";
    KeysMenu => "Menu: ↑↓ choose · Enter open · PgUp/PgDn text · Esc back · q quit",
        "Menu : ↑↓ choisir · Entrée ouvrir · PgUp/PgDn texte · Échap retour · q quitter";
    KeysText => "Input: type the text · Enter confirm · Esc back · Ctrl-C quit",
        "Saisie : tapez le texte · Entrée valider · Échap retour · Ctrl-C quitter";
    KeysPicker => "List on the right: ↑↓ choose · Enter confirm · Esc back · q quit",
        "Liste à droite : ↑↓ choisir · Entrée valider · Échap retour · q quitter";
    KeysRunning => "Output on the right: ↑↓ PgUp PgDn scroll · Esc interrupt",
        "Sortie à droite : ↑↓ PgUp PgDn défiler · Échap interrompre";
    KeysDone => "Output on the right: ↑↓ PgUp PgDn scroll · Esc back to the menu · q quit",
        "Sortie à droite : ↑↓ PgUp PgDn défiler · Échap revenir au menu · q quitter";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(text: &str) -> Vec<usize> {
        (0..4)
            .filter(|i| text.contains(&format!("{{{i}}}")))
            .collect()
    }

    #[test]
    fn every_string_exists_in_both_languages() {
        for msg in Msg::ALL {
            let (en, fr) = msg.pair();
            assert!(!en.trim().is_empty(), "{msg:?}: empty English");
            assert!(!fr.trim().is_empty(), "{msg:?}: empty French");
            assert_eq!(
                placeholders(en),
                placeholders(fr),
                "{msg:?}: placeholders differ"
            );
        }
        // Real translations, not copies (a few strings are the same on purpose).
        let same: Vec<_> = Msg::ALL
            .iter()
            .filter(|m| m.pair().0 == m.pair().1)
            .collect();
        assert!(same.len() <= 6, "{same:?}");
    }

    #[test]
    fn language_comes_from_the_locale() {
        let l = Lang::from_locale;
        assert_eq!(l(None, None, Some("fr_FR.UTF-8")), Lang::Fr);
        assert_eq!(l(None, None, Some("fr")), Lang::Fr);
        assert_eq!(l(None, None, Some("en_US.UTF-8")), Lang::En);
        assert_eq!(l(None, None, Some("C")), Lang::En);
        assert_eq!(l(None, None, Some("")), Lang::En);
        assert_eq!(l(None, None, None), Lang::En);
        // LC_ALL overrides LC_MESSAGES, which overrides LANG; empty ones are skipped.
        assert_eq!(l(Some("en_GB.UTF-8"), None, Some("fr_FR.UTF-8")), Lang::En);
        assert_eq!(
            l(Some("fr_CA.UTF-8"), Some("en_US"), Some("en_US")),
            Lang::Fr
        );
        assert_eq!(l(Some(""), Some("fr_BE"), Some("en_US")), Lang::Fr);
        assert_eq!(l(None, Some("C"), Some("fr_FR")), Lang::En);
    }

    #[test]
    fn fill_replaces_placeholders_in_order() {
        assert_eq!(Lang::En.f(Msg::FailedCode, &["3"]), "✗ failed (code 3)");
        assert_eq!(
            Lang::Fr.f(Msg::StopOneQ, &["1a2b3c4d"]),
            "Arrêter l'agent 1a2b3c4d ?"
        );
        assert_eq!(fill("{0} {1} {0}", &["a", "b"]), "a b a");
    }

    #[test]
    fn console_lang_option_is_parsed_by_the_cli() {
        use clap::Parser;
        let parse = |args: &[&str]| crate::cli::Cli::try_parse_from(args).map(|c| c.command);
        assert!(matches!(
            parse(&["zevbox", "console", "--lang", "fr"]),
            Ok(crate::cli::Command::Console {
                lang: Some(Lang::Fr)
            })
        ));
        assert!(matches!(
            parse(&["zevbox", "console", "--lang", "en"]),
            Ok(crate::cli::Command::Console {
                lang: Some(Lang::En)
            })
        ));
        assert!(matches!(
            parse(&["zevbox", "console"]),
            Ok(crate::cli::Command::Console { lang: None })
        ));
        let bad = parse(&["zevbox", "console", "--lang", "de"]);
        assert!(bad.is_err_and(|e| e.kind() == clap::error::ErrorKind::InvalidValue));
    }

    #[test]
    fn lang_values_on_the_command_line() {
        use clap::ValueEnum;
        assert_eq!(Lang::from_str("en", false), Ok(Lang::En));
        assert_eq!(Lang::from_str("fr", false), Ok(Lang::Fr));
        assert!(Lang::from_str("de", false).is_err());
    }
}
