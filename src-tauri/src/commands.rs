//! Tauri commands: the bridge between the HTML frontend and Rust.
//! The frontend calls these with `window.__TAURI__.core.invoke("name", {...})`.

use crate::api::{self, Guest, Session};
use crate::config::{self, Config, HostConfig};
use crate::error::{Error, Result};
use crate::{secrets, spice, AppState};
use tauri::webview::Cookie;
use tauri::{AppHandle, Manager, State, Url, WebviewUrl, WebviewWindowBuilder};

/// Label of the admin panel window.
///
/// Deliberately absent from `capabilities/default.json`: that window loads the
/// remote Proxmox UI, and no capability naming it means it cannot reach a
/// single app command (CLAUDE.md hard rule).
const ADMIN_WINDOW: &str = "admin";

/// The connected session, or an error if nothing is connected.
fn current_session(state: &State<'_, AppState>) -> Result<Session> {
    state.current().map(|(_, s)| s).ok_or(Error::NotLoggedIn)
}

/// The connected session together with the config entry it belongs to.
fn current_host(state: &State<'_, AppState>) -> Result<(HostConfig, Session)> {
    let (name, session) = state.current().ok_or(Error::NotLoggedIn)?;
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

/// F16: connect to a saved entry using its keyring password.
#[tauri::command]
pub async fn connect(state: State<'_, AppState>, name: String) -> Result<()> {
    let mut cfg = config::load()?.ok_or(Error::NotConfigured)?;
    let host = cfg.host(&name)?.clone();
    let password = secrets::get_password(&host.userid(), &host.host)?;
    let session = api::login(&host, &password).await?;
    state.set_only(name.clone(), session);

    // Remember which entry to preselect next time (Q16.4).
    if cfg.default != name {
        cfg.default = name;
        config::save(&cfg)?;
    }
    // TODO(F2): spawn a background task that re-logs-in every ~90 min.
    Ok(())
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
) -> Result<()> {
    let host = host.trim().to_string();
    let username = username.trim().to_string();
    let realm = realm.trim().to_string();
    let entry = HostConfig {
        name: HostConfig::id(&username, &realm, &host),
        host,
        port,
        username,
        realm,
        cert_fingerprint: None, // TODO(F3): capture + confirm on first connect
    };
    entry.validate()?;

    // Only save if the login actually works.
    let session = api::login(&entry, &password).await?;
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
    Ok(())
}

/// F5
#[tauri::command]
pub async fn list_guests(state: State<'_, AppState>) -> Result<Vec<Guest>> {
    current_session(&state)?.list_guests().await
}

/// F6
#[tauri::command]
pub async fn power_action(
    state: State<'_, AppState>,
    node: String,
    kind: String,
    vmid: u32,
    action: String,
) -> Result<()> {
    current_session(&state)?
        .power(&node, &kind, vmid, &action)
        .await
}

/// F7
#[tauri::command]
pub async fn open_spice(state: State<'_, AppState>, node: String, vmid: u32) -> Result<()> {
    let cfg = config::load()?.ok_or(Error::NotConfigured)?;
    let data = current_session(&state)?.spice_proxy(&node, vmid).await?;
    spice::launch(&cfg.remote_viewer_path, &data)
}

/// F4: open the Proxmox web UI in its own window, already logged in.
///
/// The window starts blank so the session ticket can be installed before any
/// request goes out, then navigates to the node. WebKitGTK uses the system
/// trust store, so this only renders once the Proxmox cluster CA is trusted
/// on this machine — otherwise it shows a TLS error page.
#[tauri::command]
pub async fn open_admin_panel(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    if let Some(existing) = app.get_webview_window(ADMIN_WINDOW) {
        existing.set_focus()?;
        return Ok(());
    }

    let (host, session) = current_host(&state)?;
    let ticket = session.ticket;

    let url: Url = host
        .base_url()
        .parse()
        .map_err(|e| Error::Config(format!("bad base url: {e}")))?;
    let blank: Url = "about:blank".parse().expect("valid url");

    let window = WebviewWindowBuilder::new(&app, ADMIN_WINDOW, WebviewUrl::External(blank))
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
    window.set_cookie(cookie)?;

    #[cfg(target_os = "linux")]
    setup_admin_webview(&window, app.clone(), host.host.clone(), url.to_string())?;
    #[cfg(not(target_os = "linux"))]
    window.navigate(url)?;

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
    window: &tauri::WebviewWindow,
    app: AppHandle,
    hostname: String,
    target: String,
) -> Result<()> {
    use std::cell::Cell;
    use tauri::Emitter;
    use webkit2gtk::gio::prelude::TlsCertificateExt;
    use webkit2gtk::glib;
    use webkit2gtk::{WebContextExt, WebViewExt};

    window.with_webview(move |platform| {
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

            match pin_fingerprint(&seen) {
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
fn pin_fingerprint(seen: &str) -> Result<Pinned> {
    let mut cfg = config::load()?.ok_or(Error::NotConfigured)?;
    let pinned = cfg.active()?.cert_fingerprint.clone();
    match pinned {
        Some(expected) if expected == seen => Ok(Pinned::Matches),
        Some(expected) => Ok(Pinned::Mismatch(expected)),
        None => {
            let name = cfg.default.clone();
            let entry = cfg
                .hosts
                .iter_mut()
                .find(|h| h.name == name)
                .ok_or_else(|| Error::Config(format!("no host entry named {name:?}")))?;
            entry.cert_fingerprint = Some(seen.to_string());
            config::save(&cfg)?;
            Ok(Pinned::Stored)
        }
    }
}

/// Drop the session and close the admin panel.
///
/// Saved credentials are left alone: the config file and the keyring entry
/// stay, so the next launch auto-logs in again.
#[tauri::command]
pub fn logout(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    state.clear();
    if let Some(panel) = app.get_webview_window(ADMIN_WINDOW) {
        panel.close()?;
    }
    Ok(())
}
