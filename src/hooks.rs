use std::process::Command;

use tracing::{info, warn};

use crate::config::HookConfig;

/// Executes hook commands inside a running container.
pub struct HookRunner<'a> {
    container_id: &'a str,
    hooks: &'a HookConfig,
}

impl<'a> HookRunner<'a> {
    pub fn new(container_id: &'a str, hooks: &'a HookConfig) -> Self {
        Self {
            container_id,
            hooks,
        }
    }

    /// Runs all `post_start` hooks sequentially inside the container.
    /// Hook failures are logged but do not prevent further execution.
    pub fn run_post_start(&self) {
        if self.hooks.post_start.is_empty() {
            return;
        }
        info!("running post_start hooks");
        for cmd in &self.hooks.post_start {
            self.run_hook("post_start", cmd);
        }
    }

    /// Runs all `pre_stop` hooks sequentially inside the container.
    /// Hook failures are logged but do not prevent further execution.
    pub fn run_pre_stop(&self) {
        if self.hooks.pre_stop.is_empty() {
            return;
        }
        info!("running pre_stop hooks");
        for cmd in &self.hooks.pre_stop {
            self.run_hook("pre_stop", cmd);
        }
    }

    /// Executes a single hook command inside the container via `docker exec`.
    /// Output is streamed to the user's terminal (inherited stdio).
    /// Failures are logged as warnings but do not propagate errors.
    fn run_hook(&self, phase: &str, cmd: &str) {
        info!(phase, cmd, "executing hook");

        let status = Command::new("docker")
            .args(["exec", self.container_id, "sh", "-c", cmd])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status();

        match status {
            Ok(s) if s.success() => {
                info!(phase, cmd, "hook completed successfully");
            }
            Ok(s) => {
                let code = s.code().unwrap_or(-1);
                warn!(phase, cmd, code, "hook failed");
            }
            Err(e) => {
                warn!(phase, cmd, error = %e, "hook execution error");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_runner_creates_with_config() {
        let hooks = HookConfig {
            post_start: vec!["echo hello".to_string()],
            pre_stop: vec!["echo bye".to_string()],
        };
        let runner = HookRunner::new("test-container", &hooks);
        assert_eq!(runner.container_id, "test-container");
        assert_eq!(runner.hooks.post_start.len(), 1);
        assert_eq!(runner.hooks.pre_stop.len(), 1);
    }

    #[test]
    fn hook_runner_with_empty_hooks() {
        let hooks = HookConfig::default();
        let runner = HookRunner::new("test-container", &hooks);
        assert!(runner.hooks.post_start.is_empty());
        assert!(runner.hooks.pre_stop.is_empty());
    }

    #[test]
    fn hook_runner_with_multiple_hooks() {
        let hooks = HookConfig {
            post_start: vec![
                "composer install".to_string(),
                "npm ci".to_string(),
                "php artisan migrate".to_string(),
            ],
            pre_stop: vec!["echo shutting down".to_string(), "cleanup.sh".to_string()],
        };
        let runner = HookRunner::new("container-123", &hooks);
        assert_eq!(runner.hooks.post_start.len(), 3);
        assert_eq!(runner.hooks.pre_stop.len(), 2);
    }

    #[test]
    fn hook_config_from_default_is_empty() {
        let hooks = HookConfig::default();
        assert!(hooks.post_start.is_empty());
        assert!(hooks.pre_stop.is_empty());
    }
}
