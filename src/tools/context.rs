use schemars::JsonSchema;
use serde::Deserialize;

/// Parameters for the `context` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ContextParams {
    /// What kind of context is needed (e.g., "architecture", "recent-changes", "related-to")
    #[serde(default)]
    pub focus: Option<String>,
    /// File or symbol to get context about
    #[serde(default)]
    pub target: Option<String>,
    /// Max depth of context traversal
    #[serde(default = "default_depth")]
    pub depth: u32,
}

fn default_depth() -> u32 {
    3
}
