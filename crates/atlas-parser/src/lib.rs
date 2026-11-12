//! Tree-sitter integration and language adapters.
//!
//! All adapters are syntax-only. Each declares its limits in `capabilities`.

use std::collections::HashMap;

use atlas_domain::{
    fnv1a_hex, CallForm, CallSite, Capabilities, Diagnostic, FileAnalysis, Import, ParseStatus,
    SourceRange, Symbol, SymbolKind,
};
use tree_sitter::{Language, Node, Parser};

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
    fn capabilities(&self) -> Capabilities;
    /// Syntax errors are not an `Err`: they come back as diagnostics.
    fn analyze(&self, path: &str, source: &str) -> Result<FileAnalysis, ParseError>;
}

#[derive(Clone, Copy, PartialEq)]
enum Family {
    /// TypeScript, TSX, JavaScript, JSX.
    Script,
    Rust,
    Python,
}

struct SyntaxAdapter {
    id: &'static str,
    extensions: &'static [&'static str],
    family: Family,
    capabilities: Capabilities,
}

/// Replaces string literals in parameter defaults, which can hold secrets.
pub const REDACTED: &str = "\"<redacted>\"";

const CALLS: &str = "inferred from a unique name match; never verified";

static TYPESCRIPT: SyntaxAdapter = SyntaxAdapter {
    id: "typescript",
    extensions: &["ts", "tsx", "mts", "cts"],
    family: Family::Script,
    capabilities: Capabilities {
        syntax_extraction: true,
        export_detection: true,
        import_resolution: "relative specifiers to project files",
        call_resolution: CALLS,
    },
};

static JAVASCRIPT: SyntaxAdapter = SyntaxAdapter {
    id: "javascript",
    extensions: &["js", "jsx", "mjs", "cjs"],
    family: Family::Script,
    capabilities: Capabilities {
        syntax_extraction: true,
        export_detection: true,
        import_resolution: "relative specifiers to project files",
        call_resolution: CALLS,
    },
};

static RUST: SyntaxAdapter = SyntaxAdapter {
    id: "rust",
    extensions: &["rs"],
    family: Family::Rust,
    capabilities: Capabilities {
        syntax_extraction: true,
        export_detection: true,
        import_resolution: "none",
        call_resolution: CALLS,
    },
};

static PYTHON: SyntaxAdapter = SyntaxAdapter {
    id: "python",
    extensions: &["py"],
    family: Family::Python,
    capabilities: Capabilities {
        syntax_extraction: true,
        export_detection: false,
        import_resolution: "none",
        call_resolution: CALLS,
    },
};

static ADAPTERS: &[&dyn LanguageAdapter] = &[&TYPESCRIPT, &JAVASCRIPT, &RUST, &PYTHON];

pub fn adapters() -> &'static [&'static dyn LanguageAdapter] {
    ADAPTERS
}

/// Adapter for a file extension (no dot), or `None` if unsupported.
pub fn adapter_for(extension: &str) -> Option<&'static dyn LanguageAdapter> {
    ADAPTERS
        .iter()
        .copied()
        .find(|a| a.file_extensions().contains(&extension))
}

impl SyntaxAdapter {
    fn language(&self, path: &str) -> Language {
        match self.id {
            "typescript" if path.ends_with(".tsx") => tree_sitter_typescript::LANGUAGE_TSX.into(),
            "typescript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            "javascript" => tree_sitter_javascript::LANGUAGE.into(),
            "rust" => tree_sitter_rust::LANGUAGE.into(),
            _ => tree_sitter_python::LANGUAGE.into(),
        }
    }
}

impl LanguageAdapter for SyntaxAdapter {
    fn language_id(&self) -> &'static str {
        self.id
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        self.extensions
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    fn analyze(&self, path: &str, source: &str) -> Result<FileAnalysis, ParseError> {
        let mut parser = Parser::new();
        parser
            .set_language(&self.language(path))
            .map_err(|e| ParseError(e.to_string()))?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| ParseError(format!("parser returned no tree for {path}")))?;

        let mut walk = Walk {
            family: self.family,
            src: source.as_bytes(),
            scopes: Vec::new(),
            seen: HashMap::new(),
            exported_names: Vec::new(),
            esm: false,
            out: FileAnalysis {
                path: path.to_string(),
                language: self.id.to_string(),
                parse_status: ParseStatus::Parsed,
                symbols: Vec::new(),
                imports: Vec::new(),
                calls: Vec::new(),
                diagnostics: Vec::new(),
            },
        };

        // Iterative preorder walk: untrusted source can nest deep enough to
        // overflow the stack with recursion.
        let mut cursor = tree.walk();
        let mut depth = 0usize;
        'walk: loop {
            while walk.scopes.last().is_some_and(|s| s.depth >= depth) {
                walk.scopes.pop();
            }
            walk.visit(cursor.node(), depth);

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

        let (exported_names, esm) = (walk.exported_names, walk.esm);
        let mut out = walk.out;
        if self.family == Family::Script {
            // Only top-level script symbols hold `Some` at this point.
            for symbol in &mut out.symbols {
                if symbol.exported != Some(false) {
                    continue;
                }
                if exported_names.contains(&symbol.name) {
                    symbol.exported = Some(true);
                } else if !esm {
                    // No module syntax at all: may be CommonJS, where
                    // exports are assignments we do not read. Unknown.
                    symbol.exported = None;
                }
            }
        }
        if !out.diagnostics.is_empty() {
            out.parse_status = ParseStatus::ParsedWithErrors;
        }
        Ok(out)
    }
}

struct Scope {
    depth: usize,
    name: String,
    /// `None` for scopes that are not symbols (Rust `impl` blocks).
    id: Option<String>,
    is_type: bool,
}

struct Walk<'a> {
    family: Family,
    src: &'a [u8],
    scopes: Vec<Scope>,
    /// Occurrences of each id key, to keep duplicate declarations apart.
    seen: HashMap<String, u32>,
    /// Script only: names listed in `export { a }` or `export default a`.
    exported_names: Vec<String>,
    /// Script only: the file has an `import` or `export` statement.
    esm: bool,
    out: FileAnalysis,
}

impl<'a> Walk<'a> {
    fn text(&self, node: Node) -> Option<&'a str> {
        node.utf8_text(self.src).ok()
    }

    fn field_text(&self, node: Node, field: &str) -> Option<&'a str> {
        node.child_by_field_name(field).and_then(|n| self.text(n))
    }

    fn visit(&mut self, node: Node, depth: usize) {
        if node.is_error() || node.is_missing() {
            let message = if node.is_missing() {
                format!("missing {}", node.kind())
            } else {
                "syntax error".to_string()
            };
            self.out.diagnostics.push(Diagnostic {
                message,
                range: range(node),
            });
            return;
        }
        if self.import(node) || self.call(node) {
            return;
        }
        if self.family == Family::Script && node.kind() == "export_statement" {
            self.export_names(node);
            return;
        }
        if self.family == Family::Rust && node.kind() == "impl_item" {
            // `impl Foo<T>` scopes its methods under `Foo` without being a symbol.
            if let Some(ty) = self.field_text(node, "type") {
                let name = ty.split('<').next().unwrap_or(ty).trim().to_string();
                self.scopes.push(Scope {
                    depth,
                    name,
                    id: None,
                    is_type: true,
                });
            }
            return;
        }
        self.symbol(node, depth);
    }

    /// Records names exported apart from their declaration:
    /// `export { a, b as c }` and `export default a`.
    fn export_names(&mut self, node: Node) {
        self.esm = true;
        // `export { a } from './x'` re-exports another module's `a`.
        if node.child_by_field_name("source").is_some() {
            return;
        }
        if let Some(value) = node.child_by_field_name("value") {
            if value.kind() == "identifier" {
                self.exported_names
                    .extend(self.text(value).map(str::to_string));
            }
        }
        for clause in node.named_children(&mut node.walk()) {
            if clause.kind() != "export_clause" {
                continue;
            }
            for specifier in clause.named_children(&mut clause.walk()) {
                self.exported_names
                    .extend(self.field_text(specifier, "name").map(str::to_string));
            }
        }
    }

    /// `x.f()` callee: the name, and whether the receiver is the enclosing
    /// object. Python's `self` is a convention, not a keyword.
    fn member(&self, function: Node, name: &str, receiver: &str) -> Option<(&'a str, CallForm)> {
        let own = match self.family {
            Family::Script => "this",
            Family::Rust | Family::Python => "self",
        };
        let form = match function
            .child_by_field_name(receiver)
            .and_then(|r| self.text(r))
        {
            Some(text) if text == own => CallForm::SelfMember,
            _ => CallForm::Member,
        };
        self.field_text(function, name).map(|n| (n, form))
    }

    fn import(&mut self, node: Node) -> bool {
        if self.family == Family::Script && node.kind() == "import_statement" {
            self.esm = true;
        }
        let sources: Vec<&str> = match (self.family, node.kind()) {
            (Family::Script, "import_statement") => self
                .field_text(node, "source")
                .map(|s| s.trim_matches(['"', '\'', '`']))
                .into_iter()
                .collect(),
            (Family::Rust, "use_declaration") => {
                self.field_text(node, "argument").into_iter().collect()
            }
            (Family::Python, "import_from_statement") => {
                self.field_text(node, "module_name").into_iter().collect()
            }
            (Family::Python, "import_statement") => node
                .named_children(&mut node.walk())
                .filter_map(|child| match child.kind() {
                    "dotted_name" => self.text(child),
                    "aliased_import" => self.field_text(child, "name"),
                    _ => None,
                })
                .collect(),
            _ => return false,
        };
        for source in sources {
            self.out.imports.push(Import {
                source: source.to_string(),
                range: range(node),
            });
        }
        true
    }

    fn call(&mut self, node: Node) -> bool {
        let is_call = match self.family {
            Family::Python => node.kind() == "call",
            _ => node.kind() == "call_expression",
        };
        let is_new = self.family == Family::Script && node.kind() == "new_expression";
        if !is_call && !is_new {
            return false;
        }
        let field = if is_new { "constructor" } else { "function" };
        let Some(function) = node.child_by_field_name(field) else {
            return true;
        };
        let callee = match function.kind() {
            "identifier" => self.text(function).map(|n| (n, CallForm::Bare)),
            // x.f() in Script, Rust and Python respectively.
            "member_expression" => self.member(function, "property", "object"),
            "field_expression" => self.member(function, "field", "value"),
            "attribute" => self.member(function, "attribute", "object"),
            "scoped_identifier" => self
                .field_text(function, "name")
                .map(|n| (n, CallForm::Path)),
            // Computed, generic or immediately-invoked callees: not recorded.
            _ => None,
        };
        if let Some((name, form)) = callee {
            let form = if is_new { CallForm::New } else { form };
            self.out.calls.push(CallSite {
                caller_id: self.scopes.iter().rev().find_map(|s| s.id.clone()),
                callee: name.to_string(),
                form,
                range: range(node),
            });
        }
        true
    }

    fn symbol(&mut self, node: Node, depth: usize) {
        use SymbolKind::*;
        let kind = match (self.family, node.kind()) {
            (Family::Script, "function_declaration" | "generator_function_declaration") => Function,
            (Family::Script, "class_declaration" | "abstract_class_declaration") => Class,
            (Family::Script, "method_definition") => Method,
            (Family::Script, "interface_declaration") => Interface,
            (Family::Script, "enum_declaration") => Enum,
            (Family::Script, "type_alias_declaration") => TypeAlias,
            // const f = () => ... / const f = function () ...
            (Family::Script, "variable_declarator")
                if node.child_by_field_name("value").is_some_and(|v| {
                    matches!(
                        v.kind(),
                        "arrow_function" | "function_expression" | "function"
                    )
                }) =>
            {
                Function
            }
            (Family::Rust, "function_item" | "function_signature_item") => Function,
            (Family::Rust, "struct_item" | "union_item") => Struct,
            (Family::Rust, "enum_item") => Enum,
            (Family::Rust, "trait_item") => Trait,
            (Family::Rust, "mod_item") => Module,
            (Family::Rust, "type_item") => TypeAlias,
            (Family::Python, "function_definition") => Function,
            (Family::Python, "class_definition") => Class,
            _ => return,
        };
        let kind = if kind == Function && self.scopes.last().is_some_and(|s| s.is_type) {
            Method
        } else {
            kind
        };
        let Some(name) = self.field_text(node, "name") else {
            return;
        };

        let mut qualified: Vec<&str> = self.scopes.iter().map(|s| s.name.as_str()).collect();
        qualified.push(name);
        let qualified_name = qualified.join(".");

        let mut key = format!("{}\0{qualified_name}\0{kind:?}", self.out.path);
        let count = self.seen.entry(key.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            key.push_str(&format!("\0{count}"));
        }
        let id = fnv1a_hex(key.as_bytes());

        let (signature, parameters) = if matches!(kind, Function | Method) {
            self.signature(node)
        } else {
            (None, Vec::new())
        };

        self.out.symbols.push(Symbol {
            id: id.clone(),
            name: name.to_string(),
            qualified_name,
            kind,
            signature,
            parameters,
            exported: self.exported(node),
            range: range(node),
        });
        self.scopes.push(Scope {
            depth,
            name: name.to_string(),
            id: Some(id),
            is_type: kind.is_type(),
        });
    }

    fn signature(&self, node: Node) -> (Option<String>, Vec<String>) {
        // For `const f = () => ...` the function is the declarator's value.
        let function = if node.kind() == "variable_declarator" {
            node.child_by_field_name("value").unwrap_or(node)
        } else {
            node
        };
        let end = function
            .child_by_field_name("body")
            .map_or(node.end_byte(), |body| body.start_byte());
        let params = function.child_by_field_name("parameters");
        let secrets = match params {
            Some(p) if !p.has_error() => self.secret_spans(p),
            // A broken parameter list (often an unterminated string) cannot
            // be split reliably: hide everything from its first quote on.
            Some(p) => self.after_first_quote(p.start_byte(), p.end_byte()),
            None => self.after_first_quote(node.start_byte(), end),
        };
        let signature = self.redacted(node.start_byte(), end, &secrets).map(|text| {
            let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
            let trimmed = one_line.trim_end_matches("=>").trim_end();
            let trimmed = trimmed.trim_end_matches(['{', ':']).trim_end();
            // Cap pathological declarations; 300 chars covers real ones.
            trimmed.chars().take(300).collect::<String>()
        });

        let parameters = if let Some(list) = params {
            list.named_children(&mut list.walk())
                .filter(|p| !p.kind().contains("comment"))
                .filter_map(|p| self.redacted(p.start_byte(), p.end_byte(), &secrets))
                .collect()
        } else {
            // `x => x` has a single bare parameter.
            self.field_text(function, "parameter")
                .map(str::to_string)
                .into_iter()
                .collect()
        };
        (signature, parameters)
    }

    /// Byte spans inside a parameter list that must never be stored: every
    /// string literal outside a type annotation, plus anything the parser
    /// could not read (an unterminated string lands in an error node).
    /// Default values can hold API keys and passwords.
    fn secret_spans(&self, params: Node) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        let mut stack = vec![params];
        while let Some(node) = stack.pop() {
            let kind = node.kind();
            if matches!(kind, "type_annotation" | "type" | "literal_type") {
                continue;
            }
            if node.is_error()
                || matches!(
                    kind,
                    "string"
                        | "template_string"
                        | "concatenated_string"
                        | "string_literal"
                        | "raw_string_literal"
                )
            {
                spans.push((node.start_byte(), node.end_byte()));
                continue;
            }
            let mut cursor = node.walk();
            stack.extend(node.children(&mut cursor));
        }
        spans.sort_unstable();
        spans
    }

    /// Fallback when the syntax tree cannot be trusted: one span from the
    /// first quote character in `start..end` to the end (keeping a closing
    /// parenthesis so the signature still reads as one).
    fn after_first_quote(&self, start: usize, end: usize) -> Vec<(usize, usize)> {
        let Some(slice) = self.src.get(start..end) else {
            return Vec::new();
        };
        let Some(first) = slice.iter().position(|b| matches!(b, b'"' | b'\'' | b'`')) else {
            return Vec::new();
        };
        let stop = if slice.last() == Some(&b')') {
            end - 1
        } else {
            end
        };
        vec![(start + first, stop.max(start + first))]
    }

    /// Source text of `start..end` with every span in `secrets` replaced by
    /// a fixed marker.
    fn redacted(&self, start: usize, end: usize, secrets: &[(usize, usize)]) -> Option<String> {
        let mut out = String::new();
        let mut at = start;
        for &(from, to) in secrets {
            if to <= at || from >= end {
                continue;
            }
            out.push_str(std::str::from_utf8(self.src.get(at..from.max(at))?).ok()?);
            out.push_str(REDACTED);
            at = to.min(end);
        }
        out.push_str(std::str::from_utf8(self.src.get(at..end)?).ok()?);
        Some(out)
    }

    fn exported(&self, node: Node) -> Option<bool> {
        match self.family {
            Family::Script if self.scopes.is_empty() => {
                // declarator -> lexical_declaration -> export_statement
                let holder = if node.kind() == "variable_declarator" {
                    node.parent()
                } else {
                    Some(node)
                };
                Some(
                    holder
                        .and_then(|n| n.parent())
                        .is_some_and(|p| p.kind() == "export_statement"),
                )
            }
            Family::Rust => Some(
                node.children(&mut node.walk())
                    .any(|c| c.kind() == "visibility_modifier"),
            ),
            // Nested script symbols and all of Python: no export syntax.
            _ => None,
        }
    }
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

    fn analyze_as(file: &str, src: &str) -> FileAnalysis {
        let extension = file.rsplit('.').next().unwrap();
        adapter_for(extension).unwrap().analyze(file, src).unwrap()
    }

    fn analyze(src: &str) -> FileAnalysis {
        analyze_as("a.ts", src)
    }

    fn symbols(a: &FileAnalysis) -> Vec<(&str, SymbolKind)> {
        a.symbols
            .iter()
            .map(|s| (s.qualified_name.as_str(), s.kind))
            .collect()
    }

    fn calls(a: &FileAnalysis) -> Vec<(Option<&str>, &str, CallForm)> {
        a.calls
            .iter()
            .map(|c| {
                let caller = c.caller_id.as_ref().map(|id| {
                    let symbol = a.symbols.iter().find(|s| &s.id == id).unwrap();
                    symbol.qualified_name.as_str()
                });
                (caller, c.callee.as_str(), c.form)
            })
            .collect()
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
        assert!(adapter_for("go").is_none());
    }

    #[test]
    fn typescript_signatures_parameters_exports_and_calls() {
        let a = analyze(
            "export function pay(cart: Cart, tip = 0): number {\n  return total(cart) + log.info(tip);\n}\n\
             const twice = x => pay(x) * 2;\n\
             class S { run(): void { this.pay(); } }\n",
        );
        let pay = &a.symbols[0];
        assert_eq!(
            pay.signature.as_deref(),
            Some("function pay(cart: Cart, tip = 0): number")
        );
        assert_eq!(pay.parameters, ["cart: Cart", "tip = 0"]);
        assert_eq!(pay.exported, Some(true));

        let twice = &a.symbols[1];
        assert_eq!(twice.signature.as_deref(), Some("twice = x"));
        assert_eq!(twice.parameters, ["x"]);
        assert_eq!(twice.exported, Some(false));
        // Methods are not top-level: no export syntax applies.
        assert_eq!(a.symbols[3].exported, None);

        assert_eq!(
            calls(&a),
            [
                (Some("pay"), "total", CallForm::Bare),
                (Some("pay"), "info", CallForm::Member),
                (Some("twice"), "pay", CallForm::Bare),
                (Some("S.run"), "pay", CallForm::SelfMember),
            ]
        );
    }

    #[test]
    fn symbol_ids_survive_edits_but_not_renames() {
        let before = analyze("function a() {}\nfunction b() {}\n");
        let after = analyze("// moved down\n\nfunction a() { return 1; }\nfunction c() {}\n");
        assert_eq!(before.symbols[0].id, after.symbols[0].id);
        assert_ne!(before.symbols[1].id, after.symbols[1].id);
        // Same name, different file: different id.
        let other = analyze_as("b.ts", "function a() {}\n");
        assert_ne!(before.symbols[0].id, other.symbols[0].id);
        // Redeclarations in one file stay distinct.
        let twice = analyze("function a() {}\nfunction a() {}\n");
        assert_ne!(twice.symbols[0].id, twice.symbols[1].id);
    }

    #[test]
    fn javascript_and_jsx() {
        let a = analyze_as(
            "app.jsx",
            "import React from 'react';\n\
             export const App = () => <div>{render()}</div>;\n\
             function render() { return helper.make(); }\n",
        );
        assert_eq!(a.language, "javascript");
        assert_eq!(a.parse_status, ParseStatus::Parsed);
        assert_eq!(
            symbols(&a),
            [
                ("App", SymbolKind::Function),
                ("render", SymbolKind::Function)
            ]
        );
        assert_eq!(a.imports[0].source, "react");
        assert_eq!(
            calls(&a),
            [
                (Some("App"), "render", CallForm::Bare),
                (Some("render"), "make", CallForm::Member),
            ]
        );
    }

    #[test]
    fn rust_symbols_calls_and_visibility() {
        let a = analyze_as(
            "lib.rs",
            "use std::fmt::Display;\n\
             pub struct Cart { n: u32 }\n\
             impl Cart {\n    pub fn new(n: u32) -> Self { check(n); Cart { n } }\n    fn size(&self) -> u32 { self.n }\n}\n\
             trait Price { fn price(&self) -> u32; }\n\
             mod inner { pub fn check() {} }\n\
             fn check(n: u32) { let c = Cart::new(n); c.size(); }\n",
        );
        assert_eq!(a.parse_status, ParseStatus::Parsed);
        assert_eq!(
            symbols(&a),
            [
                ("Cart", SymbolKind::Struct),
                ("Cart.new", SymbolKind::Method),
                ("Cart.size", SymbolKind::Method),
                ("Price", SymbolKind::Trait),
                ("Price.price", SymbolKind::Method),
                ("inner", SymbolKind::Module),
                ("inner.check", SymbolKind::Function),
                ("check", SymbolKind::Function),
            ]
        );
        assert_eq!(a.imports[0].source, "std::fmt::Display");
        let new = &a.symbols[1];
        assert_eq!(new.signature.as_deref(), Some("pub fn new(n: u32) -> Self"));
        assert_eq!(new.parameters, ["n: u32"]);
        assert_eq!(new.exported, Some(true));
        assert_eq!(a.symbols[2].exported, Some(false));
        assert_eq!(a.symbols[2].parameters, ["&self"]);
        assert_eq!(
            calls(&a),
            [
                (Some("Cart.new"), "check", CallForm::Bare),
                (Some("check"), "new", CallForm::Path),
                (Some("check"), "size", CallForm::Member),
            ]
        );
    }

    #[test]
    fn python_symbols_imports_and_calls() {
        let a = analyze_as(
            "shop.py",
            "import os, json as j\nfrom .models import Cart\n\n\
             class Shop:\n    def pay(self, cart, tip=0):\n        return total(cart) + self.log(tip)\n\n\
             def total(cart):\n    return len(cart)\n",
        );
        assert_eq!(a.parse_status, ParseStatus::Parsed);
        assert_eq!(
            symbols(&a),
            [
                ("Shop", SymbolKind::Class),
                ("Shop.pay", SymbolKind::Method),
                ("total", SymbolKind::Function),
            ]
        );
        let sources: Vec<_> = a.imports.iter().map(|i| i.source.as_str()).collect();
        assert_eq!(sources, ["os", "json", ".models"]);
        let pay = &a.symbols[1];
        assert_eq!(pay.signature.as_deref(), Some("def pay(self, cart, tip=0)"));
        assert_eq!(pay.parameters, ["self", "cart", "tip=0"]);
        assert_eq!(pay.exported, None);
        assert_eq!(
            calls(&a),
            [
                (Some("Shop.pay"), "total", CallForm::Bare),
                (Some("Shop.pay"), "log", CallForm::SelfMember),
                (Some("total"), "len", CallForm::Bare),
            ]
        );
    }

    #[test]
    fn exports_named_apart_from_the_declaration() {
        let a = analyze(
            "function a() {}\nfunction b() {}\nfunction c() {}\nfunction d() {}\nfunction e() {}\n\
             export { a, b as other };\nexport default c;\nexport { e } from './e';\n",
        );
        let exported: Vec<_> = a
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.exported))
            .collect();
        assert_eq!(
            exported,
            [
                ("a", Some(true)),
                ("b", Some(true)),
                ("c", Some(true)),
                ("d", Some(false)),
                // Re-exported from another module: not this `e`.
                ("e", Some(false)),
            ]
        );
    }

    #[test]
    fn files_without_module_syntax_do_not_claim_not_exported() {
        let cjs = analyze_as("x.js", "function used() {}\nmodule.exports = { used };\n");
        assert_eq!(cjs.symbols[0].exported, None);
        // One import is enough to make `export` the only way out.
        let esm = analyze_as("y.js", "import './x';\nfunction hidden() {}\n");
        assert_eq!(esm.symbols[0].exported, Some(false));
    }

    #[test]
    fn receivers_and_constructors() {
        let forms = |file: &str, src: &str| -> Vec<(String, CallForm)> {
            let a = analyze_as(file, src);
            a.calls.iter().map(|c| (c.callee.clone(), c.form)).collect()
        };
        let n = |form| ("n".to_string(), form);
        assert_eq!(
            forms(
                "a.ts",
                "class K { m() { this.n(); other.n(); self.n(); new K(); new ns.K(1); } n() {} }"
            ),
            [
                n(CallForm::SelfMember),
                n(CallForm::Member),
                // `self` is an ordinary variable in scripts.
                n(CallForm::Member),
                ("K".to_string(), CallForm::New),
                ("K".to_string(), CallForm::New),
            ]
        );
        assert_eq!(
            forms(
                "a.rs",
                "struct S; impl S { fn m(&self, o: S) { self.n(); o.n(); } fn n(&self) {} }"
            ),
            [n(CallForm::SelfMember), n(CallForm::Member)]
        );
        assert_eq!(
            forms(
                "a.py",
                "class C:\n    def m(self, o):\n        self.n()\n        o.n()\n"
            ),
            [n(CallForm::SelfMember), n(CallForm::Member)]
        );
    }

    /// Documented blind spot: macro arguments are unparsed token trees.
    #[test]
    fn rust_calls_inside_macros_are_not_seen() {
        let a = analyze_as(
            "a.rs",
            "fn h() -> u32 { 1 }\nfn main() { println!(\"{}\", h()); }\n",
        );
        assert!(a.calls.is_empty());
    }

    fn leaks(a: &FileAnalysis, secret: &str) -> bool {
        a.symbols.iter().any(|s| {
            s.signature.as_deref().is_some_and(|t| t.contains(secret))
                || s.parameters.iter().any(|p| p.contains(secret))
        })
    }

    #[test]
    fn python_string_defaults_are_redacted() {
        let a = analyze_as(
            "a.py",
            "def f(api_key=\"sk-live-123\", tip=0, *, mode=f'x{1}', b=b'raw', n=None):\n    pass\n\
             def g(t: Literal[\"a\"] = 'sk-live-123'):\n    pass\n",
        );
        assert!(!leaks(&a, "sk-live-123"));
        let f = &a.symbols[0];
        assert_eq!(
            f.signature.as_deref(),
            Some(
                "def f(api_key=\"<redacted>\", tip=0, *, mode=\"<redacted>\", \
                 b=\"<redacted>\", n=None)"
            )
        );
        assert_eq!(f.parameters[0], "api_key=\"<redacted>\"");
        assert_eq!(f.parameters[1], "tip=0");
        // The type annotation is not a default and stays readable.
        assert_eq!(
            a.symbols[1].parameters,
            ["t: Literal[\"a\"] = \"<redacted>\""]
        );
    }

    #[test]
    fn script_string_defaults_are_redacted() {
        for file in ["a.ts", "a.js"] {
            let a = analyze_as(
                file,
                "function f(token = \"sk-live-123\", n = 2) { return n; }\n\
                 const g = (k = 'sk-live-123') => k;\n\
                 class C { m(t = `sk-live-123${1}`) {} }\n",
            );
            assert!(!leaks(&a, "sk-live-123"), "{file}");
            assert_eq!(
                a.symbols[0].signature.as_deref(),
                Some("function f(token = \"<redacted>\", n = 2)"),
                "{file}"
            );
            assert_eq!(a.symbols[1].parameters, ["k = \"<redacted>\""], "{file}");
        }
        // TypeScript literal types are types, not defaults.
        let a = analyze_as("a.ts", "function h(m: \"on\" | \"off\" = \"on\") {}\n");
        assert_eq!(
            a.symbols[0].parameters,
            ["m: \"on\" | \"off\" = \"<redacted>\""]
        );
    }

    #[test]
    fn malformed_string_defaults_do_not_leak() {
        for (file, src) in [
            ("a.py", "def f(key=\"sk-live-123):\n    pass\n"),
            ("a.py", "def f(key='sk-live-123', x=):\n    pass\n"),
            ("a.ts", "function f(key = \"sk-live-123) { }\n"),
            ("a.js", "function f(key = 'sk-live-123', ) { return }}\n"),
            ("a.rs", "fn f(key: &str = \"sk-live-123\") {}\n"),
        ] {
            let a = analyze_as(file, src);
            assert!(!leaks(&a, "sk-live-123"), "{file}: {:?}", a.symbols);
        }
    }

    #[test]
    fn every_adapter_declares_capabilities() {
        for adapter in adapters() {
            let caps = adapter.capabilities();
            assert!(caps.syntax_extraction);
            assert!(!caps.call_resolution.is_empty());
        }
        assert!(!adapter_for("py").unwrap().capabilities().export_detection);
    }
}
