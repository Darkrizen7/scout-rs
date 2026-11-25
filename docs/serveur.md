# Serveur de prix (API Blizzard) — spec de travail

> Démarrée le 2026-11-25. Étape 1 (collecteur) codée dans `scout-server/` ; sonde d'origine :
> `scout-sources/examples/forever_ah.rs`.
> Décisions dans [Décisions](#décisions), points à trancher dans [Questions ouvertes](#questions-ouvertes).

## 1. But

Un petit serveur sur le VPS Oracle (module du homelab, `~/dev/vps`) relève l'HdV de WoW Forever via l'API
Battle.net, garde les **instantanés bruts** et en tire des prix par heure ; l'appli les télécharge et les
affiche à côté des scans Scout. Les identifiants Battle.net ne quittent jamais le serveur.

## 2. Ce qu'on sait de l'API (vérifié le 2026-11-25)

- Jeton : `POST https://oauth.battle.net/token` (client credentials, Basic auth), valable 24 h (`expires_in`).
- Namespace `dynamic-forever-{région}`, hôte `{région}.api.blizzard.com`, langue de la région (`fr_FR` pour eu,
  `en_US`, `ko_KR`, `zh_TW`). Le serveur relève **eu, us, kr, tw** (`SCOUT_REGIONS`) : une base et un dossier de
  bruts par région (`data/` pour eu, `data/<région>/` pour les autres) ; `/v1/regions` ne liste que celles où le
  relevé a trouvé des maisons (une région sans HdV Forever répond 404, dit une seule fois dans le journal).
- `GET /data/wow/ruleset/2/auctions/` : les 15 maisons (le ruleset doit exister mais ne filtre rien).
- `GET /data/wow/ruleset/2/auctions/{house}` (objets) et `…/{house}/commodities` (empilables).
- 15 maisons par région, **les mêmes n° partout**, par type de royaume et côté (Alliance / Horde / Flots noirs,
  ce dernier commun aux deux factions) : PvP 110-113-116, **Normal 111-114-117 (l'utilisateur : Horde = 114,
  Orgrimmar / Pitons-du-Tonnerre)**, RP 119-120-121, RP-PvP 123-124-125 ; 112-115-118 vides partout (2026-11-25). KR et TW
  n'ont que Normal et PvP ; EU et US les quatre.
  Comparaison avec un scan Scout pris 2 min avant : 71 % de prix minimum identiques, 86 % à ±5 % sur la 114,
  ≤ 5 % ailleurs.
- Taille d'un relevé complet (30 réponses) : 83 Mo de JSON, **7,2 Mo en zstd -19**, ~6 s.
- Limites : 36 000 appels / h, 100 / s (429 au-delà). Nous : 30 appels par relevé.
- Fréquence de régénération côté Blizzard : **inconnue** pour Forever (environ 1 h sur retail).
  `Last-Modified` / `If-Modified-Since` : à tester.
- Enchères : `item.id` (+ `bonus_lists`, `modifiers`…), `buyout` (total du lot, absent si enchère seule),
  `bid`, `quantity`, `time_left`. Empilables : `item.id`, `quantity`, `unit_price`, `time_left`.
  Ni vendeur ni identifiant de royaume. Maisons vides : pas de champ `auctions`.

CGU (Blizzard Developer API Terms, 2019) à respecter : clé confidentielle (jamais dans l'appli), Blizzard cité
comme source sans laisser croire à un partenariat, aucune marque Blizzard dans le nom ou l'URL, politique de
confidentialité publiée, gratuit et sans pub. La clause des 30 jours (§2(s)) n'est **volontairement** pas
appliquée (choix de l'utilisateur) : tout ce qui vient de l'API reste séparé pour pouvoir être supprimé d'un coup.

## 3. Pièces

```
Battle.net ──► scout-server (VPS) ──HTTPS──► app (worker, source « blizzard ») ──► prix.db
                 collecteur  : relevés bruts .json.zst + index SQLite
                 agrégateur  : prix par heure / maison / objet
                 API HTTP    : maisons, prix depuis une date
```

### 3.1 Code

- Crate **`scout-server`** (binaire) dans ce workspace, avec son propre client Battle.net (`scout-server/src/blizzard.rs`) :
  pas dans `scout-sources`, qui embarque Lua (C) et tout ce dont seule l'appli a besoin. Il dépend de `scout-core` et de
  `scout-gamedata` (données du jeu, §3.7). `app` n'en dépend pas.
- HTTP entrant : `axum` + `tokio` (déjà la pile de `dofus_follow_up`). HTTP sortant : `ureq` dans
  `spawn_blocking`, comme le reste de l'outil.
- Réglages par variables d'environnement : `BLIZZARD_CLIENT_ID`, `BLIZZARD_CLIENT_SECRET`, `BLIZZARD_REGION` (eu),
  `SCOUT_DATA_DIR` (data), `SCOUT_LISTEN` (0.0.0.0:3000), `SCOUT_INTERVAL_SECS` (600), `SCOUT_ZSTD_LEVEL` (15),
  `SCOUT_GAMEDATA_SECS` (21600 : vérification d'une nouvelle version de Forever, §3.7),
  `SCOUT_STAGGER_SECS` (5 : décalage entre les premiers relevés de deux régions).
- Mémoire : un seul travail lourd (téléchargement + agrégation + compression) à la fois, toutes régions
  confondues (sémaphore à 1 permis, `Collector::heavy`), après des OOM à 256 Mo. Mesure du pic :
  `SCOUT_PEAK_FILE=<relevé.json.zst> cargo test -p scout-server --release peak -- --ignored --nocapture`
  (affiche `VmHWM`, `src/peak.rs`).
- Image arm64 construite sur le PC (`tools/update.sh scout <version>`, qui lit `modules/scout/release.conf`),
  Dockerfile à la racine sur le modèle de `dofus_follow_up` (Debian slim, certificats copiés, uid 10001).

### 3.2 Collecteur

- Boucle toutes les **10 min** au début (pour mesurer la cadence de Blizzard), puis on espacera.
- Pour **chacune des 15 maisons** (y compris les vides, qui peuvent se remplir) × {objets, empilables} : GET (avec `If-Modified-Since` s'il marche), calcul du
  SHA-256 du corps ; **même empreinte que le dernier relevé → rien n'est écrit**, juste `last_seen` mis à jour.
- Écrit `data/raw/AAAA/MM/JJ/{house}_{kind}_{HHMMSS}.json.zst` (zstd niveau ~15 : bon ratio, peu de CPU sur l'ARM).
- Base SQLite `data/scout-server.db`, via **sqlx 0.9** (mêmes conventions que `scout-db` : SQL statique, valeurs en
  `bind`, migrations au démarrage). Elle contient l'index des bruts **et les agrégats que l'appli télécharge** :

  | Table | Colonnes |
  |---|---|
  | `houses` | `id`, `name`, `first_seen`, `last_seen` |
  | `raw_snapshots` | `house`, `kind`, `fetched_at`, `last_seen`, `last_modified`, `sha256`, `path`, `raw_bytes`, `bytes`, `auctions`, `aggregated` |
  | `prices` | `house`, `ts` (= `fetched_at` de l'instantané), `item_id`, `min_unit`, `p25_unit`, `median_unit`, `quantity`, `auctions`, `buyout_less` — PK (`house`, `ts`, `item_id`) : l'appli lit par maison et par date |
  | `fetch_log` | `ts`, `house`, `kind`, `status`, `ms`, `error` (supervision, purgé après 7 jours) |

- Erreurs : 401 → nouveau jeton puis un seul nouvel essai ; 429 / 5xx → on saute ce tour pour cette maison ;
  le jeton est gardé en mémoire et renouvelé avant `expires_in`.

### 3.3 Agrégation (`aggregate.rs`, `db/prices.rs`)

- À chaque **nouvel** instantané (contenu différent du précédent), dans la même transaction que sa ligne
  `raw_snapshots` : une ligne `prices` par objet de la liste, plus une ligne `quantity = 0` (sans prix) pour chaque
  objet de la version précédente **de la même liste** qui n'y est plus. Les deux listes d'une maison n'ont jamais le
  même `ts` (au besoin une seconde de plus), ce qui permet de retrouver la version précédente de chacune.
- Par `item_id` seul : les variantes (suffixes aléatoires, `bonus_lists`) sont fusionnées pour l'instant.
- Prix unitaire = `unit_price` (empilables), ou `buyout / quantity` ; les enchères sans `buyout` (`buyout_less`)
  comptent dans `quantity` et `auctions` mais pas dans les prix (`NULL` si toutes le sont).
- `p25_unit` et `median_unit` sont **pondérés par la quantité** : le prix auquel on achète un quart / la moitié
  des unités en vente. Pas de prix max (trop bruité : annonces absurdes).
- Au démarrage, les instantanés écrits sans leurs prix (`aggregated = 0` : avant cette table, ou interruption)
  sont relus depuis leur fichier et agrégés (30 instantanés en 0,6 s).
- Les bruts restent la référence : on pourra recalculer `prices` si l'on change de statistiques.
- Vérifié le 2026-11-25 sur la 114 contre un scan Scout de 8 min plus ancien : 90 % de prix min identiques,
  96 % à ±5 %.

### 3.4 API exposée

Route **`https://scout.arthurdevtest.duckdns.org`** (Traefik, `[web] host = "scout"` du module ; ni « wow » ni
« blizzard » dans l'URL, CGU §2(m)).

| Méthode | Chemin | Clé | Réponse |
|---|---|---|---|
| GET | `/health` | non | JSON : par région `last_success` / `seconds_since`, puis `free_gb`, `low_disk`, `raw_stopped` |
| GET | `/privacy` | non | politique de confidentialité (HTML, `api/privacy.html`) + mention Blizzard |
| GET | `/thanks` | non | remerciements : sources des données (HTML généré une fois depuis `scout_core::credits`, `api/thanks.rs`) |
| GET | `/v1/regions` | oui | JSON : les régions où Forever a des hôtels des ventes |
| GET | `/v1/{region}/houses` | oui | JSON : `id`, `name`, `last_update` (dernier `ts` de prix), `last_check` |
| GET | `/v1/{region}/houses/{id}/latest` | oui | CSV (même format) : la dernière version des deux listes, tous les objets ; `quantity = 0` = vient de partir |
| GET | `/v1/{region}/houses/{id}/items/{item}/history?since=<unix>` | oui | CSV : toutes les lignes d'un objet après `since` (index `prices_item`) |
| GET | `/v1/{region}/houses/{id}/prices?since=<unix>` | oui | CSV `ts,item_id,min_unit,p25_unit,median_unit,quantity,auctions,buyout_less` (vide = `NULL`), du plus ancien au plus récent |
| GET | `/v1/gamedata/latest?format=<GAME_FORMAT>` | oui | JSON `Manifest` : `format`, `build`, `updated` (= version), `complete`, `bytes` ; 404 si aucun paquet de ce format |
| GET | `/v1/gamedata/{format}/{updated}` | oui | le paquet (`application/gzip`, pas recompressé) ; 404 s'il n'est plus le dernier |

- **Pages** (`api.rs`, `db/prices.rs`) : ~200 000 lignes au plus (environ un jour d'une grosse maison), toujours
  des instantanés entiers. En-têtes `X-Scout-Until` (le `since` de la page suivante) et `X-Scout-More: 1` s'il
  en reste : l'appli boucle jusqu'à `X-Scout-More: 0`.
- Réponses compressées en gzip si le client le demande (`tower-http`, ureq le fait de lui-même). Mesuré sur une
  version de la 114 : 6 847 lignes, 270 Ko de CSV, **66 Ko en gzip**, 14 ms.
- Pas d'accès aux bruts par l'API (gros, et inutiles à l'appli).
- **Clé d'accès** (`api/access.rs`) : en-tête `X-Scout-Key`, réglage secret `SCOUT_KEYS` du module
  (`nom=clé,…`, une par personne, révocable, ≥ 16 caractères, comparaison en temps constant). Aucune clé =
  `/v1` refuse tout. Pas de comptes.
- **Limite de débit** : seaux à jetons en mémoire, **600 requêtes / h par clé** (un premier téléchargement
  parcourt tout l'historique) et **20 / h par adresse IP pour les requêtes sans clé valide** (contre qui devine
  des clés) ; 429 au-delà. IP = dernière entrée de `X-Forwarded-For` (celle qu'ajoute Traefik).
- Le service ne journalise pas les requêtes et n'écrit aucune IP ; **Traefik, lui, a ses journaux d'accès
  activés** (`--accesslog=true` dans `homelab-core`) : `/privacy` le dit.

### 3.5 Module homelab (`~/dev/vps/modules/scout/`)

- `module.toml` : `[runtime] autostart = true`, `[data] owner = "10001:10001"`, `[web]` (port 3000,
  `auth = "public"`, la clé est vérifiée par l'app), `[health] http = "/health"`,
  `[[settings]]` `type = "secret"` + `env` (déjà gérés par l'agent : `agent/src/settings.rs`) pour `BLIZZARD_CLIENT_ID`, `BLIZZARD_CLIENT_SECRET`, `SCOUT_KEYS` saisis dans le panel,
  `[resources] memory = "256m"` (une réponse de 9 Mo se parse en streaming), `[backup] paths = ["scout-server.db"]`
  (les bruts sont trop gros pour l'archive : on accepte de les perdre en cas de panne du disque).
- `docker-compose.yml` : un service, `./data:/app/data`.

### 3.6 Appli

- Source **`scout-api`** dans `snapshots` (`price` = `min_unit`, `NULL` si l'objet est parti ; `quantity`), à côté de
  `scout` (les anciennes sources `auctionator` / `foreverguide` ne sont plus importées et sont purgées à l'ouverture de la base). Supprimer cette source = une requête.
- **Derniers prix** (`/v1/{region}/houses/{id}/latest`, tous les objets) : vérifiés au démarrage puis toutes les 10 min,
  téléchargés quand ils ont plus d'**1 h** (ou par « ⟳ Mettre à jour » dans les Réglages). Une ligne n'est écrite que
  si le prix a changé, plus la première de chaque jour local (chaque jour garde son prix) ; un objet qui n'y est plus
  reçoit une ligne `NULL` (`scout_core::scout_api::latest_rows`).
- **Historique d'un objet** (`/v1/{region}/houses/{id}/items/{item}/history?since=`) : à l'ouverture de sa fiche Market, la base
  est affichée tout de suite, puis complétée depuis le serveur (tout la première fois, ensuite ce qui suit `until`) si
  le cache a plus d'1 h ; gardé en entier (une ligne par version). Cache : table `scout_api_history`.
- Fusion des graphiques (`merge`) : un jour avec un scan Scout, Scout jusqu’à son dernier scan puis les relevés Scout API qui suivent (point du jour = le plus récent, Scout à égalité) ; sinon Scout API seul.
- Réglages (`ui/settings/price_server.rs`) : adresse (défaut `scout.arthurdevtest.duckdns.org`), clé, région (parmi
  `/v1/regions`), mode de jeu (type de royaume de l'HdV choisi, `ServerChoice::modes` / `house_for_mode` : pas de réglage
  propre, changer de mode change d'HdV), hôtel des ventes ; le même sélecteur d'HdV est dans l'en-tête
  (`ui/widgets/house.rs`) et ne propose que les HdV du mode courant (`ServerChoice::houses_to_offer`).
  `[price_server]` dans `config.toml`. Noms : « EU Normal · Horde (114) » (`ServerHouse::label`, `scout_api::realm_type`).
- Crédits (`ui/credits.rs`) : API Battle.net de Blizzard Entertainment, non approuvé ni affilié.
- Chaque ligne de `snapshots` a sa région et son HdV : changer d'HdV ne mêle rien, chaque HdV garde son cache.
- **TODO** : compresser à un point par jour les relevés `scout-api` de plus de 3 mois (`prix.db` grossit d'environ
  2 400 lignes par version téléchargée).
- **Données du jeu** : « Mettre à jour » (Réglages, ou les boutons des onglets Objets / Recettes) et la vérification au
  démarrage demandent `/v1/gamedata/latest` pour le `GAME_FORMAT` de l'appli ; le paquet est téléchargé s'il diffère
  (autre build, autre format) ou est plus récent (`Manifest::newer_than` contre `game_meta`), puis remplace les
  `game_*`. Sans clé : la base de référence intégrée seulement.

### 3.7 Données du jeu (`gamedata.rs`, crate `scout-gamedata`)

- Les tables du client (wago.tools) et les objets que Wowhead nomme sont téléchargés **par le serveur seul**, une fois
  par version de Forever, au lieu d'une fois par appli (Wowhead bloque après quelques centaines de requêtes).
- Au démarrage puis toutes les 6 h (`SCOUT_GAMEDATA_SECS`) : dernière version `wowf` de wago.tools ; nouveau paquet si
  elle a changé, si le `GAME_FORMAT` du serveur (redéployé) n'en a pas encore, ou si le dernier est incomplet
  (Wowhead a refusé : nouvel essai au tour suivant). Environ 10 min la première fois (1 s entre deux objets Wowhead).
- `data/gamedata/wowhead.json` : objets déjà nommés par Wowhead (chacun n'est demandé qu'une fois) ;
  `data/gamedata/<format>/<updated>.json.gz` (`scout_gamedata::Package` : `Tables` en JSON gzip) et `latest.json`
  (`Manifest`). Seul le dernier paquet de chaque format est gardé ; les anciens formats restent pour les anciennes
  versions de l'appli. Tout se reconstruit : rien à sauvegarder.

## 4. Étapes

1. **Collecteur seul** (bruts + index + `fetch_log`), déployé le 2026-11-25 ; une semaine d'observation : cadence réelle de
   Blizzard, volume disque, erreurs.
2. Agrégation, API, clé, limite de débit, `/privacy` (**faits** le 2026-11-25).
3. Source `scout-api` dans l'appli et ses réglages (**faits** le 2026-11-25).
4. Plus tard : choisir quoi garder des bruts (différences entre instantanés, sous-échantillonnage).

## 5. Volume disque

Place libre sur `/mnt/games` : **82 Go** (2026-11-25).

Mesuré sur une version complète (15 maisons, 2026-11-25) : **8 Mo de bruts** (zstd 15, ~9 % du JSON) et
**1,7 Mo dans `prices`** (52 778 lignes). Si Blizzard publie une version par heure : ~190 Mo + ~40 Mo par jour,
**~70 Go + ~15 Go par an** ; rien n'est écrit quand rien ne change.

- Le disque est donc plein en ~1 an : `/health` signale « disque presque plein » sous **10 Go libres** et le
  collecteur cesse d'écrire des bruts sous **3 Go** (les agrégats continuent). La décision sur les bruts (§4,
  étape 4) est à prendre dans les premiers mois.

## Décisions

| Date | Décision |
|---|---|
| 2026-11-25 | Serveur sur le VPS Oracle, module du homelab ; identifiants seulement côté serveur. |
| 2026-11-25 | Garder les instantanés bruts tant qu'on ne sait pas quoi conserver ; pas de purge à 30 jours. |
| 2026-11-25 | Toutes les maisons sont relevées, gardées et agrégées (bruts et `prices`), pas seulement celle de l'utilisateur. |
| 2026-11-25 | Service en Rust ; base SQLite (sqlx) qui garde les agrégats servis à l'appli. |
| 2026-11-25 | Sous-domaine `scout.arthurdevtest.duckdns.org`. |
| 2026-11-25 | Crate `scout-server` dans ce workspace, avec son propre client Battle.net (pas `scout-sources`). |
| 2026-11-25 | `prices` : min, p25 et médiane pondérés par la quantité, quantité, enchères, sans prix immédiat ; pas de max ; variantes fusionnées par `item_id` (à séparer plus tard). |
| 2026-11-25 | Cadence de Blizzard : une version par heure, vers HH:21 UTC. |
| 2026-11-25 | Toutes les régions où Forever a une API d'HdV (eu, us, kr, tw vérifiées par le relevé) ; `prix.db` a une région et un HdV par ligne, les relevés existants rattachés à EU 114. |
| 2026-11-25 | Noms des HdV par type de royaume et côté. |
| 2026-11-25 | Données du jeu (wago.tools, Wowhead) téléchargées par le serveur et servies en paquet versionné ; l'appli ne les télécharge plus elle-même ; code partagé dans la crate `scout-gamedata`. |
| 2026-11-25 | Auctionator et Forever Guide ne sont plus importés ; leurs relevés de prix sont supprimés de `prix.db`, le butin reste celui de la base de référence. |
| 2026-11-25 | Plus tard : estimer les ventes (une enchère qui disparaît avant expiration, `id` stable d'un relevé à l'autre). |

## Questions ouvertes

- Séparer les variantes (`bonus_lists` → suffixes) ; estimation des ventes.
