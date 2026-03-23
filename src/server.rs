use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
};

use crate::tools::{ContextParams, RecallParams, RememberParams, ScanParams};

#[derive(Clone)]
pub struct ContextForgeServer {
    tool_router: ToolRouter<Self>,
}

impl Default for ContextForgeServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router]
impl ContextForgeServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Remember a decision, pattern, or discovery. Stub — not yet implemented.")]
    async fn remember(
        &self,
        params: Parameters<RememberParams>,
    ) -> Result<CallToolResult, McpError> {
        let content = &params.0.content;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "[stub] Would remember: {content}"
        ))]))
    }

    #[tool(
        description = "Recall relevant context using hybrid search. Stub — not yet implemented."
    )]
    async fn recall(&self, params: Parameters<RecallParams>) -> Result<CallToolResult, McpError> {
        let query = &params.0.query;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "[stub] Would search for: {query}"
        ))]))
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
