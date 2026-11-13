//! What changed since a git commit, and what may be affected.
//!
//! Git is run as a read-only subprocess. Changed symbols are found by
//! comparing the source text of the two versions (`Syntax` evidence). The
//! callers that may be affected come from the index and are name matches
//! (`Inferred`). Nothing here says whether a change works or was tested.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};

use serde::Serialize;
use tracemark_domain::{Evidence, FileAnalysis, SourceRange, Symbol, SymbolKind};

use crate::{family, Direction, Edge, Error, Index, Result, MAX_EDGES, SKIP_DIRS};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Change {
    Added,
    Removed,
    /// Declaration text up to the body, or the parameters, differ.
    SignatureChanged,
    /// Only the text after the signature differs.
    BodyChanged,
}

impl Change {
    pub fn label(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::SignatureChanged => "signature changed",
            Self::BodyChanged => "body changed",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SymbolChange {
    pub qualified_name: String,
    pub kind: String,
    pub change: Change,
    /// Line in the working tree; for a removed symbol, in the base commit.
    pub line: u32,
    /// Always `Syntax`: the two versions of the source text differ here.
    pub evidence: Evidence,
    /// Call sites that may be affected, with their own evidence class.
    /// Changed symbols: callers linked to it, followed `depth` hops.
    /// Removed symbols: call sites that still use its name.
    pub may_affect: Vec<Edge>,
    /// `may_affect` hit the edge limit.
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct FileChange {
    /// Relative to the project root, `/` separated.
    pub path: String,
    /// Set when git reports a rename.
    pub old_path: Option<String>,
    /// `added`, `modified`, `deleted`, `renamed` or `untracked`.
    pub status: &'static str,
    /// False when the file was not parsed; `note` says why.
    pub analysed: bool,
    pub note: Option<String>,
    /// Parser diagnostics in the working-tree version, and in the base.
    pub syntax_errors: u32,
    pub syntax_errors_before: u32,
    pub symbols: Vec<SymbolChange>,
}

#[derive(Debug, Serialize)]
pub struct ChangeReport {
    /// `--base` as given.
    pub base: String,
    /// The commit it resolved to.
    pub base_commit: String,
    pub files: Vec<FileChange>,
    pub limitations: Vec<&'static str>,
}

const LIMITATIONS: &[&str] = &[
    "Changes are found by comparing source text. Whether a change works or is tested is not known.",
    "`may_affect` lists name-matched calls (`Inferred`). `x.f()` callers other than this/self are not found.",
    "A symbol renamed inside a file shows as one removed and one added.",
    "A body change in a method also shows as a body change of its class.",
    "Rust calls inside macros are not seen. Rust and Python imports are not resolved.",
];

impl Index {
    /// Rescans the project, then compares the working tree with `base`.
    pub fn changes(&mut self, base: &str, depth: u32) -> Result<ChangeReport> {
        // An option-like ref would be read by git as a flag.
        if base.is_empty() || base.starts_with('-') {
            return Err(Error(format!("invalid --base `{base}`")));
        }
        let root = self.root.clone();
        let commit = git_text(
            &root,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                "--end-of-options",
                &format!("{base}^{{commit}}"),
            ],
        )
        .map_err(|_| {
            Error(format!(
                "`{base}` is not a commit in a git repository at {}",
                root.display()
            ))
        })?;
        let commit = commit.trim().to_string();

        let mut entries = diff(&root, &commit)?;
        let untracked = git(
            &root,
            &[
                "ls-files",
                "-z",
                "--others",
                "--exclude-standard",
                "--",
                ".",
            ],
        )?;
        for path in split_z(&untracked) {
            entries.push(("untracked", None, path));
        }
        entries.retain(|(_, _, path)| !path.split('/').any(|part| SKIP_DIRS.contains(&part)));
        entries.sort_by(|a, b| a.2.cmp(&b.2));

        self.scan()?;
        let mut files = Vec::new();
        for (status, old_path, path) in entries {
            files.push(self.file_change(&root, &commit, status, old_path, path, depth)?);
        }
        Ok(ChangeReport {
            base: base.to_string(),
            base_commit: commit,
            files,
            limitations: LIMITATIONS.to_vec(),
        })
    }

    fn file_change(
        &self,
        root: &Path,
        commit: &str,
        status: &'static str,
        old_path: Option<String>,
        path: String,
        depth: u32,
    ) -> Result<FileChange> {
        let mut change = FileChange {
            path,
            old_path,
            status,
            analysed: false,
            note: None,
            syntax_errors: 0,
            syntax_errors_before: 0,
            symbols: Vec::new(),
        };
        let extension = change.path.rsplit_once('.').map_or("", |(_, e)| e);
        let Some(adapter) = tracemark_parser::adapter_for(extension) else {
            change.note = Some("no language adapter".into());
            return Ok(change);
        };
        let before_path = change
            .old_path
            .clone()
            .unwrap_or_else(|| change.path.clone());
        let before = match status {
            "added" | "untracked" => None,
            _ => Some(git(
                root,
                &["cat-file", "blob", &format!("{commit}:./{before_path}")],
            )?),
        };
        let after = match status {
            "deleted" => None,
            _ => Some(std::fs::read(root.join(&change.path))?),
        };
        let parse = |bytes: Option<Vec<u8>>, path: &str| -> std::result::Result<_, String> {
            let Some(bytes) = bytes else {
                return Ok(None);
            };
            let text = String::from_utf8(bytes).map_err(|_| "not UTF-8".to_string())?;
            // git stores LF; a CRLF checkout would otherwise change every body.
            let text = text.replace("\r\n", "\n");
            let analysis = adapter.analyze(path, &text).map_err(|e| e.to_string())?;
            Ok(Some((analysis, text)))
        };
        let (before, after) = match (parse(before, &before_path), parse(after, &change.path)) {
            (Ok(b), Ok(a)) => (b, a),
            (Err(reason), _) | (_, Err(reason)) => {
                change.note = Some(reason);
                return Ok(change);
            }
        };
        change.analysed = true;
        let errors = |side: &Option<(FileAnalysis, String)>| {
            side.as_ref().map_or(0, |(a, _)| a.diagnostics.len() as u32)
        };
        change.syntax_errors = errors(&after);
        change.syntax_errors_before = errors(&before);

        let old = keyed(before.as_ref());
        let new = keyed(after.as_ref());
        for (key, (symbol, text)) in &new {
            let kind = match old.get(key) {
                None => Change::Added,
                Some((was, _))
                    if was.signature != symbol.signature
                        || was.parameters != symbol.parameters
                        || was.signature_digest != symbol.signature_digest =>
                {
                    Change::SignatureChanged
                }
                Some((_, was_text)) if was_text != text => Change::BodyChanged,
                Some(_) => continue,
            };
            let (may_affect, truncated) = if kind == Change::Added {
                (Vec::new(), false)
            } else {
                let graph = self.walk(vec![symbol.id.clone()], Direction::Dependents, depth)?;
                (graph.edges, graph.truncated)
            };
            change
                .symbols
                .push(symbol_change(symbol, kind, may_affect, truncated));
        }
        let language = after
            .as_ref()
            .or(before.as_ref())
            .map(|(a, _)| a.language.clone())
            .unwrap_or_default();
        for (key, (symbol, _)) in &old {
            if new.contains_key(key) {
                continue;
            }
            let (may_affect, truncated) = self.callers_by_name(symbol, &language)?;
            change.symbols.push(symbol_change(
                symbol,
                Change::Removed,
                may_affect,
                truncated,
            ));
        }
        change
            .symbols
            .sort_by(|a, b| (a.line, &a.qualified_name).cmp(&(b.line, &b.qualified_name)));
        Ok(change)
    }

    /// Call sites whose callee name and form could have meant `symbol`.
    fn callers_by_name(&self, symbol: &Symbol, language: &str) -> Result<(Vec<Edge>, bool)> {
        let forms = match symbol.kind {
            SymbolKind::Function => "'Bare', 'Path'",
            SymbolKind::Method => "'SelfMember', 'Path'",
            SymbolKind::Class => "'New'",
            _ => return Ok((Vec::new(), false)),
        };
        let languages = match family(language) {
            "script" => "'typescript', 'javascript'",
            "rust" => "'rust'",
            _ => "'python'",
        };
        let filter =
            format!("c.callee = ?1 AND c.form IN ({forms}) AND f.language IN ({languages})");
        let mut edges = self.edges(&filter, Some(&symbol.name), 1)?;
        let truncated = edges.len() > MAX_EDGES;
        edges.truncate(MAX_EDGES);
        Ok((edges, truncated))
    }
}

fn symbol_change(
    symbol: &Symbol,
    change: Change,
    may_affect: Vec<Edge>,
    truncated: bool,
) -> SymbolChange {
    SymbolChange {
        qualified_name: symbol.qualified_name.clone(),
        kind: format!("{:?}", symbol.kind),
        change,
        line: symbol.range.start_line,
        evidence: Evidence::Syntax,
        may_affect,
        truncated,
    }
}

type Keyed<'a> = HashMap<String, (&'a Symbol, &'a str)>;

/// Symbols keyed by qualified name, kind and occurrence, with their text.
fn keyed(analysis: Option<&(FileAnalysis, String)>) -> Keyed<'_> {
    let mut out = HashMap::new();
    let Some((analysis, text)) = analysis else {
        return out;
    };
    let mut seen: HashMap<String, u32> = HashMap::new();
    for symbol in &analysis.symbols {
        let base = format!("{}\0{:?}", symbol.qualified_name, symbol.kind);
        let n = seen.entry(base.clone()).or_insert(0);
        *n += 1;
        out.insert(format!("{base}\0{n}"), (symbol, slice(text, symbol.range)));
    }
    out
}

/// Source text of a range. Columns are 1-based byte offsets.
fn slice(text: &str, range: SourceRange) -> &str {
    let offset = |line: u32, column: u32| {
        let start: usize = text
            .split_inclusive('\n')
            .take(line.saturating_sub(1) as usize)
            .map(str::len)
            .sum();
        (start + column.saturating_sub(1) as usize).min(text.len())
    };
    let start = offset(range.start_line, range.start_column);
    let end = offset(range.end_line, range.end_column);
    text.get(start..end.max(start)).unwrap_or("")
}

type Entry = (&'static str, Option<String>, String);

fn diff(root: &Path, commit: &str) -> Result<Vec<Entry>> {
    let out = git(
        root,
        &[
            "diff",
            "--name-status",
            "-z",
            "-M",
            "--no-ext-diff",
            "--no-textconv",
            "--relative",
            commit,
            "--",
            ".",
        ],
    )?;
    let malformed = || Error("unexpected git diff output".into());
    let mut parts = split_z(&out).into_iter();
    let mut entries = Vec::new();
    while let Some(code) = parts.next() {
        let path = parts.next().ok_or_else(malformed)?;
        entries.push(match code.chars().next() {
            Some('A') => ("added", None, path),
            Some('D') => ("deleted", None, path),
            Some('R') => ("renamed", Some(path), parts.next().ok_or_else(malformed)?),
            // M, T (type change) and anything else: compare both sides.
            _ => ("modified", None, path),
        });
    }
    Ok(entries)
}

fn split_z(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|b| *b == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).replace('\\', "/"))
        .collect()
}

fn git_text(root: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8_lossy(&git(root, args)?).into_owned())
}

/// Runs a read-only git command in `root`. Never through a shell.
fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        // fsmonitor is a repository setting that can start another program.
        .args(["-c", "core.fsmonitor=false", "-c", "core.quotepath=false"])
        .args(args)
        // Do not rewrite .git/index while reading, and never prompt.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| Error(format!("cannot run git: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error(format!("git {}: {}", args[0], stderr.trim())));
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_uses_one_based_byte_columns() {
        let text = "ab\ncdef\ng";
        let range = SourceRange {
            start_line: 2,
            start_column: 2,
            end_line: 3,
            end_column: 2,
        };
        assert_eq!(slice(text, range), "def\ng");
    }

    #[test]
    fn split_z_drops_the_trailing_terminator() {
        assert_eq!(split_z(b"M\0a b.ts\0"), ["M", "a b.ts"]);
    }
}
