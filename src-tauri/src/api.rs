//! Proxmox VE API client.
//!
//! Auth flow (F2):
//!   POST /api2/json/access/ticket  (username=user@realm, password=...)
//!   -> data.ticket               sent as cookie `PVEAuthCookie` on every request
//!   -> data.CSRFPreventionToken  sent as header on POST/PUT/DELETE
//! Tickets are valid ~2 hours; we re-login before that.

use crate::config::HostConfig;
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// A logged-in session.
#[derive(Clone)]
pub struct Session {
    pub base_url: String,
    pub ticket: String,
    pub csrf_token: String,
    pub obtained_at: Instant,
    http: reqwest::Client,
}

/// Wrapper every Proxmox API response uses: `{ "data": ... }`.
#[derive(Deserialize)]
struct ApiResponse<T> {
    data: T,
}

#[derive(Deserialize)]
struct TicketData {
    ticket: String,
    #[serde(rename = "CSRFPreventionToken")]
    csrf_token: String,
}

/// One row from `/cluster/resources?type=vm` (VMs and LXC containers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Guest {
    pub vmid: u32,
    #[serde(default)]
    pub name: Option<String>,
    /// `qemu` or `lxc`
    #[serde(rename = "type")]
    pub kind: String,
    pub node: String,
    pub status: String,
    #[serde(default)]
    pub cpu: Option<f64>,
    #[serde(default)]
    pub maxcpu: Option<u32>,
    #[serde(default)]
    pub mem: Option<u64>,
    #[serde(default)]
    pub maxmem: Option<u64>,
    #[serde(default)]
    pub template: Option<u8>,
    #[serde(default)]
    pub tags: Option<String>,
}

/// Build the HTTP client.
///
/// TODO(F3): replace `danger_accept_invalid_certs` with a custom rustls
/// verifier that checks the pinned SHA-256 fingerprint from `Config`.
/// Until then this skeleton accepts any cert — do NOT use outside your LAN.
fn http_client(_host: &HostConfig) -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?)
}

/// F16: can the node be reached at all? Sends no credentials.
///
/// Probes `/` (the login page), **not** an API endpoint: pveproxy delays
/// unauthenticated API responses by ~3s to slow brute-forcing, so an
/// unauthenticated `/api2/json/*` probe costs 3 seconds and looks like a
/// failed login attempt. Measured against the node: `/` answers in ~8ms,
/// `/api2/json/version` without a cookie takes 3.005s.
pub async fn reachable(host: &HostConfig) -> bool {
    let Ok(http) = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(std::time::Duration::from_secs(5))
        .build()
    else {
        return false;
    };
    let url = format!("{}/", host.base_url());
    http.get(&url).send().await.is_ok()
}

/// Log in with username + password and return a session.
pub async fn login(host: &HostConfig, password: &str) -> Result<Session> {
    let http = http_client(host)?;
    let url = format!("{}/api2/json/access/ticket", host.base_url());
    let resp = http
        .post(&url)
        .form(&[("username", host.userid().as_str()), ("password", password)])
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(Error::Api(format!("login failed: HTTP {}", resp.status())));
    }

    let body: ApiResponse<TicketData> = resp.json().await?;
    Ok(Session {
        base_url: host.base_url(),
        ticket: body.data.ticket,
        csrf_token: body.data.csrf_token,
        obtained_at: Instant::now(),
        http,
    })
}

impl Session {
    fn cookie(&self) -> String {
        format!("PVEAuthCookie={}", self.ticket)
    }

    /// True once the ticket is old enough that we should re-login (F2).
    #[allow(dead_code)]
    pub fn needs_refresh(&self) -> bool {
        self.obtained_at.elapsed().as_secs() > 90 * 60
    }

    /// F16: is this ticket still accepted? `/version` needs auth (verified
    /// against the node: 401 without a cookie), so a 200 proves the ticket.
    ///
    /// A *valid* ticket answers immediately. An expired one pays pveproxy's
    /// ~3s unauthenticated-request delay — acceptable, since it only happens
    /// when the session has actually lapsed.
    pub async fn is_valid(&self) -> bool {
        let url = format!("{}/api2/json/version", self.base_url);
        match self
            .http
            .get(&url)
            .header("Cookie", self.cookie())
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// F5: list VMs and containers across the cluster.
    pub async fn list_guests(&self) -> Result<Vec<Guest>> {
        let url = format!("{}/api2/json/cluster/resources?type=vm", self.base_url);
        let resp = self
            .http
            .get(&url)
            .header("Cookie", self.cookie())
            .send()
            .await?
            .error_for_status()?;
        let body: ApiResponse<Vec<Guest>> = resp.json().await?;
        Ok(body.data)
    }

    /// F6: start / shutdown / stop / reboot.
    pub async fn power(&self, node: &str, kind: &str, vmid: u32, action: &str) -> Result<()> {
        // Allow-list so the frontend can't hit arbitrary endpoints.
        // Final list depends on FEATURES.md Q6.1.
        const ALLOWED: &[&str] = &["start", "shutdown", "stop", "reboot"];
        if !ALLOWED.contains(&action) {
            return Err(Error::Api(format!("action not allowed: {action}")));
        }
        if kind != "qemu" && kind != "lxc" {
            return Err(Error::Api(format!("unknown guest type: {kind}")));
        }
        let url = format!(
            "{}/api2/json/nodes/{node}/{kind}/{vmid}/status/{action}",
            self.base_url
        );
        self.http
            .post(&url)
            .header("Cookie", self.cookie())
            .header("CSRFPreventionToken", &self.csrf_token)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// F7: ask Proxmox for SPICE connection details. Returns the raw
    /// key/value map that goes into the `.vv` file.
    pub async fn spice_proxy(
        &self,
        node: &str,
        vmid: u32,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let url = format!(
            "{}/api2/json/nodes/{node}/qemu/{vmid}/spiceproxy",
            self.base_url
        );
        let resp = self
            .http
            .post(&url)
            .header("Cookie", self.cookie())
            .header("CSRFPreventionToken", &self.csrf_token)
            .send()
            .await?
            .error_for_status()?;
        let body: ApiResponse<serde_json::Map<String, serde_json::Value>> = resp.json().await?;
        Ok(body.data)
    }
}
