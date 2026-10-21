# pve-desk

Linux desktop client for Proxmox VE. Saved login + auto-login, embedded
Proxmox web UI (admin panel), VM/container list, power controls, SPICE launch.

`FEATURES.md` is the source of truth for scope, status and open questions.
Read it before starting any feature.

## Decisions already made (don't revisit without asking)

- Language: Rust. UI shell: **Tauri 2** (not Tauri 1 — the APIs differ;
  check any Tauri code or docs you use are for v2).
- Frontend: plain HTML/CSS/JS in `ui/`, no Node, no bundler, no framework.
  Backend is called via `window.__TAURI__.core.invoke`.
- Auth: username + password against `/api2/json/access/ticket`. Password
  lives in the system keyring (`keyring` crate, Secret Service). No SSH keys.
  No 2FA on the account, so login is fully automatic.
- HTTP: `reqwest` with rustls (no OpenSSL).
- Target platform: Linux (WebKitGTK). Other OSes are not a goal.
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
- Never disable TLS verification. The one existing
  `danger_accept_invalid_certs(true)` in `api.rs` is a marked TODO for F3
  (certificate pinning) and is the only allowed exception until F3 replaces it.
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
- **Unauthenticated API requests are delayed ~3s** by pveproxy to slow
  brute-forcing. Measured against the node: `GET /api2/json/version` with no
  cookie takes 3.005s and returns 401, while `GET /` returns 200 in 8ms.
  So never use an `/api2/json/*` endpoint for a reachability check — probe
  `/` instead. An authenticated call with a *valid* ticket is not delayed.
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

If a decision isn't already covered by this file or `FEATURES.md`, and it
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
- After I answer, record the decision under **Decisions already made** in
  `FEATURES.md` (one line: the decision + the reason), so it isn't asked again.

If you're unsure whether something counts as architecture, ask.

## How to work on a feature

1. Read the feature's entry in `FEATURES.md`.
2. Ask its open questions **one at a time**. Wait for each answer before the
   next question. Don't stack questions whose answers depend on each other.
3. Write the answers back into `FEATURES.md` under that feature.
4. Write the reasoning from "Reason before you code". Ask any architecture
   questions it raises.
5. Implement it. Keep changes scoped to that feature.
6. Run fmt / clippy / test.
7. Update the feature's status (`[ ]` → `[~]` → `[x]`).
8. Say what you could NOT verify (anything that needs a live Proxmox node).

## Testing reality

Login, the VM list, power actions, SPICE and the admin panel can only be
truly verified against a real Proxmox node. If one isn't reachable, say the
feature is untested — don't call it working. Unit-test the pure parts
(`.vv` formatting, config round-trip, response parsing from sample JSON).

## Communication

- Work step by step. When a task has several steps that need input or depend
  on the previous result, do one, report, then continue.
- No praise or flattery. State facts. If I'm wrong, say so directly.
- Be honest about uncertainty: say "I don't know" or "I haven't verified this".
