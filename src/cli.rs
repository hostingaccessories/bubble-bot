use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "bubble-bot", about = "Ephemeral Docker dev containers")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub runtime: RuntimeFlags,

    #[command(flatten)]
    pub service: ServiceFlags,

    #[command(flatten)]
    pub container: ContainerFlags,
}

impl Cli {
    /// Returns the resolved command, defaulting to `shell` if none provided.
    pub fn command(&self) -> Command {
        self.command.clone().unwrap_or(Command::Shell)
    }
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Open an interactive shell in the container (default)
    Shell,

    /// Run Claude Code inside the container
    Claude {
        /// Arguments passed to Claude Code
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Run Chief (autonomous Claude Code task runner) inside the container
    Chief {
        /// Arguments passed to Chief
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Run a command inside the container and exit
    Exec {
        /// Command and arguments to run
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        cmd: Vec<String>,
    },

    /// Build the container image without starting a container
    Build,

    /// Show the resolved configuration
    Config,

    /// Remove Bubble Bot images, networks, and optionally volumes
    Clean {
        /// Also remove named volumes
        #[arg(long)]
        volumes: bool,
    },
}

#[derive(Debug, Clone, Args)]
pub struct RuntimeFlags {
    /// Include PHP runtime (e.g. 8.1, 8.2, 8.3)
    #[arg(long = "with-php", value_name = "VERSION")]
    pub php: Option<String>,

    /// Include Node.js runtime (e.g. 18, 20, 22)
    #[arg(long = "with-node", value_name = "VERSION")]
    pub node: Option<String>,

    /// Include Rust toolchain
    #[arg(long = "with-rust")]
    pub rust: bool,

    /// Include Go runtime (e.g. 1.22, 1.23)
    #[arg(long = "with-go", value_name = "VERSION")]
    pub go: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct ServiceFlags {
    /// Start a MySQL service container (optional version, default 8.0)
    #[arg(long = "with-mysql", value_name = "VERSION", num_args = 0..=1, default_missing_value = "8.0")]
    pub mysql: Option<String>,

    /// Start a Redis service container
    #[arg(long = "with-redis")]
    pub redis: bool,

    /// Start a PostgreSQL service container (optional version, default 16)
    #[arg(long = "with-postgres", value_name = "VERSION", num_args = 0..=1, default_missing_value = "16")]
    pub postgres: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct ContainerFlags {
    /// Docker network name
    #[arg(long)]
    pub network: Option<String>,

    /// Container name
    #[arg(long)]
    pub name: Option<String>,

    /// Shell to use inside the container
    #[arg(long, default_value = "bash")]
    pub shell: String,

    /// Force rebuild ignoring cache
    #[arg(long)]
    pub no_cache: bool,

    /// Show what would be run without executing
    #[arg(long)]
    pub dry_run: bool,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn no_subcommand_defaults_to_shell() {
        let cli = Cli::parse_from(["bubble-bot"]);
        assert!(cli.command.is_none());
        assert!(matches!(cli.command(), Command::Shell));
    }

    #[test]
    fn shell_subcommand() {
        let cli = Cli::parse_from(["bubble-bot", "shell"]);
        assert!(matches!(cli.command(), Command::Shell));
    }

    #[test]
    fn claude_subcommand_with_trailing_args() {
        let cli = Cli::parse_from(["bubble-bot", "claude", "--", "-p", "fix bug"]);
        match cli.command() {
            Command::Claude { args } => {
                assert_eq!(args, vec!["-p", "fix bug"]);
            }
            _ => panic!("expected Claude subcommand"),
        }
    }

    #[test]
    fn chief_subcommand_with_trailing_args() {
        let cli = Cli::parse_from(["bubble-bot", "chief", "--", "--task", "deploy"]);
        match cli.command() {
            Command::Chief { args } => {
                assert_eq!(args, vec!["--task", "deploy"]);
            }
            _ => panic!("expected Chief subcommand"),
        }
    }

    #[test]
    fn exec_subcommand_requires_cmd() {
        let cli = Cli::parse_from(["bubble-bot", "exec", "--", "ls", "-la"]);
        match cli.command() {
            Command::Exec { cmd } => {
                assert_eq!(cmd, vec!["ls", "-la"]);
            }
            _ => panic!("expected Exec subcommand"),
        }
    }

    #[test]
    fn build_subcommand() {
        let cli = Cli::parse_from(["bubble-bot", "build"]);
        assert!(matches!(cli.command(), Command::Build));
    }

    #[test]
    fn config_subcommand() {
        let cli = Cli::parse_from(["bubble-bot", "config"]);
        assert!(matches!(cli.command(), Command::Config));
    }

    #[test]
    fn clean_subcommand_default() {
        let cli = Cli::parse_from(["bubble-bot", "clean"]);
        match cli.command() {
            Command::Clean { volumes } => assert!(!volumes),
            _ => panic!("expected Clean subcommand"),
        }
    }

    #[test]
    fn clean_subcommand_with_volumes() {
        let cli = Cli::parse_from(["bubble-bot", "clean", "--volumes"]);
        match cli.command() {
            Command::Clean { volumes } => assert!(volumes),
            _ => panic!("expected Clean subcommand"),
        }
    }

    #[test]
    fn runtime_flags() {
        let cli = Cli::parse_from([
            "bubble-bot",
            "--with-php", "8.3",
            "--with-node", "22",
            "--with-rust",
            "--with-go", "1.23",
        ]);
        assert_eq!(cli.runtime.php.as_deref(), Some("8.3"));
        assert_eq!(cli.runtime.node.as_deref(), Some("22"));
        assert!(cli.runtime.rust);
        assert_eq!(cli.runtime.go.as_deref(), Some("1.23"));
    }

    #[test]
    fn service_flags() {
        let cli = Cli::parse_from([
            "bubble-bot",
            "--with-mysql",
            "--with-redis",
            "--with-postgres",
        ]);
        assert_eq!(cli.service.mysql.as_deref(), Some("8.0"));
        assert!(cli.service.redis);
        assert_eq!(cli.service.postgres.as_deref(), Some("16"));
    }

    #[test]
    fn service_flags_with_versions() {
        let cli = Cli::parse_from([
            "bubble-bot",
            "--with-mysql", "8.4",
            "--with-postgres", "15",
        ]);
        assert_eq!(cli.service.mysql.as_deref(), Some("8.4"));
        assert_eq!(cli.service.postgres.as_deref(), Some("15"));
    }

    #[test]
    fn container_flags() {
        let cli = Cli::parse_from([
            "bubble-bot",
            "--network", "mynet",
            "--name", "mycontainer",
            "--shell", "bash",
            "--no-cache",
            "--dry-run",
        ]);
        assert_eq!(cli.container.network.as_deref(), Some("mynet"));
        assert_eq!(cli.container.name.as_deref(), Some("mycontainer"));
        assert_eq!(cli.container.shell, "bash");
        assert!(cli.container.no_cache);
        assert!(cli.container.dry_run);
    }

    #[test]
    fn shell_defaults_to_bash() {
        let cli = Cli::parse_from(["bubble-bot"]);
        assert_eq!(cli.container.shell, "bash");
    }

    #[test]
    fn combined_flags_with_subcommand() {
        let cli = Cli::parse_from([
            "bubble-bot",
            "--with-php", "8.3",
            "--with-node", "22",
            "--with-mysql",
            "--with-redis",
            "--dry-run",
            "claude",
            "--",
            "-p",
            "help me",
        ]);
        assert_eq!(cli.runtime.php.as_deref(), Some("8.3"));
        assert_eq!(cli.runtime.node.as_deref(), Some("22"));
        assert_eq!(cli.service.mysql.as_deref(), Some("8.0"));
        assert!(cli.service.redis);
        assert!(cli.container.dry_run);
        match cli.command() {
            Command::Claude { args } => {
                assert_eq!(args, vec!["-p", "help me"]);
            }
            _ => panic!("expected Claude subcommand"),
        }
    }
}
