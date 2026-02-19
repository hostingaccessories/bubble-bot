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

use std::sync::Arc;

use anyhow::Result;
use bollard::Docker;
use clap::Parser;
use tokio::sync::Mutex;
use tracing::{info, warn};

use auth::resolve_oauth_token;
use cli::{Cli, Command};
use config::Config;
use docker::clean::Cleaner;
use docker::containers::{ContainerManager, ContainerOpts, default_container_name};
use docker::images::ImageBuilder;
use docker::networks::{NetworkManager, default_network_name};
use hooks::HookRunner;
use services::{Service, collect_service_env_vars, collect_services};
use shell::{collect_dotfile_mounts, resolve_shell};
use templates::TemplateRenderer;

/// Tracks all Docker resources that need cleanup on shutdown.
/// Shared between the main task and signal handler.
#[derive(Default)]
struct CleanupState {
    docker: Option<Docker>,
    dev_container_id: Option<String>,
    service_container_ids: Vec<String>,
    network_name: Option<String>,
}

impl CleanupState {
    /// Performs cleanup of all tracked Docker resources.
    /// Safe to call multiple times — resources are cleared after cleanup.
    async fn cleanup(&mut self) {
        let Some(docker) = self.docker.take() else {
            return;
        };

        let container_mgr = ContainerManager::new(docker.clone());
        let network_mgr = NetworkManager::new(docker);

        // Stop and remove dev container
        if let Some(id) = self.dev_container_id.take() {
            if let Err(e) = container_mgr.stop_and_remove(&id).await {
                warn!(error = %e, "failed to clean up dev container");
            }
        }

        // Stop and remove service containers
        for id in self.service_container_ids.drain(..) {
            if let Err(e) = container_mgr.stop_and_remove(&id).await {
                warn!(error = %e, "failed to clean up service container");
            }
        }

        // Remove network
        if let Some(name) = self.network_name.take() {
            if let Err(e) = network_mgr.remove_network(&name).await {
                warn!(error = %e, "failed to clean up network");
            }
        }
    }
}

/// Spawns a background task that listens for SIGINT/SIGTERM and performs
/// cleanup of all tracked Docker resources. Returns a `JoinHandle` that
/// should be aborted once the normal cleanup path completes.
fn spawn_signal_handler(state: Arc<Mutex<CleanupState>>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let ctrl_c = tokio::signal::ctrl_c();
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to install SIGTERM handler");

        tokio::select! {
            _ = ctrl_c => {
                warn!("received SIGINT — cleaning up containers");
            }
            _ = sigterm.recv() => {
                warn!("received SIGTERM — cleaning up containers");
            }
        }

        state.lock().await.cleanup().await;
        std::process::exit(130); // 128 + 2 (SIGINT convention)
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let config = Config::load(&cli)?;
    let command = cli.command();

    if cli.container.dry_run {
        return run_dry_run(&config, &command);
    }

    match command {
        Command::Shell => run_shell(&cli, &config).await,
        Command::Claude { args } => run_claude(&cli, &config, &args).await,
        Command::Chief { args } => run_chief(&cli, &config, &args).await,
        Command::Config => run_config(&config),
        Command::Clean { volumes } => run_clean(volumes).await,
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

/// Prints a dry-run summary: resolved config, generated Dockerfile, and Docker
/// commands that would be executed — without creating any containers, networks,
/// or images.
fn run_dry_run(config: &Config, command: &Command) -> Result<()> {
    // Resolved config
    let config_output = toml::to_string_pretty(config)?;
    println!("=== Resolved Config ===\n{config_output}");

    // Determine the exec command and whether Chief layer is needed
    let (exec_cmd, install_chief) = match command {
        Command::Shell => {
            let shell = resolve_shell(config.container.shell.as_deref());
            (format!("docker exec -it <container> {shell}"), false)
        }
        Command::Claude { args } => {
            let mut parts = vec![
                "docker exec -it <container> claude --permission-mode bypassPermissions".to_string(),
            ];
            for arg in args {
                parts.push(arg.clone());
            }
            (parts.join(" "), false)
        }
        Command::Chief { args } => {
            let mut parts = vec!["docker exec -it <container> chief".to_string()];
            for arg in args {
                parts.push(arg.clone());
            }
            (parts.join(" "), true)
        }
        Command::Exec { cmd } => {
            let mut parts = vec!["docker exec <container>".to_string()];
            for c in cmd {
                parts.push(c.clone());
            }
            (parts.join(" "), false)
        }
        Command::Build => ("(build only — no container started)".to_string(), false),
        Command::Config => {
            println!("(config subcommand — no Docker operations)");
            return Ok(());
        }
        Command::Clean { volumes } => {
            println!(
                "(clean subcommand — would remove bubble-boy:* images and bubble-boy-* networks{})",
                if *volumes { " and volumes" } else { "" }
            );
            return Ok(());
        }
    };

    // Render Dockerfile
    let renderer = TemplateRenderer::new()?;
    let render_result = renderer.render_with_options(config, install_chief)?;
    let image_tag = ImageBuilder::compute_tag(&render_result.dockerfile);

    println!("=== Generated Dockerfile ===\n{}", render_result.dockerfile);

    // Docker commands
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
    let project_dir = std::env::current_dir()?
        .to_string_lossy()
        .to_string();
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };

    println!("=== Docker Commands ===");
    println!("Image tag: {image_tag}");
    println!("docker build -t {image_tag} .");
    println!("docker network create {network_name}");

    // Service containers
    let project = project_name();
    let services = collect_services(config, &project);
    for service in &services {
        let svc_name = service.container_name(&project);
        println!(
            "docker run -d --name {svc_name} --network {network_name} {}",
            service.image()
        );
    }

    // Dev container
    let mut docker_run = format!(
        "docker run -d --name {container_name} --user {uid}:{gid} -v {project_dir}:/workspace --network {network_name}"
    );

    // Dotfile mounts
    if config.shell.mount_configs {
        for mount in collect_dotfile_mounts() {
            docker_run.push_str(&format!(" -v {mount}"));
        }
    }

    // Env vars (service env only — token redacted)
    let service_envs = collect_service_env_vars(&services);
    for env in &service_envs {
        docker_run.push_str(&format!(" -e {env}"));
    }
    docker_run.push_str(" -e CLAUDE_CODE_OAUTH_TOKEN=<token>");

    docker_run.push_str(&format!(" {image_tag} sleep infinity"));
    println!("{docker_run}");

    // Exec command
    println!("{exec_cmd}");

    // Hooks
    if !config.hooks.post_start.is_empty() {
        println!("\npost_start hooks:");
        for hook in &config.hooks.post_start {
            println!("  docker exec <container> sh -c {hook:?}");
        }
    }
    if !config.hooks.pre_stop.is_empty() {
        println!("\npre_stop hooks:");
        for hook in &config.hooks.pre_stop {
            println!("  docker exec <container> sh -c {hook:?}");
        }
    }

    Ok(())
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

fn run_config(config: &Config) -> Result<()> {
    let output = toml::to_string_pretty(config)?;
    print!("{output}");
    Ok(())
}

async fn run_clean(remove_volumes: bool) -> Result<()> {
    let docker = Docker::connect_with_local_defaults()
        .map_err(|e| anyhow::anyhow!("failed to connect to Docker: {e}"))?;

    let cleaner = Cleaner::new(docker);
    cleaner.clean(remove_volumes).await
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

    // Set up shared cleanup state and signal handler
    let cleanup_state = Arc::new(Mutex::new(CleanupState {
        docker: Some(docker.clone()),
        network_name: Some(network_name.clone()),
        ..Default::default()
    }));
    let signal_handle = spawn_signal_handler(Arc::clone(&cleanup_state));

    // Create bridge network
    let network_mgr = NetworkManager::new(docker.clone());
    network_mgr.ensure_network(&network_name).await?;

    // Container lifecycle
    let container_mgr = ContainerManager::new(docker);

    // Start service containers
    let service_ids = start_services(&container_mgr, &services, &network_name).await?;

    // Register service containers for signal cleanup
    cleanup_state.lock().await.service_container_ids = service_ids.clone();

    // Clean up any existing dev container with the same name
    container_mgr.cleanup_existing(&container_name).await?;

    // Collect dotfile mounts if configured
    let extra_binds = if config.shell.mount_configs {
        collect_dotfile_mounts()
    } else {
        Vec::new()
    };

    let opts = ContainerOpts {
        image_tag: build_result.tag,
        container_name: container_name.clone(),
        shell: "zsh".to_string(),
        project_dir,
        env_vars,
        network: Some(network_name.clone()),
        extra_binds,
    };

    let container_id = container_mgr.create_and_start(&opts).await?;

    // Register dev container for signal cleanup
    cleanup_state.lock().await.dev_container_id = Some(container_id.clone());

    // Run post_start hooks
    let hook_runner = HookRunner::new(&container_id, &config.hooks);
    hook_runner.run_post_start();

    // Build Chief command
    let mut cmd: Vec<&str> = vec!["chief"];
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    cmd.extend(&arg_refs);

    // Launch Chief (blocking)
    let exit_code = container_mgr.exec_interactive_command(&container_id, &cmd)?;

    // Normal exit — cancel signal handler and clean up
    signal_handle.abort();

    // Run pre_stop hooks
    hook_runner.run_pre_stop();

    // Cleanup on exit
    cleanup_state.lock().await.cleanup().await;

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

    // Set up shared cleanup state and signal handler
    let cleanup_state = Arc::new(Mutex::new(CleanupState {
        docker: Some(docker.clone()),
        network_name: Some(network_name.clone()),
        ..Default::default()
    }));
    let signal_handle = spawn_signal_handler(Arc::clone(&cleanup_state));

    // Create bridge network
    let network_mgr = NetworkManager::new(docker.clone());
    network_mgr.ensure_network(&network_name).await?;

    // Container lifecycle
    let container_mgr = ContainerManager::new(docker);

    // Start service containers
    let service_ids = start_services(&container_mgr, &services, &network_name).await?;

    // Register service containers for signal cleanup
    cleanup_state.lock().await.service_container_ids = service_ids.clone();

    // Clean up any existing dev container with the same name
    container_mgr.cleanup_existing(&container_name).await?;

    // Collect dotfile mounts if configured
    let extra_binds = if config.shell.mount_configs {
        collect_dotfile_mounts()
    } else {
        Vec::new()
    };

    let opts = ContainerOpts {
        image_tag: build_result.tag,
        container_name: container_name.clone(),
        shell: "zsh".to_string(),
        project_dir,
        env_vars,
        network: Some(network_name.clone()),
        extra_binds,
    };

    let container_id = container_mgr.create_and_start(&opts).await?;

    // Register dev container for signal cleanup
    cleanup_state.lock().await.dev_container_id = Some(container_id.clone());

    // Run post_start hooks
    let hook_runner = HookRunner::new(&container_id, &config.hooks);
    hook_runner.run_post_start();

    // Build Claude Code command
    let mut cmd: Vec<&str> = vec!["claude", "--permission-mode", "bypassPermissions"];
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    cmd.extend(&arg_refs);

    // Launch Claude Code (blocking)
    let exit_code = container_mgr.exec_interactive_command(&container_id, &cmd)?;

    // Normal exit — cancel signal handler and clean up
    signal_handle.abort();

    // Run pre_stop hooks
    hook_runner.run_pre_stop();

    // Cleanup on exit
    cleanup_state.lock().await.cleanup().await;

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

    // Resolve shell (config > $SHELL > bash fallback)
    let shell = resolve_shell(config.container.shell.as_deref());

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

    // Set up shared cleanup state and signal handler
    let cleanup_state = Arc::new(Mutex::new(CleanupState {
        docker: Some(docker.clone()),
        network_name: Some(network_name.clone()),
        ..Default::default()
    }));
    let signal_handle = spawn_signal_handler(Arc::clone(&cleanup_state));

    // Create bridge network
    let network_mgr = NetworkManager::new(docker.clone());
    network_mgr.ensure_network(&network_name).await?;

    // Container lifecycle
    let container_mgr = ContainerManager::new(docker);

    // Start service containers
    let service_ids = start_services(&container_mgr, &services, &network_name).await?;

    // Register service containers for signal cleanup
    cleanup_state.lock().await.service_container_ids = service_ids.clone();

    // Clean up any existing dev container with the same name
    container_mgr.cleanup_existing(&container_name).await?;

    // Collect dotfile mounts if configured
    let extra_binds = if config.shell.mount_configs {
        collect_dotfile_mounts()
    } else {
        Vec::new()
    };

    let opts = ContainerOpts {
        image_tag: build_result.tag,
        container_name: container_name.clone(),
        shell: shell.clone(),
        project_dir,
        env_vars,
        network: Some(network_name.clone()),
        extra_binds,
    };

    let container_id = container_mgr.create_and_start(&opts).await?;

    // Register dev container for signal cleanup
    cleanup_state.lock().await.dev_container_id = Some(container_id.clone());

    // Run post_start hooks
    let hook_runner = HookRunner::new(&container_id, &config.hooks);
    hook_runner.run_post_start();

    // Launch interactive shell (blocking)
    let exit_code = container_mgr.exec_interactive_shell(&container_id, &shell)?;

    // Normal exit — cancel signal handler and clean up
    signal_handle.abort();

    // Run pre_stop hooks
    hook_runner.run_pre_stop();

    // Cleanup on shell exit
    cleanup_state.lock().await.cleanup().await;

    if exit_code != 0 {
        std::process::exit(exit_code);
    }

    Ok(())
}
