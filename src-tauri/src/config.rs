//! Non-secret settings, stored at `~/.config/pve-desk/config.toml`.
//! The password is NOT stored here — see `secrets.rs`.
//!
//! Schema (FEATURES.md Q1.1): app-level settings at the top level, then an
//! `[[hosts]]` array. v1 writes exactly one entry, named `main`, and shows no
//! host picker; the array exists so adding a second host later needs no
//! format migration.
//!
//! Field order matters: TOML requires scalar keys before an array of tables,
//! so `hosts` must stay the last field of `Config`.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Realms offered by the setup form (FEATURES.md Q1.3).
const REALMS: &[&str] = &["pam", "pve"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// Entry used most recently; the launcher preselects it (F16, Q16.4).
    /// Not an auto-connect: startup always shows the launcher.
    pub default: String,
    /// Local `remote-viewer` binary (F7). App-level, not per host.
    #[serde(default = "default_viewer")]
    pub remote_viewer_path: String,
    /// Saved hosts. v1 writes one. Keep last — see the module note on ordering.
    pub hosts: Vec<HostConfig>,
}

/// One Proxmox connection.
///
/// For a cluster this is the entry-point node: any member serves the whole
/// `/cluster/resources` list and proxies consoles. v1 does not fail over to
/// other node addresses (FEATURES.md Q1.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostConfig {
    pub name: String,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    /// `pam` or `pve` (FEATURES.md Q1.3).
    pub realm: String,
    /// SHA-256 fingerprint of the node's TLS cert, pinned on first connect (F3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cert_fingerprint: Option<String>,
}

fn default_port() -> u16 {
    8006
}

/// Default path to `remote-viewer` (F7).
///
/// Linux has a stable absolute path. On Windows and macOS virt-viewer is
/// installed in version-stamped or package-manager-specific locations, so
/// fall back to the bare command name and let PATH resolve it; the user can
/// override it in `config.toml`.
fn default_viewer() -> String {
    if cfg!(target_os = "linux") {
        "/usr/bin/remote-viewer".into()
    } else if cfg!(target_os = "windows") {
        "remote-viewer.exe".into()
    } else {
        "remote-viewer".into()
    }
}

impl Config {
    /// A config holding exactly the one host that first-run setup collected.
    pub fn single(host: HostConfig) -> Self {
        Self {
            default: host.name.clone(),
            remote_viewer_path: default_viewer(),
            hosts: vec![host],
        }
    }

    /// The host entry named by `default`.
    pub fn active(&self) -> Result<&HostConfig> {
        self.hosts
            .iter()
            .find(|h| h.name == self.default)
            .ok_or_else(|| Error::Config(format!("no host entry named {:?}", self.default)))
    }

    /// Look up one entry by its id.
    pub fn host(&self, name: &str) -> Result<&HostConfig> {
        self.hosts
            .iter()
            .find(|h| h.name == name)
            .ok_or_else(|| Error::Config(format!("no host entry named {name:?}")))
    }

    /// Replace the entry with the same name, or add it, and make it current.
    /// Used when setup is re-run so app-level settings survive.
    pub fn upsert_host(&mut self, host: HostConfig) {
        self.default = host.name.clone();
        match self.hosts.iter_mut().find(|h| h.name == host.name) {
            Some(existing) => *existing = host,
            None => self.hosts.push(host),
        }
    }
}

impl HostConfig {
    /// Stable id for an entry: unique per user+realm+host, so the same
    /// account on two nodes (or two accounts on one node) never collide.
    /// Never shown in the UI, which displays the fields themselves.
    pub fn id(username: &str, realm: &str, host: &str) -> String {
        format!("{username}@{realm}@{host}")
    }

    /// `user@realm`, the form the Proxmox API expects.
    pub fn userid(&self) -> String {
        format!("{}@{}", self.username, self.realm)
    }

    pub fn base_url(&self) -> String {
        format!("https://{}:{}", self.host, self.port)
    }

    /// Check what the setup form sent before we try to use it.
    pub fn validate(&self) -> Result<()> {
        if self.host.is_empty() {
            return Err(Error::Config("host is empty".into()));
        }
        if self.username.is_empty() {
            return Err(Error::Config("username is empty".into()));
        }
        if self.port == 0 {
            return Err(Error::Config("port must be 1-65535".into()));
        }
        if !REALMS.contains(&self.realm.as_str()) {
            return Err(Error::Config(format!(
                "realm must be one of {REALMS:?}, got {:?}",
                self.realm
            )));
        }
        Ok(())
    }
}

fn path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| Error::Config("no config directory".into()))?
        .join("pve-desk");
    Ok(dir.join("config.toml"))
}

pub fn load() -> Result<Option<Config>> {
    let p = path()?;
    if !p.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(p)?;
    parse(&text).map(Some)
}

pub fn save(cfg: &Config) -> Result<()> {
    let p = path()?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(p, render(cfg)?)?;
    Ok(())
}

/// Pure half of `load`, so it can be tested without touching the filesystem.
fn parse(text: &str) -> Result<Config> {
    toml::from_str(text).map_err(|e| Error::Config(e.to_string()))
}

/// Pure half of `save`.
fn render(cfg: &Config) -> Result<String> {
    toml::to_string_pretty(cfg).map_err(|e| Error::Config(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deliberately the legacy name: entries written before `HostConfig::id`
    /// existed must still load and resolve.
    fn host() -> HostConfig {
        HostConfig {
            name: "main".into(),
            host: "192.168.1.10".into(),
            port: 8006,
            username: "root".into(),
            realm: "pam".into(),
            cert_fingerprint: None,
        }
    }

    /// Also guards the TOML ordering rule: scalars must come before `[[hosts]]`,
    /// so a rendered config has to parse back.
    #[test]
    fn round_trip_without_fingerprint() {
        let cfg = Config::single(host());
        let text = render(&cfg).unwrap();
        assert_eq!(parse(&text).unwrap(), cfg);
    }

    #[test]
    fn round_trip_with_fingerprint() {
        let mut h = host();
        h.cert_fingerprint = Some("ab:cd:ef".into());
        let cfg = Config::single(h);
        let text = render(&cfg).unwrap();
        assert_eq!(parse(&text).unwrap(), cfg);
    }

    #[test]
    fn rendered_config_has_hosts_last() {
        let text = render(&Config::single(host())).unwrap();
        let array = text.find("[[hosts]]").expect("array of tables");
        assert!(text.find("default").unwrap() < array);
        assert!(text.find("remote_viewer_path").unwrap() < array);
    }

    #[test]
    fn optional_keys_fall_back_to_defaults() {
        let cfg = parse(
            r#"
            default = "main"

            [[hosts]]
            name = "main"
            host = "pve.lan"
            username = "adam"
            realm = "pve"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.remote_viewer_path, default_viewer());
        let h = cfg.active().unwrap();
        assert_eq!(h.port, 8006);
        assert_eq!(h.cert_fingerprint, None);
    }

    #[test]
    fn fingerprint_key_is_omitted_when_unset() {
        let text = render(&Config::single(host())).unwrap();
        assert!(!text.contains("cert_fingerprint"), "{text}");
    }

    #[test]
    fn active_resolves_the_named_entry() {
        let mut other = host();
        other.name = "work".into();
        other.host = "10.0.0.5".into();
        let cfg = Config {
            default: "work".into(),
            remote_viewer_path: default_viewer(),
            hosts: vec![host(), other],
        };
        assert_eq!(cfg.active().unwrap().host, "10.0.0.5");
    }

    #[test]
    fn active_errors_when_default_names_nothing() {
        let cfg = Config {
            default: "missing".into(),
            remote_viewer_path: default_viewer(),
            hosts: vec![host()],
        };
        assert!(cfg.active().is_err());
    }

    #[test]
    fn upsert_replaces_the_entry_and_keeps_app_settings() {
        let mut cfg = Config::single(host());
        cfg.remote_viewer_path = "/opt/bin/remote-viewer".into();
        let mut changed = host();
        changed.host = "192.168.1.99".into();
        cfg.upsert_host(changed);
        assert_eq!(cfg.hosts.len(), 1);
        assert_eq!(cfg.active().unwrap().host, "192.168.1.99");
        assert_eq!(cfg.remote_viewer_path, "/opt/bin/remote-viewer");
    }

    /// Regression: replacing an entry must not point `default` at a different
    /// host just because the replaced one isn't last in the array.
    #[test]
    fn upsert_sets_default_to_the_upserted_entry() {
        let mut work = host();
        work.name = "work".into();
        work.host = "10.0.0.5".into();
        let mut cfg = Config {
            default: "work".into(),
            remote_viewer_path: default_viewer(),
            hosts: vec![host(), work],
        };
        let mut changed = host();
        changed.host = "192.168.1.99".into();
        cfg.upsert_host(changed);
        assert_eq!(cfg.hosts.len(), 2);
        assert_eq!(cfg.default, "main");
        assert_eq!(cfg.active().unwrap().host, "192.168.1.99");
    }

    #[test]
    fn upsert_adds_a_new_entry() {
        let mut cfg = Config::single(host());
        let mut work = host();
        work.name = "work".into();
        work.host = "10.0.0.5".into();
        cfg.upsert_host(work);
        assert_eq!(cfg.hosts.len(), 2);
        assert_eq!(cfg.active().unwrap().host, "10.0.0.5");
    }

    #[test]
    fn userid_and_base_url_formatting() {
        let h = host();
        assert_eq!(h.userid(), "root@pam");
        assert_eq!(h.base_url(), "https://192.168.1.10:8006");
    }

    #[test]
    fn validate_accepts_both_realms() {
        for realm in ["pam", "pve"] {
            let mut h = host();
            h.realm = realm.into();
            assert!(h.validate().is_ok(), "{realm}");
        }
    }

    #[test]
    fn validate_rejects_bad_input() {
        let mut no_host = host();
        no_host.host = String::new();
        let mut no_user = host();
        no_user.username = String::new();
        let mut no_port = host();
        no_port.port = 0;
        let mut bad_realm = host();
        bad_realm.realm = "ldap".into();

        for (label, h) in [
            ("empty host", no_host),
            ("empty username", no_user),
            ("zero port", no_port),
            ("unknown realm", bad_realm),
        ] {
            assert!(h.validate().is_err(), "{label}");
        }
    }
}
