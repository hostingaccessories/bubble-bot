use std::collections::HashMap;
use std::process::Command;

use anyhow::{Context, Result};
use bollard::Docker;
use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, RemoveContainerOptions,
    StopContainerOptions,
};
use bollard::models::HostConfig;
use tracing::{info, warn};

/// Manages the lifecycle of the dev container: create, start, exec, stop, remove.
pub struct ContainerManager {
    docker: Docker,
}

/// Options for creating a dev container.
pub struct ContainerOpts {
    pub image_tag: String,
    pub container_name: String,
    pub shell: String,
    pub project_dir: String,
    pub env_vars: Vec<String>,
}

impl ContainerManager {
    pub fn new(docker: Docker) -> Self {
        Self { docker }
    }

    /// Detects and removes an existing container with the given name.
    pub async fn cleanup_existing(&self, name: &str) -> Result<()> {
        let filters: HashMap<String, Vec<String>> =
            [("name".to_string(), vec![name.to_string()])]
                .into_iter()
                .collect();

        let containers = self
            .docker
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters,
                ..Default::default()
            }))
            .await
            .context("failed to list containers")?;

        // Filter for exact name match (Docker returns partial matches)
        let exact_name = format!("/{name}");
        for container in &containers {
            let names = container.names.as_deref().unwrap_or_default();
            if names.iter().any(|n| n == &exact_name) {
                let id = container.id.as_deref().unwrap_or("unknown");
                warn!(name, id, "removing existing container");

                // Stop if running
                let _ = self
                    .docker
                    .stop_container(id, Some(StopContainerOptions { t: 5 }))
                    .await;

                self.docker
                    .remove_container(
                        id,
                        Some(RemoveContainerOptions {
                            force: true,
                            ..Default::default()
                        }),
                    )
                    .await
                    .context("failed to remove existing container")?;
            }
        }

        Ok(())
    }

    /// Creates and starts a container, returning the container ID.
    pub async fn create_and_start(&self, opts: &ContainerOpts) -> Result<String> {
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };
        let user = format!("{uid}:{gid}");

        let bind = format!("{}:/workspace", opts.project_dir);

        let host_config = HostConfig {
            binds: Some(vec![bind]),
            ..Default::default()
        };

        let env = if opts.env_vars.is_empty() {
            None
        } else {
            Some(opts.env_vars.clone())
        };

        let config = Config {
            image: Some(opts.image_tag.clone()),
            cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
            user: Some(user),
            working_dir: Some("/workspace".to_string()),
            host_config: Some(host_config),
            env,
            ..Default::default()
        };

        let create_opts = CreateContainerOptions {
            name: opts.container_name.clone(),
            ..Default::default()
        };

        let response = self
            .docker
            .create_container(Some(create_opts), config)
            .await
            .context("failed to create container")?;

        let container_id = response.id;
        info!(id = %container_id, name = %opts.container_name, "container created");

        self.docker
            .start_container::<String>(&container_id, None)
            .await
            .context("failed to start container")?;

        info!(id = %container_id, "container started");

        Ok(container_id)
    }

    /// Launches an interactive shell inside the container via `docker exec -it`.
    /// This is a blocking call that inherits stdio.
    pub fn exec_interactive_shell(&self, container_id: &str, shell: &str) -> Result<i32> {
        info!(container = %container_id, shell, "launching interactive shell");

        let status = Command::new("docker")
            .args(["exec", "-it", container_id, shell])
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .context("failed to exec into container")?;

        Ok(status.code().unwrap_or(1))
    }

    /// Stops and removes the container.
    pub async fn stop_and_remove(&self, container_id: &str) -> Result<()> {
        info!(id = %container_id, "stopping container");

        let _ = self
            .docker
            .stop_container(container_id, Some(StopContainerOptions { t: 5 }))
            .await;

        self.docker
            .remove_container(
                container_id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await
            .context("failed to remove container")?;

        info!(id = %container_id, "container removed");

        Ok(())
    }
}

/// Derives the default container name from the current working directory.
/// Returns `bubble-boy-<dir-name>` or `bubble-boy-project` as fallback.
pub fn default_container_name() -> String {
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
    fn default_container_name_has_prefix() {
        let name = default_container_name();
        assert!(name.starts_with("bubble-boy-"));
    }

    #[test]
    fn default_container_name_not_empty_suffix() {
        let name = default_container_name();
        let suffix = name.strip_prefix("bubble-boy-").unwrap();
        assert!(!suffix.is_empty());
    }
}
