# pve-desk

A Linux desktop client for Proxmox VE, written in Rust (Tauri 2).

I wanted to be able to access my Proxmox admin GUI from a desktop app. I used
to use Sakakun's [Proxmox-Desktop-Client](https://github.com/sakakun/Proxmox-Desktop-Client)
for Windows, and I wanted a similar experience on Linux.

Status: **working for daily use, with caveats.** Connecting, the embedded
admin panel and its consoles are verified against a real Proxmox node. See
`ROADMAP.md` for what is left, and `DECISIONS.md` for why things are built
the way they are.

Certificates are verified: a node's certificate must either validate against
the system trust store or match the fingerprint you confirmed when the
connection was first set up. Nothing in the app accepts a certificate blindly.

## What works

- **Connection launcher.** The app always opens on a list of saved
  connections. Each shows whether the node is reachable, and whether the
  session ticket is still valid. No credentials are sent by the reachability
  check.
- **Saved logins.** Password lives in the system keyring; host, port,
  username and realm in a config file. Several connections can be saved;
  one is live at a time.
- **Embedded admin panel.** The full Proxmox web UI in its own window,
  already signed in — no login form. Opens automatically on connect.
- **Certificate verification.** The API client checks the node against the
  OS trust store first and falls back to the fingerprint you confirmed, so a
  real certificate (Tailscale, Let's Encrypt, or a CA you imported) works
  and survives renewal, while Proxmox's self-signed default is pinned. An
  unknown certificate stops the connection and shows you its SHA-256 before
  anything is sent. The admin panel webview pins the same fingerprint.
- **Consoles.** Shell and container/VM consoles work inside the panel.
- **Guest list.** Read-only summary of VMs and containers, refreshed every
  30s, with a search box over name, VMID, node and tag. Templates are hidden.
- **Session refresh.** Proxmox tickets last ~2 hours. A session older than
  ~90 minutes is renewed before the next call, and a ticket the node rejects
  anyway triggers one silent re-login and a retry. If that also fails, the
  app returns you to the launcher and says why.
- **Log out.** Drops the session and returns to the launcher. Saved
  credentials are kept.

## What isn't done

- **SPICE (F7)** is written but has never run — `remote-viewer` isn't
  installed here. The `.vv` file it writes is not yet mode 0600, which it
  must be before this is used.
- **Removing a saved connection** isn't implemented (it means deleting a
  keyring entry, which is a decision still open).
- **Native power controls** were cut: the admin panel already does this
  better. The backend remains if a native UI is ever revisited (F15).
- **Linux only** by verification. Windows builds in CI but has never been
  run. See F17.

`ROADMAP.md` keeps a "Known gaps" list of what is built but still unproven.

## Building

Install the dependencies for your distro, then `cargo run`. This is the route
you want: it's also what lets you actually launch the app.

Arch / CachyOS:

```sh
sudo pacman -S webkit2gtk-4.1 libsoup3 librsvg virt-viewer base-devel
```

Debian / Ubuntu:

```sh
sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev librsvg2-dev \
    build-essential pkg-config virt-viewer
```

Fedora:

```sh
sudo dnf install webkit2gtk4.1-devel libsoup3-devel librsvg2-devel \
    gcc gcc-c++ make pkgconf-pkg-config virt-viewer
```

Rust via [rustup](https://rustup.rs) — `rust-toolchain.toml` pins the version,
so cargo installs the right one on first build. `virt-viewer` is only needed
for SPICE.

You also need a **Secret Service** keyring running (GNOME Keyring, KWallet,
or KeePassXC). Without one the password can't be stored and saved logins
won't work.

Optional, for `cargo tauri build` (packaging):

```sh
cargo install tauri-cli --version "^2"
```

### Optional: build in a container

`.devcontainer/` holds a pinned Debian 13 image, useful for one narrow thing:
checking the build and tests on the distro family the `.deb` target is built
for, without installing anything on your host.

```sh
podman build -f .devcontainer/Containerfile -t pve-desk-dev .
podman run --rm --userns=keep-id -v "$PWD":/workspace \
    -e CARGO_TARGET_DIR=/tmp/ctarget pve-desk-dev \
    bash -lc 'cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test'
```

VS Code users: *Dev Containers: Reopen in Container*, after setting
`dev.containers.dockerPath` to `podman`.

It is **not** the main build route and you don't need it:

- **It can't run the app.** Display and session-D-Bus passthrough are written
  but commented in `.devcontainer/devcontainer.json`; without the D-Bus
  socket there's no Secret Service, so no keyring and no saved logins.
- **Build output stays in a container volume**, so container and host builds
  don't invalidate each other — meaning the binary it produces isn't on your
  host.
- CI already covers the same ground on a clean machine, so this is mostly for
  offline or pre-push checks.

## Run

```sh
cd src-tauri
cargo run
```

### Rendering workaround

WebKitGTK's dmabuf renderer allocates GBM buffers, which fails on the NVIDIA
proprietary driver — on Wayland the app dies with
`Error 71 (Protocol error)` before a window appears, and consoles render
badly. The app therefore sets `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` by
default, keeping accelerated compositing but routing buffers through shared
memory.

Override if your GPU wants something else:

```sh
PVE_DESK_RENDER=legacy cargo run   # disable the dmabuf renderer entirely
PVE_DESK_RENDER=raw cargo run      # stock WebKit, no workaround
```

This is Linux-only and has no effect on other platforms.

## Platforms

Linux is the only platform verified by hand. Windows is in scope and builds
in CI (`.github/workflows/ci.yml`) but has never been run; macOS is planned.
See F17 in `ROADMAP.md`.

## Layout

```
pve-desk/
├── CLAUDE.md            working rules + Proxmox API notes
├── .devcontainer/       pinned build environment (Debian 13 + Rust 1.97.1)
├── rust-toolchain.toml  toolchain pin, shared by host and container
├── ROADMAP.md           outstanding work + open questions
├── DECISIONS.md         why it is built this way, and what failed
├── .claude/skills/      project skills (Tauri 2 reference, feature workflow)
├── ui/                  frontend (plain HTML/CSS/JS, no build step)
│   ├── index.html
│   ├── main.js
│   └── style.css
└── src-tauri/           Rust backend
    ├── Cargo.toml
    ├── tauri.conf.json
    ├── capabilities/    Tauri permission config
    ├── icons/           icon.svg + generated PNG sizes
    └── src/
        ├── main.rs      entry point
        ├── lib.rs       app setup, shared state, render workaround
        ├── commands.rs  functions the UI calls + admin panel / cert pinning
        ├── api.rs       Proxmox API client (login, list, power, spiceproxy)
        ├── config.rs    ~/.config/pve-desk/config.toml
        ├── secrets.rs   password in system keyring
        ├── spice.rs     .vv file + remote-viewer
        └── error.rs     error type
```

## Where things are stored

- Settings: `~/.config/pve-desk/config.toml` — host, port, username, realm,
  pinned certificate fingerprint. **Never the password.**
- Password: system keyring, service name `pve-desk`, one entry per
  `user@realm@host`.

## Development

All three must pass clean before any change is done — on the host from
`src-tauri/`:

```sh
cargo fmt
cargo clippy -- -D warnings
cargo test
```

or in the container, if you'd rather not install the dependencies:

```sh
podman run --rm --userns=keep-id -v "$PWD":/workspace \
    -e CARGO_TARGET_DIR=/tmp/ctarget pve-desk-dev \
    bash -lc 'cd src-tauri && cargo fmt --check \
        && cargo clippy --all-targets -- -D warnings && cargo test'
```

CI (`.github/workflows/ci.yml`) runs the same checks on `ubuntu-latest` and
`windows-latest` using the dependency list above — not the container image.

Login, the guest list, the admin panel and SPICE can only be truly verified
against a real Proxmox node.

Test coverage is thin but no longer confined to one file: 16 unit tests, 13 in
`config.rs` (schema round-trip, defaults, validation, entry lookup) and 3 in
`api.rs` (parsing a sample cluster-resources response, tolerating absent
optional fields, and checking the payload field names the frontend reads).
`.vv` formatting in `spice.rs` still has none, despite being a pure function
that should.

## License

TBD.
