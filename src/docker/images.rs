use anyhow::{Context, Result};
use bollard::Docker;
use bollard::image::{BuildImageOptions, ListImagesOptions};
use sha2::{Digest, Sha256};
use tracing::info;

use crate::templates::ContextFile;

/// Builds Docker images with content-hash caching.
///
/// The rendered Dockerfile is SHA-256 hashed (first 12 chars) and used as the
/// image tag. If an image with that tag already exists, the build is skipped
/// unless `no_cache` is set.
pub struct ImageBuilder {
    docker: Docker,
}

/// Result of an image build or cache lookup.
#[derive(Debug)]
pub struct BuildResult {
    pub tag: String,
    pub cached: bool,
}

impl ImageBuilder {
    pub fn new(docker: Docker) -> Self {
        Self { docker }
    }

    /// Computes the content-hash tag for a rendered Dockerfile.
    /// Returns `bubble-boy:<first-12-chars-of-sha256>`.
    pub fn compute_tag(dockerfile_content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(dockerfile_content.as_bytes());
        let hash = hasher.finalize();
        let hex = format!("{hash:x}");
        let prefix = &hex[..12];
        format!("bubble-boy:{prefix}")
    }

    /// Checks whether an image with the given tag already exists locally.
    pub async fn image_exists(&self, tag: &str) -> Result<bool> {
        let filters: std::collections::HashMap<String, Vec<String>> =
            [("reference".to_string(), vec![tag.to_string()])]
                .into_iter()
                .collect();

        let images = self
            .docker
            .list_images(Some(ListImagesOptions {
                filters,
                ..Default::default()
            }))
            .await
            .context("failed to list Docker images")?;

        Ok(!images.is_empty())
    }

    /// Builds an image from the given Dockerfile content, or returns a cached
    /// result if the image already exists.
    ///
    /// - `dockerfile_content`: the fully rendered Dockerfile string
    /// - `context_files`: additional files to include in the build context
    /// - `no_cache`: if true, forces a rebuild even if the image tag exists
    pub async fn build(
        &self,
        dockerfile_content: &str,
        context_files: &[ContextFile],
        no_cache: bool,
    ) -> Result<BuildResult> {
        let tag = Self::compute_tag(dockerfile_content);

        // Check cache unless --no-cache
        if !no_cache && self.image_exists(&tag).await? {
            info!(tag = %tag, "image cache hit — skipping build");
            return Ok(BuildResult {
                tag,
                cached: true,
            });
        }

        info!(tag = %tag, "building image");

        // Create a tar archive with the Dockerfile and context files
        let tar_bytes = Self::create_build_context(dockerfile_content, context_files)?;

        let options = BuildImageOptions {
            t: tag.clone(),
            rm: true,
            forcerm: true,
            ..Default::default()
        };

        use futures_util::StreamExt;

        let mut stream = self.docker.build_image(
            options,
            None,
            Some(tar_bytes.into()),
        );

        while let Some(result) = stream.next().await {
            match result {
                Ok(output) => {
                    if let Some(stream_msg) = &output.stream {
                        let trimmed = stream_msg.trim_end();
                        if !trimmed.is_empty() {
                            info!("{}", trimmed);
                        }
                    }
                    if let Some(error) = &output.error {
                        anyhow::bail!("Docker build error: {error}");
                    }
                }
                Err(e) => {
                    anyhow::bail!("Docker build stream error: {e}");
                }
            }
        }

        info!(tag = %tag, "image build complete");

        Ok(BuildResult {
            tag,
            cached: false,
        })
    }

    /// Creates an in-memory tar archive containing the Dockerfile and any
    /// additional context files (e.g., entrypoint.sh).
    fn create_build_context(
        dockerfile_content: &str,
        context_files: &[ContextFile],
    ) -> Result<Vec<u8>> {
        let mut archive = tar::Builder::new(Vec::new());

        let dockerfile_bytes = dockerfile_content.as_bytes();
        let mut header = tar::Header::new_gnu();
        header.set_path("Dockerfile")?;
        header.set_size(dockerfile_bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();

        archive.append(&header, dockerfile_bytes)?;

        for file in context_files {
            let bytes = file.content.as_bytes();
            let mut file_header = tar::Header::new_gnu();
            file_header.set_path(&file.path)?;
            file_header.set_size(bytes.len() as u64);
            file_header.set_mode(file.mode);
            file_header.set_cksum();
            archive.append(&file_header, bytes)?;
        }

        archive.finish()?;

        Ok(archive.into_inner()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_tag_uses_first_12_hex_chars() {
        let tag = ImageBuilder::compute_tag("FROM ubuntu:24.04\n");
        // Tag format: bubble-boy:<12-hex-chars>
        assert!(tag.starts_with("bubble-boy:"));
        let hash_part = tag.strip_prefix("bubble-boy:").unwrap();
        assert_eq!(hash_part.len(), 12);
        assert!(hash_part.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn compute_tag_is_deterministic() {
        let content = "FROM ubuntu:24.04\nRUN apt-get update\n";
        let tag1 = ImageBuilder::compute_tag(content);
        let tag2 = ImageBuilder::compute_tag(content);
        assert_eq!(tag1, tag2);
    }

    #[test]
    fn compute_tag_changes_with_content() {
        let tag1 = ImageBuilder::compute_tag("FROM ubuntu:24.04\n");
        let tag2 = ImageBuilder::compute_tag("FROM ubuntu:22.04\n");
        assert_ne!(tag1, tag2);
    }

    #[test]
    fn create_build_context_produces_valid_tar() {
        let content = "FROM ubuntu:24.04\nRUN echo hello\n";
        let tar_bytes = ImageBuilder::create_build_context(content, &[]).unwrap();

        // Verify the tar contains a Dockerfile entry
        let mut archive = tar::Archive::new(tar_bytes.as_slice());
        let entries: Vec<_> = archive.entries().unwrap().collect();
        assert_eq!(entries.len(), 1);

        let entry = entries.into_iter().next().unwrap().unwrap();
        assert_eq!(
            entry.path().unwrap().to_str().unwrap(),
            "Dockerfile"
        );
    }

    #[test]
    fn create_build_context_content_matches() {
        use std::io::Read;

        let content = "FROM ubuntu:24.04\nRUN echo hello\n";
        let tar_bytes = ImageBuilder::create_build_context(content, &[]).unwrap();

        let mut archive = tar::Archive::new(tar_bytes.as_slice());
        let mut entry = archive.entries().unwrap().next().unwrap().unwrap();
        let mut buf = String::new();
        entry.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, content);
    }

    #[test]
    fn create_build_context_includes_extra_files() {
        let content = "FROM ubuntu:24.04\n";
        let context_files = vec![ContextFile {
            path: "entrypoint.sh".to_string(),
            content: "#!/bin/bash\nexec \"$@\"\n".to_string(),
            mode: 0o755,
        }];
        let tar_bytes = ImageBuilder::create_build_context(content, &context_files).unwrap();

        let mut archive = tar::Archive::new(tar_bytes.as_slice());
        let entries: Vec<_> = archive.entries().unwrap().collect();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn create_build_context_extra_file_content_matches() {
        use std::io::Read;

        let content = "FROM ubuntu:24.04\n";
        let script = "#!/bin/bash\nexec \"$@\"\n";
        let context_files = vec![ContextFile {
            path: "entrypoint.sh".to_string(),
            content: script.to_string(),
            mode: 0o755,
        }];
        let tar_bytes = ImageBuilder::create_build_context(content, &context_files).unwrap();

        let mut archive = tar::Archive::new(tar_bytes.as_slice());
        let mut entries = archive.entries().unwrap();
        // Skip Dockerfile
        entries.next();
        let mut entry = entries.next().unwrap().unwrap();
        assert_eq!(entry.path().unwrap().to_str().unwrap(), "entrypoint.sh");
        let mut buf = String::new();
        entry.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, script);
    }
}
