use serde::Serialize;

/// Single error type for the app. Serialized as a plain string so the
/// frontend can show it directly.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not configured yet — run first-time setup")]
    NotConfigured,
    #[error("not logged in")]
    NotLoggedIn,
    #[error("config error: {0}")]
    Config(String),
    #[error("keyring error: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Proxmox API error: {0}")]
    Api(String),
    #[error("certificate not trusted (SHA-256 {0})")]
    CertUnverified(String),
    #[error("session rejected by the node")]
    Unauthorized,
    /// F19: refuses a destructive action on the connection that holds the live
    /// session. Shown to the user as-is, so it reads as an instruction.
    #[error("this connection is signed in — log out before removing it")]
    Connected,
    /// F39: Admin on an OS whose webview cannot pin (Windows, macOS), for a
    /// node whose certificate the OS trust store does not vouch for. Shown
    /// to the user as-is, so it reads as an instruction.
    #[error(
        "Admin cannot open: this system does not trust the node's certificate. \
         Import the cluster CA (/etc/pve/pve-root-ca.pem on the node) into the \
         system trust store, then log out and connect again."
    )]
    AdminNeedsCa,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("window error: {0}")]
    Tauri(#[from] tauri::Error),
    #[error("not implemented yet: {0}")]
    Todo(&'static str),
}

impl Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
