use super::Runtime;

/// Supported Go versions.
const SUPPORTED_VERSIONS: &[&str] = &["1.22", "1.23"];

#[derive(Debug)]
pub struct GoRuntime {
    pub version: String,
}

impl GoRuntime {
    pub fn new(version: &str) -> anyhow::Result<Self> {
        if !SUPPORTED_VERSIONS.contains(&version) {
            anyhow::bail!(
                "unsupported Go version '{}': supported versions are {}",
                version,
                SUPPORTED_VERSIONS.join(", ")
            );
        }
        Ok(Self {
            version: version.to_string(),
        })
    }
}

impl Runtime for GoRuntime {
    fn name(&self) -> &str {
        "go"
    }

    fn template(&self) -> &str {
        include_str!("../templates/go.dockerfile")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_versions() {
        for v in SUPPORTED_VERSIONS {
            let rt = GoRuntime::new(v).unwrap();
            assert_eq!(rt.version, *v);
            assert_eq!(rt.name(), "go");
        }
    }

    #[test]
    fn unsupported_version_errors() {
        let result = GoRuntime::new("1.21");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("unsupported Go version"));
        assert!(msg.contains("1.21"));
    }

    #[test]
    fn template_contains_go_placeholder() {
        let rt = GoRuntime::new("1.23").unwrap();
        let tmpl = rt.template();
        assert!(tmpl.contains("{{ go_version }}"));
        assert!(tmpl.contains("go.dev"));
        assert!(tmpl.contains("uname -m"));
        assert!(tmpl.contains("/usr/local/go/bin"));
    }
}
