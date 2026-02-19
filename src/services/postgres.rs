use crate::config::PostgresConfig;
use crate::services::Service;

pub struct PostgresService {
    config: PostgresConfig,
    project_name: String,
}

impl PostgresService {
    pub fn new(config: PostgresConfig, project_name: String) -> Self {
        Self {
            config,
            project_name,
        }
    }

    /// Volume name for PostgreSQL data persistence.
    fn volume_name(&self) -> String {
        format!("bubble-boy-{}-postgres-data", self.project_name)
    }
}

impl Service for PostgresService {
    fn name(&self) -> &str {
        "postgres"
    }

    fn image(&self) -> String {
        format!("postgres:{}", self.config.version)
    }

    fn container_env(&self) -> Vec<String> {
        vec![
            format!("POSTGRES_USER={}", self.config.username),
            format!("POSTGRES_PASSWORD={}", self.config.password),
            format!("POSTGRES_DB={}", self.config.database),
        ]
    }

    fn dev_env(&self) -> Vec<String> {
        vec![
            "DB_HOST=postgres".to_string(),
            "DB_PORT=5432".to_string(),
            format!("DB_DATABASE={}", self.config.database),
            format!("DB_USERNAME={}", self.config.username),
            format!("DB_PASSWORD={}", self.config.password),
        ]
    }

    fn volume(&self) -> Option<String> {
        Some(format!("{}:/var/lib/postgresql/data", self.volume_name()))
    }

    fn readiness_cmd(&self) -> Vec<String> {
        vec![
            "pg_isready".to_string(),
            "-U".to_string(),
            self.config.username.clone(),
        ]
    }

    fn container_name(&self, _project: &str) -> String {
        format!("bubble-boy-{}-postgres", self.project_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_service() -> PostgresService {
        PostgresService::new(PostgresConfig::default(), "testproject".to_string())
    }

    #[test]
    fn name_is_postgres() {
        assert_eq!(default_service().name(), "postgres");
    }

    #[test]
    fn image_uses_config_version() {
        let svc = default_service();
        assert_eq!(svc.image(), "postgres:16");

        let svc = PostgresService::new(
            PostgresConfig {
                version: "15".to_string(),
                ..Default::default()
            },
            "proj".to_string(),
        );
        assert_eq!(svc.image(), "postgres:15");
    }

    #[test]
    fn container_env_has_postgres_vars() {
        let svc = default_service();
        let env = svc.container_env();
        assert!(env.contains(&"POSTGRES_USER=postgres".to_string()));
        assert!(env.contains(&"POSTGRES_PASSWORD=password".to_string()));
        assert!(env.contains(&"POSTGRES_DB=app".to_string()));
        assert_eq!(env.len(), 3);
    }

    #[test]
    fn container_env_custom_config() {
        let svc = PostgresService::new(
            PostgresConfig {
                username: "admin".to_string(),
                password: "secret".to_string(),
                database: "mydb".to_string(),
                ..Default::default()
            },
            "proj".to_string(),
        );
        let env = svc.container_env();
        assert!(env.contains(&"POSTGRES_USER=admin".to_string()));
        assert!(env.contains(&"POSTGRES_PASSWORD=secret".to_string()));
        assert!(env.contains(&"POSTGRES_DB=mydb".to_string()));
    }

    #[test]
    fn dev_env_has_connection_details() {
        let svc = default_service();
        let env = svc.dev_env();
        assert!(env.contains(&"DB_HOST=postgres".to_string()));
        assert!(env.contains(&"DB_PORT=5432".to_string()));
        assert!(env.contains(&"DB_DATABASE=app".to_string()));
        assert!(env.contains(&"DB_USERNAME=postgres".to_string()));
        assert!(env.contains(&"DB_PASSWORD=password".to_string()));
    }

    #[test]
    fn volume_uses_project_name() {
        let svc = default_service();
        assert_eq!(
            svc.volume().unwrap(),
            "bubble-boy-testproject-postgres-data:/var/lib/postgresql/data"
        );
    }

    #[test]
    fn container_name_includes_project() {
        let svc = default_service();
        assert_eq!(
            svc.container_name("testproject"),
            "bubble-boy-testproject-postgres"
        );
    }

    #[test]
    fn readiness_cmd_is_pg_isready() {
        let svc = default_service();
        let cmd = svc.readiness_cmd();
        assert_eq!(cmd, vec!["pg_isready", "-U", "postgres"]);
    }
}
