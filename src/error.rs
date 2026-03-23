use thiserror::Error;

pub type Result<T> = std::result::Result<T, ContextForgeError>;

#[derive(Debug, Error)]
pub enum ContextForgeError {
    #[error("MCP server error: {0}")]
    Server(String),

    #[error("Tool error: {0}")]
    Tool(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
