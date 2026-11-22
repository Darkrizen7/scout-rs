# scout-rs (Rust)

Outil de bureau compagnon de l'addon [Scout](../Scout) (voir son `CLAUDE.md`) pour **WoW Forever** : prix de
l'HdV dans le temps, tous les objets du jeu, toutes les recettes. Rust édition 2024, egui / eframe 0.37
(backend `glow`), SQLite via sqlx 0.9, HTTP via ureq 3. Dépôt privé `github.com/Darkrizen7/scout-prix` (à renommer en `scout-rs`).

## Façon de travailler
- Claude écrit le code ; l'utilisateur relit et teste. Réponses en français, tutoiement.
- Exigences : Rust idiomatique, sobriété CPU/RAM (pas de rendu continu), séparation des responsabilités,
  fichiers courts (< ~300 lignes hors tests ; découper au-delà, ex. `impl Items` réparti dans `items/{rows,sidebar}.rs`).
- Claude ne voit pas l'interface : il compile, teste, lance le binaire quelques secondes pour détecter un plantage,
  et dit que le rendu reste à juger par l'utilisateur.
- Si Claude se trompe, il le dit franchement. Rien de bannissable : l'outil ne touche jamais au jeu (il lit des
  fichiers sur disque et télécharge des données publiques) ; voir aussi le `CLAUDE.md` de Scout.

## Commandes
- `cargo run --release` (binaire `target/release/scout-rs`).
- `cargo clippy --workspace --all-targets` : **zéro avertissement** avant chaque commit. `cargo fmt --all`.
- `cargo test --workspace` (tests unitaires) ; sur les vraies données : `cargo test --workspace -- --ignored --nocapture`
  (import de l'installation WoW dans une base jetable, résumé de l'analyse sur la base locale, téléchargement
  complet des données du jeu : `cargo test -p scout-worker --test local downloads -- --ignored --nocapture`).
- Prérequis : `lua` 5.4 dans le `PATH` (les exports passent par des scripts Lua).

## Modules (onglets en haut)
- **Market** : historique HdV. Barre latérale façon Fichiers (tous les objets, favoris, Analyse, Rentabilité,
  catégories dépliables), liste, graphique prix / quantité, analyse, calculateur de rentabilité.
- **Objets** : les ~19 400 objets du jeu par catégorie, avec prix HdV / achat PNJ / revente ; une **fiche objet**
  à onglets : Market (prix et graphique), Recettes (produit par, utilisé dans, enseigne), PNJ (où l'acheter),
  Butin (à faire).
- **Recettes** : recherche d'une recette ou d'un objet, par métier ; fiche recette (composants au moins cher entre
  PNJ et HdV, bénéfice, patron). Un objet cliqué ouvre sa fiche dans Objets (`State::show_item`), une recette
  cliquée depuis une fiche objet s'ouvre dans Recettes (`State::show_recipe`).

## Données
- Base `~/.local/share/scout-rs/prix.db`, réglages `~/.config/scout-rs/config.toml` (`wow_dir` = dossier
  `_forever_`, détecté dans les préfixes Wine usuels ; `theme` = `light` / `dark`). Chemins via `directories`.
- Tables :
  - Market : `items (item_id, name, class_id, subclass_id, class_name, subclass_name)` (noms et catégories vus par
    Scout), `snapshots (item_id, source, ts, price, quantity)` clé `(item_id, source, ts)`, `source` ∈
    `scout | auctionator | foreverguide`, prix en cuivre (NULL = pas en vente), `ts` en secondes Unix.
    Imports **idempotents** (`INSERT OR IGNORE`) : la base garde tout, même ce que Scout oublie après 21 jours.
  - `favorites (item_id)` : partagés par Market et Objets.
  - `vendor_prices (item_id, source, npc_id, price, npc, zone, x, y, limited)` : remplacée à chaque import Scout.
  - Données du jeu `game_*` (remplacées en bloc) : `game_meta` (`build`, `updated`, `format`), `game_items`,
    `game_categories`, `game_professions`, `game_recipes`, `game_reagents`, `game_recipe_items`.
- Évolution du schéma : `CREATE TABLE IF NOT EXISTS` + colonnes ajoutées si absentes (`ITEM_COLUMNS` dans `scout-db/src/lib.rs`
  et `game.rs`). Ne jamais supprimer de données sans demander ; copier `prix.db` avant toute manipulation.
- Les numéros d'objet sont ceux du jeu, identiques dans Scout, Auctionator, Forever Guide et les tables du client.

## Sources
- **Scout** : `lua <wow_dir>/Interface/AddOns/Scout/tools/export.lua <Scout.lua>` (historique) et `… --names`
  (noms + catégories). Format CSV = **contrat avec l'addon** : le changer des deux côtés (`scout-sources/src/import.rs`).
  Rappeler `/reload` en jeu avant d'importer (le jeu n'écrit le fichier qu'à ce moment).
- **Auctionator / Forever Guide** (import ponctuel) : `scout-sources/lua/legacy_export.lua`, intégré (`include_str!`), passé à
  `lua -`. Auctionator stocke son historique en **CBOR** (décodeur Lua inclus) : par jour (depuis le 01/01/2020
  local), `l`/`h` = prix min le plus bas / haut, `a` = quantité ; relevé daté de midi. Forever Guide ne garde que
  le dernier prix vu par objet.
- **PNJ** : `scout-sources/lua/vendor_export.lua` (intégré), à chaque import Scout : `ScoutDB.merchants` (PNJ, zone, position,
  stock limité ; prioritaire), `ScoutDB.vendor` (anciens relevés sans PNJ), et `VendorPrices.lua` de Forever Guide
  (composants vendus sans limite). `BuyPrice` du client existe pour **tous** les objets : ne jamais s'en servir pour
  dire « vendu par un PNJ ». Revente au marchand = `SellPrice`.
- **Données du jeu** : tables du client Forever (DB2) en CSV, français, depuis
  `https://wago.tools/db2/<Table>/csv?build=<build>&locale=frFR` ; dernière version = produit `wowf` de
  `https://wago.tools/api/builds`, vérifiée au démarrage (Réglages affiche « À jour » ou « Nouvelle version »).
  11 tables (`gamedata::TABLES`) ; assemblage comme le dépôt public `alcaras/forever-ref` : métiers = lignes 171,
  164, 333, 202, 182, 165, 186, 393, 197, 185, 129, 356, 40 (+ lignes enfants via `ParentSkillLineID`, ex. 2948 →
  197) ; objet produit = effet 24 (`EffectItemType`, quantité `EffectBasePointsF`) ; composants `Reagent_0..7` ;
  objets de recette = `ItemEffect.TriggerType` 6 (niveau requis = leur `RequiredSkillRank`) ; catégories =
  `ItemClass` / `ItemSubClass` (nom détaillé `VerboseName_lang` sinon `DisplayName_lang`). ~2 500 recettes
  (Classic ≈ 1 300 + ajouts Forever), cohérent avec Forever Guide ; sont écartés à raison : rangs de métier,
  récolte, spécialisations, invocations, sorts sans nom.
  **`GAME_FORMAT`** (`scout-core/src/gamedata.rs`) est stocké avec la version : l'augmenter quand on garde de nouvelles colonnes ou
  tables, pour que « Mettre à jour » retélécharge même si la version de Forever n'a pas changé.

## Architecture
Workspace Cargo (`resolver = "3"`). Les dépendances ne vont que dans un sens :
`app → scout-worker → {scout-db, scout-sources} → scout-core`. Le compilateur fait respecter la règle « l'UI ne
touche ni disque ni base ni réseau » : `app` n'a ni `sqlx` ni `ureq` dans ses dépendances directes.

| Crate | Rôle |
|---|---|
| `scout-core` | Données et logique **pures**, testées (ni disque, ni réseau, ni egui ; `sqlx` seulement avec la feature `sqlx`, pour `#[derive(FromRow)]`). `analysis` + `analysis/stats` : prix du jour = médiane des prix min du jour (date locale) ; week-end vs semaine par **test de permutation** (exact ou Monte-Carlo à graine fixe), effet de marché avec **les jours comme unité** ; prix du jour anormal par **z-score robuste** (MAD, plancher 3 %), seuils en constantes publiques. `gamedata` (types, index `made_by` / `used_in` / `teaches`, `GAME_FORMAT`), `money` (cuivre ↔ texte, rentabilité, commission HdV 5 %), `pricing` (`cheapest` : marchand ou HdV), `time` (dates locales), types partagés (`Row`, `ItemName`, `VendorPrice`, `ItemSummary`, `Snapshot`, `VendorData` / `VendorOffer`, `Theme`). |
| `scout-sources` | Tout ce qui vient de l'extérieur : `import` (scripts Lua de `lua/`, intégrés par `include_str!`, et lecture de leur CSV), `wago` (téléchargement wago.tools) + `wago/assemble` (CSV → recettes, testé), `wow` (dossier `_forever_` : détection Wine, `export_script`, `saved_variables`, liste de Forever Guide). |
| `scout-db` | SQLite (sqlx) : `lib.rs` (`Db`, schéma, migrations), `market` (objets, favoris, relevés), `game` (données du jeu), `vendor` (prix et offres des marchands). |
| `scout-worker` | Thread de fond, runtime tokio `current_thread` : `Command` → `Event` via `mpsc` + callback `notify` (l'app y met `request_repaint`). **Seul** endroit qui touche disque, base, réseau et processus. `config` : réglages, chemins, migration des dossiers `scout-prix` → `scout-rs`. Tests sur la vraie machine dans `tests/local.rs` (`#[ignore]`). |
| `app` (paquet et binaire `scout-rs`) | UI egui uniquement. `main.rs`, `app.rs` (onglets de modules, barre d'état, module courant), `state.rs` (`State` lu par l'UI : données du worker, module / vue / filtre / sélection ; `revision` change à chaque modification des objets ou favoris, pour les caches ; `auction_price`, `buy_price`). |

`app/src/ui/` : `header` (onglets, en-tête Market, barre d'état, fenêtre Réglages), `sidebar` (barre latérale Market,
icônes de catégories), `item_list`, `chart` (`plots` réutilisé par la fiche objet), `analysis_view` (+ `charts`),
`calculator`, `items` (+ `items/{rows,sidebar,sheet}`), `recipes` (+ `recipes/detail`), `widgets` (étoile de favori,
couleurs de qualité, prix ou tiret : partagés), `theme` (palettes libadwaita), `fonts` (Inter), `coins` (pièces dessinées).

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
- Axes et bulles des graphiques (egui_plot) : texte seulement (`1g 20s`) ; pièces dessinées partout ailleurs.
- `Panel::show_inside` est déprécié dans egui 0.37 : `Panel::show(ui, …)` sert aussi à l'intérieur d'un `Ui`.

## Pistes
- Onglet **Butin** de la fiche objet : monstres qui lâchent l'objet et chance. Données possibles : la base de butin
  de Scout (`Scout/Farm/Loot.lua`, générée depuis cmangos classic-db par `Scout/tools/build_loot.py`) ou QuestieDB
  (données Forever, pas les ajouts Forever).

## Commits
- Sujet en français, phrase courte avec majuscule, corps qui explique le pourquoi. Branche `master`.
  Commit et push **seulement à la demande**.
