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
use clap::Parser;

use cli::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let _cli = Cli::parse();

    Ok(())
}
