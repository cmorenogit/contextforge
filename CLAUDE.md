# ContextForge

MCP server de memoria semantica persistente para AI coding assistants. Escrito en Rust.

## Stack

- Rust 1.94+
- MCP SDK: rust-sdk (oficial Anthropic)
- Database: libSQL / Turso (SQLite fork con vector search nativo)
- Embeddings: candle (HuggingFace) — all-MiniLM-L6-v2, local, offline
- Code parsing: tree-sitter (nativo Rust)
- Git: gitoxide (pure Rust)
- Distribucion: brew install + cargo install + GitHub Releases

## Arquitectura

```
ContextForge MCP Server (Rust)
├── MCP layer (stdio + HTTP transport)
├── Storage (libSQL)
│   ├── FTS5 (keyword search)
│   ├── Vector nativo (DiskANN, semantic search)
│   └── Hybrid ranking (RRF: 40% keyword + 60% semantic)
├── Embeddings (candle, all-MiniLM-L6-v2, 384 dims)
├── Code Intelligence (tree-sitter, 50+ lenguajes)
├── Git Context (gitoxide, conventional commits parser)
└── Hooks (Claude Code integration)
```

## Tools MCP

| Tool | Funcion |
|---|---|
| `remember` | Guarda decision/patron/descubrimiento con embedding |
| `recall` | Busqueda hibrida (FTS5 + vector) |
| `scan` | Analiza codebase (tree-sitter + git log) |
| `context` | Contexto relevante para sesion actual |

## Comandos

```bash
cargo build              # Build debug
cargo build --release    # Build produccion
cargo test               # Tests
cargo run -- mcp         # Ejecutar MCP server (stdio)
```

## Convenciones

- Commits: Conventional Commits en ingles (feat:, fix:, refactor:, etc.)
- Codigo: rustfmt + clippy
- Tests: cargo test, integration tests en tests/
- Errores: thiserror para tipos, anyhow para propagacion

## Tracking

- Linear: CMO-23 (epic) con sub-issues CMO-29 a CMO-34
- Proyecto: Portfolio (cmorenodev workspace)
