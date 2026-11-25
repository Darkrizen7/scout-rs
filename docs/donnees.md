# Données

- Base `~/.local/share/scout-rs/prix.db`, réglages `~/.config/scout-rs/config.toml` (`wow_dir` = dossier
  `_forever_`, détecté dans les préfixes Wine usuels ; `theme` = `light` / `dark` ; `hide_obsolete`,
  vrai par défaut : masque les objets inutilisables : « Deprecated / Unused / [INUTILISÉ] / périmé / [PH] / OLD /
  Monster - / DNT », `gamedata::is_obsolete` ;
  `last_import` = date du dernier import Scout réussi ; `[price_server]` : `url` du serveur de prix (défaut
  `scout_api::DEFAULT_SERVER`), `key` = clé d'accès personnelle, `region` (eu, us, kr, tw) et `house` = hôtel des ventes affiché partout, `last_sync` = date
  des derniers prix téléchargés). Chemins via `directories`.
- Tables :
  - Market : `items (item_id, name, class_id, subclass_id, class_name, subclass_name)` (noms et catégories vus par
    Scout), `snapshots (item_id, source, region, house, ts, price, quantity)` clé `(item_id, region, house,
    source, ts)` : **hôtel des ventes** = région Battle.net (`eu`…) + n° de maison (114…), `''` / 0 tant qu'aucun n'est
    choisi (rattachés au premier choisi : `Db::claim_unassigned`, migration `market/migrate` du 2026-11-25 qui a
    gardé toutes les lignes) ; toutes les lectures ne montrent que l'HdV choisi, et un import Scout / Auctionator /
    Forever Guide prend l'HdV choisi à ce moment ; `source` ∈
    `scout | scout-api | auctionator | foreverguide`, prix en cuivre (NULL = pas en vente), `ts` en secondes Unix.
    `scout-api` = notre serveur de prix (API Battle.net) : derniers prix gardés seulement quand ils changent (plus le
    premier de chaque jour), historique complet d'un objet à l'ouverture de sa fiche ; `scout_api_history (item_id,
    region, house, until, checked_at)` dit jusqu'où cet historique est téléchargé (redemandé après 1 h).
    Imports **idempotents** (`INSERT OR IGNORE`) : la base garde tout, même ce que Scout oublie après 21 jours.
  - `favorites (item_id)` : partagés par Market et Objets. `zone_favorites (zone)` : zones favorites du Butin.
  - `vendor_prices (item_id, source, npc_id, price, npc, zone, x, y, limited)` : complétée à chaque import Scout
    (voir la base de référence).
  - Butin de Forever Guide (remplacé en bloc quand ses fichiers changent) : `drop_sources (item_id, kind,
    source_id, chance, quest_only, more)` (`source_id` 0 = « et `more` autres non listées »), `drop_npcs`,
    `drop_names` (objets du monde, quêtes), `drop_meta` (`fingerprint`, `updated`). `drop_npcs.ui_map` = carte (UiMap) de la
    zone du PNJ : lien avec la carte du jeu (les noms de zone de Forever Guide et du client diffèrent parfois).
  - Butin réel de Scout (remplacé à chaque import : l'addon garde des totaux) : `loot_npcs (npc_id, name, zone,
    kills, skins)`, `loot_observed (npc_id, kind, item_id, drops, quantity)`, `kind` ∈ `loot | skin`.
  - Données du jeu `game_*` (remplacées en bloc) : `game_meta` (`build`, `updated`, `format`, `complete` : 0 si Wowhead a refusé des requêtes), `game_items`,
    `game_categories`, `game_professions`, `game_recipes`, `game_reagents`, `game_recipe_items`, `game_maps`, `game_zone_factions` (nom de zone → 2 Alliance / 4 Horde)
    (continents et zones : UiMap, rectangle en coordonnées monde).
  - `wowhead_items` (mêmes colonnes que `game_items`) : objets demandés à Wowhead, **gardés** d'une mise à jour à
    l'autre (chaque objet n'est demandé qu'une fois).
  - `meta (key, value)` : `reference` = identifiant (fin du gzip : CRC-32 et taille) de la base de référence
    déjà appliquée.
- **Base de référence** (`scout-worker/data/reference.db.gz`, intégrée par `include_bytes!`, GPL-3.0-or-later) :
  données du jeu `game_*`, `wowhead_items`, butin `drop_*`, `vendor_prices`. Jamais `items`, `snapshots`,
  favoris ni `loot_*`. Au premier lancement de chaque nouvelle version (identifiant différent de `meta`),
  décompressée à côté de `prix.db` puis attachée (`scout-db/src/reference.rs`) : données du jeu remplacées si
  elles sont au `GAME_FORMAT` courant et plus récentes (`game_meta.updated`) ou si la base locale n'en a pas ;
  butin remplacé si plus récent (`drop_meta.updated`, écrit par chaque import Forever Guide ; absent = plus
  ancien) ; marchands et objets Wowhead ajoutés (`INSERT OR IGNORE` : le local gagne). Un import Forever Guide
  local la remplace ensuite (empreinte différente). Les marchands vus par Scout s'ajoutent à chaque import (jamais
  supprimés), la liste de Forever Guide n'est remplacée que si Forever Guide est installé.
  Régénérer : `cargo run -p scout-worker --example reference` (depuis `prix.db`, qui doit avoir les données du
  jeu et le butin de Forever Guide). Le test `embedded_reference_is_up_to_date` échoue si son `GAME_FORMAT` ou
  son `DROPS_FORMAT` n'est plus celui du code : la régénérer.
- Évolution du schéma : `CREATE TABLE IF NOT EXISTS` + colonnes ajoutées si absentes (`ITEM_COLUMNS` dans
  `scout-db/src/lib.rs` et `game.rs`).
- Les numéros d'objet sont ceux du jeu, identiques dans Scout, Auctionator, Forever Guide et les tables du client.
