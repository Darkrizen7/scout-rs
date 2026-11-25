# scout-rs (Rust)

Outil de bureau compagnon de l'addon [Scout](../scout-addon) (voir son `CLAUDE.md`) pour **WoW Forever** : prix de
l'HdV dans le temps, tous les objets du jeu, toutes les recettes, butin par zone. Rust édition 2024, egui / eframe 0.37
(backend `glow`), SQLite via sqlx 0.9, HTTP via ureq 3. Dépôt privé `github.com/Darkrizen7/scout-rs`.

## Documentation détaillée (à lire seulement si la tâche la concerne)
- `docs/modules.md` : ce que fait chaque onglet (Market, Objets, Butin, Recettes), fichiers de `app/src/ui/`, pistes.
- `docs/donnees.md` : base SQLite (tables, colonnes), réglages `config.toml`, évolution du schéma.
- `docs/sources.md` : imports Scout / Auctionator / Forever Guide / PNJ / butin, OpenMap, wago.tools, Wowhead.
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
  la sortie des commandes courte (voir ci-dessous), ne pas lancer de sous-agents sans demande.

## Commandes (sortie silencieuse : n'afficher que les erreurs)
- Vérifier : `cargo clippy -q --workspace --all-targets --message-format short` : **zéro avertissement** avant
  chaque commit. Puis `cargo fmt --all`.
- Tests : `cargo test -q --workspace 2>&1 | grep -E 'FAILED|panicked|^error' || echo OK` ; si un test échoue,
  relancer ce test seul sans filtre pour voir le détail.
- Lancer : `cargo run --release` (binaire `target/release/scout-rs`) ; test de plantage : `target/debug`.
- Vraies données : `cargo test --workspace -- --ignored --nocapture` (import de l'installation WoW dans une base
  jetable, résumé de l'analyse sur la base locale ; téléchargement complet des données du jeu :
  `cargo test -p scout-worker --test local downloads -- --ignored --nocapture`).
- Base de référence intégrée (données du jeu, butin, marchands ; GPL) : `cargo run -p scout-worker --example reference`
  la régénère depuis `prix.db` (après une mise à jour des données du jeu, de Forever Guide, ou de `GAME_FORMAT` /
  `DROPS_FORMAT` : le test `embedded_reference_is_up_to_date` le rappelle).
- Les scripts Lua tournent dans Lua 5.4 intégré (`mlua`, feature `vendored`) : rien à installer.

## Données (règles ; détail dans `docs/donnees.md`)
- Base `~/.local/share/scout-rs/prix.db`, réglages `~/.config/scout-rs/config.toml`.
- **Ne jamais supprimer de données sans demander ; copier `prix.db` avant toute manipulation.**
- Formats CSV des exports Scout = **contrat avec l'addon** : les changer des deux côtés.
- `GAME_FORMAT` (`scout-core/src/gamedata.rs`) : l'augmenter quand on garde de nouvelles colonnes ou tables du jeu ;
  `DROPS_FORMAT` : l'augmenter quand `drops_export.lua` change.
- La base de référence (`scout-worker/data/reference.db.gz`) n'a **jamais** de données personnelles : ni prix,
  ni favoris, ni butin enregistré (liste des tables dans `scout-db/src/reference.rs`).
- Une nouvelle source de données se cite dans `ui/credits.rs` (`SOURCES`).

## Architecture
Workspace Cargo (`resolver = "3"`). Les dépendances ne vont que dans un sens :
`app → scout-worker → {scout-db, scout-sources} → scout-core`. Le compilateur fait respecter la règle « l'UI ne
touche ni disque ni base ni réseau » : `app` n'a ni `sqlx` ni `ureq` dans ses dépendances directes.

| Crate | Rôle (détail : `docs/architecture.md`) |
|---|---|
| `scout-core` | Données et logique **pures**, testées : analyse, statistiques, prix, recherche, cartes, butin, types partagés. |
| `scout-sources` | Tout ce qui vient de l'extérieur : scripts Lua (`lua/`, `include_str!`), wago.tools, Wowhead, TGA d'OpenMap, dossier WoW. |
| `scout-db` | SQLite (sqlx) : schéma, migrations, requêtes par domaine. |
| `scout-worker` | Threads de fond (`Command` → `Event`) : **seul** endroit qui touche disque, base, réseau et processus. |
| `app` (binaire `scout-rs`) | UI egui uniquement (`app.rs`, `state.rs`, `ui/`). |

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
- `style.interaction.selectable_labels = false` : sinon le texte des cellules capte les clics des lignes.
- Axes egui_plot : texte seulement (`1g 20s`), sauf l'axe des prix de `ui/chart`, dessiné à la main avec des pièces
  (`chart/money_axis.rs`) ; bulles au survol et pièces ailleurs : dessinées.
- `Panel::show_inside` est déprécié dans egui 0.37 : `Panel::show(ui, …)` sert aussi à l'intérieur d'un `Ui`.
- OpenMap : images de Blizzard, lues dans le dossier AddOns, jamais copiées dans le dépôt.

## Commits
- Sujet en français, phrase courte avec majuscule, corps qui explique le pourquoi. Branche `master`.
  Commit et push **seulement à la demande**.
- Quand un changement modifie ce que décrit `docs/`, mettre à jour le fichier concerné.

## Versions et releases
- Une version = un tag `vX.Y.Z` posé **par l'utilisateur** (jamais de tag sans demande). Le push du tag lance
  `.github/workflows/release.yml` : tests, compilation, Release GitHub avec les binaires Linux et Windows et leurs `.sha256`.
- Avant chaque version, **demander à l'utilisateur** s'il faut régénérer la base de référence
  (`cargo run -p scout-worker --example reference`, après un import Scout récent avec Forever Guide à jour et les
  données du jeu à jour) et la commiter avec le reste : c'est elle qui donne aux autres ses nouveaux marchands et le butin.
- Avant le tag : augmenter `version` dans `[workspace.package]` du `Cargo.toml` racine (commit à part,
  `Cargo.lock` compris). Le tag doit être identique à cette version (`v` + version), sinon l'Action échoue :
  la mise à jour automatique (`worker/update`, bouton « Mettre à jour » en haut) compare `CARGO_PKG_VERSION` à
  la dernière Release et télécharge le binaire dont le nom est dans `scout_core::update::BINARY` (= l'Action).
- Addon : sa propre Action publie `Scout.zip` (+ `.sha256`) à chaque tag de `scout-addon` ; l'outil l'installe ou
  le met à jour (`ADDON_ARCHIVE`), jamais par-dessus un lien symbolique ou un clone git (installation de dev).
- Poser la version : `git tag -a vX.Y.Z -m "Version X.Y.Z" && git push origin vX.Y.Z`.
