# scout-rs (Rust)

Outil de bureau compagnon de l'addon [Scout](../scout-addon) (voir son `CLAUDE.md`) pour **WoW Forever** : prix de
l'HdV dans le temps, tous les objets du jeu, toutes les recettes. Rust édition 2024, egui / eframe 0.37
(backend `glow`), SQLite via sqlx 0.9, HTTP via ureq 3. Dépôt privé `github.com/Darkrizen7/scout-rs`.

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
- **Objets** : les ~19 400 objets du jeu par catégorie (icône de catégorie aussi dans le tableau), avec prix HdV /
  achat PNJ / revente. À l'arrivée, seule la recherche, au milieu de la page (`widgets::landing_search`) ; le
  tableau s'affiche dès qu'on tape ou qu'on choisit une catégorie. Une **fiche objet**
  à onglets : Market (prix et graphique), Recettes (produit par, utilisé dans, enseigne), PNJ (où l'acheter),
  Butin (monstres, dépeçage, vol à la tire, coffres, conteneurs, quêtes : chance de Forever Guide et chance
  réelle enregistrée par Scout, avec intervalle à 95 %).
- **Butin** : par zone de la carte. **Carte cliquable** des deux continents (zones à leur place, contour d'OpenMap,
  couleur selon le niveau habituel des monstres ; affichée tant qu'aucune zone n'est choisie, bouton « 🗺 Carte »).
  Barre latérale des zones puis des donjons et raids (recherche). **Ctrl+clic** (liste ou carte) ajoute une zone :
  les monstres de toutes les zones choisies sont listés ensemble (`ZoneLoot::merge`). Liste des monstres :
  recherche par nom **ou par objet lâché**, masquer élites / rares / boss, tri par niveau, valeur estimée par kill
  ou **valeur ÷ niveau**. Butin du monstre choisi (butin, dépeçage, vol à la tire : chance Forever Guide, chance
  réelle, prix). Zone d'un monstre = son donjon, sinon sa zone Forever Guide ; un monstre que seul Scout connaît va
  dans la zone où il a été fouillé. L'île de Zéphras (Forever) n'est ni dans Forever Guide ni sur un continent :
  rien sur la carte, son butin ne viendra que de Scout.
- **Positions cliquables** (fiche objet : Butin et PNJ ; module Butin) : copient `/way #<UiMap> x y Nom` (syntaxe
  TomTom ; l'addon Scout fournit `/way` quand TomTom est absent) à coller dans le chat du jeu. L'outil ne fait que
  remplir le presse-papiers : c'est le joueur qui colle. Forever Guide ne donne qu'un point par monstre (pas toutes
  ses apparitions).
- **Recettes** : recherche d'une recette ou d'un objet, par métier (même page d'arrivée que Objets, recherche puis
  résultats avec l'icône du métier ou de la catégorie) ; fiche recette (composants au moins cher entre
  PNJ et HdV, bénéfice, patron). Un objet cliqué ouvre sa fiche dans Objets (`State::show_item`), une recette
  cliquée depuis une fiche objet s'ouvre dans Recettes (`State::show_recipe`).

## Données
- Base `~/.local/share/scout-rs/prix.db`, réglages `~/.config/scout-rs/config.toml` (`wow_dir` = dossier
  `_forever_`, détecté dans les préfixes Wine usuels ; `theme` = `light` / `dark` ; `hide_obsolete`,
  vrai par défaut : masque les objets inutilisables : « Deprecated / Unused / [INUTILISÉ] / périmé / [PH] / OLD /
  Monster - / DNT », `gamedata::is_obsolete` ;
  `last_import` = date du dernier import Scout réussi). Chemins via `directories`.
- Tables :
  - Market : `items (item_id, name, class_id, subclass_id, class_name, subclass_name)` (noms et catégories vus par
    Scout), `snapshots (item_id, source, ts, price, quantity)` clé `(item_id, source, ts)`, `source` ∈
    `scout | auctionator | foreverguide`, prix en cuivre (NULL = pas en vente), `ts` en secondes Unix.
    Imports **idempotents** (`INSERT OR IGNORE`) : la base garde tout, même ce que Scout oublie après 21 jours.
  - `favorites (item_id)` : partagés par Market et Objets.
  - `vendor_prices (item_id, source, npc_id, price, npc, zone, x, y, limited)` : remplacée à chaque import Scout.
  - Butin de Forever Guide (remplacé en bloc quand ses fichiers changent) : `drop_sources (item_id, kind,
    source_id, chance, quest_only, more)` (`source_id` 0 = « et `more` autres non listées »), `drop_npcs`,
    `drop_names` (objets du monde, quêtes), `drop_meta` (`fingerprint`). `drop_npcs.ui_map` = carte (UiMap) de la
    zone du PNJ : lien avec la carte du jeu (les noms de zone de Forever Guide et du client diffèrent parfois).
  - Butin réel de Scout (remplacé à chaque import : l'addon garde des totaux) : `loot_npcs (npc_id, name, zone,
    kills, skins)`, `loot_observed (npc_id, kind, item_id, drops, quantity)`, `kind` ∈ `loot | skin`.
  - Données du jeu `game_*` (remplacées en bloc) : `game_meta` (`build`, `updated`, `format`), `game_items`,
    `game_categories`, `game_professions`, `game_recipes`, `game_reagents`, `game_recipe_items`, `game_maps`
    (continents et zones : UiMap, rectangle en coordonnées monde).
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
- **Butin** :
  - Forever Guide : `scout-sources/lua/drops_export.lua` (intégré), `lua - <AddOns/ForeverGuide> sources | npcs |
    names`. Lit `ItemSearchData.lua` (`SI_ITEM` : sources par objet, ids en base 36 sauf les quêtes `q=`, `c` =
    au choix, `pct` suivi de `q` = seulement quête en cours, `+N` = sources non listées ; `SI_NPC`, `SI_OBJ`,
    `SI_QUEST`), `DungeonLoot.lua` (boss, chances plus récentes : elles remplacent celles de `d=`), `DataFrFR.lua`
    (noms FR) et `Locale.lua` (zones / donjons FR, via un faux `GetLocale` → `frFR`). Réimporté à l'import Scout
    seulement si l'empreinte (taille + date des 4 fichiers + `DROPS_FORMAT`, à augmenter quand le script change)
    a changé : ~3,8 s, ~314 000 sources.
  - Scout : `export.lua <Scout.lua> --loot` (contrat avec l'addon : `npc_id,name,zone,kills,skins,kind,item_id,drops,quantity`).
  - Une erreur de butin n'interrompt pas l'import des prix.
- **Contours des zones** : addon **OpenMap** (`Media/Textures/Forever/MapTerritory/<UiMap>.tga`, 256×256, forme dans
  l'alpha, lignes de bas en haut), lus par `scout-sources/src/tga.rs` quand la carte s'ouvre. Ce sont des
  images de Blizzard : on les lit dans le dossier AddOns, on ne les copie jamais dans le dépôt. Sans OpenMap,
  zones en rectangles.
- **Données du jeu** : tables du client Forever (DB2) en CSV, français, depuis
  `https://wago.tools/db2/<Table>/csv?build=<build>&locale=frFR` ; dernière version = produit `wowf` de
  `https://wago.tools/api/builds`, vérifiée au démarrage (Réglages affiche « À jour » ou « Nouvelle version »).
  13 tables (`TABLES` dans `scout-sources/src/wago.rs`) ; assemblage comme le dépôt public `alcaras/forever-ref` : métiers = lignes 171,
  164, 333, 202, 182, 165, 186, 393, 197, 185, 129, 356, 40 (+ lignes enfants via `ParentSkillLineID`, ex. 2948 →
  197) ; objet produit = effet 24 (`EffectItemType`, quantité `EffectBasePointsF`) ; composants `Reagent_0..7` ;
  objets de recette = `ItemEffect.TriggerType` 6 (niveau requis = leur `RequiredSkillRank`) ; catégories =
  `ItemClass` / `ItemSubClass` (nom détaillé `VerboseName_lang` sinon `DisplayName_lang`). ~2 500 recettes
  (Classic ≈ 1 300 + ajouts Forever), cohérent avec Forever Guide ; sont écartés à raison : rangs de métier,
  récolte, spécialisations, invocations, sorts sans nom. Cartes : `UiMap` (nom, parent, type) +
  `UiMapAssignment` (rectangle monde `Region_0/1` min `x,y`, `Region_3/4` max, gardé seulement s'il couvre toute la
  carte : UiMin 0,0 / UiMax 1,1). Monde : `x` vers le nord, `y` vers l'ouest (`scout-core/src/map.rs`).
  **`GAME_FORMAT`** (`scout-core/src/gamedata.rs`) est stocké avec la version : l'augmenter quand on garde de nouvelles colonnes ou
  tables, pour que « Mettre à jour » retélécharge même si la version de Forever n'a pas changé.

## Architecture
Workspace Cargo (`resolver = "3"`). Les dépendances ne vont que dans un sens :
`app → scout-worker → {scout-db, scout-sources} → scout-core`. Le compilateur fait respecter la règle « l'UI ne
touche ni disque ni base ni réseau » : `app` n'a ni `sqlx` ni `ureq` dans ses dépendances directes.

| Crate | Rôle |
|---|---|
| `scout-core` | Données et logique **pures**, testées (ni disque, ni réseau, ni egui ; `sqlx` seulement avec la feature `sqlx`, pour `#[derive(FromRow)]`). `analysis` + `analysis/stats` : prix du jour = médiane des prix min du jour (date locale) ; week-end vs semaine par **test de permutation** (exact ou Monte-Carlo à graine fixe), effet de marché avec **les jours comme unité** ; prix du jour anormal par **z-score robuste** (MAD, plancher 3 %), seuils en constantes publiques. `gamedata` (types, index `made_by` / `used_in` / `teaches`, `GAME_FORMAT`), `money` (cuivre ↔ texte, rentabilité, commission HdV 5 %), `pricing` (`cheapest` : marchand ou HdV ; `sale_value` : ce que rapporte un objet ramassé, `AUCTION_CUT`), `time` (dates locales), `map` (cartes, placement d'une zone sur son continent, masques), `drops` (+ `drops/records`, `drops/zone` : butin par zone,
niveaux habituels = 10ᵉ / 90ᵉ centiles, valeur par kill) : fusion base Forever Guide / réel Scout, intervalle de Wilson, tri par colonne (`drops/sort` : valeurs inconnues toujours en dernier), `TRUSTED_KILLS` = 20 corps (chance réelle en vert), `format_percent` ; types partagés (`Row`, `ItemName`, `VendorPrice`, `ItemSummary`, `Snapshot`, `VendorData` / `VendorOffer`, `Theme`). |
| `scout-sources` | Tout ce qui vient de l'extérieur : `import` (scripts Lua de `lua/`, intégrés par `include_str!`, et lecture de leur CSV), `wago` (téléchargement wago.tools) + `wago/assemble` (+ `assemble/maps`) (CSV → recettes et cartes, testé), `tga` (masques d'OpenMap), `wow` (dossier `_forever_` : détection Wine, `export_script`, `saved_variables`, liste de Forever Guide). |
| `scout-db` | SQLite (sqlx) : `lib.rs` (`Db`, schéma, migrations), `market` (objets, favoris, relevés), `game` (données du jeu), `vendor` (prix et offres des marchands), `drops` (butin : base et réel ; `drops/zones` : par zone), `game/maps`. |
| `scout-worker` | Thread de fond, runtime tokio `current_thread` : `Command` → `Event` via `mpsc` + callback `notify` (l'app y met `request_repaint`). **Seul** endroit qui touche disque, base, réseau et processus. `config` : réglages, chemins, migration des dossiers `scout-prix` → `scout-rs`. Tests sur la vraie machine dans `tests/local.rs` (`#[ignore]`). |
| `app` (paquet et binaire `scout-rs`) | UI egui uniquement. `main.rs`, `app.rs` (onglets de modules, barre d'état, module courant), `state.rs` (`State` lu par l'UI : données du worker, module / vue / filtre / sélection ; `revision` change à chaque modification des objets ou favoris, pour les caches ; `auction_price`, `buy_price`). |

`app/src/ui/` : `header` (onglets, en-tête Market, barre d'état, fenêtre Réglages), `sidebar` (barre latérale Market,
icônes de catégories), `item_list`, `chart` (`plots` réutilisé par la fiche objet), `analysis_view` (+ `charts`),
`calculator`, `items` (+ `items/{list,rows,sidebar,sheet,merchants,drops}`), `loot` (+ `loot/{map,map/paint,npcs,npc}`), `recipes` (+ `recipes/{results,detail}`), `widgets` (étoile de favori,
couleurs de qualité, champ de recherche et page d'arrivée, prix ou tiret, chance et chance réelle : partagés), `theme` (palettes libadwaita), `fonts` (Inter), `coins` (pièces dessinées).

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
- Butin : `ForeverGuide/ForeverBossLoot.lua` (butin confirmé de boss Forever, chances inconnues, appliqué par
  l'addon à l'exécution) n'est pas lu.
- Carte : points des monstres de la zone (coordonnées Forever Guide `x`, `y` sur `ui_map`), entrées des donjons
  (`OpenMap/Data/Forever/Places.lua`, coordonnées monde), fond peint (`MapZones` d'OpenMap, BLP).
- Toutes les apparitions d'un monstre : QuestieDB (`QuestieDB_Forever`) les a, mais dans un format codé
  (`src/meta/codec.lua`) pas encore lu.

## Commits
- Sujet en français, phrase courte avec majuscule, corps qui explique le pourquoi. Branche `master`.
  Commit et push **seulement à la demande**.
