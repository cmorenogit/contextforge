use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContextForgeError {
    #[error("MCP server error: {0}")]
    Server(String),

    #[error("Tool error: {0}")]
    Tool(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
