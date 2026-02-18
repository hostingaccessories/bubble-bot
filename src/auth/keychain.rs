use anyhow::Result;
use security_framework::passwords::{PasswordOptions, generic_password};
use tracing::{info, warn};

const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
const KEYCHAIN_ACCOUNT: &str = "oauth_token";

/// Attempts to extract the Claude Code OAuth token from the macOS Keychain.
/// Returns `Ok(Some(token))` if found, `Ok(None)` if not found (graceful fallback).
pub fn get_oauth_token() -> Result<Option<String>> {
    let opts = PasswordOptions::new_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT);

    match generic_password(opts) {
        Ok(bytes) => {
            let token = String::from_utf8(bytes)
                .map_err(|e| anyhow::anyhow!("keychain token is not valid UTF-8: {e}"))?;
            if token.is_empty() {
                warn!("keychain entry found but token is empty");
                return Ok(None);
            }
            info!("OAuth token extracted from macOS Keychain");
            Ok(Some(token))
        }
        Err(e) => {
            warn!("keychain lookup failed (this is normal if not configured): {e}");
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keychain_constants_are_correct() {
        assert_eq!(KEYCHAIN_SERVICE, "Claude Code-credentials");
        assert_eq!(KEYCHAIN_ACCOUNT, "oauth_token");
    }

    #[test]
    fn get_oauth_token_does_not_panic() {
        // This test runs on macOS CI and should not panic regardless of whether
        // the keychain entry exists. It should return Ok (either Some or None).
        let result = get_oauth_token();
        assert!(result.is_ok());
    }
}
