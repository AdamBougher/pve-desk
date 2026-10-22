# pve-desk — Feature List

Status key: `[ ]` not started · `[~]` in progress / unverified · `[x]` done
· `[-]` cut or superseded

Each feature has **open questions**. These get answered one at a time before
the feature is built. Answers get written back into this file.

---

## v1 (current scope)

### F1. First-run setup
`[~]` A setup screen shown when no saved login exists.

Built 2026-10-20: config schema below, input validation in
`HostConfig::validate`, realm placeholder in the form, 13 unit tests in
`config.rs`. Still `[~]` not `[x]`: `save_login` only persists after a
successful login, so the path end to end is unverified without a live node.

- Fields: host, port (default 8006), username, realm (`pam` / `pve`), password
- On save: test the login, then store the password in the system keyring
  and the rest in a config file (`~/.config/pve-desk/config.toml`)

Decisions (2026-10-20):
- Q1.1 **Multi-host schema, one host in it.** `config.toml` uses an
  `[[hosts]]` array from day one, with a single entry (`name = "main"`) and no
  host picker in the UI, so adding hosts later needs no format migration.
  App-level settings (`remote_viewer_path`) stay top-level, outside the host
  entries; only per-connection settings live in a host entry.
- Q1.2 **Cluster, no failover in v1.** One address is the entry point; any
  cluster member serves the whole `/cluster/resources` list and proxies
  consoles. Trying several node addresses at login is a later feature.
- Q1.3 **Both realms offered, neither preselected.** The realm `<select>`
  starts on a disabled placeholder and is `required`, so setup cannot be
  submitted without an explicit choice.

### F2. Saved login + auto-login
`[~]` On launch, read the password from the keyring and call
`POST /api2/json/access/ticket` to get a session ticket + CSRF token.

- Re-login quietly before the ticket expires (tickets last ~2h; refresh
  every ~90 min)
- If the keyring is locked or unavailable, fall back to a password prompt

Open questions:
- Q2.1 **Answered by probing the machine (2026-10-20).** KDE Plasma on
  Wayland. `org.freedesktop.secrets` is owned by `gnome-keyring-daemon`
  (`kwalletd6` and `ksecretd` also run, but do not own that name). A
  throwaway secret was written and read back with no unlock prompt, so the
  `keyring` crate's Secret Service backend works as-is. A fallback password
  prompt is therefore not needed to get connected, but is still worth having
  for a locked keyring.
- Q2.2 If auto-login fails (wrong password, host down), what should
  happen: show an error + retry button, or reopen setup? **Still open.**
  Current behaviour: `main.js` shows the error text and falls back to the
  setup screen.

### F3. Certificate pinning
`[x]` Proxmox uses a self-signed cert by default. On first connect, show the
cert's SHA-256 fingerprint, ask you to confirm it, and store it. On later
connects, reject any cert that doesn't match.

Done for the **webview** (2026-10-21): the admin panel catches WebKitGTK's
TLS failure, compares the cert's SHA-256 against `cert_fingerprint` in the
config, and calls `allow_tls_certificate_for_host` for that one host. First
connect stores what it sees and reports it to the UI; a later mismatch is
refused and the error page stands. Verified: the stored fingerprint matches
the node's real leaf cert byte for byte.

Done for the **API client** (2026-10-21). `danger_accept_invalid_certs` is
gone from the codebase entirely. A custom `rustls::ServerCertVerifier` accepts
a certificate when it verifies against the OS trust store **or** matches the
pinned SHA-256, so a real certificate works and survives renewal while a
self-signed one is pinned. Decided under Q3.3: an unverified, unpinned
certificate stops the connection and the UI shows the fingerprint for
confirmation — the TLS handshake is refused before the request body is sent,
so the password never reaches an unverified server.

Verified against the live node: with the fingerprint pinned, connect succeeds
and the guest list loads; with the pin removed, the connection is refused and
the UI shows `F8:53:5C:…:51:40`, matching the node's real certificate.

New direct dependencies: `rustls`, `rustls-native-certs`, `sha2` — all were
already in the tree via reqwest, so nothing new is compiled.

Open questions:
- Q3.3 **Answered: ask.** An unverified certificate blocks the connection
  until the user confirms the fingerprint. The admin panel's webview still
  trusts on first use, but by then the API client has already pinned the
  fingerprint, so in practice it validates against it.

- Q3.1 **Answered by inspection:** self-signed, issued by the cluster's own
  `PVE Cluster Manager CA`, `CN=home.opah-tuna.ts.net`, valid to 2027-09-08.
- Q3.2 **Answered by inspection:** reached over the LAN at `192.168.1.2`.
  The cert's SAN also covers `home`, `localhost` and the Tailscale name
  `home.opah-tuna.ts.net`, so a VPN route would work without a new cert.

### F4. Embedded admin panel
`[x]` A window showing the full Proxmox web UI, already logged in, by handing
the session ticket to the webview as the `PVEAuthCookie` cookie.

Default view since 2026-10-21: the panel opens automatically as soon as a
session starts (auto-login or first-run setup), centered and focused. The
guest list stays open in the `pve-desk` window behind it; its "Admin panel"
button refocuses the panel if you close or lose it.

Verified 2026-10-21 against the live node: panel opens on the datacenter view
with both nodes listed, signed in as `root@pam` with no login form, so
`WebviewWindow::set_cookie` does survive the navigation. Not yet checked:
whether the Console button works, since Proxmox opens consoles with
`window.open` and Tauri does not handle popups by default.

Decisions (2026-10-21):
- Q4.1 **Separate window**, label `admin`. Asked but not answered directly;
  the stated goal was "do everything from the webview without opening my
  browser", which either layout satisfies. Chose the separate window because
  no capability names that label, so the hard rule (remote UI must not reach
  app commands) is visible at a glance rather than argued from origin rules.
  Proxmox sends no `X-Frame-Options` or CSP, so a tab/iframe stays viable if
  this proves annoying.
- Q4.2 Should closing the main window quit the app, or keep it in the
  system tray? **Still open.**

Cert handling: see F3. The CA-trust route was abandoned when it turned out
the node's sshd refuses password auth for root, so the CA file could not be
copied off without manual steps.

### F5. VM / container list
`[~]` List from `GET /api2/json/cluster/resources?type=vm`.

- Shows: name, VMID, type (VM / LXC), node, status, CPU, RAM
- Auto-refresh on an interval

Kept 2026-10-21, but as a read-only summary: power buttons are cut (F6).
Moves into the tabbed window (F14) rather than its own window.

Open questions:
- Q5.1 Refresh interval? (cv4pve uses 30s, sakakun 60s) — currently 30s.
- Q5.2 Any filtering you want (by tag, by status, search box), or
  just a plain list?
- Q5.3 Show templates, or hide them?

### F6. Power controls
`[-]` **Cut from v1 (2026-10-21).** The embedded Proxmox UI already does
start / shutdown / reboot, and that is where the work happens in practice, so
our own buttons duplicated it without ever being exercised.

The backend is harmless and stays: `Session::power` and its allow-list are
still there and still registered, just not driven by any UI. Delete or revive
when F15 (native UI) is decided. Q6.1 and Q6.2 are moot until then.

### F7. SPICE console
`[~]` `POST /nodes/{node}/qemu/{vmid}/spiceproxy` → write a temporary `.vv`
file → launch `remote-viewer` with it → delete the file after launch.

Parked, not cut: `remote-viewer` is not installed on this desktop and Q7.1 is
unanswered, so it has never run. Note the outstanding security debt in
`spice.rs` (the `.vv` file is written world-readable and never deleted) —
that must be fixed before this feature is ever enabled.

Open questions:
- Q7.1 Do any of your VMs actually use SPICE (Display = SPICE/qxl), or
  are they mostly default VGA/VNC? If none use SPICE, this drops in
  priority and the VNC console (F9) moves up.
- Q7.2 Path to `remote-viewer` — default `/usr/bin/remote-viewer` OK?

### F13. Log out
`[x]` Button in the main header. Drops the in-memory session and closes the
admin panel window, then returns to the setup screen.

Confirmed working by the user 2026-10-21.

Decided 2026-10-21: **non-destructive**. The config file and the keyring
entry are left in place, so the next launch auto-logs in again. Open
question: should there also be a "Forget this login" action that deletes the
keyring entry and config (i.e. a real sign-out rather than a session drop)?

### F16. Login launcher with saved connections
`[ ]` The app always opens on a login screen listing saved connections
(host, username, realm), rather than auto-connecting to one. Pick one to
connect. Each entry shows whether it is usable.

Asked for 2026-10-21: "always open to login, add a list of saved ip,
username, password combos, maybe show if an active login is still good."

**Reverses F2's auto-login on startup.** Quiet re-login before ticket expiry
still matters *within* a session — that is a separate concern and still
needed (the ~90 min refresh is still a TODO).

Already in place, no migration needed:
- `config.toml` is already an `[[hosts]]` array (Q1.1), today holding one
  entry. Adding entries needs no format change.
- `secrets.rs` already keys the keyring on `userid@host`, so several
  passwords coexist.

Hard rule unchanged: passwords live in the keyring only. The list shows host,
username and realm — never the password, and nothing is written to
`config.toml` but the non-secret fields.

Decisions (2026-10-21):
- Q16.1 **One live session, map-shaped.** `AppState` holds
  `HashMap<String, Session>` keyed by config entry id, with `set_only`
  enforcing a single entry. Allowing several later is a local change.
- Q16.2 **Reachable + ticket valid, on demand.** Opening the launcher (and
  the Re-check button) probes each entry; no timers, no background tasks.
  Reachability sends no credentials.
- Q16.3 **Connecting opens the admin panel immediately**, as before — it is
  the view actually used. Decided rather than asked.
- Q16.4 **`default` becomes "last used"**, written on every  successful connect
  and used only to preselect a row. Never auto-connects. Decided rather than
  asked.

Entry ids: new entries are named `user@realm@host` (`HostConfig::id`), so the
same account on two nodes never collides. The pre-existing `main` entry keeps
working untouched — ids are opaque and never shown; the UI displays the
fields.

Built 2026-10-21. Verified: launcher lists the saved entry, detects the
keyring password, and reports Reachable. **Not yet verified: clicking
Connect** (and therefore that connecting still opens the panel), and adding a
second connection.

Still open:
- Q16.5 Should entries be removable from the launcher? Deleting one means
  deleting its keyring password, which is destructive and needs a decision.

### F14. One window with tabs
`[ ]` Fold the two windows into one: a slim local tab bar with **Admin** (the
Proxmox UI) and **Guests** (the F5 list), instead of a separate panel window.

Asked for 2026-10-21: "keeping one window that has tabs would be nice", with
the login window kept around and the running list living in it.

Why it is not trivial — the Proxmox UI would move into an `<iframe>` inside
our own page, and the cert pinning (F3) currently hangs off WebKitGTK's
`load-failed-with-tls-errors` signal, which fires for main-frame loads. It is
unverified whether it fires for a subframe; if it does not, the iframe fails
with an unacceptable-certificate error and nothing catches it.

Likely answer: stop relying on the failure signal. The fingerprint is already
stored in the config, so the cert can be pre-authorised before any load —
build a `gio::TlsCertificate` with `from_pem()` and call
`allow_tls_certificate_for_host` up front. That needs the cert's PEM, which
means either storing it alongside the fingerprint or fetching it with a
handshake at startup. Decide before building.

Known safe: Proxmox sends no `X-Frame-Options` and no CSP, so framing itself
is allowed (checked 2026-10-21).

Isolation still has to hold: a remote origin inside our own page must not
reach app commands. Tauri gates IPC by origin, so as long as no capability
lists the Proxmox URL under `remote.urls`, it cannot — but this becomes an
argument from origin rules rather than something visible at a glance, which
is the trade-off noted under Q4.1.

Open questions:
- Q14.1 Does `load-failed-with-tls-errors` fire for subframe loads? (Test
  before designing around it.)
- Q14.2 Store the node's cert PEM in the config next to the fingerprint, or
  fetch it at startup with a handshake?

---

## Later (not in v1)

### F8. VNC console (non-SPICE VMs)
`[-]` **Dropped (2026-10-21): superseded by F4.** The embedded Proxmox UI
already provides the noVNC console, and it works. Only worth revisiting as
part of F15.

### F9. LXC terminal
`[-]` **Dropped (2026-10-21): superseded by F4.** The embedded Proxmox UI
already provides the container shell, and it works. Only worth revisiting as
part of F15.

### F15. Native UI for Proxmox
`[ ]` Replace the embedded web panel with a native UI built on the API —
guest management, consoles, storage, tasks. This is what F6, F8 and F9 were
heading towards before they were cut.

Added 2026-10-21 at the user's request: "it might be nice to re-implement a
UI for Proxmox later". Large: it means re-doing in this app what the Proxmox
web UI already does well. Only worth it if the embedded panel proves limiting
in daily use. Revisit F6 (power controls), F8 (VNC) and F9 (LXC terminal)
together with this — they are the natural first slices.

### F17. Cross-platform support (Windows / macOS)
`[ ]` Run on Windows and macOS, not just Linux.

Added 2026-10-21 at the user's request. **Requires changing a decision**:
`CLAUDE.md` currently states "Target platform: Linux (WebKitGTK). Other OSes
are not a goal." That line has to change first.

Already portable, no work: Tauri 2, reqwest + rustls, serde/toml, the whole
`ui/` frontend, and `dirs::config_dir()` (already resolves per-OS). The API
client, config, launcher and guest list need nothing.

Verified 2026-10-21 by reading the vendored sources:
- **`set_cookie` exists on all four wry backends** (webkitgtk, wkwebview,
  webview2, android). The admin panel's "opens already signed in" trick is
  portable as-is — this was the feature most at risk and it is fine.
- The keyring crate has `windows-native` and `apple-native` features, so
  `secrets.rs` is unchanged; only the `Cargo.toml` feature table moves.
- Tauri hands out a *typed* `ICoreWebView2Controller` on Windows (not a raw
  pointer), but only `*mut c_void` for WKWebView on macOS.
- **wry has no server-certificate handling on any platform.** Our Linux
  pinning reaches past wry into `webkit2gtk` directly; there is no
  cross-platform seam to extend, so each platform is bespoke.
- **`reqwest` is configured with `rustls-tls`, which uses `webpki-roots`
  (compiled-in Mozilla roots), not the OS trust store.** Consequence: on
  Windows/macOS, importing the Proxmox CA into the system store would make
  the *webview* trust the node but the *API client* would still refuse it.
  The CA-install escape hatch therefore only solves half the problem on any
  platform; the API client needs `rustls-tls-native-roots` or explicit
  pinning regardless. This ties F17 to the unfinished half of F3.

Small and mechanical:
- Keyring features. `Cargo.toml` pins `sync-secret-service` (Linux only); the
  crate also offers `apple-native` (Keychain) and `windows-native`
  (Credential Manager). A per-target feature table; `secrets.rs` is unchanged.
- SPICE. `/usr/bin/remote-viewer` is hardcoded as the default in `config.rs`
  and needs a per-OS one. The outstanding 0600 TODO in `spice.rs` is Unix-only,
  so Windows needs a different approach (ACLs, or a per-user temp dir).
- Bundle targets: add `msi`/`nsis` and `dmg`; icons need `.ico` and `.icns`,
  which `tauri icon` generates from `icons/icon.svg`.
- `cfg`-gate the NVIDIA render workaround in `lib.rs` — harmless elsewhere,
  but misleading.

The hard part — **certificate pinning (F3/F4)**. Each platform's webview is a
different object, so it is three implementations of one idea:
- Linux: `webkit2gtk`, `allow_tls_certificate_for_host`. Done.
- Windows: WebView2 has `ServerCertificateErrorDetected`, but Tauri does not
  surface it; reach the raw `ICoreWebView2` through `with_webview` and call it
  via `windows-rs`.
- macOS: WKWebView's `didReceiveAuthenticationChallenge`, which means
  Objective-C interop *and* wry already installs its own navigation delegate.

Escape hatch: on Windows and macOS, installing the Proxmox cluster CA into the
system trust store is a normal double-click operation (unlike this Linux
desktop, where the node's sshd blocked getting the file at all). Those
platforms could ship with no pinning code and a setup note instead.

Estimate: everything except pinning is about half a day of config and build
plumbing. Pinning on all three is a few days, most of it interop that cannot
be verified without the hardware.

Real constraint is testing, not code. Every interesting failure found on
2026-10-21 — NVIDIA DMABUF, console rendering, pveproxy's 3s delay — only
showed up on real hardware. A Windows build nobody can run is a build nobody
can call working.

Decisions (2026-10-21):
- Q17.1 **Windows first, macOS later.** The user has a Windows laptop to test
  on, which is what makes it worth doing — see the testing note above.
- Q17.2 **Native roots + CA import**, not per-platform pinning. `reqwest`
  switches from `webpki-roots` to `rustls-tls-native-roots` so the API client
  reads the OS trust store; importing the Proxmox cluster CA then satisfies
  both the webview and the API client on Windows/macOS. Linux keeps its
  existing in-app pinning, which already works and needed no CA install.

Phases:
1. Compile everywhere — per-target keyring features, per-OS `remote-viewer`
   default, `cfg(unix)` on the `.vv` permissions work, `cfg`-gate the NVIDIA
   render workaround, non-Linux admin panel path (`set_cookie` + navigate,
   no pinning).
2. CI matrix on `ubuntu-latest` + `windows-latest`. This is the only
   cross-platform verification available without the hardware in hand.
3. Packaging: `msi`/`nsis`, `.ico` from `icons/icon.svg`.
4. TLS: the native-roots switch above, plus a documented CA-import step.

Open question:
- Q17.3 Does the admin panel actually open signed-in on Windows? `set_cookie`
  exists on the webview2 backend, but cookie domain/secure handling may
  differ. Needs the laptop.

### F18. Containerized dev environment
`[~]` A reproducible, pinned build/dev environment instead of relying on
whatever this CachyOS install happens to have.

Tier 1 built and verified 2026-10-21: `cargo fmt --check`, `cargo clippy
--all-targets -- -D warnings` and `cargo test` (13 passed) all run clean
inside the image. Clippy compiled the `webkit2gtk` crate there, which proves
the `v2_40` features the cert pinning needs are available on Debian's
webkit — the main risk in this feature, now retired.

Files: `.devcontainer/Containerfile`, `.devcontainer/devcontainer.json`,
`.containerignore`, and `rust-toolchain.toml` (pins 1.97.1 for host and
container alike, so they cannot drift).

Decisions (2026-10-21):
- Q18.1 **Tier 1 now, Tier 2 staged.** Build and test work today; the
  display / D-Bus / `/dev/dri` wiring is written but commented in
  `devcontainer.json`, ready to enable.
- Q18.2 **Debian 13 (trixie).** Matches the `.deb` bundle target *and* ships
  webkit2gtk-4.1 **2.54.0** (host: 2.54.1) — measured, not assumed. Arch was
  not needed.
- Q18.3 **`.devcontainer/` + plain Containerfile.** Works with VS Code Dev
  Containers (set `dev.containers.dockerPath` to `podman`) and the same
  Containerfile is reusable for CI unchanged. distrobox stays available for
  Tier 2 if the devcontainer mounts prove awkward.
- Q18.4 **Local for now, CI-ready.** No CI config added; the image is built
  so one can use it later.

Not using Debian's `rustc` (1.85.1, far behind this project's 1.97.1) — Rust
is installed via rustup and pinned. `libssl-dev` is deliberately absent: this
project uses rustls and must not acquire an OpenSSL dependency.

**Demoted 2026-10-21, same day it was built.** Per-distro install
instructions are the primary route in the README; the container is an
optional extra. Reasoning, so this isn't re-litigated:

- CI on `ubuntu-latest` already does the container's main job — a clean
  Debian-family machine installing the documented dependency list from
  scratch on every push. The image duplicates that.
- The container cannot run the app, and this is a GUI client. Every bug
  found on 2026-10-21 (NVIDIA DMABUF, console rendering, keyring, window
  focus) needed the GUI on real hardware; none were the kind a build
  container catches.
- `rust-toolchain.toml` pins the compiler for host and container alike, so
  most of the reproducibility win is not the image's doing.
- It does nothing for anyone *installing* pve-desk — that's F12 packaging.

Kept because it works, costs a few files, and is useful for offline or
pre-push checks against Debian. Note the README does **not** claim CI uses
this image; CI installs deps with apt directly.

Still to do for Tier 2: uncomment the mounts, then confirm a window actually
appears and that the keyring resolves over the mounted session bus.

Added 2026-10-21 at the user's request. Splits into two tiers with very
different difficulty.

**Tier 1 — build and test container (easy, most of the value).**
No GUI: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, `cargo
build`. Pins the toolchain and the system libs, and doubles as the CI image.

Versions on this machine 2026-10-21, to pin against:
- rustc / cargo 1.97.1
- webkit2gtk-4.1 and javascriptcoregtk-4.1 2.54.1
- libsoup-3.0 3.8.0, gtk+-3.0 / gdk-3.0 3.24.52, glib-2.0 2.90.1
- plus pkg-config and a C toolchain

Debian stable turned out to be the right base: the bundle targets are `deb`
and `appimage` (F12), so the build host should resemble the target, and the
feared "older webkit" problem did not materialise — trixie ships 2.54.0.

**Tier 2 — running the GUI from the container (harder).**
Needs the Wayland socket (`$XDG_RUNTIME_DIR/wayland-*`) or X11
(`/tmp/.X11-unix` + xauth), `/dev/dri` for rendering, and the **session D-Bus**
— without it `org.freedesktop.secrets` is unreachable and the keyring, hence
auto-login, simply doesn't work.

Already installed here, which makes this much cheaper: **podman 6.1.4** and
**distrobox 1.8.2.5** (`docker` is the podman shim). distrobox wires up
display, D-Bus and GPU passthrough by default, so Tier 2 is largely "write a
distrobox recipe", not "solve socket passthrough".

**Honest caveat.** Every interesting bug found on 2026-10-21 — the NVIDIA
DMABUF crash, console rendering, pveproxy's 3s delay — was host- or
network-specific. A container improves reproducibility and CI; it would not
have caught any of them, and GPU and keyring behaviour *inside* a container
differs from the host, so a green container run is not evidence the app works
on a real desktop. Keep verifying on the host.

Open question:
- Q18.5 When Tier 2 is switched on, does the GUI actually work from inside —
  window, rendering, and the keyring over the mounted session bus? Unverified.

### F10. System tray
`[ ]` Tray icon with quick start/stop for favourite VMs.

### F11. Favourites / pinned VMs
`[ ]` Pin VMs to the top of the list.

### F12. Desktop entry + packaging
`[ ]` `.desktop` file, icon, and a package (AppImage, `.deb`, or Flatpak).

Open question for later:
- Q12.1 Which distro are you on, and which package format do you want?

---

## Decisions already made
- Language: **Rust**
- UI shell: **Tauri 2** (WebKitGTK on Linux), plain HTML/CSS/JS frontend,
  no Node build step
- Auth: password login, saved in system keyring; **no SSH keys**
- Sign-in: single factor on my Proxmox account, so the client can fetch a
  ticket without an interactive prompt
- License: **TBD** (see README)
- TLS/F3 (2026-10-21): the node serves a self-signed cert from the PVE
  Cluster Manager CA. Decision: **install that CA into this desktop's system
  trust store** (`/etc/ca-certificates/trust-source/anchors/` +
  `update-ca-trust`), because WebKitGTK has no per-window TLS override and
  reads the system store. This also opens the door to switching reqwest to
  native roots and deleting `danger_accept_invalid_certs` entirely.
  Rejected: an in-app webkit2gtk exception (new crate, Linux-specific, does
  nothing for reqwest) and giving the node a real Tailscale cert (node-side
  work plus renewal automation; still the best end state later).
- Console typing (resolved 2026-10-21): rough, laggy typing in Proxmox
  consoles was a **rendering** problem, not an input one — it disappeared
  once the WebKit rendering path changed (see below). The same consoles were
  always smooth in Firefox on this machine, which is the comparison that
  should have been made first.
  Tried and removed as wrong: a JS shim injected into the panel to patch
  xterm.js's hidden-textarea handling, and disabling the webview's GTK input
  method context. Both chased a `keyCode 229` composition theory that was not
  the cause. Don't re-add them without evidence.
- Linux/NVIDIA rendering (revised 2026-10-21): `lib.rs` now defaults to
  `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1`, keeping WebKit's dmabuf renderer and
  accelerated compositing but routing buffers through shared memory instead
  of GBM. GBM is the part that fails on the NVIDIA proprietary driver, where
  native Wayland dies with "Error 71 (Protocol error)" before any window
  appears. Verified on this machine: clean start on Wayland, no errors,
  renders correctly. `PVE_DESK_RENDER=legacy` restores the earlier
  `WEBKIT_DISABLE_DMABUF_RENDERER=1`; `PVE_DESK_RENDER=raw` sets nothing.
  Kept as an escape hatch: the right path is GPU-specific. **This change
  is what fixed console typing.**
  Tried and rejected: pointing WebKit at the AMD iGPU via
  `WEBKIT_WEB_RENDER_DEVICE_FILE=/dev/dri/renderD129` (displays are all on the
  NVIDIA card, so the compositor hands Mesa the NVIDIA device and it falls
  back to software), and `WEBKIT_DMABUF_RENDERER_DISABLE_GBM=1` (still
  crashes).
