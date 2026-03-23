use tree_sitter::{Language, Parser};

use crate::error::{ContextForgeError, Result};

/// A code symbol extracted from source.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub start_line: usize,
    pub end_line: usize,
    pub signature: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Class,
    Interface,
    TypeAlias,
    Import,
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SymbolKind::Function => write!(f, "function"),
            SymbolKind::Struct => write!(f, "struct"),
            SymbolKind::Enum => write!(f, "enum"),
            SymbolKind::Trait => write!(f, "trait"),
            SymbolKind::Impl => write!(f, "impl"),
            SymbolKind::Class => write!(f, "class"),
            SymbolKind::Interface => write!(f, "interface"),
            SymbolKind::TypeAlias => write!(f, "type_alias"),
            SymbolKind::Import => write!(f, "import"),
        }
    }
}

/// Parses source code into symbols using tree-sitter.
pub struct CodeParser {
    parser: Parser,
}

impl Default for CodeParser {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeParser {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
        }
    }

    /// Parse source code and extract top-level symbols.
    pub fn parse(
        &mut self,
        source: &str,
        language: Language,
        lang_name: &str,
    ) -> Result<Vec<Symbol>> {
        self.parser
            .set_language(&language)
            .map_err(|e| ContextForgeError::Parse(format!("Set language failed: {e}")))?;

        let tree = self
            .parser
            .parse(source, None)
            .ok_or_else(|| ContextForgeError::Parse("Parse returned None".into()))?;

        let root = tree.root_node();
        let mut symbols = Vec::new();
        let source_bytes = source.as_bytes();

        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            match lang_name {
                "Rust" => self.extract_rust(&child, source_bytes, &mut symbols),
                "JavaScript" | "TypeScript" | "TSX" => {
                    self.extract_js_ts(&child, source_bytes, &mut symbols)
                }
                "Python" => self.extract_python(&child, source_bytes, &mut symbols),
                _ => {}
            }
        }

        Ok(symbols)
    }

    fn extract_rust(&self, node: &tree_sitter::Node, source: &[u8], symbols: &mut Vec<Symbol>) {
        let kind = node.kind();
        let (sym_kind, name_field) = match kind {
            "function_item" => (SymbolKind::Function, Some("name")),
            "struct_item" => (SymbolKind::Struct, Some("name")),
            "enum_item" => (SymbolKind::Enum, Some("name")),
            "trait_item" => (SymbolKind::Trait, Some("name")),
            "impl_item" => (SymbolKind::Impl, None),
            "type_item" => (SymbolKind::TypeAlias, Some("name")),
            "use_declaration" => (SymbolKind::Import, None),
            _ => return,
        };

        let name = if let Some(field) = name_field {
            self.child_text(node, field, source)
                .unwrap_or_else(|| "<anonymous>".into())
        } else if sym_kind == SymbolKind::Impl {
            // For impl blocks, extract the type name
            self.child_text(node, "type", source)
                .or_else(|| self.first_named_child_text(node, source))
                .unwrap_or_else(|| "<impl>".into())
        } else {
            self.node_first_line(node, source)
        };

        symbols.push(Symbol {
            name,
            kind: sym_kind,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            signature: self.node_first_line(node, source),
        });
    }

    fn extract_js_ts(&self, node: &tree_sitter::Node, source: &[u8], symbols: &mut Vec<Symbol>) {
        let kind = node.kind();
        let (sym_kind, name_field) = match kind {
            "function_declaration" => (SymbolKind::Function, Some("name")),
            "class_declaration" => (SymbolKind::Class, Some("name")),
            "interface_declaration" => (SymbolKind::Interface, Some("name")),
            "type_alias_declaration" => (SymbolKind::TypeAlias, Some("name")),
            "import_statement" => (SymbolKind::Import, None),
            "export_statement" => {
                // Recurse into exported declarations
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.extract_js_ts(&child, source, symbols);
                }
                return;
            }
            "lexical_declaration" => {
                // Check for arrow functions: const foo = () => ...
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "variable_declarator"
                        && let Some(value) = child.child_by_field_name("value")
                        && value.kind() == "arrow_function"
                    {
                        let name = self
                            .child_text(&child, "name", source)
                            .unwrap_or_else(|| "<anonymous>".into());
                        symbols.push(Symbol {
                            name,
                            kind: SymbolKind::Function,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            signature: self.node_first_line(node, source),
                        });
                    }
                }
                return;
            }
            _ => return,
        };

        let name = if let Some(field) = name_field {
            self.child_text(node, field, source)
                .unwrap_or_else(|| "<anonymous>".into())
        } else {
            self.node_first_line(node, source)
        };

        symbols.push(Symbol {
            name,
            kind: sym_kind,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            signature: self.node_first_line(node, source),
        });
    }

    fn extract_python(&self, node: &tree_sitter::Node, source: &[u8], symbols: &mut Vec<Symbol>) {
        let kind = node.kind();
        let (sym_kind, name_field) = match kind {
            "function_definition" => (SymbolKind::Function, Some("name")),
            "class_definition" => (SymbolKind::Class, Some("name")),
            "import_statement" | "import_from_statement" => (SymbolKind::Import, None),
            "decorated_definition" => {
                // Recurse into the decorated function/class
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "function_definition" || child.kind() == "class_definition" {
                        self.extract_python(&child, source, symbols);
                    }
                }
                return;
            }
            _ => return,
        };

        let name = if let Some(field) = name_field {
            self.child_text(node, field, source)
                .unwrap_or_else(|| "<anonymous>".into())
        } else {
            self.node_first_line(node, source)
        };

        symbols.push(Symbol {
            name,
            kind: sym_kind,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            signature: self.node_first_line(node, source),
        });
    }

    /// Get text of a named child field.
    fn child_text(&self, node: &tree_sitter::Node, field: &str, source: &[u8]) -> Option<String> {
        node.child_by_field_name(field)
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.to_string())
    }

    /// Get text of the first named child.
    fn first_named_child_text(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        node.named_child(0)
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.to_string())
    }

    /// Get first line of a node's text (for signatures).
    fn node_first_line(&self, node: &tree_sitter::Node, source: &[u8]) -> String {
        node.utf8_text(source)
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or("")
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_intel::languages::LanguageRegistry;

    #[test]
    fn test_parse_rust() {
        let source = r#"
use std::collections::HashMap;

fn hello(name: &str) -> String {
    format!("Hello, {name}")
}

struct Config {
    port: u16,
}

enum Status {
    Active,
    Inactive,
}

trait Greet {
    fn greet(&self) -> String;
}

impl Greet for Config {
    fn greet(&self) -> String {
        "hi".into()
    }
}
"#;
        let mut parser = CodeParser::new();
        let entry = LanguageRegistry::get("rs").unwrap();
        let symbols = parser.parse(source, entry.language, entry.name).unwrap();

        assert!(
            symbols
                .iter()
                .any(|s| s.name == "hello" && s.kind == SymbolKind::Function)
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Config" && s.kind == SymbolKind::Struct)
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Status" && s.kind == SymbolKind::Enum)
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Greet" && s.kind == SymbolKind::Trait)
        );
        assert!(symbols.iter().any(|s| s.kind == SymbolKind::Impl));
        assert!(symbols.iter().any(|s| s.kind == SymbolKind::Import));
    }

    #[test]
    fn test_parse_javascript() {
        let source = r#"
import { foo } from 'bar';

function greet(name) {
    return `Hello, ${name}`;
}

class Widget {
    constructor(id) {
        this.id = id;
    }
}

const handler = () => {
    return true;
};
"#;
        let mut parser = CodeParser::new();
        let entry = LanguageRegistry::get("js").unwrap();
        let symbols = parser.parse(source, entry.language, entry.name).unwrap();

        assert!(
            symbols
                .iter()
                .any(|s| s.name == "greet" && s.kind == SymbolKind::Function)
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Widget" && s.kind == SymbolKind::Class)
        );
        assert!(symbols.iter().any(|s| s.kind == SymbolKind::Import));
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "handler" && s.kind == SymbolKind::Function)
        );
    }

    #[test]
    fn test_parse_python() {
        let source = r#"
import os
from sys import path

def hello():
    pass

class MyClass:
    def method(self):
        pass
"#;
        let mut parser = CodeParser::new();
        let entry = LanguageRegistry::get("py").unwrap();
        let symbols = parser.parse(source, entry.language, entry.name).unwrap();

        assert!(
            symbols
                .iter()
                .any(|s| s.name == "hello" && s.kind == SymbolKind::Function)
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "MyClass" && s.kind == SymbolKind::Class)
        );
        assert!(symbols.iter().any(|s| s.kind == SymbolKind::Import));
    }
}
