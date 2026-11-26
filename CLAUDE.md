# scout-rs (Rust)

Outil de bureau compagnon de l'addon [Scout](../scout-addon) (voir son `CLAUDE.md`) pour **WoW Forever** : prix de
l'HdV dans le temps, tous les objets du jeu, toutes les recettes, butin par zone. Rust édition 2024, egui / eframe 0.37
(backend `glow`), SQLite via sqlx 0.9, HTTP via ureq 3. Dépôt privé `github.com/Darkrizen7/scout-rs`.

## Documentation détaillée (à lire seulement si la tâche la concerne)
- `docs/modules.md` : ce que fait chaque onglet (Market, Objets, Butin, Recettes), fichiers de `app/src/ui/`, pistes.
- `docs/donnees.md` : base SQLite (tables, colonnes), réglages `config.toml`, évolution du schéma.
- `docs/sources.md` : imports Scout / PNJ / butin, OpenMap, données du jeu (wago.tools, Wowhead, via le serveur).
- `docs/architecture.md` : rôle détaillé de chaque crate et de ses modules.

## Façon de travailler
- Claude écrit le code ; l'utilisateur relit et teste. Réponses en français, tutoiement.
- Exigences : Rust idiomatique, sobriété CPU/RAM (pas de rendu continu), séparation des responsabilités,
  fichiers courts (< ~300 lignes hors tests ; découper au-delà, ex. `impl Items` réparti dans `items/{rows,sidebar}.rs`).
- Claude ne voit pas l'interface : il compile, teste, lance le binaire quelques secondes pour détecter un plantage,
  et dit que le rendu reste à juger par l'utilisateur.
- Si Claude se trompe, il le dit franchement. Rien de bannissable : l'outil ne touche jamais au jeu (il lit des
  fichiers sur disque et télécharge des données publiques) ; voir aussi le `CLAUDE.md` de Scout.
- Économie de tokens : lire seulement les fichiers / passages utiles (`grep -n` puis lecture ciblée), garder
  la sortie des commandes courte (voir ci-dessous), ne pas lancer de sous-agents sans demande
  (sauf via `/pman`, voir « Tickets (pman) »).

## Tickets (pman)

<!-- pman init: début (bloc géré par `pman init`) -->
- Workspace pman : `wow` (`PMAN_WORKSPACE`, posé par `.claude/settings.json`) ; serveur http://127.0.0.1:8090.
- Projets : `APP`, `SERVER`, `ADDON`, `OUTILS`.
- Lien d'un ticket : http://127.0.0.1:8090/wow/projects/APP/issues/APP-42
- Workflow : skill `/pman` (`.claude/skills/pman/`) et agent `dev` (`.claude/agents/dev.md`), installés par `pman init` ; CLI : `pman --help`.
<!-- pman init: fin -->

- Source de vérité des tâches : **pman** (http://127.0.0.1:8090, dépôt `~/dev/outils/pman`, `docker compose`),
  lu et écrit avec le CLI global `pman` (README de `crates/pman-cli` ; `--json` pour les sorties à analyser).
- **Workspace `wow`** : `PMAN_WORKSPACE=wow` est posé par `.claude/settings.json` (`env`) ; s'il manque (session
  ouverte ailleurs), préfixer chaque commande : `PMAN_WORKSPACE=wow pman …`. Projets : `APP` (crate `app`),
  `SERVER` (`scout-server`), `ADDON` (dépôt `../scout-addon`), `OUTILS` (skills, CLAUDE.md, outillage du dépôt).
  Clés `APP-42`, attribuées par `task create`. Lien d'un ticket : `http://127.0.0.1:8090/wow/projects/APP/issues/APP-42`.
  OpenProject et Plane ne sont plus tenus.
- Comptes : Claude agit **uniquement** avec le bot `claude` (jeton dans le keyring, jamais affiché) ; l'utilisateur
  est `admin` (`human` dans `~/.config/pman/config.toml`). Label **Feature** (nouveauté, évolution) ou **Fix**
  (correction), posé à la création ; label **Validation** = vérification confiée à l'utilisateur.
- États : Backlog → Todo → In progress → In review → Done (ou Cancelled) ; noms CLI `backlog`, `todo`,
  `in_progress`, `in_review`, `done`, `cancelled`. Un ticket en Todo est prenable ; l'utilisateur choisit toujours
  lequel traiter (argument de `/pman`, sinon liste proposée).
- Prenable (calculé par `pman agent next`) : Todo, pas assigné à quelqu'un d'autre, pas de label Validation,
  bloquants fermés. Réserver (`agent claim`, assigne à `claude`) ≠ démarrer (`agent start`, In progress).
- Une branche par ticket depuis `master`, nommée avec la clé (`APP-42-description-courte`, `agent branch`), et la
  clé en tête du sujet des commits (`APP-42 : …`). Un ticket à la fois (pas de parallèle).
- À la fin, sans attendre de demande : commit(s) sur la branche, `git push -u origin <branche>`, PR vers `master`
  (`gh pr create`, titre `APP-42 : …`), relecture tracée en commentaire de PR, fusion par Claude (voir « Commits »),
  synthèse + relevé de tokens (`agent report --usage-since`, remplit `AI Tokens`, `AI Usage 5h %`, `AI Usage 7d %`),
  puis « Done » (`agent complete`) ou, s'il faut un test humain (UI…), `agent request-validation` : ticket
  Validation précis assigné à l'utilisateur, ticket en « In review » jusqu'à sa réponse, **et message explicite dans
  la conversation** (pourquoi, quoi, comment, résultat attendu, clé et lien).
- Travail découvert en route : nouveau ticket (avec relation si pertinent), pas d'élargissement du périmètre.
- Blocage ou ambiguïté : commenter le ticket avec la question précise, `agent release`, ticket suivant.
- `/pman` (`.claude/skills/pman/`) : choix, délégation du code à l'agent `dev`
  (`.claude/agents/dev.md`, Haiku ou Sonnet), relecture du diff par Claude. Seuls Claude et l'utilisateur
  touchent à pman, jamais le sous-agent. La skill est générique (installée par `pman init`, ne pas l'éditer) ;
  ce qui est propre à scout-rs est ici :
  - Relecture : relancer clippy et les tests, plus le test de plantage `target/debug` si l'UI a changé.
    Haiku décroche vite sur egui : préférer Sonnet pour l'UI.
  - PR : corps = résumé, comment tester, `pman : APP-31 (<lien du ticket>)` ; relecture tracée en `gh pr comment`
    (GitHub refuse l'approbation de sa propre PR : ce commentaire en tient lieu). Fusion par Claude (« Commits »),
    message `Fusion de la PR #N : <titre>`.
  - L'agent `dev` est générique lui aussi (installé par `pman init`) : il lit ses vérifications et ses interdits
    dans ce fichier (« Commandes », « Données », « Architecture »).
- Statuts fixes ; labels et champs `AI …` par projet (`pman project show APP`, `project setup-agent APP`).
  Administration (utilisateurs, jetons) : `docker compose exec pman pman-server admin …` dans `~/dev/outils/pman`.

## Commandes (sortie silencieuse : n'afficher que les erreurs)
- Vérifier : `cargo clippy -q --workspace --all-targets --message-format short` : **zéro avertissement** avant
  chaque commit. Puis `cargo fmt --all`.
- Tests : `cargo test -q --workspace 2>&1 | grep -E 'FAILED|panicked|^error' || echo OK` ; si un test échoue,
  relancer ce test seul sans filtre pour voir le détail.
- Lancer : `cargo run --release` (binaire `target/release/scout-rs`) ; test de plantage : `target/debug`.
- Vraies données : `cargo test --workspace -- --ignored --nocapture` (import de l'installation WoW dans une base
  jetable, résumé de l'analyse sur la base locale ; téléchargement complet des données du jeu :
  `cargo test -p scout-worker --test local downloads -- --ignored --nocapture`).
- Paquet de butin et marchands du serveur (GPL ; plus rien n'est intégré au binaire, MIT) :
  `cargo run -p scout-worker --example server_reference` le régénère depuis `prix.db` dans `scout-server/data/`
  (à commiter puis à déployer avec le serveur).
- Les scripts Lua tournent dans Lua 5.4 intégré (`mlua`, feature `vendored`) : rien à installer.

## Données (règles ; détail dans `docs/donnees.md`)
- Base `~/.local/share/scout-rs/prix.db`, réglages `~/.config/scout-rs/config.toml`.
- **Ne jamais supprimer de données sans demander ; copier `prix.db` avant toute manipulation.**
- Formats CSV des exports Scout = **contrat avec l'addon** : les changer des deux côtés.
- `GAME_FORMAT` (`scout-core/src/gamedata.rs`) : l'augmenter quand on garde de nouvelles colonnes ou tables du jeu.
  Le paquet de butin et marchands du serveur a son propre format, `REFERENCE_FORMAT` (`scout-core/src/reference.rs`).
- Le paquet de référence du serveur (`scout-server/data/reference.db.gz`) n'a **jamais** de données personnelles :
  ni prix, ni favoris, ni butin enregistré (seulement `drop_*`, `vendor_prices` et les noms et catégories d'objets de `items`, `scout-db/src/reference.rs`).
  Le binaire n'embarque aucune donnée : premier lancement = tout vient du serveur.
- Une nouvelle source de données se cite dans `scout-core/src/credits.rs` (`SOURCES`, affichée par la page `/thanks` du serveur, ouverte depuis les réglages).

## Architecture
Workspace Cargo (`resolver = "3"`). Les dépendances ne vont que dans un sens :
`app → scout-worker → {scout-db, scout-sources} → scout-core`, et `{scout-sources, scout-server} → scout-gamedata → scout-core`. Le compilateur fait respecter la règle « l'UI ne
touche ni disque ni base ni réseau » : `app` n'a ni `sqlx` ni `ureq` dans ses dépendances directes.

| Crate | Rôle (détail : `docs/architecture.md`) |
|---|---|
| `scout-core` | Données et logique **pures**, testées : analyse, statistiques, prix, recherche, cartes, butin, types partagés. |
| `scout-gamedata` | Données du jeu : wago.tools, Wowhead, assemblage, paquet versionné (construit par le serveur, lu par l'appli). |
| `scout-sources` | Tout ce qui vient de l'extérieur : scripts Lua (`lua/`, `include_str!`), serveur de prix, GitHub, TGA d'OpenMap, dossier WoW. |
| `scout-db` | SQLite (sqlx) : schéma, migrations, requêtes par domaine. |
| `scout-worker` | Threads de fond (`Command` → `Event`) : **seul** endroit qui touche disque, base, réseau et processus. |
| `app` (binaire `scout-rs`) | UI egui uniquement (`app.rs`, `state.rs`, `ui/`). |
| `scout-server` (binaire) | Serveur du VPS (prix de l'API Battle.net, paquet des données du jeu), avec `scout-core` et `scout-gamedata` seulement : `docs/serveur.md`. |

- Aucune opération longue sur le thread UI ; l'UI ne lit jamais le disque.
- Une règle métier utilisée par l'UI va dans `scout-core` ; un widget utilisé par deux panneaux va dans `ui/widgets.rs`.
- Pas d'abstraction avant d'avoir deux usages (pas de trait `Repository`, etc.).
- Convention de modules `foo.rs` + `foo/` (lint `mod_module_files`).
- Commentaires du code en **anglais** ; textes affichés en **français**.

## Pièges connus
- sqlx 0.9 refuse le SQL construit à l'exécution (`SqlSafeStr`) : requêtes en chaînes statiques, valeurs en `bind`.
  Pas de macros `query!` (pas de `DATABASE_URL` à la compilation).
- Polices : Inter (OFL, `assets/fonts/`) en premier, polices d'egui en secours. Avant d'utiliser un symbole,
  vérifier qu'une police le contient (fontTools sur `epaint_default_fonts` + Inter) : sinon carré. egui ne dessine
  pas les emoji en couleur : les icônes sont teintées une à une (`sidebar::tint`). Feature egui
  `monochrome_emoji_fonts` requise.
- GNOME/Wayland ne dessine pas les barres de titre : la feature winit `wayland-csd-adwaita` (dépendance directe
  `winit` dans `app/Cargo.toml`, non utilisée dans le code) donne des boutons fermer / réduire au style GNOME.
- Liens (`hyperlink`, `ctx.open_url`) : feature eframe `links` requise (navigateur via `webbrowser`) ; sans elle,
  rien ne s'ouvre et aucune erreur ne s'affiche.
- `style.interaction.selectable_labels = false` : sinon le texte des cellules capte les clics des lignes.
- Axes egui_plot : texte seulement (`1g 20s`), sauf l'axe des prix de `ui/chart`, dessiné à la main avec des pièces
  (`chart/money_axis.rs`) ; bulles au survol et pièces ailleurs : dessinées.
- `Panel::show_inside` est déprécié dans egui 0.37 : `Panel::show(ui, …)` sert aussi à l'intérieur d'un `Ui`.
- OpenMap : images de Blizzard, lues dans le dossier AddOns, jamais copiées dans le dépôt.

## Commits
- Sujet en français, phrase courte avec majuscule, corps qui explique le pourquoi. Branche `master`,
  ou la branche du ticket (voir « Tickets (pman) »).
  Hors tickets, commit et push **seulement à la demande** ; sur une branche de ticket, Claude commite,
  pousse, ouvre la PR et la fusionne lui-même : fusion par Claude : `git merge --no-ff` de la branche dans `master` en local (commit de fusion daté hors
  plage, `GIT_AUTHOR_DATE` et `GIT_COMMITTER_DATE`), clippy et tests sur le résultat, puis `git push origin master` (le seul
  push sur `master` ; vérifier avant, avec `gh pr view`, qu'elle n'est pas déjà fusionnée). Une fusion par GitHub
  daterait le commit de maintenant. Après le push, supprimer la branche du ticket
  (`git push origin --delete <branche>`, `git branch -d <branche>`). Jamais de `--force`, jamais de commit direct sur `master`.
  Dates des commits : jamais en semaine 9h–12h ni 14h–18h (heure de Paris) ; sinon `--date` et
  `GIT_COMMITTER_DATE` juste après la plage.
- Quand un changement modifie ce que décrit `docs/`, mettre à jour le fichier concerné.

## Versions et releases
- Une version = un tag `vX.Y.Z` posé **par l'utilisateur** (jamais de tag sans demande). Le push du tag lance
  `.github/workflows/release.yml` : tests, compilation, Release GitHub avec les binaires Linux et Windows et leurs `.sha256`.
- Avant chaque version, **demander à l'utilisateur** s'il y a de nouveaux marchands ou du butin à publier : alors
  régénérer le paquet du serveur (`cargo run -p scout-worker --example server_reference`, après un import Scout
  récent), le commiter et déployer le serveur. Le binaire n'en dépend pas.
- Avant le tag : augmenter `version` dans `[workspace.package]` du `Cargo.toml` racine (commit à part,
  `Cargo.lock` compris). Le tag doit être identique à cette version (`v` + version), sinon l'Action échoue :
  la mise à jour automatique (`worker/update`, bouton « Mettre à jour » en haut) compare `CARGO_PKG_VERSION` à
  la dernière Release et télécharge le binaire dont le nom est dans `scout_core::update::BINARY` (= l'Action).
- Addon : sa propre Action publie `Scout.zip` (+ `.sha256`) à chaque tag de `scout-addon` ; l'outil l'installe ou
  le met à jour (`ADDON_ARCHIVE`), jamais par-dessus un lien symbolique ou un clone git (installation de dev).
- Poser la version : `git tag -a vX.Y.Z -m "Version X.Y.Z" && git push origin vX.Y.Z`.
