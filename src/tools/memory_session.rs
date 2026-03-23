use schemars::JsonSchema;
use serde::Deserialize;

/// Parameters for the `memory_session_start` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemorySessionStartParams {
    /// Optional description of what you're working on
    #[serde(default)]
    pub description: Option<String>,
    /// Scope override (default: auto-detected from project)
    #[serde(default)]
    pub scope: Option<String>,
}

/// Parameters for the `memory_session_end` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemorySessionEndParams {
    /// Optional notes to add to the session summary
    #[serde(default)]
    pub notes: Option<String>,
}

/// Parameters for the `memory_session_summary` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemorySessionSummaryParams {
    /// Time filter: "today", "week", "all", or a specific session ID. Default: current/latest session.
    #[serde(default)]
    pub period: Option<String>,
}
