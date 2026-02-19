use tracing::debug;

/// Dotfiles to consider mounting when `mount_configs = true`.
const DOTFILES: &[&str] = &[
    ".zshrc",
    ".bashrc",
    ".bash_profile",
    ".profile",
    ".aliases",
    ".inputrc",
    ".vimrc",
    ".gitconfig",
    ".tmux.conf",
];

/// Detects the user's shell from the `$SHELL` environment variable.
/// Returns the shell name (e.g., "zsh", "bash") or "bash" as fallback.
pub fn detect_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .and_then(|s| {
            std::path::Path::new(&s)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "bash".to_string())
}

/// Returns a list of read-only bind mount strings for dotfiles that exist
/// on the host. Each entry is in the format `host_path:/home/dev/filename:ro`.
pub fn collect_dotfile_mounts() -> Vec<String> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => {
            debug!("could not determine home directory, skipping dotfile mounts");
            return Vec::new();
        }
    };

    let mut mounts = Vec::new();
    for dotfile in DOTFILES {
        let host_path = home.join(dotfile);
        if host_path.exists() {
            let mount = format!(
                "{}:/home/dev/{}:ro",
                host_path.display(),
                dotfile
            );
            debug!(dotfile, "mounting dotfile");
            mounts.push(mount);
        }
    }

    mounts
}

/// Resolves the shell to use for the container.
/// Priority: config value > detected shell > "bash" fallback.
pub fn resolve_shell(config_shell: Option<&str>) -> String {
    config_shell
        .map(|s| s.to_string())
        .unwrap_or_else(detect_shell)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn detect_shell_returns_string() {
        let shell = detect_shell();
        assert!(!shell.is_empty());
    }

    #[test]
    fn detect_shell_extracts_basename() {
        // If SHELL is set (which it typically is in CI and dev), it should
        // return just the basename like "zsh" or "bash", not the full path
        let shell = detect_shell();
        assert!(!shell.contains('/'));
    }

    #[test]
    fn resolve_shell_prefers_config() {
        let shell = resolve_shell(Some("fish"));
        assert_eq!(shell, "fish");
    }

    #[test]
    fn resolve_shell_falls_back_to_detected() {
        let shell = resolve_shell(None);
        // Should return detected shell (non-empty)
        assert!(!shell.is_empty());
    }

    #[test]
    fn collect_dotfile_mounts_returns_valid_format() {
        let mounts = collect_dotfile_mounts();
        for mount in &mounts {
            assert!(mount.contains(":/home/dev/"));
            assert!(mount.ends_with(":ro"));
        }
    }

    #[test]
    fn collect_dotfile_mounts_only_existing_files() {
        let mounts = collect_dotfile_mounts();
        // Every mount should reference a file that exists
        for mount in &mounts {
            let host_path = mount.split(':').next().unwrap();
            assert!(
                PathBuf::from(host_path).exists(),
                "mounted file should exist: {}",
                host_path
            );
        }
    }

    #[test]
    fn dotfiles_list_contains_common_files() {
        assert!(DOTFILES.contains(&".zshrc"));
        assert!(DOTFILES.contains(&".bashrc"));
        assert!(DOTFILES.contains(&".aliases"));
        assert!(DOTFILES.contains(&".gitconfig"));
    }
}
