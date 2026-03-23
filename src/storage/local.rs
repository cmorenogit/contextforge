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

    /// Initialize the database schema.
    pub async fn init(&self) -> Result<()> {
        for sql in schema::MIGRATIONS {
            self.conn
                .execute(sql, ())
                .await
                .map_err(|e| ContextForgeError::Database(format!("Migration failed: {e}")))?;
        }
        Ok(())
    }

    /// Store a new memory.
    pub async fn store(
        &self,
        content: String,
        category: Option<String>,
        files: Vec<String>,
        tags: Vec<String>,
        embedding: Option<Vec<f32>>,
    ) -> Result<Memory> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let files_json =
            serde_json::to_string(&files).map_err(|e| ContextForgeError::Storage(e.to_string()))?;
        let tags_json =
            serde_json::to_string(&tags).map_err(|e| ContextForgeError::Storage(e.to_string()))?;
        let created_at_str = now.to_rfc3339();

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
                        "INSERT INTO memories (id, content, category, files, tags, embedding, created_at) \
                         VALUES (?1, ?2, ?3, ?4, ?5, vector32(?6), ?7)",
                        libsql::params![
                            id.clone(),
                            content.clone(),
                            category.clone(),
                            files_json.clone(),
                            tags_json.clone(),
                            vec_str,
                            created_at_str
                        ],
                    )
                    .await
                    .map_err(|e| ContextForgeError::Database(e.to_string()))?;
            }
            None => {
                self.conn
                    .execute(
                        "INSERT INTO memories (id, content, category, files, tags, embedding, created_at) \
                         VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)",
                        libsql::params![
                            id.clone(),
                            content.clone(),
                            category.clone(),
                            files_json,
                            tags_json,
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

    /// Search memories. Currently keyword-only; vector and hybrid added in CF-04.
    pub async fn search(&self, query: &str, filter: SearchFilter) -> Result<Vec<SearchResult>> {
        let limit = if filter.limit == 0 { 10 } else { filter.limit };

        match filter.mode {
            SearchMode::Keyword | SearchMode::Hybrid => {
                self.search_keyword(query, filter.category.as_deref(), limit)
                    .await
            }
            SearchMode::Vector => {
                // Vector search requires embeddings — defer to CF-04
                Ok(vec![])
            }
        }
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
