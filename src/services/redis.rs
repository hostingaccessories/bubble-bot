use crate::services::Service;

pub struct RedisService {
    project_name: String,
}

impl RedisService {
    pub fn new(project_name: String) -> Self {
        Self { project_name }
    }
}

impl Service for RedisService {
    fn name(&self) -> &str {
        "redis"
    }

    fn image(&self) -> String {
        "redis:alpine".to_string()
    }

    fn container_env(&self) -> Vec<String> {
        Vec::new()
    }

    fn dev_env(&self) -> Vec<String> {
        vec![
            "REDIS_HOST=redis".to_string(),
            "REDIS_PORT=6379".to_string(),
        ]
    }

    fn volume(&self) -> Option<String> {
        None
    }

    fn readiness_cmd(&self) -> Vec<String> {
        vec![
            "redis-cli".to_string(),
            "ping".to_string(),
        ]
    }

    fn container_name(&self, _project: &str) -> String {
        format!("bubble-boy-{}-redis", self.project_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_service() -> RedisService {
        RedisService::new("testproject".to_string())
    }

    #[test]
    fn name_is_redis() {
        assert_eq!(default_service().name(), "redis");
    }

    #[test]
    fn image_is_alpine() {
        assert_eq!(default_service().image(), "redis:alpine");
    }

    #[test]
    fn container_env_is_empty() {
        assert!(default_service().container_env().is_empty());
    }

    #[test]
    fn dev_env_has_connection_details() {
        let env = default_service().dev_env();
        assert!(env.contains(&"REDIS_HOST=redis".to_string()));
        assert!(env.contains(&"REDIS_PORT=6379".to_string()));
        assert_eq!(env.len(), 2);
    }

    #[test]
    fn no_volume() {
        assert!(default_service().volume().is_none());
    }

    #[test]
    fn readiness_cmd_is_redis_cli_ping() {
        let cmd = default_service().readiness_cmd();
        assert_eq!(cmd, vec!["redis-cli", "ping"]);
    }

    #[test]
    fn container_name_includes_project() {
        let svc = default_service();
        assert_eq!(svc.container_name("testproject"), "bubble-boy-testproject-redis");
    }
}
