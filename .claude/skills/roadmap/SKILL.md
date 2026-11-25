---
name: roadmap
description: Traiter les tickets Plane de scout-rs (projets APP, SERVER, ADDON) en déléguant le code à des sous-agents Sonnet ou Haiku, puis en relisant leur travail. À lancer seulement quand l'utilisateur tape /roadmap (ex. `/roadmap`, `/roadmap APP`, `/roadmap APP-3`).
---

# /roadmap : traiter les tickets Plane

Argument : un projet (`APP`, `SERVER`, `ADDON`), un ticket (`APP-3`) ou rien (tous les projets).
C'est la seule situation où Claude lance des sous-agents sans demande explicite. Les règles de
`CLAUDE.md` (section « Tickets (Plane) ») s'appliquent : états, branche par ticket, commentaires.

## 1. Tri rapide
- `workitem list` sur le ou les projets (pql sur l'état « Todo »), puis `workitem_relation list` pour les
  candidats. Un ticket est prenable s'il est en « Todo », sans assigné autre que l'utilisateur, et si tous ses
  « blocked_by » sont « Done ». Ordre : priorité (urgent > high > medium > low), puis dépendances.
- Lire la description et les commentaires des tickets retenus, puis quelques `grep -n` ciblés dans le code.
  Pas d'exploration lourde : c'est le sous-agent qui explore.

## 2. Classement de chaque ticket
| Catégorie | Quand | Exemples |
|---|---|---|
| **Haiku, consigne directe** | changement mécanique, bien délimité | retirer un champ des réglages, retirer région et mode du sélecteur d'HdV |
| **Sonnet, écrit sa propre spec** | feature moyenne, un seul crate, suit un modèle existant | sélecteur de mode de jeu (comme celui de la région), page `/thanks` |
| **Claude écrit la spec, Sonnet code** | architecture, plusieurs crates, contrat avec l'addon | cache serveur, premier lancement, envoi des données de l'app vers l'addon |
| **Pas de délégation** | décision qui revient à l'utilisateur | passer à MySQL HeatWave : poser la question (commentaire + AskUserQuestion) |

Annoncer le classement à l'utilisateur en quelques lignes avant de lancer.

## 3. Spec (catégories 2 et 3)
Une quinzaine de lignes, dans le scratchpad (jamais dans le dépôt) : objectif, fichiers concernés,
contraintes, critères d'acceptation, ce qu'il ne faut pas toucher. En catégorie 2, la consigne demande au
sous-agent d'écrire cette spec lui-même en tête de son travail et de la rappeler dans son rapport.

## 4. Délégation (séquentielle)
Pour chaque ticket, dans l'ordre :
1. Passer le ticket en « In Progress » et l'assigner à l'utilisateur (`member list_workspace` pour l'id).
2. Depuis `master` à jour : `git switch -c <ID>-<description-courte>` (ex. `APP-3-selecteur-hdv`).
3. Lancer l'agent `dev` (`model: haiku` ou `sonnet` selon le classement) avec la consigne ou la spec,
   l'identifiant du ticket et le dépôt (`../scout-addon` pour ADDON).
Plusieurs petits tickets Haiku peuvent partir dans un seul agent, mais chacun garde sa branche : dans ce cas,
les traiter l'un après l'autre avec SendMessage au même agent après chaque changement de branche.
Pas de parallèle : beaucoup de tickets touchent les mêmes fichiers (réglages).

## 5. Relecture (Opus : rester sobre)
- `git diff --stat`, puis le diff ciblé, comparé à la spec. Pas de relecture complète des fichiers.
- Relancer soi-même clippy et les tests (ne pas se fier au rapport), et le test de plantage `target/debug`
  si l'UI a changé.
- Problème : renvoyer la remarque au **même** agent avec SendMessage (il a gardé son contexte).
- Haiku qui bute deux fois : repasser le ticket à Sonnet.

## 6. Fin de ticket
- Relecture OK : Claude commite sur la branche du ticket (sujet `APP-42 : …` en français, corps = pourquoi,
  lignes d'attribution ; vérifier `date` : en semaine 9h–12h / 14h–18h, dater juste après la plage avec
  `--date` et `GIT_COMMITTER_DATE`). Pas de commit par le sous-agent.
- `git push -u origin <branche>`, puis `gh pr create --base master` (titre = sujet du commit, corps : résumé,
  comment tester, `Plane : APP-42`). Jamais de `--force`.
- Commentaire de synthèse sur le ticket : ce qui a été fait, comment tester, limites, lien de la PR.
- Tokens : lancer `python3 -I .claude/skills/roadmap/tokens.py --since <passage en « In Progress »>`
  (horodatage dans `updated_at` du `workitem update` qui a posé l'état, ou dans l'activité Plane) et coller les
  deux lignes (tokens, puis « Plan : +x % sur 5 h, +y % sur 7 j ») dans le commentaire de synthèse. Si Claude corrige après la relecture : relevé complémentaire avec
  `--since` = fin du premier relevé.
- Fusion par Claude (voir `CLAUDE.md`, « Commits ») : `gh pr view` pour vérifier qu'elle n'est pas déjà fusionnée,
  `git switch master && git pull`, `git merge --no-ff origin/<branche>` daté hors plage (`GIT_AUTHOR_DATE`,
  `GIT_COMMITTER_DATE`, message `Fusion de la PR #N : <titre>`), clippy et tests, `git push origin master`.
- Ticket sans UI : « Done ». Ticket qui change l'interface : « In Review », assigné à l'utilisateur, qui teste et
  passe en « Done » (ses dépendants attendent).
- Repartir de `master` à jour avant le ticket suivant.
- Travail découvert en route : nouveau ticket (avec relation si pertinent), pas d'élargissement du périmètre.
- Blocage ou ambiguïté : commentaire avec la question précise sur le ticket, puis ticket suivant.

## 7. Bilan pour l'utilisateur
Par ticket : branche, lien de la PR, ce qui est fait, ce qu'il faut tester dans l'UI, ce que Claude a corrigé après relecture.

## Limites à garder en tête
- Un sous-agent repart de zéro (relit `CLAUDE.md` et les fichiers) : pour un ticket minuscule, le faire
  soi-même peut coûter moins cher. D'où le regroupement des petits tickets.
- L'économie vient de la boucle explorer / modifier / compiler / corriger sur un modèle moins cher.
- Haiku décroche vite sur egui (emprunts dans les closures, API 0.37).
- Personne ne voit le rendu : le test de l'UI par l'utilisateur reste indispensable.
