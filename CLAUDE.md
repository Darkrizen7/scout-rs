# scout-prix (Rust)

Outil de bureau compagnon de l'addon [Scout](../Scout) (voir son `CLAUDE.md`) pour **WoW Forever** : prix de
l'HdV dans le temps, tous les objets du jeu, toutes les recettes. Rust édition 2024, egui / eframe 0.37
(backend `glow`), SQLite via sqlx 0.9, HTTP via ureq 3. Dépôt privé `github.com/Darkrizen7/scout-prix`.

## Façon de travailler
- Claude écrit le code ; l'utilisateur relit et teste. Réponses en français, tutoiement.
- Exigences : Rust idiomatique, sobriété CPU/RAM (pas de rendu continu), séparation des responsabilités,
  fichiers courts (< ~300 lignes hors tests ; découper au-delà, ex. `impl Items` réparti dans `items/sidebar.rs`).
- Claude ne voit pas l'interface : il compile, teste, lance le binaire quelques secondes pour détecter un plantage,
  et dit que le rendu reste à juger par l'utilisateur.
- Si Claude se trompe, il le dit franchement. Rien de bannissable : l'outil ne touche jamais au jeu (il lit des
  fichiers sur disque et télécharge des données publiques) ; voir aussi le `CLAUDE.md` de Scout.

## Commandes
- `cargo run --release` (binaire `target/release/scout-prix`).
- `cargo clippy --all-targets` : **zéro avertissement** avant chaque commit. `cargo fmt`.
- `cargo test` (tests unitaires) ; sur les vraies données : `cargo test -- --ignored --nocapture`
  (import de l'installation WoW dans une base jetable, résumé de l'analyse sur la base locale, téléchargement
  complet des données du jeu : `cargo test gamedata::online -- --ignored --nocapture`).
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
- Base `~/.local/share/scout-prix/prix.db`, réglages `~/.config/scout-prix/config.toml` (`wow_dir` = dossier
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
- Évolution du schéma : `CREATE TABLE IF NOT EXISTS` + colonnes ajoutées si absentes (`ITEM_COLUMNS` dans `db.rs`
  et `db/game.rs`). Ne jamais supprimer de données sans demander ; copier `prix.db` avant toute manipulation.
- Les numéros d'objet sont ceux du jeu, identiques dans Scout, Auctionator, Forever Guide et les tables du client.

## Sources
- **Scout** : `lua <wow_dir>/Interface/AddOns/Scout/tools/export.lua <Scout.lua>` (historique) et `… --names`
  (noms + catégories). Format CSV = **contrat avec l'addon** : le changer des deux côtés (`src/import.rs`).
  Rappeler `/reload` en jeu avant d'importer (le jeu n'écrit le fichier qu'à ce moment).
- **Auctionator / Forever Guide** (import ponctuel) : `tools/legacy_export.lua`, intégré (`include_str!`), passé à
  `lua -`. Auctionator stocke son historique en **CBOR** (décodeur Lua inclus) : par jour (depuis le 01/01/2020
  local), `l`/`h` = prix min le plus bas / haut, `a` = quantité ; relevé daté de midi. Forever Guide ne garde que
  le dernier prix vu par objet.
- **PNJ** : `tools/vendor_export.lua` (intégré), à chaque import Scout : `ScoutDB.merchants` (PNJ, zone, position,
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
  **`GAME_FORMAT`** (`db/game.rs`) est stocké avec la version : l'augmenter quand on garde de nouvelles colonnes ou
  tables, pour que « Mettre à jour » retélécharge même si la version de Forever n'a pas changé.

## Architecture
| Module | Rôle |
|---|---|
| `main.rs`, `app.rs` | Lancement eframe ; barre d'onglets de modules, barre d'état, puis le module courant. |
| `state.rs` | `State` lu par l'UI : données du worker (objets Market, historique, analyse, favoris, `game`, `vendor`, dernière version), module / vue / filtre / sélection, navigation entre modules. `revision` change à chaque modification des objets ou favoris (caches de l'UI). |
| `worker.rs` | Thread de fond, runtime tokio `current_thread` : `Command` → `Event` via `mpsc` + `request_repaint`. **Seul** endroit qui touche disque, base, réseau et processus. |
| `config.rs`, `import.rs` | Réglages et chemins ; exécution des scripts Lua et lecture de leur CSV. |
| `db.rs` + `db/{game,vendor}.rs` | SQLite : Market, favoris ; données du jeu ; prix et offres des marchands (`VendorData`). |
| `gamedata.rs` + `gamedata/{download,assemble}.rs` | Types et index (`made_by`, `used_in`, `teaches`) ; téléchargement wago.tools ; assemblage testé. |
| `analysis.rs` + `analysis/stats.rs` | Analyse pure, testée : prix du jour = médiane des prix min du jour (date locale) ; week-end vs semaine par **test de permutation** (exact ou Monte-Carlo à graine fixe), effet de marché avec **les jours comme unité** ; prix du jour anormal par **z-score robuste** (MAD, plancher 3 %). Seuils en constantes publiques, affichés. |
| `money.rs` | Cuivre ↔ texte (`1g 20s 5c`, nombre nu = or) et rentabilité (commission HdV 5 %). |
| `ui/` | `header` (onglets, en-tête Market, barre d'état, fenêtre Réglages : thème, imports, données du jeu, dossier), `sidebar` (barre latérale Market + `entry`, icônes de catégories), `item_list`, `chart` (`plots` réutilisé par la fiche objet), `analysis_view`, `calculator`, `items` (+ `items/{sidebar,sheet}`), `recipes` (+ `recipes/detail`, qui contient aussi les petits outils de prix partagés), `theme` (palettes libadwaita), `fonts` (Inter), `coins` (pièces dessinées). |

- Aucune opération longue sur le thread UI ; l'UI ne lit jamais le disque.
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
  `winit` dans `Cargo.toml`, non utilisée dans le code) donne des boutons fermer / réduire au style GNOME.
- `style.interaction.selectable_labels = false` : sinon le texte des cellules capte les clics des lignes.
- Axes et bulles des graphiques (egui_plot) : texte seulement (`1g 20s`) ; pièces dessinées partout ailleurs.
- `Panel::show_inside` est déprécié dans egui 0.37 : `Panel::show(ui, …)` sert aussi à l'intérieur d'un `Ui`.

## Pistes
- Onglet **Butin** de la fiche objet : monstres qui lâchent l'objet et chance. Données possibles : la base de butin
  de Scout (`Scout/Farm/Loot.lua`, générée depuis cmangos classic-db par `Scout/tools/build_loot.py`) ou QuestieDB
  (données Forever, pas les ajouts Forever).
- Fichiers à découper quand on y touche : `db.rs`, `ui/analysis_view.rs`, `ui/items.rs` (un peu > 300 lignes).

## Commits
- Sujet en français, phrase courte avec majuscule, corps qui explique le pourquoi. Branche `master`.
  Commit et push **seulement à la demande**.
