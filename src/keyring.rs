//! Secure API key storage using OS keyring.
//!
//! This module provides cross-platform credential storage:
//! - macOS: Keychain
//! - Windows: Credential Manager
//! - Linux: Secret Service (requires D-Bus and a keyring daemon)

use anyhow::{Context, Result};

const SERVICE_NAME: &str = "linear-cli";

/// Get an API key from the keyring for a profile.
/// Returns Ok(None) if no key is stored, Ok(Some(key)) if found.
pub fn get_key(profile: &str) -> Result<Option<String>> {
    let entry = keyring::Entry::new(SERVICE_NAME, profile)
        .context("Failed to create keyring entry")?;

    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(keyring::Error::NoStorageAccess(_)) => {
            eprintln!("Warning: Keyring not available, falling back to config file");
            Ok(None)
        }
        Err(e) => {
            eprintln!("Warning: Keyring error ({}), falling back to config file", e);
            Ok(None)
        }
    }
}

/// Store an API key in the keyring for a profile.
pub fn set_key(profile: &str, api_key: &str) -> Result<()> {
    let entry = keyring::Entry::new(SERVICE_NAME, profile)
        .context("Failed to create keyring entry")?;

    entry
        .set_password(api_key)
        .context("Failed to store API key in keyring")?;

    Ok(())
}

/// Delete an API key from the keyring for a profile.
/// Returns Ok(()) even if no key was stored.
pub fn delete_key(profile: &str) -> Result<()> {
    let entry = keyring::Entry::new(SERVICE_NAME, profile)
        .context("Failed to create keyring entry")?;

    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()), // Already gone, that's fine
        Err(e) => Err(e).context("Failed to delete API key from keyring"),
    }
}

/// Check if keyring is available on this system.
pub fn is_available() -> bool {
    // Try to create an entry and check if we can access it
    match keyring::Entry::new(SERVICE_NAME, "__test__") {
        Ok(entry) => {
            // Try a non-destructive operation
            match entry.get_password() {
                Err(keyring::Error::NoStorageAccess(_)) => false,
                _ => true, // NoEntry or Ok means storage is accessible
            }
        }
        Err(_) => false,
    }
}
