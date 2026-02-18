use super::Runtime;

/// Supported Node.js versions.
const SUPPORTED_VERSIONS: &[&str] = &["18", "20", "22"];

#[derive(Debug)]
pub struct NodeRuntime {
    pub version: String,
}

impl NodeRuntime {
    pub fn new(version: &str) -> anyhow::Result<Self> {
        if !SUPPORTED_VERSIONS.contains(&version) {
            anyhow::bail!(
                "unsupported Node.js version '{}': supported versions are {}",
                version,
                SUPPORTED_VERSIONS.join(", ")
            );
        }
        Ok(Self {
            version: version.to_string(),
        })
    }
}

impl Runtime for NodeRuntime {
    fn name(&self) -> &str {
        "node"
    }

    fn template(&self) -> &str {
        include_str!("../templates/node.dockerfile")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_versions() {
        for v in SUPPORTED_VERSIONS {
            let rt = NodeRuntime::new(v).unwrap();
            assert_eq!(rt.version, *v);
            assert_eq!(rt.name(), "node");
        }
    }

    #[test]
    fn unsupported_version_errors() {
        let result = NodeRuntime::new("16");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("unsupported Node.js version"));
        assert!(msg.contains("16"));
    }

    #[test]
    fn template_contains_node_placeholder() {
        let rt = NodeRuntime::new("22").unwrap();
        let tmpl = rt.template();
        assert!(tmpl.contains("{{ node_version }}"));
        assert!(tmpl.contains("nodesource"));
        assert!(tmpl.contains("nodejs"));
    }
}
