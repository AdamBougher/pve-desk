# pve-desk — decisions and dead ends

Why the app is built the way it is, and what has already been tried and
failed. **Read this before changing auth, TLS, the admin panel, the refresh
model or the Linux rendering setup** — several of the ideas below were
implemented, tested against the real node, and then removed.

Companion files: `ROADMAP.md` is outstanding work, `README.md` is what the app
does today. Question numbers (Q1.1, Q3.3 …) are kept because source comments
cite them.

---

## Dead ends — don't re-propose without new evidence

**Installing the cluster CA into this desktop's trust store** (2026-10-21).
The intended fix for the self-signed certificate. Abandoned: the node's sshd
refuses password auth for root, so the CA file can't be copied off without
manual steps through the Proxmox UI. Still the plan on **Windows** (F17),
where importing a CA is a normal operation.

**A real Tailscale certificate for the node** (2026-10-21). Rejected for now:
node-side work plus renewal automation. Genuinely the best end state, later.

**A background timer for ticket refresh** (2026-10-21). Rejected in favour of
refresh-on-demand plus a 401 retry. `Instant` uses `CLOCK_MONOTONIC`, which
does not advance while the machine is suspended, so a laptop that slept
through a ticket's lifetime still looks fresh — age alone is not trustworthy.
The 401 path is ground truth and is needed either way, which left the timer
earning very little, at the cost of a task whose lifetime has to track
connect, logout and switching connections.

**Two input-layer fixes for rough console typing** (2026-10-21): a JS shim
injected into the panel to patch xterm.js's hidden-textarea handling, and
disabling the webview's GTK input method context. Both chased a `keyCode 229`
composition theory that was **not** the cause — the problem was *rendering*,
and vanished when the WebKit rendering path changed. Both are removed. Don't
re-add them without evidence. The same consoles were always smooth in Firefox
on this machine, which is the comparison that should have been made first.

**Two GPU workarounds that didn't work** (2026-10-21): pointing WebKit at the
AMD iGPU with `WEBKIT_WEB_RENDER_DEVICE_FILE=/dev/dri/renderD129` (all
displays are on the NVIDIA card, so the compositor hands Mesa the NVIDIA
device and it falls back to software), and
`WEBKIT_DMABUF_RENDERER_DISABLE_GBM=1` (still crashes).

**Probing an API endpoint for reachability.** pveproxy deliberately delays
responses to *unauthenticated* `/api2/json/*` requests by ~3s, so the probe
took 3s instead of milliseconds and logged a failed authentication on the node
every time. Measured: `GET /api2/json/version` with no cookie = 3.005s / 401;
`GET /` = 8ms / 200. The probe uses `/`.

---

## Decisions

### Config and storage (2026-10-20)
- **Q1.1 Multi-host schema from day one.** `config.toml` uses an `[[hosts]]`
  array, so adding connections needed no format migration — which is exactly
  what F16 later relied on. App-level settings stay top-level, outside the
  host entries. Scalars must be written before the array-of-tables, or the
  TOML is invalid.
- **Q1.2 Cluster, no failover.** One address is the entry point; any cluster
  member serves the whole `/cluster/resources` list and proxies consoles.
  Trying several node addresses at login is a later feature.
- **Q1.3 Both realms offered, neither preselected.** The realm `<select>`
  starts on a disabled placeholder and is `required`, so a connection can't be
  saved without an explicit choice.
- Entry ids are `user@realm@host` (`HostConfig::id`), so the same account on
  two nodes never collides. Ids are opaque and never shown.

### Auth and tickets
- **Q2.1 The keyring works as-is** (probed 2026-10-20). KDE Plasma on
  Wayland; `org.freedesktop.secrets` is owned by `gnome-keyring-daemon`
  (`kwalletd6` and `ksecretd` also run but don't own that name). A throwaway
  secret was written and read back with no unlock prompt. A fallback password
  prompt is therefore not needed to get connected — it is still worth having
  for a *locked* keyring, and is not built.
- **Refresh on demand, retry on 401** (2026-10-21). Before an authenticated
  call, a session older than ~90 min is renewed; if the node rejects a ticket
  anyway, the app logs in once more and retries that call. See the rejected
  timer above.
- **Q2.2 On total auth failure, return to the launcher with the reason.** The
  backend drops the session and emits `session-lost`; the UI stops the 30s
  poll, clears the list and shows why. The original framing (error+retry vs
  reopen setup) was overtaken by F16 — there is no setup screen any more.
- **Log out is non-destructive.** The config file and keyring entry are left
  in place.

### TLS (settled 2026-10-21)
The node serves a self-signed certificate from its own PVE Cluster Manager
CA. Final shape: the API client verifies against the OS trust store **or** a
pinned SHA-256, so a real certificate works and survives renewal while a
self-signed one is pinned. The admin panel webview pins the same fingerprint
via `webkit2gtk`'s `allow_tls_certificate_for_host`.

- **Q3.3 An unverified, unpinned certificate blocks the connection** until the
  fingerprint is confirmed. The handshake is refused before the request body
  is sent, so the password never reaches an unverified server.
- `danger_accept_invalid_certs` is gone from the codebase and **must not come
  back.**
- Direct dependencies added (`rustls`, `rustls-native-certs`, `sha2`) were
  already in the tree via reqwest, so nothing new is compiled.

### Admin panel (2026-10-21)
- **Q4.1 A separate window**, label `admin`, not a tab or iframe. Either
  layout met the stated goal; the separate window was chosen because no
  capability names that label, so the hard rule — a remote UI must not reach
  the app's commands — is visible at a glance rather than argued from origin
  rules. Proxmox sends no `X-Frame-Options` or CSP, so a tab stays viable
  (F14).
- It opens automatically on connect, because it is the view actually used.
  (Q16.3)

### Power controls — cut, then revived (F6)
**Cut 2026-10-21.** The embedded Proxmox UI already does start / shutdown /
reboot, and that is where the work happened in practice, so our own buttons
duplicated it without ever being exercised. The backend was left registered
and undriven, with the note "delete or revive when F15 is decided".

**Revived 2026-10-22** at the user's request. What changed the answer: the
Guests tab is read-only, so stopping a guest meant switching to Admin and
walking the Proxmox tree; and `Session::power` was reachable from the
frontend yet never called, which is the worst of the three states for a
write path in an app whose hard rules are about not letting the frontend
drive arbitrary API calls. Either exercise it or delete it — exercising it
won.

**Confirmed working by the user 2026-10-22**, the same day it was built.

- **Q6.1 Start, graceful shutdown, forced stop.** Not reboot: it is shutdown
  then start, rarely wanted in a hurry, and still in the admin panel. The
  `api.rs` allow-list shrank to the same three, so `reboot` is now *refused*
  by `api::check_power` rather than merely unused — anything that wants it
  back has to say so. Narrowest that works, per the hard rule.
- **Q6.2 Only the forced stop asks.** Start and shutdown are reversible and
  fire on one click; stop can lose whatever the guest had not flushed, so it
  reuses the in-page confirmation panel that F3's certificate trust and F19's
  removal use. Three confirmations, one helper (`askConfirm` in `main.js`).
- **Matching is exact, not sanitising.** An exact match against a fixed list
  cannot carry a path segment, a traversal or a query string, which is the
  property that keeps the frontend off arbitrary endpoints. Don't replace it
  with escaping.
- **The check is a free function, not inline in `power`.** `Session::power`
  needs a live server, so the guard could not be tested through it. As
  `api::check_power` it has tests for traversal, query strings, case folding
  and the dropped `reboot` — the hard rule is now covered by tests rather
  than by reading.
- **Buttons follow the guest's status, and an unrecognised status gets
  none.** Running offers Shutdown and Stop, stopped offers Start; paused,
  suspended, and the `unknown` a down node reports for its guests offer
  nothing. Guessing wrong there means firing the wrong action at a real VM,
  and the admin panel covers those states. The status lookup uses
  `Object.hasOwn`, because the key comes from the server.
- **In-flight guests are tracked outside the DOM** (a `busy` set keyed
  `node/vmid`). `render()` rebuilds every row from scratch, including on the
  30s refresh, which would otherwise re-enable a button mid-request and let a
  second POST through.
- **Two refreshes per action, immediately and after 3s.** Proxmox answers with
  a task id and works on it afterwards, so `/cluster/resources` lags the
  request. The 3s is a guess — see Known gaps in `ROADMAP.md`.

### Removing a saved connection (F19, 2026-10-22)
Answers **Q16.5** and the **"Forget this login"** question together; both had
been waiting on the same decision. Confirmed working by the user 2026-10-22.

- **Q19.1 One action: the config entry and the keyring password go
  together.** A keyring entry is only reachable through its config entry, so
  keeping one without the other leaves a secret the app can neither show nor
  clean up. Rejected: a separate "forget password" that keeps the row (there
  is no password prompt — see the keyring fallback gap — so the row could only
  sit there with Connect greyed out), and deleting the config entry alone,
  which orphans the secret.
- **Q19.2 Launcher only, refused while connected.** Greyed in the page, and
  `Error::Connected` in the backend whatever the page asks. The session is
  keyed by the entry's id, so deleting it underneath would leave a session
  whose refresh and admin panel can no longer resolve a host. **"Forget this
  login" is therefore a no**: log out, then remove. No destructive button
  beside Log out, and no one-click logout-and-delete.
- **The password is deleted before the config is rewritten.** A keyring that
  refuses — locked, or no Secret Service — then leaves everything in place to
  retry. The reverse order orphans the secret, which is what Q19.1 rejected.
  A missing keyring entry counts as success: the goal is that no password is
  left behind, and there being none already satisfies it.
- `Config::remove_host` repoints `default` at a surviving entry, or clears it,
  so it never names something deleted. **`default = ""` is a valid state**
  meaning "no row preselected" — it is what removing the last connection
  leaves behind.

### Guest list (2026-10-21)
- **Q5.1** Refresh every **30s**, matching cv4pve.
- **Q5.2** A **search box only**, over name, VMID, node and tags, filtering
  the list already in hand. No status or tag filters: the list exists to find
  a guest, and the admin panel covers anything heavier.
- **Q5.3 Templates are hidden** — dropped at fetch, not at render, so they
  can't surface through the search either.

### Launcher (2026-10-21)
- **Q16.1 One live session, map-shaped.** `AppState` holds
  `HashMap<String, Session>` keyed by config entry id, with `set_only`
  enforcing a single entry. Allowing several later is a local change.
- **Q16.2 Status checked on demand**, when the launcher opens and on
  Re-check. No timers, no background tasks. The reachability probe sends no
  credentials.
- **Q16.4 `default` means "last used"** — written on every successful
  connect, used only to preselect a row. It never auto-connects.

### One window with tabs (F14, 2026-10-21)
The Proxmox UI is a **child webview** in the main window
(`Window::add_child`, behind tauri's `unstable` feature), not an `<iframe>`.

Why: it keeps the Proxmox page a *main-frame* load, so F3's certificate
pinning — which hangs off WebKitGTK's `load-failed-with-tls-errors` — keeps
working untouched, and `window.open`, which is how Proxmox opens consoles and
which works today, behaves as it did in the separate window. Both of F14's
open questions became **moot rather than answered**: whether that signal
fires for subframes (Q14.1) and where to keep the certificate PEM for
pre-authorisation (Q14.2) only mattered to the iframe design.

Rejected: the `<iframe>`. It would have meant rebuilding pinning as
pre-authorisation (storing the PEM in the config, or a handshake at startup),
reduced isolation to an argument from origin rules rather than something
visible at a glance, and reopened `window.open`.

Accepted costs: an unstable API surface, and geometry managed by hand.

- **The capability is scoped by `webviews`, not `windows`.** A window-scoped
  capability applies to *every* webview in that window, so the previous
  `windows: ["main"]` would have granted `core:default` to the Proxmox
  webview once it moved inside the main window. tauri-utils says so in its
  own docs ("prefer specifying webviews ... for fine grained access
  control"), and `resolve_access` requires
  `origin.matches(context) && (webview matches || window matches)` — the
  local-only origin gate would have denied it anyway, but a hard rule should
  not rest on one mechanism.
- **On Linux the tab bar is a native GTK widget**, not part of the page, and
  exactly one webview is visible at a time. Two measured facts forced this,
  both found by running it rather than by reading the API:
  1. A child webview **cannot be positioned**.
  `tauri-runtime-wry` builds a `WindowChild` webview into the window's
  `GtkBox` ("only way to account for menu bar height, and also works for
  multiwebviews :)"), and wry packs it with
  `pack_start(webview, true, true, 0)` and sets `is_in_fixed_parent = false`,
  which makes `set_bounds` a no-op. Two webviews therefore split the window
  vertically, which is exactly what the first attempt produced: Proxmox in
  the bottom half, a blank strip above it.
  2. A webview **cannot be capped** to the tab bar's height either. The
     obvious repair — pack the local webview `expand = false` with
     `set_size_request(-1, 36)` — does not work, and this was tried and
     measured: `set_size_request` sets a *minimum*, and GTK3 gives a box
     child its **natural** size before giving anything to expanding
     siblings. With `req = -1x36` the local webview reported
     `preferred_h = (min 36, nat 700)`, took all 700 pixels, and left the
     admin webview 1. GTK3 has no maximum-size property.

  What does work is a native strip: a `gtk::Box` of two `gtk::Button`s
  packed non-expanding above both webviews, with the webviews shown one at a
  time so the visible one fills the rest. A button box has a small, honest
  natural height, which is exactly what a webview does not. Switching is
  driven from Rust, not from the page — while the panel is up the local
  webview is hidden, and depending on a hidden webview's JavaScript to switch
  back would be a way to get stuck. `native_tabs` tells the page to hide its
  own tab buttons so there is only ever one tab bar.

  This needs `gtk` as a direct dependency, pinned to `0.18` to match
  webkit2gtk's own — already in the tree, so nothing new compiles.
- **Tabs are Linux-only; elsewhere the panel is its own window**, as it was
  before F14. The native strip is a GTK widget with no Windows or macOS
  equivalent, and a child webview with no way to switch back from it would
  be worse than a separate window. The speculative positioning path for
  those platforms was deleted rather than kept untested: it was written
  against an API whose behaviour on Linux turned out to be nothing like the
  documentation implied, and no one has run this on Windows to find out
  whether it is any better there.
- **Use `set_bounds`, never `set_position` + `set_size`** on the platforms
  that do position webviews. Those two are read-modify-write: each reads the
  webview's current geometry first, and wry's GTK `bounds()` reads the live X
  window, so a layout pass between the calls makes the second overwrite the
  first with stale coordinates. `set_bounds` applies the whole rectangle with
  no read-back.
- **`auto_resize()` is deliberately not used.** It stores position and size
  as *proportions* of the window, so a fixed 36px tab bar offset would become
  72px at double the window height.
- **The tab bar height is duplicated** between `TAB_BAR` in `commands.rs` and
  `--tab-bar-height` in `ui/style.css`. Nothing keeps them in sync; if they
  diverge the local webview either clips the tabs or leaves a gap under them.

### Linux first (2026-10-21)
Where the UI can't be the same on every platform, Linux wins and the others
wait. Decided when F14's tab bar turned out to need a GTK widget: Linux is
the only platform ever run, and holding its UI back to whatever all three
can share would trade something working for something hypothetical.

Windows and macOS stay buildable and stay in F17 — this is about not
compromising the Linux UI, not about dropping them. Non-UI code stays
portable.

### Release format (2026-10-21)
**AppImage, plus an Arch package.** AppImage for everyone else: one file, no
distro assumptions, and Tauri bundles it already. Arch because it is what
this is developed and run on, which makes it the one package that gets used
daily rather than only tested.

Not `.deb` (nobody has asked, and CI building on Debian is not the same as
wanting to ship there) and not Flatpak (sandboxing would complicate Secret
Service access and launching `remote-viewer` for F7). `nsis` stops being
built with them: Windows has no tab bar, so it is not what gets released.

### Releases on GitHub (2026-10-21)
Tagged releases on GitHub, built by a workflow on a `v*` tag, published as a
**draft** so the notes are written before anyone sees it. Not the AUR yet —
a separate submission with its own review, and the in-repo `PKGBUILD` can be
built by hand meanwhile.

The AppImage is built on **ubuntu-22.04**, not `ubuntu-latest`: an AppImage
is only as portable as the glibc it was linked against, and building on
24.04 would make it unrunnable on anything older.

### SPICE `.vv` handling (2026-10-21)
The `.vv` file carries a one-time console ticket, so `CLAUDE.md` requires it
be private and short-lived.

- **Mode 0600 at creation, not afterwards.** Set via `OpenOptionsExt::mode`
  on unix so there is no window in which the file exists world-readable.
  Windows has no mode bits; it relies on `%LOCALAPPDATA%\Temp` already being
  ACL'd to the user, and is marked as needing a check under F17.
- **Deleted by a short await in `open_spice`, not a background task.** Spawn
  the viewer, wait ~3s, delete, return. `remote-viewer` reads the file
  asynchronously after spawn, so there is no observable moment that is
  provably safe; a fixed delay is the simplest thing that keeps the file
  brief.
  Rejected: waiting on the viewer process to exit (needs a task alive for the
  whole console session, possibly hours, and leaks the file if pve-desk is
  killed first) and sweeping only at the next launch (a single console would
  leave the file until quit). The sweep is kept anyway as a backstop for
  crashes, since it costs nothing.
  **Unretired risk:** the 3s figure is a guess. `remote-viewer` is not
  installed on this machine, so how long it actually takes to read the file
  has never been measured. If a launch fails with a missing-file error, that
  number is why.
- **One file per console, not per process.** The name was
  `console-{pid}.vv`, one fixed path per app run, so a second console
  overwrote the first's file while the viewer might still be reading it. Now
  keyed on node and VMID.

### Linux rendering (revised 2026-10-21)
`lib.rs` defaults to `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1`, keeping WebKit's
dmabuf renderer and accelerated compositing but routing buffers through shared
memory instead of GBM. GBM is the part that fails on the NVIDIA proprietary
driver, where native Wayland dies with "Error 71 (Protocol error)" before any
window appears. Verified on this machine: clean start on Wayland, renders
correctly. **This is also what fixed console typing.**

Escape hatches, because the right path is GPU-specific:
`PVE_DESK_RENDER=legacy` restores `WEBKIT_DISABLE_DMABUF_RENDERER=1`,
`PVE_DESK_RENDER=raw` sets nothing.

### Project-level
These are in `CLAUDE.md` under **Decisions already made**, not repeated here:
the language and UI shell (Rust, Tauri 2 — *not* v1), the no-build-step
frontend, `reqwest` with rustls rather than OpenSSL, password-in-keyring with
no SSH keys, FOSS dependencies only, and Linux-primary with Windows in scope.

They live there rather than here because they are constraints that have to be
in context while code is being written, not history to look up afterwards.
One source each: a decision is in `CLAUDE.md` or in this file, never both.

---

## Cut and superseded features

**F1. First-run setup** `[-]` Superseded by F16. There is no separate
first-run screen; the same fields are the launcher's "Add a connection" form,
open by default when no connections are saved. Input validation lives in
`HostConfig::validate`.

**F8. VNC console** and **F9. LXC terminal** `[-]` Dropped 2026-10-21,
superseded by F4. The embedded Proxmox UI provides both the noVNC console and
the container shell, and they work. Only worth revisiting as part of F15.

---

## Environment facts

- The node is reached over the LAN at `192.168.1.2`. Its certificate is
  self-signed by `PVE Cluster Manager CA`, `CN=home.opah-tuna.ts.net`, valid
  to **2027-09-08**, SHA-256 beginning `F8:53:5C`. The SAN also covers
  `home`, `localhost` and the Tailscale name, so a VPN route works without a
  new certificate.
- Two nodes in the cluster. Account is `root@pam`.
- Desktop: KDE Plasma on Wayland, NVIDIA proprietary driver, AMD iGPU present
  but not driving displays.
