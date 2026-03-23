use schemars::JsonSchema;
use serde::Deserialize;

fn default_max_commits() -> u32 {
    200
}

/// Parameters for the `scan` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScanParams {
    /// Directory path to scan (defaults to current working directory)
    #[serde(default)]
    pub path: Option<String>,
    /// File patterns to include (e.g., "*.rs", "src/**/*.ts")
    #[serde(default)]
    pub patterns: Option<Vec<String>>,
    /// Whether to include git history analysis
    #[serde(default)]
    pub include_git: bool,
    /// Maximum number of git commits to analyze (default: 200)
    #[serde(default = "default_max_commits")]
    pub max_commits: u32,
}
