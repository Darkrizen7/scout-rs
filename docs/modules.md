# Modules (onglets en haut)

- **Market** : historique HdV. Barre latérale façon Fichiers (tous les objets, favoris, Analyse, Rentabilité,
  catégories dépliables), liste, graphique prix / quantité (une ligne par jour : Scout seul, sinon moyenne Auctionator + Forever Guide ; source au survol), calculateur de rentabilité ; l'analyse n'est compilée qu'avec `--features analysis` (désactivée par défaut).
- **Objets** : les ~19 400 objets du jeu par catégorie (icône de catégorie aussi dans le tableau), avec prix HdV /
  achat PNJ / revente. À l'arrivée, seule la recherche, au milieu de la page (`widgets::landing_search`) ; le
  tableau s'affiche dès qu'on tape ou qu'on choisit une catégorie. Une **fiche objet**
  à onglets : Market (prix et graphique), Recettes (produit par, utilisé dans, enseigne), PNJ (où l'acheter),
  Butin (monstres, dépeçage, vol à la tire, coffres, conteneurs, quêtes : chance de Forever Guide et chance
  réelle enregistrée par Scout, avec intervalle à 95 %).
- **Butin** : par zone de la carte. **Carte cliquable** des deux continents côte à côte, Kalimdor à gauche (zones à leur place, contour d'OpenMap,
  couleur selon le niveau habituel des monstres ; affichée tant qu'aucune zone n'est choisie, bouton « 🗺 Carte »).
  Barre latérale en dossiers repliables comme les catégories d'objets : **Favoris** (clic droit sur une zone),
  un dossier par continent (Kalimdor d'abord, zones triées par nom, pastille bleue Alliance / rouge Horde après
  le nom, rien si contestée), puis donjons et raids (recherche). **Ctrl+clic** (liste ou carte) ajoute une zone :
  les monstres de toutes les zones choisies sont listés ensemble (`ZoneLoot::merge`). Liste des monstres :
  recherche par nom **ou par objet lâché**, masquer élites / rares / boss, tri par niveau, valeur estimée par kill
  ou **valeur ÷ niveau** (clic : un sens, l'autre, puis ordre d'origine). Une ligne = nom, niveau avec icône
  élite / rare, icône dépeçable, prix. Butin du monstre choisi (butin, dépeçage, vol à la tire : chance Forever Guide, chance
  réelle, prix). Zone d'un monstre = son donjon, sinon sa zone Forever Guide ; un monstre que seul Scout connaît va
  dans la zone où il a été fouillé. L'île de Zéphras (Forever) n'est ni dans Forever Guide ni sur un continent :
  rien sur la carte, son butin ne viendra que de Scout.
- **Positions cliquables** (fiche objet : Butin et PNJ ; module Butin) : copient `/way #<UiMap> x y Nom` (syntaxe
  TomTom ; l'addon Scout fournit `/way` quand TomTom est absent) à coller dans le chat du jeu. L'outil ne fait que
  remplir le presse-papiers : c'est le joueur qui colle. Forever Guide ne donne qu'un point par monstre (pas toutes
  ses apparitions).
- **Recettes** : recherche d'une recette ou d'un objet, par métier (même page d'arrivée que Objets, recherche puis
  résultats avec l'icône du métier ou de la catégorie) ; fiche recette (barre de compétence orange / jaune / vert / gris de 0 à 300 sur la ligne du métier, sections séparées, composants en tableau bordé avec ligne Total
  au moins cher entre PNJ et HdV, ou au prix de son craft quand le même métier le fabrique pour moins cher,
  étiquette « Craft » ; bénéfice, patron). Un objet cliqué ouvre sa fiche dans Objets (`State::show_item`), une recette
  cliquée depuis une fiche objet s'ouvre dans Recettes (`State::show_recipe`).

## Fichiers de l'UI (`app/src/ui/`)
`header` (onglets, en-tête Market, barre d'état, sélecteur d'hôtel des ventes, fenêtre Réglages ; `settings/price_server` : serveur de prix, région, HdV), `sidebar` (barre latérale Market ; `sidebar/entry` : lignes et
icônes de catégories partagées par les barres latérales), `item_list`, `chart` (`plots` réutilisé par la fiche objet, qui n'a pas les boutons d'agrégation Détaillé / Journalier / Hebdo / Mensuel du Market : tous les relevés, un point par jour, moyenne par semaine ou par mois, avec la fenêtre de temps correspondante ; sur le graphe du Market le titre mène à la fiche objet ; `chart/money_axis` : axe des prix avec pièces ; `chart/limits` : déplacement et zoom des graphes, bornés aux jours de données et à 0, avec effet élastique, axe du temps partagé et bouton « Aujourd'hui »), `analysis_view` (+ `charts`),
`calculator`, `credits` (fenêtre « Sources des données »), `items` (+ `items/{list,rows,sidebar,sheet,merchants,drops}`),
`loot` (+ `loot/{map,map/paint,npcs,npc,sidebar}`), `recipes` (+ `recipes/{results,detail,reagents,skill_bar}`), `widgets` (étoile de favori,
couleurs de qualité, champ de recherche et page d'arrivée, prix ou tiret, chance et chance réelle : partagés ; `widgets/house` : sélecteur d'hôtel des ventes de l'en-tête et des Réglages),
`theme` (palettes libadwaita), `fonts` (Inter), `coins` (pièces dessinées).

## Pistes
- Butin : `ForeverGuide/ForeverBossLoot.lua` (butin confirmé de boss Forever, chances inconnues, appliqué par
  l'addon à l'exécution) n'est pas lu.
- Carte : points des monstres de la zone (coordonnées Forever Guide `x`, `y` sur `ui_map`), entrées des donjons
  (`OpenMap/Data/Forever/Places.lua`, coordonnées monde), fond peint (`MapZones` d'OpenMap, BLP).
- Toutes les apparitions d'un monstre : QuestieDB (`QuestieDB_Forever`) les a, mais dans un format codé
  (`src/meta/codec.lua`) pas encore lu.
