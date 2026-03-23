/// Main memories table.
pub const CREATE_MEMORIES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS memories (
    id              TEXT PRIMARY KEY,
    content         TEXT NOT NULL,
    category        TEXT,
    files           TEXT,
    tags            TEXT,
    embedding       F32_BLOB(384),
    embedding_model TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
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
    // V1: memories (CF-02)
    CREATE_MEMORIES_TABLE,
    CREATE_FTS_TABLE,
    CREATE_VECTOR_INDEX,
    CREATE_FTS_INSERT_TRIGGER,
    CREATE_FTS_DELETE_TRIGGER,
    CREATE_FTS_UPDATE_TRIGGER,
    CREATE_CATEGORY_INDEX,
    CREATE_CREATED_AT_INDEX,
    // V2: code intelligence (CF-03)
    CREATE_CODE_SYMBOLS_TABLE,
    CREATE_SYMBOLS_FILE_INDEX,
    CREATE_SYMBOLS_NAME_INDEX,
    CREATE_SYMBOLS_KIND_INDEX,
    CREATE_SCAN_STATE_TABLE,
    CREATE_GIT_COMMITS_TABLE,
    CREATE_COMMITS_TYPE_INDEX,
    CREATE_COMMITS_DATE_INDEX,
];

/// Migration: add embedding_model column to existing databases.
pub const ADD_EMBEDDING_MODEL_COLUMN: &str = "ALTER TABLE memories ADD COLUMN embedding_model TEXT";

// --- CF-03: Code Intelligence tables ---

/// Code symbols extracted by tree-sitter.
pub const CREATE_CODE_SYMBOLS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS code_symbols (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path  TEXT NOT NULL,
    name       TEXT NOT NULL,
    kind       TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line   INTEGER NOT NULL,
    signature  TEXT,
    file_hash  TEXT NOT NULL
)
"#;

pub const CREATE_SYMBOLS_FILE_INDEX: &str =
    "CREATE INDEX IF NOT EXISTS idx_symbols_file ON code_symbols(file_path)";
pub const CREATE_SYMBOLS_NAME_INDEX: &str =
    "CREATE INDEX IF NOT EXISTS idx_symbols_name ON code_symbols(name)";
pub const CREATE_SYMBOLS_KIND_INDEX: &str =
    "CREATE INDEX IF NOT EXISTS idx_symbols_kind ON code_symbols(kind)";

/// Incremental scan state — tracks file content hashes.
pub const CREATE_SCAN_STATE_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS scan_state (
    file_path    TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL,
    scanned_at   TEXT NOT NULL DEFAULT (datetime('now'))
)
"#;

/// Git commits parsed from repository history.
pub const CREATE_GIT_COMMITS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS git_commits (
    hash         TEXT PRIMARY KEY,
    message      TEXT NOT NULL,
    author       TEXT,
    committed_at TEXT,
    commit_type  TEXT,
    scope        TEXT,
    breaking     INTEGER DEFAULT 0,
    files_changed TEXT
)
"#;

pub const CREATE_COMMITS_TYPE_INDEX: &str =
    "CREATE INDEX IF NOT EXISTS idx_commits_type ON git_commits(commit_type)";
pub const CREATE_COMMITS_DATE_INDEX: &str =
    "CREATE INDEX IF NOT EXISTS idx_commits_date ON git_commits(committed_at)";
