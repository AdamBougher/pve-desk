//! F10: the system tray — Show and Quit.
//!
//! The menu lists no guests (decided 2026-11-21, superseding Q10.1, Q10.2 and
//! Q11.3; see DECISIONS.md, "The tray"). Its job is to bring the window back
//! after a close hid it, and to quit.
//!
//! Built in Rust, not declared as `trayIcon` in `tauri.conf.json`: a Rust
//! tray needs no config key and no capability, since `core:tray` permissions
//! govern the *frontend* calling tray APIs and the frontend never does.
//!
//! `TrayIconEvent` is never emitted on Linux (read from tauri 2.12.4's source,
//! recorded in DECISIONS.md), so there is no click handler to hang anything
//! on. "Show pve-desk" is a menu item for that reason — on other platforms a
//! left click would do it for free.

use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

/// Id of the one tray icon.
///
/// `AppHandle::tray_by_id` with this id is also the test for "a tray exists",
/// which is what Q4.2's fallback needs: no tray means closing the window
/// quits, so the app can never hide with no way back. That keeps the answer
/// in one place instead of adding a flag to `AppState`.
pub const TRAY_ID: &str = "main";

const SHOW: &str = "show";
const QUIT: &str = "quit";

/// Create the tray icon and its fixed menu.
///
/// A tray that cannot be created is not fatal: the app runs exactly as it did
/// before F10, and `CloseRequested` finds no tray and quits.
pub fn install(app: &AppHandle) {
    let Ok(menu) = menu(app) else {
        return;
    };
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("pve-desk")
        .menu(&menu)
        .on_menu_event(on_menu_event);
    // F20 gave the icon a light fill on its chevrons so it reads on a dark
    // panel; this is the same image, so the tray inherits that fix.
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    let _ = builder.build(app);
}

/// Whether a tray icon exists.
///
/// Q4.2's fallback asks this before hiding the window: with no tray there is
/// nowhere to hide to, so the close must be allowed through.
pub fn exists(app: &AppHandle) -> bool {
    app.tray_by_id(TRAY_ID).is_some()
}

/// The menu never changes, so it is built once.
fn menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let show = MenuItem::with_id(app, SHOW, "Show pve-desk", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit", true, None::<&str>)?;
    Menu::with_items(app, &[&show, &separator, &quit])
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        SHOW => show_window(app),
        QUIT => app.exit(0),
        _ => {}
    }
}

/// Bring the main window back. `set_focus` as well as `show`, so F23's
/// `Focused(true)` fires and the guest-list poll starts again — a shown but
/// unfocused window would sit there not refreshing.
fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
