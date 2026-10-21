# pve-desk

A Linux desktop client for Proxmox VE, written in Rust (Tauri 2).

Status: **skeleton**. Login, VM list, power buttons and SPICE launch are
wired up but untested against a real node. See `FEATURES.md` for the
feature list and open questions.

> ⚠️ The skeleton accepts any TLS certificate (certificate pinning, F3,
> is not built yet). Only use it on your own LAN until F3 is done.

## Build dependencies (Debian / Ubuntu)

```sh
sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev librsvg2-dev \
    build-essential pkg-config virt-viewer
```

Fedora: `sudo dnf install webkit2gtk4.1-devel libsoup3-devel librsvg2-devel virt-viewer`
Arch: `sudo pacman -S webkit2gtk-4.1 libsoup3 librsvg virt-viewer base-devel`

Rust: install via [rustup](https://rustup.rs).

Optional, for `cargo tauri dev` / `cargo tauri build`:

```sh
cargo install tauri-cli --version "^2"
```

## Run

```sh
cd src-tauri
cargo run            # plain run
# or
cargo tauri dev      # with tauri-cli
```

## Layout

```
pve-desk/
├── FEATURES.md          feature list + open questions
├── ui/                  frontend (plain HTML/CSS/JS, no build step)
│   ├── index.html
│   ├── main.js
│   └── style.css
└── src-tauri/           Rust backend
    ├── Cargo.toml
    ├── tauri.conf.json
    ├── capabilities/    Tauri permission config
    ├── icons/           placeholder icon
    └── src/
        ├── main.rs      entry point
        ├── lib.rs       app setup + module map
        ├── commands.rs  functions the UI calls
        ├── api.rs       Proxmox API client (login, list, power, spiceproxy)
        ├── config.rs    ~/.config/pve-desk/config.toml
        ├── secrets.rs   password in system keyring
        ├── spice.rs     .vv file + remote-viewer
        └── error.rs     error type
```

## Where things are stored

- Settings: `~/.config/pve-desk/config.toml` (no password)
- Password: system keyring, service name `pve-desk`

## License

TBD.
