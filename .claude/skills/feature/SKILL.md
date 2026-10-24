---
name: feature
description: Work one pve-desk feature end to end, following the project's required loop — read its GitHub issue, ask its open questions one at a time, write the answers back, reason before coding, implement, run fmt/clippy/test, close the issue when it is done, and state what couldn't be verified. Invoke as /feature F5 (or with the feature name). Use this for any work framed as "do F4", "implement the VM list", "start on power controls".
---

# Work a feature

Argument: a feature id (`F5`) or a description that maps to one. Features are
GitHub issues titled `F<n> — <name>`; `ROADMAP.md` maps F-numbers to issue
numbers, and the two never match (F12 is issue #9). Find it with
`gh issue list --state open`. If it maps to no open issue, say so and ask —
don't invent a feature.

Work the steps in order. Several of them end in "wait" — honour that; this is a
one-step-at-a-time project.

## 1. Read

Read the feature's issue — `gh issue view <n>` — plus `DECISIONS.md`, which
records what was already tried and failed; re-proposing one of those is the
most likely way to waste the session. The issue body is Gherkin: the
`Scenario:` blocks are what it has to do, the `#` comments are what is still
undecided. Read the files the feature touches before describing them —
`CLAUDE.md` has the module map. Don't assume what a function does.

If the issue says it's blocked on another feature, or carries the `blocked`
label, say so now rather than at the end.

## 2. Ask the open questions — one at a time

Every feature entry has open questions. They are the user's call, not yours.

- Ask **one**, wait for the answer, then ask the next. Never stack questions
  whose answers depend on each other.
- Use `AskUserQuestion` with 2–3 concrete options when the question has
  discrete answers (it does, mostly); one question per call.
- If an answer makes a later question moot, drop it and say why.
- A question you can answer by reading the repo isn't an open question — answer
  it yourself and move on.

## 3. Write the answers back

Edit the issue body (`gh issue edit <n> --body-file ...`): replace each open
question `#` comment with the answer as a decision line, or with the scenario
it implies. Record the reasoning in `DECISIONS.md` too. This is what stops the
same question being asked twice. Do it before writing code, not after.

## 4. Reason before you code

Write out, in the reply (not in a file):

1. **Goal** — what this change must do, one or two sentences.
2. **Current state** — the files and functions involved and what they do now.
3. **Approach** — the ordered steps.
4. **Assumptions** — what you're taking as true without having verified it.
5. **Risks** — what could break and how you'll check.

If an assumption is architectural, stop here and ask. Architecture per
`CLAUDE.md`: new or swapped crates, new modules or moved responsibility, how
state is stored or shared, anything touching auth/tickets/cookies/TLS/certs, new
Tauri permissions or windows, on-disk data formats, background tasks or timers,
or a choice between two reasonable designs. One question, 2–3 options with
trade-offs, your pick and why, and what you'll do while waiting.

After the user answers, record it in `DECISIONS.md` with its reason.

## 5. Implement

Scope the change to this feature. No drive-by refactors, no starting the next
feature because it's adjacent.

Read `.claude/skills/tauri2/SKILL.md` before touching anything under `tauri::`,
`tauri.conf.json`, `capabilities/`, or `window.__TAURI__`.

The hard rules in `CLAUDE.md` are not negotiable and are easy to break by
accident. The ones that bite most often:

- Password goes to the keyring and the login request only — never to disk,
  logs, config, or an error string.
- `danger_accept_invalid_certs` appears nowhere in the tree and must not be
  added — not even temporarily, not behind a flag. TLS is checked by the
  custom verifier in `api.rs`: valid against the OS trust store, or matching
  the pinned fingerprint. An unknown certificate stops the connection and is
  shown to the user.
- Power actions stay behind the allow-list in `Session::power`; the frontend
  never passes an API path.
- Server-supplied text (VM names, node names, tags, errors) goes in via
  `textContent`. `ui/main.js` builds rows with `innerHTML` for the static cells
  and assigns server text to `.textContent` separately — keep that split.
- The `.vv` file is mode 0600 and short-lived.

## 6. Verify

From `src-tauri/`, all three must pass clean:

```
cargo fmt
cargo clippy -- -D warnings
cargo test
```

Unit-test the pure parts: `.vv` formatting, config round-trip, response parsing
from sample JSON. Don't write a test that needs a live node.

If `cargo clippy` fails for a reason unrelated to your change, say so instead of
fixing unrelated code.

## 7. Update status and report

- Move the issue's state label: `todo` → `wip`, with
  `gh issue edit <n> --add-label wip --remove-label todo`. One state label at
  a time. If it only compiles and can't be exercised without a live Proxmox
  node, use `unverified` and stop there until the user confirms it.
- **Once it is confirmed working, retire it in the same commit:**
  `gh issue close <n> --comment "..."`, delete its row from `ROADMAP.md`, move
  its decisions and dead ends to `DECISIONS.md`, push anything still unrun to
  the "Shipped but unproven" issue (#30) as an `unverified` scenario, and
  update `README.md` if the app now does something new. Open issues are
  outstanding work and nothing else — never a finished feature left open.
- Compare the result against the Goal from step 4 and say: met, partly met, or
  not met.
- State plainly what you could **not** verify. Login, the guest list, power
  actions, SPICE and the admin panel can only be truly verified against a real
  node. `remote-viewer` is also not installed on this machine (no `virt-viewer`
  package), so F7 can't be exercised locally even with a node reachable.

Don't call a feature working because it compiles.
