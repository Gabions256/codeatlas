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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Interface,
    Enum,
    TypeAlias,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    /// Enclosing symbol names joined with `.`, then the name.
    pub qualified_name: String,
    pub kind: SymbolKind,
    pub range: SourceRange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Import {
    /// Module specifier exactly as written, without quotes. Not resolved.
    pub source: String,
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
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisReport {
    pub engine_version: String,
    pub root: String,
    pub generated_at_unix: u64,
    pub files: Vec<FileAnalysis>,
    /// Files with no language adapter, or unreadable. Reasons go to stderr.
    pub skipped_files: u64,
}
