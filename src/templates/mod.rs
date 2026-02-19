use anyhow::Result;
use minijinja::{Environment, context};

use crate::config::Config;
use crate::runtime;

static BASE_TEMPLATE: &str = include_str!("base.dockerfile");
static CHIEF_TEMPLATE: &str = include_str!("chief.dockerfile");
static ENTRYPOINT_SCRIPT: &str = include_str!("entrypoint.sh");

/// The result of rendering templates, containing the Dockerfile and any extra
/// files that must be included in the Docker build context.
#[derive(Debug, Clone)]
pub struct RenderResult {
    pub dockerfile: String,
    pub context_files: Vec<ContextFile>,
}

/// An extra file to include in the Docker build context alongside the Dockerfile.
#[derive(Debug, Clone)]
pub struct ContextFile {
    pub path: String,
    pub content: String,
    pub mode: u32,
}

/// Parameters extracted from config for template rendering.
#[derive(Debug, Clone)]
pub struct TemplateParams {
    pub php_version: Option<String>,
    pub node_version: Option<String>,
    pub rust_enabled: bool,
    pub go_version: Option<String>,
}

impl TemplateParams {
    pub fn from_config(config: &Config) -> Self {
        Self {
            php_version: config.runtimes.php.clone(),
            node_version: config.runtimes.node.clone(),
            rust_enabled: config.runtimes.rust.unwrap_or(false),
            go_version: config.runtimes.go.clone(),
        }
    }
}

pub struct TemplateRenderer<'a> {
    env: Environment<'a>,
}

impl<'a> TemplateRenderer<'a> {
    pub fn new() -> Result<Self> {
        let mut env = Environment::new();
        env.add_template("base", BASE_TEMPLATE)?;
        Ok(Self { env })
    }

    /// Renders the full Dockerfile by composing the base template with runtime
    /// layers discovered from the runtime registry, plus the entrypoint script.
    pub fn render(&self, config: &Config) -> Result<RenderResult> {
        self.render_with_options(config, false)
    }

    /// Renders the full Dockerfile with optional Chief installation.
    pub fn render_with_options(
        &self,
        config: &Config,
        install_chief: bool,
    ) -> Result<RenderResult> {
        let tmpl = self.env.get_template("base")?;
        let mut rendered = tmpl.render(context! {})?;

        // Collect runtimes via the registry (deterministic order: PHP, Node, Rust, Go)
        let runtimes = runtime::collect_runtimes(config)?;

        for rt in &runtimes {
            let mut rt_env = Environment::new();
            rt_env.add_template(rt.name(), rt.template())?;
            let rt_tmpl = rt_env.get_template(rt.name())?;
            let layer = rt_tmpl.render(rt.template_context())?;
            rendered.push('\n');
            rendered.push_str(&layer);
        }

        // Install Chief binary from GitHub releases when requested
        if install_chief {
            rendered.push('\n');
            rendered.push_str(CHIEF_TEMPLATE);
        }

        // Append entrypoint instructions
        rendered.push_str("\nCOPY entrypoint.sh /usr/local/bin/entrypoint.sh\n");
        rendered.push_str("RUN chmod +x /usr/local/bin/entrypoint.sh\n");
        rendered.push_str("ENTRYPOINT [\"/usr/local/bin/entrypoint.sh\"]\n");
        rendered.push_str("CMD [\"sleep\", \"infinity\"]\n");

        let context_files = vec![ContextFile {
            path: "entrypoint.sh".to_string(),
            content: ENTRYPOINT_SCRIPT.to_string(),
            mode: 0o755,
        }];

        Ok(RenderResult {
            dockerfile: rendered,
            context_files,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_runtimes(
        php: Option<&str>,
        node: Option<&str>,
        rust: bool,
        go: Option<&str>,
    ) -> Config {
        let mut config = Config::default();
        config.runtimes.php = php.map(String::from);
        config.runtimes.node = node.map(String::from);
        if rust {
            config.runtimes.rust = Some(true);
        }
        config.runtimes.go = go.map(String::from);
        config
    }

    #[test]
    fn render_base_template() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = Config::default();
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(output.contains("FROM ubuntu:24.04"));
        assert!(output.contains("git"));
        assert!(output.contains("curl"));
        assert!(output.contains("wget"));
        assert!(output.contains("unzip"));
        assert!(output.contains("build-essential"));
        assert!(output.contains("ca-certificates"));
        assert!(output.contains("mkdir -p /home/dev/.claude"));
        assert!(output.contains("chmod -R 777 /home/dev"));
        assert!(output.contains("claude.ai/install.sh"));
        assert!(output.contains("/home/dev/.local/bin"));
        assert!(output.contains("/etc/profile.d/claude.sh"));
        assert!(output.contains("WORKDIR /workspace"));
    }

    #[test]
    fn render_is_deterministic() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(Some("8.3"), Some("22"), true, Some("1.23"));

        let result1 = renderer.render(&config).unwrap();
        let result2 = renderer.render(&config).unwrap();
        assert_eq!(result1.dockerfile, result2.dockerfile);
    }

    #[test]
    fn base_template_uses_ubuntu_2404() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = Config::default();
        let result = renderer.render(&config).unwrap();
        let first_line = result.dockerfile.lines().next().unwrap();
        assert_eq!(first_line, "FROM ubuntu:24.04");
    }

    #[test]
    fn params_from_config() {
        let config = Config::default();
        let params = TemplateParams::from_config(&config);
        assert!(params.php_version.is_none());
        assert!(params.node_version.is_none());
        assert!(!params.rust_enabled);
        assert!(params.go_version.is_none());
    }

    #[test]
    fn params_from_config_with_runtimes() {
        let config = config_with_runtimes(Some("8.3"), Some("22"), true, Some("1.23"));

        let params = TemplateParams::from_config(&config);
        assert_eq!(params.php_version.as_deref(), Some("8.3"));
        assert_eq!(params.node_version.as_deref(), Some("22"));
        assert!(params.rust_enabled);
        assert_eq!(params.go_version.as_deref(), Some("1.23"));
    }

    #[test]
    fn render_with_php_83() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(Some("8.3"), None, false, None);
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(output.contains("FROM ubuntu:24.04"));
        assert!(output.contains("php8.3-cli"));
        assert!(output.contains("php8.3-mbstring"));
        assert!(output.contains("php8.3-xml"));
        assert!(output.contains("php8.3-curl"));
        assert!(output.contains("php8.3-zip"));
        assert!(output.contains("php8.3-bcmath"));
        assert!(output.contains("php8.3-intl"));
        assert!(output.contains("php8.3-mysql"));
        assert!(output.contains("php8.3-pgsql"));
        assert!(output.contains("php8.3-sqlite3"));
        assert!(output.contains("php8.3-redis"));
        assert!(output.contains("php8.3-gd"));
        assert!(output.contains("php8.3-dom"));
        assert!(output.contains("php8.3-tokenizer"));
        assert!(output.contains("ppa:ondrej/php"));
        assert!(output.contains("composer"));
    }

    #[test]
    fn render_with_php_81() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(Some("8.1"), None, false, None);
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(output.contains("php8.1-cli"));
        assert!(output.contains("php8.1-mbstring"));
        assert!(!output.contains("php8.3-cli"));
    }

    #[test]
    fn render_with_php_82() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(Some("8.2"), None, false, None);
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(output.contains("php8.2-cli"));
        assert!(output.contains("php8.2-redis"));
    }

    #[test]
    fn render_without_php_has_no_php_layer() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = Config::default();
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(!output.contains("php"));
        assert!(!output.contains("composer"));
    }

    #[test]
    fn render_php_is_deterministic() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(Some("8.3"), None, false, None);
        let result1 = renderer.render(&config).unwrap();
        let result2 = renderer.render(&config).unwrap();
        assert_eq!(result1.dockerfile, result2.dockerfile);
    }

    #[test]
    fn render_php_unsupported_version_errors() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(Some("7.4"), None, false, None);
        let result = renderer.render(&config);
        assert!(result.is_err());
    }

    #[test]
    fn render_with_node_22() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(None, Some("22"), false, None);
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(output.contains("FROM ubuntu:24.04"));
        assert!(output.contains("nodesource"));
        assert!(output.contains("setup_22.x"));
        assert!(output.contains("nodejs"));
    }

    #[test]
    fn render_with_node_18() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(None, Some("18"), false, None);
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(output.contains("setup_18.x"));
        assert!(!output.contains("setup_22.x"));
    }

    #[test]
    fn render_with_node_20() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(None, Some("20"), false, None);
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(output.contains("setup_20.x"));
    }

    #[test]
    fn render_without_node_has_no_node_layer() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = Config::default();
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(!output.contains("nodesource"));
        assert!(!output.contains("nodejs"));
    }

    #[test]
    fn render_node_unsupported_version_errors() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(None, Some("16"), false, None);
        let result = renderer.render(&config);
        assert!(result.is_err());
    }

    #[test]
    fn render_with_php_and_node() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(Some("8.3"), Some("22"), false, None);
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(output.contains("FROM ubuntu:24.04"));
        assert!(output.contains("php8.3-cli"));
        assert!(output.contains("setup_22.x"));
        assert!(output.contains("nodejs"));

        let php_pos = output.find("php8.3-cli").unwrap();
        let node_pos = output.find("nodesource").unwrap();
        assert!(
            php_pos < node_pos,
            "PHP layer should come before Node layer"
        );
    }

    #[test]
    fn render_with_rust() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(None, None, true, None);
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(output.contains("FROM ubuntu:24.04"));
        assert!(output.contains("rustup.rs"));
        assert!(output.contains("CARGO_HOME"));
        assert!(output.contains("RUSTUP_HOME"));
        assert!(output.contains("/usr/local/cargo/bin"));
    }

    #[test]
    fn render_without_rust_has_no_rust_layer() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = Config::default();
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(!output.contains("rustup"));
        assert!(!output.contains("CARGO_HOME"));
    }

    #[test]
    fn render_with_node_and_rust_ordering() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(None, Some("22"), true, None);
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(output.contains("nodesource"));
        assert!(output.contains("rustup.rs"));

        let node_pos = output.find("nodesource").unwrap();
        let rust_pos = output.find("rustup.rs").unwrap();
        assert!(
            node_pos < rust_pos,
            "Node layer should come before Rust layer"
        );
    }

    #[test]
    fn render_with_php_node_and_rust_ordering() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(Some("8.3"), Some("22"), true, None);
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        let php_pos = output.find("php8.3-cli").unwrap();
        let node_pos = output.find("nodesource").unwrap();
        let rust_pos = output.find("rustup.rs").unwrap();
        assert!(php_pos < node_pos, "PHP before Node");
        assert!(node_pos < rust_pos, "Node before Rust");
    }

    #[test]
    fn render_with_go_123() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(None, None, false, Some("1.23"));
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(output.contains("FROM ubuntu:24.04"));
        assert!(output.contains("go1.23.linux-${ARCH}"));
        assert!(output.contains("go.dev"));
        assert!(output.contains("/usr/local/go/bin"));
        assert!(output.contains("uname -m"));
    }

    #[test]
    fn render_with_go_122() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(None, None, false, Some("1.22"));
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(output.contains("go1.22.linux-${ARCH}"));
        assert!(!output.contains("go1.23"));
    }

    #[test]
    fn render_without_go_has_no_go_layer() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = Config::default();
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(!output.contains("go.dev"));
        assert!(!output.contains("GOPATH"));
    }

    #[test]
    fn render_go_unsupported_version_errors() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(None, None, false, Some("1.21"));
        let result = renderer.render(&config);
        assert!(result.is_err());
    }

    #[test]
    fn render_go_architecture_aware() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(None, None, false, Some("1.23"));
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(output.contains("uname -m"));
        assert!(output.contains("amd64"));
        assert!(output.contains("arm64"));
    }

    #[test]
    fn render_with_rust_and_go_ordering() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(None, None, true, Some("1.23"));
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(output.contains("rustup.rs"));
        assert!(output.contains("go.dev"));

        let rust_pos = output.find("rustup.rs").unwrap();
        let go_pos = output.find("go.dev").unwrap();
        assert!(rust_pos < go_pos, "Rust layer should come before Go layer");
    }

    #[test]
    fn render_with_all_runtimes_ordering() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(Some("8.3"), Some("22"), true, Some("1.23"));
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        let php_pos = output.find("php8.3-cli").unwrap();
        let node_pos = output.find("nodesource").unwrap();
        let rust_pos = output.find("rustup.rs").unwrap();
        let go_pos = output.find("go.dev").unwrap();
        assert!(php_pos < node_pos, "PHP before Node");
        assert!(node_pos < rust_pos, "Node before Rust");
        assert!(rust_pos < go_pos, "Rust before Go");
    }

    #[test]
    fn content_hash_changes_with_runtime_addition() {
        let renderer = TemplateRenderer::new().unwrap();
        let config1 = config_with_runtimes(Some("8.3"), None, false, None);
        let config2 = config_with_runtimes(Some("8.3"), Some("22"), false, None);

        let result1 = renderer.render(&config1).unwrap();
        let result2 = renderer.render(&config2).unwrap();
        assert_ne!(
            result1.dockerfile, result2.dockerfile,
            "Adding a runtime should change the Dockerfile"
        );
    }

    #[test]
    fn content_hash_changes_with_version_change() {
        let renderer = TemplateRenderer::new().unwrap();
        let config1 = config_with_runtimes(Some("8.2"), None, false, None);
        let config2 = config_with_runtimes(Some("8.3"), None, false, None);

        let result1 = renderer.render(&config1).unwrap();
        let result2 = renderer.render(&config2).unwrap();
        assert_ne!(
            result1.dockerfile, result2.dockerfile,
            "Changing a version should change the Dockerfile"
        );
    }

    #[test]
    fn render_includes_entrypoint() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = Config::default();
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(output.contains("COPY entrypoint.sh /usr/local/bin/entrypoint.sh"));
        assert!(output.contains("ENTRYPOINT [\"/usr/local/bin/entrypoint.sh\"]"));
        assert!(output.contains("CMD [\"sleep\", \"infinity\"]"));
    }

    #[test]
    fn render_entrypoint_after_runtimes() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(Some("8.3"), None, false, None);
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        let php_pos = output.find("php8.3-cli").unwrap();
        let entrypoint_pos = output.find("ENTRYPOINT").unwrap();
        assert!(
            php_pos < entrypoint_pos,
            "Entrypoint should come after runtime layers"
        );
    }

    #[test]
    fn render_context_files_includes_entrypoint() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = Config::default();
        let result = renderer.render(&config).unwrap();

        assert_eq!(result.context_files.len(), 1);
        assert_eq!(result.context_files[0].path, "entrypoint.sh");
        assert!(result.context_files[0].content.contains("exec"));
        assert_eq!(result.context_files[0].mode, 0o755);
    }

    #[test]
    fn entrypoint_script_does_not_contain_secrets() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = Config::default();
        let result = renderer.render(&config).unwrap();

        let entrypoint = &result.context_files[0].content;
        // Credentials are written via docker exec after start, not in the entrypoint
        assert!(!entrypoint.contains("CLAUDE_CODE_OAUTH_TOKEN"));
        assert!(!entrypoint.contains("credentials"));
    }

    #[test]
    fn render_without_chief_has_no_chief_layer() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = Config::default();
        let result = renderer.render(&config).unwrap();
        let output = &result.dockerfile;

        assert!(!output.contains("chief"));
    }

    #[test]
    fn render_with_chief_downloads_binary() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = Config::default();
        let result = renderer.render_with_options(&config, true).unwrap();
        let output = &result.dockerfile;

        assert!(output.contains("MiniCodeMonkey/chief/releases"));
        assert!(output.contains("TARGETARCH"));
        assert!(output.contains("/usr/local/bin/chief"));
    }

    #[test]
    fn render_chief_layer_before_entrypoint() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = Config::default();
        let result = renderer.render_with_options(&config, true).unwrap();
        let output = &result.dockerfile;

        let chief_pos = output.find("MiniCodeMonkey/chief").unwrap();
        let entrypoint_pos = output.find("ENTRYPOINT").unwrap();
        assert!(
            chief_pos < entrypoint_pos,
            "Chief layer should come before entrypoint"
        );
    }

    #[test]
    fn render_chief_with_runtimes() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = config_with_runtimes(Some("8.3"), Some("22"), false, None);
        let result = renderer.render_with_options(&config, true).unwrap();
        let output = &result.dockerfile;

        // All layers present
        assert!(output.contains("php8.3-cli"));
        assert!(output.contains("nodesource"));
        assert!(output.contains("MiniCodeMonkey/chief"));

        // Correct ordering: runtimes before chief before entrypoint
        let php_pos = output.find("php8.3-cli").unwrap();
        let node_pos = output.find("nodesource").unwrap();
        let chief_pos = output.find("MiniCodeMonkey/chief").unwrap();
        let entrypoint_pos = output.find("ENTRYPOINT").unwrap();
        assert!(php_pos < node_pos, "PHP before Node");
        assert!(node_pos < chief_pos, "Node before Chief");
        assert!(chief_pos < entrypoint_pos, "Chief before entrypoint");
    }

    #[test]
    fn render_chief_changes_content_hash() {
        let renderer = TemplateRenderer::new().unwrap();
        let config = Config::default();

        let without_chief = renderer.render(&config).unwrap();
        let with_chief = renderer.render_with_options(&config, true).unwrap();
        assert_ne!(
            without_chief.dockerfile, with_chief.dockerfile,
            "Chief layer should change the Dockerfile"
        );
    }
}
