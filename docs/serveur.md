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
- Namespace `dynamic-forever-eu`, `locale=fr_FR`, hôte `eu.api.blizzard.com`.
- `GET /data/wow/ruleset/2/auctions/` : les 15 maisons (le ruleset doit exister mais ne filtre rien).
- `GET /data/wow/ruleset/2/auctions/{house}` (objets) et `…/{house}/commodities` (empilables).
- 15 maisons = 5 groupes de trois (Alliance / Horde / Flots noirs) : 110-113-116, **111-114-117 (celui de
  l'utilisateur, Horde = 114)**, 112-115-118 (vides), 119-120-121, 123-124-125.
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
  pas dans `scout-sources`, qui embarque Lua (C) et tout ce dont seule l'appli a besoin. `scout-core` le rejoindra à
  l'étape 2 pour les statistiques. `app` n'en dépend pas.
- HTTP entrant : `axum` + `tokio` (déjà la pile de `dofus_follow_up`). HTTP sortant : `ureq` dans
  `spawn_blocking`, comme le reste de l'outil.
- Réglages par variables d'environnement : `BLIZZARD_CLIENT_ID`, `BLIZZARD_CLIENT_SECRET`, `BLIZZARD_REGION` (eu),
  `SCOUT_DATA_DIR` (data), `SCOUT_LISTEN` (0.0.0.0:3000), `SCOUT_INTERVAL_SECS` (600), `SCOUT_ZSTD_LEVEL` (15).
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
  | `houses` | `id`, `name`, `faction` (alliance / horde / neutral), `group_id`, `first_seen`, `last_seen` |
  | `raw_snapshots` | `house`, `kind`, `fetched_at`, `last_modified`, `sha256`, `path`, `bytes`, `auctions`, `last_seen` |
  | `prices` | `house`, `item_id`, `ts` (heure du relevé), `min_unit`, `median_unit`, `quantity`, `auctions` — PK (`house`, `item_id`, `ts`) |
  | `fetch_log` | `ts`, `house`, `kind`, `status`, `ms`, `error` (supervision, purgé après quelques jours) |

- Erreurs : 401 → nouveau jeton puis un seul nouvel essai ; 429 / 5xx → on saute ce tour pour cette maison ;
  le jeton est gardé en mémoire et renouvelé avant `expires_in`.

### 3.3 Agrégation

- Après chaque nouvel instantané : une ligne `prices` par objet **dont une valeur a changé** depuis le relevé
  précédent de cette maison (un objet absent devient une ligne `quantity = 0`) ; la lecture prend la dernière
  ligne connue. Beaucoup d'objets ne bougent pas d'une heure à l'autre : c'est le principal gain de place.
- Prix unitaire = `unit_price`, ou `buyout / quantity` ; enchères sans `buyout` ignorées pour le prix mais
  comptées dans la quantité.
- Les bruts restent la référence : `scout-server reaggregate` recalcule `prices` depuis les fichiers quand on
  change de statistiques (d'où l'intérêt de les garder pour l'instant).
- Le calcul vit dans `scout-core` (pur, testé), partagé avec l'appli si besoin.

### 3.4 API exposée

Route **`https://scout.arthurdevtest.duckdns.org`** (Traefik, `[web] host = "scout"` du module ; ni « wow » ni
« blizzard » dans l'URL, CGU §2(m)).

| Méthode | Chemin | Réponse |
|---|---|---|
| GET | `/health` | `ok` + date du dernier relevé |
| GET | `/v1/houses` | maisons, faction, groupe, dernier relevé |
| GET | `/v1/houses/{id}/prices?since=<unix>` | CSV gzip `item_id,ts,min_unit,median_unit,quantity,auctions`, au plus ~7 jours par réponse (l'appli boucle) |
| GET | `/privacy` | politique de confidentialité + mention Blizzard |

- Pas d'accès aux bruts par l'API (gros, et inutiles à l'appli).
- **Clé d'accès** : en-tête `X-Scout-Key`, liste de clés dans un réglage secret du module (une par personne,
  révocable). Pas de comptes.
- **Limite de débit** : seau à jetons en mémoire par clé (et par IP pour les requêtes sans clé valide),
  ex. 60 requêtes / h ; 429 au-delà. Les IP ne sont jamais écrites sur disque (à dire dans `/privacy`).

### 3.5 Module homelab (`~/dev/vps/modules/scout/`)

- `module.toml` : `[runtime] autostart = true`, `[data] owner = "10001:10001"`, `[web]` (port 3000,
  `auth = "public"`, la clé est vérifiée par l'app), `[health] http = "/health"`,
  `[[settings]]` `type = "secret"` + `env` (déjà gérés par l'agent : `agent/src/settings.rs`) pour `BLIZZARD_CLIENT_ID`, `BLIZZARD_CLIENT_SECRET`, `SCOUT_KEYS` saisis dans le panel,
  `[resources] memory = "256m"` (une réponse de 9 Mo se parse en streaming), `[backup] paths = ["scout-server.db"]`
  (les bruts sont trop gros pour l'archive : on accepte de les perdre en cas de panne du disque).
- `docker-compose.yml` : un service, `./data:/app/data`.

### 3.6 Appli

- Nouvelle source `blizzard` dans `snapshots` (`price` = `min_unit`, `quantity`) ; même schéma, donc le Market
  la montre sans changement. La suppression de cette source se fait en une requête.
- Réglages : adresse du serveur, clé, maison (liste venant de `/v1/houses`, 114 par défaut pour l'utilisateur).
- Worker : synchro au démarrage puis toutes les heures, incrémentale (`since` = dernier `ts` reçu).
- Crédits (`ui/credits.rs`) : « Données d'hôtel des ventes : API Blizzard Battle.net. Outil non affilié à
  Blizzard Entertainment. »

## 4. Étapes

1. **Collecteur seul** (bruts + index + `fetch_log`), déployé ; une semaine d'observation : cadence réelle de
   Blizzard, volume disque, erreurs.
2. Agrégation + API + clé + limite de débit + `/privacy`.
3. Source `blizzard` dans l'appli et ses réglages.
4. Plus tard : choisir quoi garder des bruts (différences entre instantanés, sous-échantillonnage).

## 5. Volume disque

Place libre sur `/mnt/games` : **82 Go** (2026-11-25).

- Bruts, si chaque maison change à chaque heure : 7,2 Mo × 24 ≈ **170 Mo / jour, ~63 Go / an** ; moitié moins si
  Blizzard régénère toutes les deux heures, et rien n'est écrit quand rien ne change.
- `prices`, une ligne par objet et par relevé : ~53 000 objets (somme des 12 maisons non vides) × 24 ≈ 1,3 M
  lignes / jour, **~460 M lignes, 12 à 16 Go / an**. Avec « seulement ce qui change » (§3.3), à mesurer à l'étape 1 ;
  on peut aussi passer en journalier au-delà d'un certain âge.
- Au pire, le disque est plein en ~1 an : `/health` signale « disque presque plein » sous **10 Go libres** et le
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

## Questions ouvertes

- Cadence réelle de Blizzard et gain du « seulement ce qui change » : mesurés à l'étape 1.
