use std::collections::HashMap;

use anyhow::{Context, Result};
use bollard::Docker;
use bollard::network::{CreateNetworkOptions, ListNetworksOptions};
use tracing::{info, warn};

/// Manages bridge networks for container communication.
pub struct NetworkManager {
    docker: Docker,
}

impl NetworkManager {
    pub fn new(docker: Docker) -> Self {
        Self { docker }
    }

    /// Creates a bridge network with the given name.
    /// If the network already exists, it is reused.
    /// Returns the network name.
    pub async fn ensure_network(&self, name: &str) -> Result<String> {
        if self.network_exists(name).await? {
            info!(network = %name, "network already exists — reusing");
            return Ok(name.to_string());
        }

        let options = CreateNetworkOptions {
            name: name.to_string(),
            driver: "bridge".to_string(),
            check_duplicate: true,
            ..Default::default()
        };

        self.docker
            .create_network(options)
            .await
            .context("failed to create network")?;

        info!(network = %name, "bridge network created");

        Ok(name.to_string())
    }

    /// Checks whether a network with the given name exists.
    pub async fn network_exists(&self, name: &str) -> Result<bool> {
        let filters: HashMap<String, Vec<String>> =
            [("name".to_string(), vec![name.to_string()])]
                .into_iter()
                .collect();

        let networks = self
            .docker
            .list_networks(Some(ListNetworksOptions { filters }))
            .await
            .context("failed to list networks")?;

        // Docker name filter returns partial matches — check for exact match
        Ok(networks
            .iter()
            .any(|n| n.name.as_deref() == Some(name)))
    }

    /// Detects and removes stale networks matching the `bubble-bot-<project>` prefix.
    /// Returns the number of networks removed.
    pub async fn cleanup_stale(&self, project_prefix: &str) -> Result<usize> {
        let filters: HashMap<String, Vec<String>> =
            [("name".to_string(), vec![project_prefix.to_string()])]
                .into_iter()
                .collect();

        let networks = self
            .docker
            .list_networks(Some(ListNetworksOptions { filters }))
            .await
            .context("failed to list networks for stale detection")?;

        let mut removed = 0;

        for network in &networks {
            let name = network.name.as_deref().unwrap_or("");
            if matches_stale_prefix(name, project_prefix) {
                warn!(network = %name, "removing stale network from previous session");
                match self.docker.remove_network(name).await {
                    Ok(()) => {
                        removed += 1;
                    }
                    Err(e) => {
                        warn!(network = %name, error = %e, "failed to remove stale network (may have active endpoints)");
                    }
                }
            }
        }

        Ok(removed)
    }

    /// Removes the network. Logs a warning if it doesn't exist or removal fails.
    pub async fn remove_network(&self, name: &str) -> Result<()> {
        match self.docker.remove_network(name).await {
            Ok(()) => {
                info!(network = %name, "network removed");
            }
            Err(e) => {
                warn!(network = %name, error = %e, "failed to remove network (may already be removed)");
            }
        }
        Ok(())
    }
}

/// Checks whether a network name matches the stale detection prefix.
/// Returns true if the name is exactly the prefix or starts with `prefix-`.
pub fn matches_stale_prefix(network_name: &str, prefix: &str) -> bool {
    network_name == prefix || network_name.starts_with(&format!("{prefix}-"))
}

/// Derives the default network name from the current working directory.
/// Returns `bubble-bot-<dir-name>` matching the container naming convention.
pub fn default_network_name() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .map(|name| format!("bubble-bot-{name}"))
        .unwrap_or_else(|| "bubble-bot-project".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_network_name_has_prefix() {
        let name = default_network_name();
        assert!(name.starts_with("bubble-bot-"));
    }

    #[test]
    fn default_network_name_not_empty_suffix() {
        let name = default_network_name();
        let suffix = name.strip_prefix("bubble-bot-").unwrap();
        assert!(!suffix.is_empty());
    }

    #[test]
    fn default_network_name_matches_container_convention() {
        // Network name should follow the same naming convention as container names
        let network_name = default_network_name();
        let container_name = crate::docker::containers::default_container_name();
        assert_eq!(network_name, container_name);
    }

    #[test]
    fn stale_prefix_matches_exact_network_name() {
        assert!(matches_stale_prefix("bubble-bot-myproject", "bubble-bot-myproject"));
    }

    #[test]
    fn stale_prefix_matches_network_with_suffix() {
        // Unlikely for networks but should handle `prefix-*` pattern
        assert!(matches_stale_prefix(
            "bubble-bot-myproject-extra",
            "bubble-bot-myproject"
        ));
    }

    #[test]
    fn stale_prefix_rejects_different_project_network() {
        assert!(!matches_stale_prefix(
            "bubble-bot-otherproject",
            "bubble-bot-myproject"
        ));
    }

    #[test]
    fn stale_prefix_rejects_non_bubble_boy_network() {
        assert!(!matches_stale_prefix("my-network", "bubble-bot-myproject"));
    }
}
