use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
};

use crate::embeddings::LazyEmbeddingEngine;
use crate::storage::local::LocalStorage;
use crate::tools::{
    MemoryContextParams, MemoryForgetParams, MemoryInspectParams, MemorySaveParams,
    MemorySearchParams, MemorySessionEndParams, MemorySessionStartParams,
    MemorySessionSummaryParams, MemoryUpdateParams,
};

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
    pub fn with_storage(storage: Arc<LocalStorage>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            storage,
            embeddings: LazyEmbeddingEngine::new(),
        }
    }

    // ── memory_save ──────────────────────────────────────────────────────

    #[tool(
        description = "Save a decision, pattern, discovery, or convention to memory with semantic embedding."
    )]
    async fn memory_save(
        &self,
        params: Parameters<MemorySaveParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;

        let embedding = match self.embeddings.embed(&p.content).await {
            Ok(emb) => Some(emb),
            Err(e) => {
                tracing::warn!("Embedding generation failed: {e}");
                None
            }
        };

        let model_id = self.embeddings.model_id().map(|s| s.to_string());
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
            "created_at": memory.created_at.to_rfc3339(),
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&response)
                .unwrap_or_else(|_| format!("Stored memory: {}", memory.id)),
        )]))
    }

    // ── memory_search ────────────────────────────────────────────────────

    #[tool(
        description = "Search memories using hybrid semantic + keyword search. Returns results ranked by relevance."
    )]
    async fn memory_search(
        &self,
        params: Parameters<MemorySearchParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;

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

    // ── memory_inspect ───────────────────────────────────────────────────

    #[tool(
        description = "Inspect the knowledge base: view stats, list memories by scope/category, or get a specific memory by ID."
    )]
    async fn memory_inspect(
        &self,
        params: Parameters<MemoryInspectParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;

        // Get by ID
        if let Some(id) = &p.id {
            let memory = self
                .storage
                .get_memory(id)
                .await
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;

            return match memory {
                Some(m) => Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&m).unwrap_or_default(),
                )])),
                None => Ok(CallToolResult::success(vec![Content::text(format!(
                    "No memory found with ID: {id}"
                ))])),
            };
        }

        // List or stats
        let source = p.source.as_deref().unwrap_or("stats");

        match source {
            "memories" => {
                let memories = self
                    .storage
                    .list_memories(p.scope.as_deref(), p.category.as_deref(), p.limit)
                    .await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&memories).unwrap_or_default(),
                )]))
            }
            _ => {
                let stats = self
                    .storage
                    .stats()
                    .await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&stats).unwrap_or_default(),
                )]))
            }
        }
    }

    // ── memory_update ────────────────────────────────────────────────────

    #[tool(
        description = "Update an existing memory's content, category, tags, or scope. Provide the memory ID and the fields to change."
    )]
    async fn memory_update(
        &self,
        params: Parameters<MemoryUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;

        // Re-embed if content changed
        let new_embedding = if let Some(content) = &p.content {
            self.embeddings.embed(content).await.ok()
        } else {
            None
        };
        let model_id = if new_embedding.is_some() {
            self.embeddings.model_id().map(|s| s.to_string())
        } else {
            None
        };

        let updated = self
            .storage
            .update_memory(
                &p.id,
                p.content.as_deref(),
                p.category.as_deref(),
                p.tags.as_deref(),
                p.scope.as_deref(),
                new_embedding,
                model_id.as_deref(),
            )
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let response = if updated {
            serde_json::json!({"id": p.id, "status": "updated"})
        } else {
            serde_json::json!({"id": p.id, "status": "not_found"})
        };

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&response).unwrap_or_default(),
        )]))
    }

    // ── memory_forget ────────────────────────────────────────────────────

    #[tool(description = "Delete a memory by ID.")]
    async fn memory_forget(
        &self,
        params: Parameters<MemoryForgetParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;

        let deleted = self
            .storage
            .delete_memory(&p.id)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let response = if deleted {
            serde_json::json!({"id": p.id, "status": "deleted"})
        } else {
            serde_json::json!({"id": p.id, "status": "not_found"})
        };

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&response).unwrap_or_default(),
        )]))
    }

    // ── memory_context ───────────────────────────────────────────────────

    #[tool(
        description = "Get unified context about a topic: combines memories, code symbols, and git history. Use focus='architecture' for decisions, focus='recent-changes' for git activity, focus='file' with target path for file details, or omit for general."
    )]
    async fn memory_context(
        &self,
        params: Parameters<MemoryContextParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let focus = p.focus.as_deref().unwrap_or("general");
        let target = p.target.as_deref().unwrap_or("");
        let limit = p.limit.min(15);

        let (mem_limit, sym_limit, git_limit) = match focus {
            "architecture" => (limit, limit * 2, 0),
            "recent-changes" => (2, 3, limit * 2),
            "file" => (3, limit * 3, 5),
            _ => (limit, limit, limit),
        };

        // 1. Search memories
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
            let is_file = target.contains('/') || target.contains('.');
            if is_file {
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

        // 3. Search commits
        let commits = if git_limit > 0 {
            let q = if target.is_empty() {
                None
            } else {
                Some(target)
            };
            self.storage
                .recent_commits(q, git_limit)
                .await
                .unwrap_or_default()
        } else {
            vec![]
        };

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
            .map(|r| serde_json::json!({"content": r.memory.content, "category": r.memory.category, "score": r.score}))
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

    // ── memory_session_start ─────────────────────────────────────────────

    #[tool(
        description = "Start a work session. Tracks what you save during the session for automatic summarization."
    )]
    async fn memory_session_start(
        &self,
        params: Parameters<MemorySessionStartParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let scope = p.scope.unwrap_or_else(detect_project_scope);

        let session_id = self
            .storage
            .create_session(&scope, p.description.as_deref())
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let response = serde_json::json!({
            "session_id": session_id,
            "scope": scope,
            "status": "started",
            "description": p.description,
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&response).unwrap_or_default(),
        )]))
    }

    // ── memory_session_end ───────────────────────────────────────────────

    #[tool(
        description = "End the current work session. Generates a summary of all memories saved during the session."
    )]
    async fn memory_session_end(
        &self,
        params: Parameters<MemorySessionEndParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let scope = detect_project_scope();

        let result = self
            .storage
            .end_session(&scope, p.notes.as_deref())
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        match result {
            Some(summary) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&summary).unwrap_or_default(),
            )])),
            None => Ok(CallToolResult::success(vec![Content::text(
                "No active session found for this project.",
            )])),
        }
    }

    // ── memory_session_summary ───────────────────────────────────────────

    #[tool(
        description = "View session summaries. Use period='today', 'week', 'month' to filter. Shows what was accomplished in each session."
    )]
    async fn memory_session_summary(
        &self,
        params: Parameters<MemorySessionSummaryParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let scope = detect_project_scope();

        let sessions = self
            .storage
            .get_sessions(&scope, p.period.as_deref())
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        if sessions.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No sessions found for this period.",
            )]));
        }

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&sessions).unwrap_or_default(),
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
