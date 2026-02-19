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

/// Derives the default network name from the current working directory.
/// Returns `bubble-boy-<dir-name>` matching the container naming convention.
pub fn default_network_name() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .map(|name| format!("bubble-boy-{name}"))
        .unwrap_or_else(|| "bubble-boy-project".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_network_name_has_prefix() {
        let name = default_network_name();
        assert!(name.starts_with("bubble-boy-"));
    }

    #[test]
    fn default_network_name_not_empty_suffix() {
        let name = default_network_name();
        let suffix = name.strip_prefix("bubble-boy-").unwrap();
        assert!(!suffix.is_empty());
    }

    #[test]
    fn default_network_name_matches_container_convention() {
        // Network name should follow the same naming convention as container names
        let network_name = default_network_name();
        let container_name = crate::docker::containers::default_container_name();
        assert_eq!(network_name, container_name);
    }
}
