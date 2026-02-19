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
use docker::networks::{NetworkManager, default_network_name};
use hooks::HookRunner;
use services::{Service, collect_service_env_vars, collect_services};
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
        Command::Chief { args } => run_chief(&cli, &config, &args).await,
        _ => {
            info!("subcommand not yet implemented");
            Ok(())
        }
    }
}

/// Returns the project directory name used for naming containers and volumes.
fn project_name() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "project".to_string())
}

/// Starts all configured service containers on the given network.
/// Returns a list of (service_name, container_id) tuples for cleanup.
async fn start_services(
    container_mgr: &ContainerManager,
    services: &[Box<dyn Service>],
    network: &str,
) -> Result<Vec<String>> {
    let project = project_name();
    let mut service_ids = Vec::new();

    for service in services {
        let id = container_mgr
            .start_service(service.as_ref(), network, &project)
            .await?;
        container_mgr.wait_for_ready(&id, service.as_ref(), 30, 2)?;
        service_ids.push(id);
    }

    Ok(service_ids)
}

/// Stops and removes all service containers.
async fn cleanup_services(
    container_mgr: &ContainerManager,
    service_ids: &[String],
) -> Result<()> {
    for id in service_ids {
        container_mgr.stop_and_remove(id).await?;
    }
    Ok(())
}

async fn run_chief(cli: &Cli, config: &Config, args: &[String]) -> Result<()> {
    let docker = Docker::connect_with_local_defaults()
        .map_err(|e| anyhow::anyhow!("failed to connect to Docker: {e}"))?;

    // Resolve container and network names
    let container_name = config
        .container
        .name
        .clone()
        .unwrap_or_else(default_container_name);
    let network_name = config
        .container
        .network
        .clone()
        .unwrap_or_else(default_network_name);

    // Render Dockerfile with Chief installation
    let renderer = TemplateRenderer::new()?;
    let render_result = renderer.render_with_options(config, true)?;

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

    // Collect service env vars for the dev container
    let project = project_name();
    let services = collect_services(config, &project);
    env_vars.extend(collect_service_env_vars(&services));

    // Create bridge network
    let network_mgr = NetworkManager::new(docker.clone());
    network_mgr.ensure_network(&network_name).await?;

    // Container lifecycle
    let container_mgr = ContainerManager::new(docker);

    // Start service containers
    let service_ids = start_services(&container_mgr, &services, &network_name).await?;

    // Clean up any existing dev container with the same name
    container_mgr.cleanup_existing(&container_name).await?;

    let opts = ContainerOpts {
        image_tag: build_result.tag,
        container_name: container_name.clone(),
        shell: "zsh".to_string(),
        project_dir,
        env_vars,
        network: Some(network_name.clone()),
    };

    let container_id = container_mgr.create_and_start(&opts).await?;

    // Run post_start hooks
    let hook_runner = HookRunner::new(&container_id, &config.hooks);
    hook_runner.run_post_start();

    // Build Chief command
    let mut cmd: Vec<&str> = vec!["chief"];
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    cmd.extend(&arg_refs);

    // Launch Chief (blocking)
    let exit_code = container_mgr.exec_interactive_command(&container_id, &cmd)?;

    // Run pre_stop hooks
    hook_runner.run_pre_stop();

    // Cleanup on exit
    container_mgr.stop_and_remove(&container_id).await?;
    cleanup_services(&container_mgr, &service_ids).await?;
    network_mgr.remove_network(&network_name).await?;

    if exit_code != 0 {
        std::process::exit(exit_code);
    }

    Ok(())
}

async fn run_claude(cli: &Cli, config: &Config, args: &[String]) -> Result<()> {
    let docker = Docker::connect_with_local_defaults()
        .map_err(|e| anyhow::anyhow!("failed to connect to Docker: {e}"))?;

    // Resolve container and network names
    let container_name = config
        .container
        .name
        .clone()
        .unwrap_or_else(default_container_name);
    let network_name = config
        .container
        .network
        .clone()
        .unwrap_or_else(default_network_name);

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

    // Collect service env vars for the dev container
    let project = project_name();
    let services = collect_services(config, &project);
    env_vars.extend(collect_service_env_vars(&services));

    // Create bridge network
    let network_mgr = NetworkManager::new(docker.clone());
    network_mgr.ensure_network(&network_name).await?;

    // Container lifecycle
    let container_mgr = ContainerManager::new(docker);

    // Start service containers
    let service_ids = start_services(&container_mgr, &services, &network_name).await?;

    // Clean up any existing dev container with the same name
    container_mgr.cleanup_existing(&container_name).await?;

    let opts = ContainerOpts {
        image_tag: build_result.tag,
        container_name: container_name.clone(),
        shell: "zsh".to_string(),
        project_dir,
        env_vars,
        network: Some(network_name.clone()),
    };

    let container_id = container_mgr.create_and_start(&opts).await?;

    // Run post_start hooks
    let hook_runner = HookRunner::new(&container_id, &config.hooks);
    hook_runner.run_post_start();

    // Build Claude Code command
    let mut cmd: Vec<&str> = vec!["claude", "--permission-mode", "bypassPermissions"];
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    cmd.extend(&arg_refs);

    // Launch Claude Code (blocking)
    let exit_code = container_mgr.exec_interactive_command(&container_id, &cmd)?;

    // Run pre_stop hooks
    hook_runner.run_pre_stop();

    // Cleanup on exit
    container_mgr.stop_and_remove(&container_id).await?;
    cleanup_services(&container_mgr, &service_ids).await?;
    network_mgr.remove_network(&network_name).await?;

    if exit_code != 0 {
        std::process::exit(exit_code);
    }

    Ok(())
}

async fn run_shell(cli: &Cli, config: &Config) -> Result<()> {
    let docker = Docker::connect_with_local_defaults()
        .map_err(|e| anyhow::anyhow!("failed to connect to Docker: {e}"))?;

    // Resolve container and network names
    let container_name = config
        .container
        .name
        .clone()
        .unwrap_or_else(default_container_name);
    let network_name = config
        .container
        .network
        .clone()
        .unwrap_or_else(default_network_name);

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

    // Collect service env vars for the dev container
    let project = project_name();
    let services = collect_services(config, &project);
    env_vars.extend(collect_service_env_vars(&services));

    // Create bridge network
    let network_mgr = NetworkManager::new(docker.clone());
    network_mgr.ensure_network(&network_name).await?;

    // Container lifecycle
    let container_mgr = ContainerManager::new(docker);

    // Start service containers
    let service_ids = start_services(&container_mgr, &services, &network_name).await?;

    // Clean up any existing dev container with the same name
    container_mgr.cleanup_existing(&container_name).await?;

    let opts = ContainerOpts {
        image_tag: build_result.tag,
        container_name: container_name.clone(),
        shell: shell.clone(),
        project_dir,
        env_vars,
        network: Some(network_name.clone()),
    };

    let container_id = container_mgr.create_and_start(&opts).await?;

    // Run post_start hooks
    let hook_runner = HookRunner::new(&container_id, &config.hooks);
    hook_runner.run_post_start();

    // Launch interactive shell (blocking)
    let exit_code = container_mgr.exec_interactive_shell(&container_id, &shell)?;

    // Run pre_stop hooks
    hook_runner.run_pre_stop();

    // Cleanup on shell exit
    container_mgr.stop_and_remove(&container_id).await?;
    cleanup_services(&container_mgr, &service_ids).await?;
    network_mgr.remove_network(&network_name).await?;

    if exit_code != 0 {
        std::process::exit(exit_code);
    }

    Ok(())
}
