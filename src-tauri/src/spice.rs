//! F7: write a `.vv` file from the spiceproxy response and launch
//! remote-viewer with it.
//!
//! The file holds a one-time console ticket, so `CLAUDE.md` requires it be
//! private and short-lived. It is created mode 0600 — at creation, not
//! afterwards — and removed a few seconds after the viewer starts. See
//! `DECISIONS.md`, "SPICE `.vv` handling", for what was rejected.

use crate::error::Result;
use crate::platform::{Os, Platform};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// How long the file is left on disk for `remote-viewer` to read it.
///
/// `remote-viewer` reads the `.vv` asynchronously after being spawned, so
/// there is no moment we can observe that is provably safe to delete. This is
/// a guess: the viewer is not installed on the development machine, so the
/// real time has never been measured. Too short surfaces as a loud
/// missing-file error from the viewer rather than silent breakage.
const READ_WINDOW: Duration = Duration::from_secs(3);

/// Format the spiceproxy data as a virt-viewer `.vv` file.
fn to_vv(data: &serde_json::Map<String, serde_json::Value>) -> String {
    let mut out = String::from("[virt-viewer]\n");
    for (k, v) in data {
        let val = match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        // Proxmox returns the CA cert with literal newlines; .vv wants `\n`.
        out.push_str(&format!("{k}={}\n", val.replace('\n', "\\n")));
    }
    out
}

/// Reduce server-supplied text to something safe in a filename.
///
/// The node name comes from the Proxmox API, i.e. from outside this app. A
/// `/` or a `..` in one must not be able to steer where the ticket is
/// written, so anything that isn't a plain ASCII word character becomes `-`.
fn slug(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .take(64)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    // An all-punctuation or empty name must still produce a usable filename.
    if cleaned.trim_matches('-').is_empty() {
        "node".to_string()
    } else {
        cleaned
    }
}

/// A distinct file per launch. It used to be one per *process*, which meant a
/// second console overwrote the first one's file while the viewer might still
/// have been reading it. The timestamp keeps two launches of the *same*
/// console from colliding too, so `create_new` never has to fail.
fn vv_name(node: &str, vmid: u32, unique: u128) -> String {
    format!("console-{}-{vmid}-{unique}.vv", slug(node))
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

fn spice_dir() -> PathBuf {
    std::env::temp_dir().join("pve-desk")
}

/// A file this old can only be a leftover: nothing waits anywhere near this
/// long for the viewer to read one.
const STALE: Duration = Duration::from_secs(60);

/// Remove `.vv` files left behind by an earlier run — a backstop for the app
/// being killed between writing a file and deleting it.
///
/// Only files older than [`STALE`] are touched. Deleting every `.vv` here
/// would re-create the very bug this change fixes: a console opened seconds
/// after another would pull the first one's file out from under its viewer.
/// A file whose age can't be determined is left alone.
fn sweep(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|e| e == "vv") {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > STALE);
        if stale {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Write the `.vv` to a private file and launch remote-viewer, then delete it.
pub async fn launch(
    viewer_path: &str,
    node: &str,
    vmid: u32,
    data: &serde_json::Map<String, serde_json::Value>,
) -> Result<()> {
    let dir = spice_dir();
    std::fs::create_dir_all(&dir)?;
    Os::restrict_dir(&dir)?;
    sweep(&dir);

    let file = dir.join(vv_name(node, vmid, now_nanos()));
    let mut f = Os::create_private(&file)?;
    f.write_all(to_vv(data).as_bytes())?;
    drop(f);

    match Command::new(viewer_path).arg(&file).spawn() {
        Ok(_child) => {
            // The child is deliberately not awaited: the viewer owns its
            // window for the length of the console session.
            tokio::time::sleep(READ_WINDOW).await;
            let _ = std::fs::remove_file(&file);
            Ok(())
        }
        Err(e) => {
            // Nothing is going to read it, so don't leave a live ticket on
            // disk while the error travels back to the UI.
            let _ = std::fs::remove_file(&file);
            Err(e.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{slug, to_vv, vv_name};
    use serde_json::json;

    fn sample() -> serde_json::Map<String, serde_json::Value> {
        // Shaped as the spiceproxy response is documented: a one-time
        // password, host/port, and the cluster CA with literal newlines.
        json!({
            "password": "abc123",
            "proxy": "https://192.168.1.2",
            "tls-port": 61000,
            "ca": "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n",
        })
        .as_object()
        .unwrap()
        .clone()
    }

    #[test]
    fn writes_a_virt_viewer_section() {
        let out = to_vv(&sample());
        assert!(out.starts_with("[virt-viewer]\n"));
        assert!(out.contains("password=abc123\n"));
        assert!(out.contains("proxy=https://192.168.1.2\n"));
    }

    /// Numbers must not come through quoted — `"61000"` is not a port.
    #[test]
    fn numbers_are_unquoted() {
        let out = to_vv(&sample());
        assert!(out.contains("tls-port=61000\n"), "{out}");
        assert!(!out.contains("\"61000\""), "{out}");
    }

    /// A `.vv` is line-oriented, so the CA's real newlines have to be escaped
    /// or every line after it is read as a separate key.
    #[test]
    fn ca_newlines_are_escaped() {
        let out = to_vv(&sample());
        let ca = out
            .lines()
            .find(|l| l.starts_with("ca="))
            .expect("no ca line");
        assert!(ca.contains("\\n"), "{ca}");
        assert!(ca.ends_with("-----END CERTIFICATE-----\\n"), "{ca}");
        // One line per key: the cert must not have split the file up.
        assert_eq!(out.lines().count(), 5);
    }

    /// Node names come from the server, so they must not be able to steer
    /// where a one-time ticket gets written.
    #[test]
    fn slug_strips_path_traversal() {
        assert_eq!(slug("pve"), "pve");
        assert_eq!(slug("pve-node_2"), "pve-node_2");
        // Six separator characters before `etc`: `..` `/` `..` `/`.
        assert_eq!(slug("../../etc/shadow"), "------etc-shadow");
        assert_eq!(slug("a/b"), "a-b");
        assert_eq!(slug(".."), "node");
        assert_eq!(slug(""), "node");
        assert_eq!(slug("/"), "node");
        assert_eq!(slug(&"x".repeat(200)).len(), 64);
    }

    #[test]
    fn filename_is_per_console_not_per_process() {
        let a = vv_name("pve", 100, 1);
        let b = vv_name("pve", 101, 1);
        let c = vv_name("pve", 100, 2);
        assert_ne!(a, b, "two VMs must not share a file");
        assert_ne!(a, c, "two launches must not share a file");
        assert!(a.ends_with(".vv"));
        assert!(!vv_name("../etc", 1, 1).contains('/'));
    }
}
