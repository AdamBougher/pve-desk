# pve-desk

Linux desktop client for Proxmox VE. A launcher of saved connections (addable
and removable), Admin — the embedded Proxmox web UI — signed in and ready, and
a guest list with start / shutdown / stop on each row. SPICE is written but
unproven.

The words this project is opinionated about are in `GLOSSARY.md`. Use them:
a **guest** is a VM or a container, a **connection** is a saved login, a
**ticket** is the Proxmox credential and never a work item.

Where things live, each with one job. Read the issue and `DECISIONS.md`
before starting anything:

- **GitHub issues** — **outstanding work only**, one issue per feature,
  titled `F<n> — <name>`. The body is the spec, written as Gherkin:
  `Scenario:` blocks as its acceptance criteria and `#` comments carrying its
  open questions. Labels give the pass (`v1` `v2` `v3` `later`) and the state
  (`todo` `wip` `unverified` `blocked` `open`). The scenarios are the spec,
  not an executable suite. A finished feature is closed. Conventions:
  `docs/agents/issue-tracker.md`. `ROADMAP.md` is only the map of which
  issues are in which pass.
- `DECISIONS.md` — why the app is built this way, and **what was already
  tried and failed**. Check it before re-proposing something; several ideas
  in there were built, tested and removed.
- `README.md` — what the app does today, for someone reading the repo.
- `GLOSSARY.md` — the agreed term for each concept, and the words to avoid.

## Context

pve-desk is a desktop client for a Proxmox VE server on my own home network.
Every host, account, certificate and ticket referred to in this repo is my
own infrastructure. The **Hard rules** below exist to protect my credentials
from careless handling by this app: the password must never reach disk or
logs, TLS verification must stay on, and the frontend must never be able to
drive arbitrary API calls.

## Decisions already made (don't revisit without asking)

- Language: Rust. UI shell: **Tauri 2** (not Tauri 1 — the APIs differ;
  check any Tauri code or docs you use are for v2).
- Frontend: plain HTML/CSS/JS in `ui/`, no Node, no bundler, no framework.
  Backend is called via `window.__TAURI__.core.invoke`.
- Auth: my own Proxmox account, username + password against
  `/api2/json/access/ticket`. The password lives in the system keyring
  (`keyring` crate, Secret Service) and nowhere else — not in the repo, not
  in the config file. No SSH keys. The account signs in with a single factor,
  so the client can fetch a ticket without an interactive prompt.
- HTTP: `reqwest` with rustls (no OpenSSL).
- Target platform: **Linux first, and deliberately so** (decided 2026-10-21).
  It is the only platform verified by hand, and where the UI diverges, Linux
  wins — the tab bar is a GTK widget with no Windows or macOS equivalent
  (F14). Windows and macOS stay buildable and stay in F17, but are not worth
  compromising the Linux UI for until someone actually runs them. Keep
  non-UI code portable, put platform-specific work behind `cfg`, and say
  which platforms you actually tested.
- FOSS dependencies only.

## Layout

- `ui/` — frontend (`index.html`, `main.js`, `style.css`)
- `src-tauri/src/commands.rs` — every function the UI can call
- `src-tauri/src/api.rs` — Proxmox API client
- `src-tauri/src/config.rs` — `~/.config/pve-desk/config.toml` (no secrets)
- `src-tauri/src/secrets.rs` — keyring access
- `src-tauri/src/spice.rs` — `.vv` file + `remote-viewer`
- `src-tauri/src/error.rs` — shared error type, serialized as a string to the UI

## Commands

Run from `src-tauri/`:

- Check: `cargo check`
- Lint: `cargo clippy -- -D warnings`
- Format: `cargo fmt`
- Test: `cargo test`
- Run: `cargo run` (or `cargo tauri dev` if tauri-cli is installed)

Before saying any change is done: `cargo fmt`, `cargo clippy -- -D warnings`
and `cargo test` must pass with no warnings.

## Hard rules

- Never write the password to disk, logs, the config file, or error messages.
  It only goes to the keyring and the login request.
- TLS verification must stay enabled. There is no
  `danger_accept_invalid_certs` anywhere in the codebase any more and none
  may be added. Certificates are checked by the custom verifier in `api.rs`:
  valid against the OS trust store, or matching the pinned fingerprint.
  An unknown certificate must stop the connection and be shown to the user,
  never accepted silently.
- Power actions go through the allow-list in `Session::power`. Don't let the
  frontend pass arbitrary API paths.
- Text that comes from the server (VM names, node names, tags, errors) is set
  with `textContent`, never `innerHTML`.
- The `.vv` file contains a one-time ticket: keep it private (0600) and
  short-lived.
- New Tauri permissions in `capabilities/` must be the narrowest that works.
  The admin panel window loads a remote site; it must not get access to the
  app's commands.

## Proxmox API notes

- Every response is wrapped: `{ "data": ... }`.
- Login body is form-encoded, `username` is `user@realm`.
- Every request sends `Cookie: PVEAuthCookie=<ticket>`.
- POST/PUT/DELETE also send header `CSRFPreventionToken: <token>`. GET does not.
- Tickets expire after ~2 hours; re-login before then (plan: every ~90 min).
- Needed permissions: `VM.Audit` (list), `VM.PowerMgmt` (power),
  `VM.Console` (SPICE).
- pveproxy deliberately delays responses to **unauthenticated**
  `/api2/json/*` requests by ~3s. Two consequences for this app: a liveness
  probe aimed at an API endpoint takes 3s instead of milliseconds, and each
  one records a failed authentication in my node's log for no reason. So
  probe `/` for reachability — it needs no credentials, answers in ~8ms, and
  leaves the auth log clean. Authenticated calls with a valid ticket are not
  delayed. (Measured: `GET /api2/json/version` with no cookie = 3.005s / 401;
  `GET /` = 8ms / 200.)
- Official API reference: https://pve.proxmox.com/pve-docs/api-viewer/
- If unsure how an endpoint behaves, say so and check the API viewer rather
  than guessing.

## Reason before you code

Before writing or changing code, write out your reasoning in the reply:

1. **Goal:** what this change must do, in one or two sentences.
2. **Current state:** which files/functions are involved and what they do now
   (read them first; don't assume).
3. **Approach:** the steps you'll take, in order.
4. **Assumptions:** anything you're taking as true without having verified it.
   If an assumption is about architecture, stop and ask (next section).
5. **Risks:** what could break, and how you'll check it.

Only then edit files. After editing, compare the result against the goal and
say whether it was met, partly met, or not met.

## Architecture questions: ask, don't guess

If a decision isn't already covered by this file or `DECISIONS.md`, and it
would be hard to change later, **stop and ask me before implementing it.**

Counts as architecture (always ask):
- Adding a new crate/dependency, or replacing one
- New modules, or moving responsibilities between modules
- How state is stored or shared (files, keyring, in-memory, Tauri state)
- Anything touching auth, tickets, cookies, TLS or certificate handling
- New Tauri permissions/capabilities, or a new window/webview
- Data formats on disk (config schema changes, new files)
- Background tasks, threads, timers, or async runtime choices
- Choosing between two or more reasonable designs

Doesn't need asking: naming inside a function, small refactors within one
module, test structure, fixing a clear bug without changing behaviour.

How to ask:
- One question at a time.
- Give 2–3 concrete options, the trade-offs of each, and which one you'd pick
  and why.
- Say what you'll do while waiting (usually: nothing on that part).
- After I answer, record the decision in `DECISIONS.md` (the decision + the
  reason), so it isn't asked again.

If you're unsure whether something counts as architecture, ask.

## How to work on a feature

1. Read the feature's issue — `gh issue view <n>`. The scenarios are what it
   has to do, the `#` comments are what is still undecided. `ROADMAP.md` maps
   F-numbers to issue numbers; the F-number in the title is permanent and is
   not the issue number.
2. Ask its open questions **one at a time**. Wait for each answer before the
   next question. Don't stack questions whose answers depend on each other.
3. For each answer: record it in `DECISIONS.md` with its reasoning, and edit
   the issue body so the question's comment becomes the scenario it implies —
   or delete the comment if it decided nothing new.
4. Write the reasoning from "Reason before you code". Ask any architecture
   questions it raises.
5. Implement it. Keep changes scoped to that feature — make its scenarios
   true and don't invent scenarios it didn't have.
6. Run fmt / clippy / test.
7. Update the issue's state label (`todo` → `wip`, and `unverified` once the
   code is there but nobody has run the scenarios). One state label at a
   time; `gh issue edit <n> --add-label ... --remove-label ...`.
8. Say which scenarios you could NOT run (anything needing a live Proxmox
   node, a GUI click, or hardware you don't have).

### When it's finished

Don't leave a feature whose scenarios all pass sitting open. Finishing
includes retiring the issue, in the same commit as the code:

- **Close the issue** (`gh issue close <n> --comment "..."`), and delete its
  row from `ROADMAP.md`. Anything still open about it — a question you
  deferred, a scenario nobody ran — moves to the "Shipped but unproven"
  issue as an `unverified` scenario instead.
- **Move its decisions to `DECISIONS.md`**: the answers to its questions with
  the reasoning, and anything you tried that didn't work, with the
  measurement that killed it. Not a build log — `git log` has that.
- **Update `README.md`** if the feature changed what the app does.

"Finished" means its scenarios were run, not that it compiled. If they can
only be run against a live node you don't have, the issue stays open with the
`unverified` label and I'll tell you when I've confirmed it.

## Testing reality

Login, the VM list, power actions, SPICE and the admin panel can only be
truly verified against a real Proxmox node. If one isn't reachable, say the
feature is untested — don't call it working. Unit-test the pure parts
(`.vv` formatting, config round-trip, response parsing from sample JSON).

## Communication

- Reply in ASD-STE100 Simplified Technical English, using `GLOSSARY.md`'s
  vocabulary.
- Work step by step. When a task has several steps that need input or depend
  on the previous result, do one, report, then continue.
- No praise or flattery. State facts. If I'm wrong, say so directly.
- Be honest about uncertainty: say "I don't know" or "I haven't verified this".

## Agent skills

### Issue tracker

GitHub issues in `AdamBougher/pve-desk`. The issue body is the Gherkin spec
and the title carries the permanent `F<n>` number. See
`docs/agents/issue-tracker.md`.

### Triage labels

The five default triage labels, plus the old roadmap tags as labels — pass
(`v1`…`later`), state (`todo` `wip` `unverified` `blocked` `open`) and
qualifier (`perf` `linux` `windows`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context, and decisions live in `DECISIONS.md` rather than
`docs/adr/`. See `docs/agents/domain.md`.
