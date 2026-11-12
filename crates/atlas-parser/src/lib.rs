//! Tree-sitter integration and language adapters.

use atlas_domain::{
    Diagnostic, FileAnalysis, Import, ParseStatus, SourceRange, Symbol, SymbolKind,
};
use tree_sitter::{Node, Parser};

#[derive(Debug)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

pub trait LanguageAdapter: Sync {
    fn language_id(&self) -> &'static str;
    fn file_extensions(&self) -> &'static [&'static str];
    /// Syntax errors are not an `Err`: they come back as diagnostics.
    fn analyze(&self, path: &str, source: &str) -> Result<FileAnalysis, ParseError>;
}

static ADAPTERS: &[&dyn LanguageAdapter] = &[&TypeScriptAdapter];

/// Adapter for a file extension (no dot), or `None` if unsupported.
pub fn adapter_for(extension: &str) -> Option<&'static dyn LanguageAdapter> {
    ADAPTERS
        .iter()
        .copied()
        .find(|a| a.file_extensions().contains(&extension))
}

pub struct TypeScriptAdapter;

impl LanguageAdapter for TypeScriptAdapter {
    fn language_id(&self) -> &'static str {
        "typescript"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["ts", "tsx"]
    }

    fn analyze(&self, path: &str, source: &str) -> Result<FileAnalysis, ParseError> {
        let language = if path.ends_with(".tsx") {
            tree_sitter_typescript::LANGUAGE_TSX
        } else {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT
        };
        let mut parser = Parser::new();
        parser
            .set_language(&language.into())
            .map_err(|e| ParseError(e.to_string()))?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| ParseError(format!("parser returned no tree for {path}")))?;

        let src = source.as_bytes();
        let mut out = FileAnalysis {
            path: path.to_string(),
            language: self.language_id().to_string(),
            parse_status: ParseStatus::Parsed,
            symbols: Vec::new(),
            imports: Vec::new(),
            diagnostics: Vec::new(),
        };

        // Iterative preorder walk: untrusted source can nest deep enough to
        // overflow the stack with recursion.
        let mut cursor = tree.walk();
        let mut scopes: Vec<(usize, String)> = Vec::new();
        let mut depth = 0usize;
        'walk: loop {
            let node = cursor.node();
            while scopes.last().is_some_and(|(d, _)| *d >= depth) {
                scopes.pop();
            }
            visit(node, src, depth, &mut scopes, &mut out);

            if cursor.goto_first_child() {
                depth += 1;
                continue;
            }
            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() {
                    break 'walk;
                }
                depth -= 1;
            }
        }

        if !out.diagnostics.is_empty() {
            out.parse_status = ParseStatus::ParsedWithErrors;
        }
        Ok(out)
    }
}

fn visit(
    node: Node,
    src: &[u8],
    depth: usize,
    scopes: &mut Vec<(usize, String)>,
    out: &mut FileAnalysis,
) {
    if node.is_error() || node.is_missing() {
        let message = if node.is_missing() {
            format!("missing {}", node.kind())
        } else {
            "syntax error".to_string()
        };
        out.diagnostics.push(Diagnostic {
            message,
            range: range(node),
        });
        return;
    }

    if node.kind() == "import_statement" {
        if let Some(text) = node
            .child_by_field_name("source")
            .and_then(|s| s.utf8_text(src).ok())
        {
            out.imports.push(Import {
                source: text.trim_matches(['"', '\'']).to_string(),
                range: range(node),
            });
        }
        return;
    }

    let kind = match node.kind() {
        "function_declaration" | "generator_function_declaration" => SymbolKind::Function,
        "class_declaration" | "abstract_class_declaration" => SymbolKind::Class,
        "method_definition" => SymbolKind::Method,
        "interface_declaration" => SymbolKind::Interface,
        "enum_declaration" => SymbolKind::Enum,
        "type_alias_declaration" => SymbolKind::TypeAlias,
        // const f = () => ... / const f = function () ...
        "variable_declarator"
            if node.child_by_field_name("value").is_some_and(|v| {
                matches!(
                    v.kind(),
                    "arrow_function" | "function_expression" | "function"
                )
            }) =>
        {
            SymbolKind::Function
        }
        _ => return,
    };
    let Some(name) = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(src).ok())
    else {
        return;
    };

    let mut qualified: Vec<&str> = scopes.iter().map(|(_, n)| n.as_str()).collect();
    qualified.push(name);
    out.symbols.push(Symbol {
        name: name.to_string(),
        qualified_name: qualified.join("."),
        kind,
        range: range(node),
    });
    scopes.push((depth, name.to_string()));
}

fn range(node: Node) -> SourceRange {
    let (s, e) = (node.start_position(), node.end_position());
    // u32 is enough: a line or column past 4 billion is not source code.
    SourceRange {
        start_line: s.row as u32 + 1,
        start_column: s.column as u32 + 1,
        end_line: e.row as u32 + 1,
        end_column: e.column as u32 + 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze(src: &str) -> FileAnalysis {
        adapter_for("ts").unwrap().analyze("a.ts", src).unwrap()
    }

    #[test]
    fn extracts_symbols_imports_and_positions() {
        let a = analyze(
            "import { x } from './x';\n\
             export function top() {}\n\
             const arrow = () => 1;\n\
             class C { m() { function inner() {} } }\n\
             interface I {}\ntype T = number;\nenum E { A }\n",
        );
        let got: Vec<_> = a
            .symbols
            .iter()
            .map(|s| (s.qualified_name.as_str(), s.kind, s.range.start_line))
            .collect();
        assert_eq!(
            got,
            vec![
                ("top", SymbolKind::Function, 2),
                ("arrow", SymbolKind::Function, 3),
                ("C", SymbolKind::Class, 4),
                ("C.m", SymbolKind::Method, 4),
                ("C.m.inner", SymbolKind::Function, 4),
                ("I", SymbolKind::Interface, 5),
                ("T", SymbolKind::TypeAlias, 6),
                ("E", SymbolKind::Enum, 7),
            ]
        );
        assert_eq!(a.imports.len(), 1);
        assert_eq!(a.imports[0].source, "./x");
        assert_eq!(a.parse_status, ParseStatus::Parsed);
        // `function top() {}` starts after `export `.
        assert_eq!(a.symbols[0].range.start_column, 8);
    }

    #[test]
    fn malformed_source_yields_diagnostics_not_a_crash() {
        let a = analyze("function ok() {}\nfunction broken( {\n");
        assert_eq!(a.parse_status, ParseStatus::ParsedWithErrors);
        assert!(!a.diagnostics.is_empty());
        assert!(a.symbols.iter().any(|s| s.name == "ok"));
    }

    #[test]
    fn deep_nesting_does_not_overflow_the_stack() {
        let src = format!("const x = {}1{};", "(".repeat(20_000), ")".repeat(20_000));
        analyze(&src);
    }

    #[test]
    fn unsupported_extension_has_no_adapter() {
        assert!(adapter_for("py").is_none());
    }
}
