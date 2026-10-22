//! Tauri commands: the bridge between the HTML frontend and Rust.
//! The frontend calls these with `window.__TAURI__.core.invoke("name", {...})`.

use crate::api::{self, Guest, Session};
use crate::config::{self, Config, HostConfig};
use crate::error::{Error, Result};
use crate::{secrets, spice, AppState};
use tauri::webview::{Cookie, WebviewBuilder};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, Url, WebviewUrl, Window,
};

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

/// Height of the local tab bar, in logical pixels. The admin webview is
/// positioned below it, so this must match `--tab-bar-height` in
/// `ui/style.css` or the bar is either covered or leaves a gap.
const TAB_BAR: f64 = 36.0;

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

/// F1/F16: test the login, then save the entry and its password.
#[tauri::command]
pub async fn add_connection(
    state: State<'_, AppState>,
    host: String,
    port: u16,
    username: String,
    realm: String,
    password: String,
    trust_fingerprint: Option<String>,
) -> Result<ConnectOutcome> {
    let host = host.trim().to_string();
    let username = username.trim().to_string();
    let realm = realm.trim().to_string();
    let entry = HostConfig {
        name: HostConfig::id(&username, &realm, &host),
        host,
        port,
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

/// Where the admin webview sits: the full client area below the tab bar.
fn admin_bounds(window: &Window) -> Result<(LogicalPosition<f64>, LogicalSize<f64>)> {
    let scale = window.scale_factor()?;
    let size = window.inner_size()?.to_logical::<f64>(scale);
    Ok((
        LogicalPosition::new(0.0, TAB_BAR),
        LogicalSize::new(size.width, (size.height - TAB_BAR).max(0.0)),
    ))
}

/// Keep the admin webview matched to the window.
///
/// Registered once as a resize handler in `lib.rs`, not per call — Tauri
/// accumulates window event handlers, so registering it on each connect would
/// stack duplicates. `auto_resize()` is deliberately not used: it stores
/// position and size as *proportions* of the window, so a fixed-height tab bar
/// would drift (at 700px tall a 36px offset becomes 72px at 1400px tall).
pub fn relayout_admin(app: &AppHandle) {
    let (Some(webview), Some(window)) = (app.get_webview(ADMIN_WEBVIEW), app.get_window(MAIN))
    else {
        return;
    };
    if let Ok((pos, size)) = admin_bounds(&window) {
        let _ = webview.set_position(pos);
        let _ = webview.set_size(size);
    }
}

/// F4/F14: show the Proxmox web UI, already logged in, as a webview inside
/// the main window below the tab bar.
///
/// The webview starts blank so the session ticket can be installed before any
/// request goes out, then navigates to the node. It stays a *main-frame* load
/// rather than an iframe, which is what lets the certificate pinning below
/// keep working unchanged.
#[tauri::command]
pub async fn open_admin_panel(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let window = app
        .get_window(MAIN)
        .ok_or(Error::Config("main window is gone".into()))?;

    // Already built: just bring it back and re-fit it.
    if let Some(existing) = app.get_webview(ADMIN_WEBVIEW) {
        let (pos, size) = admin_bounds(&window)?;
        existing.set_position(pos)?;
        existing.set_size(size)?;
        existing.show()?;
        return Ok(());
    }

    let (host, session) = current_host(&state).await?;
    let ticket = session.ticket;

    let url: Url = host
        .base_url()
        .parse()
        .map_err(|e| Error::Config(format!("bad base url: {e}")))?;
    let blank: Url = "about:blank".parse().expect("valid url");

    let (pos, size) = admin_bounds(&window)?;
    let webview = window.add_child(
        WebviewBuilder::new(ADMIN_WEBVIEW, WebviewUrl::External(blank)),
        pos,
        size,
    )?;

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
    if let Some(webview) = app.get_webview(ADMIN_WEBVIEW) {
        webview.hide()?;
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
    // Closed, not hidden: the webview holds a session cookie, and the next
    // login may be a different connection entirely.
    if let Some(panel) = app.get_webview(ADMIN_WEBVIEW) {
        panel.close()?;
    }
    Ok(())
}
