# pve-desk

Linux desktop client for Proxmox VE, in Rust on Tauri 2. A launcher of saved
connections, Admin (the embedded Proxmox web UI, signed in), a guest list
with start / shutdown / stop on each row, and a tray icon (Show and Quit)
that the window closes to. SPICE is written but unproven.

## Where things live

Each place has one job:

- **GitHub issues** (`AdamBougher/pve-desk`): outstanding work only. One
  issue per feature, titled `F<n> — <name>`; the body is Gherkin
  (`Scenario:` = acceptance criteria, `#` comments = open questions). The
  F-number is permanent and is not the issue number, so find an issue by its
  title. Labels and `gh` usage: `docs/agents/issue-tracker.md`,
  `docs/agents/triage-labels.md`.
- `DECISIONS.md`: why the app is built this way, and what was tried and
  failed, with the measurement. Read the sections for your area before you
  propose a design.
- `GLOSSARY.md`: the agreed term for each concept. Use its words. A
  **guest** is a VM or a container, a **connection** is a saved login, a
  **ticket** is the Proxmox credential (a work item is an **issue**).
- `README.md`: what the app does today.

To work a feature, use the `/feature` skill. It holds the full loop: read the
issue, ask its open questions, reason, implement, verify, update labels,
retire the issue.

Before you touch `tauri::`, `tauri.conf.json`, `capabilities/` or
`window.__TAURI__`, read the `tauri2` skill. Tauri 1 and Tauri 2 differ in
exactly these places.

## Decisions already made

Change these only after you ask me:

- Rust backend, **Tauri 2**.
- Frontend: plain HTML/CSS/JS in `ui/`. No Node, no bundler, no framework.
  The UI calls the backend with `window.__TAURI__.core.invoke`.
- Auth: username + password against `/api2/json/access/ticket`. The
  password is stored in the system keyring (`keyring` crate, Secret
  Service). The account uses a single factor.
- HTTP: `reqwest` with rustls.
- FOSS dependencies only.
- Linux first. Where the UI differs per platform, Linux wins. Keep non-UI
  code portable, put platform code behind `cfg`, and say which platforms you
  tested.

## Module map

`src-tauri/src/`: `commands.rs` holds every function the UI can call;
`api.rs` the Proxmox client and certificate verifier; `config.rs`
`~/.config/pve-desk/config.toml` (no secrets); `secrets.rs` the keyring;
`spice.rs` the `.vv` file and `remote-viewer`; `tray.rs` the tray;
`error.rs` the error type, sent to the UI as a string.

## Security rules

These protect the user's credentials and server. They apply to every change:

- **Password**: it goes only to the keyring and the login request body.
  Logs, the config file, error strings and any file on disk never contain it.
- **TLS**: every connection is verified by `PinnedVerifier` in `api.rs`:
  valid against the OS trust store, or equal to the pinned fingerprint. An
  unknown certificate stops the connection and goes to the user for a
  decision. Keep this as the only verifier.
- **Power actions**: `Session::power` calls `check_power`, an allow-list of
  guest types and actions. The frontend sends a node, guest type, VMID and
  action name; it never sends an API path.
- **Server text** (guest names, node names, tags, errors) goes into the DOM
  with `textContent`.
- **`.vv` file**: it holds a one-time ticket. Mode 0600, deleted soon after
  use.
- **Capabilities**: grant the narrowest permission that works. Capabilities
  are scoped by webview label; the Admin webview loads a remote site and
  gets no access to app commands.

## Architecture questions

Ask me before you implement a decision that is hard to change later and that
`DECISIONS.md` does not cover: a new or replaced crate, a new module or moved
responsibility, how state is stored or shared, anything about auth, tickets,
cookies, TLS or certificates, a new capability or window/webview, an on-disk
format, or a background task or timer.

Ask one question. Give 2–3 options with trade-offs and your pick. After I
answer, record the decision and its reason in `DECISIONS.md`.

## Proxmox API notes

- Every response is wrapped: `{ "data": ... }`.
- Login body is form-encoded; `username` is `user@realm`.
- Every request sends `Cookie: PVEAuthCookie=<ticket>`. POST/PUT/DELETE also
  send `CSRFPreventionToken: <token>`.
- Tickets expire after ~2 h; `commands.rs` refreshes after ~90 min.
- Permissions: `VM.Audit` (list), `VM.PowerMgmt` (power), `VM.Console`
  (SPICE).
- pveproxy delays unauthenticated `/api2/json/*` requests by ~3 s and logs
  each as a failed login. Probe `GET /` for reachability (~8 ms, no auth).
- Reference: https://pve.proxmox.com/pve-docs/api-viewer/. If you are not
  sure how an endpoint behaves, check there and say so.

## Done means

From `src-tauri/`: `cargo fmt`, `cargo clippy -- -D warnings` and
`cargo test` pass with no warnings. Unit-test the pure parts (`.vv`
formatting, config round-trip, response parsing from sample JSON).

Login, the guest list, power actions, SPICE, the tray and Admin need a live
Proxmox node to verify. Without one, report the feature as untested and name
the scenarios you could not run.

## Versions

Every pull request bumps the patch number by 1 (`1.2.1` → `1.2.2`), in the
same PR, in all four places:

- `src-tauri/tauri.conf.json`: `"version"`
- `src-tauri/Cargo.toml`: `version`, then `cargo build` to update
  `src-tauri/Cargo.lock`
- `packaging/arch/PKGBUILD`: `pkgver`, with `pkgrel=1`

Bump from the version on `main` when you open the PR. If another PR merges
first with the same number, bump again. The `version` job in `ci.yml` fails
a PR whose four versions disagree or are not higher than its base branch.

A bump is not a release. A release is a `v<MAJOR>.<MINOR>.<PATCH>` tag on
`main` (three numbers), and `release.yml` builds it. Until you tag,
building the PKGBUILD from `main` gets a 404, because its source is tag
`v$pkgver`.

## Communication

- Reply in ASD-STE100 Simplified Technical English, with `GLOSSARY.md`'s
  words.
- State facts. If I am wrong, say so. If you do not know, say "I don't know".
