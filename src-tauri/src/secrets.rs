//! Password storage in the system keyring (Secret Service on Linux:
//! GNOME Keyring, KWallet, KeePassXC). See FEATURES.md F2 / Q2.1.

use crate::error::Result;

const SERVICE: &str = "pve-desk";

/// One keyring entry per `user@realm@host`, so multiple hosts can coexist
/// later if Q1.1 says so.
fn entry(userid: &str, host: &str) -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(SERVICE, &format!("{userid}@{host}"))?)
}

pub fn set_password(userid: &str, host: &str, password: &str) -> Result<()> {
    entry(userid, host)?.set_password(password)?;
    Ok(())
}

pub fn get_password(userid: &str, host: &str) -> Result<String> {
    Ok(entry(userid, host)?.get_password()?)
}

#[allow(dead_code)] // used once "forget login" is added
pub fn delete_password(userid: &str, host: &str) -> Result<()> {
    entry(userid, host)?.delete_credential()?;
    Ok(())
}
