# Données

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
  - `favorites (item_id)` : partagés par Market et Objets. `zone_favorites (zone)` : zones favorites du Butin.
  - `vendor_prices (item_id, source, npc_id, price, npc, zone, x, y, limited)` : remplacée à chaque import Scout.
  - Butin de Forever Guide (remplacé en bloc quand ses fichiers changent) : `drop_sources (item_id, kind,
    source_id, chance, quest_only, more)` (`source_id` 0 = « et `more` autres non listées »), `drop_npcs`,
    `drop_names` (objets du monde, quêtes), `drop_meta` (`fingerprint`). `drop_npcs.ui_map` = carte (UiMap) de la
    zone du PNJ : lien avec la carte du jeu (les noms de zone de Forever Guide et du client diffèrent parfois).
  - Butin réel de Scout (remplacé à chaque import : l'addon garde des totaux) : `loot_npcs (npc_id, name, zone,
    kills, skins)`, `loot_observed (npc_id, kind, item_id, drops, quantity)`, `kind` ∈ `loot | skin`.
  - Données du jeu `game_*` (remplacées en bloc) : `game_meta` (`build`, `updated`, `format`, `complete` : 0 si Wowhead a refusé des requêtes), `game_items`,
    `game_categories`, `game_professions`, `game_recipes`, `game_reagents`, `game_recipe_items`, `game_maps`, `game_zone_factions` (nom de zone → 2 Alliance / 4 Horde)
    (continents et zones : UiMap, rectangle en coordonnées monde).
  - `wowhead_items` (mêmes colonnes que `game_items`) : objets demandés à Wowhead, **gardés** d'une mise à jour à
    l'autre (chaque objet n'est demandé qu'une fois).
- Évolution du schéma : `CREATE TABLE IF NOT EXISTS` + colonnes ajoutées si absentes (`ITEM_COLUMNS` dans
  `scout-db/src/lib.rs` et `game.rs`).
- Les numéros d'objet sont ceux du jeu, identiques dans Scout, Auctionator, Forever Guide et les tables du client.
