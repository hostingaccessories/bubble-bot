use crate::config::MysqlConfig;
use crate::services::Service;

pub struct MysqlService {
    config: MysqlConfig,
    project_name: String,
}

impl MysqlService {
    pub fn new(config: MysqlConfig, project_name: String) -> Self {
        Self {
            config,
            project_name,
        }
    }

    /// Volume name for MySQL data persistence.
    fn volume_name(&self) -> String {
        format!("bubble-bot-{}-mysql-data", self.project_name)
    }
}

impl Service for MysqlService {
    fn name(&self) -> &str {
        "mysql"
    }

    fn image(&self) -> String {
        format!("mysql:{}", self.config.version)
    }

    fn container_env(&self) -> Vec<String> {
        let mut env = vec![
            format!("MYSQL_ROOT_PASSWORD={}", self.config.password),
            format!("MYSQL_DATABASE={}", self.config.database),
        ];
        // Only set MYSQL_USER and MYSQL_PASSWORD for non-root users
        if self.config.username != "root" {
            env.push(format!("MYSQL_USER={}", self.config.username));
            env.push(format!("MYSQL_PASSWORD={}", self.config.password));
        }
        env
    }

    fn dev_env(&self) -> Vec<String> {
        vec![
            "DB_HOST=mysql".to_string(),
            "DB_PORT=3306".to_string(),
            format!("DB_DATABASE={}", self.config.database),
            format!("DB_USERNAME={}", self.config.username),
            format!("DB_PASSWORD={}", self.config.password),
        ]
    }

    fn volume(&self) -> Option<String> {
        Some(format!("{}:/var/lib/mysql", self.volume_name()))
    }

    fn readiness_cmd(&self) -> Vec<String> {
        vec![
            "mysqladmin".to_string(),
            "ping".to_string(),
            "-h".to_string(),
            "127.0.0.1".to_string(),
            "--silent".to_string(),
        ]
    }

    fn container_name(&self, _project: &str) -> String {
        format!("bubble-bot-{}-mysql", self.project_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_service() -> MysqlService {
        MysqlService::new(MysqlConfig::default(), "testproject".to_string())
    }

    #[test]
    fn name_is_mysql() {
        assert_eq!(default_service().name(), "mysql");
    }

    #[test]
    fn image_uses_config_version() {
        let svc = default_service();
        assert_eq!(svc.image(), "mysql:8.0");

        let svc = MysqlService::new(
            MysqlConfig {
                version: "8.4".to_string(),
                ..Default::default()
            },
            "proj".to_string(),
        );
        assert_eq!(svc.image(), "mysql:8.4");
    }

    #[test]
    fn container_env_root_user() {
        let svc = default_service();
        let env = svc.container_env();
        assert!(env.contains(&"MYSQL_ROOT_PASSWORD=password".to_string()));
        assert!(env.contains(&"MYSQL_DATABASE=app".to_string()));
        // root user should NOT have MYSQL_USER set
        assert!(!env.iter().any(|e| e.starts_with("MYSQL_USER=")));
    }

    #[test]
    fn container_env_non_root_user() {
        let svc = MysqlService::new(
            MysqlConfig {
                username: "admin".to_string(),
                password: "secret".to_string(),
                ..Default::default()
            },
            "proj".to_string(),
        );
        let env = svc.container_env();
        assert!(env.contains(&"MYSQL_ROOT_PASSWORD=secret".to_string()));
        assert!(env.contains(&"MYSQL_USER=admin".to_string()));
        assert!(env.contains(&"MYSQL_PASSWORD=secret".to_string()));
    }

    #[test]
    fn dev_env_has_connection_details() {
        let svc = default_service();
        let env = svc.dev_env();
        assert!(env.contains(&"DB_HOST=mysql".to_string()));
        assert!(env.contains(&"DB_PORT=3306".to_string()));
        assert!(env.contains(&"DB_DATABASE=app".to_string()));
        assert!(env.contains(&"DB_USERNAME=root".to_string()));
        assert!(env.contains(&"DB_PASSWORD=password".to_string()));
    }

    #[test]
    fn volume_uses_project_name() {
        let svc = default_service();
        assert_eq!(
            svc.volume().unwrap(),
            "bubble-bot-testproject-mysql-data:/var/lib/mysql"
        );
    }

    #[test]
    fn container_name_includes_project() {
        let svc = default_service();
        assert_eq!(
            svc.container_name("testproject"),
            "bubble-bot-testproject-mysql"
        );
    }

    #[test]
    fn readiness_cmd_is_mysqladmin_ping() {
        let svc = default_service();
        let cmd = svc.readiness_cmd();
        assert_eq!(cmd[0], "mysqladmin");
        assert!(cmd.contains(&"ping".to_string()));
    }
}
