---
name: tauri2
description: Tauri 2 API facts for pve-desk — read BEFORE writing or changing any Rust that touches tauri::, any tauri.conf.json key, any file in capabilities/, or any frontend call to window.__TAURI__. Tauri 1 and Tauri 2 differ in exactly these places and v1 habits compile-fail or silently widen permissions. Covers v1→v2 renames, the capability/permission format, command signatures, WebviewWindowBuilder, and the cookie API needed for the F4 admin panel.
---

# Tauri 2 in this project

Pinned version: **tauri 2.12.4** (`src-tauri/Cargo.lock`). Everything below was
checked against the v2 docs and docs.rs for that version on 2026-10-20.

**If you are about to use a Tauri API not listed here, check docs.rs for 2.12.4
rather than recalling it.** Most v1 answers in circulation still compile-look
plausible and are wrong.

## The authoritative local source

`cargo check` generates `src-tauri/gen/schemas/desktop-schema.json`. That file
lists **every valid permission identifier and capability field** for this exact
dependency set. When a permission name is in doubt, grep that file — don't guess
and don't trust this skill over it. It does not exist until a build has run.

## v1 → v2 renames (the ones this project can hit)

Rust:

| v1 | v2 |
|---|---|
| `Window` | `WebviewWindow` |
| `WindowBuilder` | `WebviewWindowBuilder` |
| `WindowUrl` | `WebviewUrl` |
| `Manager::get_window()` | `Manager::get_webview_window()` |

Frontend:

- `@tauri-apps/api/tauri` → `@tauri-apps/api/core`
- `@tauri-apps/api/window` → `@tauri-apps/api/webviewWindow`
- Global is `window.__TAURI__.core.invoke` (v1: `window.__TAURI__.invoke`).
  Requires `app.withGlobalTauri: true` — already set in `tauri.conf.json`.
  This project uses the global, not the npm package (no Node, no bundler).

`tauri.conf.json`:

- `allowlist` **does not exist** — replaced entirely by capabilities.
- `tauri` key → `app`
- `build.distDir` → `build.frontendDist`; `build.devPath` → `build.devUrl`
- `build.withGlobalTauri` → `app.withGlobalTauri`
- `package.productName` / `package.version` / `bundle.identifier` → root level
- `systemTray` → `trayIcon` (relevant to F10)
- `windows.fileDropEnabled` → `windows.dragDropEnabled`
- `bundle.deb` → `bundle.linux.deb`; `bundle.appimage` → `bundle.linux.appimage`
  (relevant to F12). `bundle.targets` stays where it is.

Removed Cargo features that no longer exist: `reqwest-client`,
`process-command-api`, `shell-open-api`, `updater`, `system-tray`.

## Capabilities and permissions

Files live in `src-tauri/capabilities/*.json`. **All capability files in that
directory are enabled automatically.** Once `app.security.capabilities` lists
any identifiers, only those are used. This project does not list them, so
adding a file is enough to grant what it says — there is no second step that
would catch a mistake.

Fields: `$schema`, `identifier`, `description`, `windows` (array of labels,
`"*"` wildcard), `webviews`, `permissions`, `platforms`
(`["linux","macOS","windows","iOS","android"]`), and `remote: { urls: [...] }`.

Permission identifiers are `core:<domain>:<action>` or `<plugin>:<action>`,
e.g. `core:window:allow-set-title`, `core:path:default`.

`core:default` expands to nine permissions: `core:app:default`,
`core:event:default`, `core:image:default`, `core:menu:default`,
`core:path:default`, `core:resources:default`, `core:tray:default`,
`core:webview:default`, `core:window:default`.

The `*:default` sets are query/info operations. Actions are separate and opt-in:
`core:window:allow-close`, `core:window:allow-create`,
`core:window:allow-set-size`, `core:webview:allow-create-webview`,
`core:app:allow-exit`, `core:webview:allow-print`.

### Rule for this project

`CLAUDE.md`: new permissions must be the narrowest that works, and **the admin
panel window must not get access to the app's commands.** In practice:

- Keep `capabilities/default.json` scoped to `"windows": ["main"]`. It currently
  grants `core:default` only.
- Give a new window its own capability file naming only that label, or no
  capability at all. A window no capability mentions can call no commands.
- Never add `remote: { urls: [...] }` for the Proxmox host. That field is the
  mechanism for handing IPC to a remote origin, which is exactly the thing the
  hard rule forbids. There is no reason F4 needs it — the admin panel is the
  stock Proxmox web UI talking to its own server, not to us.
- Prefer creating windows in Rust (no permission needed) over granting the
  frontend `core:window:allow-create` / `core:webview:allow-create-webview`.

## Commands

```rust
#[tauri::command]
pub async fn list_guests(state: State<'_, AppState>) -> Result<Vec<Guest>> { … }
```

- Register in `tauri::generate_handler![…]` in `lib.rs`. A command that isn't
  registered fails at runtime, not compile time.
- An **async command taking `State` must use the `'_` lifetime**
  (`State<'_, AppState>`); without it the macro won't compile.
- The error type must be `Serialize`. This project serializes `error::Error` to
  a plain string, so the frontend gets a string in the promise rejection.
  Keep passwords and tickets out of those strings (hard rule).
- Injectable arguments: `tauri::AppHandle`, `tauri::WebviewWindow`,
  `tauri::State<'_, T>`. Everything else comes from the JS payload, matched by
  parameter name, camelCase on the JS side unless renamed.
- `.manage(AppState::default())` registers state; `State<'_, AppState>` retrieves
  it. State is keyed by type — one value per type.

## WebviewWindowBuilder (F4)

```rust
pub fn new<L: Into<String>>(manager: &'a M, label: L, url: WebviewUrl) -> Self
pub fn initialization_script(self, script: impl Into<String>) -> Self
pub fn initialization_script_for_all_frames(self, script: impl Into<String>) -> Self
pub fn on_navigation<F: Fn(&Url) -> bool + Send + 'static>(self, f: F) -> Self
pub fn user_agent(self, user_agent: &str) -> Self
pub fn title<S: Into<String>>(self, title: S) -> Self
pub fn inner_size(self, width: f64, height: f64) -> Self
pub fn incognito(self, incognito: bool) -> Self
pub fn data_store_identifier(self, data_store_identifier: [u8; 16]) -> Self
pub fn build(self) -> Result<WebviewWindow<R>>
```

`WebviewUrl::External("https://host:8006/".parse().unwrap())` for the Proxmox UI;
`WebviewUrl::App("index.html".into())` for bundled pages.

`on_navigation` returning `false` blocks the navigation — use it to keep the
admin window pinned to the Proxmox origin.

## Cookies: use `set_cookie`, not `document.cookie`

The F4 TODO in `commands.rs` plans an initialization script that sets
`document.cookie`. **2.12.4 has a real API for this** — prefer it:

```rust
pub fn set_cookie(&self, cookie: Cookie<'_>) -> Result<()>          // WebviewWindow
pub fn cookies(&self) -> Result<Vec<Cookie<'static>>>
pub fn cookies_for_url(&self, url: Url) -> Result<Vec<Cookie<'static>>>
```

Documented notes: cookies are only returned for `http`/`https` URLs; cookies set
via JavaScript for local files are unsupported; `cookies()` deadlocks in
synchronous commands on Windows (irrelevant here, Linux-only). `Cookie`
re-exports the `cookie` crate via `tauri_runtime` and may change in minor
releases.

## Unverified — check before relying on these

Stated honestly because the project forbids guessing:

1. **The import path for `Cookie`.** docs.rs says it re-exports
   `tauri_runtime::Cookie`; `tauri::Cookie` is likely but I did not confirm it
   resolves. Let the compiler settle it.
2. **Whether `set_cookie` must be called before or after the first navigation**,
   and what `domain`/`path`/`secure` the cookie needs for WebKitGTK to send it
   to Proxmox. Needs a live node.
3. **`initialization_script` ordering on remote URLs** — reportedly not
   guaranteed to run before the page's own scripts on some platforms. Another
   reason to prefer `set_cookie`.
4. **The self-signed certificate.** WebKitGTK will reject it and there is no
   documented Tauri switch to accept it per-window. F4 is genuinely blocked on
   F3, and the webview may need the cert trusted at the system level. Do not
   invent an API for this — say it's unsolved.
5. **Capability `local` field semantics.** The docs pages I checked don't
   describe it. Read `gen/schemas/desktop-schema.json` after a build.

Sources: [v1→v2 migration](https://v2.tauri.app/start/migrate/from-tauri-1/),
[capabilities](https://v2.tauri.app/security/capabilities/),
[core permissions](https://v2.tauri.app/reference/acl/core-permissions/),
[calling Rust](https://v2.tauri.app/develop/calling-rust/),
[WebviewWindow](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindow.html),
[WebviewWindowBuilder](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindowBuilder.html).
