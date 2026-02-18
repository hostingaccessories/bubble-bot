use minijinja::{context, value::Value};

use super::Runtime;

/// Supported PHP versions.
const SUPPORTED_VERSIONS: &[&str] = &["8.1", "8.2", "8.3"];

#[derive(Debug)]
pub struct PhpRuntime {
    pub version: String,
}

impl PhpRuntime {
    pub fn new(version: &str) -> anyhow::Result<Self> {
        if !SUPPORTED_VERSIONS.contains(&version) {
            anyhow::bail!(
                "unsupported PHP version '{}': supported versions are {}",
                version,
                SUPPORTED_VERSIONS.join(", ")
            );
        }
        Ok(Self {
            version: version.to_string(),
        })
    }
}

impl Runtime for PhpRuntime {
    fn name(&self) -> &str {
        "php"
    }

    fn template(&self) -> &str {
        include_str!("../templates/php.dockerfile")
    }

    fn template_context(&self) -> Value {
        context! { php_version => &self.version }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_versions() {
        for v in SUPPORTED_VERSIONS {
            let rt = PhpRuntime::new(v).unwrap();
            assert_eq!(rt.version, *v);
            assert_eq!(rt.name(), "php");
        }
    }

    #[test]
    fn unsupported_version_errors() {
        let result = PhpRuntime::new("7.4");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("unsupported PHP version"));
        assert!(msg.contains("7.4"));
    }

    #[test]
    fn template_contains_php_placeholder() {
        let rt = PhpRuntime::new("8.3").unwrap();
        let tmpl = rt.template();
        assert!(tmpl.contains("{{ php_version }}"));
        assert!(tmpl.contains("composer"));
        assert!(tmpl.contains("ppa:ondrej/php"));
    }
}
