use std::process::Command;

use anyhow::Result;
use tracing::{info, warn};

const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

/// Attempts to extract the Claude Code OAuth token from the macOS Keychain.
///
/// Uses the `security` CLI to search by service name only, since the account
/// name used by Claude Code's keyring storage is not fixed.
///
/// Returns `Ok(Some(token))` if found, `Ok(None)` if not found (graceful fallback).
pub fn get_oauth_token() -> Result<Option<String>> {
    let output = Command::new("security")
        .args(["find-generic-password", "-s", KEYCHAIN_SERVICE, "-w"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let token = String::from_utf8(out.stdout)
                .map_err(|e| anyhow::anyhow!("keychain token is not valid UTF-8: {e}"))?;
            let token = token.trim_end().to_string();
            if token.is_empty() {
                warn!("keychain entry found but token is empty");
                return Ok(None);
            }
            info!("OAuth token extracted from macOS Keychain");
            Ok(Some(token))
        }
        Ok(_) => {
            warn!("keychain lookup found no entry for service {KEYCHAIN_SERVICE:?}");
            Ok(None)
        }
        Err(e) => {
            warn!("failed to run security CLI: {e}");
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keychain_service_is_correct() {
        assert_eq!(KEYCHAIN_SERVICE, "Claude Code-credentials");
    }

    #[test]
    fn get_oauth_token_does_not_panic() {
        // This test runs on macOS CI and should not panic regardless of whether
        // the keychain entry exists. It should return Ok (either Some or None).
        let result = get_oauth_token();
        assert!(result.is_ok());
    }
}
