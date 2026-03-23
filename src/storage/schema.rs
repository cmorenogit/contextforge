/// Main memories table.
pub const CREATE_MEMORIES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS memories (
    id         TEXT PRIMARY KEY,
    content    TEXT NOT NULL,
    category   TEXT,
    files      TEXT,
    tags       TEXT,
    embedding  F32_BLOB(384),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
)
"#;

/// FTS5 virtual table for keyword search (external content, synced via triggers).
pub const CREATE_FTS_TABLE: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    content,
    category,
    tags,
    content=memories,
    content_rowid=rowid
)
"#;

/// Vector index for semantic search (DiskANN).
pub const CREATE_VECTOR_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS memories_vec_idx ON memories (
    libsql_vector_idx(embedding, 'metric=cosine', 'compress_neighbors=float8', 'max_neighbors=64')
)
"#;

/// Trigger: sync FTS on INSERT.
pub const CREATE_FTS_INSERT_TRIGGER: &str = r#"
CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(rowid, content, category, tags)
    VALUES (new.rowid, new.content, new.category, new.tags);
END
"#;

/// Trigger: sync FTS on DELETE.
pub const CREATE_FTS_DELETE_TRIGGER: &str = r#"
CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, content, category, tags)
    VALUES ('delete', old.rowid, old.content, old.category, old.tags);
END
"#;

/// Trigger: sync FTS on UPDATE.
pub const CREATE_FTS_UPDATE_TRIGGER: &str = r#"
CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, content, category, tags)
    VALUES ('delete', old.rowid, old.content, old.category, old.tags);
    INSERT INTO memories_fts(rowid, content, category, tags)
    VALUES (new.rowid, new.content, new.category, new.tags);
END
"#;

/// Category index for filtered queries.
pub const CREATE_CATEGORY_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_memories_category ON memories(category)
"#;

/// Created-at index for time-ordered queries.
pub const CREATE_CREATED_AT_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_memories_created_at ON memories(created_at)
"#;

/// All schema statements in order.
pub const MIGRATIONS: &[&str] = &[
    CREATE_MEMORIES_TABLE,
    CREATE_FTS_TABLE,
    CREATE_VECTOR_INDEX,
    CREATE_FTS_INSERT_TRIGGER,
    CREATE_FTS_DELETE_TRIGGER,
    CREATE_FTS_UPDATE_TRIGGER,
    CREATE_CATEGORY_INDEX,
    CREATE_CREATED_AT_INDEX,
];
