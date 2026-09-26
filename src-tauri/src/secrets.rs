//! Secrets in the macOS Keychain (never in SQLite or plain files).

use anyhow::Result;

const SERVICE: &str = "com.travelsnapmap.app";
const ACCOUNT: &str = "deepseek-api-key";

fn entry() -> Option<keyring::Entry> {
    keyring::Entry::new(SERVICE, ACCOUNT).ok()
}

pub fn deepseek_key() -> Option<String> {
    entry()?.get_password().ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
}

/// Stores the key, or removes it when `None`.
pub fn set_deepseek_key(key: Option<&str>) -> Result<()> {
    let entry = entry().ok_or_else(|| anyhow::anyhow!("Keychain unavailable"))?;
    match key.map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => entry.set_password(k)?,
        None => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(e.into()),
        },
    }
    Ok(())
}
