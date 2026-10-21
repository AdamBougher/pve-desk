//! F7: write a `.vv` file from the spiceproxy response and launch
//! remote-viewer with it.

use crate::error::Result;
use std::io::Write;
use std::process::Command;

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

/// Write the `.vv` to a private temp file and launch remote-viewer.
pub fn launch(viewer_path: &str, data: &serde_json::Map<String, serde_json::Value>) -> Result<()> {
    let dir = std::env::temp_dir().join("pve-desk");
    std::fs::create_dir_all(&dir)?;
    let file = dir.join(format!("console-{}.vv", std::process::id()));
    let mut f = std::fs::File::create(&file)?;
    f.write_all(to_vv(data).as_bytes())?;

    // TODO(F7): set 0600 permissions on the file (it contains a one-time
    // ticket) and delete it after remote-viewer has read it.
    Command::new(viewer_path).arg(&file).spawn()?;
    Ok(())
}
