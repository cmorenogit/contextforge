use schemars::JsonSchema;
use serde::Deserialize;

fn default_limit() -> u32 {
    5
}

/// Parameters for the `context` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ContextParams {
    /// What kind of context: "architecture", "recent-changes", "file", or omit for general
    #[serde(default)]
    pub focus: Option<String>,
    /// Topic, file path, or symbol name to get context about
    #[serde(default)]
    pub target: Option<String>,
    /// Max results per section (default: 5)
    #[serde(default = "default_limit")]
    pub limit: u32,
}
