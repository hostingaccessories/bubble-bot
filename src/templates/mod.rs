use anyhow::Result;
use minijinja::{Environment, context};

use crate::config::Config;

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
    /// runtime/service templates. Currently only renders the base layer;
    /// runtime templates will be added in future stories.
    pub fn render(&self, _params: &TemplateParams) -> Result<String> {
        let tmpl = self.env.get_template("base")?;
        let rendered = tmpl.render(context! {})?;
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
}
