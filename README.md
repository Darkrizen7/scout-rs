# scout-rs

Outil de bureau (Rust, egui) pour étudier l'historique des prix de l'HdV relevé par l'addon
[Scout](https://github.com/Darkrizen7/scout-addon) sur WoW Forever.

- **Import Scout** : lance `lua Interface/AddOns/Scout/tools/export.lua` sur le `SavedVariables/Scout.lua`
  le plus récent et ajoute les nouveaux relevés à une base SQLite (sqlx). La base garde tout, même ce que
  Scout oublie en jeu après 21 jours. Fais `/reload` en jeu avant d'importer.
- **Historique Auctionator / Forever Guide** : import ponctuel des prix passés gardés par ces addons
  (`scout-sources/lua/legacy_export.lua`, intégré au binaire ; décode le CBOR d'Auctionator). Mêmes numéros d'objet
  que Scout ; les noms viennent des scans Scout.
- **Objets** : liste avec recherche (nom ou numéro) et tri. **Graphique** : prix min et quantité en vente
  dans le temps, une courbe par source. **Rentabilité** : bénéfice net après commission HdV.

## Commandes
- `cargo run --release`
- `cargo test --workspace` ; sur l'installation locale : `cargo test --workspace -- --ignored --nocapture`
- `cargo clippy --workspace --all-targets` (zéro avertissement), `cargo fmt --all`

Prérequis : `lua` (5.4) dans le `PATH`.

## Données
- Base : `~/.local/share/scout-rs/prix.db` — tables `items (item_id, name)` et
  `snapshots (item_id, source, ts, price, quantity)` ; `price` en cuivre, `ts` en secondes Unix.
- Réglages : `~/.config/scout-rs/config.toml` (dossier `_forever_`, détecté automatiquement ; thème).

Police : [Inter](https://github.com/rsms/inter) (SIL Open Font License 1.1, `app/assets/fonts/Inter-LICENSE.txt`),
intégrée au binaire.

Workspace : `scout-core` (logique pure) ← `scout-sources` (Lua, wago.tools) et `scout-db` (SQLite) ← `scout-worker` ← `app`.
Ancien nom : `scout-prix` ; ses dossiers de données sont déplacés automatiquement au premier lancement.
