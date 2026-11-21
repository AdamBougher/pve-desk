//! Windows: Admin is its own window, as before F14. Tabs are Linux-only
//! because the native strip they need is a GTK widget; a child webview with
//! no way to switch back would be worse than a separate window. No top bar,
//! no native status label, no certificate pinning yet (F40).
//!
//! macOS calls this file for the shell too, until F42.
//!
//! Moved from `commands.rs` by F38 without change. Built and tested by CI on
//! windows-latest; never run on Windows.

use super::Platform;
use crate::commands::ADMIN_WEBVIEW;
use crate::error::Result;
use std::path::Path;
use tauri::{AppHandle, Manager, Url, Webview, WebviewUrl, WebviewWindowBuilder};

pub struct Windows;

impl Platform for Windows {
    /// WebView2 has no equivalent of the WebKitGTK renderer problem.
    fn before_start() {}

    /// No native tab strip: Admin is its own window.
    fn install(_app: &AppHandle) {}

    fn has_top_bar() -> bool {
        false
    }

    fn create_admin(app: &AppHandle, label: &str, blank: Url) -> Result<Webview> {
        let window = WebviewWindowBuilder::new(app, label, WebviewUrl::External(blank))
            .title("Proxmox")
            .inner_size(1280.0, 860.0)
            .center()
            .focused(true)
            .build()?;
        Ok(window.as_ref().clone())
    }

    fn show(app: &AppHandle) -> Result<()> {
        if let Some(window) = app.get_webview_window(ADMIN_WEBVIEW) {
            window.set_focus()?;
        }
        Ok(())
    }

    /// No tab to switch to: the panel is its own window, so hiding it would
    /// strand it. Closing is what the caller (session loss) wants.
    fn hide(app: &AppHandle) -> Result<()> {
        if let Some(window) = app.get_webview_window(ADMIN_WEBVIEW) {
            window.close()?;
        }
        Ok(())
    }

    /// Home's window was never covered.
    fn show_home(_app: &AppHandle) {}

    /// Admin never covers the page, so the page's own status text is enough.
    fn set_status(_app: &AppHandle, _msg: String) {}

    /// The page greys its own buttons; there are no native ones.
    fn set_admin_enabled(_app: &AppHandle, _enabled: bool) {}

    /// No pinning yet (F40): the node's certificate must be valid against
    /// the OS trust store (Q17.2).
    fn attach(
        webview: &Webview,
        _app: AppHandle,
        _entry: String,
        _hostname: String,
        target: Url,
    ) -> Result<()> {
        webview.navigate(target)?;
        Ok(())
    }

    /// virt-viewer installs in a version-stamped location, so fall back to
    /// the bare command name and let PATH resolve it; the user can override
    /// it in `config.toml`.
    fn default_viewer() -> &'static str {
        "remote-viewer.exe"
    }

    /// No mode bits. `create_new` still refuses a path that already exists.
    fn create_private(path: &Path) -> std::io::Result<std::fs::File> {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
    }

    /// `%LOCALAPPDATA%\Temp` is already per-user and ACL'd to that user,
    /// which is what this relies on — unverified, since Windows has never
    /// been run (F17).
    fn restrict_dir(_dir: &Path) -> std::io::Result<()> {
        Ok(())
    }

    fn tray_exists(app: &AppHandle) -> bool {
        crate::tray::exists(app)
    }
}
