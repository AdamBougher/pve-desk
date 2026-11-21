//! Non-secret settings, stored at `~/.config/pve-desk/config.toml`.
//! The password is NOT stored here — see `secrets.rs`.
//!
//! Schema (DECISIONS.md Q1.1): app-level settings at the top level, then an
//! `[[hosts]]` array. v1 writes exactly one connection, named `main`, and shows no
//! host picker; the array exists so adding a second host later needs no
//! format migration.
//!
//! Field order matters: TOML requires scalar keys before an array of tables,
//! so `hosts` must stay the last field of `Config`.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Realms offered by the setup form (DECISIONS.md Q1.3).
const REALMS: &[&str] = &["pam", "pve"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// The default connection: used most recently, so the launcher preselects
    /// it (F16, Q16.4).
    /// Not an auto-connect: startup always shows the launcher.
    pub default: String,
    /// Local `remote-viewer` binary (F7). App-level, not per host.
    #[serde(default = "default_viewer")]
    pub remote_viewer_path: String,
    /// Which column Home sorts by (F26, Q26.2). One of `SORT_COLUMNS`;
    /// anything else is read as the default, so a hand-edited file cannot
    /// leave the list in a state the UI has no ordering for.
    #[serde(default = "default_sort")]
    pub sort: String,
    /// Whether that sort runs descending (F26).
    #[serde(default)]
    pub sort_desc: bool,
    /// Saved hosts. v1 writes one. Keep last — see the module note on ordering.
    pub hosts: Vec<HostConfig>,
}

/// The columns Home can sort by (F26 Q26.1).
///
/// The four whose order is stable. CPU and RAM are deliberately absent: they
/// move on every 30s refresh, so sorting by them would reshuffle rows under
/// the cursor. This is also the allow-list `set_sort` checks, so the frontend
/// cannot write an unknown column into the config file.
pub const SORT_COLUMNS: &[&str] = &["vmid", "name", "node", "status"];

fn default_sort() -> String {
    "vmid".into()
}

/// The stored sort, with anything unrecognised falling back to the default.
pub fn sort_or_default(sort: &str) -> &str {
    if SORT_COLUMNS.contains(&sort) {
        sort
    } else {
        "vmid"
    }
}

/// One Proxmox connection.
///
/// For a cluster this is the entry-point node: any member serves the whole
/// `/cluster/resources` list and proxies consoles. v1 does not fail over to
/// other node addresses (DECISIONS.md Q1.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostConfig {
    pub name: String,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    /// `pam` or `pve` (DECISIONS.md Q1.3).
    pub realm: String,
    /// SHA-256 fingerprint of the node's TLS cert, pinned on first connect (F3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cert_fingerprint: Option<String>,
    /// Pinned guests by vmid (F11, Q11.1). Per connection, because a vmid is
    /// unique only inside one cluster. Home lists them first; the tray lists
    /// only them (Q10.1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pinned: Vec<u32>,
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
            sort: default_sort(),
            sort_desc: false,
            hosts: vec![host],
        }
    }

    /// Look up one connection by its id.
    pub fn host(&self, name: &str) -> Result<&HostConfig> {
        self.hosts
            .iter()
            .find(|h| h.name == name)
            .ok_or_else(|| Error::Config(format!("no connection named {name:?}")))
    }

    /// Replace the connection with the same name, or add it, and make it the
    /// default.
    /// Used when setup is re-run so app-level settings survive.
    ///
    /// An existing entry keeps its pins (F11): the same id is the same
    /// cluster, and entering the password again must not erase them.
    pub fn upsert_host(&mut self, mut host: HostConfig) {
        self.default = host.name.clone();
        match self.hosts.iter_mut().find(|h| h.name == host.name) {
            Some(existing) => {
                if host.pinned.is_empty() {
                    host.pinned = std::mem::take(&mut existing.pinned);
                }
                *existing = host;
            }
            None => self.hosts.push(host),
        }
    }

    /// Delete one entry, returning it so the caller can reach its keyring
    /// password (F19).
    ///
    /// `default` is repointed at a remaining connection, or cleared when none is
    /// left, so it never names something that is gone. An empty `default`
    /// simply means no row is preselected (Q16.4).
    pub fn remove_host(&mut self, name: &str) -> Result<HostConfig> {
        let at = self
            .hosts
            .iter()
            .position(|h| h.name == name)
            .ok_or_else(|| Error::Config(format!("no connection named {name:?}")))?;
        let removed = self.hosts.remove(at);
        if self.default == removed.name {
            self.default = self
                .hosts
                .first()
                .map(|h| h.name.clone())
                .unwrap_or_default();
        }
        Ok(removed)
    }
}

impl HostConfig {
    /// Pin or unpin one guest (F11). Returns whether anything changed, so the
    /// caller writes the file only when it must. The list stays sorted and
    /// holds each vmid once.
    pub fn set_pinned(&mut self, vmid: u32, pinned: bool) -> bool {
        match (self.pinned.binary_search(&vmid), pinned) {
            (Err(at), true) => {
                self.pinned.insert(at, vmid);
                true
            }
            (Ok(at), false) => {
                self.pinned.remove(at);
                true
            }
            _ => false,
        }
    }

    /// Stable id for a connection: unique per user+realm+host, so the same
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

    /// Deliberately the legacy name: connections written before `HostConfig::id`
    /// existed must still load and resolve.
    fn host() -> HostConfig {
        HostConfig {
            name: "main".into(),
            host: "192.168.1.10".into(),
            port: 8006,
            username: "root".into(),
            realm: "pam".into(),
            cert_fingerprint: None,
            pinned: Vec::new(),
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
        // F26's keys are scalars too, so they must also precede the array.
        assert!(text.find("sort").unwrap() < array);
        assert!(text.find("sort_desc").unwrap() < array);
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
        // F26 Q26.2: a config file written before the sort existed must load
        // unchanged. This block is exactly such a file.
        assert_eq!(cfg.sort, "vmid");
        assert!(!cfg.sort_desc);
        let h = cfg.host(&cfg.default).unwrap();
        assert_eq!(h.port, 8006);
        assert_eq!(h.cert_fingerprint, None);
    }

    #[test]
    fn sort_round_trips() {
        let mut cfg = Config::single(host());
        cfg.sort = "node".into();
        cfg.sort_desc = true;
        let back = parse(&render(&cfg).unwrap()).unwrap();
        assert_eq!(back.sort, "node");
        assert!(back.sort_desc);
    }

    /// A hand-edited file naming a column the UI has no ordering for must not
    /// leave the list in a state it cannot render. CPU is the realistic
    /// mistake: it looks sortable and is deliberately not (Q26.1).
    #[test]
    fn an_unknown_sort_column_falls_back() {
        assert_eq!(sort_or_default("name"), "name");
        assert_eq!(sort_or_default("vmid"), "vmid");
        assert_eq!(sort_or_default("cpu"), "vmid");
        assert_eq!(sort_or_default(""), "vmid");
        assert_eq!(sort_or_default("../etc/passwd"), "vmid");
        for c in SORT_COLUMNS {
            assert_eq!(sort_or_default(c), *c);
        }
    }

    /// F11: pins survive a restart, which is a write and a read of this file.
    #[test]
    fn pins_round_trip() {
        let mut h = host();
        h.pinned = vec![101, 205];
        let cfg = Config::single(h);
        let text = render(&cfg).unwrap();
        assert_eq!(parse(&text).unwrap(), cfg);
    }

    /// A file written before F11 has no `pinned` key and must load with no
    /// pins. An empty list is not written, so such a file stays unchanged.
    #[test]
    fn pins_default_to_none_and_are_omitted_when_empty() {
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
        assert!(cfg.hosts[0].pinned.is_empty());
        assert!(!render(&cfg).unwrap().contains("pinned"));
    }

    /// F11 scenario "Pins belong to their connection": the same vmid on a
    /// second connection is not pinned.
    #[test]
    fn pins_belong_to_their_connection() {
        let mut work = host();
        work.name = "work".into();
        work.host = "10.0.0.5".into();
        let mut cfg = Config::single(host());
        cfg.upsert_host(work);
        cfg.hosts[0].set_pinned(101, true);
        let back = parse(&render(&cfg).unwrap()).unwrap();
        assert_eq!(back.host("main").unwrap().pinned, vec![101]);
        assert!(back.host("work").unwrap().pinned.is_empty());
    }

    #[test]
    fn set_pinned_keeps_one_sorted_copy() {
        let mut h = host();
        assert!(h.set_pinned(205, true));
        assert!(h.set_pinned(101, true));
        assert!(!h.set_pinned(101, true), "pinning twice changes nothing");
        assert_eq!(h.pinned, vec![101, 205]);
        assert!(h.set_pinned(205, false));
        assert!(
            !h.set_pinned(999, false),
            "unpinning a stranger changes nothing"
        );
        assert_eq!(h.pinned, vec![101]);
    }

    /// Re-adding a connection (same id) must not erase its pins.
    #[test]
    fn upsert_keeps_the_pins() {
        let mut h = host();
        h.pinned = vec![101];
        let mut cfg = Config::single(h);
        let mut again = host();
        again.port = 8007;
        cfg.upsert_host(again);
        assert_eq!(cfg.host("main").unwrap().pinned, vec![101]);
        assert_eq!(cfg.host("main").unwrap().port, 8007);
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
            sort: default_sort(),
            sort_desc: false,
            hosts: vec![host(), other],
        };
        assert_eq!(cfg.host(&cfg.default).unwrap().host, "10.0.0.5");
    }

    #[test]
    fn active_errors_when_default_names_nothing() {
        let cfg = Config {
            default: "missing".into(),
            remote_viewer_path: default_viewer(),
            sort: default_sort(),
            sort_desc: false,
            hosts: vec![host()],
        };
        assert!(cfg.host(&cfg.default).is_err());
    }

    #[test]
    fn upsert_replaces_the_entry_and_keeps_app_settings() {
        let mut cfg = Config::single(host());
        cfg.remote_viewer_path = "/opt/bin/remote-viewer".into();
        let mut changed = host();
        changed.host = "192.168.1.99".into();
        cfg.upsert_host(changed);
        assert_eq!(cfg.hosts.len(), 1);
        assert_eq!(cfg.host(&cfg.default).unwrap().host, "192.168.1.99");
        assert_eq!(cfg.remote_viewer_path, "/opt/bin/remote-viewer");
    }

    /// Regression: replacing a connection must not point `default` at a different
    /// host just because the replaced one isn't last in the array.
    #[test]
    fn upsert_sets_default_to_the_upserted_entry() {
        let mut work = host();
        work.name = "work".into();
        work.host = "10.0.0.5".into();
        let mut cfg = Config {
            default: "work".into(),
            remote_viewer_path: default_viewer(),
            sort: default_sort(),
            sort_desc: false,
            hosts: vec![host(), work],
        };
        let mut changed = host();
        changed.host = "192.168.1.99".into();
        cfg.upsert_host(changed);
        assert_eq!(cfg.hosts.len(), 2);
        assert_eq!(cfg.default, "main");
        assert_eq!(cfg.host(&cfg.default).unwrap().host, "192.168.1.99");
    }

    #[test]
    fn upsert_adds_a_new_entry() {
        let mut cfg = Config::single(host());
        let mut work = host();
        work.name = "work".into();
        work.host = "10.0.0.5".into();
        cfg.upsert_host(work);
        assert_eq!(cfg.hosts.len(), 2);
        assert_eq!(cfg.host(&cfg.default).unwrap().host, "10.0.0.5");
    }

    /// F19: removal hands back the connection, because the caller needs its
    /// `userid`/`host` to delete the matching keyring password.
    #[test]
    fn remove_returns_the_entry_and_drops_it() {
        let mut cfg = Config::single(host());
        let removed = cfg.remove_host("main").unwrap();
        assert_eq!(removed.host, "192.168.1.10");
        assert!(cfg.hosts.is_empty());
    }

    #[test]
    fn remove_errors_on_an_unknown_entry() {
        let mut cfg = Config::single(host());
        assert!(cfg.remove_host("nope").is_err());
        assert_eq!(cfg.hosts.len(), 1, "a failed remove changes nothing");
    }

    /// `default` must never name a connection that is gone: `host(&cfg.default)`
    /// would then error on every launcher load.
    #[test]
    fn remove_repoints_default_at_a_survivor() {
        let mut work = host();
        work.name = "work".into();
        work.host = "10.0.0.5".into();
        let mut cfg = Config {
            default: "work".into(),
            remote_viewer_path: default_viewer(),
            sort: default_sort(),
            sort_desc: false,
            hosts: vec![host(), work],
        };
        cfg.remove_host("work").unwrap();
        assert_eq!(cfg.default, "main");
        assert!(cfg.host(&cfg.default).is_ok());
    }

    #[test]
    fn remove_leaves_default_empty_when_nothing_is_left() {
        let mut cfg = Config::single(host());
        cfg.remove_host("main").unwrap();
        assert_eq!(cfg.default, "");
    }

    /// Removing one of two connections must not disturb the other, nor the
    /// app-level settings that sit outside the array.
    #[test]
    fn remove_keeps_the_other_entry_and_app_settings() {
        let mut work = host();
        work.name = "work".into();
        work.host = "10.0.0.5".into();
        let mut cfg = Config {
            default: "main".into(),
            remote_viewer_path: "/opt/bin/remote-viewer".into(),
            sort: default_sort(),
            sort_desc: false,
            hosts: vec![host(), work],
        };
        cfg.remove_host("main").unwrap();
        assert_eq!(cfg.default, "work");
        assert_eq!(cfg.hosts.len(), 1);
        assert_eq!(cfg.host("work").unwrap().host, "10.0.0.5");
        assert_eq!(cfg.remote_viewer_path, "/opt/bin/remote-viewer");
    }

    /// An emptied config still has to render and parse: `default = ""` is a
    /// valid state the launcher reaches by removing the last connection.
    #[test]
    fn empty_config_round_trips() {
        let mut cfg = Config::single(host());
        cfg.remove_host("main").unwrap();
        let text = render(&cfg).unwrap();
        assert_eq!(parse(&text).unwrap(), cfg);
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
