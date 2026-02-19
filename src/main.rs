#![allow(dead_code)]

mod auth;
mod cli;
mod config;
mod docker;
mod hooks;
mod runtime;
mod services;
mod shell;
mod templates;

use anyhow::Result;
use bollard::Docker;
use clap::Parser;
use tracing::info;

use auth::resolve_oauth_token;
use cli::{Cli, Command};
use config::Config;
use docker::containers::{ContainerManager, ContainerOpts, default_container_name};
use docker::images::ImageBuilder;
use templates::TemplateRenderer;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let config = Config::load(&cli)?;
    let command = cli.command();

    match command {
        Command::Shell => run_shell(&cli, &config).await,
        Command::Claude { args } => run_claude(&cli, &config, &args).await,
        _ => {
            info!("subcommand not yet implemented");
            Ok(())
        }
    }
}

async fn run_claude(cli: &Cli, config: &Config, args: &[String]) -> Result<()> {
    let docker = Docker::connect_with_local_defaults()
        .map_err(|e| anyhow::anyhow!("failed to connect to Docker: {e}"))?;

    // Resolve container name
    let container_name = config
        .container
        .name
        .clone()
        .unwrap_or_else(default_container_name);

    // Render Dockerfile
    let renderer = TemplateRenderer::new()?;
    let render_result = renderer.render(config)?;

    // Build or use cached image
    let image_builder = ImageBuilder::new(docker.clone());
    let build_result = image_builder
        .build(
            &render_result.dockerfile,
            &render_result.context_files,
            cli.container.no_cache,
        )
        .await?;
    info!(tag = %build_result.tag, cached = build_result.cached, "image ready");

    // Get project directory
    let project_dir = std::env::current_dir()?
        .to_string_lossy()
        .to_string();

    // Resolve auth token
    let mut env_vars = Vec::new();
    if let Some(token) = resolve_oauth_token()? {
        env_vars.push(format!("CLAUDE_CODE_OAUTH_TOKEN={token}"));
    }

    // Container lifecycle
    let container_mgr = ContainerManager::new(docker);

    // Clean up any existing container with the same name
    container_mgr.cleanup_existing(&container_name).await?;

    let opts = ContainerOpts {
        image_tag: build_result.tag,
        container_name: container_name.clone(),
        shell: "zsh".to_string(),
        project_dir,
        env_vars,
    };

    let container_id = container_mgr.create_and_start(&opts).await?;

    // Build Claude Code command
    let mut cmd: Vec<&str> = vec!["claude", "--permission-mode", "bypassPermissions"];
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    cmd.extend(&arg_refs);

    // Launch Claude Code (blocking)
    let exit_code = container_mgr.exec_interactive_command(&container_id, &cmd)?;

    // Cleanup on exit
    container_mgr.stop_and_remove(&container_id).await?;

    if exit_code != 0 {
        std::process::exit(exit_code);
    }

    Ok(())
}

async fn run_shell(cli: &Cli, config: &Config) -> Result<()> {
    let docker = Docker::connect_with_local_defaults()
        .map_err(|e| anyhow::anyhow!("failed to connect to Docker: {e}"))?;

    // Resolve container name
    let container_name = config
        .container
        .name
        .clone()
        .unwrap_or_else(default_container_name);

    // Resolve shell
    let shell = config
        .container
        .shell
        .clone()
        .unwrap_or_else(|| "zsh".to_string());

    // Render Dockerfile
    let renderer = TemplateRenderer::new()?;
    let render_result = renderer.render(config)?;

    // Build or use cached image
    let image_builder = ImageBuilder::new(docker.clone());
    let build_result = image_builder
        .build(
            &render_result.dockerfile,
            &render_result.context_files,
            cli.container.no_cache,
        )
        .await?;
    info!(tag = %build_result.tag, cached = build_result.cached, "image ready");

    // Get project directory
    let project_dir = std::env::current_dir()?
        .to_string_lossy()
        .to_string();

    // Resolve auth token
    let mut env_vars = Vec::new();
    if let Some(token) = resolve_oauth_token()? {
        env_vars.push(format!("CLAUDE_CODE_OAUTH_TOKEN={token}"));
    }

    // Container lifecycle
    let container_mgr = ContainerManager::new(docker);

    // Clean up any existing container with the same name
    container_mgr.cleanup_existing(&container_name).await?;

    let opts = ContainerOpts {
        image_tag: build_result.tag,
        container_name: container_name.clone(),
        shell: shell.clone(),
        project_dir,
        env_vars,
    };

    let container_id = container_mgr.create_and_start(&opts).await?;

    // Launch interactive shell (blocking)
    let exit_code = container_mgr.exec_interactive_shell(&container_id, &shell)?;

    // Cleanup on shell exit
    container_mgr.stop_and_remove(&container_id).await?;

    if exit_code != 0 {
        std::process::exit(exit_code);
    }

    Ok(())
}
