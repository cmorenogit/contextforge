use schemars::JsonSchema;
use serde::Deserialize;

/// Parameters for the `memory_update` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryUpdateParams {
    /// ID of the memory to update
    pub id: String,
    /// New content (replaces existing)
    #[serde(default)]
    pub content: Option<String>,
    /// New category
    #[serde(default)]
    pub category: Option<String>,
    /// New tags
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// New scope
    #[serde(default)]
    pub scope: Option<String>,
}
