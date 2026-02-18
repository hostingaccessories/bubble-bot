use minijinja::{context, value::Value};

use super::Runtime;

#[derive(Debug)]
pub struct RustRuntime;

impl RustRuntime {
    pub fn new() -> Self {
        Self
    }
}

impl Runtime for RustRuntime {
    fn name(&self) -> &str {
        "rust"
    }

    fn template(&self) -> &str {
        include_str!("../templates/rust.dockerfile")
    }

    fn template_context(&self) -> Value {
        context! {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_runtime() {
        let rt = RustRuntime::new();
        assert_eq!(rt.name(), "rust");
    }

    #[test]
    fn template_contains_rustup() {
        let rt = RustRuntime::new();
        let tmpl = rt.template();
        assert!(tmpl.contains("rustup.rs"));
        assert!(tmpl.contains("CARGO_HOME"));
        assert!(tmpl.contains("RUSTUP_HOME"));
        assert!(tmpl.contains("/usr/local/cargo/bin"));
    }
}
