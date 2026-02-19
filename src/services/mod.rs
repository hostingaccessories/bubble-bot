pub mod mysql;
pub mod postgres;
pub mod redis;

/// Trait for service containers (MySQL, Redis, PostgreSQL, etc.)
/// that run alongside the dev container on a shared network.
pub trait Service {
    /// Short identifier used as hostname alias on the bridge network (e.g., "mysql").
    fn name(&self) -> &str;

    /// Docker image to pull/use (e.g., "mysql:8.0").
    fn image(&self) -> String;

    /// Environment variables for the **service** container itself
    /// (e.g., `MYSQL_ROOT_PASSWORD`). Returned as `KEY=VALUE` strings.
    fn container_env(&self) -> Vec<String>;

    /// Environment variables to inject into the **dev** container
    /// so the application can connect to this service.
    /// Returned as `KEY=VALUE` strings.
    fn dev_env(&self) -> Vec<String>;

    /// Optional named volume mount in `host_src:container_dest` format
    /// for data persistence across container restarts.
    fn volume(&self) -> Option<String>;

    /// Command to run via `docker exec` to check if the service is ready.
    /// Returns the full command as a string slice.
    fn readiness_cmd(&self) -> Vec<String>;

    /// Container name for this service instance.
    fn container_name(&self, project: &str) -> String {
        format!("bubble-boy-{project}-{}", self.name())
    }
}
