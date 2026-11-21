//! F38: every OS-specific behaviour, behind one interface.
//!
//! `Platform` is the interface; each OS has one file that implements it, and
//! `Os` is the implementation for the OS this build targets. Callers write
//! `Os::show(&app)` and never branch on the OS themselves. See DECISIONS.md,
//! "Platform seam".
//!
//! - `linux.rs`   — the native GTK tab strip, Admin as a child webview,
//!   WebKitGTK certificate pinning (F14, F21, F33).
//! - `windows.rs` — Admin as its own window, no tab strip, no pinning (F39).
//! - `macos.rs`   — the Windows shell for now (until F42), the unix `.vv`
//!   file.

use crate::error::Result;
use std::path::Path;
use tauri::{AppHandle, Url, Webview};

#[cfg(target_os = "linux")]
mod linux;
// macOS reuses the Windows shell until F42 gives it its own.
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod windows;

#[cfg(target_os = "linux")]
pub use self::linux::Linux as Os;
#[cfg(target_os = "macos")]
pub use self::macos::MacOs as Os;
#[cfg(target_os = "windows")]
pub use self::windows::Windows as Os;

/// What differs per OS. Associated functions only: there is no per-OS state
/// to carry in a value (Linux keeps its GTK widgets in thread-locals, because
/// GTK widgets are not `Send`).
pub trait Platform {
    // --- Start-up -------------------------------------------------------

    /// Runs before the webview starts.
    fn before_start();

    /// Runs once in `setup`, before the tray.
    fn install(app: &AppHandle);

    // --- AdminShell -----------------------------------------------------

    /// Whether the build has the native top bar, which holds Log out (F33).
    fn has_top_bar() -> bool;

    /// Build the Admin webview with label `label`, on `blank`.
    fn create_admin(app: &AppHandle, label: &str, blank: Url) -> Result<Webview>;

    /// Admin is up (just built and loading, or shown again): make it the
    /// thing the user sees.
    fn show(app: &AppHandle) -> Result<()>;

    /// The user asked for Home, or the session is lost: put Admin away.
    fn hide(app: &AppHandle) -> Result<()>;

    /// Admin was closed (log out): show Home. Nothing to put away.
    fn show_home(app: &AppHandle);

    /// Write status text where it stays visible while Admin covers the page.
    fn set_status(app: &AppHandle, msg: String);

    /// Grey or enable the controls that need a session (Admin, Log out).
    fn set_admin_enabled(app: &AppHandle, enabled: bool);

    // --- CertPinning ----------------------------------------------------

    /// The Admin webview can load only a node the OS trust store vouches
    /// for: it has no pinning of its own (F39; F40 adds it on Windows).
    fn admin_needs_os_trust() -> bool;

    /// Load `target` in the Admin webview, with this OS's certificate
    /// pinning for connection `entry` on `hostname`, if it has any.
    fn attach(
        webview: &Webview,
        app: AppHandle,
        entry: String,
        hostname: String,
        target: Url,
    ) -> Result<()>;

    // --- Viewer ---------------------------------------------------------

    /// The `remote-viewer` binary when the config names none (F7).
    fn default_viewer() -> &'static str;

    /// Create a new file only this user can read. Fails if `path` exists.
    fn create_private(path: &Path) -> std::io::Result<std::fs::File>;

    /// Make a directory only this user can list.
    fn restrict_dir(dir: &Path) -> std::io::Result<()>;

    // --- TrayCaps -------------------------------------------------------

    /// A tray icon exists, so closing the window can hide to it (F10).
    fn tray_exists(app: &AppHandle) -> bool;
}
