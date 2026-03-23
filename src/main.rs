use std::sync::Arc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use rmcp::ServiceExt;
use rmcp::transport::stdio;
use tracing_subscriber::EnvFilter;

use contextforge::server::ContextForgeServer;
use contextforge::storage::local::LocalStorage;

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
    Mcp {
        /// Database file path (default: ~/.contextforge/memory.db)
        #[arg(long, short)]
        db: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Mcp { db } => {
            let db_path = match db {
                Some(path) => path,
                None => {
                    let home = dirs::home_dir().expect("Could not determine home directory");
                    let dir = home.join(".contextforge");
                    std::fs::create_dir_all(&dir)?;
                    dir.join("memory.db").to_string_lossy().to_string()
                }
            };

            tracing::info!("Starting ContextForge MCP server (stdio), db: {db_path}");

            let storage = LocalStorage::new(&db_path).await?;
            storage.init().await?;

            let server = ContextForgeServer::with_storage(Arc::new(storage));
            let service = server.serve(stdio()).await?;
            service.waiting().await?;
        }
    }

    Ok(())
}
