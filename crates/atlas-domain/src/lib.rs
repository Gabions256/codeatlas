//! Core domain types. No UI, database or AI dependencies.
//!
//! Everything here is syntax-level evidence: it says code exists at a source
//! range, not that it works or is ever executed.

use serde::{Deserialize, Serialize};

/// Lines and columns are 1-based. Columns count bytes, as Tree-sitter does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRange {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

/// How a claim was established. Every reported symbol, import and call
/// carries one. A weaker class must never be shown as a stronger one.
///
/// Only the first three are produced today: nothing in CodeAtlas runs
/// code, reads test results or calls an AI provider yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Evidence {
    /// Read from the source text: it exists at the stated position.
    Syntax,
    /// Derived from the source by a rule, such as file-path conventions.
    StaticAnalysis,
    /// A guess from names. Not type-checked.
    Inferred,
    RuntimeObserved,
    TestResult,
    AiSuggestion,
}

impl Evidence {
    pub fn label(self) -> &'static str {
        match self {
            Self::Syntax => "syntax",
            Self::StaticAnalysis => "static analysis",
            Self::Inferred => "inferred",
            Self::RuntimeObserved => "runtime observed",
            Self::TestResult => "test result",
            Self::AiSuggestion => "AI suggestion",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Interface,
    Trait,
    Module,
    Enum,
    TypeAlias,
}

impl SymbolKind {
    /// Symbols whose nested functions are methods.
    pub fn is_type(self) -> bool {
        matches!(
            self,
            Self::Class | Self::Struct | Self::Interface | Self::Trait
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    /// Hash of file path, qualified name and kind. Survives edits to the
    /// body and moves within the file; changes on rename or file move.
    pub id: String,
    pub name: String,
    /// Enclosing symbol names joined with `.`, then the name.
    pub qualified_name: String,
    pub kind: SymbolKind,
    /// Declaration text up to the body. Functions and methods only.
    pub signature: Option<String>,
    /// Parameter source text, one entry per parameter.
    pub parameters: Vec<String>,
    /// Hash of the declaration text before redaction, so a change to a
    /// redacted default still counts as a signature change. Kept in memory
    /// only: never serialized or stored, so it cannot be used to guess the
    /// hidden value from the index.
    #[serde(skip)]
    pub signature_digest: Option<String>,
    /// Script: an `export` names this symbol. Rust: it has a `pub` modifier,
    /// which says nothing about whether it is reachable from outside.
    /// `None` when the syntax cannot tell: nested symbols, Python, and
    /// script files with no `import` or `export` (possibly CommonJS).
    pub exported: Option<bool>,
    pub range: SourceRange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Import {
    /// Module specifier exactly as written, without quotes. Not resolved.
    pub source: String,
    pub range: SourceRange,
}

/// How a call names its target. Decides which symbol kinds may match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallForm {
    /// `f()`
    Bare,
    /// `x.f()`: the receiver's type is unknown, so it is never linked.
    Member,
    /// `this.f()` in scripts, `self.f()` in Rust and Python.
    SelfMember,
    /// `new X()` in scripts.
    New,
    /// `a::f()`: a module function or an associated function.
    Path,
}

/// A call expression as written. Says nothing about what runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallSite {
    /// Id of the innermost enclosing symbol, if any.
    pub caller_id: Option<String>,
    /// Last name segment of the callee.
    pub callee: String,
    pub form: CallForm,
    pub range: SourceRange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub message: String,
    pub range: SourceRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParseStatus {
    Parsed,
    /// Symbols were still extracted, but the tree contains error nodes.
    ParsedWithErrors,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileAnalysis {
    /// Relative to the project root, `/` separated.
    pub path: String,
    pub language: String,
    pub parse_status: ParseStatus,
    pub symbols: Vec<Symbol>,
    pub imports: Vec<Import>,
    pub calls: Vec<CallSite>,
    pub diagnostics: Vec<Diagnostic>,
}

/// What an adapter can actually establish. Adapters must not claim more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    /// Symbols, signatures, parameters, imports and call sites are extracted.
    pub syntax_extraction: bool,
    /// `exported` is filled in from export or visibility syntax.
    pub export_detection: bool,
    /// Import specifiers are resolved to project files.
    pub import_resolution: &'static str,
    /// How call sites are linked to symbols.
    pub call_resolution: &'static str,
}

/// FNV-1a 64-bit, as hex. Used for symbol ids and change detection.
// shortcut: 64-bit non-cryptographic hash, a collision would hide one file
// change or merge two ids. Upgrade to SHA-256 if the index is ever shared or
// trusted across machines.
pub fn fnv1a_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_matches_published_vectors() {
        assert_eq!(fnv1a_hex(b""), "cbf29ce484222325");
        assert_eq!(fnv1a_hex(b"a"), "af63dc4c8601ec8c");
    }
}
