**SERVEUR**
[NOW - 1] Ajouter un cache pour soulager la BDD
[NOW - 2] Ajouter une page accessible par tous qui donne les sources de données utilisées "/thanks"
[LATER - 1] Retirer la clé d'accès du serveur
[VERY FUTURE - 1] Passer la BDD en BDD mysql-heatwave fourni par Oracle Cloud

**APP**
[NOW - 1] Remplacer la fenêtre source de donnée par ouvrir le lien créé sur le serveur "/thanks"
[NOW - 2] Ajouter une sélecteur dans les réglages comme pour la région pour choisir son mode de jeu Normal / PVP / etc ...
[NOW - 3] Retirer de l'affichage du sélecteur d'HDV sur la page principale la région et le mode de jeu du serveur car déjà visible dans les réglages
[NOW - 4] Retirer le champ adresse du serveur des réglages hors build dev
[LATER - 1] Retirer le champ de la clé d'accès du serveur des réglages (non utilisé depuis LATER - 1 de SERVEUR)
[LATER - 2] Pastille rouge lorsqu'une mise à jours des données WOW est nécessaire sur le bouton réglage.
[VERY FUTURE - 1] Lors du premier lancement de l'application, demander à l'utilisateur si pas auto trouvé son dossier wow, lui proposer d'installer l'addon et lui demander de relancer le jeu puis lui dire qu'on va télécharger les informations depuis le serveur pour mettre à jours les données WOW (et le faire)

**ADDON**
[LATER - 1] Refacto UI à réfléchir
[VERY FUTURE - 1] L'app permet d'update les données de l'addon : lorsque l'app est lancé elle peut pousser à l'addon les données sur les 30 derniers jours (celle qu'elle a déja en cache), jour par jour seulement. Lorsqu'elle doit faire ça (pas poussé de la journée sur l'addon), elle informe l'utilisateur via une popup, fais l'action puis indique qu'il faut / reload.
[VERY FUTURE - 2] Affichage d'une courbe de prix sur 30 jours depuis les données poussées par l'APP.
