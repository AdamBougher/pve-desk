# pve-desk — decisions and dead ends

Why the app is built the way it is, and what has already been tried and
failed. **Read this before changing auth, TLS, the admin panel, the refresh
model or the Linux rendering setup** — several of the ideas below were
implemented, tested against the real node, and then removed.

Companion files: outstanding work is the
[GitHub issues](https://github.com/AdamBougher/pve-desk/issues) with
`ROADMAP.md` as their map, `GLOSSARY.md` is the agreed word for each concept,
and `README.md` is what the app does today. Question numbers (Q1.1, Q3.3 …)
are kept because source comments cite them.

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
  request. The 3s is a guess, never measured.

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

Written 2026-10-21 and since exercised — `release.yml` has run green three
times (the `v1.0` tag push, then two dispatches), and `tauri-cli` 2.12.4,
`makepkg` and `xprop` are all installed on the dev machine now. The pieces:
`bundle.targets` is AppImage only, with publisher/homepage/license and a
description for the generated launcher entry; `packaging/arch/PKGBUILD`
builds from the release tarball with `cargo build --release`, runs the tests
and installs the binary, the `.desktop` entry, three icon sizes and the
licence; `packaging/pve-desk.desktop`; and
`.github/workflows/release.yml`, which on a `v*` tag builds on ubuntu-22.04
and uploads the AppImage plus a `.sha256` to a **draft** release.

Pushing `release.yml` needs a token with the **`workflow` scope**. The HTTPS
remote's token did not have it and SSH was used instead — not a code problem,
but it stops a `git push` of that file dead.

### Release tags carry three components (2026-10-22)
**Tags are `vMAJOR.MINOR.PATCH` from `v1.0.1` on.** `packaging/arch/PKGBUILD`
builds its source URL from `v$pkgver`, and `pkgver` tracks the version in
`tauri.conf.json` — which is `1.0.0`. The first release was tagged **`v1.0`**,
so the PKGBUILD asks GitHub for `v1.0.0.tar.gz` and gets a 404 (`v1.0.tar.gz`
is a 200, but extracts to `pve-desk-1.0` while the PKGBUILD `cd`s into
`pve-desk-1.0.0`). Two components cannot satisfy a three-component `pkgver`.

`v1.0` is left published rather than retagged: its `.sha256` has a download,
rewriting a public release to fix a naming slip is worse than superseding it,
and `v1.0.1` has a second job anyway — it is the first tag to run the fixed
checksum step (below).

### The v1.0 checksum names a build path (2026-10-22)
The `.sha256` published beside the v1.0 AppImage reads
`51d59bef…  src-tauri/target/release/bundle/appimage/pve-desk_1.0.0_amd64.AppImage`.
The hash matches GitHub's own digest, so the binary is intact, but
`sha256sum -c` — the command `README.md` tells people to run — cannot find a
file by that path and fails. `release.yml` was fixed in `97a1fcc` to
`( cd "$dir" && sha256sum "$file" )`, which lands **after** the `v1.0` tag
(`51c12c9`), so the fix has never run on a tag. Nothing to change in the
workflow; it needs a tag to prove it.

### The window class is `pve-desk` (Q12.1, 2026-10-23)
**Both packages name `pve-desk`, the string the app actually reports.**
`packaging/pve-desk.desktop` claimed `StartupWMClass=dev.pvedesk.app` on the
assumption — written in a comment above it — that tao sets the GTK
application id from the bundle identifier. Measured on this machine:

```
GDK_BACKEND=x11 ./src-tauri/target/release/pve-desk &
xprop -name pve-desk WM_CLASS
WM_CLASS(STRING) = "pve-desk", "Pve-desk"
```

GTK takes the class from the program name, not the identifier. So the
AppImage's generated entry (`StartupWMClass=pve-desk`, from `productName`)
was right all along and the Arch package was the one handing users a generic
taskbar icon on X11. Fixed in the `.desktop` file, one line.

Rejected: setting the GTK application id to `dev.pvedesk.app` in code so the
string is ours to pick. Everything else in the repo is already named
`pve-desk` — the entry's own basename in both packages, `Icon=pve-desk`, the
three installed icons, `/usr/bin/pve-desk` — so owning the string would mean
renaming all of it, and the AppImage's entry is generated by Tauri and could
not follow. More churn than the one-line fix, and it would leave the two
packages disagreeing anyway.

Measuring this needs no X11 session, which is what had blocked it: there is
none installed here (`/usr/share/xsessions` is empty) but XWayland runs
anyway (`DISPLAY=:0`) and KWin keeps `_NET_CLIENT_LIST` on its root, so an
X11 window is readable from the Wayland session. Use `xprop -name`, which
walks the window tree itself — bare `xprop` wants a click, and under
XWayland the click goes to the compositor, not the X server, which is why
the earlier attempt found no window to read.

The Wayland `app_id` is **also `pve-desk`** — measured 2026-10-24, which was
the last unknown here. KWin exposes no window-list over plain D-Bus
(`queryWindowInfo` wants a click), but a KWin script does, and it is
read-only:

```js
// loaded via: qdbus6 org.kde.KWin /Scripting loadScript <file> probe
for (const w of workspace.windowList())
  print(w.resourceClass + " / " + w.resourceName);
```

```
kwin_wayland[2284]: class=pve-desk name=pve-desk caption=pve-desk
```

So both display servers report the same string, as predicted — GTK derives
`app_id` and the `WM_CLASS` instance name from the program name. The
consequence is specific: Wayland icon-matching keys `app_id` against a
desktop file **basename**, so the Arch package's `pve-desk.desktop` matches
exactly, while an integrated AppImage entry named
`pve-desk_1.0.0_amd64.desktop` cannot match at all. An AppImage user's dock
icon therefore comes from the entry's own `Icon=` key and the window's
embedded icon, not from window-to-launcher matching — which is why that
combination still looks right, and why `StartupWMClass` plays no part in it
on a Wayland session.

### A stale AppImage dock entry is documented, not fixed (Q12.2, 2026-10-24)
**README names the two files to delete; the bundle does nothing.** AppImage
desktop integration writes
`~/.local/share/applications/pve-desk_<version>_amd64.desktop` and an icon
beside it, both named after the AppImage's filename — so an upgrade can leave
the old entry in place, still carrying the pre-fix icon and an `Exec` pointing
at an AppImage that has been deleted. That is exactly how a stale icon came to
be photographed here as if it were F20's fix.

Not ours to fix in the bundle: the naming is done by `appimaged` or the
AppImageLauncher prompt, not by anything this repo ships, and we cannot rename
a file we never write. Nor is it reproducible on demand — running the
published v1.0.1 AppImage created no second entry, because whether anything
gets integrated at all depends on the user's desktop. Shipping a workaround
for an unreproduced problem in someone else's component is how you get code
nobody can test.

But a dead launcher entry is a real trap with a two-line cure, so README's
AppImage section carries the `rm` commands. The Arch package needs no note: it
installs unversioned `pve-desk.desktop` and `pve-desk.png` and replaces them
in place.

### The Arch package needs `options=(!lto)` (F12, 2026-10-23)
**Without it the release build does not link.** `makepkg.conf` ships
`OPTIONS=(... lto ...)` with `LTOFLAGS="-flto=auto"`, so `ring`'s build script
compiles its C sources into GCC GIMPLE objects. `rust-lld` cannot read those,
so every C symbol in `libring.a` is undefined at link time: a dozen
`ring_core_0_17_14__*` misses (`aes_nohw_encrypt`, `x25519_sc_reduce`,
`CRYPTO_memcmp`, …), then "A failure occurred in build()", exit 4.

Not a toolchain mismatch — `RUSTUP_TOOLCHAIN=stable` resolves to the same
1.97.1 the repo pins. `options=(!lto)` is what Rust packages on Arch
conventionally carry. Measured both ways against the published v1.0.1 tarball
on 2026-10-23: fails without it, builds with it.

The `v1.0.1` tarball still contains the pre-fix PKGBUILD. Harmless, and not
worth a retag: the PKGBUILD is used from the repo or the AUR and pulls the
tarball by version, so it is never the copy inside the tarball that runs.

### F12 is finished except on a second machine (2026-10-24)
**`makepkg -si` is verified; the AppImage has still never started on a machine
without Rust.** The install half ran on 2026-10-24: exit 0,
`pve-desk 1.0.1-1`, and all six payload paths present — the binary,
`pve-desk.desktop`, 32x32 + 128x128 + scalable icons, and the LICENSE. The
installed entry is byte-identical to `packaging/pve-desk.desktop`, which is
what finally let the window-class scenario close on the file it actually
names.

A method note worth keeping, because the first attempt produced a false pass.
With the package not installed, the only menu entry naming pve-desk was the
AppImage's integrated `pve-desk_1.0.0_amd64.desktop`. Launching from it shows
an icon — from that entry's own `Icon=` key — while proving nothing about
`StartupWMClass` or about `packaging/`, since a versioned basename cannot
match `app_id`. **Clear the integrated entries before measuring the taskbar;
it is part of the measurement, not tidiness.**

One correction to the Q12.2 record above: by 2026-10-24 that stale entry was
not the trap it was first described as. Its icon hashed `eb8b4dbc…`, identical
to the current `src-tauri/icons/icon.png`, and its `Exec` target still
existed. README's warning stands as a general warning about integrated
entries, not as a description of this one.

The Wayland `app_id` was read twice more the same day (13:16:08 and 13:36:59),
both `class=pve-desk name=pve-desk`, which agrees with the KWin-script reading
already recorded. `xprop` could **not** be re-run: the session is
Wayland-native and there is no X11 window to read. The string-match clause
therefore rests on the 2026-10-23 XWayland reading, still valid because
`packaging/pve-desk.desktop` has not changed since.

What is unproven and moved to #30: this desktop has glibc 2.44 and a full Rust
toolchain, so it disqualifies itself as the portability test. A Guest on the
node is the cheapest second machine; none existed when F12 closed.

`.gitignore` gained `/packaging/arch/{src,pkg}/` and the two archive globs at
the same time. README tells Arch users to run `makepkg -si` in that directory,
so the documented path left 1.5G of `src/` untracked in the tree — anyone
following README hit it, not just this run.

### The icon's chevrons carry an explicit light fill (F20, 2026-10-23)
`src-tauri/icons/icon.svg` gave the X `fill:#e57000` but left the top and
bottom chevrons with **no fill at all**, so they defaulted to black. On a dark
panel or a dark tray only the orange X showed — half the mark invisible on
exactly the desktops this app is used on.

**Fixed with a light fill (`#f2f2f2`) on the chevrons, not a backplate.** One
attribute, the mark keeps its shape, and the background stays transparent,
which sits better in a tray than a plate does. The trade-off was accepted
knowingly: on a *light* desktop the chevrons go faint and the orange X carries
the mark. Checked by compositing `128x128.png` over `#1c1c1c`, `#2e3436` and
`#ffffff` — both halves read on the two dark grounds, faint on white.

Confirmed on a real dock 2026-10-24: the pale mark showed correctly in the
Plasma panel at login. The icon that resolved was verified to be the current
one by hash (`icon.png` and the integrated entry's 256px raster are both
`eb8b4dbc…`), which matters because an earlier sighting had photographed a
*stale* pre-fix icon from the v1.0 AppImage's integrated entry and read it as
the fix.

**Regenerating the rasters:** `cargo tauri icon` emits far more than this tree
carries — an `.icns`, Windows Store logos, Android and iOS sets. It was run
into a temp directory and the five tracked rasters copied back selectively, so
the extra platforms stay out of a repo that never had them. Do it that way
again rather than letting it write in place.

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

### SPICE: what actually gates it (F7, 2026-10-21)
**Q7.1 Parked until there is a graphical desktop VM**, not cut. No guest here
is a desktop, so SPICE would buy nothing that noVNC in the admin panel does
not already do; one is planned, so this is deferred. It unblocks the moment
such a VM exists.

**Don't confuse two Proxmox settings** — this cost a round of questions. The
**Console** dropdown (noVNC / SPICE / xterm.js) is how you *view* a guest and
always offers all three. Whether SPICE works depends on **Hardware →
Display**. noVNC works on any display type; xterm.js needs a serial port on a
VM (containers get one free); only SPICE needs a local viewer binary, which
is the entire reason F7 exists.

**Q7.2 was never an open question** — answered by reading `config.rs`.
`remote_viewer_path` already defaults per-OS (`/usr/bin/remote-viewer` on
Linux, `remote-viewer.exe` on Windows) and is overridable in the config file.

Prerequisites, in order, when a desktop VM exists: install `virt-viewer`
(which is also what makes the launch testable at all); set that VM's
Hardware → Display to `SPICE (qxl)`; add the button for qualifying rows only;
then measure the real read window and replace the 3s guess in `spice.rs`.

### Cross-platform findings (F17, 2026-10-21)
Verified by reading the vendored sources, not assumed:

- **`set_cookie` exists on all four wry backends** (webkitgtk, wkwebview,
  webview2, android). The admin panel's "opens already signed in" trick is
  portable as-is — the feature most at risk, and it is fine.
- The keyring crate has `windows-native` and `apple-native` features, so
  `secrets.rs` is unchanged; only the `Cargo.toml` feature table moves.
  `Cargo.toml` currently pins `sync-secret-service` (Linux only).
- Tauri hands out a *typed* `ICoreWebView2Controller` on Windows, but only
  `*mut c_void` for WKWebView on macOS.
- **wry has no server-certificate handling on any platform.** The Linux
  pinning reaches past wry into `webkit2gtk` directly; there is no
  cross-platform seam to extend, so each platform would be bespoke.
- **`reqwest` is configured with `rustls-tls`, which uses `webpki-roots`**
  (compiled-in Mozilla roots), not the OS trust store. So on Windows/macOS,
  importing the Proxmox CA would make the *webview* trust the node while the
  *API client* still refused it. The CA-install escape hatch only solves half
  the problem on any platform.

Decisions: **Q17.1 Windows first, macOS later** — there is a Windows laptop
to test on, which is what makes it worth doing. **Q17.2 Native roots plus a
CA import, not per-platform pinning**: `reqwest` switches from `webpki-roots`
to `rustls-tls-native-roots` so the API client reads the OS trust store, and
importing the cluster CA then satisfies both halves. Linux keeps its existing
in-app pinning, which already works and needed no CA install.

Mechanical when it happens: per-target keyring features; a per-OS
`remote-viewer` default; `cfg(unix)` on the `.vv` permission work (Windows
has no mode bits and would need ACLs or a per-user temp dir); `cfg`-gate the
NVIDIA render workaround; `msi`/`nsis` and `dmg` bundle targets with `.ico`
and `.icns` from `src-tauri/icons/icon.svg`.

**The real constraint is testing, not code.** Every interesting failure found
on 2026-10-21 — NVIDIA DMABUF, console rendering, pveproxy's 3s delay — only
showed up on real hardware. A Windows build nobody can run is a build nobody
can call working. Everything except pinning is about half a day of config and
build plumbing; pinning on all three platforms is days, most of it interop
that cannot be verified without the hardware.

### Dev container (F18, 2026-10-21)
Tier 1 (build and test, no GUI) built and verified the same day: `cargo fmt
--check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` all run
clean inside the image. Clippy compiling the `webkit2gtk` crate there proved
Debian's webkit carries the `v2_40` features the certificate pinning needs —
the main risk in this feature, retired.

- **Q18.1 Tier 1 now, Tier 2 staged.** The display / D-Bus / `/dev/dri`
  wiring is written but commented out in `devcontainer.json`.
- **Q18.2 Debian 13 (trixie).** Matches the bundle target *and* ships
  webkit2gtk-4.1 **2.54.0** against the host's 2.54.1 — measured, not
  assumed. Arch was not needed.
- **Q18.3 `.devcontainer/` plus a plain Containerfile.** Works with VS Code
  Dev Containers (set `dev.containers.dockerPath` to `podman`) and the same
  Containerfile is reusable for CI unchanged.
- **Q18.4 Local for now, CI-ready.** No CI config added.
- Not Debian's `rustc` (1.85.1, far behind this project's 1.97.1): Rust comes
  from rustup and `rust-toolchain.toml` pins 1.97.1 for host and container
  alike, so they cannot drift. `libssl-dev` is deliberately absent — this
  project uses rustls and must not acquire an OpenSSL dependency.

**Demoted the same day it was built**, so this isn't re-litigated: CI on
`ubuntu-latest` already does the container's main job (a clean Debian-family
machine installing the documented dependency list from scratch on every
push); the container cannot run the app, and this is a GUI client — every bug
found on 2026-10-21 needed the GUI on real hardware; `rust-toolchain.toml`
pins the compiler either way; and it does nothing for anyone *installing*
pve-desk. Kept because it works, costs a few files, and is useful for offline
or pre-push checks against Debian. The README does **not** claim CI uses this
image; CI installs deps with apt directly.

**Honest caveat:** GPU and keyring behaviour *inside* a container differs
from the host, so a green container run is not evidence the app works on a
real desktop. Keep verifying on the host.

### The roadmap is Gherkin (2026-10-22)
`ROADMAP.md` is written as Gherkin feature files: tags for the pass and
status, a `Feature:` block per outstanding feature, `Scenario:` blocks as its
acceptance criteria, and `#` comments for its open questions. Asked for by
the user; chosen in the "pure Gherkin, prose trimmed hard" form, which is why
the measurements that used to pad that file are in the sections above.

- **Scenarios are the spec, not a suite.** There is no `cucumber-rs` in the
  tree. Most scenarios need a live node or a GUI, which is exactly what
  cannot be automated here; the pure parts are already unit-tested in
  `src-tauri/src/*.rs`. Whether to wire them up is an open question in the
  roadmap, not a decision.
- **A feature leaves the file when its scenarios pass by hand.** Same rule as
  before: decisions here, behaviour in `README.md`, entry deleted.
- **Version passes replaced the old "v1 / Later" split**: v1 is the shipped
  app plus packaging, v2 is performance and the Home view, v3 is Home as a
  first-pass replacement for Admin (logs, shell, options).

### The features moved to GitHub issues (2026-10-24)
The 21 outstanding `Feature:` blocks and the "Shipped but unproven" block left
`ROADMAP.md` and became issues #9–#30 in `AdamBougher/pve-desk`, one per
feature. Asked for by the user while setting up the mattpocock engineering
skills, which key off an issue tracker: `/triage`, `/to-spec`,
`/implement-spec` and `/code-review` all read a ticket, and there was nothing
for them to read.

- **Full move, not a mirror.** The whole Gherkin block is the issue body,
  verbatim in a ```gherkin fence — narrative, `#` open questions, scenarios
  and the measurement notes. Chosen over keeping the spec in the file with
  issues as a thin queue, and over mirroring both, which would have drifted
  within a week.
- **The cost, accepted knowingly:** the spec is no longer greppable or
  diffable. Those 1060 lines held measurements (`3.005s / 401` vs `8ms / 200`,
  `xprop` reporting `"pve-desk", "Pve-desk"`) that one `grep` used to find and
  `git log` used to track. Now it is `gh issue list --json body` and the
  issue's own edit history.
- **`ROADMAP.md` survives as the map**, 106 lines: the passes, which issues
  are in them, and the F-number rule. Not the spec.
- **F-numbers are not issue numbers** and never will be. The `F<n>` is
  permanent and is how features cite each other; the `#number` is only for
  links. F12 is issue #9.
- **Labels replaced the tags** one for one — pass, state and qualifier. The
  five canonical triage labels exist but were deliberately **not** applied
  during the migration: these features already carry specs and are not
  incoming requests awaiting evaluation.
- **F35 was deleted, not migrated.** `ROADMAP.md` cited it twice as the
  feature leading v2 ("makes the scenarios run") and pointed at a *The
  scenarios become executable* section here that does not exist. There was no
  `Feature:` block for it anywhere and no spec to migrate. Since the section
  above already records that wiring up `cucumber-rs` is an open question
  rather than a decision, the user chose to drop both references. If it comes
  back it needs a spec first.
- **The scenarios are still not a suite.** The move changed where they live,
  not what they are. Nothing executes them; "done" still means someone ran
  them against the real node by hand.

### The vocabulary is fixed, and the on-disk names are not (2026-10-24)
Agreed in a `/grill-with-docs` session the same day as the issue migration.
`GLOSSARY.md` is the output: 13 terms, each with the words to avoid. It holds
words only — this section holds the decisions behind them.

- **Connection** is a saved login: name, address, port, username, realm,
  pinned fingerprint. It is a stored record; a **Session** is the live
  authenticated link, and a **Node** is the Proxmox machine. **Host** is only
  the address field inside a Connection and never the record itself.
- **The on-disk names stay wrong on purpose.** `Config.hosts`,
  `HostConfig` and `default` keep their names even though the agreed terms
  are Connection and Default Connection. Renaming them changes the
  `config.toml` schema, which breaks every existing config and needs a
  migration, and buys no behaviour. Expect the mismatch; it is not drift to
  tidy up. `error.rs` already showed the user "this connection is signed in",
  so the user-facing word was never in doubt.
- **`kind` is a spelling, not a term.** The domain word is **Type**
  (`qemu` or `lxc`) — what the table column says, what the Proxmox API field
  is called, and what F34 is named after. The Rust field is `kind` only
  because `type` is a reserved word. Do not "correct" the UI label or the
  issue title to match the Rust field.
- **Never sweep the word "entry".** It has four unrelated and correct senses
  in this tree: the `keyring` crate's own `entry()` function in `secrets.rs`,
  `std::fs` directory entries in `spice.rs`, the "desktop entry" of
  packaging, and the cluster's "entry-point node" in `config.rs`. Only about
  24 doc comments ever meant a Connection, and those are already fixed. A
  find-and-replace here corrupts working code and user-facing packaging prose.
- **Guest is the union** — a VM (`qemu`) or a Container (`lxc`). "VM" alone
  never means both. A **Template** is a Guest that cannot be started: the
  domain includes it, and Home hides it. The filter stays in
  `ui/main.js` (Q5.3) rather than `api.rs`, so the backend keeps telling the
  truth about what `/cluster/resources` returns and F26's filters keep the
  option of showing them.
- **Home is a planned name.** It appears nowhere in the code; the tab still
  reads "Guests" until F21 (#10). Five issues are written in terms of it, so
  it is in the glossary now, marked as planned.
- **Ticket is never a unit of work.** It is the Proxmox credential a Session
  holds. The collision only became live when outstanding work moved into
  GitHub issues — work items are **issues**, and a feature is an
  **F-number**.
- **What was deliberately not done:** no identifier or test renames. The
  sweep was prose only, so `fn remove_errors_on_an_unknown_entry` still
  carries the old word and still passes. Renaming identifiers is a separate
  job nobody has asked for.

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
