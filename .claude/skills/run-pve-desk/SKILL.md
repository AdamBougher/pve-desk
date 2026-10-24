---
name: run-pve-desk
description: Build, launch, drive and screenshot pve-desk — the Tauri 2 desktop client. Use when asked to run or start the app, take a screenshot of it, click through the launcher or guest list, reproduce a UI bug, or confirm a change works in the real app rather than only in cargo test. Covers the WebKitGTK frontend harness (driver.mjs), the real-window screenshot script (app-shot.sh), and the Rust test filters.
---

# Running pve-desk

Rust + Tauri 2 shell, plain HTML/JS frontend in `ui/`. Two things to drive,
and they are driven differently:

| Layer | How | Needs a node? |
| --- | --- | --- |
| Frontend (`ui/main.js`, 10 `invoke` commands) | `driver.mjs` — WebKitGTK + WebDriver, `window.__TAURI__` stubbed | no |
| Rust (`src-tauri/src/*.rs`) | `cargo test` | no |
| The whole app, visually | `app-shot.sh` | runs without one; the launcher just reports unreachable |

Paths below are relative to the repo root. **Run everything from the repo
root** — `cargo` commands are written with an explicit `cd src-tauri`,
because this tool's shell keeps its working directory between calls.

## Prerequisites

Already installed on this machine. On Arch:

```sh
sudo pacman -S --needed webkit2gtk-4.1 webkitgtk-6.0 imagemagick xorg-xprop nodejs
```

- `webkit2gtk-4.1` — what Tauri links against, and it ships `MiniBrowser`.
- `webkitgtk-6.0` — **only** for `/usr/bin/WebKitWebDriver`. 4.1 ships no
  driver binary. Driving the 4.1 MiniBrowser with the 6.0 driver works; the
  driver is pointed at the 4.1 binary explicitly.
- `imagemagick` (`import`, `magick`) and `xorg-xprop` — screenshots.

Rust comes from `rust-toolchain.toml` (pinned 1.97.1); cargo installs it.

## Build

```sh
cd src-tauri && cargo build --release
```

11s warm, several minutes cold. `app-shot.sh` runs this for you if the
binary is missing.

## Run: the frontend harness (start here)

`driver.mjs` loads the real `ui/` in WebKitGTK — the same engine the Tauri
webview uses on Linux — with `window.__TAURI__` replaced by a stub. No
Proxmox node, no keyring, no Rust build. It mirrors `ui/` to a temp dir and
injects the stub ahead of `main.js`; **nothing is written into `ui/`**.

```sh
node .claude/skills/run-pve-desk/driver.mjs smoke
```

27 checks, ~25s, exit 0 on success. It walks: launcher render → probe
statuses → certificate refusal and its fingerprint → connect → guest rows
and which power buttons each status gets → the hostile-name XSS check →
search filtering → force-stop confirm (cancel *and* confirm) → the exact
`power_action` arguments → a backend error reaching the status line →
`session-lost` returning to the launcher.

For poking at it by hand:

```sh
node .claude/skills/run-pve-desk/driver.mjs repl
```

Reads commands on stdin: `click <css>`, `type <css> <value>`, `text <css>`,
`count <css>`, `vis <css>`, `eval <js>`, `resp <cmd> <json>` (override a
stubbed reply), `fail <cmd> <msg>` (make one reject), `emit <name> <payload>`
(fire `tab` / `session-lost` / `admin-cert`), `calls` (every `invoke` the
frontend made, with arguments), `ss <name>`, `quit`.

Under tmux when you need it interactive:

```sh
tmux new-session -d -s pve 'node .claude/skills/run-pve-desk/driver.mjs repl'
tmux send-keys -t pve 'count #guests tr' Enter
tmux capture-pane -t pve -p | tail -5
tmux kill-session -t pve
```

`calls` is the useful one for backend work: it shows exactly what the
frontend sends, which is how you check the frontend never passes an API path
(a hard rule in `CLAUDE.md`).

Fixtures are at the top of `driver.mjs`, with shapes copied from the Rust
types. Edit them to reproduce a bug — a weird status, a missing name, 200
guests.

## Run: a picture of the real UI

```sh
.claude/skills/run-pve-desk/app-shot.sh launcher
```

Launches the real binary, waits for its window, captures it to
`.claude/skills/run-pve-desk/shots/launcher.png`, and stops it. It prints the
size and a colour count and warns if the capture came out blank; a real one
is `1000x700` with several hundred colours. Add `--keep` to leave it running.

It sleeps 2s after the window appears so the launcher's per-connection probe
has finished — capture sooner and the row is caught mid-check, without its
status or its Remove button.

**Use this, not the harness, for anything visual** — see Gotchas.

## Run: Rust

```sh
cd src-tauri && cargo test            # 31 tests
cd src-tauri && cargo test api::      # 6 — response parsing, power allow-list
cd src-tauri && cargo test config::   # 19 — config round-trip
cd src-tauri && cargo test spice::    # 6 — .vv formatting and 0600
```

There is no entry point for calling one function from outside; `commands.rs`
and `secrets.rs` have no tests because they need Tauri state and a keyring.
To exercise internals, add a `#[test]` in the module and filter to it.

Before calling anything done, per `CLAUDE.md`:

```sh
cd src-tauri && cargo fmt && cargo clippy -- -D warnings && cargo test
```

## Run: human path

```sh
cd src-tauri && cargo run
```

Opens the window against the real config in `~/.config/pve-desk/`. Needs a
Secret Service keyring for saved passwords. Ctrl-C to quit.

## Gotchas

- **`GDK_BACKEND=x11` is required for both scripts.** On a Wayland session
  MiniBrowser dies the moment it paints real content — `Gdk-Message: Error
  71 (Protocol error) dispatching to Wayland display`, surfacing through
  WebDriver as `session deleted because of page crash or hang`. `about:blank`
  survives, so the session looks healthy until the first real navigation.
  XWayland is fine. The app needs it too, or there is no X window to capture.
- **The harness window never paints on this machine.** The DOM is correct and
  every assertion is valid, but the pixels are one flat colour — WebKit
  cannot render into an offscreen buffer here (`Failed to create GBM buffer
  of size 1024x732`, and with `WEBKIT_DMABUF_RENDERER_DISABLE_GBM=1` it
  becomes `Failed to import DMABuf`). Same family of trouble as the GPU
  dead ends in `DECISIONS.md`. `driver.mjs` detects this and labels the file
  `BLANK` rather than handing back a picture of nothing. For visuals use
  `app-shot.sh`, whose window paints correctly.
- **WebDriver's own `GET /screenshot` hangs** — no answer, no error, the run
  just dies on whatever timeout wraps it. Both scripts read the X pixmap
  with `import` instead.
- **`import -window pve-desk` works for the app, not for the harness.**
  MiniBrowser launched with `--automation` titles its window
  `[Automation] pve-desk-driver`, which `import` will not match — it blocks
  until killed, which reads as a hang. `driver.mjs` resolves the id out of
  `xprop -root _NET_CLIENT_LIST` first. The harness page is retitled
  `pve-desk-driver` precisely so it can't be confused with the real app,
  which also titles its window `pve-desk`.
- **Never `pkill -x pve-desk`.** It matches by process name and kills any
  AppImage or dev build the user already had open. `app-shot.sh` kills only
  the pid it started. (This was learned the hard way — it closed the user's
  running app.)
- **Fingerprints are bare lowercase hex on the wire.** `api.rs:93` emits
  `{b:02x}` with no separators; `askTrust()` in `main.js` groups them into
  colon-separated pairs for eyeballing against openssl. A pre-grouped
  fixture gets re-grouped into nonsense.
- **The driver takes an ephemeral port.** A WebKitWebDriver whose session
  died of a page crash keeps running and keeps its port; a fixed port would
  make the next run fail to bind and look broken. Override with `WKD_PORT`.

## Troubleshooting

| Symptom | Cause / fix |
| --- | --- |
| `driver failed: WebKitWebDriver not ready on :N` | A leftover driver holds the port. `pgrep -a WebKitWebDriver`, kill it by pid. Current code takes a free port, so this means `WKD_PORT` was set. |
| `session deleted because of page crash or hang` | `GDK_BACKEND=x11` was lost. See Gotchas. |
| `(no screenshot: no X window named pve-desk-driver)` | The browser window closed before the capture, or the run is on a session with no X. |
| screenshot labelled `BLANK` | Expected on this machine. The DOM checks still hold; use `app-shot.sh`. |
| `no pve-desk X window appeared after 20s` | Check `/tmp/pve-desk-app.log`. A missing `libwebkit2gtk-4.1` is the usual cause on a fresh box. |
| `smoke` fails at `ui/index.html no longer loads main.js the expected way` | The `<script src="main.js">` tag changed; update `buildPage()` in `driver.mjs`. |
| `(no screenshot: none of N window(s) could be captured)` | An orphaned MiniBrowser from a run killed by a timeout still owns a window with the same title. `driver.mjs` now kills the browsers it started and tries every matching window, so this should be rare — if it persists, `pgrep -a MiniBrowser` and kill by pid. |

## Not verified here

Anything needing the live node — login, the guest list, power actions, the
admin panel — is stubbed in the harness, so a green `smoke` says the
frontend holds up its end of the contract, not that the node agrees.
`remote-viewer` is not installed, so SPICE cannot be exercised at all.
