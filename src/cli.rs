use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "bubble-boy", about = "Ephemeral Docker dev containers")]
pub struct Cli;
