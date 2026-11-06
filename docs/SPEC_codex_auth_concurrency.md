# Zevbox et les connexions Codex simultanées

Statut au 2026-11-06 : **implémentée et validée sans compte** sur
`fix/codex-auth-concurrency`. Tests et gates détaillés dans
[le plan d’exécution](PLAN_codex_auth_concurrency.md#8-résultats). C4 reste ouverte.

## 1. Problème et résultat attendu

Avant cette correction, dans `src/cli/auth.rs`, `auth_codex` supprimait puis recréait
`state/codex/.codex-login`, lançait le login natif, publiait éventuellement son cache,
puis supprimait ce dossier. Deux commandes sur le même état utilisaient donc le
même espace de travail. Le démarrage ou la fin de l’une pouvait supprimer les
données de l’autre. Le constat initial vient de la revue de `87db544` ; la course a ensuite été
reproduite par deux processus du vrai binaire et des caches synthétiques, sans
compte réel (PROGRESS, 2026-11-06).

Deux commandes `zevbox auth codex` doivent pouvoir attendre leur validation
navigateur simultanément. Chacune doit lire, publier et nettoyer uniquement le
cache de sa propre tentative. Un échec ou une annulation ne publie rien et ne
perturbe pas l’autre connexion.

## 2. Solution retenue pour l’exécution

Chaque tentative crée atomiquement un répertoire privé unique avec `tempfile`,
déjà dépendance du projet, sous `state/codex`, avec le préfixe
`.codex-login-`. Son nom est attribué par la création du répertoire temporaire,
sans séquence « tester l’existence puis créer ».

| Élément | Comportement cible |
|---|---|
| Dossier de tentative | `state/codex/.codex-login-<suffixe>`, mode `0700`. |
| HOME de login dans la VM | `CODEX_HOME=/home/agent/.codex-login-<suffixe>`. |
| Cache candidat | `auth.json` dans ce dossier précis. |
| Cache actif | `state/codex/.codex/auth.json`, chemin inchangé. |
| Publication | Seulement après code de sortie 0 et validation native complète ; renommage atomique vers le cache actif. |
| Succès simultanés | La dernière publication atomique réussie gagne, indépendamment de l’ordre de démarrage ou de validation navigateur. |
| Échec ou annulation | Aucun remplacement du cache actif par cette tentative. Une autre tentative peut toutefois réussir et changer le compte. |
| Nettoyage normal | Uniquement le dossier possédé par la tentative, après la fin du processus enfant. |

Il n’y a pas de verrou global ni de file d’attente. Un verrou autour du dossier
fixe empêcherait les connexions simultanées et imposerait de traiter sa durée
de vie avec les interruptions. Des dossiers distincts suppriment directement
le partage fautif ; seul le fichier actif reste commun, avec une règle de
publication explicite. Le renommage doit rester sur le même système de
fichiers : créer le candidat sous `state/codex`, pas dans le `/tmp` système.

Un succès signifie que cette tentative a publié son cache ; il ne réserve pas
le compte contre une reconnexion ultérieure. Aucun mélange de jetons entre
deux candidats n’est permis. Le rafraîchissement concurrent par des sessions
Codex déjà ouvertes reste la limite existante de SPEC Codex §5 ; cette
évolution ne garantit pas non plus le comportement distant du fournisseur
lors de plusieurs connexions au même compte.

## 3. Compatibilité des releases et périmètre

Les releases existantes enregistrent dans `authCommand` l’affectation fixe
`CODEX_HOME=/home/agent/.codex-login`. Elles doivent fonctionner avec le nouveau
binaire **sans rebuild ni modification de leur lock**.

Garder la commande persistée actuelle comme modèle. Lors du login seulement,
cloner son vecteur et remplacer l’affectation attendue par le chemin de la
tentative. Conserver l’adaptateur et tous les autres arguments du lock. Le
remplacement est structurel : exécutable `env`, affectation attendue en deuxième
élément, une seule affectation `CODEX_HOME=`. Pas de remplacement textuel global,
de concaténation shell ou d’héritage de l’environnement du Mac. Une forme
incompatible est refusée avant tout lancement avec :

```text
zevbox: unsupported Codex sign-in command in this release. Fix: run 'zevbox update --agent codex' and retry 'zevbox auth codex'.
```

Code de sortie : 1. Ne pas inscrire le vecteur complet dans l’erreur.
Le modèle persisté, le schéma du lock et `codex_login_command` restent compatibles.
Documenter la distinction entre ce modèle et la commande réellement exécutée.

L’ancien dossier fixe `.codex-login`, s’il existe, est laissé intact : il peut
appartenir à un ancien processus encore actif. Aucun balayage des dossiers
`.codex-login-*` au début d’une connexion, même s’ils semblent anciens. La
coexistence avec un ancien binaire ne corrige pas les courses entre deux
processus de cet ancien binaire.

I1–I13 restent applicables : deux montages (projet vide et état Codex), aucun
accès au `~/.codex` du Mac, aucun secret dans argv ou les journaux, aucune
modification du relais, des autres agents ou des réglages Zed. Le nom du dossier
dans `CODEX_HOME` n’est pas un secret. Aucune nouvelle dépendance nécessaire.

## 4. Erreurs et interruptions

Le propriétaire du répertoire temporaire reste vivant pendant l’attente du
processus, la validation et la publication. Une sortie normale, un échec de
démarrage ou un retour d’erreur tente de libérer son seul répertoire. Le cache
candidat doit être un fichier régulier, pas un lien ; son contenu passe le
validateur natif existant. Avant publication, assurer `0600` sur le cache et
`0700` sur `.codex`. Une erreur de validation, de droits ou de renommage ne doit
pas supprimer le cache actif pour « préparer » le remplacement.

Si le cache a été publié mais que le nettoyage échoue, ne pas prétendre que la
connexion a échoué ou que l’ancien compte est inchangé : succès de connexion,
code 0 et avertissement stderr sur le résidu. Avant publication, conserver le
code d’échec et le diagnostic principal ; une erreur de nettoyage est secondaire.
Messages de nettoyage sans contenu de cache ; indiquer le dossier concerné et
que sa suppression manuelle doit attendre la fin des processus de cette tentative.
Le diagnostic d’échec du login doit dire que **cette tentative** n’a pas remplacé
la connexion, sans nier une publication concurrente.

`SIGKILL` et les terminaisons sans déroulement de pile peuvent laisser un dossier
privé ; la destruction automatique Rust ne peut pas garantir son nettoyage.
Un enfant qui survivrait provisoirement reste lié à son propre chemin. La
connexion suivante crée un autre dossier et n’adopte jamais ce cache résiduel.
La collecte automatique de ces résidus et une refonte du cycle de vie interactif
sont hors périmètre. Leur absence ne doit pas être présentée comme un nettoyage
garanti après toute interruption.

## 5. Acceptation

Les scénarios détaillés figurent dans le plan. Le résultat est accepté lorsque
deux processus réels du binaire, pilotés par un double de container sans compte,
prouvent l’indépendance des candidats, les deux ordres de publication, l’échec
et l’interruption de l’un pendant l’attente de l’autre. Les anciennes releases
doivent fonctionner, les empreintes protégées rester conformes, et les gates
séquentielles passer. Tous les documents concernés sont actualisés dans le
même commit que le comportement.

Une vérification facultative sur le vrai CLI peut vérifier l’affichage du code
dans deux VM jetables puis annuler, sans utiliser ni capturer ce code. Elle ne
doit pas ouvrir de connexion personnelle ni consommer de quota sans mission
explicite. L’acceptation de cette correction ne vaut pas acceptation globale C4.
