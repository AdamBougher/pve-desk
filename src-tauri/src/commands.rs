//! Tauri commands: the bridge between the HTML frontend and Rust.
//! The frontend calls these with `window.__TAURI__.core.invoke("name", {...})`.

use crate::api::{self, Guest, Session};
use crate::config::{self, Config, HostConfig};
use crate::error::{Error, Result};
use crate::platform::{Os, Platform};
use crate::{secrets, spice, AppState};
use tauri::webview::Cookie;
use tauri::{AppHandle, Emitter, Manager, State, Url};

/// Label of the admin panel's webview (F14).
///
/// Deliberately absent from `capabilities/default.json`: it loads the remote
/// Proxmox UI, and no capability naming it means it cannot reach a single app
/// command (CLAUDE.md hard rule). Note the capability names `webviews`, not
/// `windows` — a window-scoped capability applies to *every* webview in that
/// window, which would hand this one `core:default`.
pub(crate) const ADMIN_WEBVIEW: &str = "admin";

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
    Os::set_admin_enabled(app, false);
    let _ = app.emit("session-lost", why.to_string());
}

/// The connected session together with the connection it belongs to,
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
    /// This connection is the live session.
    pub connected: bool,
    /// Used most recently; the launcher preselects it.
    pub last_used: bool,
}

/// Result of probing one connection (F16, Q16.2).
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

/// F16: check one connection. Called per row so the launcher fills in as results
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
/// certificate in the UI; it is pinned to the connection once the login succeeds.
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
    Os::set_admin_enabled(&app, true);

    // Persist what the user just confirmed, so later connects verify against
    // it instead of asking again.
    let mut dirty = false;
    if let Some(fp) = trust_fingerprint {
        if let Some(entry) = cfg.hosts.iter_mut().find(|h| h.name == name) {
            entry.cert_fingerprint = Some(fp);
            dirty = true;
        }
    }
    // Remember which connection is the default next time (Q16.4).
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

/// Home's saved sort (F26). Separate from the config file's shape so the
/// frontend reads two plain fields rather than the whole `Config`.
#[derive(serde::Serialize)]
pub struct Sort {
    pub column: String,
    pub desc: bool,
}

/// F26: the remembered sort, or the default when nothing is saved yet.
///
/// Returns the default rather than an error when there is no config file at
/// all — Home can be reached only through a connection, but a missing file
/// is not a reason to refuse to order a list.
#[tauri::command]
pub fn get_sort() -> Result<Sort> {
    let Some(cfg) = config::load()? else {
        return Ok(Sort {
            column: config::sort_or_default("").to_string(),
            desc: false,
        });
    };
    Ok(Sort {
        column: config::sort_or_default(&cfg.sort).to_string(),
        desc: cfg.sort_desc,
    })
}

/// F26 Q26.2: remember the sort across restarts.
///
/// `column` is checked against `config::SORT_COLUMNS` before anything is
/// written, so the frontend cannot put a column the UI has no ordering for
/// into the config file — the same shape of guard as the power allow-list,
/// for the same reason.
#[tauri::command]
pub fn set_sort(column: String, desc: bool) -> Result<()> {
    if !config::SORT_COLUMNS.contains(&column.as_str()) {
        return Err(Error::Config(format!("not a sortable column: {column:?}")));
    }
    let Some(mut cfg) = config::load()? else {
        // Nothing saved yet means no connection has been added, so there is
        // no file to add a preference to. Not an error: the sort still works
        // for this session, it just has nowhere to be remembered.
        return Ok(());
    };
    if cfg.sort == column && cfg.sort_desc == desc {
        return Ok(());
    }
    cfg.sort = column;
    cfg.sort_desc = desc;
    config::save(&cfg)
}

/// F11: the pinned vmids of the live connection.
///
/// Read from the session's own entry, never from a name the page sends, so
/// Home cannot show the pins of a different cluster.
#[tauri::command]
pub fn get_pins(state: State<'_, AppState>) -> Result<Vec<u32>> {
    let (name, _) = state.current().ok_or(Error::NotLoggedIn)?;
    let cfg = config::load()?.ok_or(Error::NotConfigured)?;
    Ok(cfg.host(&name)?.pinned.clone())
}

/// F11: pin or unpin one guest on the live connection.
///
/// The page sends a vmid and a flag only; the connection comes from the
/// session. The file is written only when the list changed.
#[tauri::command]
pub fn set_pinned(state: State<'_, AppState>, vmid: u32, pinned: bool) -> Result<()> {
    let (name, _) = state.current().ok_or(Error::NotLoggedIn)?;
    let mut cfg = config::load()?.ok_or(Error::NotConfigured)?;
    let entry = cfg
        .hosts
        .iter_mut()
        .find(|h| h.name == name)
        .ok_or_else(|| Error::Config(format!("no connection named {name:?}")))?;
    if entry.set_pinned(vmid, pinned) {
        config::save(&cfg)?;
    }
    Ok(())
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

/// F1/F16: test the login, then save the connection and its password.
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
        // `upsert_host` keeps the pins of an entry that already exists.
        pinned: Vec::new(),
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
    Os::set_admin_enabled(&app, true);
    Ok(ConnectOutcome::Connected)
}

/// F19: remove a saved connection — its config entry and its keyring
/// password, which is all this app ever stored for it. Nothing on the node is
/// touched.
///
/// Refused for the connection that holds the live session: the session is keyed
/// by that connection's id, so deleting it underneath would leave a session whose
/// refresh and admin panel can no longer resolve a host. Log out first. The
/// launcher greys the button out too; this is the half that cannot be
/// bypassed from the page.
///
/// **Password first, config second.** A keyring that refuses (locked, no
/// Secret Service) then leaves everything in place to retry, rather than
/// deleting the connection that is the only way back to the secret.
#[tauri::command]
pub fn remove_connection(state: State<'_, AppState>, name: String) -> Result<()> {
    if state.is_connected(&name) {
        return Err(Error::Connected);
    }
    let mut cfg = config::load()?.ok_or(Error::NotConfigured)?;
    let host = cfg.host(&name)?.clone();
    secrets::delete_password(&host.userid(), &host.host)?;
    cfg.remove_host(&name)?;
    config::save(&cfg)?;
    Ok(())
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

/// F6: start / shutdown / stop one guest.
///
/// `action` is checked against the allow-list in `api::check_power` before a
/// URL is built, so the three the UI offers (Q6.1) are the three that can
/// reach the node. The confirmation on a forced stop (Q6.2) is the page's
/// job; this command does what it is told, as the Proxmox API would.
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

/// F33: whether this build has the native top bar, which holds Log out.
///
/// The page hides its own Log out when this is true, so there is one Log out
/// only (Q33.1). Linux-only like the strip itself (F14).
#[tauri::command]
pub fn has_top_bar() -> bool {
    Os::has_top_bar()
}

/// Write a status message where it stays visible while Admin covers the
/// page (F21). On Linux that is the native label beside the tab strip;
/// elsewhere Admin is its own window and never covers anything, so this
/// does nothing.
#[tauri::command]
pub fn set_status(app: AppHandle, msg: String) {
    Os::set_status(&app, msg);
}

/// F4/F14: show the Proxmox web UI, already logged in. On Linux it is a
/// webview inside the main window below the tab bar; elsewhere its own
/// window (`platform`).
///
/// The webview starts blank so the session ticket can be installed before any
/// request goes out, then navigates to the node. It stays a *main-frame* load
/// rather than an iframe, which is what lets the certificate pinning keep
/// working unchanged.
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
        return Os::show(&app);
    }

    let (host, session) = current_host(&state).await?;
    let ticket = session.ticket;

    let url: Url = host
        .base_url()
        .parse()
        .map_err(|e| Error::Config(format!("bad base url: {e}")))?;
    let blank: Url = "about:blank".parse().expect("valid url");

    let webview = Os::create_admin(&app, ADMIN_WEBVIEW, blank)?;

    // Hand the session ticket over so the panel opens already logged in,
    // rather than showing the Proxmox login form.
    let cookie = Cookie::build(("PVEAuthCookie", ticket))
        .domain(host.host.clone())
        .path("/")
        .secure(true)
        .build();
    webview.set_cookie(cookie)?;

    Os::attach(
        &webview,
        app.clone(),
        host.name.clone(),
        host.host.clone(),
        url,
    )?;
    Os::show(&app)
}

/// F14/F21: switch back to the local Home tab.
///
/// On Linux the webview is hidden rather than closed, so returning to the
/// Admin tab doesn't reload the Proxmox UI or need another ticket. Elsewhere
/// the panel's window is closed.
#[tauri::command]
pub fn hide_admin_panel(app: AppHandle) -> Result<()> {
    Os::hide(&app)
}

/// The page's Log out (Windows and macOS, where there is no top bar).
#[tauri::command]
pub fn logout(app: AppHandle) -> Result<()> {
    end_session(&app)
}

/// Drop the session and tear down the admin panel, then tell the page
/// (F33 Q33.2). The one teardown behind both the top bar's Log out and the
/// `logout` command.
///
/// Everything a user can see outside the page happens here: the ticket goes,
/// the Admin webview closes, Home is shown, Admin and Log out grey out, and
/// the title says Home. The page's `logged-out` listener only clears its own
/// view and shows the launcher.
///
/// Saved credentials are left alone: the config file and the keyring entry
/// stay, so the next launch auto-logs in again.
pub fn end_session(app: &AppHandle) -> Result<()> {
    app.state::<AppState>().clear();
    Os::set_admin_enabled(app, false);
    // Closed, not hidden: the webview holds a session cookie, and the next
    // login may be a different connection entirely. A failed close is
    // reported, but only after the rest has run: the session is already gone,
    // so the user must still land on Home.
    let closed = match app.get_webview(ADMIN_WEBVIEW) {
        Some(panel) => panel.close(),
        None => Ok(()),
    };
    Os::show_home(app);
    let _ = app.emit("logged-out", ());
    Ok(closed?)
}
