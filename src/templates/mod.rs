use anyhow::Result;
use minijinja::{Environment, context};

use crate::config::Config;
use crate::runtime::Runtime;
use crate::runtime::node::NodeRuntime;
use crate::runtime::php::PhpRuntime;
use crate::runtime::go::GoRuntime;
use crate::runtime::rust::RustRuntime;

static BASE_TEMPLATE: &str = include_str!("base.dockerfile");

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

    /// Renders the full Dockerfile by composing the base template with any
    /// runtime templates based on the provided parameters.
    pub fn render(&self, params: &TemplateParams) -> Result<String> {
        let tmpl = self.env.get_template("base")?;
        let mut rendered = tmpl.render(context! {})?;

        // Compose runtime layers in deterministic order: PHP, Node, Rust, Go
        if let Some(ref version) = params.php_version {
            let php = PhpRuntime::new(version)?;
            let mut rt_env = Environment::new();
            rt_env.add_template("php", php.template())?;
            let rt_tmpl = rt_env.get_template("php")?;
            let layer = rt_tmpl.render(context! { php_version => version })?;
            rendered.push('\n');
            rendered.push_str(&layer);
        }

        if let Some(ref version) = params.node_version {
            let node = NodeRuntime::new(version)?;
            let mut rt_env = Environment::new();
            rt_env.add_template("node", node.template())?;
            let rt_tmpl = rt_env.get_template("node")?;
            let layer = rt_tmpl.render(context! { node_version => version })?;
            rendered.push('\n');
            rendered.push_str(&layer);
        }

        if params.rust_enabled {
            let rust = RustRuntime::new();
            let mut rt_env = Environment::new();
            rt_env.add_template("rust", rust.template())?;
            let rt_tmpl = rt_env.get_template("rust")?;
            let layer = rt_tmpl.render(context! {})?;
            rendered.push('\n');
            rendered.push_str(&layer);
        }

        if let Some(ref version) = params.go_version {
            let go = GoRuntime::new(version)?;
            let mut rt_env = Environment::new();
            rt_env.add_template("go", go.template())?;
            let rt_tmpl = rt_env.get_template("go")?;
            let layer = rt_tmpl.render(context! { go_version => version })?;
            rendered.push('\n');
            rendered.push_str(&layer);
        }

        Ok(rendered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_base_template() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: None,
            node_version: None,
            rust_enabled: false,
            go_version: None,
        };
        let output = renderer.render(&params).unwrap();

        assert!(output.contains("FROM ubuntu:24.04"));
        assert!(output.contains("git"));
        assert!(output.contains("curl"));
        assert!(output.contains("wget"));
        assert!(output.contains("unzip"));
        assert!(output.contains("build-essential"));
        assert!(output.contains("ca-certificates"));
        assert!(output.contains("mkdir -p /home/dev"));
        assert!(output.contains("chmod 777 /home/dev"));
        assert!(output.contains("WORKDIR /workspace"));
    }

    #[test]
    fn render_is_deterministic() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: Some("8.3".to_string()),
            node_version: Some("22".to_string()),
            rust_enabled: true,
            go_version: Some("1.23".to_string()),
        };

        let output1 = renderer.render(&params).unwrap();
        let output2 = renderer.render(&params).unwrap();
        assert_eq!(output1, output2);
    }

    #[test]
    fn base_template_uses_ubuntu_2404() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: None,
            node_version: None,
            rust_enabled: false,
            go_version: None,
        };
        let output = renderer.render(&params).unwrap();
        let first_line = output.lines().next().unwrap();
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
        let mut config = Config::default();
        config.runtimes.php = Some("8.3".to_string());
        config.runtimes.node = Some("22".to_string());
        config.runtimes.rust = Some(true);
        config.runtimes.go = Some("1.23".to_string());

        let params = TemplateParams::from_config(&config);
        assert_eq!(params.php_version.as_deref(), Some("8.3"));
        assert_eq!(params.node_version.as_deref(), Some("22"));
        assert!(params.rust_enabled);
        assert_eq!(params.go_version.as_deref(), Some("1.23"));
    }

    #[test]
    fn render_with_php_83() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: Some("8.3".to_string()),
            node_version: None,
            rust_enabled: false,
            go_version: None,
        };
        let output = renderer.render(&params).unwrap();

        // Base template still present
        assert!(output.contains("FROM ubuntu:24.04"));
        // PHP layer appended
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
        let params = TemplateParams {
            php_version: Some("8.1".to_string()),
            node_version: None,
            rust_enabled: false,
            go_version: None,
        };
        let output = renderer.render(&params).unwrap();

        assert!(output.contains("php8.1-cli"));
        assert!(output.contains("php8.1-mbstring"));
        assert!(!output.contains("php8.3-cli"));
    }

    #[test]
    fn render_with_php_82() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: Some("8.2".to_string()),
            node_version: None,
            rust_enabled: false,
            go_version: None,
        };
        let output = renderer.render(&params).unwrap();

        assert!(output.contains("php8.2-cli"));
        assert!(output.contains("php8.2-redis"));
    }

    #[test]
    fn render_without_php_has_no_php_layer() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: None,
            node_version: None,
            rust_enabled: false,
            go_version: None,
        };
        let output = renderer.render(&params).unwrap();

        assert!(!output.contains("php"));
        assert!(!output.contains("composer"));
    }

    #[test]
    fn render_php_is_deterministic() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: Some("8.3".to_string()),
            node_version: None,
            rust_enabled: false,
            go_version: None,
        };
        let output1 = renderer.render(&params).unwrap();
        let output2 = renderer.render(&params).unwrap();
        assert_eq!(output1, output2);
    }

    #[test]
    fn render_php_unsupported_version_errors() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: Some("7.4".to_string()),
            node_version: None,
            rust_enabled: false,
            go_version: None,
        };
        let result = renderer.render(&params);
        assert!(result.is_err());
    }

    #[test]
    fn render_with_node_22() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: None,
            node_version: Some("22".to_string()),
            rust_enabled: false,
            go_version: None,
        };
        let output = renderer.render(&params).unwrap();

        assert!(output.contains("FROM ubuntu:24.04"));
        assert!(output.contains("nodesource"));
        assert!(output.contains("setup_22.x"));
        assert!(output.contains("nodejs"));
    }

    #[test]
    fn render_with_node_18() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: None,
            node_version: Some("18".to_string()),
            rust_enabled: false,
            go_version: None,
        };
        let output = renderer.render(&params).unwrap();

        assert!(output.contains("setup_18.x"));
        assert!(!output.contains("setup_22.x"));
    }

    #[test]
    fn render_with_node_20() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: None,
            node_version: Some("20".to_string()),
            rust_enabled: false,
            go_version: None,
        };
        let output = renderer.render(&params).unwrap();

        assert!(output.contains("setup_20.x"));
    }

    #[test]
    fn render_without_node_has_no_node_layer() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: None,
            node_version: None,
            rust_enabled: false,
            go_version: None,
        };
        let output = renderer.render(&params).unwrap();

        assert!(!output.contains("nodesource"));
        assert!(!output.contains("nodejs"));
    }

    #[test]
    fn render_node_unsupported_version_errors() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: None,
            node_version: Some("16".to_string()),
            rust_enabled: false,
            go_version: None,
        };
        let result = renderer.render(&params);
        assert!(result.is_err());
    }

    #[test]
    fn render_with_php_and_node() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: Some("8.3".to_string()),
            node_version: Some("22".to_string()),
            rust_enabled: false,
            go_version: None,
        };
        let output = renderer.render(&params).unwrap();

        // Base present
        assert!(output.contains("FROM ubuntu:24.04"));
        // PHP layer present
        assert!(output.contains("php8.3-cli"));
        // Node layer present
        assert!(output.contains("setup_22.x"));
        assert!(output.contains("nodejs"));

        // PHP comes before Node (deterministic order)
        let php_pos = output.find("php8.3-cli").unwrap();
        let node_pos = output.find("nodesource").unwrap();
        assert!(php_pos < node_pos, "PHP layer should come before Node layer");
    }

    #[test]
    fn render_with_rust() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: None,
            node_version: None,
            rust_enabled: true,
            go_version: None,
        };
        let output = renderer.render(&params).unwrap();

        assert!(output.contains("FROM ubuntu:24.04"));
        assert!(output.contains("rustup.rs"));
        assert!(output.contains("CARGO_HOME"));
        assert!(output.contains("RUSTUP_HOME"));
        assert!(output.contains("/usr/local/cargo/bin"));
    }

    #[test]
    fn render_without_rust_has_no_rust_layer() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: None,
            node_version: None,
            rust_enabled: false,
            go_version: None,
        };
        let output = renderer.render(&params).unwrap();

        assert!(!output.contains("rustup"));
        assert!(!output.contains("CARGO_HOME"));
    }

    #[test]
    fn render_with_node_and_rust_ordering() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: None,
            node_version: Some("22".to_string()),
            rust_enabled: true,
            go_version: None,
        };
        let output = renderer.render(&params).unwrap();

        // Both present
        assert!(output.contains("nodesource"));
        assert!(output.contains("rustup.rs"));

        // Node comes before Rust (deterministic order: PHP, Node, Rust, Go)
        let node_pos = output.find("nodesource").unwrap();
        let rust_pos = output.find("rustup.rs").unwrap();
        assert!(node_pos < rust_pos, "Node layer should come before Rust layer");
    }

    #[test]
    fn render_with_php_node_and_rust_ordering() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: Some("8.3".to_string()),
            node_version: Some("22".to_string()),
            rust_enabled: true,
            go_version: None,
        };
        let output = renderer.render(&params).unwrap();

        let php_pos = output.find("php8.3-cli").unwrap();
        let node_pos = output.find("nodesource").unwrap();
        let rust_pos = output.find("rustup.rs").unwrap();
        assert!(php_pos < node_pos, "PHP before Node");
        assert!(node_pos < rust_pos, "Node before Rust");
    }

    #[test]
    fn render_with_go_123() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: None,
            node_version: None,
            rust_enabled: false,
            go_version: Some("1.23".to_string()),
        };
        let output = renderer.render(&params).unwrap();

        assert!(output.contains("FROM ubuntu:24.04"));
        assert!(output.contains("go1.23.linux-${ARCH}"));
        assert!(output.contains("go.dev"));
        assert!(output.contains("/usr/local/go/bin"));
        assert!(output.contains("uname -m"));
    }

    #[test]
    fn render_with_go_122() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: None,
            node_version: None,
            rust_enabled: false,
            go_version: Some("1.22".to_string()),
        };
        let output = renderer.render(&params).unwrap();

        assert!(output.contains("go1.22.linux-${ARCH}"));
        assert!(!output.contains("go1.23"));
    }

    #[test]
    fn render_without_go_has_no_go_layer() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: None,
            node_version: None,
            rust_enabled: false,
            go_version: None,
        };
        let output = renderer.render(&params).unwrap();

        assert!(!output.contains("go.dev"));
        assert!(!output.contains("GOPATH"));
    }

    #[test]
    fn render_go_unsupported_version_errors() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: None,
            node_version: None,
            rust_enabled: false,
            go_version: Some("1.21".to_string()),
        };
        let result = renderer.render(&params);
        assert!(result.is_err());
    }

    #[test]
    fn render_go_architecture_aware() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: None,
            node_version: None,
            rust_enabled: false,
            go_version: Some("1.23".to_string()),
        };
        let output = renderer.render(&params).unwrap();

        // Verify architecture detection is in the template
        assert!(output.contains("uname -m"));
        assert!(output.contains("amd64"));
        assert!(output.contains("arm64"));
    }

    #[test]
    fn render_with_rust_and_go_ordering() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: None,
            node_version: None,
            rust_enabled: true,
            go_version: Some("1.23".to_string()),
        };
        let output = renderer.render(&params).unwrap();

        assert!(output.contains("rustup.rs"));
        assert!(output.contains("go.dev"));

        // Rust comes before Go (deterministic order: PHP, Node, Rust, Go)
        let rust_pos = output.find("rustup.rs").unwrap();
        let go_pos = output.find("go.dev").unwrap();
        assert!(rust_pos < go_pos, "Rust layer should come before Go layer");
    }

    #[test]
    fn render_with_all_runtimes_ordering() {
        let renderer = TemplateRenderer::new().unwrap();
        let params = TemplateParams {
            php_version: Some("8.3".to_string()),
            node_version: Some("22".to_string()),
            rust_enabled: true,
            go_version: Some("1.23".to_string()),
        };
        let output = renderer.render(&params).unwrap();

        let php_pos = output.find("php8.3-cli").unwrap();
        let node_pos = output.find("nodesource").unwrap();
        let rust_pos = output.find("rustup.rs").unwrap();
        let go_pos = output.find("go.dev").unwrap();
        assert!(php_pos < node_pos, "PHP before Node");
        assert!(node_pos < rust_pos, "Node before Rust");
        assert!(rust_pos < go_pos, "Rust before Go");
    }
}
