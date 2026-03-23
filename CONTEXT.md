# ContextForge — Session Context

> Este archivo contiene todo el contexto necesario para retomar el trabajo en ContextForge.
> Linear: CMO-23 (epic) | Repo: github.com/cmorenogit/contextforge

## Que es ContextForge

MCP server en Rust que da **memoria semantica persistente** a AI coding assistants (Claude Code, Cursor, Copilot). Diferenciador vs Engram (Go): busqueda semantica, code intelligence, captura automatica, sync multi-device.

## Decisiones de arquitectura (finales)

| Componente | Tecnologia | Razon |
|---|---|---|
| Lenguaje | **Rust** | 33x menos RAM, 18x mas rapido, binario unico ~4-8MB |
| MCP SDK | **rust-sdk** (oficial Anthropic) | Maduro, stdio + HTTP |
| Database | **libSQL / Turso** | Fork SQLite: vector search nativo (DiskANN) + embedded replicas |
| Keyword search | **FTS5** (nativo libSQL) | Probado, stemming Porter |
| Vector search | **Vector nativo libSQL** | Zero extensiones, built-in, F32_BLOB |
| Embeddings | **candle** (HuggingFace) | Local, offline, all-MiniLM-L6-v2 (384 dims, 22MB) |
| Code parsing | **tree-sitter** (nativo Rust) | 50+ lenguajes, no necesita compilar |
| Git | **gitoxide** | Pure Rust, 4x mas rapido que libgit2 |
| Distribucion | brew + cargo install + GitHub Releases | Mismo patron que Engram |

## 4 Tools MCP

| Tool | Funcion | Tipo captura |
|---|---|---|
| `remember` | Guarda decision/patron/descubrimiento con embedding | Manual |
| `recall` | Busqueda hibrida (FTS5 + vector, RRF ranking) | — |
| `scan` | Analiza codebase (tree-sitter + git log) | Automatica |
| `context` | Contexto relevante para sesion actual | — |

## Hooks (Claude Code integration)

| Hook | Trigger | Que hace |
|---|---|---|
| session-start | Abrir Claude Code | Scan incremental (git diff), inyecta contexto real del proyecto |
| session-stop | Cerrar Claude Code | Parsea commits nuevos, extrae decisiones, actualiza indice |
| post-compaction | Context truncation | Semantic recall de lo relevante (no dump completo) |

**Diferencia clave vs Engram**: hooks hacen trabajo real (scan, parse, recall semantico), no solo inyectan texto con instrucciones.

## Modos de operacion

| Modo | Config | Sync | Costo |
|---|---|---|---|
| Local-only (default) | `contextforge mcp` | No | $0 |
| Synced (opcional) | `TURSO_URL=... TURSO_TOKEN=... contextforge mcp` | Si, automatico | $0 (free tier 500M reads/mes) |

## Busqueda hibrida (RRF)

```
Query: "como maneja auth este proyecto?"

1. FTS5 (keyword): busca "auth", "proyecto" → resultados exactos
2. Vector (semantic): embedding del query → cosine similarity
   → encuentra "JWT middleware", "OAuth2", "session handling"

3. Reciprocal Rank Fusion:
   Score = 0.4 * keyword_rank + 0.6 * semantic_rank
   → Top resultados combinados
```

## Linear tracking

| ID | Fase | Titulo | Blocked by |
|---|---|---|---|
| CMO-29 | 1 | CF-01: Setup Rust + MCP SDK | — (PRIMERO) |
| CMO-30 | 1 | CF-02: Storage (libSQL + FTS5 + Vector) | CMO-29 |
| CMO-31 | 2 | CF-03: Code intelligence (tree-sitter + gitoxide) | CMO-30 |
| CMO-32 | 2 | CF-04: Embeddings (candle + all-MiniLM) | CMO-30 |
| CMO-33 | 3 | CF-05: Context tool + Hooks | CMO-31, CMO-32 |
| CMO-34 | 3 | CF-06: Distribucion (brew + cargo + releases) | CMO-33 |

## Referencia: Engram (competencia/inspiracion)

- Escrito en Go, binario 12MB, brew install
- SQLite + FTS5 (keyword only, BM25)
- Manual: mem_save, mem_search, mem_context
- Hooks: session-start (inyecta texto), session-stop, post-compaction
- SIN: embeddings, vector search, code intelligence, sync automatico
- DB: ~/.engram/engram.db (2.2MB, 164 sesiones, 100 observaciones)

## Investigaciones completadas

Toda la investigacion tecnica esta en engram bajo topic keys:
- `portfolio/2026-projects-roadmap` — roadmap completo 6 proyectos
- `portfolio/linear-tracking` — IDs de Linear

### Tecnologias investigadas y validadas:
1. **MCP SDK Rust** — oficial Anthropic, maduro, tokio async, macros procedurales
2. **libSQL crate** — v0.4.0, production-ready, vector nativo, FTS5, embedded replicas
3. **candle** — HuggingFace ML framework para Rust, all-MiniLM-L6-v2 funciona
4. **tree-sitter** — bindings nativos Rust (es proyecto Rust original), 50+ lenguajes
5. **gitoxide** — pure Rust, 4x mas rapido que libgit2, lee git log y commits

## Siguiente paso: SDD

```bash
# En este proyecto, ejecutar:
/sdd-explore contextforge-cf01

# Esto debe explorar:
# 1. MCP rust-sdk: como crear un server minimo con stdio transport
# 2. Validar que compila con las dependencias principales
# 3. Probar conexion con Claude Code
# 4. Definir estructura de proyecto (modules, error handling)
```

## Crates principales (Cargo.toml futuro)

```toml
# MCP
rmcp = "0.1"  # o el SDK oficial

# Async
tokio = { version = "1", features = ["full"] }

# Database
libsql = "0.4"

# Embeddings
candle-core = "0.8"
candle-nn = "0.8"
candle-transformers = "0.8"
hf-hub = "0.3"  # descargar modelos

# Code parsing
tree-sitter = "0.25"
tree-sitter-typescript = "0.25"
tree-sitter-javascript = "0.25"
tree-sitter-python = "0.25"
tree-sitter-rust = "0.25"

# Git
gix = "0.70"  # gitoxide

# CLI
clap = { version = "4", features = ["derive"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Error handling
thiserror = "2"
anyhow = "1"
```
