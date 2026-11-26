---
name: pman
description: Traiter les tickets pman du dépôt avec le CLI `pman` - choisir, prendre, spécifier, déléguer à des sous-agents Sonnet ou Haiku, relire, livrer, relever les tokens, demander les validations humaines. Workspace, projets et règles du dépôt sont dans son CLAUDE.md. À lancer quand l'utilisateur tape /pman (ex. `/pman`, `/pman APP`, `/pman APP-31`) ou demande de traiter un ticket.
---

# /pman : traiter les tickets

Argument : un ticket (`APP-31`), un projet (`APP`) ou rien (tous les projets du dépôt). C'est la seule situation
où Claude lance des sous-agents sans demande explicite.

**Rien de propre au dépôt dans cette skill.** Le `CLAUDE.md` du dépôt donne : workspace et clés de projet
(section « Tickets (pman) », bloc écrit par `pman init`), commandes de vérification (clippy, tests, build, lint…),
règles de commit, de PR et de fusion, interdits. Ses règles priment sur cette skill ; le lire avant de commencer.
Skill installée et mise à jour par `pman init` (dépôt pman) : ne pas l'éditer ici, sinon `pman init` la conserve
sans la mettre à jour.

## Le CLI (global, `pman --help` ; `--json` pour les sorties à analyser)
- Workspace : `PMAN_WORKSPACE` vient de `.claude/settings.json` ; s'il manque, préfixer les commandes
  (`PMAN_WORKSPACE=<slug> pman …`, slug donné par le CLAUDE.md).
- Identité : bot **claude** (jeton dans le keyring) ; l'humain de référence est `human` du
  `~/.config/pman/config.toml`. Ne jamais lire, afficher ni transmettre le jeton.
- Un ticket se désigne par sa clé `APP-31`. Les erreurs sortent en JSON sur stderr (`kind` : `not_found`,
  `conflict`, `assigned_to_human`, `blocked`, `not_eligible`, `not_mine`…) avec un `hint` ; code de sortie 6 =
  refus du workflow.
- Lecture : `task show KEY` (description, champs, bloqueurs, relations, commentaires),
  `task list --project APP --status todo`, `agent mine`, `agent next [--project APP]` (choisi + suivants + écartés
  avec la raison).
- Cycle d'un ticket : `agent claim KEY` (réserve, renvoie tout le contexte en un appel : ne pas relire le ticket
  ensuite) → `agent start KEY` (In progress, renvoie `started_at`) → `agent report KEY --file - --usage-since
  <started_at>` → `agent complete KEY` (Done), `agent complete KEY --review` (In review, à l'humain) ou
  `agent request-validation KEY --title … --file -`. `agent release KEY --reason …` rend un ticket impossible à traiter.
- Créer : `task create --project APP --label Feature|Fix --title … --description-file -` (la clé est attribuée) ;
  relier : `task relate KEY --blocks|--blocked-by|--relates KEY2`. Branche : `agent branch KEY`.

## 1. Choix du ticket (toujours par l'utilisateur)
- Argument = un ticket : le traiter (vérifier qu'il est éligible avec `agent claim`, qui refuse sinon).
- Sinon : `agent next [--project APP]` (sans argument : chaque projet du CLAUDE.md), puis une seule
  `AskUserQuestion` (`multiSelect: true`) avec les éligibles dans l'ordre (choisi puis suivants ; libellé = clé +
  titre court, description = priorité · catégorie et modèle prévus, ex. « High · Haiku, consigne directe ») ; au
  plus 4 options par question, 4 questions au maximum, sinon proposer les premiers et citer les autres dans le
  texte. Aucun coché : s'arrêter sans rien toucher.
- Pour classer, lire la description (`task show`) et quelques `grep -n` ciblés ; l'exploration lourde revient au sous-agent.
- Éligible = Todo, pas assigné à quelqu'un d'autre, bloquants fermés, pas de label Validation (calculé par le CLI).

## 2. Classement
| Catégorie | Quand | Exemples |
|---|---|---|
| **Haiku, consigne directe** | changement mécanique, bien délimité | retirer un champ d'un formulaire |
| **Sonnet, écrit sa propre spec** | feature moyenne, un seul module, suit un modèle existant | nouveau filtre calqué sur un filtre existant |
| **Claude écrit la spec, Sonnet code** | architecture, plusieurs modules ou crates, contrat entre composants | cache, nouveau format d'échange |
| **Pas de délégation** | décision qui revient à l'utilisateur | poser la question (commentaire + AskUserQuestion) |
Un ticket minuscule : le faire soi-même coûte souvent moins qu'un sous-agent (qui relit `CLAUDE.md`).

## 3. Spec (catégories 2 et 3)
Une quinzaine de lignes dans le scratchpad (jamais dans le dépôt) : objectif, fichiers concernés, contraintes,
critères d'acceptation, tests attendus, ce qu'il ne faut pas toucher. En catégorie 2, le sous-agent l'écrit en tête
de son travail et la rappelle dans son rapport. Les commandes de vérification (format, lint, tests, build) sont
**copiées du CLAUDE.md du dépôt**, jamais reprises d'un autre dépôt ni supposées (un `cargo fmt` lancé sur un dépôt
non formaté réécrit des dizaines de fichiers hors périmètre).

## 4. Délégation (séquentielle, un ticket à la fois)
1. `agent claim KEY`, puis `agent start KEY` (noter `started_at`).
2. Depuis la branche principale à jour : `git switch -c $(pman agent branch KEY)` (ex. `APP-31-selecteur-mode`),
   sauf si le CLAUDE.md dit de travailler autrement.
3. Agent `dev` (`.claude/agents/dev.md`, `model: haiku` ou `sonnet`) avec seulement : consigne ou spec, clé du
   ticket, dépôt (un autre dépôt si le CLAUDE.md y rattache le projet), fichiers connus, critères d'acceptation.
   Jamais toute la roadmap ni l'historique. Plusieurs petits tickets Haiku peuvent partir dans un seul agent
   (SendMessage après chaque changement de branche).

## 5. Relecture (Opus : rester sobre)
- `git diff --stat`, puis le diff ciblé, comparé à la spec. Pas de relecture complète des fichiers.
- Relancer soi-même les vérifications du CLAUDE.md, telles qu'il les écrit (ne pas se fier au rapport).
- `git status --short` ne liste que des fichiers du périmètre ; sinon, comprendre pourquoi avant de livrer.
- Problème : renvoyer la remarque au **même** agent (SendMessage). Haiku qui bute deux fois : repasser à Sonnet.

## 6. Fin de ticket
- Livraison selon le CLAUDE.md (commits, PR, fusion, dates). Par défaut, s'il n'en dit rien : commit par Claude sur
  la branche (sujet `APP-31 : …`, corps = pourquoi, attribution), `git push -u origin <branche>`,
  `gh pr create` vers la branche principale (corps : résumé, comment tester, `pman : APP-31 (<lien du ticket>)`),
  relecture tracée en `gh pr comment` (diff vs spec, vérifications) ; la fusion seulement si le CLAUDE.md la confie
  à Claude. Jamais de `--force`. Pas de commit par le sous-agent.
- Si le CLAUDE.md interdit les commits à Claude : laisser les changements non commités, les résumer dans le
  rapport, et terminer par `agent complete KEY --review` (l'humain relit et commite).
- Synthèse et tokens en un appel : `agent report KEY --file - --usage-since <started_at>` (texte : fait, comment
  tester, limites, lien de la PR s'il y en a une ; le CLI ajoute les lignes de tokens et de quota, le détail par
  sous-agent, et additionne `AI Tokens`, `AI Usage 5h %`, `AI Usage 7d %`). Correction après relecture : relevé
  complémentaire, il s'additionne.
- Sans validation humaine nécessaire : `agent complete KEY` (Done), ou `--review` comme ci-dessus.
- Validation humaine nécessaire (UI, rendu, usage réel, décision) : `agent request-validation KEY --title "Valider : …"
  --file -` avec les étapes précises, l'environnement et le résultat attendu (jamais « vérifier que tout marche ») :
  ticket au label Validation assigné à l'humain, relié au ticket de dev, qui passe en In review. Il n'est pas
  terminé tant que l'utilisateur n'a pas fermé la Validation (Done = accepté, Cancelled = refusé → remettre le
  ticket en Todo avec sa remarque). **Le dire aussi dans la conversation** : pourquoi, quoi vérifier, comment,
  résultat attendu, clé et lien de la Validation.
- Repartir de la branche principale à jour avant le ticket suivant ; d'autres tickets indépendants peuvent avancer
  pendant une validation.
- Travail découvert en route : `task create` (+ `task relate`), pas d'élargissement du périmètre.
- Blocage ou ambiguïté : `task comment KEY --file -` avec la question précise, `agent release KEY --reason …`, ticket suivant.

## 7. Bilan pour l'utilisateur
Par ticket : branche, lien de la PR (ou fichiers modifiés non commités), ce qui est fait, validations demandées
(clé + lien), corrections après relecture.

## Limites
- L'économie vient de la boucle explorer / modifier / compiler sur un modèle moins cher.
- Le quota (5 h / 7 j) couvre tout le compte : l'écart par ticket est approximatif. Une métrique absente reste vide.
- Personne ne voit le rendu : la validation humaine de l'UI reste indispensable.
<!-- pman init: af9107d65b0c9b7e -->
