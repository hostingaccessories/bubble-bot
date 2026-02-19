#[cfg(target_os = "macos")]
pub mod keychain;

use anyhow::Result;
use serde_json::{Map, Value};
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

/// Builds the `.claude.json` config to write into the container.
///
/// Reads `~/.claude.json` from the host to extract `oauthAccount`.
/// Always includes `hasCompletedOnboarding: true` and `theme: "dark-daltonized"`.
pub fn resolve_claude_config() -> Result<String> {
    let mut config = Map::new();
    config.insert("hasCompletedOnboarding".to_string(), Value::Bool(true));
    config.insert(
        "theme".to_string(),
        Value::String("dark-daltonized".to_string()),
    );

    if let Some(home) = dirs::home_dir() {
        let path = home.join(".claude.json");
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if let Ok(Value::Object(host_config)) = serde_json::from_str(&contents) {
                if let Some(oauth_account) = host_config.get("oauthAccount") {
                    config.insert("oauthAccount".to_string(), oauth_account.clone());
                    info!("oauthAccount extracted from host ~/.claude.json");
                }
            } else {
                warn!("~/.claude.json exists but is not valid JSON");
            }
        }
    }

    let json = serde_json::to_string(&Value::Object(config))?;
    Ok(json)
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
