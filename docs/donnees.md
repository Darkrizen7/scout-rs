# Données

- Base `~/.local/share/scout-rs/prix.db`, réglages `~/.config/scout-rs/config.toml` (`wow_dir` = dossier
  `_forever_`, détecté dans les préfixes Wine usuels ; `theme` = `light` / `dark` ; `hide_obsolete`,
  vrai par défaut : masque les objets inutilisables : « Deprecated / Unused / [INUTILISÉ] / périmé / [PH] / OLD /
  Monster - / DNT » et les objets de test (mot entier « Test » / « Testing »), `gamedata::is_obsolete` ;
  `last_import` = date du dernier import Scout réussi ; `addon_history_day` = jour local (`AAAA-MM-JJ`) de la dernière
  écriture réussie de `History.lua` dans l'addon (au plus une par jour et par maison) ; `addon_history_house` = (région, maison) de ce fichier ; `setup_done` = assistant de premier lancement
  terminé (absent d'une ancienne config : vrai, pas d'assistant ; pas de fichier : faux, l'assistant s'ouvre) ; `[price_server]` : `url` du serveur de prix (défaut
  `scout_api::DEFAULT_SERVER`), `region` (eu, us, kr, tw) et `house` = hôtel des ventes affiché partout, `last_sync` = date
  des derniers prix téléchargés) ; `[profession_skills]` : compétence du personnage par métier (clé = id du métier
  en texte, valeur 1–300), saisie dans l'onglet Recettes. Chemins via `directories`.
- Tables :
  - Market : `items (item_id, name, class_id, subclass_id, class_name, subclass_name)` (noms et catégories vus par
    Scout), `snapshots (item_id, source, region, house, ts, price, quantity)` clé `(item_id, region, house,
    source, ts)` : **hôtel des ventes** = région Battle.net (`eu`…) + n° de maison (114…), `''` / 0 tant qu'aucun n'est
    choisi (rattachés au premier choisi : `Db::claim_unassigned`, migration `market/migrate` du 2026-11-25 qui a
    gardé toutes les lignes) ; toutes les lectures ne montrent que l'HdV choisi, et un import Scout donne à chaque relevé la maison du même mode que l'HdV choisi, du côté de sa faction (APP-26, sans nouvelle colonne ni migration ; ancien export : l'HdV choisi) à ce
    moment ; `source` ∈ `scout | scout-api` (les lignes `auctionator` / `foreverguide` des anciens imports sont
    supprimées à l'ouverture, une fois : `market/migrate::drop_addon_prices`, clé `addon_prices_removed` de `meta`), prix en cuivre (NULL = pas en vente), `ts` en secondes Unix.
    `scout-api` = notre serveur de prix (API Battle.net) : derniers prix gardés seulement quand ils changent (plus le
    premier de chaque jour), historique complet d'un objet à l'ouverture de sa fiche ; `scout_api_history (item_id,
    region, house, until, checked_at)` dit jusqu'où cet historique est téléchargé (redemandé après 1 h).
    `sales (region, house, day, item_id, sold_proven, sold_estimated, sold_value)` : totaux de ventes par jour UTC
    du serveur (`/sales`), les lignes d'un jour remplacées à chaque téléchargement (totaux cumulés), 30 jours gardés
    au plus (agrégats du serveur, pas des données de l'utilisateur : purgés sans risque).
    Imports **idempotents** (`INSERT OR IGNORE`) : la base garde tout, même ce que Scout oublie après 21 jours.
  - `favorites (item_id)` : partagés par Market et Objets. `zone_favorites (zone)` : zones favorites du Butin.
  - `vendor_prices (item_id, source, npc_id, price, npc, zone, x, y, limited)` : complétée à chaque import Scout
    (voir la base de référence).
  - Butin de référence (dérivé de Forever Guide, figé : n'est plus importé ; remplacé en bloc par la base de référence) : `drop_sources (item_id, kind,
    source_id, chance, quest_only, more)` (`source_id` 0 = « et `more` autres non listées »), `drop_npcs`,
    `drop_names` (objets du monde, quêtes), `drop_meta` (`fingerprint`, `updated`). `drop_npcs.ui_map` = carte (UiMap) de la
    zone du PNJ : lien avec la carte du jeu (les noms de zone de Forever Guide et du client diffèrent parfois).
  - Butin réel de Scout (remplacé à chaque import : l'addon garde des totaux) : `loot_npcs (npc_id, name, zone,
    kills, skins)`, `loot_observed (npc_id, kind, item_id, drops, quantity)`, `kind` ∈ `loot | skin`.
  - Données du jeu `game_*` (remplacées en bloc) : `game_meta` (`build`, `updated` = version du paquet du serveur, `format`, `complete` : 0 si Wowhead a refusé des requêtes au serveur), `game_items`,
    `game_categories`, `game_professions`, `game_recipes`, `game_reagents`, `game_recipe_items`, `game_maps`, `game_zone_factions` (nom de zone → 2 Alliance / 4 Horde)
    (continents et zones : UiMap, rectangle en coordonnées monde).
  - `wowhead_items` (mêmes colonnes que `game_items`) : objets demandés à Wowhead par les anciennes versions de
    l'outil ; plus lue ni écrite par l'appli (le serveur les met dans son paquet), gardée parce que la base de
    référence la contient encore.
  - `meta (key, value)` : `reference` = ancienne clé (base intégrée, supprimée), inoffensive et conservée ;
    `server_reference` = `format:updated` du paquet butin + marchands du serveur déjà appliqué
    (`Db::server_reference`).
- **Paquet de référence du serveur** (`scout-server/data/reference.db.gz`, GPL-3.0-or-later, téléchargé, jamais
  intégré au binaire) : butin `drop_*`, `vendor_prices` et `items` (noms et catégories seulement, lignes nommées). Jamais
  `snapshots`, favoris ni `loot_*`. Appliqué par `Db::apply_loot_vendors` (`scout-db/src/reference.rs`) : butin remplacé, marchands
  ajoutés (`INSERT OR IGNORE` : le local gagne), noms et catégories d'objets comblés sans jamais écraser
  (`ON CONFLICT … COALESCE`). Les marchands vus par Scout s'ajoutent à chaque import (jamais
  supprimés). Régénérer : `cargo run -p scout-worker --example server_reference` (depuis `prix.db`).
- **Premier lancement** : sans données du jeu (`game_meta` vide), le worker télécharge tout seul les données du
  jeu puis le butin et les marchands (`tasks::update_all`, tâche `GameData`) ; les onglets Objets, Recettes et
  Butin affichent « pas encore téléchargées » avec le bouton de téléchargement en attendant.
- Évolution du schéma : `CREATE TABLE IF NOT EXISTS` + colonnes ajoutées si absentes (`ITEM_COLUMNS` dans
  `scout-db/src/lib.rs` et `game.rs`).
- Les numéros d'objet sont ceux du jeu, identiques dans Scout et les tables du client.
