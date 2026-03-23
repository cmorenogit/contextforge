use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
};

use crate::storage::local::LocalStorage;
use crate::tools::{ContextParams, RecallParams, RememberParams, ScanParams};

#[derive(Clone)]
pub struct ContextForgeServer {
    tool_router: ToolRouter<Self>,
    storage: Arc<LocalStorage>,
}

#[tool_router]
impl ContextForgeServer {
    /// Create server with the given storage backend.
    pub fn with_storage(storage: Arc<LocalStorage>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            storage,
        }
    }

    #[tool(
        description = "Remember a decision, pattern, or discovery. Stores it with full-text indexing for later recall."
    )]
    async fn remember(
        &self,
        params: Parameters<RememberParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let memory = self
            .storage
            .store(
                p.content,
                p.category,
                p.files.unwrap_or_default(),
                p.tags.unwrap_or_default(),
                None, // Embeddings deferred to CF-04
            )
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let response = serde_json::json!({
            "id": memory.id,
            "status": "stored",
            "category": memory.category,
            "files": memory.files,
            "created_at": memory.created_at.to_rfc3339(),
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&response)
                .unwrap_or_else(|_| format!("Stored memory: {}", memory.id)),
        )]))
    }

    #[tool(
        description = "Recall relevant context using hybrid search (keyword + semantic). Returns memories ranked by relevance."
    )]
    async fn recall(&self, params: Parameters<RecallParams>) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let filter = crate::storage::SearchFilter {
            category: p.category,
            limit: p.limit,
            mode: crate::storage::SearchMode::Keyword,
        };

        let results = self
            .storage
            .search(&p.query, filter)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        if results.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No memories found matching your query.",
            )]));
        }

        let response: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.memory.id,
                    "content": r.memory.content,
                    "category": r.memory.category,
                    "score": r.score,
                    "created_at": r.memory.created_at.to_rfc3339(),
                })
            })
            .collect();

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&response)
                .unwrap_or_else(|_| format!("Found {} results", results.len())),
        )]))
    }

    #[tool(description = "Scan codebase structure and git history. Stub — not yet implemented.")]
    async fn scan(&self, params: Parameters<ScanParams>) -> Result<CallToolResult, McpError> {
        let path = params.0.path.as_deref().unwrap_or(".");
        Ok(CallToolResult::success(vec![Content::text(format!(
            "[stub] Would scan: {path}"
        ))]))
    }

    #[tool(
        description = "Get relevant context for the current session. Stub — not yet implemented."
    )]
    async fn context(&self, params: Parameters<ContextParams>) -> Result<CallToolResult, McpError> {
        let focus = params.0.focus.as_deref().unwrap_or("general");
        Ok(CallToolResult::success(vec![Content::text(format!(
            "[stub] Would provide context for: {focus}"
        ))]))
    }
}

#[tool_handler]
impl ServerHandler for ContextForgeServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("ContextForge: Persistent semantic memory for AI coding assistants")
    }
}
