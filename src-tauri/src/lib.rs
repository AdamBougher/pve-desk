//! pve-desk: desktop client for Proxmox VE.
//!
//! Module map (feature IDs: ROADMAP.md for open work, DECISIONS.md for done):
//! - `config`   — non-secret settings on disk (F1, F3)
//! - `secrets`  — password in the system keyring (F1, F2)
//! - `api`      — Proxmox API client, login ticket, VM list, power (F2, F5, F6)
//! - `spice`    — SPICE .vv file + remote-viewer launch (F7)
//! - `commands` — Tauri commands the frontend calls
//! - `error`    — shared error type

mod api;
mod commands;
mod config;
mod error;
mod secrets;
mod spice;

use std::collections::HashMap;
use std::sync::Mutex;

/// App-wide state shared with Tauri commands.
///
/// Keyed by config entry id. v1 holds at most one (Q16.1: "one now, shaped
/// for several"), enforced by `set_only`; the map is here so allowing
/// several later is a local change.
#[derive(Default)]
pub struct AppState {
    pub sessions: Mutex<HashMap<String, api::Session>>,
}

impl AppState {
    /// The one live session, with the id of the entry it belongs to.
    pub fn current(&self) -> Option<(String, api::Session)> {
        let sessions = self.sessions.lock().unwrap();
        sessions.iter().next().map(|(k, v)| (k.clone(), v.clone()))
    }

    /// Replace whatever was connected with this one.
    pub fn set_only(&self, name: String, session: api::Session) {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.clear();
        sessions.insert(name, session);
    }

    pub fn clear(&self) {
        self.sessions.lock().unwrap().clear();
    }

    pub fn is_connected(&self, name: &str) -> bool {
        self.sessions.lock().unwrap().contains_key(name)
    }
}

/// Pick a WebKitGTK rendering path that works on this machine.
///
/// WebKit's dmabuf renderer allocates GBM buffers, which fails on the NVIDIA
/// proprietary driver: on a native Wayland session GDK then dies with
/// "Error 71 (Protocol error)" before any window appears.
///
/// Default is `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1`: the dmabuf renderer and
/// accelerated compositing stay on, but buffers go through shared memory
/// instead of GBM. Verified on this machine to start cleanly on Wayland with
/// no errors and render correctly.
///
/// `PVE_DESK_RENDER` overrides it while the console-smoothness problem is
/// being chased:
///   - `legacy` — disable the dmabuf renderer entirely (the previous default)
///   - `raw`    — set nothing; WebKit's stock behaviour, crashes on NVIDIA
#[cfg(target_os = "linux")]
fn apply_linux_webkit_workarounds() {
    match std::env::var("PVE_DESK_RENDER").as_deref() {
        Ok("raw") => {}
        Ok("legacy") => {
            if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
                std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            }
        }
        _ => {
            if std::env::var_os("WEBKIT_DMABUF_RENDERER_FORCE_SHM").is_none() {
                std::env::set_var("WEBKIT_DMABUF_RENDERER_FORCE_SHM", "1");
            }
        }
    }
}

pub fn run() {
    // Must happen before the webview starts. WebKitGTK only; WebView2 and
    // WKWebView have no equivalent problem.
    #[cfg(target_os = "linux")]
    apply_linux_webkit_workarounds();

    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::list_connections,
            commands::probe_connection,
            commands::add_connection,
            commands::connect,
            commands::list_guests,
            commands::power_action,
            commands::open_spice,
            commands::open_admin_panel,
            commands::hide_admin_panel,
            commands::logout,
        ])
        .setup(|app| {
            // F14: keep the admin webview fitted to the window. Registered
            // once, here, rather than when the webview is created — Tauri
            // accumulates window event handlers, so doing it per connect
            // would stack a duplicate every time.
            if let Some(window) = tauri::Manager::get_window(app, "main") {
                let handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if matches!(event, tauri::WindowEvent::Resized(_)) {
                        commands::relayout_admin(&handle);
                    }
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running pve-desk");
}
