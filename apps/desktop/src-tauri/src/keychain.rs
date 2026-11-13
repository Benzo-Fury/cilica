//! The OS keychain, under the app's service name. Keys saved while the app was called Margin
//! (service `app.margin.desktop`) move to the new service the first time they are read.

use keyring::{Entry, Error, Result};

/// Keychain service every key is saved under.
const SERVICE: &str = "app.cilica.desktop";
/// Service used before the rename. Only read (once per key) and deleted, never written.
const LEGACY_SERVICE: &str = "app.margin.desktop";

/// The key saved for `account`, or `None` when there is none.
pub fn read(account: &str) -> Result<Option<String>> {
    match Entry::new(SERVICE, account)?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(Error::NoEntry) => move_legacy(account),
        Err(e) => Err(e),
    }
}

/// Saves `key` for `account`.
pub fn write(account: &str, key: &str) -> Result<()> {
    Entry::new(SERVICE, account)?.set_password(key)
}

/// Removes the key for `account`, including any copy left under the old service name.
pub fn delete(account: &str) -> Result<()> {
    for service in [SERVICE, LEGACY_SERVICE] {
        match Entry::new(service, account)?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Copies a key saved under the old service name to the new one, then removes the old copy.
fn move_legacy(account: &str) -> Result<Option<String>> {
    let old = Entry::new(LEGACY_SERVICE, account)?;
    match old.get_password() {
        Ok(key) => {
            write(account, &key)?;
            let _ = old.delete_credential();
            Ok(Some(key))
        }
        Err(Error::NoEntry) => Ok(None),
        Err(e) => Err(e),
    }
}
