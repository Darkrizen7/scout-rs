---
name: pman
description: Traiter les tickets pman de scout-rs (workspace wow, projets APP, SERVER, ADDON, OUTILS) avec le CLI `pman` - choisir, prendre, spécifier, déléguer à des sous-agents Sonnet ou Haiku, relire, fusionner, relever les tokens, demander les validations humaines. À lancer quand l'utilisateur tape /pman (ex. `/pman`, `/pman APP`, `/pman APP-31`) ou demande de traiter un ticket.
---

# /pman : traiter les tickets

Argument : un ticket (`APP-31`), un projet (`APP`, `SERVER`, `ADDON`, `OUTILS`) ou rien (tous). C'est la seule
situation où Claude lance des sous-agents sans demande explicite. Les règles de `CLAUDE.md` (« Tickets
(pman) », « Commits ») s'appliquent.

## Le CLI (global, `pman --help` ; `--json` pour les sorties à analyser)
- Workspace **`wow`** : `PMAN_WORKSPACE=wow` vient de `.claude/settings.json` ; s'il manque, préfixer les commandes.
- Identité : bot **claude** (jeton dans le keyring) ; l'humain de référence est `admin` (`human` du `config.toml`).
  Ne jamais lire, afficher ni transmettre le jeton.
- Un ticket se désigne par sa clé `APP-31`. Les erreurs sortent en JSON sur stderr (`kind` : `not_found`,
  `conflict`, `assigned_to_human`, `blocked`, `not_eligible`, `not_mine`…) avec un `hint` ; code de sortie 6 =
  refus du workflow.
- Lecture : `task show KEY` (description, champs, bloqueurs, relations, commentaires),
  `task list --project APP --status todo`, `agent mine`, `agent next [--project APP]` (choisi + suivants + écartés
  avec la raison).
- Cycle d'un ticket : `agent claim KEY` (réserve, renvoie tout le contexte en un appel : ne pas relire le ticket
  ensuite) → `agent start KEY` (In progress, renvoie `started_at`) → `agent report KEY --file - --usage-since
  <started_at>` → `agent complete KEY` (Done) ou `agent request-validation KEY --title … --file -` (In review).
  `agent release KEY --reason …` rend un ticket impossible à traiter.
- Créer : `task create --project APP --label Feature|Fix --title … --description-file -` (la clé est attribuée) ;
  relier : `task relate KEY --blocks|--blocked-by|--relates KEY2`. Branche : `agent branch KEY`.

## 1. Choix du ticket (toujours par l'utilisateur)
- Argument = un ticket : le traiter (vérifier qu'il est éligible avec `agent claim`, qui refuse sinon).
- Sinon : `agent next [--project APP]`, puis une seule `AskUserQuestion` (`multiSelect: true`) avec les éligibles
  dans l'ordre (choisi puis suivants ; libellé = clé + titre court, description = priorité · catégorie et modèle
  prévus, ex. « High · Haiku, consigne directe ») ; au plus 4 options par question, 4 questions au maximum, sinon
  proposer les premiers et citer les autres dans le texte. Aucun coché : s'arrêter sans rien toucher.
- Pour classer, lire la description (`task show`) et quelques `grep -n` ciblés ; l'exploration lourde revient au sous-agent.
- Éligible = Todo, pas assigné à quelqu'un d'autre, bloquants fermés, pas de label Validation (calculé par le CLI).

## 2. Classement
| Catégorie | Quand | Exemples |
|---|---|---|
| **Haiku, consigne directe** | changement mécanique, bien délimité | retirer un champ des réglages |
| **Sonnet, écrit sa propre spec** | feature moyenne, un seul crate, suit un modèle existant | sélecteur de mode de jeu (comme la région) |
| **Claude écrit la spec, Sonnet code** | architecture, plusieurs crates, contrat avec l'addon | cache serveur, envoi des données vers l'addon |
| **Pas de délégation** | décision qui revient à l'utilisateur | poser la question (commentaire + AskUserQuestion) |
Un ticket minuscule : le faire soi-même coûte souvent moins qu'un sous-agent (qui relit `CLAUDE.md`).

## 3. Spec (catégories 2 et 3)
Une quinzaine de lignes dans le scratchpad (jamais dans le dépôt) : objectif, fichiers concernés, contraintes,
critères d'acceptation, tests attendus, ce qu'il ne faut pas toucher. En catégorie 2, le sous-agent l'écrit en tête
de son travail et la rappelle dans son rapport.

## 4. Délégation (séquentielle, un ticket à la fois)
1. `agent claim KEY`, puis `agent start KEY` (noter `started_at`).
2. Depuis `master` à jour : `git switch -c $(pman agent branch KEY)` (ex. `APP-31-selecteur-hdv`).
3. Agent `dev` (`model: haiku` ou `sonnet`) avec seulement : consigne ou spec, clé du ticket, dépôt
   (`../scout-addon` pour ADDON), fichiers connus, critères d'acceptation. Jamais toute la roadmap ni l'historique.
   Plusieurs petits tickets Haiku peuvent partir dans un seul agent (SendMessage après chaque changement de branche).

## 5. Relecture (Opus : rester sobre)
- `git diff --stat`, puis le diff ciblé, comparé à la spec. Pas de relecture complète des fichiers.
- Relancer soi-même clippy et les tests (ne pas se fier au rapport), et le test de plantage `target/debug` si l'UI a changé.
- Problème : renvoyer la remarque au **même** agent (SendMessage). Haiku qui bute deux fois : repasser à Sonnet.

## 6. Fin de ticket
- Commit par Claude sur la branche (sujet `APP-31 : …`, corps = pourquoi, attribution ; dates hors 9h–12h / 14h–18h
  en semaine : `--date` et `GIT_COMMITTER_DATE`). Pas de commit par le sous-agent.
- `git push -u origin <branche>`, `gh pr create --base master` (titre = sujet ; corps : résumé, comment tester,
  `pman : APP-31 (http://127.0.0.1:8090/wow/projects/APP/issues/APP-31)`). Jamais de `--force`.
- Relecture tracée : `gh pr comment` avec ce qui a été vérifié (diff vs spec, clippy, tests). GitHub refuse
  l'approbation de sa propre PR (même compte) : ce commentaire en tient lieu.
- Fusion par Claude (voir `CLAUDE.md`, « Commits ») : `gh pr view` (pas déjà fusionnée), `git switch master && git pull`,
  `git merge --no-ff origin/<branche>` daté hors plage (message `Fusion de la PR #N : <titre>`), clippy et tests,
  `git push origin master`, suppression de la branche (distante et locale).
- Synthèse et tokens en un appel : `agent report KEY --file - --usage-since <started_at>` (texte : fait, comment tester,
  limites, lien de la PR ; le CLI ajoute les lignes de tokens et de quota, le détail par sous-agent, et additionne
  `AI Tokens`, `AI Usage 5h %`, `AI Usage 7d %`). Correction après relecture : relevé complémentaire, il s'additionne.
- Sans validation humaine nécessaire : `agent complete KEY` (Done).
- Validation humaine nécessaire (UI, rendu, jeu réel, décision) : `agent request-validation KEY --title "Valider : …"
  --file -` avec les étapes précises, l'environnement et le résultat attendu (jamais « vérifier que tout marche ») :
  ticket au label Validation assigné à `admin`, relié au ticket de dev, qui passe en In review. Il n'est pas terminé
  tant que l'utilisateur n'a pas fermé la Validation (Done = accepté, Cancelled = refusé → remettre le ticket en
  Todo avec sa remarque). **Le dire aussi dans la conversation** : pourquoi, quoi vérifier, comment, résultat
  attendu, clé et lien de la Validation.
- Repartir de `master` à jour avant le ticket suivant ; d'autres tickets indépendants peuvent avancer pendant une validation.
- Travail découvert en route : `task create` (+ `task relate`), pas d'élargissement du périmètre.
- Blocage ou ambiguïté : `task comment KEY --file -` avec la question précise, `agent release KEY --reason …`, ticket suivant.

## 7. Bilan pour l'utilisateur
Par ticket : branche, lien de la PR, ce qui est fait, validations demandées (clé + lien), corrections après relecture.

## Limites
- L'économie vient de la boucle explorer / modifier / compiler sur un modèle moins cher ; Haiku décroche vite sur egui.
- Le quota (5 h / 7 j) couvre tout le compte : l'écart par ticket est approximatif. Une métrique absente reste vide.
- Personne ne voit le rendu : la validation humaine de l'UI reste indispensable.
