---
name: pman
description: Handle the repository's pman tickets with the `pman` CLI - pick, claim, specify, delegate to Sonnet or Haiku subagents, review, deliver, record tokens, request human validations. Workspace, projects and repository rules are in its CLAUDE.md. Run when the user types /pman (e.g. `/pman`, `/pman APP`, `/pman APP-31`) or asks to handle a ticket.
---

# /pman: handle tickets

Argument: a ticket (`APP-31`), a project (`APP`) or nothing (all the repository's projects). This is the only
situation where Claude launches subagents without an explicit request.

**Nothing repository-specific in this skill.** The repository's `CLAUDE.md` gives: workspace and project keys
("Tickets (pman)" section, block written by `pman init`), check commands (clippy, tests, build, lint…),
commit, PR and merge rules, prohibitions. Its rules take precedence over this skill; read it before starting.
Skill installed and updated by `pman init` (pman repository): do not edit it here, otherwise `pman init` keeps it
without updating it.

## The CLI (global, `pman --help`; `--json` for outputs to parse)
- Workspace: `PMAN_WORKSPACE` comes from `.claude/settings.json`; if it is missing, prefix the commands
  (`PMAN_WORKSPACE=<slug> pman …`, slug given by CLAUDE.md).
- Identity: bot **claude** (token in the keyring); the reference human is `human` from
  `~/.config/pman/config.toml`. Never read, display or forward the token.
- A ticket is designated by its key `APP-31`. Errors come out as JSON on stderr (`kind`: `not_found`,
  `conflict`, `assigned_to_human`, `blocked`, `not_eligible`, `not_mine`…) with a `hint`; exit code 6 =
  workflow refusal.
- Reading: `task show KEY` (description, fields, blockers, relations, comments),
  `task list --project APP --status todo`, `agent mine`, `agent next [--project APP]` (chosen + next ones + skipped
  with the reason).
- Ticket cycle: `agent claim KEY` (reserves, returns the whole context in one call: do not re-read the ticket
  afterwards) → `agent start KEY` (In progress, returns `started_at`) → `agent report KEY --file - --usage-since
  <started_at>` → `agent complete KEY` (Done), `agent complete KEY --review` (In review, to the human) or
  `agent request-validation KEY --title … --file -` (human validation). `agent release KEY --reason …` gives back a ticket that cannot be handled.
- Create: `task create --project APP --label Feature|Fix --title … --description-file -` (the key is assigned);
  link: `task relate KEY --blocks|--blocked-by|--relates KEY2`. Branch: `agent branch KEY`.

## 1. Choosing the ticket (always by the user)
- Argument = a ticket: handle it (check that it is eligible with `agent claim`, which refuses otherwise).
- Otherwise: `agent next [--project APP]` (without argument: each project of CLAUDE.md), then a single
  `AskUserQuestion` (`multiSelect: true`) with the eligible ones in order (chosen then next ones; label = key +
  short title, description = priority · category and planned model, e.g. "High · Haiku, direct instruction"); at
  most 4 options per question, 4 questions at most, otherwise offer the first ones and name the others in the
  text. None ticked: stop without touching anything.
- To classify, read the description (`task show`) and a few targeted `grep -n`; heavy exploration goes to the subagent.
- Eligible = Todo, not assigned to someone else, blockers closed, no Validation label (old validation tickets; computed by the CLI).

## 2. Classification
| Category | When | Examples |
|---|---|---|
| **Haiku, direct instruction** | mechanical, well-delimited change | remove a field from a form |
| **Sonnet, writes its own spec** | medium feature, a single module, follows an existing pattern | new filter modelled on an existing filter |
| **Claude writes the spec, Sonnet codes** | architecture, several modules or crates, contract between components | cache, new exchange format |
| **No delegation** | decision that belongs to the user | ask the question (comment + AskUserQuestion) |
A tiny ticket: doing it yourself often costs less than a subagent (which re-reads `CLAUDE.md`).

## 3. Spec (categories 2 and 3)
About fifteen lines in the scratchpad (never in the repository): goal, files concerned, constraints,
acceptance criteria, expected tests, what must not be touched. In category 2, the subagent writes it at the start
of its work and recalls it in its report. The check commands (format, lint, tests, build) are
**copied from the repository's CLAUDE.md**, never taken from another repository nor assumed (a `cargo fmt` run on an
unformatted repository rewrites dozens of files out of scope).

## 4. Delegation (sequential, one ticket at a time)
1. `agent claim KEY`, then `agent start KEY` (note `started_at`).
2. From the up-to-date main branch: `git switch -c $(pman agent branch KEY)` (e.g. `APP-31-mode-selector`),
   unless CLAUDE.md says to work otherwise.
3. `dev` agent (`.claude/agents/dev.md`, `model: haiku` or `sonnet`) with only: instruction or spec, ticket
   key, repository (another repository if CLAUDE.md attaches the project to it), known files, acceptance criteria.
   Never the whole roadmap nor the history. Several small Haiku tickets can go to a single agent
   (SendMessage after each branch change).

## 5. Review (Opus: stay sober)
- `git diff --stat`, then the targeted diff, compared with the spec. No full-file review.
- Re-run CLAUDE.md's checks yourself, exactly as it writes them (do not trust the report).
- `git status --short` lists only files within the scope; otherwise, understand why before delivering.
- Problem: send the remark back to the **same** agent (SendMessage). Haiku stuck twice: switch back to Sonnet.

## 6. End of ticket
- Delivery according to CLAUDE.md (commits, PR, merge, dates). By default, if it says nothing: commit by Claude on
  the branch (subject `APP-31: …`, body = why, attribution), `git push -u origin <branch>`,
  `gh pr create` towards the main branch (body: summary, how to test, `pman: APP-31 (<ticket link>)`),
  review traced as `gh pr comment` (diff vs spec, checks); merge only if CLAUDE.md entrusts it
  to Claude. Never `--force`. No commit by the subagent.
- If CLAUDE.md forbids commits by Claude: leave the changes uncommitted, summarize them in the
  report, and finish with `agent complete KEY --review` (the human reviews and commits).
- Summary and tokens in one call: `agent report KEY --file - --usage-since <started_at>` (text: what was done, how to
  test, limits, PR link if there is one; the CLI adds the token and quota lines, the per-subagent detail,
  and adds up `AI Tokens`, `AI Usage 5h %`, `AI Usage 7d %`). Fix after review: additional
  report, it adds up.
- No human validation needed: `agent complete KEY` (Done), or `--review` as above.
- Human validation needed (UI, rendering, real usage, decision): first prepare the test environment (build,
  server restarted, per CLAUDE.md), then `agent request-validation KEY --title "…" --file -` with the precise
  steps, the environment and the expected result (never "check that everything works"): comment on the ticket
  that mentions the human (notification), ticket in In review assigned to the human. No separate ticket. The human
  accepts by moving the ticket to **Done**, refuses by putting it back in **Todo** with their remark (then resume
  the ticket). Before delivering changes left awaiting validation, check with `task show KEY` that
  the ticket is Done. **Also say it in the conversation**: why, what to check, how, expected result,
  ticket link.
- Start again from the up-to-date main branch before the next ticket; other independent tickets can progress
  during a validation.
- Work discovered along the way: `task create` (+ `task relate`), no widening of the scope.
- Blocker or ambiguity: `task comment KEY --file -` with the precise question, `agent release KEY --reason …`, next ticket.

## 7. Summary for the user
Per ticket: branch, PR link (or uncommitted modified files), what is done, validations requested
(key + link), corrections after review.

## Limits
- The saving comes from the explore / modify / compile loop on a cheaper model.
- The quota (5 h / 7 d) covers the whole account: the per-ticket gap is approximate. A missing metric stays empty.
- Nobody sees the rendering: human validation of the UI remains essential.
<!-- pman init: 5bd86e769c7db27a -->
