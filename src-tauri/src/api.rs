//! Proxmox VE API client.
//!
//! Auth flow (F2):
//!   POST /api2/json/access/ticket  (username=user@realm, password=...)
//!   -> data.ticket               sent as cookie `PVEAuthCookie` on every request
//!   -> data.CSRFPreventionToken  sent as header on POST/PUT/DELETE
//! Tickets are valid ~2 hours; we re-login before that.

use crate::config::HostConfig;
use crate::error::{Error, Result};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
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

/// Lowercase hex SHA-256 of a certificate's DER bytes — the same value the
/// admin panel pins and that `config.cert_fingerprint` stores.
fn fingerprint(cert: &CertificateDer<'_>) -> String {
    let digest = Sha256::digest(cert.as_ref());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// What the verifier saw during a handshake.
///
/// Recorded even when verification fails, so a rejected connection can tell
/// the user *which* certificate was offered without handshaking again.
#[derive(Debug, Default)]
struct Sighting {
    fingerprint: Option<String>,
    accepted: bool,
}

/// Accepts a server certificate if it verifies normally against the OS trust
/// store, **or** if its SHA-256 matches the fingerprint pinned for this host
/// (F3, Q3.3).
///
/// The standard path means a real certificate — a Tailscale or Let's Encrypt
/// one, or a CA imported into the system store — works without pinning and
/// survives renewal. The pinned path covers Proxmox's default self-signed
/// certificate, which no CA vouches for.
#[derive(Debug)]
struct PinnedVerifier {
    inner: Arc<dyn ServerCertVerifier>,
    pinned: Option<String>,
    seen: Arc<Mutex<Sighting>>,
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        let seen = fingerprint(end_entity);
        let standard = self.inner.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        );

        let accepted = standard.is_ok()
            || self
                .pinned
                .as_deref()
                .is_some_and(|p| p.eq_ignore_ascii_case(&seen));

        if let Ok(mut record) = self.seen.lock() {
            record.fingerprint = Some(seen);
            record.accepted = accepted;
        }

        match standard {
            Ok(ok) => Ok(ok),
            // Pin matched: trust it despite no CA vouching for it.
            Err(_) if accepted => Ok(ServerCertVerified::assertion()),
            Err(e) => Err(e),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// Build an HTTP client that verifies the node's certificate.
///
/// `pinned` is the fingerprint to accept in addition to anything the OS trust
/// store already vouches for. Returns the shared sighting so the caller can
/// report the offered certificate when the handshake is refused.
fn http_client(
    pinned: Option<String>,
    timeout: Option<std::time::Duration>,
) -> Result<(reqwest::Client, Arc<Mutex<Sighting>>)> {
    let mut roots = rustls::RootCertStore::empty();
    let loaded = rustls_native_certs::load_native_certs();
    for cert in loaded.certs {
        let _ = roots.add(cert);
    }

    let inner = rustls::client::WebPkiServerVerifier::builder(Arc::new(roots))
        .build()
        .map_err(|e| Error::Api(format!("could not build TLS verifier: {e}")))?;

    let seen = Arc::new(Mutex::new(Sighting::default()));
    let verifier = Arc::new(PinnedVerifier {
        inner,
        pinned,
        seen: Arc::clone(&seen),
    });

    let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| Error::Api(format!("TLS configuration: {e}")))?
    .dangerous()
    .with_custom_certificate_verifier(verifier)
    .with_no_client_auth();

    let mut builder = reqwest::Client::builder().use_preconfigured_tls(tls);
    if let Some(t) = timeout {
        builder = builder.timeout(t);
    }
    Ok((builder.build()?, seen))
}

/// Turn a transport failure into a certificate error when the handshake was
/// refused over the certificate specifically.
fn cert_rejection(seen: &Arc<Mutex<Sighting>>) -> Option<Error> {
    let record = seen.lock().ok()?;
    match (&record.fingerprint, record.accepted) {
        (Some(fp), false) => Some(Error::CertUnverified(fp.clone())),
        _ => None,
    }
}

/// F16: can the node be reached at all? Sends no credentials.
///
/// Probes `/` (the login page), **not** an API endpoint: pveproxy delays
/// responses to unauthenticated `/api2/json/*` requests by ~3s, so probing
/// one would take 3 seconds and would log a pointless failed authentication
/// on the node every time. Measured: `/` answers in ~8ms,
/// `/api2/json/version` without a cookie takes 3.005s.
pub async fn reachable(host: &HostConfig) -> bool {
    let Ok((http, seen)) = http_client(
        host.cert_fingerprint.clone(),
        Some(std::time::Duration::from_secs(5)),
    ) else {
        return false;
    };
    let url = format!("{}/", host.base_url());
    if http.get(&url).send().await.is_ok() {
        return true;
    }
    // A certificate we refused still means something answered on that port.
    seen.lock()
        .map(|s| s.fingerprint.is_some())
        .unwrap_or(false)
}

/// Log in with username + password and return a session.
/// `trust` is a fingerprint the user has just confirmed, accepted in addition
/// to the one already pinned in the config (F3, Q3.3). The password is only
/// sent once the TLS handshake succeeds, so a refused certificate never sees
/// it.
pub async fn login(host: &HostConfig, password: &str, trust: Option<&str>) -> Result<Session> {
    let pinned = trust
        .map(str::to_string)
        .or_else(|| host.cert_fingerprint.clone());
    let (http, seen) = http_client(pinned, None)?;
    let url = format!("{}/api2/json/access/ticket", host.base_url());
    let resp = http
        .post(&url)
        .form(&[("username", host.userid().as_str()), ("password", password)])
        .send()
        .await
        .map_err(|e| cert_rejection(&seen).unwrap_or(Error::Http(e)))?;

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
