//! Password storage in the system keyring (Secret Service on Linux:
//! GNOME Keyring, KWallet, KeePassXC). See DECISIONS.md Q2.1.

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

/// Delete an entry's password (F19).
///
/// A missing entry counts as success: the caller wants no password for this
/// connection left in the keyring, and there being none already satisfies
/// that. Every other keyring failure — a locked collection, no Secret
/// Service — is reported, so removal can stop with the config entry intact
/// instead of orphaning a secret the app can no longer reach.
pub fn delete_password(userid: &str, host: &str) -> Result<()> {
    match entry(userid, host)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
