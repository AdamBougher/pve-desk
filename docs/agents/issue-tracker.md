# Issue tracker: GitHub

Issues and specs for this repo live as GitHub issues. Use the `gh` CLI for all operations.

## Conventions

- **Create an issue**: `gh issue create --title "..." --body "..."`. Use a heredoc for multi-line bodies.
- **Read an issue**: `gh issue view <number> --json number,title,body,labels,comments`.
- **List issues**: `gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'` with appropriate `--label` and `--state` filters.
- **Make an issue a sub-issue of a parent**: `gh issue create --parent <parent> ...`, or `gh issue edit <parent> --add-sub-issue <child>` afterwards (`gh` 2.94+). Older `gh`: `gh api --method POST repos/<owner>/<repo>/issues/<parent>/sub_issues -F sub_issue_id=<child-db-id>` (database id, as in **Blocking** below). Without sub-issues, put `Part of #<parent>` at the top of the child body.
- **Comment on an issue**: `gh issue comment <number> --body "..."`
- **Apply / remove labels**: `gh issue edit <number> --add-label "..."` / `--remove-label "..."`
- **Close**: `gh issue close <number> --comment "..."`

Infer the repo from `git remote -v`; `gh` does this automatically when run inside a clone.

## Pull requests as a triage surface

**PRs as a request surface: no.** _(Set to `yes` if this repo treats external PRs as feature requests; `/triage` reads this flag.)_

When set to `yes`, PRs run through the same labels and states as issues, using the `gh pr` equivalents:

- **Read a PR**: `gh pr view <number> --comments` and `gh pr diff <number>` for the diff.
- **List external PRs for triage**: `gh api --paginate 'repos/{owner}/{repo}/pulls?state=open' --jq '.[] | select(.author_association | IN("OWNER","MEMBER","COLLABORATOR") | not) | {number, title, author: .user.login, author_association, labels: [.labels[].name]}'`.
- **Comment / label / close**: `gh pr comment`, `gh pr edit --add-label`/`--remove-label`, `gh pr close`.

GitHub shares one number space across issues and PRs, so a bare `#42` may be either: resolve with `gh pr view 42` and fall back to `gh issue view 42`.

## When a skill says "publish to the issue tracker"

Create a GitHub issue.

## When a skill says "fetch the relevant ticket"

Read it as in **Read an issue** above.

## Wayfinding operations

Used by `/wayfinder`. The **map** is a single issue with **child** issues as tickets.

- **Map**: a single issue labelled `wayfinder:map`, holding the Notes / Decisions-so-far / Fog body. `gh issue create --label wayfinder:map`.
- **Child ticket**: an issue linked to the map as a GitHub sub-issue (see **Make an issue a sub-issue of a parent**). Where sub-issues aren't enabled, add the child to a task list in the map body and put `Part of #<map>` at the top of the child body. Labels: `wayfinder:<type>` (`research`/`prototype`/`grilling`/`task`). Once claimed, the ticket is assigned to the driving dev.
- **Blocking**: GitHub's **native issue dependencies**, the canonical, UI-visible representation. Add an edge with `gh api --method POST repos/<owner>/<repo>/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-db-id>`, where `<blocker-db-id>` is the blocker's numeric **database id** (`gh api repos/<owner>/<repo>/issues/<n> --jq .id`, _not_ the `#number` or `node_id`). GitHub reports `issue_dependencies_summary.blocked_by` (open blockers only, the live gate). Where dependencies aren't available, fall back to a `Blocked by: #<n>, #<n>` line at the top of the child body. A ticket is unblocked when every blocker is closed.
- **Frontier query**: list the map's open children (`gh issue list --state open`, scoped to the map's sub-issues / task list), drop any with an open blocker (`issue_dependencies_summary.blocked_by > 0`, or an open issue in the `Blocked by` line) or an assignee; first in map order wins.
- **Claim**: `gh issue edit <n> --add-assignee @me`, the session's first write.
- **Resolve**: `gh issue comment <n> --body "<answer>"`, then `gh issue close <n>`, then append a context pointer (gist + link) to the map's Decisions-so-far.

---

## pve-desk conventions

Work for this repo is tracked as GitHub issues in `AdamBougher/pve-desk`. The
issue body **is** the spec, written as Gherkin, migrated out of the old
`ROADMAP.md`.

- **Title**: `F<n> — <feature name>`, e.g. `F12 — Packaging and release`. The
  F-number is permanent and is how features refer to each other ("F20 gates
  F12", "F35 makes the scenarios run"). It is not the issue number and the
  two will not match — always cite the F-number in prose and the `#number`
  only in GitHub links.
- **Body**: a ```gherkin fence holding the `Feature:` block verbatim — its
  narrative, its `#` open-question comments, and its `Scenario:` blocks with
  the measurement notes that say what was actually run. Scenarios are the
  acceptance criteria; they are documentation, not an executable suite
  (until F35 lands there is no `cucumber-rs` in the tree).
- **Labels** carry what the Gherkin tags used to: pass (`v1` `v2` `v3`
  `later`), state (`todo` `wip` `unverified` `blocked` `open`), qualifier
  (`perf` `linux` `windows`). See `triage-labels.md`.
- **Open questions** are the `#` comments in the body. Ask them **one at a
  time**, wait for each answer, then record the answer in `DECISIONS.md` and
  edit the issue body so the comment becomes the scenario it implies — or
  delete it if it decided nothing. This loop is mandatory; see *How to work
  on a feature* in `CLAUDE.md`.
- **Finishing a feature** means its scenarios were run by hand against the
  real node, not that it compiled. Then: move its decisions and dead ends to
  `DECISIONS.md`, update `README.md` if behaviour changed, and
  `gh issue close`. Anything still unrun moves to the `Shipped but unproven`
  issue as an `unverified` scenario instead of being dropped.
- **Don't re-propose from memory.** A closed issue may have been cut rather
  than built. Check `DECISIONS.md` first — several approaches in there were
  built, measured and removed.
