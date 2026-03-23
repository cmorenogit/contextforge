use anyhow::Result;
use clap::{Parser, Subcommand};
use rmcp::ServiceExt;
use rmcp::transport::stdio;
use tracing_subscriber::EnvFilter;

use contextforge::server::ContextForgeServer;

#[derive(Parser)]
#[command(
    name = "contextforge",
    version,
    about = "Persistent semantic memory for AI coding assistants"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the MCP server (stdio transport)
    Mcp,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Mcp => {
            tracing::info!("Starting ContextForge MCP server (stdio)");
            let server = ContextForgeServer::new();
            let service = server.serve(stdio()).await?;
            service.waiting().await?;
        }
    }

    Ok(())
}
