use std::collections::HashMap;

use anyhow::{Context, Result};
use bollard::Docker;
use bollard::image::{ListImagesOptions, RemoveImageOptions};
use bollard::network::ListNetworksOptions;
use bollard::volume::ListVolumesOptions;
use tracing::info;

/// Handles cleanup of Bubble Bot Docker resources (images, networks, volumes).
pub struct Cleaner {
    docker: Docker,
}

impl Cleaner {
    pub fn new(docker: Docker) -> Self {
        Self { docker }
    }

    /// Removes all `bubble-bot:*` images, `bubble-bot-*` networks, and optionally
    /// `bubble-bot-*` named volumes. Prints what was removed.
    pub async fn clean(&self, remove_volumes: bool) -> Result<()> {
        let removed_images = self.remove_images().await?;
        let removed_networks = self.remove_networks().await?;
        let removed_volumes = if remove_volumes {
            self.remove_volumes().await?
        } else {
            Vec::new()
        };

        if removed_images.is_empty() && removed_networks.is_empty() && removed_volumes.is_empty() {
            println!("Nothing to clean.");
            return Ok(());
        }

        if !removed_images.is_empty() {
            println!("Removed images:");
            for tag in &removed_images {
                println!("  {tag}");
            }
        }

        if !removed_networks.is_empty() {
            println!("Removed networks:");
            for name in &removed_networks {
                println!("  {name}");
            }
        }

        if !removed_volumes.is_empty() {
            println!("Removed volumes:");
            for name in &removed_volumes {
                println!("  {name}");
            }
        }

        Ok(())
    }

    /// Lists and removes all `bubble-bot:*` images. Returns the tags that were removed.
    async fn remove_images(&self) -> Result<Vec<String>> {
        let filters: HashMap<String, Vec<String>> =
            [("reference".to_string(), vec!["bubble-bot".to_string()])]
                .into_iter()
                .collect();

        let images = self
            .docker
            .list_images(Some(ListImagesOptions {
                filters,
                ..Default::default()
            }))
            .await
            .context("failed to list images")?;

        let mut removed = Vec::new();

        for image in &images {
            // Use the first repo tag for display, or the image ID
            let display_name = image
                .repo_tags
                .first()
                .cloned()
                .unwrap_or_else(|| image.id.clone());

            let remove_id = image
                .repo_tags
                .first()
                .cloned()
                .unwrap_or_else(|| image.id.clone());

            match self
                .docker
                .remove_image(
                    &remove_id,
                    Some(RemoveImageOptions {
                        force: true,
                        ..Default::default()
                    }),
                    None,
                )
                .await
            {
                Ok(_) => {
                    info!(image = %display_name, "image removed");
                    removed.push(display_name);
                }
                Err(e) => {
                    info!(image = %display_name, error = %e, "failed to remove image");
                }
            }
        }

        Ok(removed)
    }

    /// Lists and removes all `bubble-bot-*` networks. Returns the names that were removed.
    async fn remove_networks(&self) -> Result<Vec<String>> {
        let filters: HashMap<String, Vec<String>> =
            [("name".to_string(), vec!["bubble-bot-".to_string()])]
                .into_iter()
                .collect();

        let networks = self
            .docker
            .list_networks(Some(ListNetworksOptions { filters }))
            .await
            .context("failed to list networks")?;

        let mut removed = Vec::new();

        for network in &networks {
            let name = match &network.name {
                Some(n) if n.starts_with("bubble-bot-") => n.clone(),
                _ => continue,
            };

            match self.docker.remove_network(&name).await {
                Ok(()) => {
                    info!(network = %name, "network removed");
                    removed.push(name);
                }
                Err(e) => {
                    info!(network = %name, error = %e, "failed to remove network");
                }
            }
        }

        Ok(removed)
    }

    /// Lists and removes all `bubble-bot-*` named volumes. Returns the names that were removed.
    async fn remove_volumes(&self) -> Result<Vec<String>> {
        let filters: HashMap<String, Vec<String>> =
            [("name".to_string(), vec!["bubble-bot-".to_string()])]
                .into_iter()
                .collect();

        let response = self
            .docker
            .list_volumes(Some(ListVolumesOptions { filters }))
            .await
            .context("failed to list volumes")?;

        let volumes = response.volumes.unwrap_or_default();
        let mut removed = Vec::new();

        for volume in &volumes {
            if !volume.name.starts_with("bubble-bot-") {
                continue;
            }

            match self.docker.remove_volume(&volume.name, None).await {
                Ok(()) => {
                    info!(volume = %volume.name, "volume removed");
                    removed.push(volume.name.clone());
                }
                Err(e) => {
                    info!(volume = %volume.name, error = %e, "failed to remove volume");
                }
            }
        }

        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleaner_can_be_constructed() {
        // Verify Cleaner struct is constructable (basic smoke test)
        // Actual Docker API calls require a running daemon so we test construction only
        let docker = Docker::connect_with_local_defaults().unwrap();
        let _cleaner = Cleaner::new(docker);
    }
}
