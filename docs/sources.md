# Sources

Les scripts Lua tournent dans le Lua 5.4 intégré au binaire (`mlua`, `scout-sources/src/import.rs` +
`lua/sandbox.lua` : `arg` rempli, `io.write`/`io.stdout` collectés en mémoire, `io.stderr` ignoré, `loadfile`
lu par Rust pour les chemins Unicode sous Windows). Ci-dessous, `lua x.lua a b` = script `x.lua` lancé avec `a b`.

- **Scout** : addon détecté par `Interface/AddOns/Scout/Scout.toc` (sinon bouton d'import rouge dans Réglages,
  vérifié au démarrage, à l'ouverture des Réglages et quand le dossier WoW change). `lua <wow_dir>/Interface/AddOns/Scout/tools/export.lua <Scout.lua>` (historique) et `… --names`
  (noms + catégories). Format CSV = **contrat avec l'addon** : le changer des deux côtés (`scout-sources/src/import.rs`).
  Rappeler `/reload` en jeu avant d'importer (le jeu n'écrit le fichier qu'à ce moment).
- **Auctionator / Forever Guide** (import ponctuel) : `scout-sources/lua/legacy_export.lua`, intégré (`include_str!`), lancé avec
  le dossier SavedVariables. Auctionator stocke son historique en **CBOR** (décodeur Lua inclus) : par jour (depuis le 01/01/2020
  local), `l`/`h` = prix min le plus bas / haut, `a` = quantité ; relevé daté de midi. Forever Guide ne garde que
  le dernier prix vu par objet.
- **PNJ** : `scout-sources/lua/vendor_export.lua` (intégré), à chaque import Scout : `ScoutDB.merchants` (PNJ, zone, position,
  stock limité ; prioritaire), `ScoutDB.vendor` (anciens relevés sans PNJ), et `VendorPrices.lua` de Forever Guide
  (composants vendus sans limite). `BuyPrice` du client existe pour **tous** les objets : ne jamais s'en servir pour
  dire « vendu par un PNJ ». Revente au marchand = `SellPrice`.
- **Butin** :
  - Forever Guide : `scout-sources/lua/drops_export.lua` (intégré), lancé avec `<AddOns/ForeverGuide> sources | npcs |
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
  `AreaTable` (faction des zones : `FactionGroupMask` 2 Alliance, 4 Horde, 0 contestée ; zones de premier niveau
  seulement, appariées par nom sans le suffixe « (nom anglais) ») + `UiMapAssignment` (rectangle monde `Region_0/1` min `x,y`, `Region_3/4` max, gardé seulement s'il couvre toute la
  carte : UiMin 0,0 / UiMax 1,1). Monde : `x` vers le nord, `y` vers l'ouest (`scout-core/src/map.rs`).
  **`GAME_FORMAT`** (`scout-core/src/gamedata.rs`) est stocké avec la version : l'augmenter quand on garde de nouvelles colonnes ou
  tables, pour que « Mettre à jour » retélécharge même si la version de Forever n'a pas changé.
- **Wowhead** (`scout-sources/src/wowhead.rs`, `https://www.wowhead.com/classic/fr/item=<id>&xml`) : ~550 objets de
  recettes (surtout repris de Season of Discovery) sont envoyés par le serveur de Forever en hotfix : ils sont
  dans `Item` mais pas dans `ItemSparse`, donc sans nom chez wago.tools (et absents de `DBCache.bin` tant
  qu'on ne les a pas croisés). À la mise à jour, `Tables::missing_recipe_items` → nom, qualité, niveaux, revente
  depuis Wowhead ; la catégorie reste celle de `Item` (`Tables::unnamed_categories`). 1 s de pause entre deux
  requêtes (sans pause, CloudFront bloque l'adresse en 403 après ~300) ; au premier refus (403…) on
  arrête de demander, le reste des données est enregistré mais marqué incomplet (`complete = 0` dans `game_meta`, donc « ⚠ Incomplètes » dans Réglages et la mise à jour suivante recommence)
  et un message rouge dit de réessayer plus tard (le cache garde ce qui a été lu).
- Les sources sont citées dans Réglages → bouton « ℹ Sources des données » (`ui/credits.rs`, `SOURCES` : à compléter
  si on en ajoute une).
