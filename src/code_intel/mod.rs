pub mod git;
pub mod languages;
pub mod parser;

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::error::{ContextForgeError, Result};
use crate::storage::local::LocalStorage;

use self::git::GitAnalyzer;
use self::languages::LanguageRegistry;
use self::parser::CodeParser;

/// Summary of a scan operation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanSummary {
    pub files_scanned: usize,
    pub files_skipped: usize,
    pub symbols_found: usize,
    pub commits_analyzed: usize,
    pub errors: Vec<String>,
    pub languages: std::collections::HashMap<String, usize>,
}

/// Result of scanning a single file.
#[derive(Debug)]
pub struct ScanFileResult {
    pub scanned: bool,
    pub symbols: usize,
    pub language: Option<String>,
}

/// Orchestrates code scanning: file walking, parsing, and git analysis.
pub struct CodeScanner {
    parser: CodeParser,
}

/// Directories to always skip.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "vendor",
    "__pycache__",
    ".venv",
    "venv",
    "dist",
    "build",
    ".next",
];

impl Default for CodeScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeScanner {
    pub fn new() -> Self {
        Self {
            parser: CodeParser::new(),
        }
    }

    /// Run a full scan: walk files, parse symbols, optionally analyze git.
    pub async fn scan(
        &mut self,
        root: &Path,
        patterns: &[String],
        include_git: bool,
        max_commits: usize,
        storage: &LocalStorage,
    ) -> Result<ScanSummary> {
        let files = self.collect_files(root, patterns);

        let mut summary = ScanSummary {
            files_scanned: 0,
            files_skipped: 0,
            symbols_found: 0,
            commits_analyzed: 0,
            errors: Vec::new(),
            languages: std::collections::HashMap::new(),
        };

        for file_path in &files {
            match self.scan_file(file_path, storage).await {
                Ok(result) => {
                    if result.scanned {
                        summary.files_scanned += 1;
                        summary.symbols_found += result.symbols;
                        if let Some(lang) = result.language {
                            *summary.languages.entry(lang).or_insert(0) += 1;
                        }
                    } else {
                        summary.files_skipped += 1;
                    }
                }
                Err(e) => {
                    summary.errors.push(format!("{}: {e}", file_path.display()));
                }
            }
        }

        if include_git {
            match GitAnalyzer::open(root) {
                Ok(analyzer) => match analyzer.walk_commits(max_commits) {
                    Ok(commits) => {
                        summary.commits_analyzed = commits.len();
                        if let Err(e) = storage.store_commits(&commits).await {
                            summary.errors.push(format!("Store commits: {e}"));
                        }
                    }
                    Err(e) => summary.errors.push(format!("Git walk: {e}")),
                },
                Err(e) => summary.errors.push(format!("Git open: {e}")),
            }
        }

        Ok(summary)
    }

    /// Collect files to scan, filtering by patterns and supported extensions.
    fn collect_files(&self, root: &Path, patterns: &[String]) -> Vec<PathBuf> {
        let glob_patterns: Vec<glob::Pattern> = patterns
            .iter()
            .filter_map(|p| glob::Pattern::new(p).ok())
            .collect();

        WalkDir::new(root)
            .into_iter()
            .filter_entry(|entry| {
                if entry.file_type().is_dir() {
                    let name = entry.file_name().to_str().unwrap_or("");
                    !name.starts_with('.') || name == "." || !SKIP_DIRS.contains(&name)
                } else {
                    true
                }
            })
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                let path = e.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !LanguageRegistry::is_supported(ext) {
                    return false;
                }
                if glob_patterns.is_empty() {
                    return true;
                }
                let path_str = path.to_str().unwrap_or("");
                glob_patterns.iter().any(|p| p.matches(path_str))
            })
            .map(|e| e.into_path())
            .collect()
    }

    /// Scan a single file: hash, check state, parse if changed.
    async fn scan_file(
        &mut self,
        file_path: &Path,
        storage: &LocalStorage,
    ) -> Result<ScanFileResult> {
        let content = std::fs::read_to_string(file_path).map_err(ContextForgeError::Io)?;

        let hash = sha256_hex(&content);
        let path_str = file_path.to_str().unwrap_or("");

        // Check if file changed since last scan
        let existing_hash = storage.get_scan_hash(path_str).await?;
        if existing_hash.as_deref() == Some(hash.as_str()) {
            return Ok(ScanFileResult {
                scanned: false,
                symbols: 0,
                language: None,
            });
        }

        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let entry = match LanguageRegistry::get(ext) {
            Some(e) => e,
            None => {
                return Ok(ScanFileResult {
                    scanned: false,
                    symbols: 0,
                    language: None,
                });
            }
        };

        let symbols = self.parser.parse(&content, entry.language, entry.name)?;
        let symbol_count = symbols.len();

        // Delete old symbols, store new ones
        storage.delete_symbols_for_file(path_str).await?;
        storage.store_symbols(path_str, &symbols, &hash).await?;
        storage.upsert_scan_state(path_str, &hash).await?;

        Ok(ScanFileResult {
            scanned: true,
            symbols: symbol_count,
            language: Some(entry.name.to_string()),
        })
    }
}

/// Compute SHA-256 hex digest of a string.
pub fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_deterministic() {
        let hash1 = sha256_hex("hello world");
        let hash2 = sha256_hex("hello world");
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, sha256_hex("hello world!"));
    }
}
