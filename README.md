# pve-desk

A Linux desktop client for Proxmox VE, written in Rust (Tauri 2).

I wanted to be able to access my Proxmox admin GUI from a desktop app. I used
to use Sakakun's [Proxmox-Desktop-Client](https://github.com/sakakun/Proxmox-Desktop-Client)
for Windows, and I wanted a similar experience on Linux.

Status: **working for daily use, with caveats.** Connecting, the embedded
admin panel and its consoles are verified against a real Proxmox node. See
`FEATURES.md` for the full feature list, decisions and open questions.

> ⚠️ **The API client still accepts any TLS certificate.**
> The embedded admin panel pins the node's certificate (F3), but the Rust
> HTTP client — the one that carries your password and session ticket — does
> not yet. Use it on a network you trust until that half of F3 is done.

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
- **Certificate pinning (webview).** On first connect the node's certificate
  is pinned to its SHA-256 and allowed for that host only. A certificate
  that later changes is refused.
- **Consoles.** Shell and container/VM consoles work inside the panel.
- **Guest list.** Read-only summary of VMs and containers, refreshed every
  30s.
- **Log out.** Drops the session and returns to the launcher. Saved
  credentials are kept.

## What isn't done

- **Ticket refresh.** Proxmox tickets last ~2 hours and are not yet renewed,
  so a long-running session eventually starts failing. Reconnect from the
  launcher.
- **SPICE (F7)** is written but has never run — `remote-viewer` isn't
  installed here. The `.vv` file it writes is not yet mode 0600, which it
  must be before this is used.
- **Removing a saved connection** isn't implemented (it means deleting a
  keyring entry, which is a decision still open).
- **Native power controls** were cut: the admin panel already does this
  better. The backend remains if a native UI is ever revisited (F15).
- **Linux only.** See F17 for what cross-platform would take.

## Requirements

Arch / CachyOS:

```sh
sudo pacman -S webkit2gtk-4.1 libsoup3 librsvg virt-viewer base-devel
```

Debian / Ubuntu:

```sh
sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev librsvg2-dev \
    build-essential pkg-config virt-viewer
```

Fedora: `sudo dnf install webkit2gtk4.1-devel libsoup3-devel librsvg2-devel virt-viewer`

Rust via [rustup](https://rustup.rs). `virt-viewer` is only needed for SPICE.

You also need a **Secret Service** keyring running (GNOME Keyring, KWallet,
or KeePassXC). Without one the password can't be stored and auto-connect
won't work.

Optional, for `cargo tauri dev` / `cargo tauri build`:

```sh
cargo install tauri-cli --version "^2"
```

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

## Layout

```
pve-desk/
├── CLAUDE.md            working rules + Proxmox API notes
├── FEATURES.md          feature list, decisions, open questions
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

From `src-tauri/`, all three must pass clean before any change is done:

```sh
cargo fmt
cargo clippy -- -D warnings
cargo test
```

Login, the guest list, the admin panel and SPICE can only be truly verified
against a real Proxmox node.

Test coverage is currently lopsided: all 13 unit tests live in `config.rs`
(schema round-trip, defaults, validation, entry lookup). `.vv` formatting in
`spice.rs` and response parsing in `api.rs` have none yet, despite both being
pure functions that should.

## License

TBD.
