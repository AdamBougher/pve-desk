//! F10: the system tray — the session's guests, Show and Quit.
//!
//! Built in Rust, not declared as `trayIcon` in `tauri.conf.json`: a Rust
//! tray needs no config key and no capability, since `core:tray` permissions
//! govern the *frontend* calling tray APIs and the frontend never does.
//!
//! Two Linux facts shape everything here, read from tauri 2.12.4's source
//! rather than assumed (both recorded in DECISIONS.md, "The tray"):
//!
//! - `TrayIconEvent` is never emitted on Linux, so there is no click handler
//!   to hang anything on. Every interaction is a menu item, "Show pve-desk"
//!   included — on other platforms a left click would do it for free.
//! - Nothing offers a menu-about-to-open hook, so the menu cannot be built
//!   lazily when it is opened. It is rebuilt at the moments listed on
//!   `refresh`.

use crate::api::Guest;
use crate::{commands, AppState};
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
/// Prefix on every per-guest item id, so a guest item is told apart from
/// `SHOW`/`QUIT` by its prefix rather than by elimination.
const GUEST: &str = "g:";

/// Create the tray icon, with a menu that lists nothing yet.
///
/// Called from `setup`, where the app always starts on the launcher with no
/// session (F16), so an empty list is correct. A tray that cannot be created
/// is not fatal: the app runs exactly as it did before F10, and
/// `CloseRequested` finds no tray and quits.
pub fn install(app: &AppHandle) {
    let Ok(menu) = menu_for(app, &[]) else {
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

/// Fetch the guest list and rebuild the menu from it.
///
/// Called when a session connects, when the window is hidden, and after a
/// tray power action — the three moments that can change what the menu
/// should say. There is deliberately no timer: F23 removed polling that
/// nobody watches, and an app left closed to the tray would otherwise hit
/// the node every 30s for as long as it ran.
///
/// Returns at once; the fetch and the rebuild happen on a task, because the
/// callers are event handlers that must not block the main thread.
pub fn refresh(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let guests = guests(&app).await;
        rebuild(&app, &guests);
    });
}

/// Empty the menu's guest list, without asking the node anything.
///
/// For logging out and for a lost session, where there is no longer a
/// session to ask and the answer is known to be "nothing".
pub fn clear(app: &AppHandle) {
    rebuild(app, &[]);
}

/// The guests the menu should list, or none if the node could not be asked.
///
/// Mirrors `refresh()` in `main.js`: templates dropped (Q5.3) and sorted by
/// vmid, so the tray lists what Home lists in the order Home lists it.
async fn guests(app: &AppHandle) -> Vec<Guest> {
    let state = app.state::<AppState>();
    // Reuses the command the frontend calls, which already re-logs in on a
    // stale ticket and drops a dead session (F2, Q2.2). An error here is
    // either "no session" or a node that did not answer; both mean the menu
    // lists no guests, and nothing is logged — error strings are not a place
    // this project writes to (hard rule: nothing about a credential reaches
    // a log).
    let Ok(all) = commands::list_guests(app.clone(), state).await else {
        return Vec::new();
    };
    let mut guests: Vec<Guest> = all.into_iter().filter(|g| g.template != Some(1)).collect();
    guests.sort_by_key(|g| g.vmid);
    guests
}

/// Swap in a menu built from `guests`. A tray that is not there is not an
/// error — see `install`.
fn rebuild(app: &AppHandle, guests: &[Guest]) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    if let Ok(menu) = menu_for(app, guests) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// One item per guest, then Show and Quit.
///
/// Show and Quit come last and are always present, so the menu is useful
/// even with no session (and so Quit does not move under the cursor as the
/// guest list changes length).
fn menu_for(app: &AppHandle, guests: &[Guest]) -> tauri::Result<Menu<tauri::Wry>> {
    let mut items: Vec<MenuItem<tauri::Wry>> = Vec::with_capacity(guests.len() + 2);
    if guests.is_empty() {
        // Disabled, so it reads as a statement rather than something to pick.
        items.push(MenuItem::with_id(
            app,
            "none",
            "No connection",
            false,
            None::<&str>,
        )?);
    }
    for g in guests {
        let label = escape_mnemonic(&guest_label(g));
        match action_for(&g.status) {
            Some(action) => items.push(MenuItem::with_id(
                app,
                guest_id(&g.node, &g.kind, g.vmid, action),
                format!("{label} — {}", action_label(action)),
                true,
                None::<&str>,
            )?),
            // Paused, suspended, or a node that is down reporting something
            // else: Home shows the row with no buttons, so the tray shows the
            // guest with nothing to pick (Q6.1's two known states).
            None => items.push(MenuItem::with_id(
                app,
                format!("{GUEST}unknown:{}", g.vmid),
                format!("{label} — {}", escape_mnemonic(&g.status)),
                false,
                None::<&str>,
            )?),
        }
    }
    let show = MenuItem::with_id(app, SHOW, "Show pve-desk", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;

    let mut refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = Vec::new();
    for item in &items {
        refs.push(item);
    }
    refs.push(&separator);
    refs.push(&show);
    refs.push(&quit);
    Menu::with_items(app, &refs)
}

/// Act on a picked item. Runs on the main thread, so the power action goes
/// to a task rather than blocking the menu.
fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        SHOW => show_window(app),
        QUIT => app.exit(0),
        id => {
            if let Some((node, kind, vmid, action)) = parse_guest_id(id) {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    // The frontend's own command, so the action still passes
                    // through the allow-list in `Session::power` — the tray
                    // is not a second way in (hard rule).
                    let _ = commands::power_action(
                        app.clone(),
                        state,
                        node,
                        kind,
                        vmid,
                        action.to_string(),
                    )
                    .await;
                    // Rebuild either way: the new menu is the only report the
                    // user gets with the window hidden. If the action failed,
                    // the item stays as it was, which is the truth.
                    let guests = guests(&app).await;
                    rebuild(&app, &guests);
                });
            }
        }
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

/// The one action a guest in this state can be given (Q10.2: start and
/// shutdown only — the forced stop stays in Home, where the confirmation
/// panel it needs exists).
fn action_for(status: &str) -> Option<&'static str> {
    match status {
        "stopped" => Some("start"),
        "running" => Some("shutdown"),
        _ => None,
    }
}

fn action_label(action: &str) -> &'static str {
    match action {
        "start" => "Start",
        _ => "Shutdown",
    }
}

/// Same text as Home's `guestLabel` in `main.js`, so a guest reads the same
/// in both places.
fn guest_label(g: &Guest) -> String {
    match &g.name {
        Some(name) if !name.is_empty() => format!("{name} ({})", g.vmid),
        _ => g.vmid.to_string(),
    }
}

/// Double any `&`, which a menu would otherwise read as a mnemonic marker.
///
/// Guest names, node names and statuses come from the server, so this is the
/// menu's version of the `textContent` rule: server text is shown, never
/// interpreted. A guest named `R&D` must not silently give the item a
/// keyboard mnemonic on `D`.
fn escape_mnemonic(s: &str) -> String {
    s.replace('&', "&&")
}

/// `g:<action>:<kind>:<vmid>:<node>`.
///
/// The node goes last because it is the only part whose characters this app
/// does not choose, so it needs no escaping and cannot shift the fields in
/// front of it.
fn guest_id(node: &str, kind: &str, vmid: u32, action: &str) -> String {
    format!("{GUEST}{action}:{kind}:{vmid}:{node}")
}

/// Read back what `guest_id` wrote. `None` for anything else, including the
/// disabled items, which carry no action.
fn parse_guest_id(id: &str) -> Option<(String, String, u32, &'static str)> {
    let rest = id.strip_prefix(GUEST)?;
    let mut parts = rest.splitn(4, ':');
    let action = parts.next()?;
    let kind = parts.next()?;
    let vmid: u32 = parts.next()?.parse().ok()?;
    let node = parts.next()?;
    if node.is_empty() {
        return None;
    }
    // Re-derived from a fixed list rather than passed through, so a menu id
    // can never widen what the tray is able to ask for.
    let action = match action {
        "start" => "start",
        "shutdown" => "shutdown",
        _ => return None,
    };
    Some((node.to_string(), kind.to_string(), vmid, action))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guest(vmid: u32, name: Option<&str>, status: &str) -> Guest {
        Guest {
            vmid,
            name: name.map(str::to_string),
            kind: "qemu".into(),
            node: "pve".into(),
            status: status.into(),
            cpu: None,
            maxcpu: None,
            mem: None,
            maxmem: None,
            template: None,
            tags: None,
            uptime: None,
            disk: None,
            maxdisk: None,
            netin: None,
            netout: None,
        }
    }

    #[test]
    fn id_round_trips() {
        let id = guest_id("pve", "lxc", 101, "shutdown");
        assert_eq!(
            parse_guest_id(&id),
            Some(("pve".into(), "lxc".into(), 101, "shutdown"))
        );
    }

    #[test]
    fn a_node_name_with_a_colon_survives() {
        // Not a name Proxmox allows, but the node is the field this app does
        // not choose, so parsing must not depend on it being tame.
        let id = guest_id("odd:name", "qemu", 7, "start");
        assert_eq!(
            parse_guest_id(&id),
            Some(("odd:name".into(), "qemu".into(), 7, "start"))
        );
    }

    #[test]
    fn only_the_two_allowed_actions_parse() {
        let forged = format!("{GUEST}stop:qemu:101:pve");
        assert_eq!(parse_guest_id(&forged), None);
        let forged = format!("{GUEST}../destroy:qemu:101:pve");
        assert_eq!(parse_guest_id(&forged), None);
    }

    #[test]
    fn non_guest_ids_do_not_parse() {
        assert_eq!(parse_guest_id(SHOW), None);
        assert_eq!(parse_guest_id(QUIT), None);
        assert_eq!(parse_guest_id("g:start:qemu:notanumber:pve"), None);
        assert_eq!(parse_guest_id("g:start:qemu:101:"), None);
        assert_eq!(parse_guest_id("g:start:qemu"), None);
    }

    #[test]
    fn a_guest_gets_one_action_for_its_state() {
        assert_eq!(action_for("stopped"), Some("start"));
        assert_eq!(action_for("running"), Some("shutdown"));
        // Q10.2: no forced stop in the menu, whatever the state.
        assert_eq!(action_for("paused"), None);
        assert_eq!(action_for("suspended"), None);
        assert_eq!(action_for(""), None);
    }

    #[test]
    fn label_matches_home() {
        assert_eq!(
            guest_label(&guest(101, Some("web"), "running")),
            "web (101)"
        );
        assert_eq!(guest_label(&guest(101, None, "running")), "101");
        assert_eq!(guest_label(&guest(101, Some(""), "running")), "101");
    }

    #[test]
    fn ampersands_in_server_text_are_not_mnemonics() {
        assert_eq!(escape_mnemonic("R&D"), "R&&D");
        assert_eq!(escape_mnemonic("plain"), "plain");
        assert_eq!(escape_mnemonic("&&"), "&&&&");
    }
}
