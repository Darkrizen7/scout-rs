# scout-rs

Outil de bureau (Rust, egui) compagnon de l'addon [Scout](https://github.com/Darkrizen7/scout-addon) sur
WoW Forever. Il lit des fichiers sur disque et télécharge des données publiques ; il ne touche jamais au jeu.

Quatre modules :
- **Objets** : les ~19 400 objets du jeu par catégorie. Fiche objet : prix HdV et graphique, recettes (produit
  par, utilisé dans, enseigne), marchands qui le vendent, **butin** (monstres, dépeçage, vol à la tire, coffres,
  conteneurs, quêtes ; chance de Forever Guide et chance réelle relevée par Scout, intervalle à 95 %).
- **Recettes** : recherche par recette, objet ou métier ; composants au moins cher (marchand ou HdV), bénéfice.
- **Butin** : carte cliquable des continents, zones colorées par niveau ; monstres d'une ou plusieurs zones
  (Ctrl+clic), recherche par monstre ou par objet, tri par niveau, valeur par kill ou valeur ÷ niveau ; butin
  détaillé de chaque monstre.
- **Market** : historique des prix de l'HdV, analyse (week-end, prix anormaux), rentabilité.

Un clic sur une position copie `/way #<carte> x y Nom` : colle-le dans le chat du jeu pour poser un point sur la
carte (commande de l'addon Scout, ou de TomTom s'il est installé).

## Sources
- **Scout** (⚙ Réglages → Importer depuis Scout, après un `/reload` en jeu) : relevés HdV, noms et catégories,
  marchands vus, butin enregistré (`tools/export.lua` de l'addon). La base garde tout, même ce que Scout oublie
  en jeu après 21 jours.
- **Forever Guide** : le butin (sources, chances, noms français) et la liste des marchands du paquet de référence
  du serveur en dérivent ; l'outil ne lit plus l'addon.
- **OpenMap** : contours des zones pour la carte (lus dans le dossier AddOns, jamais copiés).
- **wago.tools** et **Wowhead** : tables du client Forever en français (objets, recettes, métiers, cartes), téléchargées
  une fois par le serveur de prix et distribuées à l'outil.

## Commandes
- `cargo run --release`
- `cargo test --workspace` ; sur l'installation locale : `cargo test --workspace -- --ignored --nocapture`
- `cargo clippy --workspace --all-targets` (zéro avertissement), `cargo fmt --all`

Aucun prérequis : Lua 5.4 est intégré au binaire (`mlua`). Binaires Linux et Windows : page *Releases* du dépôt ;
l'outil propose ensuite lui-même les nouvelles versions (bouton « Mettre à jour » en haut à droite), et installe ou
met à jour l'addon Scout.

## Données
- Base SQLite : `~/.local/share/scout-rs/prix.db` (relevés HdV, données du jeu, marchands, butin ; détail des
  tables dans `CLAUDE.md`). Prix en cuivre, dates en secondes Unix.
- Rien n'est intégré à l'exécutable : au premier lancement, l'outil télécharge du serveur les données du jeu, puis
  le butin et les marchands de référence (aucun prix ni donnée personnelle), sans installer Forever Guide.
- Réglages : `~/.config/scout-rs/config.toml` (dossier `_forever_`, détecté automatiquement ; thème).

Police : [Inter](https://github.com/rsms/inter) (SIL Open Font License 1.1, `app/assets/fonts/Inter-LICENSE.txt`),
intégrée au binaire.

Licence : MIT (`LICENSE`) pour tout le code et le binaire. Seul le paquet de butin et de marchands du serveur,
`scout-server/data/reference.db.gz` (butin de Forever Guide, tiré de
[cmangos classic-db](https://github.com/cmangos/classic-db) et de QuestieDB), est GPL-3.0-or-later
(`LICENSE-GPL-3.0`) ; il n'est pas dans l'exécutable, l'outil le télécharge du serveur.

Workspace : `scout-core` (logique pure) ← `scout-gamedata` (données du jeu) ← `scout-sources` (Lua, serveur de prix) et
`scout-db` (SQLite) ← `scout-worker` ← `app` ; `scout-server` (serveur du VPS) ← `scout-gamedata`.
Ancien nom : `scout-prix` ; ses dossiers de données sont déplacés automatiquement au premier lancement.
