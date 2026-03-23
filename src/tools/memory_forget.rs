use schemars::JsonSchema;
use serde::Deserialize;

/// Parameters for the `memory_forget` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryForgetParams {
    /// ID of the memory to delete
    pub id: String,
}
