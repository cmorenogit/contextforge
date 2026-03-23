use std::path::Path;

use chrono::{DateTime, Utc};
use libsql::{Builder, Connection, Database};
use uuid::Uuid;

use super::{Memory, SearchFilter, SearchMode, SearchResult, schema};
use crate::error::{ContextForgeError, Result};

/// Local file-based storage using libSQL.
pub struct LocalStorage {
    #[allow(dead_code)]
    db: Database,
    conn: Connection,
}

impl LocalStorage {
    /// Create a new LocalStorage backed by a file.
    pub async fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = Builder::new_local(path.as_ref())
            .build()
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;
        let conn = db
            .connect()
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;
        Ok(Self { db, conn })
    }

    /// Create an in-memory storage (for testing).
    pub async fn in_memory() -> Result<Self> {
        let db = Builder::new_local(":memory:")
            .build()
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;
        let conn = db
            .connect()
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;
        Ok(Self { db, conn })
    }

    /// Initialize the database schema and run pending migrations.
    pub async fn init(&self) -> Result<()> {
        for sql in schema::MIGRATIONS {
            self.conn
                .execute(sql, ())
                .await
                .map_err(|e| ContextForgeError::Database(format!("Migration failed: {e}")))?;
        }

        // Migrations for existing databases
        let _ = self
            .conn
            .execute(schema::ADD_EMBEDDING_MODEL_COLUMN, ())
            .await;
        let _ = self.conn.execute(schema::ADD_SCOPE_COLUMN, ()).await;

        Ok(())
    }

    /// Store a new memory with optional embedding, model tracking, and scope.
    pub async fn store(
        &self,
        content: String,
        category: Option<String>,
        files: Vec<String>,
        tags: Vec<String>,
        embedding: Option<Vec<f32>>,
        embedding_model: Option<&str>,
        scope: &str,
    ) -> Result<Memory> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let files_json =
            serde_json::to_string(&files).map_err(|e| ContextForgeError::Storage(e.to_string()))?;
        let tags_json =
            serde_json::to_string(&tags).map_err(|e| ContextForgeError::Storage(e.to_string()))?;
        let created_at_str = now.to_rfc3339();
        let model_str = embedding_model.map(|s| s.to_string());

        match &embedding {
            Some(emb) => {
                let vec_str = format!(
                    "[{}]",
                    emb.iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                );
                self.conn
                    .execute(
                        "INSERT INTO memories (id, content, category, files, tags, embedding, embedding_model, scope, created_at) \
                         VALUES (?1, ?2, ?3, ?4, ?5, vector32(?6), ?7, ?8, ?9)",
                        libsql::params![
                            id.clone(),
                            content.clone(),
                            category.clone(),
                            files_json.clone(),
                            tags_json.clone(),
                            vec_str,
                            model_str.clone(),
                            scope,
                            created_at_str
                        ],
                    )
                    .await
                    .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            }
            None => {
                self.conn
                    .execute(
                        "INSERT INTO memories (id, content, category, files, tags, embedding, embedding_model, scope, created_at) \
                         VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?8)",
                        libsql::params![
                            id.clone(),
                            content.clone(),
                            category.clone(),
                            files_json,
                            tags_json,
                            model_str,
                            scope,
                            created_at_str
                        ],
                    )
                    .await
                    .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            }
        }

        Ok(Memory {
            id,
            content,
            category,
            files,
            tags,
            embedding,
            created_at: now,
        })
    }

    /// Search memories using keyword, vector, or hybrid mode.
    pub async fn search(&self, query: &str, filter: SearchFilter) -> Result<Vec<SearchResult>> {
        let limit = if filter.limit == 0 { 10 } else { filter.limit };

        match filter.mode {
            SearchMode::Keyword => {
                self.search_keyword(query, filter.category.as_deref(), limit)
                    .await
            }
            SearchMode::Vector => match &filter.query_embedding {
                Some(emb) => {
                    self.search_vector(emb, filter.category.as_deref(), limit)
                        .await
                }
                None => Ok(vec![]),
            },
            SearchMode::Hybrid => match &filter.query_embedding {
                Some(emb) => {
                    self.search_hybrid(query, emb, filter.category.as_deref(), limit)
                        .await
                }
                None => {
                    self.search_keyword(query, filter.category.as_deref(), limit)
                        .await
                }
            },
        }
    }

    /// Vector similarity search using libSQL's DiskANN index.
    async fn search_vector(
        &self,
        embedding: &[f32],
        category: Option<&str>,
        limit: u32,
    ) -> Result<Vec<SearchResult>> {
        let vec_str = format!(
            "[{}]",
            embedding
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        // vector_top_k returns rows ordered by similarity, but no distance column.
        // Use vector_distance_cos() to compute actual cosine distance on the narrowed result set.
        let rows = match category {
            Some(cat) => self
                .conn
                .query(
                    "SELECT m.id, m.content, m.category, m.files, m.tags, m.created_at, \
                         vector_distance_cos(m.embedding, vector32(?1)) AS distance \
                         FROM vector_top_k('memories_vec_idx', vector32(?1), ?2) v \
                         JOIN memories m ON m.rowid = v.id \
                         WHERE m.category = ?3 \
                         ORDER BY distance ASC",
                    libsql::params![vec_str, limit, cat.to_string()],
                )
                .await
                .map_err(|e| ContextForgeError::Database(e.to_string()))?,
            None => self
                .conn
                .query(
                    "SELECT m.id, m.content, m.category, m.files, m.tags, m.created_at, \
                         vector_distance_cos(m.embedding, vector32(?1)) AS distance \
                         FROM vector_top_k('memories_vec_idx', vector32(?1), ?2) v \
                         JOIN memories m ON m.rowid = v.id \
                         ORDER BY distance ASC",
                    libsql::params![vec_str, limit],
                )
                .await
                .map_err(|e| ContextForgeError::Database(e.to_string()))?,
        };

        self.parse_search_rows_with_distance(rows).await
    }

    /// Hybrid search: FTS5 keyword + vector similarity, merged via Reciprocal Rank Fusion.
    async fn search_hybrid(
        &self,
        query: &str,
        embedding: &[f32],
        category: Option<&str>,
        limit: u32,
    ) -> Result<Vec<SearchResult>> {
        let candidate_limit = limit * 3;

        let keyword_results = self
            .search_keyword(query, category, candidate_limit)
            .await?;
        let vector_results = self
            .search_vector(embedding, category, candidate_limit)
            .await?;

        Ok(rrf_merge(&keyword_results, &vector_results, limit as usize))
    }

    /// Parse rows from vector search with actual cosine distance scores.
    /// Cosine distance: 0 = identical, 2 = opposite. Converted to similarity: 1.0 - distance.
    async fn parse_search_rows_with_distance(
        &self,
        mut rows: libsql::Rows,
    ) -> Result<Vec<SearchResult>> {
        let mut results = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?
        {
            let id: String = row
                .get(0)
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            let content: String = row
                .get(1)
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            let category: Option<String> = row
                .get(2)
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            let files_json: Option<String> = row
                .get(3)
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            let tags_json: Option<String> = row
                .get(4)
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            let created_at_str: String = row
                .get(5)
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            let distance: f64 = row
                .get(6)
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;

            let files: Vec<String> = files_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            let tags: Vec<String> = tags_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            results.push(SearchResult {
                memory: Memory {
                    id,
                    content,
                    category,
                    files,
                    tags,
                    embedding: None,
                    created_at,
                },
                score: 1.0 - distance, // Convert cosine distance to similarity
            });
        }

        Ok(results)
    }

    /// FTS5 keyword search.
    async fn search_keyword(
        &self,
        query: &str,
        category: Option<&str>,
        limit: u32,
    ) -> Result<Vec<SearchResult>> {
        let sanitized = sanitize_fts_query(query);

        if sanitized.is_empty() {
            return Ok(vec![]);
        }

        let rows = match category {
            Some(cat) => self
                .conn
                .query(
                    "SELECT m.id, m.content, m.category, m.files, m.tags, m.created_at, \
                                fts.rank \
                         FROM memories_fts fts \
                         JOIN memories m ON m.rowid = fts.rowid \
                         WHERE memories_fts MATCH ?1 AND m.category = ?2 \
                         ORDER BY fts.rank \
                         LIMIT ?3",
                    libsql::params![sanitized, cat.to_string(), limit],
                )
                .await
                .map_err(|e| ContextForgeError::Database(e.to_string()))?,
            None => self
                .conn
                .query(
                    "SELECT m.id, m.content, m.category, m.files, m.tags, m.created_at, \
                                fts.rank \
                         FROM memories_fts fts \
                         JOIN memories m ON m.rowid = fts.rowid \
                         WHERE memories_fts MATCH ?1 \
                         ORDER BY fts.rank \
                         LIMIT ?2",
                    libsql::params![sanitized, limit],
                )
                .await
                .map_err(|e| ContextForgeError::Database(e.to_string()))?,
        };

        self.parse_search_rows(rows).await
    }

    /// Parse rows from a search query into SearchResult vec.
    async fn parse_search_rows(&self, mut rows: libsql::Rows) -> Result<Vec<SearchResult>> {
        let mut results = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?
        {
            let id: String = row
                .get(0)
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            let content: String = row
                .get(1)
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            let category: Option<String> = row
                .get(2)
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            let files_json: Option<String> = row
                .get(3)
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            let tags_json: Option<String> = row
                .get(4)
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            let created_at_str: String = row
                .get(5)
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            let rank: f64 = row
                .get(6)
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;

            let files: Vec<String> = files_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            let tags: Vec<String> = tags_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            results.push(SearchResult {
                memory: Memory {
                    id,
                    content,
                    category,
                    files,
                    tags,
                    embedding: None,
                    created_at,
                },
                score: -rank, // FTS5 rank is negative (lower = better), invert for score
            });
        }

        Ok(results)
    }

    /// Get a memory by ID.
    pub async fn get(&self, id: &str) -> Result<Option<Memory>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, content, category, files, tags, created_at FROM memories WHERE id = ?1",
                libsql::params![id.to_string()],
            )
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;

        match rows
            .next()
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?
        {
            Some(row) => {
                let id: String = row
                    .get(0)
                    .map_err(|e| ContextForgeError::Database(e.to_string()))?;
                let content: String = row
                    .get(1)
                    .map_err(|e| ContextForgeError::Database(e.to_string()))?;
                let category: Option<String> = row
                    .get(2)
                    .map_err(|e| ContextForgeError::Database(e.to_string()))?;
                let files_json: Option<String> = row
                    .get(3)
                    .map_err(|e| ContextForgeError::Database(e.to_string()))?;
                let tags_json: Option<String> = row
                    .get(4)
                    .map_err(|e| ContextForgeError::Database(e.to_string()))?;
                let created_at_str: String = row
                    .get(5)
                    .map_err(|e| ContextForgeError::Database(e.to_string()))?;

                let files: Vec<String> = files_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default();
                let tags: Vec<String> = tags_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default();
                let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                Ok(Some(Memory {
                    id,
                    content,
                    category,
                    files,
                    tags,
                    embedding: None,
                    created_at,
                }))
            }
            None => Ok(None),
        }
    }

    /// Delete a memory by ID.
    pub async fn delete(&self, id: &str) -> Result<bool> {
        let affected = self
            .conn
            .execute(
                "DELETE FROM memories WHERE id = ?1",
                libsql::params![id.to_string()],
            )
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;
        Ok(affected > 0)
    }

    /// Count memories, optionally filtered by category.
    pub async fn count(&self, category: Option<&str>) -> Result<u64> {
        let mut rows = match category {
            Some(cat) => {
                self.conn
                    .query(
                        "SELECT COUNT(*) FROM memories WHERE category = ?1",
                        libsql::params![cat.to_string()],
                    )
                    .await
            }
            None => self.conn.query("SELECT COUNT(*) FROM memories", ()).await,
        }
        .map_err(|e| ContextForgeError::Database(e.to_string()))?;

        if let Some(row) = rows
            .next()
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?
        {
            let count: i64 = row
                .get(0)
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            Ok(count as u64)
        } else {
            Ok(0)
        }
    }
    // --- CF-03: Code Intelligence storage methods ---

    /// Get the content hash for a file from scan state.
    pub async fn get_scan_hash(&self, file_path: &str) -> Result<Option<String>> {
        let mut rows = self
            .conn
            .query(
                "SELECT content_hash FROM scan_state WHERE file_path = ?1",
                libsql::params![file_path],
            )
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;

        if let Some(row) = rows
            .next()
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?
        {
            let hash: String = row
                .get(0)
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            Ok(Some(hash))
        } else {
            Ok(None)
        }
    }

    /// Insert or update scan state for a file.
    pub async fn upsert_scan_state(&self, file_path: &str, content_hash: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO scan_state (file_path, content_hash) VALUES (?1, ?2) \
                 ON CONFLICT(file_path) DO UPDATE SET content_hash = ?2, scanned_at = datetime('now')",
                libsql::params![file_path, content_hash],
            )
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;
        Ok(())
    }

    /// Delete all symbols for a file (before re-parsing).
    pub async fn delete_symbols_for_file(&self, file_path: &str) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM code_symbols WHERE file_path = ?1",
                libsql::params![file_path],
            )
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;
        Ok(())
    }

    /// Store parsed symbols for a file.
    pub async fn store_symbols(
        &self,
        file_path: &str,
        symbols: &[crate::code_intel::parser::Symbol],
        file_hash: &str,
    ) -> Result<()> {
        for sym in symbols {
            self.conn
                .execute(
                    "INSERT INTO code_symbols (file_path, name, kind, start_line, end_line, signature, file_hash) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    libsql::params![
                        file_path,
                        sym.name.clone(),
                        sym.kind.to_string(),
                        sym.start_line as i64,
                        sym.end_line as i64,
                        sym.signature.clone(),
                        file_hash
                    ],
                )
                .await
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;
        }
        Ok(())
    }

    /// Store git commits (upsert — skip duplicates).
    pub async fn store_commits(
        &self,
        commits: &[crate::code_intel::git::CommitInfo],
    ) -> Result<()> {
        for commit in commits {
            let (commit_type, scope, breaking) = match &commit.conventional {
                Some(cc) => (
                    Some(cc.commit_type.clone()),
                    cc.scope.clone(),
                    cc.breaking as i32,
                ),
                None => (None, None, 0),
            };

            self.conn
                .execute(
                    "INSERT INTO git_commits (hash, message, author, committed_at, commit_type, scope, breaking, files_changed) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, '[]') \
                     ON CONFLICT(hash) DO NOTHING",
                    libsql::params![
                        commit.hash.clone(),
                        commit.message.clone(),
                        commit.author.clone(),
                        commit.committed_at.clone(),
                        commit_type,
                        scope,
                        breaking
                    ],
                )
                .await
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;
        }
        Ok(())
    }

    // --- CF-05: Context query methods ---

    /// Search code symbols by name or signature (keyword search).
    pub async fn search_symbols(&self, query: &str, limit: u32) -> Result<Vec<serde_json::Value>> {
        let sanitized = sanitize_fts_query(query);
        if sanitized.is_empty() {
            return Ok(vec![]);
        }

        let mut rows = self
            .conn
            .query(
                "SELECT cs.name, cs.kind, cs.file_path, cs.start_line, cs.end_line, cs.signature \
                 FROM code_symbols cs \
                 WHERE cs.name LIKE ?1 OR cs.signature LIKE ?1 \
                 LIMIT ?2",
                libsql::params![format!("%{query}%"), limit],
            )
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;

        let mut results = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?
        {
            let name: String = row.get(0).unwrap_or_default();
            let kind: String = row.get(1).unwrap_or_default();
            let file_path: String = row.get(2).unwrap_or_default();
            let start_line: i64 = row.get(3).unwrap_or_default();
            let end_line: i64 = row.get(4).unwrap_or_default();
            let signature: Option<String> = row.get(5).ok();

            results.push(serde_json::json!({
                "name": name,
                "kind": kind,
                "file": file_path,
                "lines": format!("{}-{}", start_line, end_line),
                "signature": signature,
            }));
        }
        Ok(results)
    }

    /// Search symbols by file path.
    pub async fn symbols_for_file(
        &self,
        file_path: &str,
        limit: u32,
    ) -> Result<Vec<serde_json::Value>> {
        let mut rows = self
            .conn
            .query(
                "SELECT name, kind, start_line, end_line, signature \
                 FROM code_symbols WHERE file_path LIKE ?1 \
                 ORDER BY start_line LIMIT ?2",
                libsql::params![format!("%{file_path}%"), limit],
            )
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;

        let mut results = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?
        {
            let name: String = row.get(0).unwrap_or_default();
            let kind: String = row.get(1).unwrap_or_default();
            let start_line: i64 = row.get(2).unwrap_or_default();
            let end_line: i64 = row.get(3).unwrap_or_default();
            let signature: Option<String> = row.get(4).ok();

            results.push(serde_json::json!({
                "name": name,
                "kind": kind,
                "lines": format!("{}-{}", start_line, end_line),
                "signature": signature,
            }));
        }
        Ok(results)
    }

    /// Get recent git commits, optionally filtered by keyword.
    pub async fn recent_commits(
        &self,
        query: Option<&str>,
        limit: u32,
    ) -> Result<Vec<serde_json::Value>> {
        let mut rows = match query {
            Some(q) => {
                self.conn
                    .query(
                        "SELECT hash, message, author, committed_at, commit_type, scope \
                         FROM git_commits WHERE message LIKE ?1 \
                         ORDER BY committed_at DESC LIMIT ?2",
                        libsql::params![format!("%{q}%"), limit],
                    )
                    .await
            }
            None => {
                self.conn
                    .query(
                        "SELECT hash, message, author, committed_at, commit_type, scope \
                         FROM git_commits ORDER BY committed_at DESC LIMIT ?1",
                        libsql::params![limit],
                    )
                    .await
            }
        }
        .map_err(|e| ContextForgeError::Database(e.to_string()))?;

        let mut results = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?
        {
            let hash: String = row.get(0).unwrap_or_default();
            let message: String = row.get(1).unwrap_or_default();
            let author: String = row.get(2).unwrap_or_default();
            let committed_at: String = row.get(3).unwrap_or_default();
            let commit_type: Option<String> = row.get(4).ok();
            let scope: Option<String> = row.get(5).ok();

            results.push(serde_json::json!({
                "hash": &hash[..7.min(hash.len())],
                "message": message,
                "author": author,
                "date": committed_at,
                "type": commit_type,
                "scope": scope,
            }));
        }
        Ok(results)
    }

    // --- memory_inspect methods ---

    /// Get a single memory by ID.
    pub async fn get_memory(&self, id: &str) -> Result<Option<serde_json::Value>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, content, category, files, tags, scope, embedding_model, created_at \
                 FROM memories WHERE id = ?1",
                libsql::params![id],
            )
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;

        if let Some(row) = rows
            .next()
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?
        {
            let id: String = row.get(0).unwrap_or_default();
            let content: String = row.get(1).unwrap_or_default();
            let category: Option<String> = row.get(2).ok();
            let files: Option<String> = row.get(3).ok();
            let tags: Option<String> = row.get(4).ok();
            let scope: Option<String> = row.get(5).ok();
            let model: Option<String> = row.get(6).ok();
            let created_at: String = row.get(7).unwrap_or_default();

            Ok(Some(serde_json::json!({
                "id": id,
                "content": content,
                "category": category,
                "files": files.and_then(|f| serde_json::from_str::<Vec<String>>(&f).ok()),
                "tags": tags.and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok()),
                "scope": scope,
                "embedding_model": model,
                "created_at": created_at,
            })))
        } else {
            Ok(None)
        }
    }

    /// List memories with optional filters.
    pub async fn list_memories(
        &self,
        scope: Option<&str>,
        category: Option<&str>,
        limit: u32,
    ) -> Result<Vec<serde_json::Value>> {
        let mut conditions = Vec::new();
        let mut param_values: Vec<libsql::Value> = Vec::new();
        let mut idx = 1;

        if let Some(s) = scope {
            conditions.push(format!("scope = ?{idx}"));
            param_values.push(s.to_string().into());
            idx += 1;
        }
        if let Some(c) = category {
            conditions.push(format!("category = ?{idx}"));
            param_values.push(c.to_string().into());
            idx += 1;
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        param_values.push((limit as i64).into());
        let sql = format!(
            "SELECT id, content, category, scope, created_at FROM memories \
             {where_clause} ORDER BY created_at DESC LIMIT ?{idx}"
        );

        let mut rows = self
            .conn
            .query(&sql, libsql::params_from_iter(param_values))
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;

        let mut results = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?
        {
            let id: String = row.get(0).unwrap_or_default();
            let content: String = row.get(1).unwrap_or_default();
            let category: Option<String> = row.get(2).ok();
            let scope: Option<String> = row.get(3).ok();
            let created_at: String = row.get(4).unwrap_or_default();

            results.push(serde_json::json!({
                "id": id,
                "content": content,
                "category": category,
                "scope": scope,
                "created_at": created_at,
            }));
        }
        Ok(results)
    }

    /// Get stats overview.
    pub async fn stats(&self) -> Result<serde_json::Value> {
        let mem_count = self.count(None).await?;

        let mut scope_rows = self
            .conn
            .query(
                "SELECT COALESCE(scope, 'global') as s, COUNT(*) FROM memories GROUP BY s",
                (),
            )
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;

        let mut by_scope = serde_json::Map::new();
        while let Some(row) = scope_rows
            .next()
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?
        {
            let scope: String = row.get(0).unwrap_or_default();
            let count: i64 = row.get(1).unwrap_or_default();
            by_scope.insert(scope, serde_json::json!(count));
        }

        let mut cat_rows = self
            .conn
            .query(
                "SELECT COALESCE(category, 'uncategorized') as c, COUNT(*) FROM memories GROUP BY c",
                (),
            )
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;

        let mut by_category = serde_json::Map::new();
        while let Some(row) = cat_rows
            .next()
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?
        {
            let cat: String = row.get(0).unwrap_or_default();
            let count: i64 = row.get(1).unwrap_or_default();
            by_category.insert(cat, serde_json::json!(count));
        }

        let sym_count: i64 = {
            let mut r = self
                .conn
                .query("SELECT COUNT(*) FROM code_symbols", ())
                .await
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            r.next()
                .await
                .ok()
                .flatten()
                .and_then(|row| row.get(0).ok())
                .unwrap_or(0)
        };

        let commit_count: i64 = {
            let mut r = self
                .conn
                .query("SELECT COUNT(*) FROM git_commits", ())
                .await
                .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            r.next()
                .await
                .ok()
                .flatten()
                .and_then(|row| row.get(0).ok())
                .unwrap_or(0)
        };

        Ok(serde_json::json!({
            "memories": mem_count,
            "code_symbols": sym_count,
            "git_commits": commit_count,
            "by_scope": by_scope,
            "by_category": by_category,
        }))
    }

    // --- memory_update method ---

    /// Update a memory's content, category, tags, or scope.
    pub async fn update_memory(
        &self,
        id: &str,
        content: Option<&str>,
        category: Option<&str>,
        tags: Option<&[String]>,
        scope: Option<&str>,
        new_embedding: Option<Vec<f32>>,
        embedding_model: Option<&str>,
    ) -> Result<bool> {
        let mut updates = Vec::new();
        let mut params: Vec<libsql::Value> = Vec::new();
        let mut idx = 1;

        if let Some(c) = content {
            updates.push(format!("content = ?{idx}"));
            params.push(c.to_string().into());
            idx += 1;
        }
        if let Some(c) = category {
            updates.push(format!("category = ?{idx}"));
            params.push(c.to_string().into());
            idx += 1;
        }
        if let Some(t) = tags {
            let tags_json =
                serde_json::to_string(t).map_err(|e| ContextForgeError::Storage(e.to_string()))?;
            updates.push(format!("tags = ?{idx}"));
            params.push(tags_json.into());
            idx += 1;
        }
        if let Some(s) = scope {
            updates.push(format!("scope = ?{idx}"));
            params.push(s.to_string().into());
            idx += 1;
        }
        if let Some(emb) = &new_embedding {
            let vec_str = format!(
                "[{}]",
                emb.iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            );
            updates.push(format!("embedding = vector32(?{idx})"));
            params.push(vec_str.into());
            idx += 1;
        }
        if let Some(m) = embedding_model {
            updates.push(format!("embedding_model = ?{idx}"));
            params.push(m.to_string().into());
            idx += 1;
        }

        if updates.is_empty() {
            return Ok(false);
        }

        params.push(id.to_string().into());
        let sql = format!(
            "UPDATE memories SET {} WHERE id = ?{idx}",
            updates.join(", ")
        );

        let rows_affected = self
            .conn
            .execute(&sql, libsql::params_from_iter(params))
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;

        Ok(rows_affected > 0)
    }

    // --- memory_forget method ---

    /// Delete a memory by ID.
    pub async fn delete_memory(&self, id: &str) -> Result<bool> {
        let rows = self
            .conn
            .execute("DELETE FROM memories WHERE id = ?1", libsql::params![id])
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;
        Ok(rows > 0)
    }

    // --- Session methods ---

    /// Create a new session. Closes any active session first.
    pub async fn create_session(&self, scope: &str, description: Option<&str>) -> Result<String> {
        // Close active session if any
        self.conn
            .execute(
                "UPDATE sessions SET ended_at = datetime('now') WHERE scope = ?1 AND ended_at IS NULL",
                libsql::params![scope],
            )
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;

        let id = Uuid::new_v4().to_string();
        self.conn
            .execute(
                "INSERT INTO sessions (id, scope, description) VALUES (?1, ?2, ?3)",
                libsql::params![id.clone(), scope, description],
            )
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;

        Ok(id)
    }

    /// End the active session for a scope, generating a summary.
    pub async fn end_session(
        &self,
        scope: &str,
        notes: Option<&str>,
    ) -> Result<Option<serde_json::Value>> {
        // Find active session
        let mut rows = self
            .conn
            .query(
                "SELECT id, started_at FROM sessions WHERE scope = ?1 AND ended_at IS NULL LIMIT 1",
                libsql::params![scope],
            )
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;

        let (session_id, started_at) = if let Some(row) = rows
            .next()
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?
        {
            let id: String = row.get(0).unwrap_or_default();
            let started: String = row.get(1).unwrap_or_default();
            (id, started)
        } else {
            return Ok(None);
        };

        // Get memories created during this session
        let mut mem_rows = self
            .conn
            .query(
                "SELECT content, category FROM memories \
                 WHERE (scope = ?1 OR scope = 'global') AND created_at >= ?2 \
                 ORDER BY created_at ASC",
                libsql::params![scope, started_at.clone()],
            )
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;

        let mut summary_parts = Vec::new();
        while let Some(row) = mem_rows
            .next()
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?
        {
            let content: String = row.get(0).unwrap_or_default();
            let category: Option<String> = row.get(1).ok();
            let cat = category.unwrap_or_else(|| "note".into());
            summary_parts.push(format!("[{cat}] {content}"));
        }

        if let Some(n) = notes {
            summary_parts.push(format!("[notes] {n}"));
        }

        let summary = if summary_parts.is_empty() {
            "No memories saved during this session.".to_string()
        } else {
            summary_parts.join("\n")
        };

        // Close the session
        self.conn
            .execute(
                "UPDATE sessions SET ended_at = datetime('now'), summary = ?1 WHERE id = ?2",
                libsql::params![summary.clone(), session_id.clone()],
            )
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;

        Ok(Some(serde_json::json!({
            "session_id": session_id,
            "started_at": started_at,
            "summary": summary,
            "memories_count": summary_parts.len(),
        })))
    }

    /// Get session summaries by period.
    pub async fn get_sessions(
        &self,
        scope: &str,
        period: Option<&str>,
    ) -> Result<Vec<serde_json::Value>> {
        let since = match period {
            Some("today") => "date('now')",
            Some("week") => "date('now', '-7 days')",
            Some("month") => "date('now', '-30 days')",
            _ => "date('now')", // default to today
        };

        let sql = format!(
            "SELECT id, scope, description, started_at, ended_at, summary \
             FROM sessions WHERE scope = ?1 AND started_at >= {since} \
             ORDER BY started_at DESC LIMIT 20"
        );

        let mut rows = self
            .conn
            .query(&sql, libsql::params![scope])
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?;

        let mut results = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| ContextForgeError::Database(e.to_string()))?
        {
            let id: String = row.get(0).unwrap_or_default();
            let scope: String = row.get(1).unwrap_or_default();
            let desc: Option<String> = row.get(2).ok();
            let started: String = row.get(3).unwrap_or_default();
            let ended: Option<String> = row.get(4).ok();
            let summary: Option<String> = row.get(5).ok();

            results.push(serde_json::json!({
                "id": id,
                "scope": scope,
                "description": desc,
                "started_at": started,
                "ended_at": ended,
                "summary": summary,
                "status": if ended.is_some() { "completed" } else { "active" },
            }));
        }
        Ok(results)
    }
}

// Hybrid scoring weights
const KEYWORD_WEIGHT: f64 = 0.4;
const VECTOR_WEIGHT: f64 = 0.6;

/// Merge keyword and vector search results using weighted score combination.
/// Uses real scores from both sources: FTS5 BM25 (normalized) + cosine similarity.
fn rrf_merge(
    keyword_results: &[SearchResult],
    vector_results: &[SearchResult],
    limit: usize,
) -> Vec<SearchResult> {
    use std::collections::HashMap;

    // Normalize keyword scores to [0, 1] range
    let max_kw_score = keyword_results
        .iter()
        .map(|r| r.score)
        .fold(f64::NEG_INFINITY, f64::max);
    let kw_scores: HashMap<&str, f64> = keyword_results
        .iter()
        .map(|r| {
            let normalized = if max_kw_score > 0.0 {
                r.score / max_kw_score
            } else {
                0.0
            };
            (r.memory.id.as_str(), normalized)
        })
        .collect();

    // Vector scores are already cosine similarity in [0, 1]
    let vec_scores: HashMap<&str, f64> = vector_results
        .iter()
        .map(|r| (r.memory.id.as_str(), r.score.max(0.0)))
        .collect();

    // Collect all unique memories
    let mut memories: HashMap<&str, &Memory> = HashMap::new();
    for r in keyword_results {
        memories.insert(&r.memory.id, &r.memory);
    }
    for r in vector_results {
        memories.entry(&r.memory.id).or_insert(&r.memory);
    }

    // Compute weighted scores
    let mut scored: Vec<(&str, f64)> = memories
        .keys()
        .map(|id| {
            let kw = kw_scores.get(id).copied().unwrap_or(0.0);
            let vec = vec_scores.get(id).copied().unwrap_or(0.0);
            let score = KEYWORD_WEIGHT * kw + VECTOR_WEIGHT * vec;
            (*id, score)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);

    scored
        .into_iter()
        .filter_map(|(id, score)| {
            memories.get(id).map(|mem| SearchResult {
                memory: (*mem).clone(),
                score,
            })
        })
        .collect()
}

/// Sanitize user input for FTS5 MATCH syntax.
/// Wraps each whitespace-separated token in double quotes to prevent
/// FTS5 operator interpretation (AND, OR, NOT, NEAR, etc.).
fn sanitize_fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|token| {
            let clean = token.replace('"', "");
            format!("\"{clean}\"")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_fts_empty() {
        assert_eq!(sanitize_fts_query(""), "");
        assert_eq!(sanitize_fts_query("   "), "");
    }

    #[test]
    fn test_sanitize_fts_simple() {
        assert_eq!(sanitize_fts_query("hello world"), "\"hello\" \"world\"");
    }

    #[test]
    fn test_sanitize_fts_operators() {
        assert_eq!(
            sanitize_fts_query("AND OR NOT NEAR"),
            "\"AND\" \"OR\" \"NOT\" \"NEAR\""
        );
    }

    #[test]
    fn test_sanitize_fts_existing_quotes() {
        assert_eq!(sanitize_fts_query("\"hello\" world"), "\"hello\" \"world\"");
    }

    #[test]
    fn test_sanitize_fts_single_token() {
        assert_eq!(sanitize_fts_query("auth"), "\"auth\"");
    }
}
