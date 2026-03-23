use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
};

use crate::code_intel::CodeScanner;
use crate::embeddings::LazyEmbeddingEngine;
use crate::storage::local::LocalStorage;
use crate::tools::{ContextParams, RecallParams, RememberParams, ScanParams};

#[derive(Clone)]
pub struct ContextForgeServer {
    tool_router: ToolRouter<Self>,
    storage: Arc<LocalStorage>,
    embeddings: LazyEmbeddingEngine,
}

/// Derive project scope from cwd (e.g., "/Users/cesar/Code/myproject" → "project:myproject").
fn detect_project_scope() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .map(|name| format!("project:{name}"))
        .unwrap_or_else(|| "global".to_string())
}

#[tool_router]
impl ContextForgeServer {
    /// Create server with the given storage backend.
    pub fn with_storage(storage: Arc<LocalStorage>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            storage,
            embeddings: LazyEmbeddingEngine::new(),
        }
    }

    #[tool(
        description = "Remember a decision, pattern, or discovery. Stores it with semantic embedding for later recall."
    )]
    async fn remember(
        &self,
        params: Parameters<RememberParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;

        // Generate embedding (graceful degradation: store without if it fails)
        let embedding = match self.embeddings.embed(&p.content).await {
            Ok(emb) => Some(emb),
            Err(e) => {
                tracing::warn!("Embedding generation failed, storing without vector: {e}");
                None
            }
        };

        let model_id = self.embeddings.model_id().map(|s| s.to_string());

        // Auto-detect scope: user override > project auto-detect
        let scope = p.scope.unwrap_or_else(detect_project_scope);

        let memory = self
            .storage
            .store(
                p.content,
                p.category,
                p.files.unwrap_or_default(),
                p.tags.unwrap_or_default(),
                embedding,
                model_id.as_deref(),
                &scope,
            )
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let response = serde_json::json!({
            "id": memory.id,
            "status": "stored",
            "scope": scope,
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

        // Generate query embedding for vector/hybrid search
        let query_embedding = self.embeddings.embed(&p.query).await.ok();

        let mode = if query_embedding.is_some() {
            crate::storage::SearchMode::Hybrid
        } else {
            crate::storage::SearchMode::Keyword
        };

        let filter = crate::storage::SearchFilter {
            category: p.category,
            limit: p.limit,
            mode,
            query_embedding,
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

    #[tool(
        description = "Scan codebase structure (tree-sitter) and git history (gitoxide). Extracts functions, classes, structs, imports, and recent commits."
    )]
    async fn scan(&self, params: Parameters<ScanParams>) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let root = std::path::PathBuf::from(p.path.as_deref().unwrap_or("."));
        let patterns = p.patterns.unwrap_or_default();

        let mut scanner = CodeScanner::new();
        let summary = scanner
            .scan(
                &root,
                &patterns,
                p.include_git,
                p.max_commits as usize,
                &self.storage,
            )
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let response = serde_json::to_string_pretty(&summary)
            .unwrap_or_else(|_| format!("Scanned {} files", summary.files_scanned));

        Ok(CallToolResult::success(vec![Content::text(response)]))
    }

    #[tool(
        description = "Get unified context: combines memories, code symbols, and git history relevant to your query. Use focus='file' with a target path, focus='recent-changes' for git activity, focus='architecture' for decisions and patterns, or omit for general context."
    )]
    async fn context(&self, params: Parameters<ContextParams>) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let focus = p.focus.as_deref().unwrap_or("general");
        let target = p.target.as_deref().unwrap_or("");
        let limit = p.limit.min(15);

        // Determine what to query based on focus
        let (mem_limit, sym_limit, git_limit) = match focus {
            "architecture" => (limit, limit * 2, 0),
            "recent-changes" => (2, 3, limit * 2),
            "file" => (3, limit * 3, 5),
            _ => (limit, limit, limit),
        };

        // 1. Search memories (semantic)
        let memories = if mem_limit > 0 && !target.is_empty() {
            let query_embedding = self.embeddings.embed(target).await.ok();
            let mode = if query_embedding.is_some() {
                crate::storage::SearchMode::Hybrid
            } else {
                crate::storage::SearchMode::Keyword
            };
            let filter = crate::storage::SearchFilter {
                category: None,
                limit: mem_limit,
                mode,
                query_embedding,
            };
            self.storage
                .search(target, filter)
                .await
                .unwrap_or_default()
        } else {
            vec![]
        };

        // 2. Search symbols
        let symbols = if sym_limit > 0 && !target.is_empty() {
            let is_file_path = target.contains('/') || target.contains('.');
            if is_file_path {
                self.storage
                    .symbols_for_file(target, sym_limit)
                    .await
                    .unwrap_or_default()
            } else {
                self.storage
                    .search_symbols(target, sym_limit)
                    .await
                    .unwrap_or_default()
            }
        } else {
            vec![]
        };

        // 3. Search git commits
        let commits = if git_limit > 0 {
            let query = if target.is_empty() {
                None
            } else {
                Some(target)
            };
            self.storage
                .recent_commits(query, git_limit)
                .await
                .unwrap_or_default()
        } else {
            vec![]
        };

        // Build summary
        let summary = format!(
            "Found {} memories, {} symbols, {} commits{}",
            memories.len(),
            symbols.len(),
            commits.len(),
            if !target.is_empty() {
                format!(" related to '{target}'")
            } else {
                String::new()
            }
        );

        let mem_json: Vec<serde_json::Value> = memories
            .iter()
            .map(|r| {
                serde_json::json!({
                    "content": r.memory.content,
                    "category": r.memory.category,
                    "score": r.score,
                })
            })
            .collect();

        let response = serde_json::json!({
            "summary": summary,
            "memories": mem_json,
            "symbols": symbols,
            "recent_activity": commits,
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&response).unwrap_or(summary),
        )]))
    }
}

#[tool_handler]
impl ServerHandler for ContextForgeServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("ContextForge: Persistent semantic memory for AI coding assistants")
    }
}
