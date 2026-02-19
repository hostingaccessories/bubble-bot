pub mod mysql;
pub mod postgres;
pub mod redis;

use crate::config::Config;

use mysql::MysqlService;
use postgres::PostgresService;
use redis::RedisService;

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

/// Collects service containers to start based on the resolved config.
pub fn collect_services(config: &Config, project: &str) -> Vec<Box<dyn Service>> {
    let mut services: Vec<Box<dyn Service>> = Vec::new();

    if let Some(ref mysql_config) = config.services.mysql {
        services.push(Box::new(MysqlService::new(
            mysql_config.clone(),
            project.to_string(),
        )));
    }

    if config.services.redis == Some(true) {
        services.push(Box::new(RedisService::new(project.to_string())));
    }

    if let Some(ref postgres_config) = config.services.postgres {
        services.push(Box::new(PostgresService::new(
            postgres_config.clone(),
            project.to_string(),
        )));
    }

    services
}

/// Collects all dev container environment variables contributed by active services.
pub fn collect_service_env_vars(services: &[Box<dyn Service>]) -> Vec<String> {
    let mut env_vars = Vec::new();
    for service in services {
        env_vars.extend(service.dev_env());
    }
    env_vars
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{MysqlConfig, PostgresConfig, ServiceConfig};

    #[test]
    fn collect_services_empty_config() {
        let config = Config::default();
        let services = collect_services(&config, "test");
        assert!(services.is_empty());
    }

    #[test]
    fn collect_services_mysql_only() {
        let mut config = Config::default();
        config.services.mysql = Some(MysqlConfig::default());
        let services = collect_services(&config, "test");
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name(), "mysql");
    }

    #[test]
    fn collect_services_redis_only() {
        let mut config = Config::default();
        config.services.redis = Some(true);
        let services = collect_services(&config, "test");
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name(), "redis");
    }

    #[test]
    fn collect_services_postgres_only() {
        let mut config = Config::default();
        config.services.postgres = Some(PostgresConfig::default());
        let services = collect_services(&config, "test");
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name(), "postgres");
    }

    #[test]
    fn collect_services_all_three() {
        let config = Config {
            services: ServiceConfig {
                mysql: Some(MysqlConfig::default()),
                redis: Some(true),
                postgres: Some(PostgresConfig::default()),
            },
            ..Default::default()
        };
        let services = collect_services(&config, "test");
        assert_eq!(services.len(), 3);
        assert_eq!(services[0].name(), "mysql");
        assert_eq!(services[1].name(), "redis");
        assert_eq!(services[2].name(), "postgres");
    }

    #[test]
    fn collect_env_vars_empty() {
        let services: Vec<Box<dyn Service>> = Vec::new();
        let env = collect_service_env_vars(&services);
        assert!(env.is_empty());
    }

    #[test]
    fn collect_env_vars_mysql_and_redis() {
        let mut config = Config::default();
        config.services.mysql = Some(MysqlConfig::default());
        config.services.redis = Some(true);
        let services = collect_services(&config, "test");
        let env = collect_service_env_vars(&services);

        // MySQL contributes DB_* vars
        assert!(env.contains(&"DB_HOST=mysql".to_string()));
        assert!(env.contains(&"DB_PORT=3306".to_string()));
        assert!(env.contains(&"DB_DATABASE=app".to_string()));
        assert!(env.contains(&"DB_USERNAME=root".to_string()));
        assert!(env.contains(&"DB_PASSWORD=password".to_string()));

        // Redis contributes REDIS_* vars (no conflict with DB_*)
        assert!(env.contains(&"REDIS_HOST=redis".to_string()));
        assert!(env.contains(&"REDIS_PORT=6379".to_string()));

        assert_eq!(env.len(), 7);
    }

    #[test]
    fn collect_env_vars_all_services() {
        let config = Config {
            services: ServiceConfig {
                mysql: Some(MysqlConfig::default()),
                redis: Some(true),
                postgres: Some(PostgresConfig::default()),
            },
            ..Default::default()
        };
        let services = collect_services(&config, "test");
        let env = collect_service_env_vars(&services);

        // MySQL DB_* vars
        assert!(env.contains(&"DB_HOST=mysql".to_string()));
        assert!(env.contains(&"DB_PORT=3306".to_string()));

        // Redis REDIS_* vars
        assert!(env.contains(&"REDIS_HOST=redis".to_string()));
        assert!(env.contains(&"REDIS_PORT=6379".to_string()));

        // Postgres DB_* vars (will appear after MySQL's in the list)
        // Both contribute DB_HOST etc. — last one wins at the Docker level
        assert_eq!(env.len(), 12); // 5 MySQL + 2 Redis + 5 Postgres
    }

    #[test]
    fn redis_false_not_collected() {
        let mut config = Config::default();
        config.services.redis = Some(false);
        let services = collect_services(&config, "test");
        assert!(services.is_empty());
    }

    #[test]
    fn service_env_naming_convention() {
        // Verify consistent naming: DB_* for databases, REDIS_* for Redis
        let mysql = MysqlService::new(MysqlConfig::default(), "test".to_string());
        for var in mysql.dev_env() {
            assert!(var.starts_with("DB_"), "MySQL env var should start with DB_: {var}");
        }

        let redis = RedisService::new("test".to_string());
        for var in redis.dev_env() {
            assert!(var.starts_with("REDIS_"), "Redis env var should start with REDIS_: {var}");
        }

        let pg = PostgresService::new(PostgresConfig::default(), "test".to_string());
        for var in pg.dev_env() {
            assert!(var.starts_with("DB_"), "Postgres env var should start with DB_: {var}");
        }
    }
}
