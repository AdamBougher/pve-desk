//! macOS: the Windows shell (Admin in its own window) until F42, with the
//! unix `.vv` file (mode 0600), as before F38.
//!
//! Never built or run: no macOS CI job and no Mac (F17).

use super::windows::Windows;
use super::Platform;
use crate::error::Result;
use std::path::Path;
use tauri::{AppHandle, Url, Webview};

pub struct MacOs;

impl Platform for MacOs {
    fn before_start() {
        Windows::before_start();
    }

    fn install(app: &AppHandle) {
        Windows::install(app);
    }

    fn has_top_bar() -> bool {
        Windows::has_top_bar()
    }

    fn create_admin(app: &AppHandle, label: &str, blank: Url) -> Result<Webview> {
        Windows::create_admin(app, label, blank)
    }

    fn show(app: &AppHandle) -> Result<()> {
        Windows::show(app)
    }

    fn hide(app: &AppHandle) -> Result<()> {
        Windows::hide(app)
    }

    fn show_home(app: &AppHandle) {
        Windows::show_home(app);
    }

    fn set_status(app: &AppHandle, msg: String) {
        Windows::set_status(app, msg);
    }

    fn set_admin_enabled(app: &AppHandle, enabled: bool) {
        Windows::set_admin_enabled(app, enabled);
    }

    fn admin_needs_os_trust() -> bool {
        Windows::admin_needs_os_trust()
    }

    fn attach(
        webview: &Webview,
        app: AppHandle,
        entry: String,
        hostname: String,
        target: Url,
    ) -> Result<()> {
        Windows::attach(webview, app, entry, hostname, target)
    }

    /// virt-viewer comes from a package manager with no fixed path, so let
    /// PATH resolve it; the user can override it in `config.toml`.
    fn default_viewer() -> &'static str {
        "remote-viewer"
    }

    /// Same as Linux: mode 0600 at creation, and `create_new`.
    fn create_private(path: &Path) -> std::io::Result<std::fs::File> {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
    }

    fn restrict_dir(dir: &Path) -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
    }

    fn tray_exists(app: &AppHandle) -> bool {
        Windows::tray_exists(app)
    }
}
