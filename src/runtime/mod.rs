pub mod go;
pub mod node;
pub mod php;
pub mod rust;

use anyhow::Result;
use minijinja::value::Value;

use crate::config::Config;

pub trait Runtime {
    fn name(&self) -> &str;
    fn template(&self) -> &str;
    /// Returns the minijinja context values for rendering this runtime's template.
    fn template_context(&self) -> Value;
}

/// Builds an ordered list of active runtimes from the resolved config.
///
/// Runtimes are always returned in deterministic order: PHP, Node, Rust, Go.
/// This ordering ensures the composed Dockerfile is identical given the same inputs.
pub fn collect_runtimes(config: &Config) -> Result<Vec<Box<dyn Runtime>>> {
    let mut runtimes: Vec<Box<dyn Runtime>> = Vec::new();

    if let Some(ref version) = config.runtimes.php {
        runtimes.push(Box::new(php::PhpRuntime::new(version)?));
    }

    if let Some(ref version) = config.runtimes.node {
        runtimes.push(Box::new(node::NodeRuntime::new(version)?));
    }

    if config.runtimes.rust.unwrap_or(false) {
        runtimes.push(Box::new(rust::RustRuntime::new()));
    }

    if let Some(ref version) = config.runtimes.go {
        runtimes.push(Box::new(go::GoRuntime::new(version)?));
    }

    Ok(runtimes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_no_runtimes_from_default_config() {
        let config = Config::default();
        let runtimes = collect_runtimes(&config).unwrap();
        assert!(runtimes.is_empty());
    }

    #[test]
    fn collect_single_runtime() {
        let mut config = Config::default();
        config.runtimes.php = Some("8.3".to_string());
        let runtimes = collect_runtimes(&config).unwrap();
        assert_eq!(runtimes.len(), 1);
        assert_eq!(runtimes[0].name(), "php");
    }

    #[test]
    fn collect_all_runtimes_in_order() {
        let mut config = Config::default();
        config.runtimes.php = Some("8.3".to_string());
        config.runtimes.node = Some("22".to_string());
        config.runtimes.rust = Some(true);
        config.runtimes.go = Some("1.23".to_string());

        let runtimes = collect_runtimes(&config).unwrap();
        assert_eq!(runtimes.len(), 4);
        assert_eq!(runtimes[0].name(), "php");
        assert_eq!(runtimes[1].name(), "node");
        assert_eq!(runtimes[2].name(), "rust");
        assert_eq!(runtimes[3].name(), "go");
    }

    #[test]
    fn collect_subset_preserves_order() {
        let mut config = Config::default();
        config.runtimes.node = Some("22".to_string());
        config.runtimes.go = Some("1.23".to_string());

        let runtimes = collect_runtimes(&config).unwrap();
        assert_eq!(runtimes.len(), 2);
        assert_eq!(runtimes[0].name(), "node");
        assert_eq!(runtimes[1].name(), "go");
    }

    #[test]
    fn collect_invalid_version_errors() {
        let mut config = Config::default();
        config.runtimes.php = Some("7.4".to_string());
        let result = collect_runtimes(&config);
        assert!(result.is_err());
    }

    #[test]
    fn collect_rust_false_is_skipped() {
        let mut config = Config::default();
        config.runtimes.rust = Some(false);
        let runtimes = collect_runtimes(&config).unwrap();
        assert!(runtimes.is_empty());
    }
}
