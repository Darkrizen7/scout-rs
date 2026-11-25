# Sources

Les scripts Lua tournent dans le Lua 5.4 intégré au binaire (`mlua`, `scout-sources/src/import.rs` +
`lua/sandbox.lua` : `arg` rempli, `io.write`/`io.stdout` collectés en mémoire, `io.stderr` ignoré, `loadfile`
lu par Rust pour les chemins Unicode sous Windows). Ci-dessous, `lua x.lua a b` = script `x.lua` lancé avec `a b`.

- **Scout** : addon détecté par `Interface/AddOns/Scout/Scout.toc` (sinon bouton d'import rouge dans Réglages,
  vérifié au démarrage, à l'ouverture des Réglages et quand le dossier WoW change). `lua <wow_dir>/Interface/AddOns/Scout/tools/export.lua <Scout.lua>` (historique) et `… --names`
  (noms + catégories). Format CSV = **contrat avec l'addon** : le changer des deux côtés (`scout-sources/src/import.rs`).
  Rappeler `/reload` en jeu avant d'importer (le jeu n'écrit le fichier qu'à ce moment).
- **PNJ** : `scout-sources/lua/vendor_export.lua` (intégré), à chaque import Scout : `ScoutDB.merchants` (PNJ, zone, position,
  stock limité ; prioritaire) et `ScoutDB.vendor` (anciens relevés sans PNJ). La liste de composants vendus sans limite
  (lignes `foreverguide`) ne vient plus que du paquet de référence du serveur. `BuyPrice` du client existe pour **tous**
  les objets : ne jamais s'en servir pour dire « vendu par un PNJ ». Revente au marchand = `SellPrice`.
- **Butin** :
  - Paquet de référence : le butin de monstres, coffres et quêtes (chances, PNJ, noms français) vient du paquet
    téléchargé du serveur (`docs/donnees.md`), **figé** : il dérive de Forever Guide (GPL), mais l'outil ne lit plus cet addon
    (ni son historique de prix, ni son butin, ni sa liste de marchands) ; pas d'import de remplacement.
  - Scout : `export.lua <Scout.lua> --loot` (contrat avec l'addon : `npc_id,name,zone,kills,skins,kind,item_id,drops,quantity`).
- **Contours des zones** : addon **OpenMap** (`Media/Textures/Forever/MapTerritory/<UiMap>.tga`, 256×256, forme dans
  l'alpha, lignes de bas en haut), lus par `scout-sources/src/tga.rs` quand la carte s'ouvre. Ce sont des
  images de Blizzard : on les lit dans le dossier AddOns, on ne les copie jamais dans le dépôt. Sans OpenMap,
  zones en rectangles.
- **Données du jeu** : téléchargées **par le serveur de prix** (`scout-server/src/gamedata.rs`, crate partagée
  `scout-gamedata`), une seule fois par version de Forever, puis servies à chaque appli en un paquet versionné
  (`docs/serveur.md` §3.7) ; l'appli ne contacte plus ni wago.tools ni Wowhead. Tables du client Forever (DB2) en CSV,
  français, depuis `https://wago.tools/db2/<Table>/csv?build=<build>&locale=frFR` ; dernière version = produit `wowf` de
  `https://wago.tools/api/builds`, vérifiée par le serveur toutes les 6 h ; l'appli compare au démarrage son paquet
  au dernier du serveur (Réglages affiche « À jour », « Nouvelles données » ou « Nouvelle version »).
  14 tables (`TABLES` dans `scout-gamedata/src/wago.rs`) ; assemblage comme le dépôt public `alcaras/forever-ref` : métiers = lignes 171,
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
  tables : le serveur (redéployé) construit alors un paquet de ce format, et une appli ne prend que les paquets de
  son format (`/v1/gamedata/latest?format=`).
- **Wowhead** (`scout-gamedata/src/wowhead.rs`, interrogé par le serveur seulement, `https://www.wowhead.com/classic/fr/item=<id>&xml`) : ~550 objets de
  recettes (surtout repris de Season of Discovery) sont envoyés par le serveur de Forever en hotfix : ils sont
  dans `Item` mais pas dans `ItemSparse`, donc sans nom chez wago.tools (et absents de `DBCache.bin` tant
  qu'on ne les a pas croisés). À la mise à jour, `Tables::missing_recipe_items` → nom, qualité, niveaux, revente
  depuis Wowhead ; la catégorie reste celle de `Item` (`Tables::unnamed_categories`). 1 s de pause entre deux
  requêtes (sans pause, CloudFront bloque l'adresse en 403 après ~300) ; au premier refus (403…) on
  arrête de demander : le paquet est publié marqué incomplet (`complete: false`, « ⚠ Incomplètes » dans Réglages)
  et le serveur recommence au tour suivant (son cache `data/gamedata/wowhead.json` garde ce qui a été lu).
  Le serveur nomme aussi, de la même façon, les objets vus à l'HdV que ni le client ni le paquet de référence
  ne nomment ; ceux que Wowhead ne connaît pas sont retenus dans `wowhead_unknown.json` (`docs/serveur.md` §3.7).
- Les sources sont citées dans Réglages → bouton « ℹ Sources des données » qui ouvre la page `/thanks`
  du serveur de prix (`scout_core::credits::SOURCES` : à compléter si on en ajoute une).

## Butin et marchands de référence du serveur
`GET /v1/reference/latest?format=N` (manifeste `ReferenceManifest`) puis `/v1/reference/{format}/{updated}` (SQLite gzippé :
`drop_*` et `vendor_prices`). Le worker ne télécharge que si le manifeste est plus récent que `meta.server_reference`
(au démarrage, sans bruit si le serveur ne répond pas, et après « Mettre à jour les données du jeu »). Le butin est
remplacé en bloc, les marchands sont ajoutés (ceux déjà présents, vus par Scout, gagnent). Un paquet sans butin est refusé.
