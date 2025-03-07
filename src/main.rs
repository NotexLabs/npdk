use std::path::PathBuf;
use clap::{Parser, Subcommand};
use anyhow::Result;
use npdk::packer::Packer;

#[derive(Debug, Parser)]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}

impl Cli {
    pub(crate) async fn run(self) -> Result<()> {
        match self.command {
            CliCommand::Pack { source } => {
                if let Some(source) = source {
                    Packer::new(PathBuf::from(source))?.pack().await?;
                } else {
                    Packer::new(std::env::current_dir().unwrap().join("dist"))?.pack().await?
                }

            }
        }
        Ok(())
    }
}

#[derive(Debug, Subcommand, Clone)]
enum CliCommand {
    Pack {
        #[clap(short, long)]
        source: Option<String>,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.run().await?;
    Ok(())
}
