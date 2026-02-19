use std::collections::HashMap;
use std::process::Command;

use anyhow::{Context, Result};
use bollard::Docker;
use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, NetworkingConfig,
    RemoveContainerOptions, StopContainerOptions,
};
use bollard::models::{EndpointSettings, HostConfig, Mount, MountTypeEnum};
use tracing::{info, warn};

use crate::services::Service;

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
    pub network: Option<String>,
    /// Additional read-only bind mounts (e.g., dotfiles) in `host:container:ro` format.
    pub extra_binds: Vec<String>,
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

    /// Detects and removes all stale containers matching the `bubble-boy-<project>` prefix.
    /// This catches dev containers and service containers from crashed sessions.
    /// Returns the number of containers removed.
    pub async fn cleanup_stale(&self, project_prefix: &str) -> Result<usize> {
        let filters: HashMap<String, Vec<String>> =
            [("name".to_string(), vec![project_prefix.to_string()])]
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
            .context("failed to list containers for stale detection")?;

        let mut removed = 0;

        for container in &containers {
            let names = container.names.as_deref().unwrap_or_default();
            let is_match = names
                .iter()
                .any(|n| matches_stale_prefix(n, project_prefix));

            if is_match {
                let id = container.id.as_deref().unwrap_or("unknown");
                let name = names.first().map(|s| s.as_str()).unwrap_or("unknown");
                warn!(name, id, "removing stale container from previous session");

                // Stop if running
                let _ = self
                    .docker
                    .stop_container(id, Some(StopContainerOptions { t: 5 }))
                    .await;

                match self
                    .docker
                    .remove_container(
                        id,
                        Some(RemoveContainerOptions {
                            force: true,
                            ..Default::default()
                        }),
                    )
                    .await
                {
                    Ok(()) => {
                        removed += 1;
                    }
                    Err(e) => {
                        warn!(name, id, error = %e, "failed to remove stale container");
                    }
                }
            }
        }

        Ok(removed)
    }

    /// Creates and starts a container, returning the container ID.
    pub async fn create_and_start(&self, opts: &ContainerOpts) -> Result<String> {
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };
        let user = format!("{uid}:{gid}");

        let bind = format!("{}:/workspace", opts.project_dir);
        let mut binds = vec![bind];
        binds.extend(opts.extra_binds.clone());

        let host_config = HostConfig {
            binds: Some(binds),
            network_mode: opts.network.clone(),
            ..Default::default()
        };

        let env = if opts.env_vars.is_empty() {
            None
        } else {
            Some(opts.env_vars.clone())
        };

        // Attach to network with container name as alias for hostname-based discovery
        let networking_config = opts.network.as_ref().map(|net| {
            let endpoint = EndpointSettings {
                aliases: Some(vec![opts.container_name.clone()]),
                ..Default::default()
            };
            let mut endpoints_config = HashMap::new();
            endpoints_config.insert(net.clone(), endpoint);
            NetworkingConfig { endpoints_config }
        });

        let config = Config {
            image: Some(opts.image_tag.clone()),
            cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
            user: Some(user),
            working_dir: Some("/workspace".to_string()),
            host_config: Some(host_config),
            env,
            networking_config,
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

    /// Launches an interactive command inside the container via `docker exec -it`.
    /// This is a blocking call that inherits stdio.
    pub fn exec_interactive_command(&self, container_id: &str, cmd: &[&str]) -> Result<i32> {
        info!(container = %container_id, ?cmd, "launching interactive command");

        let mut args = vec!["exec", "-it", container_id];
        args.extend(cmd);

        let status = Command::new("docker")
            .args(&args)
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .context("failed to exec command in container")?;

        Ok(status.code().unwrap_or(1))
    }

    /// Writes the OAuth credentials file inside the container.
    /// Pipes the content via stdin to avoid exposing the token in process arguments.
    pub fn write_credentials(&self, container_id: &str, credentials: &str) -> Result<()> {
        use std::io::Write;

        let mut child = Command::new("docker")
            .args([
                "exec", "-i", container_id, "sh", "-c",
                "mkdir -p \"${HOME}/.claude\" && cat > \"${HOME}/.claude/.credentials.json\" && chmod 600 \"${HOME}/.claude/.credentials.json\"",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("failed to spawn docker exec for credentials")?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(credentials.as_bytes())?;
        }

        let status = child.wait().context("failed to wait for credentials write")?;
        if !status.success() {
            anyhow::bail!("failed to write credentials to container");
        }

        info!(container = %container_id, "OAuth credentials written");
        Ok(())
    }

    /// Runs a command inside the container via `docker exec` (non-interactive).
    /// Inherits stdout and stderr but does not allocate a TTY.
    pub fn exec_command(&self, container_id: &str, cmd: &[&str]) -> Result<i32> {
        info!(container = %container_id, ?cmd, "running command");

        let mut args = vec!["exec", container_id];
        args.extend(cmd);

        let status = Command::new("docker")
            .args(&args)
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .context("failed to exec command in container")?;

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

    /// Starts a service container (e.g., MySQL, Redis, PostgreSQL) on the given network.
    /// Returns the container ID.
    pub async fn start_service(
        &self,
        service: &dyn Service,
        network: &str,
        project_name: &str,
    ) -> Result<String> {
        let container_name = service.container_name(project_name);

        // Clean up any existing service container
        self.cleanup_existing(&container_name).await?;

        let env = Some(service.container_env());

        // Configure volume mount if the service needs persistent storage
        let mounts = service.volume().map(|vol| {
            let parts: Vec<&str> = vol.splitn(2, ':').collect();
            vec![Mount {
                target: Some(parts[1].to_string()),
                source: Some(parts[0].to_string()),
                typ: Some(MountTypeEnum::VOLUME),
                ..Default::default()
            }]
        });

        let host_config = HostConfig {
            network_mode: Some(network.to_string()),
            mounts,
            ..Default::default()
        };

        // Attach to network with service name as alias for hostname-based discovery
        let endpoint = EndpointSettings {
            aliases: Some(vec![service.name().to_string()]),
            ..Default::default()
        };
        let mut endpoints_config = HashMap::new();
        endpoints_config.insert(network.to_string(), endpoint);
        let networking_config = Some(NetworkingConfig { endpoints_config });

        let config = Config {
            image: Some(service.image()),
            env,
            host_config: Some(host_config),
            networking_config,
            ..Default::default()
        };

        let create_opts = CreateContainerOptions {
            name: container_name.clone(),
            ..Default::default()
        };

        let response = self
            .docker
            .create_container(Some(create_opts), config)
            .await
            .context(format!("failed to create {} container", service.name()))?;

        let container_id = response.id;
        info!(service = service.name(), id = %container_id, "service container created");

        self.docker
            .start_container::<String>(&container_id, None)
            .await
            .context(format!("failed to start {} container", service.name()))?;

        info!(service = service.name(), id = %container_id, "service container started");

        Ok(container_id)
    }

    /// Waits for a service container to become ready by retrying a readiness command.
    /// Uses `docker exec` with a retry loop (up to `max_retries` attempts with `interval` seconds between).
    pub fn wait_for_ready(
        &self,
        container_id: &str,
        service: &dyn Service,
        max_retries: u32,
        interval_secs: u64,
    ) -> Result<()> {
        let cmd = service.readiness_cmd();
        info!(
            service = service.name(),
            container = %container_id,
            "waiting for service to be ready"
        );

        for attempt in 1..=max_retries {
            let mut args = vec!["exec", container_id];
            let cmd_refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
            args.extend(&cmd_refs);

            let status = Command::new("docker")
                .args(&args)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();

            match status {
                Ok(s) if s.success() => {
                    info!(
                        service = service.name(),
                        attempt,
                        "service is ready"
                    );
                    return Ok(());
                }
                _ => {
                    if attempt < max_retries {
                        info!(
                            service = service.name(),
                            attempt,
                            max_retries,
                            "service not ready, retrying..."
                        );
                        std::thread::sleep(std::time::Duration::from_secs(interval_secs));
                    }
                }
            }
        }

        anyhow::bail!(
            "{} service did not become ready after {} attempts",
            service.name(),
            max_retries
        );
    }
}

/// Checks whether a container name matches the stale detection prefix.
/// Returns true if the name is exactly the prefix or starts with `prefix-`.
/// Container names from Docker include a leading `/`.
pub fn matches_stale_prefix(container_name: &str, prefix: &str) -> bool {
    let prefix_with_slash = format!("/{prefix}");
    container_name == prefix_with_slash
        || container_name.starts_with(&format!("{prefix_with_slash}-"))
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

    #[test]
    fn stale_prefix_matches_exact_container_name() {
        // Docker container names have leading `/`
        assert!(matches_stale_prefix(
            "/bubble-boy-myproject",
            "bubble-boy-myproject"
        ));
    }

    #[test]
    fn stale_prefix_matches_service_container() {
        // Service containers are named `bubble-boy-<project>-<service>`
        assert!(matches_stale_prefix(
            "/bubble-boy-myproject-mysql",
            "bubble-boy-myproject"
        ));
        assert!(matches_stale_prefix(
            "/bubble-boy-myproject-redis",
            "bubble-boy-myproject"
        ));
        assert!(matches_stale_prefix(
            "/bubble-boy-myproject-postgres",
            "bubble-boy-myproject"
        ));
    }

    #[test]
    fn stale_prefix_rejects_different_project() {
        // Should not match containers from a different project
        assert!(!matches_stale_prefix(
            "/bubble-boy-otherproject",
            "bubble-boy-myproject"
        ));
        assert!(!matches_stale_prefix(
            "/bubble-boy-otherproject-mysql",
            "bubble-boy-myproject"
        ));
    }

    #[test]
    fn stale_prefix_rejects_non_bubble_boy() {
        assert!(!matches_stale_prefix("/some-other-container", "bubble-boy-myproject"));
    }
}
