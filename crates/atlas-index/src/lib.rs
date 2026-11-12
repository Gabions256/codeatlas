//! The shared engine: discovery, incremental SQLite index, relationship
//! resolution and queries. CLI, MCP and desktop all go through `Index`.
//!
//! Every row that states a relationship carries its source location (file,
//! line, column), a status and a note saying why. Nothing here is observed
//! at runtime: the strongest call status is `Inferred`.

mod html;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use atlas_domain::{fnv1a_hex, Capabilities, FileAnalysis};
use atlas_parser::LanguageAdapter;
use rusqlite::{params, Connection, Row, Transaction};
use serde::Serialize;

pub use html::render as render_html;

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error(format!("index database: {e}"))
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Directories never entered, whatever the ignore files say.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".codeatlas",
    "node_modules",
    "target",
    "dist",
    "build",
    "__pycache__",
    ".venv",
    "venv",
];

const INDEX_DIR: &str = ".codeatlas";
/// Project-specific ignore file, same syntax as `.gitignore`.
pub const IGNORE_FILE: &str = ".codeatlasignore";
const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "
DROP TABLE IF EXISTS diagnostics;
DROP TABLE IF EXISTS calls;
DROP TABLE IF EXISTS imports;
DROP TABLE IF EXISTS symbols;
DROP TABLE IF EXISTS files;
CREATE TABLE files (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    language TEXT NOT NULL,
    hash TEXT NOT NULL,
    parse_status TEXT NOT NULL
);
CREATE TABLE symbols (
    id TEXT PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    qualified_name TEXT NOT NULL,
    kind TEXT NOT NULL,
    signature TEXT,
    parameters TEXT NOT NULL,
    exported INTEGER,
    start_line INTEGER NOT NULL,
    start_column INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    end_column INTEGER NOT NULL
);
CREATE INDEX symbols_name ON symbols(name);
CREATE INDEX symbols_file ON symbols(file_id);
CREATE TABLE imports (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    source TEXT NOT NULL,
    line INTEGER NOT NULL,
    target_file_id INTEGER REFERENCES files(id) ON DELETE SET NULL
);
CREATE INDEX imports_file ON imports(file_id);
CREATE TABLE calls (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    caller_id TEXT,
    callee TEXT NOT NULL,
    form TEXT NOT NULL,
    line INTEGER NOT NULL,
    col INTEGER NOT NULL,
    target_id TEXT,
    status TEXT NOT NULL DEFAULT 'Unresolved',
    note TEXT NOT NULL DEFAULT ''
);
CREATE INDEX calls_caller ON calls(caller_id);
CREATE INDEX calls_target ON calls(target_id);
CREATE TABLE diagnostics (
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    message TEXT NOT NULL,
    line INTEGER NOT NULL,
    col INTEGER NOT NULL
);
CREATE INDEX diagnostics_file ON diagnostics(file_id);
";

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct ScanSummary {
    pub files: u64,
    pub reparsed: u64,
    pub unchanged: u64,
    pub removed: u64,
    /// Files with no language adapter.
    pub unsupported: u64,
    pub symbols: u64,
    pub calls: u64,
    pub calls_inferred: u64,
    pub imports: u64,
    pub imports_resolved: u64,
    /// Supported files that could not be read or parsed, and ignore-file
    /// rules that could not be parsed, with the reason.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SymbolRow {
    pub id: String,
    pub name: String,
    pub qualified_name: String,
    pub kind: String,
    pub signature: Option<String>,
    pub parameters: Vec<String>,
    pub exported: Option<bool>,
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
}

/// One call site. `path:line:column` is the evidence for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Edge {
    pub from_id: Option<String>,
    pub from: String,
    /// `None` when the call is unresolved; `to` is then the name as written.
    pub to_id: Option<String>,
    pub to: String,
    /// `Inferred` or `Unresolved`. Never observed at runtime.
    pub status: String,
    pub note: String,
    pub path: String,
    pub line: u32,
    pub column: u32,
    /// Hops from the queried symbol, starting at 1.
    pub depth: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Dependencies,
    Dependents,
}

#[derive(Debug, Serialize)]
pub struct Explanation {
    pub symbol: SymbolRow,
    pub summary: String,
    pub calls: Vec<Edge>,
    pub called_by: Vec<Edge>,
    pub limitations: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct LanguageInfo {
    pub id: &'static str,
    pub extensions: &'static [&'static str],
    pub capabilities: Capabilities,
}

#[derive(Debug, Serialize)]
pub struct ImportRow {
    pub source: String,
    pub line: u32,
    /// Project file the specifier resolves to, when the adapter can tell.
    pub resolved_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticRow {
    pub message: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Serialize)]
pub struct FileReport {
    pub path: String,
    pub language: String,
    pub parse_status: String,
    pub symbols: Vec<SymbolRow>,
    pub imports: Vec<ImportRow>,
    pub diagnostics: Vec<DiagnosticRow>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub engine_version: &'static str,
    pub root: String,
    pub generated_at_unix: u64,
    pub languages: Vec<LanguageInfo>,
    pub files: Vec<FileReport>,
    pub calls: Vec<Edge>,
}

const LIMITATIONS: &[&str] = &[
    "Structure only: no AI provider is configured, so this does not say what the code does.",
    "Calls are linked by name. `Inferred` means exactly one candidate matched, not that it runs.",
    "`Unresolved` calls may target libraries, built-ins, or code reached dynamically.",
    "Callers that use dynamic dispatch, reflection or string lookups are not found.",
];

const SYMBOL_SELECT: &str = "SELECT s.id, s.name, s.qualified_name, s.kind, s.signature, \
     s.parameters, s.exported, f.path, s.start_line, s.end_line \
     FROM symbols s JOIN files f ON f.id = s.file_id";

const EDGE_SELECT: &str = "SELECT c.caller_id, COALESCE(cs.qualified_name, '<file scope>'), \
     c.target_id, COALESCE(ts.qualified_name, c.callee), c.status, c.note, f.path, c.line, c.col \
     FROM calls c JOIN files f ON f.id = c.file_id \
     LEFT JOIN symbols cs ON cs.id = c.caller_id \
     LEFT JOIN symbols ts ON ts.id = c.target_id";

/// Bound on edges returned by one graph query.
const MAX_EDGES: usize = 5000;

pub struct Index {
    conn: Connection,
    root: PathBuf,
}

impl Index {
    /// True if `root` has been scanned before.
    pub fn exists(root: &Path) -> bool {
        root.join(INDEX_DIR).join("index.db").is_file()
    }

    /// Opens the index under `root/.codeatlas`, creating it if needed.
    pub fn open(root: &Path) -> Result<Self> {
        if !root.is_dir() {
            return Err(Error(format!("{} is not a directory", root.display())));
        }
        let dir = root.join(INDEX_DIR);
        std::fs::create_dir_all(&dir)?;
        // Keeps the index out of the scanned project's version control.
        let ignore = dir.join(".gitignore");
        if !ignore.exists() {
            std::fs::write(ignore, "*\n")?;
        }
        let conn = Connection::open(dir.join("index.db"))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != SCHEMA_VERSION {
            // shortcut: the index is a rebuildable cache, so a schema change
            // drops it instead of migrating. Add real migrations once the
            // index holds anything a rescan cannot recreate.
            conn.execute_batch(SCHEMA)?;
            conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        }
        Ok(Self {
            conn,
            root: root.to_path_buf(),
        })
    }

    /// Brings the index in line with the files on disk. Only files whose
    /// content hash changed are reparsed. One transaction: an interrupted
    /// scan leaves the previous index intact.
    pub fn scan(&mut self) -> Result<ScanSummary> {
        let discovered = discover(&self.root)?;
        let mut summary = ScanSummary {
            unsupported: discovered.unsupported,
            warnings: discovered.warnings,
            ..ScanSummary::default()
        };

        let tx = self.conn.transaction()?;
        let mut known: HashMap<String, (i64, String)> = HashMap::new();
        {
            let mut stmt = tx.prepare("SELECT path, id, hash FROM files")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?;
            for row in rows {
                let (path, entry) = row?;
                known.insert(path, entry);
            }
        }

        for (relative, absolute, adapter) in discovered.found {
            let previous = known.remove(&relative);
            let analysis = std::fs::read(&absolute)
                .map_err(|e| e.to_string())
                .and_then(|bytes| {
                    let hash = format!("{}:{}", fnv1a_hex(&bytes), bytes.len());
                    if previous.as_ref().is_some_and(|(_, old)| *old == hash) {
                        return Ok(None);
                    }
                    let source = String::from_utf8(bytes).map_err(|_| "not UTF-8".to_string())?;
                    let analysis = adapter
                        .analyze(&relative, &source)
                        .map_err(|e| e.to_string())?;
                    Ok(Some((analysis, hash)))
                });
            match analysis {
                Ok(None) => {
                    summary.unchanged += 1;
                    continue;
                }
                Ok(Some((analysis, hash))) => {
                    delete_file(&tx, previous)?;
                    insert_file(&tx, &analysis, &hash)?;
                    summary.reparsed += 1;
                }
                Err(reason) => {
                    // Drop any stale entry rather than keep describing a
                    // file we can no longer read.
                    delete_file(&tx, previous)?;
                    summary.warnings.push(format!("{relative}: {reason}"));
                }
            }
        }
        for (_, entry) in known {
            delete_file(&tx, Some(entry))?;
            summary.removed += 1;
        }

        resolve(&tx)?;

        // COUNT(*) is never negative.
        let count = |sql: &str| tx.query_row(sql, [], |r| Ok(r.get::<_, i64>(0)? as u64));
        summary.files = count("SELECT COUNT(*) FROM files")?;
        summary.symbols = count("SELECT COUNT(*) FROM symbols")?;
        summary.calls = count("SELECT COUNT(*) FROM calls")?;
        summary.calls_inferred = count("SELECT COUNT(*) FROM calls WHERE target_id IS NOT NULL")?;
        summary.imports = count("SELECT COUNT(*) FROM imports")?;
        summary.imports_resolved =
            count("SELECT COUNT(*) FROM imports WHERE target_file_id IS NOT NULL")?;
        tx.commit()?;
        Ok(summary)
    }

    /// Symbols whose name contains `query`, optionally of one kind.
    pub fn symbols(&self, query: Option<&str>, kind: Option<&str>) -> Result<Vec<SymbolRow>> {
        let sql = format!(
            "{SYMBOL_SELECT} WHERE (?1 IS NULL OR instr(lower(s.name), lower(?1)) > 0) \
             AND (?2 IS NULL OR s.kind = ?2 COLLATE NOCASE) ORDER BY f.path, s.start_line"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![query, kind], symbol_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Symbols matching an id, qualified name or plain name exactly.
    pub fn find(&self, name: &str) -> Result<Vec<SymbolRow>> {
        let sql = format!(
            "{SYMBOL_SELECT} WHERE s.id = ?1 OR s.qualified_name = ?1 OR s.name = ?1 \
             ORDER BY f.path, s.start_line"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([name], symbol_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn edges(&self, filter: &str, id: Option<&str>, depth: u32) -> Result<Vec<Edge>> {
        let sql = format!("{EDGE_SELECT} WHERE {filter} ORDER BY f.path, c.line, c.col");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([id], |r| {
            Ok(Edge {
                from_id: r.get(0)?,
                from: r.get(1)?,
                to_id: r.get(2)?,
                to: r.get(3)?,
                status: r.get(4)?,
                note: r.get(5)?,
                path: r.get(6)?,
                line: r.get(7)?,
                column: r.get(8)?,
                depth,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Call edges. With no symbol: every call site in the project. With a
    /// symbol: what it calls (or what calls it), followed up to `depth` hops.
    pub fn graph(
        &self,
        symbol: Option<&str>,
        direction: Direction,
        depth: u32,
    ) -> Result<Vec<Edge>> {
        let Some(name) = symbol else {
            return self.edges("?1 IS NULL", None, 1);
        };
        let roots = self.find(name)?;
        if roots.is_empty() {
            return Err(Error(format!("no symbol named `{name}` in the index")));
        }
        let filter = match direction {
            Direction::Dependencies => "c.caller_id = ?1",
            Direction::Dependents => "c.target_id = ?1",
        };
        let mut frontier: Vec<String> = roots.into_iter().map(|s| s.id).collect();
        let mut visited: HashSet<String> = frontier.iter().cloned().collect();
        let mut out = Vec::new();
        for level in 1..=depth.max(1) {
            let mut next = Vec::new();
            for id in &frontier {
                for edge in self.edges(filter, Some(id), level)? {
                    let onward = match direction {
                        Direction::Dependencies => edge.to_id.clone(),
                        Direction::Dependents => edge.from_id.clone(),
                    };
                    if let Some(onward) = onward {
                        if visited.insert(onward.clone()) {
                            next.push(onward);
                        }
                    }
                    out.push(edge);
                    if out.len() >= MAX_EDGES {
                        return Ok(out);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        Ok(out)
    }

    /// Deterministic, evidence-only description of each function or method
    /// called `name`. Empty if there is none.
    pub fn explain_function(&self, name: &str) -> Result<Vec<Explanation>> {
        let mut out = Vec::new();
        for symbol in self.find(name)? {
            if symbol.kind != "Function" && symbol.kind != "Method" {
                continue;
            }
            let calls = self.edges("c.caller_id = ?1", Some(&symbol.id), 1)?;
            let called_by = self.edges("c.target_id = ?1", Some(&symbol.id), 1)?;
            let summary = summarize(&symbol, &calls, &called_by);
            out.push(Explanation {
                symbol,
                summary,
                calls,
                called_by,
                limitations: LIMITATIONS.to_vec(),
            });
        }
        Ok(out)
    }

    pub fn report(&self) -> Result<Report> {
        let mut files = Vec::new();
        let mut stmt = self
            .conn
            .prepare("SELECT id, path, language, parse_status FROM files ORDER BY path")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                FileReport {
                    path: r.get(1)?,
                    language: r.get(2)?,
                    parse_status: r.get(3)?,
                    symbols: Vec::new(),
                    imports: Vec::new(),
                    diagnostics: Vec::new(),
                },
            ))
        })?;
        let mut symbols = self.conn.prepare(&format!(
            "{SYMBOL_SELECT} WHERE s.file_id = ?1 ORDER BY s.start_line, s.start_column"
        ))?;
        let mut imports = self.conn.prepare(
            "SELECT i.source, i.line, t.path FROM imports i \
             LEFT JOIN files t ON t.id = i.target_file_id WHERE i.file_id = ?1 ORDER BY i.line",
        )?;
        let mut diagnostics = self.conn.prepare(
            "SELECT message, line, col FROM diagnostics WHERE file_id = ?1 ORDER BY line, col",
        )?;
        for row in rows {
            let (id, mut file) = row?;
            file.symbols = symbols
                .query_map([id], symbol_row)?
                .collect::<rusqlite::Result<_>>()?;
            file.imports = imports
                .query_map([id], |r| {
                    Ok(ImportRow {
                        source: r.get(0)?,
                        line: r.get(1)?,
                        resolved_path: r.get(2)?,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
            file.diagnostics = diagnostics
                .query_map([id], |r| {
                    Ok(DiagnosticRow {
                        message: r.get(0)?,
                        line: r.get(1)?,
                        column: r.get(2)?,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
            files.push(file);
        }
        Ok(Report {
            engine_version: env!("CARGO_PKG_VERSION"),
            root: self.root.to_string_lossy().into_owned(),
            generated_at_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            languages: atlas_parser::adapters()
                .iter()
                .map(|a| LanguageInfo {
                    id: a.language_id(),
                    extensions: a.file_extensions(),
                    capabilities: a.capabilities(),
                })
                .collect(),
            files,
            calls: self.edges("?1 IS NULL", None, 1)?,
        })
    }
}

fn symbol_row(r: &Row) -> rusqlite::Result<SymbolRow> {
    let parameters: String = r.get(5)?;
    Ok(SymbolRow {
        id: r.get(0)?,
        name: r.get(1)?,
        qualified_name: r.get(2)?,
        kind: r.get(3)?,
        signature: r.get(4)?,
        // Written by insert_file as a JSON array; anything else is corruption.
        parameters: serde_json::from_str(&parameters).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
        })?,
        exported: r.get(6)?,
        path: r.get(7)?,
        start_line: r.get(8)?,
        end_line: r.get(9)?,
    })
}

fn summarize(symbol: &SymbolRow, calls: &[Edge], called_by: &[Edge]) -> String {
    let visibility = match symbol.exported {
        Some(true) => ", exported",
        Some(false) => ", not exported",
        None => "",
    };
    let mut text = format!(
        "`{}` ({}{visibility}) is declared in {} at lines {}-{}.",
        symbol.qualified_name,
        symbol.kind.to_lowercase(),
        symbol.path,
        symbol.start_line,
        symbol.end_line
    );
    match symbol.parameters.as_slice() {
        [] => text.push_str(" It takes no parameters."),
        params => text.push_str(&format!(
            " It takes {} parameter(s): {}.",
            params.len(),
            params.join(", ")
        )),
    }
    let inferred = calls.iter().filter(|e| e.to_id.is_some()).count();
    text.push_str(&format!(
        " Its body contains {} call site(s): {inferred} linked to a project symbol by name, {} unresolved.",
        calls.len(),
        calls.len() - inferred
    ));
    text.push_str(&format!(
        " {} call site(s) elsewhere in the project are linked to it by name.",
        called_by.len()
    ));
    text
}

type Found = (String, PathBuf, &'static dyn LanguageAdapter);

struct Discovered {
    /// Supported source files, sorted by relative path.
    found: Vec<Found>,
    /// Files no adapter handles. Ignored files are not counted.
    unsupported: u64,
    /// Ignore-file rules that could not be parsed and were skipped.
    warnings: Vec<String>,
}

/// Walks `root`, honouring `.gitignore` and `.codeatlasignore` files inside
/// it. Ignore files above the root, the global git excludes and
/// `.git/info/exclude` are not read, so the same tree gives the same index on
/// every machine. `SKIP_DIRS` are skipped even if an ignore file
/// re-includes them.
fn discover(root: &Path) -> Result<Discovered> {
    let mut out = Discovered {
        found: Vec::new(),
        unsupported: 0,
        warnings: Vec::new(),
    };
    let walker = ignore::WalkBuilder::new(root)
        .standard_filters(false)
        .git_ignore(true)
        .require_git(false)
        .add_custom_ignore_filename(IGNORE_FILE)
        // Symlinks are reported as links, never followed, so they cannot
        // escape the root.
        .follow_links(false)
        .filter_entry(|e| {
            !(e.depth() > 0
                && e.file_type().is_some_and(|t| t.is_dir())
                && SKIP_DIRS.contains(&e.file_name().to_string_lossy().as_ref()))
        })
        .build();
    for entry in walker {
        // A malformed ignore rule is skipped by the walker, which keeps
        // going. Say so instead of silently indexing more than was asked.
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) if e.is_io() => return Err(Error(e.to_string())),
            Err(e) => {
                out.warnings.push(ignore_warning(root, &e));
                continue;
            }
        };
        // Errors from ignore files read on entering a directory arrive
        // attached to that directory's entry.
        if let Some(e) = entry.error() {
            out.warnings.push(ignore_warning(root, e));
        }
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.into_path();
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let Some(adapter) = atlas_parser::adapter_for(extension) else {
            out.unsupported += 1;
            continue;
        };
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        out.found.push((relative, path, adapter));
    }
    // Walk order is platform-dependent; sort for reproducible results.
    out.found.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Ignore-file error message with the absolute root path made relative.
fn ignore_warning(root: &Path, e: &ignore::Error) -> String {
    let message = e.to_string();
    let message = match root.to_str() {
        Some(prefix) if !prefix.is_empty() => message.replace(prefix, "."),
        _ => message,
    };
    format!("{message} (rule not applied)")
}

fn delete_file(tx: &Transaction, entry: Option<(i64, String)>) -> Result<()> {
    if let Some((id, _)) = entry {
        tx.execute("DELETE FROM files WHERE id = ?1", [id])?;
    }
    Ok(())
}

fn insert_file(tx: &Transaction, a: &FileAnalysis, hash: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO files (path, language, hash, parse_status) VALUES (?1, ?2, ?3, ?4)",
        params![a.path, a.language, hash, format!("{:?}", a.parse_status)],
    )?;
    let file_id = tx.last_insert_rowid();

    let mut stmt = tx.prepare_cached(
        "INSERT INTO symbols VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )?;
    for s in &a.symbols {
        stmt.execute(params![
            s.id,
            file_id,
            s.name,
            s.qualified_name,
            format!("{:?}", s.kind),
            s.signature,
            serde_json::to_string(&s.parameters)?,
            s.exported,
            s.range.start_line,
            s.range.start_column,
            s.range.end_line,
            s.range.end_column,
        ])?;
    }
    let mut stmt =
        tx.prepare_cached("INSERT INTO imports (file_id, source, line) VALUES (?1, ?2, ?3)")?;
    for i in &a.imports {
        stmt.execute(params![file_id, i.source, i.range.start_line])?;
    }
    let mut stmt = tx.prepare_cached(
        "INSERT INTO calls (file_id, caller_id, callee, form, line, col) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for c in &a.calls {
        stmt.execute(params![
            file_id,
            c.caller_id,
            c.callee,
            format!("{:?}", c.form),
            c.range.start_line,
            c.range.start_column,
        ])?;
    }
    let mut stmt = tx.prepare_cached("INSERT INTO diagnostics VALUES (?1, ?2, ?3, ?4)")?;
    for d in &a.diagnostics {
        stmt.execute(params![
            file_id,
            d.message,
            d.range.start_line,
            d.range.start_column
        ])?;
    }
    Ok(())
}

/// TypeScript and JavaScript import and call each other freely.
fn family(language: &str) -> &str {
    match language {
        "typescript" | "javascript" => "script",
        other => other,
    }
}

struct Candidate {
    id: String,
    kind: String,
    file_id: i64,
    family: String,
}

/// Recomputes every import and call link. Links cross files, so one changed
/// file can change links in files that were not reparsed.
fn resolve(tx: &Transaction) -> Result<()> {
    tx.execute("UPDATE imports SET target_file_id = NULL", [])?;

    let mut files: HashMap<String, i64> = HashMap::new();
    {
        let mut stmt = tx.prepare("SELECT path, id FROM files")?;
        for row in stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))? {
            let (path, id) = row?;
            files.insert(path, id);
        }
    }
    {
        let mut stmt = tx.prepare(
            "SELECT i.id, f.path, f.language, i.source FROM imports i \
             JOIN files f ON f.id = i.file_id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        let mut update = tx.prepare("UPDATE imports SET target_file_id = ?1 WHERE id = ?2")?;
        for row in rows {
            let (id, importer, language, source) = row?;
            // Only script adapters declare import resolution.
            if family(&language) != "script" {
                continue;
            }
            if let Some(target) = resolve_script_import(&importer, &source, &files) {
                update.execute(params![target, id])?;
            }
        }
    }

    let mut candidates: HashMap<String, Vec<Candidate>> = HashMap::new();
    {
        let mut stmt = tx.prepare(
            "SELECT s.name, s.id, s.kind, s.file_id, f.language FROM symbols s \
             JOIN files f ON f.id = s.file_id WHERE s.kind IN ('Function', 'Method')",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                Candidate {
                    id: r.get(1)?,
                    kind: r.get(2)?,
                    file_id: r.get(3)?,
                    family: family(&r.get::<_, String>(4)?).to_string(),
                },
            ))
        })?;
        for row in rows {
            let (name, candidate) = row?;
            candidates.entry(name).or_default().push(candidate);
        }
    }

    let mut stmt = tx.prepare(
        "SELECT c.id, c.file_id, f.language, c.callee, c.form FROM calls c \
         JOIN files f ON f.id = c.file_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    let mut update =
        tx.prepare("UPDATE calls SET target_id = ?1, status = ?2, note = ?3 WHERE id = ?4")?;
    for row in rows {
        let (id, file_id, language, callee, form) = row?;
        let empty = Vec::new();
        let named = candidates.get(&callee).unwrap_or(&empty);
        let (target, note) = pick(named, file_id, family(&language), &form);
        let status = if target.is_some() {
            "Inferred"
        } else {
            "Unresolved"
        };
        update.execute(params![target, status, note, id])?;
    }
    Ok(())
}

/// Links a call to a symbol only when the name leaves exactly one choice.
fn pick<'a>(
    named: &'a [Candidate],
    file_id: i64,
    family: &str,
    form: &str,
) -> (Option<&'a str>, String) {
    let fits: Vec<&Candidate> = named
        .iter()
        .filter(|c| c.family == family)
        .filter(|c| match form {
            "Bare" => c.kind == "Function",
            "Member" => c.kind == "Method",
            // `a::f()` may be a module function or an associated function.
            _ => true,
        })
        .collect();
    let local: Vec<&Candidate> = fits
        .iter()
        .copied()
        .filter(|c| c.file_id == file_id)
        .collect();
    match (local.as_slice(), fits.as_slice()) {
        ([one], _) => (
            Some(one.id.as_str()),
            "only symbol with this name in the same file".to_string(),
        ),
        ([_, ..], _) => (
            None,
            format!("ambiguous: {} candidates in the same file", local.len()),
        ),
        ([], [one]) => (
            Some(one.id.as_str()),
            "only symbol with this name in the project".to_string(),
        ),
        ([], []) => (
            None,
            "no project symbol with this name (library, built-in or dynamic)".to_string(),
        ),
        ([], _) => (
            None,
            format!("ambiguous: {} candidates in the project", fits.len()),
        ),
    }
}

/// Resolves `./x` or `../x` from `importer` to an indexed file, trying the
/// extensions and `index` files that Node and TypeScript try.
fn resolve_script_import(
    importer: &str,
    source: &str,
    files: &HashMap<String, i64>,
) -> Option<i64> {
    if !source.starts_with('.') {
        return None; // package import: outside the project
    }
    let mut parts: Vec<&str> = importer.split('/').collect();
    parts.pop();
    for segment in source.split('/') {
        match segment {
            "" | "." => {}
            // Leaving the project root resolves to nothing.
            ".." => {
                parts.pop()?;
            }
            name => parts.push(name),
        }
    }
    let base = parts.join("/");
    const EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mjs", "cjs"];
    let mut tries = vec![base.clone()];
    tries.extend(EXTENSIONS.iter().map(|e| format!("{base}.{e}")));
    tries.extend(EXTENSIONS.iter().map(|e| format!("{base}/index.{e}")));
    // TypeScript ESM style: `./x.js` written for `x.ts`.
    if let Some(stem) = base.strip_suffix(".js") {
        tries.push(format!("{stem}.ts"));
        tries.push(format!("{stem}.tsx"));
    }
    tries.iter().find_map(|path| files.get(path).copied())
}
