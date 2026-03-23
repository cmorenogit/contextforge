use schemars::JsonSchema;
use serde::Deserialize;

fn default_limit() -> u32 {
    10
}

/// Parameters for the `memory_search` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemorySearchParams {
    /// Search query — natural language or keywords
    pub query: String,
    /// Maximum number of results to return
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Filter by category
    #[serde(default)]
    pub category: Option<String>,
}
