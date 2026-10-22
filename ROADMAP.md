# pve-desk — Roadmap

**Outstanding work only.** A feature leaves this file the moment it's
finished: its decisions move to `DECISIONS.md`, what it does goes in
`README.md`, and the entry here is deleted. If something isn't here, it is
either done or was cut — check `DECISIONS.md` before proposing it again,
since several approaches were tried and failed.

Status key: `[ ]` not started · `[~]` in progress / unverified

Each feature has **open questions**. These get answered one at a time before
the feature is built, and the answers go to `DECISIONS.md`.

---

## v1 (current scope)

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
  which `tauri icon` generates from `src-tauri/icons/icon.svg`.
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
3. Packaging: `msi`/`nsis`, `.ico` from `src-tauri/icons/icon.svg`.
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
- Q12.1 Which package format? The distro half is already answered: Arch /
  CachyOS, KDE Plasma on Wayland (see `DECISIONS.md`, Environment facts), so
  the daily-use answer is a `PKGBUILD` or just `cargo run`. The question is
  what to publish for everyone else — AppImage, `.deb`, or Flatpak. CI
  already builds on Debian-family and Windows runners.

---

## Open questions from shipped features

Decided-but-unfinished business, left over from features that are otherwise
done. Not big enough to be features of their own; too real to lose.

- **Q4.2** Should closing the main window quit the app, or keep it running in
  the system tray? Open since F4. Overlaps F10.
- **Q16.5** Should a saved connection be removable from the launcher?
  Deleting one means deleting its keyring password, which is destructive and
  needs a decision first.
- **"Forget this login"** — a real sign-out that clears the config entry and
  the keyring password, as opposed to today's non-destructive log out. Same
  decision as Q16.5.
- **Keyring fallback** — if the keyring is locked or unavailable, fall back to
  a password prompt. Never built; the keyring has always been available on
  this machine (see Q2.1), so it has never been exercised.

---

## Known gaps

Things that are built but unproven, so they can't be called working.

- **Windows has never been run.** It builds in CI; that is all. Q17.3 (does
  the admin panel open already signed in under WebView2, given `set_cookie`
  is WebKitGTK-verified only) is unanswered.
- **The real ~90 min ticket refresh** has never fired. Verified only with the
  threshold temporarily lowered to 10s, where two re-logins fired during an
  80-second run with no errors.
- **`session-lost` has never triggered for real** — only the code path exists.
- **Adding a second connection**, and switching between two, is untested. One
  entry is all that has ever been in the config.
- **The `.vv` file is written world-readable and never deleted** (F7). Must be
  0600 and short-lived before SPICE is ever enabled.
- **`spice.rs` has no tests**, despite `.vv` formatting being a pure function.
- **Half the app icon disappears on dark backgrounds.**
  `src-tauri/icons/icon.svg` has two paths: the X carries `fill:#e57000`, but
  the top and bottom chevrons carry no `fill` at all and so default to
  black. Against a dark panel or tray, only the orange X is visible. Raised
  2026-10-21, no fix chosen — give the first path an explicit light fill, or
  add a contrasting backplate. Blocks nothing, but the generated PNGs and
  `.ico` are built from this file, so F10 (tray) and F12 (packaging) both
  ship it. Regenerate with `tauri icon` after any change.
