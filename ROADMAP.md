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

### F14. One window with tabs
`[~]` Fold the two windows into one: a slim local tab bar with **Admin** (the
Proxmox UI) and **Guests** (the F5 list), instead of a separate panel window.

Asked for 2026-10-21: "keeping one window that has tabs would be nice", with
the login window kept around and the running list living in it.

The original plan was an `<iframe>`, which would have put a remote origin
inside our own page and broken F3's certificate pinning. That was rejected in
favour of a second webview in the same window; the reasoning, and the iframe's
specific problems, are in `DECISIONS.md`.

Still true and still useful if the iframe is ever revisited: Proxmox sends no
`X-Frame-Options` and no CSP, so framing itself is allowed (checked
2026-10-21).

Built 2026-10-21 as a **child webview**, not an iframe — see `DECISIONS.md`,
"One window with tabs", for why and for what that rules out.

- Q14.1 / Q14.2 **moot, not answered.** Both existed only because of the
  iframe design. The Proxmox page is still a main-frame load, so F3's pinning
  works unchanged and no certificate PEM needs storing. Q14.1 (does
  `load-failed-with-tls-errors` fire for subframes?) is still genuinely
  unknown — the GIR docs say only "when a TLS error occurs during a load
  operation" — and would have to be tested if anyone revisits the iframe.

Took three attempts, because GTK refuses both of the obvious approaches:
a child webview can be neither positioned nor capped to a fixed height. The
measurements are in `DECISIONS.md`. The tab bar is now a native GTK strip on
Linux, with one webview visible at a time; Windows and macOS keep the page's
own tab bar.

Unverified: the native strip, tab switching, and whether the panel still
opens signed in. `[~]` until seen running.

Rough edge: the strip is visible on the launcher too, so clicking **Admin**
before connecting reports an error rather than being disabled.

---

## Later (not in v1)

### F7. SPICE console
`[~]` `POST /nodes/{node}/qemu/{vmid}/spiceproxy` → write a temporary `.vv`
file → launch `remote-viewer` with it → delete the file after launch.

Parked, not cut: `remote-viewer` is not installed on this desktop, so it has
never run. It is also **unreachable** — nothing in `ui/` calls `open_spice`;
the button went with F6's power controls.

Hardened 2026-10-21 (`.vv` handling), which was the part that had to be right
before this could ever be switched on: mode 0600 at creation, a distinct file
per launch, server-supplied node names kept out of the path, stale files swept
only when genuinely old, and deletion after a short wait. Six tests in
`spice.rs`, including one asserting the created file's mode. The launch path
itself is still unexercised.

**Q7.1 answered 2026-10-21: parked until there is a desktop VM.** No guest
today is a graphical desktop, so SPICE would buy nothing that noVNC in the
admin panel doesn't already do — but one is planned, so this is deferred,
not cut. It unblocks the moment such a VM exists.

Prerequisites when that happens, in order:
1. Install `virt-viewer` on the desktop (`remote-viewer` is not present; it
   is also what makes the launch testable at all).
2. Set that VM's **Hardware → Display** to `SPICE (qxl)`. Without it the
   SPICE console cannot connect, whatever the console menu offers.
3. Add a SPICE button to the guest list, on qualifying rows only.
4. Measure the real read window and replace the 3s guess in `spice.rs`.

Don't confuse two Proxmox settings when checking this (it cost a round of
questions): the **Console** dropdown — noVNC / SPICE / xterm.js — is how you
*view* a guest and always offers all three. Whether SPICE works depends on
**Hardware → Display**. noVNC works on any display type; xterm.js needs a
serial port on a VM (containers get it free); only SPICE needs a local
viewer binary, which is the whole reason F7 exists.

**Q7.2 was never an open question** — answered by reading `config.rs`.
`remote_viewer_path` already defaults per-OS (`/usr/bin/remote-viewer` on
Linux, `remote-viewer.exe` on Windows) and is overridable in the config file.
No decision needed.


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
- **The 3-second `.vv` read window is a guess** (F7). `remote-viewer` isn't
  installed here, so how long it actually needs to read the file before
  deletion has never been measured. A launch failing with a missing-file
  error is the symptom.
- **Spawned viewers are never reaped.** `launch` drops the `Child` without
  waiting, so on unix each closed console leaves a zombie until pve-desk
  exits. Pre-existing, harmless at the scale of a few consoles, but it is a
  leak.
- **Half the app icon disappears on dark backgrounds.**
  `src-tauri/icons/icon.svg` has two paths: the X carries `fill:#e57000`, but
  the top and bottom chevrons carry no `fill` at all and so default to
  black. Against a dark panel or tray, only the orange X is visible. Raised
  2026-10-21, no fix chosen — give the first path an explicit light fill, or
  add a contrasting backplate. Blocks nothing, but the generated PNGs and
  `.ico` are built from this file, so F10 (tray) and F12 (packaging) both
  ship it. Regenerate with `tauri icon` after any change.
