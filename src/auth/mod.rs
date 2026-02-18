#[cfg(target_os = "macos")]
pub mod keychain;

use anyhow::Result;
use tracing::{info, warn};

const ENV_VAR_NAME: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// Resolves the Claude Code OAuth token using platform-specific strategies.
///
/// Resolution order:
/// 1. Check host environment variable `CLAUDE_CODE_OAUTH_TOKEN`
/// 2. On macOS, attempt to extract from the Keychain
///
/// Returns `Ok(None)` if no token is available (warning logged, not an error).
pub fn resolve_oauth_token() -> Result<Option<String>> {
    // Strategy 1: Check environment variable
    if let Ok(token) = std::env::var(ENV_VAR_NAME) {
        if !token.is_empty() {
            info!("OAuth token found in environment variable");
            return Ok(Some(token));
        }
    }

    // Strategy 2: macOS Keychain
    #[cfg(target_os = "macos")]
    {
        if let Some(token) = keychain::get_oauth_token()? {
            return Ok(Some(token));
        }
    }

    warn!("no OAuth token found — Claude Code authentication may fail inside the container");
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_var_name_is_correct() {
        assert_eq!(ENV_VAR_NAME, "CLAUDE_CODE_OAUTH_TOKEN");
    }

    #[test]
    fn resolve_returns_ok() {
        // Should never panic or return Err, regardless of environment state
        let result = resolve_oauth_token();
        assert!(result.is_ok());
    }
}
