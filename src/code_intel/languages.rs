use tree_sitter::Language;

/// Maps file extensions to tree-sitter languages.
pub struct LanguageRegistry;

pub struct LanguageEntry {
    pub language: Language,
    pub name: &'static str,
}

impl LanguageRegistry {
    /// Detect language from file extension.
    pub fn get(ext: &str) -> Option<LanguageEntry> {
        match ext {
            "rs" => Some(LanguageEntry {
                language: tree_sitter_rust::LANGUAGE.into(),
                name: "Rust",
            }),
            "js" | "jsx" | "mjs" | "cjs" => Some(LanguageEntry {
                language: tree_sitter_javascript::LANGUAGE.into(),
                name: "JavaScript",
            }),
            "ts" => Some(LanguageEntry {
                language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                name: "TypeScript",
            }),
            "tsx" => Some(LanguageEntry {
                language: tree_sitter_typescript::LANGUAGE_TSX.into(),
                name: "TSX",
            }),
            "py" | "pyi" => Some(LanguageEntry {
                language: tree_sitter_python::LANGUAGE.into(),
                name: "Python",
            }),
            _ => None,
        }
    }

    /// Check if a file extension is supported.
    pub fn is_supported(ext: &str) -> bool {
        Self::get(ext).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supported_extensions() {
        assert!(LanguageRegistry::is_supported("rs"));
        assert!(LanguageRegistry::is_supported("js"));
        assert!(LanguageRegistry::is_supported("jsx"));
        assert!(LanguageRegistry::is_supported("mjs"));
        assert!(LanguageRegistry::is_supported("cjs"));
        assert!(LanguageRegistry::is_supported("ts"));
        assert!(LanguageRegistry::is_supported("tsx"));
        assert!(LanguageRegistry::is_supported("py"));
        assert!(LanguageRegistry::is_supported("pyi"));
    }

    #[test]
    fn test_unsupported_extensions() {
        assert!(!LanguageRegistry::is_supported("txt"));
        assert!(!LanguageRegistry::is_supported("md"));
        assert!(!LanguageRegistry::is_supported("toml"));
        assert!(!LanguageRegistry::is_supported(""));
    }

    #[test]
    fn test_language_names() {
        assert_eq!(LanguageRegistry::get("rs").unwrap().name, "Rust");
        assert_eq!(LanguageRegistry::get("js").unwrap().name, "JavaScript");
        assert_eq!(LanguageRegistry::get("ts").unwrap().name, "TypeScript");
        assert_eq!(LanguageRegistry::get("tsx").unwrap().name, "TSX");
        assert_eq!(LanguageRegistry::get("py").unwrap().name, "Python");
    }
}
