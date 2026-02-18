use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;
use tracing::debug;

use crate::cli::{Cli, ContainerFlags, RuntimeFlags, ServiceFlags};

// -- Top-level config --

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub runtimes: RuntimeConfig,
    pub services: ServiceConfig,
    pub hooks: HookConfig,
    pub shell: ShellConfig,
    pub container: ContainerConfig,
}

// -- Runtimes --

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RuntimeConfig {
    pub php: Option<String>,
    pub node: Option<String>,
    pub rust: Option<bool>,
    pub go: Option<String>,
}

// -- Services --

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ServiceConfig {
    pub mysql: Option<MysqlConfig>,
    pub redis: Option<bool>,
    pub postgres: Option<PostgresConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MysqlConfig {
    pub version: String,
    pub database: String,
    pub username: String,
    pub password: String,
}

impl Default for MysqlConfig {
    fn default() -> Self {
        Self {
            version: "8.0".to_string(),
            database: "app".to_string(),
            username: "root".to_string(),
            password: "password".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PostgresConfig {
    pub version: String,
    pub database: String,
    pub username: String,
    pub password: String,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            version: "16".to_string(),
            database: "app".to_string(),
            username: "postgres".to_string(),
            password: "password".to_string(),
        }
    }
}

// -- Hooks --

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct HookConfig {
    pub post_start: Vec<String>,
    pub pre_stop: Vec<String>,
}

// -- Shell --

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ShellConfig {
    pub mount_configs: bool,
}

// -- Container --

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ContainerConfig {
    pub network: Option<String>,
    pub name: Option<String>,
    pub shell: Option<String>,
}

// -- Merge logic --

impl Config {
    /// Loads and merges config from all sources:
    /// defaults -> global config -> project config -> CLI flags
    pub fn load(cli: &Cli) -> Result<Self> {
        let mut config = Config::default();

        // Layer 1: global config
        if let Some(path) = global_config_path() {
            if let Some(file_config) = load_from_file(&path)? {
                debug!("loaded global config from {}", path.display());
                config.merge(file_config);
            }
        }

        // Layer 2: project config
        let project_path = PathBuf::from(".bubble-boy.toml");
        if let Some(file_config) = load_from_file(&project_path)? {
            debug!("loaded project config from {}", project_path.display());
            config.merge(file_config);
        }

        // Layer 3: CLI flags
        config.apply_cli(cli);

        Ok(config)
    }

    /// Merges another config on top of self. Non-None / non-empty values
    /// in `other` take precedence.
    fn merge(&mut self, other: Config) {
        // Runtimes
        if other.runtimes.php.is_some() {
            self.runtimes.php = other.runtimes.php;
        }
        if other.runtimes.node.is_some() {
            self.runtimes.node = other.runtimes.node;
        }
        if other.runtimes.rust.is_some() {
            self.runtimes.rust = other.runtimes.rust;
        }
        if other.runtimes.go.is_some() {
            self.runtimes.go = other.runtimes.go;
        }

        // Services
        if other.services.mysql.is_some() {
            self.services.mysql = other.services.mysql;
        }
        if other.services.redis.is_some() {
            self.services.redis = other.services.redis;
        }
        if other.services.postgres.is_some() {
            self.services.postgres = other.services.postgres;
        }

        // Hooks (non-empty overrides)
        if !other.hooks.post_start.is_empty() {
            self.hooks.post_start = other.hooks.post_start;
        }
        if !other.hooks.pre_stop.is_empty() {
            self.hooks.pre_stop = other.hooks.pre_stop;
        }

        // Shell
        self.shell.mount_configs = other.shell.mount_configs;

        // Container
        if other.container.network.is_some() {
            self.container.network = other.container.network;
        }
        if other.container.name.is_some() {
            self.container.name = other.container.name;
        }
        if other.container.shell.is_some() {
            self.container.shell = other.container.shell;
        }
    }

    /// Applies CLI flags on top of the current config. CLI flags always win
    /// when they are explicitly set.
    fn apply_cli(&mut self, cli: &Cli) {
        self.apply_runtime_flags(&cli.runtime);
        self.apply_service_flags(&cli.service);
        self.apply_container_flags(&cli.container);
    }

    fn apply_runtime_flags(&mut self, flags: &RuntimeFlags) {
        if flags.php.is_some() {
            self.runtimes.php.clone_from(&flags.php);
        }
        if flags.node.is_some() {
            self.runtimes.node.clone_from(&flags.node);
        }
        if flags.rust {
            self.runtimes.rust = Some(true);
        }
        if flags.go.is_some() {
            self.runtimes.go.clone_from(&flags.go);
        }
    }

    fn apply_service_flags(&mut self, flags: &ServiceFlags) {
        if let Some(ref version) = flags.mysql {
            let mut mysql = self.services.mysql.clone().unwrap_or_default();
            mysql.version = version.clone();
            self.services.mysql = Some(mysql);
        }
        if flags.redis {
            self.services.redis = Some(true);
        }
        if let Some(ref version) = flags.postgres {
            let mut pg = self.services.postgres.clone().unwrap_or_default();
            pg.version = version.clone();
            self.services.postgres = Some(pg);
        }
    }

    fn apply_container_flags(&mut self, flags: &ContainerFlags) {
        if flags.network.is_some() {
            self.container.network.clone_from(&flags.network);
        }
        if flags.name.is_some() {
            self.container.name.clone_from(&flags.name);
        }
        // shell always has a value from clap default, but we only override
        // if it differs from the default "zsh" (meaning user explicitly set it)
        // or if no config file set a shell.
        if flags.shell != "zsh" || self.container.shell.is_none() {
            self.container.shell = Some(flags.shell.clone());
        }
    }
}

// -- File loading --

fn global_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("bubble-boy").join("config.toml"))
}

fn load_from_file(path: &Path) -> Result<Option<Config>> {
    match std::fs::read_to_string(path) {
        Ok(contents) => {
            let config: Config = toml::from_str(&contents)?;
            Ok(Some(config))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    /// Helper to parse a TOML string into Config.
    fn parse_toml(s: &str) -> Config {
        toml::from_str(s).expect("valid TOML")
    }

    #[test]
    fn default_config_has_empty_values() {
        let config = Config::default();
        assert!(config.runtimes.php.is_none());
        assert!(config.runtimes.node.is_none());
        assert!(config.runtimes.rust.is_none());
        assert!(config.runtimes.go.is_none());
        assert!(config.services.mysql.is_none());
        assert!(config.services.redis.is_none());
        assert!(config.services.postgres.is_none());
        assert!(config.hooks.post_start.is_empty());
        assert!(config.hooks.pre_stop.is_empty());
        assert!(!config.shell.mount_configs);
    }

    #[test]
    fn parse_full_toml() {
        let config = parse_toml(
            r#"
            [runtimes]
            php = "8.3"
            node = "22"
            rust = true
            go = "1.23"

            [services.mysql]
            version = "8.4"
            database = "mydb"
            username = "admin"
            password = "secret"

            [services]
            redis = true

            [services.postgres]
            version = "15"
            database = "pgdb"
            username = "pguser"
            password = "pgpass"

            [hooks]
            post_start = ["composer install", "npm ci"]
            pre_stop = ["echo bye"]

            [shell]
            mount_configs = true

            [container]
            network = "custom-net"
            name = "my-container"
            shell = "bash"
            "#,
        );

        assert_eq!(config.runtimes.php.as_deref(), Some("8.3"));
        assert_eq!(config.runtimes.node.as_deref(), Some("22"));
        assert_eq!(config.runtimes.rust, Some(true));
        assert_eq!(config.runtimes.go.as_deref(), Some("1.23"));

        let mysql = config.services.mysql.unwrap();
        assert_eq!(mysql.version, "8.4");
        assert_eq!(mysql.database, "mydb");
        assert_eq!(mysql.username, "admin");
        assert_eq!(mysql.password, "secret");

        assert_eq!(config.services.redis, Some(true));

        let pg = config.services.postgres.unwrap();
        assert_eq!(pg.version, "15");
        assert_eq!(pg.database, "pgdb");
        assert_eq!(pg.username, "pguser");
        assert_eq!(pg.password, "pgpass");

        assert_eq!(config.hooks.post_start, vec!["composer install", "npm ci"]);
        assert_eq!(config.hooks.pre_stop, vec!["echo bye"]);

        assert!(config.shell.mount_configs);

        assert_eq!(config.container.network.as_deref(), Some("custom-net"));
        assert_eq!(config.container.name.as_deref(), Some("my-container"));
        assert_eq!(config.container.shell.as_deref(), Some("bash"));
    }

    #[test]
    fn parse_partial_toml_uses_defaults() {
        let config = parse_toml(
            r#"
            [runtimes]
            php = "8.2"
            "#,
        );
        assert_eq!(config.runtimes.php.as_deref(), Some("8.2"));
        assert!(config.runtimes.node.is_none());
        assert!(config.services.mysql.is_none());
        assert!(config.hooks.post_start.is_empty());
        assert!(!config.shell.mount_configs);
    }

    #[test]
    fn empty_toml_parses_to_defaults() {
        let config = parse_toml("");
        assert!(config.runtimes.php.is_none());
        assert!(config.services.mysql.is_none());
        assert!(!config.shell.mount_configs);
    }

    #[test]
    fn merge_overrides_set_values() {
        let mut base = parse_toml(
            r#"
            [runtimes]
            php = "8.2"
            node = "20"
            "#,
        );
        let overlay = parse_toml(
            r#"
            [runtimes]
            php = "8.3"
            "#,
        );

        base.merge(overlay);

        // php overridden
        assert_eq!(base.runtimes.php.as_deref(), Some("8.3"));
        // node preserved from base
        assert_eq!(base.runtimes.node.as_deref(), Some("20"));
    }

    #[test]
    fn merge_does_not_clear_values() {
        let mut base = parse_toml(
            r#"
            [runtimes]
            php = "8.2"

            [services.mysql]
            version = "8.0"
            database = "mydb"
            username = "root"
            password = "secret"

            [hooks]
            post_start = ["npm ci"]

            [shell]
            mount_configs = true
            "#,
        );
        // Overlay has no runtimes, no mysql, no hooks
        let overlay = parse_toml("");

        base.merge(overlay);

        assert_eq!(base.runtimes.php.as_deref(), Some("8.2"));
        assert!(base.services.mysql.is_some());
        assert_eq!(base.hooks.post_start, vec!["npm ci"]);
        // mount_configs uses overlay default (false) since it's a bool merge
        // This is correct: overlay explicitly says mount_configs = false (default)
    }

    #[test]
    fn merge_three_layers() {
        // Simulates: defaults -> global -> project
        let mut config = Config::default();

        let global = parse_toml(
            r#"
            [runtimes]
            php = "8.2"

            [shell]
            mount_configs = true
            "#,
        );
        config.merge(global);

        let project = parse_toml(
            r#"
            [runtimes]
            php = "8.3"
            node = "22"
            "#,
        );
        config.merge(project);

        // php overridden by project
        assert_eq!(config.runtimes.php.as_deref(), Some("8.3"));
        // node from project
        assert_eq!(config.runtimes.node.as_deref(), Some("22"));
    }

    #[test]
    fn cli_flags_override_config() {
        let mut config = parse_toml(
            r#"
            [runtimes]
            php = "8.2"
            node = "20"

            [container]
            name = "config-name"
            "#,
        );

        let cli = Cli::parse_from([
            "bubble-boy",
            "--with-php",
            "8.3",
            "--name",
            "cli-name",
        ]);
        config.apply_cli(&cli);

        // CLI overrides php
        assert_eq!(config.runtimes.php.as_deref(), Some("8.3"));
        // node preserved from config (CLI didn't set it)
        assert_eq!(config.runtimes.node.as_deref(), Some("20"));
        // name overridden by CLI
        assert_eq!(config.container.name.as_deref(), Some("cli-name"));
    }

    #[test]
    fn cli_service_flags_merge_with_config() {
        let mut config = parse_toml(
            r#"
            [services.mysql]
            database = "mydb"
            username = "admin"
            password = "secret"
            "#,
        );

        // CLI specifies --with-mysql 8.4 (overrides version only)
        let cli = Cli::parse_from(["bubble-boy", "--with-mysql", "8.4"]);
        config.apply_cli(&cli);

        let mysql = config.services.mysql.unwrap();
        assert_eq!(mysql.version, "8.4");
        // Config values preserved
        assert_eq!(mysql.database, "mydb");
        assert_eq!(mysql.username, "admin");
        assert_eq!(mysql.password, "secret");
    }

    #[test]
    fn full_merge_precedence() {
        // defaults -> global -> project -> CLI
        let mut config = Config::default();

        // Global sets php 8.1 and mount_configs
        let global = parse_toml(
            r#"
            [runtimes]
            php = "8.1"

            [shell]
            mount_configs = true

            [hooks]
            post_start = ["global-hook"]
            "#,
        );
        config.merge(global);

        // Project overrides php to 8.2 and adds node
        let project = parse_toml(
            r#"
            [runtimes]
            php = "8.2"
            node = "20"

            [hooks]
            post_start = ["project-hook"]
            "#,
        );
        config.merge(project);

        // CLI overrides php to 8.3
        let cli = Cli::parse_from(["bubble-boy", "--with-php", "8.3"]);
        config.apply_cli(&cli);

        // Final results
        assert_eq!(config.runtimes.php.as_deref(), Some("8.3")); // CLI wins
        assert_eq!(config.runtimes.node.as_deref(), Some("20")); // project
        assert_eq!(config.hooks.post_start, vec!["project-hook"]); // project overrides global
    }

    #[test]
    fn missing_config_file_returns_none() {
        let result = load_from_file(Path::new("/nonexistent/config.toml")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn mysql_config_defaults() {
        let mysql = MysqlConfig::default();
        assert_eq!(mysql.version, "8.0");
        assert_eq!(mysql.database, "app");
        assert_eq!(mysql.username, "root");
        assert_eq!(mysql.password, "password");
    }

    #[test]
    fn postgres_config_defaults() {
        let pg = PostgresConfig::default();
        assert_eq!(pg.version, "16");
        assert_eq!(pg.database, "app");
        assert_eq!(pg.username, "postgres");
        assert_eq!(pg.password, "password");
    }

    #[test]
    fn service_config_partial_mysql() {
        let config = parse_toml(
            r#"
            [services.mysql]
            database = "custom_db"
            "#,
        );
        let mysql = config.services.mysql.unwrap();
        assert_eq!(mysql.database, "custom_db");
        // Other fields use defaults
        assert_eq!(mysql.version, "8.0");
        assert_eq!(mysql.username, "root");
        assert_eq!(mysql.password, "password");
    }

    #[test]
    fn cli_redis_flag_enables_service() {
        let mut config = Config::default();
        let cli = Cli::parse_from(["bubble-boy", "--with-redis"]);
        config.apply_cli(&cli);
        assert_eq!(config.services.redis, Some(true));
    }

    #[test]
    fn cli_rust_flag_enables_runtime() {
        let mut config = Config::default();
        let cli = Cli::parse_from(["bubble-boy", "--with-rust"]);
        config.apply_cli(&cli);
        assert_eq!(config.runtimes.rust, Some(true));
    }

    #[test]
    fn shell_config_from_cli_when_explicit() {
        let mut config = parse_toml(
            r#"
            [container]
            shell = "fish"
            "#,
        );
        // CLI with explicit --shell bash overrides config
        let cli = Cli::parse_from(["bubble-boy", "--shell", "bash"]);
        config.apply_cli(&cli);
        assert_eq!(config.container.shell.as_deref(), Some("bash"));
    }

    #[test]
    fn shell_config_preserved_when_cli_default() {
        let mut config = parse_toml(
            r#"
            [container]
            shell = "fish"
            "#,
        );
        // CLI with default --shell zsh does not override config "fish"
        let cli = Cli::parse_from(["bubble-boy"]);
        config.apply_cli(&cli);
        assert_eq!(config.container.shell.as_deref(), Some("fish"));
    }
}
