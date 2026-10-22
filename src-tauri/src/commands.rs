//! Tauri commands: the bridge between the HTML frontend and Rust.
//! The frontend calls these with `window.__TAURI__.core.invoke("name", {...})`.

use crate::api::{self, Guest, Session};
use crate::config::{self, Config, HostConfig};
use crate::error::{Error, Result};
use crate::{secrets, spice, AppState};
use tauri::webview::{Cookie, WebviewBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, Url, WebviewUrl};
// Only the platforms that can position a webview need these (see
// `layout_admin`): on Linux the GtkBox decides the geometry.
#[cfg(not(target_os = "linux"))]
use tauri::{Rect, Window};

/// Label of the admin panel's webview (F14).
///
/// Deliberately absent from `capabilities/default.json`: it loads the remote
/// Proxmox UI, and no capability naming it means it cannot reach a single app
/// command (CLAUDE.md hard rule). Note the capability names `webviews`, not
/// `windows` — a window-scoped capability applies to *every* webview in that
/// window, which would hand this one `core:default`.
const ADMIN_WEBVIEW: &str = "admin";

/// Label of the window and local webview the frontend runs in.
const MAIN: &str = "main";

/// Log in again with the entry's keyring password, replacing the stored
/// session (F2).
async fn relogin(state: &State<'_, AppState>, name: &str) -> Result<Session> {
    let cfg = config::load()?.ok_or(Error::NotConfigured)?;
    let host = cfg.host(name)?.clone();
    let password = secrets::get_password(&host.userid(), &host.host)?;
    let session = api::login(&host, &password, None).await?;
    state.set_only(name.to_string(), session.clone());
    Ok(session)
}

/// The connected session, re-logged-in first if the ticket is old enough to
/// be near expiry (F2). Tickets last ~2h; this refreshes past ~90 min.
async fn fresh_session(state: &State<'_, AppState>) -> Result<(String, Session)> {
    let (name, session) = state.current().ok_or(Error::NotLoggedIn)?;
    if session.needs_refresh() {
        let session = relogin(state, &name).await?;
        return Ok((name, session));
    }
    Ok((name, session))
}

/// The node rejected the ticket and re-login failed too. Drop the session and
/// tell the UI, so it returns to the launcher instead of retrying every 30
/// seconds against a connection that is gone (Q2.2).
fn session_lost(app: &AppHandle, state: &State<'_, AppState>, why: &Error) {
    state.clear();
    set_admin_tab_enabled(app, false);
    let _ = app.emit("session-lost", why.to_string());
}

/// The connected session together with the config entry it belongs to,
/// refreshed if stale.
async fn current_host(state: &State<'_, AppState>) -> Result<(HostConfig, Session)> {
    let (name, session) = fresh_session(state).await?;
    let cfg = config::load()?.ok_or(Error::NotConfigured)?;
    Ok((cfg.host(&name)?.clone(), session))
}

/// One row in the launcher (F16). Never carries the password.
#[derive(serde::Serialize)]
pub struct Connection {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub realm: String,
    /// A password for this entry exists in the keyring.
    pub saved_password: bool,
    /// This entry is the live session.
    pub connected: bool,
    /// Used most recently; the launcher preselects it.
    pub last_used: bool,
}

/// Result of probing one entry (F16, Q16.2).
#[derive(serde::Serialize)]
pub struct ConnectionStatus {
    /// The node answered on host:port. No credentials were sent.
    pub reachable: bool,
    /// We hold a ticket for it and the node still accepts it.
    pub ticket_valid: bool,
}

/// F16: saved connections, for the launcher the app always opens on.
#[tauri::command]
pub fn list_connections(state: State<'_, AppState>) -> Result<Vec<Connection>> {
    let Some(cfg) = config::load()? else {
        return Ok(Vec::new());
    };
    Ok(cfg
        .hosts
        .iter()
        .map(|h| Connection {
            name: h.name.clone(),
            host: h.host.clone(),
            port: h.port,
            username: h.username.clone(),
            realm: h.realm.clone(),
            saved_password: secrets::get_password(&h.userid(), &h.host).is_ok(),
            connected: state.is_connected(&h.name),
            last_used: h.name == cfg.default,
        })
        .collect())
}

/// F16: check one entry. Called per row so the launcher fills in as results
/// arrive instead of waiting on the slowest node.
#[tauri::command]
pub async fn probe_connection(
    state: State<'_, AppState>,
    name: String,
) -> Result<ConnectionStatus> {
    let cfg = config::load()?.ok_or(Error::NotConfigured)?;
    let host = cfg.host(&name)?.clone();
    let session = state.current().and_then(|(n, s)| (n == name).then_some(s));

    let ticket_valid = match session {
        Some(s) => s.is_valid().await,
        None => false,
    };
    // A valid ticket already proves the node answered.
    let reachable = ticket_valid || api::reachable(&host).await;
    Ok(ConnectionStatus {
        reachable,
        ticket_valid,
    })
}

/// What a connect attempt produced (F3, Q3.3).
///
/// A refused certificate is an outcome, not an error: the UI shows the
/// fingerprint and asks, then retries with `trust_fingerprint` set.
#[derive(serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ConnectOutcome {
    Connected,
    CertificateUnverified { fingerprint: String },
}

/// F16: connect to a saved entry using its keyring password.
///
/// `trust_fingerprint` is set only when the user has just confirmed a
/// certificate in the UI; it is pinned to the entry once the login succeeds.
#[tauri::command]
pub async fn connect(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    trust_fingerprint: Option<String>,
) -> Result<ConnectOutcome> {
    let mut cfg = config::load()?.ok_or(Error::NotConfigured)?;
    let host = cfg.host(&name)?.clone();
    let password = secrets::get_password(&host.userid(), &host.host)?;

    let session = match api::login(&host, &password, trust_fingerprint.as_deref()).await {
        Ok(session) => session,
        Err(Error::CertUnverified(fingerprint)) => {
            return Ok(ConnectOutcome::CertificateUnverified { fingerprint });
        }
        Err(e) => return Err(e),
    };
    state.set_only(name.clone(), session);
    set_admin_tab_enabled(&app, true);

    // Persist what the user just confirmed, so later connects verify against
    // it instead of asking again.
    let mut dirty = false;
    if let Some(fp) = trust_fingerprint {
        if let Some(entry) = cfg.hosts.iter_mut().find(|h| h.name == name) {
            entry.cert_fingerprint = Some(fp);
            dirty = true;
        }
    }
    // Remember which entry to preselect next time (Q16.4).
    if cfg.default != name {
        cfg.default = name;
        dirty = true;
    }
    if dirty {
        config::save(&cfg)?;
    }
    // TODO(F2): spawn a background task that re-logs-in every ~90 min.
    Ok(ConnectOutcome::Connected)
}

/// The "Add a connection" form (F16).
///
/// Grouped into a struct so the command stays inside clippy's argument limit,
/// and so the password has exactly one place to live on the way through. No
/// `Debug`: it must never be printable (CLAUDE.md hard rule).
#[derive(serde::Deserialize)]
pub struct NewConnection {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub realm: String,
    pub password: String,
}

/// F1/F16: test the login, then save the entry and its password.
#[tauri::command]
pub async fn add_connection(
    app: AppHandle,
    state: State<'_, AppState>,
    fields: NewConnection,
    trust_fingerprint: Option<String>,
) -> Result<ConnectOutcome> {
    let password = fields.password;
    let host = fields.host.trim().to_string();
    let username = fields.username.trim().to_string();
    let realm = fields.realm.trim().to_string();
    let entry = HostConfig {
        name: HostConfig::id(&username, &realm, &host),
        host,
        port: fields.port,
        username,
        realm,
        // Pinned below, once the login proves the certificate was accepted.
        cert_fingerprint: trust_fingerprint.clone(),
    };
    entry.validate()?;

    // Only save if the login actually works. A refused certificate saves
    // nothing at all — not the entry, not the password.
    let session = match api::login(&entry, &password, trust_fingerprint.as_deref()).await {
        Ok(session) => session,
        Err(Error::CertUnverified(fingerprint)) => {
            return Ok(ConnectOutcome::CertificateUnverified { fingerprint });
        }
        Err(e) => return Err(e),
    };
    secrets::set_password(&entry.userid(), &entry.host, &password)?;

    // Re-running setup keeps app-level settings (e.g. remote_viewer_path).
    // An unreadable file is replaced: setup is the user re-entering this data.
    let mut cfg = config::load()
        .ok()
        .flatten()
        .unwrap_or_else(|| Config::single(entry.clone()));
    let name = entry.name.clone();
    cfg.upsert_host(entry);
    config::save(&cfg)?;

    state.set_only(name, session);
    set_admin_tab_enabled(&app, true);
    Ok(ConnectOutcome::Connected)
}

/// F5
#[tauri::command]
pub async fn list_guests(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<Guest>> {
    let (name, session) = fresh_session(&state).await?;
    match session.list_guests().await {
        // The ticket was refused despite looking fresh — the clock can lie
        // across a suspend. Re-login once and retry before giving up.
        Err(Error::Unauthorized) => match relogin(&state, &name).await {
            Ok(session) => session.list_guests().await,
            Err(e) => {
                session_lost(&app, &state, &e);
                Err(e)
            }
        },
        other => other,
    }
}

/// F6
#[tauri::command]
pub async fn power_action(
    app: AppHandle,
    state: State<'_, AppState>,
    node: String,
    kind: String,
    vmid: u32,
    action: String,
) -> Result<()> {
    let (name, session) = fresh_session(&state).await?;
    match session.power(&node, &kind, vmid, &action).await {
        Err(Error::Unauthorized) => match relogin(&state, &name).await {
            Ok(session) => session.power(&node, &kind, vmid, &action).await,
            Err(e) => {
                session_lost(&app, &state, &e);
                Err(e)
            }
        },
        other => other,
    }
}

/// F7
#[tauri::command]
pub async fn open_spice(
    app: AppHandle,
    state: State<'_, AppState>,
    node: String,
    vmid: u32,
) -> Result<()> {
    let cfg = config::load()?.ok_or(Error::NotConfigured)?;
    let (name, session) = fresh_session(&state).await?;
    let data = match session.spice_proxy(&node, vmid).await {
        Err(Error::Unauthorized) => match relogin(&state, &name).await {
            Ok(session) => session.spice_proxy(&node, vmid).await,
            Err(e) => {
                session_lost(&app, &state, &e);
                Err(e)
            }
        },
        other => other,
    }?;
    spice::launch(&cfg.remote_viewer_path, &node, vmid, &data).await
}

/// Fit the two webviews around the tab bar.
///
/// **Linux has no webview geometry, and no way to cap a widget's height.**
/// `tauri-runtime-wry` builds a child webview into the window's `GtkBox`, and
/// wry packs it with `pack_start(webview, true, true, 0)` with
/// `is_in_fixed_parent = false`, so `set_bounds` is ignored. Nor can the
/// local webview simply be pinned to the tab bar's height: `set_size_request`
/// sets a *minimum*, and GTK3 hands a box child its **natural** size before
/// giving anything to expanding siblings. Measured: with `req = -1x36` the
/// local webview reported `preferred_h = (min 36, nat 700)` and took all 700
/// pixels, leaving the admin webview 1.
///
/// So the tab bar is a native GTK widget (see `install_native_tabs`) and
/// exactly one webview is visible at a time — the visible one fills the space
/// below the strip, which needs no capping at all.
#[cfg(target_os = "linux")]
pub fn layout_admin(app: &AppHandle, admin_visible: bool) {
    let Some(local) = app.get_webview(MAIN) else {
        return;
    };
    let _ = if admin_visible {
        local.hide()
    } else {
        local.show()
    };
}

// The Admin button, so its sensitivity can be toggled with the session.
// A thread-local rather than app state because GTK widgets are not `Send`:
// everything that touches it runs on the main thread, either inside
// `with_webview` or via `run_on_main_thread`.
#[cfg(target_os = "linux")]
thread_local! {
    static ADMIN_TAB: std::cell::RefCell<Option<gtk::Button>> =
        const { std::cell::RefCell::new(None) };
}

/// Grey the Admin tab out until there is a session to show.
///
/// Without this the button is live on the launcher, where pressing it can
/// only produce an error.
#[cfg(target_os = "linux")]
pub fn set_admin_tab_enabled(app: &AppHandle, enabled: bool) {
    let _ = app.run_on_main_thread(move || {
        ADMIN_TAB.with_borrow(|tab| {
            if let Some(button) = tab {
                gtk::prelude::WidgetExt::set_sensitive(button, enabled);
            }
        });
    });
}

#[cfg(not(target_os = "linux"))]
pub fn set_admin_tab_enabled(_app: &AppHandle, _enabled: bool) {}

/// Style for the tab strip. Uses the theme's own colours rather than fixed
/// ones, so it follows whatever GTK theme is in use.
#[cfg(target_os = "linux")]
const TAB_CSS: &str = "
.pve-tabstrip {
  padding: 4px 6px;
  border-bottom: 1px solid @borders;
}
.pve-tabstrip button {
  padding: 1px 18px;
  min-height: 26px;
}
";

/// The tab bar, as real GTK buttons above both webviews.
///
/// Built once at startup. Switching is driven from here rather than from the
/// page: while the admin panel is up the local webview is hidden, and relying
/// on a hidden webview's JavaScript to be able to switch back would be a way
/// to get stuck.
#[cfg(target_os = "linux")]
pub fn install_native_tabs(app: &AppHandle) {
    use gtk::prelude::{BoxExt, ButtonExt, Cast, CssProviderExt, StyleContextExt, WidgetExt};

    let Some(local) = app.get_webview(MAIN) else {
        return;
    };
    let app = app.clone();
    let _ = local.with_webview(move |platform| {
        let Some(parent) = platform.inner().parent() else {
            return;
        };
        let Ok(vbox) = parent.downcast::<gtk::Box>() else {
            return;
        };

        let strip = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        strip.style_context().add_class("pve-tabstrip");

        // "linked" is GTK's own segmented-control look, so the pair reads as
        // one control in whatever theme is running.
        let group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        group.style_context().add_class("linked");

        let admin = gtk::Button::with_label("Admin");
        let guests = gtk::Button::with_label("Guests");
        group.pack_start(&admin, false, false, 0);
        group.pack_start(&guests, false, false, 0);
        strip.pack_start(&group, false, false, 0);

        // Nothing to show until a session exists; `set_admin_tab_enabled`
        // turns it on at connect and off again at logout or session loss.
        admin.set_sensitive(false);
        ADMIN_TAB.with_borrow_mut(|tab| *tab = Some(admin.clone()));

        if let Some(screen) = gtk::gdk::Screen::default() {
            let provider = gtk::CssProvider::new();
            if provider.load_from_data(TAB_CSS.as_bytes()).is_ok() {
                gtk::StyleContext::add_provider_for_screen(
                    &screen,
                    &provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }
        }

        let app_admin = app.clone();
        admin.connect_clicked(move |_| {
            let app = app_admin.clone();
            // Opening may need a login, so it cannot run on this thread.
            tauri::async_runtime::spawn(async move {
                if let Err(e) = show_admin(&app).await {
                    let _ = app.emit("admin-cert", format!("admin panel: {e}"));
                }
            });
        });

        let app_guests = app.clone();
        guests.connect_clicked(move |_| {
            if let Some(webview) = app_guests.get_webview(ADMIN_WEBVIEW) {
                let _ = webview.hide();
            }
            layout_admin(&app_guests, false);
            let _ = app_guests.emit("tab", "guests");
        });

        // Above both webviews. A non-expanding button box has a small,
        // truthful natural height, which is exactly what a webview does not.
        vbox.pack_start(&strip, false, false, 0);
        vbox.reorder_child(&strip, 0);
        strip.show_all();
    });
}

/// F4/F14: show the Proxmox web UI, already logged in, as a webview inside
/// the main window below the tab bar.
///
/// The webview starts blank so the session ticket can be installed before any
/// request goes out, then navigates to the node. It stays a *main-frame* load
/// rather than an iframe, which is what lets the certificate pinning below
/// keep working unchanged.
#[tauri::command]
pub async fn open_admin_panel(app: AppHandle) -> Result<()> {
    show_admin(&app).await
}

/// Build the admin webview if needed, then bring it up. Shared by the command
/// and by the native tab strip's Admin button.
pub async fn show_admin(app: &AppHandle) -> Result<()> {
    let state = app.state::<AppState>();
    let app = app.clone();

    // Already built: bring it back.
    if let Some(existing) = app.get_webview(ADMIN_WEBVIEW) {
        existing.show()?;
        #[cfg(target_os = "linux")]
        layout_admin(&app, true);
        #[cfg(not(target_os = "linux"))]
        if let Some(window) = app.get_webview_window(ADMIN_WEBVIEW) {
            window.set_focus()?;
        }
        return Ok(());
    }

    let (host, session) = current_host(&state).await?;
    let ticket = session.ticket;

    let url: Url = host
        .base_url()
        .parse()
        .map_err(|e| Error::Config(format!("bad base url: {e}")))?;
    let blank: Url = "about:blank".parse().expect("valid url");

    // Linux: a second webview in the main window, under the native tab strip.
    #[cfg(target_os = "linux")]
    let webview = {
        let window = app
            .get_window(MAIN)
            .ok_or(Error::Config("main window is gone".into()))?;
        // Position and size are ignored here — the GtkBox decides (see
        // `layout_admin`) — but `add_child` requires them.
        let webview = window.add_child(
            WebviewBuilder::new(ADMIN_WEBVIEW, WebviewUrl::External(blank)),
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(800.0, 600.0),
        )?;
        layout_admin(&app, true);
        webview
    };

    // Everywhere else: its own window, as before F14. Tabs are Linux-only
    // because the native strip they need is a GTK widget; a child webview
    // with no way to switch back would be worse than a separate window.
    #[cfg(not(target_os = "linux"))]
    let webview = WebviewWindowBuilder::new(&app, ADMIN_WEBVIEW, WebviewUrl::External(blank))
        .title("Proxmox")
        .inner_size(1280.0, 860.0)
        .center()
        .focused(true)
        .build()?;

    // Hand the session ticket over so the panel opens already logged in,
    // rather than showing the Proxmox login form.
    let cookie = Cookie::build(("PVEAuthCookie", ticket))
        .domain(host.host.clone())
        .path("/")
        .secure(true)
        .build();
    webview.set_cookie(cookie)?;

    #[cfg(target_os = "linux")]
    setup_admin_webview(
        &webview,
        app.clone(),
        host.name.clone(),
        host.host.clone(),
        url.to_string(),
    )?;
    #[cfg(not(target_os = "linux"))]
    webview.navigate(url)?;

    Ok(())
}

/// F14: switch back to the local Guests tab.
///
/// The webview is hidden rather than closed, so returning to the Admin tab
/// doesn't reload the Proxmox UI or need another ticket.
#[tauri::command]
pub fn hide_admin_panel(app: AppHandle) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if let Some(webview) = app.get_webview(ADMIN_WEBVIEW) {
            webview.hide()?;
        }
        // The local webview expands to fill the window again.
        layout_admin(&app, false);
    }
    // No tab to switch to off Linux: the panel is its own window, so hiding
    // it would strand it. Closing is what the caller (session loss) wants.
    #[cfg(not(target_os = "linux"))]
    if let Some(window) = app.get_webview_window(ADMIN_WEBVIEW) {
        window.close()?;
    }
    Ok(())
}

/// Configure the admin panel's webview, then load it.
///
/// The Proxmox cert is signed by the cluster's own CA, which is not in the
/// system trust store WebKitGTK reads, so the load fails with a TLS error and
/// WebKitGTK offers no way to click through. We catch that failure, check the
/// certificate's SHA-256 against the fingerprint saved in the config, allow
/// that exact certificate for that one host, and retry the load.
///
/// First connect stores the fingerprint it sees (and reports it to the UI).
/// Afterwards a certificate that does not match is refused — the window keeps
/// WebKit's error page rather than trusting a cert that changed underneath us.
///
/// Narrower than trusting the cluster CA system-wide: that would let the CA
/// vouch for any host, this accepts one certificate for one host, inside this
/// app only.
#[cfg(target_os = "linux")]
fn setup_admin_webview(
    webview: &tauri::Webview,
    app: AppHandle,
    entry: String,
    hostname: String,
    target: String,
) -> Result<()> {
    use std::cell::Cell;
    use webkit2gtk::gio::prelude::TlsCertificateExt;
    use webkit2gtk::glib;
    use webkit2gtk::{WebContextExt, WebViewExt};

    webview.with_webview(move |platform| {
        let webview = platform.inner();
        // One retry only: if the load still fails after allowing the cert,
        // let the error page stand instead of looping.
        let retried = Cell::new(false);

        webview.connect_load_failed_with_tls_errors(move |webview, uri, cert, _errors| {
            if retried.replace(true) {
                return false;
            }
            let Some(der) = cert.certificate() else {
                return false;
            };
            let Some(seen) = glib::compute_checksum_for_data(glib::ChecksumType::Sha256, &der)
            else {
                return false;
            };
            let seen = seen.to_string();

            match pin_fingerprint(&entry, &seen) {
                Ok(Pinned::Matches) => {}
                Ok(Pinned::Stored) => {
                    let _ = app.emit("admin-cert", format!("pinned node certificate {seen}"));
                }
                Ok(Pinned::Mismatch(expected)) => {
                    let _ = app.emit(
                        "admin-cert",
                        format!(
                            "REFUSED: node certificate is {seen}, expected {expected}. \
                             Not loading the admin panel."
                        ),
                    );
                    return false;
                }
                Err(e) => {
                    let _ = app.emit("admin-cert", format!("certificate check failed: {e}"));
                    return false;
                }
            }

            let Some(context) = webview.context() else {
                return false;
            };
            context.allow_tls_certificate_for_host(cert, &hostname);
            webview.load_uri(uri);
            true
        });

        webview.load_uri(&target);
    })?;
    Ok(())
}

/// Outcome of comparing a certificate against the pinned fingerprint.
#[cfg(target_os = "linux")]
enum Pinned {
    /// Matches what the config already pinned.
    Matches,
    /// Nothing was pinned; this fingerprint has now been saved.
    Stored,
    /// Does not match the pinned fingerprint, which is carried back.
    Mismatch(String),
}

/// Compare `seen` against the active host's pinned fingerprint, storing it
/// if nothing is pinned yet.
#[cfg(target_os = "linux")]
fn pin_fingerprint(entry: &str, seen: &str) -> Result<Pinned> {
    let mut cfg = config::load()?.ok_or(Error::NotConfigured)?;
    let pinned = cfg.host(entry)?.cert_fingerprint.clone();
    match pinned {
        Some(expected) if expected == seen => Ok(Pinned::Matches),
        Some(expected) => Ok(Pinned::Mismatch(expected)),
        None => {
            let target = cfg
                .hosts
                .iter_mut()
                .find(|h| h.name == entry)
                .ok_or_else(|| Error::Config(format!("no host entry named {entry:?}")))?;
            target.cert_fingerprint = Some(seen.to_string());
            config::save(&cfg)?;
            Ok(Pinned::Stored)
        }
    }
}

/// Drop the session and tear down the admin panel.
///
/// Saved credentials are left alone: the config file and the keyring entry
/// stay, so the next launch auto-logs in again.
#[tauri::command]
pub fn logout(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    state.clear();
    set_admin_tab_enabled(&app, false);
    // Closed, not hidden: the webview holds a session cookie, and the next
    // login may be a different connection entirely.
    if let Some(panel) = app.get_webview(ADMIN_WEBVIEW) {
        panel.close()?;
    }
    #[cfg(target_os = "linux")]
    layout_admin(&app, false);
    Ok(())
}
