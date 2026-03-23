use schemars::JsonSchema;
use serde::Deserialize;

/// Parameters for the `remember` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RememberParams {
    /// What to remember — a decision, pattern, discovery, or convention
    pub content: String,
    /// Category: decision, pattern, discovery, convention, bugfix
    #[serde(default)]
    pub category: Option<String>,
    /// Related file paths for context
    #[serde(default)]
    pub files: Option<Vec<String>>,
    /// Tags for additional categorization
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Scope: "global" (cross-project) or "project" (current project only). Default: auto-detected.
    #[serde(default)]
    pub scope: Option<String>,
}
