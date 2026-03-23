use schemars::JsonSchema;
use serde::Deserialize;

fn default_limit() -> u32 {
    20
}

/// Parameters for the `memory_inspect` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryInspectParams {
    /// What to inspect: "memories", "stats", or omit for stats overview
    #[serde(default)]
    pub source: Option<String>,
    /// Get a specific memory by ID
    #[serde(default)]
    pub id: Option<String>,
    /// Filter by scope ("global", "project:name")
    #[serde(default)]
    pub scope: Option<String>,
    /// Filter by category
    #[serde(default)]
    pub category: Option<String>,
    /// Maximum results to return (default: 20)
    #[serde(default = "default_limit")]
    pub limit: u32,
}
