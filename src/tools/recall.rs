use schemars::JsonSchema;
use serde::Deserialize;

/// Parameters for the `recall` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecallParams {
    /// Search query — natural language or keywords
    pub query: String,
    /// Maximum number of results to return
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Filter by category
    #[serde(default)]
    pub category: Option<String>,
}

fn default_limit() -> u32 {
    10
}
