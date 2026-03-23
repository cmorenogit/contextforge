pub mod local;
pub mod schema;

use chrono::{DateTime, Utc};

/// A stored memory entry.
#[derive(Debug, Clone)]
pub struct Memory {
    pub id: String,
    pub content: String,
    pub category: Option<String>,
    pub files: Vec<String>,
    pub tags: Vec<String>,
    pub embedding: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
}

/// A search result with relevance score.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub memory: Memory,
    pub score: f64,
}

/// How to search memories.
#[derive(Debug, Clone, Copy, Default)]
pub enum SearchMode {
    /// FTS5 keyword search only
    #[default]
    Keyword,
    /// Vector similarity search only (requires embeddings — CF-04)
    Vector,
    /// Hybrid: FTS5 + Vector combined via RRF (requires embeddings — CF-04)
    Hybrid,
}

/// Search filters.
#[derive(Debug, Clone, Default)]
pub struct SearchFilter {
    pub category: Option<String>,
    pub limit: u32,
    pub mode: SearchMode,
    pub query_embedding: Option<Vec<f32>>,
}

/// A work session.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub scope: String,
    pub description: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub summary: Option<String>,
}
