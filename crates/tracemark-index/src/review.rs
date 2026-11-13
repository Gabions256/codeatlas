//! A review of a change: findings from fixed rules over the change report,
//! and a recommendation derived only from those findings.
//!
//! Nothing here runs, builds or tests code, and no AI is involved. A
//! recommendation means "these rules found this", never "the change is
//! correct".

use std::collections::HashSet;

use serde::Serialize;
use tracemark_domain::Evidence;

use crate::{Change, ChangeReport, Edge, Index, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Severity {
    Info,
    Medium,
    High,
    Blocker,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Blocker => "blocker",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Recommendation {
    /// At least one blocker.
    RequestChanges,
    /// At least one high finding, no blocker.
    ReviewCarefully,
    /// Only medium and info findings.
    ApproveAfterChecks,
    /// Info findings or none. Not a statement that the change is correct.
    NoRuleFlagged,
}

impl Recommendation {
    pub fn label(self) -> &'static str {
        match self {
            Self::RequestChanges => "Request changes",
            Self::ReviewCarefully => "Review carefully before approving",
            Self::ApproveAfterChecks => "Approve after checking the listed items",
            Self::NoRuleFlagged => "No rule flagged a risk",
        }
    }
}

/// One rule: id, severity, and what it checks. Shown with every review.
#[derive(Debug, Serialize)]
pub struct Rule {
    pub id: &'static str,
    pub severity: Severity,
    pub checks: &'static str,
}

/// Callers at or above this count make a change "wide impact".
pub const WIDE_IMPACT: usize = 10;

pub const RULES: &[Rule] = &[
    Rule {
        id: "removed-still-called",
        severity: Severity::Blocker,
        checks: "A removed function, method or class whose name is still called, and the call no longer links to any symbol.",
    },
    Rule {
        id: "syntax-errors-added",
        severity: Severity::High,
        checks: "A changed file has more parser errors than in the base commit.",
    },
    Rule {
        id: "signature-callers-untouched",
        severity: Severity::High,
        checks: "A signature changed, and callers linked to it are in files this change did not touch.",
    },
    Rule {
        id: "wide-impact",
        severity: Severity::Medium,
        checks: "A changed function or method has at least 10 linked callers within the depth followed.",
    },
    Rule {
        id: "no-test-calls",
        severity: Severity::Info,
        checks: "A new or changed function or method is not called by name from any test file or test function.",
    },
    Rule {
        id: "not-analysed",
        severity: Severity::Info,
        checks: "A changed file that no language adapter reads, or that could not be parsed.",
    },
];

#[derive(Debug, Serialize)]
pub struct Finding {
    pub rule: &'static str,
    pub severity: Severity,
    pub path: String,
    /// Line of the symbol; 0 for file-level findings.
    pub line: u32,
    pub symbol: Option<String>,
    pub message: String,
    /// Class of the claim this finding makes.
    pub evidence: Evidence,
    /// Call sites behind the finding, each with its own evidence class.
    pub sites: Vec<Edge>,
}

#[derive(Debug, Serialize)]
pub struct Review {
    pub recommendation: Recommendation,
    pub reason: String,
    pub findings: Vec<Finding>,
    pub rules: &'static [Rule],
    pub changes: ChangeReport,
    pub limitations: Vec<&'static str>,
}

const LIMITATIONS: &[&str] = &[
    "Structure only: Tracemark did not run, build or test this code. A recommendation is what these rules found, not proof the change is correct.",
    "Callers are linked by name. Member calls other than this/self, dynamic calls and Rust calls inside macros are not seen, so impact can be larger than shown.",
    "`no-test-calls` looks for direct calls by name from test files. A function tested indirectly is still listed.",
];

/// Most findings list their call sites; cap them to keep output readable.
const MAX_SITES: usize = 20;

impl Index {
    /// Rescans, compares the working tree with `base`, and applies `RULES`.
    pub fn review(&mut self, base: &str, depth: u32) -> Result<Review> {
        let changes = self.changes(base, depth.max(1))?;
        let touched: HashSet<&str> = changes.files.iter().map(|f| f.path.as_str()).collect();
        let mut findings = Vec::new();

        for file in &changes.files {
            let finding =
                |rule, severity, line, symbol: Option<&str>, message, evidence, sites| Finding {
                    rule,
                    severity,
                    path: file.path.clone(),
                    line,
                    symbol: symbol.map(str::to_string),
                    message,
                    evidence,
                    sites,
                };
            if !file.analysed {
                let note = file.note.as_deref().unwrap_or("not parsed");
                findings.push(finding(
                    "not-analysed",
                    Severity::Info,
                    0,
                    None,
                    format!("Changed, not analysed: {note}."),
                    Evidence::Syntax,
                    Vec::new(),
                ));
                continue;
            }
            if file.syntax_errors > file.syntax_errors_before {
                findings.push(finding(
                    "syntax-errors-added",
                    Severity::High,
                    0,
                    None,
                    format!(
                        "The new version has {} parser error(s); the base had {}.",
                        file.syntax_errors, file.syntax_errors_before
                    ),
                    Evidence::Syntax,
                    Vec::new(),
                ));
            }
            for s in &file.symbols {
                let name = s.qualified_name.as_str();
                match s.change {
                    Change::Removed => {
                        let dangling = capped(s.may_affect.iter().filter(|e| e.to_id.is_none()));
                        if !dangling.is_empty() {
                            findings.push(finding(
                                "removed-still-called",
                                Severity::Blocker,
                                s.line,
                                Some(name),
                                format!(
                                    "Removed, but {} call site(s) still use the name and no longer link to any symbol.",
                                    dangling.len()
                                ),
                                Evidence::Syntax,
                                dangling,
                            ));
                        }
                    }
                    Change::SignatureChanged => {
                        let outside = capped(
                            s.may_affect
                                .iter()
                                .filter(|e| e.depth == 1 && !touched.contains(e.path.as_str())),
                        );
                        if !outside.is_empty() {
                            findings.push(finding(
                                "signature-callers-untouched",
                                Severity::High,
                                s.line,
                                Some(name),
                                format!(
                                    "Signature changed; {} caller(s) in files this change did not touch may need updating.",
                                    outside.len()
                                ),
                                Evidence::Inferred,
                                outside,
                            ));
                        }
                    }
                    _ => {}
                }
                if matches!(s.change, Change::BodyChanged | Change::SignatureChanged) {
                    let callers: HashSet<&str> = s
                        .may_affect
                        .iter()
                        .filter_map(|e| e.from_id.as_deref())
                        .collect();
                    if callers.len() >= WIDE_IMPACT {
                        findings.push(finding(
                            "wide-impact",
                            Severity::Medium,
                            s.line,
                            Some(name),
                            format!(
                                "Changed; {} symbol(s) call it within the depth followed.",
                                callers.len()
                            ),
                            Evidence::Inferred,
                            capped(s.may_affect.iter()),
                        ));
                    }
                }
                if s.change != Change::Removed && matches!(s.kind.as_str(), "Function" | "Method") {
                    let simple = name.rsplit('.').next().unwrap_or(name);
                    if self.test_call_sites(simple)?.is_empty() {
                        findings.push(finding(
                            "no-test-calls",
                            Severity::Info,
                            s.line,
                            Some(name),
                            format!("{}; no test calls it by name.", s.change.label()),
                            Evidence::Syntax,
                            Vec::new(),
                        ));
                    }
                }
            }
        }
        findings.sort_by(|a, b| {
            (b.severity, &a.path, a.line, a.rule).cmp(&(a.severity, &b.path, b.line, b.rule))
        });

        let count = |severity| findings.iter().filter(|f| f.severity == severity).count();
        let (recommendation, reason) = match findings.iter().map(|f| f.severity).max() {
            Some(Severity::Blocker) => (
                Recommendation::RequestChanges,
                format!("{} blocking finding(s).", count(Severity::Blocker)),
            ),
            Some(Severity::High) => (
                Recommendation::ReviewCarefully,
                format!("{} high finding(s), no blockers.", count(Severity::High)),
            ),
            Some(Severity::Medium) => (
                Recommendation::ApproveAfterChecks,
                format!("{} medium finding(s), nothing higher.", count(Severity::Medium)),
            ),
            _ => (
                Recommendation::NoRuleFlagged,
                "No rule flagged a risk. That is not proof the change is correct: the code was not run, built or tested.".to_string(),
            ),
        };
        Ok(Review {
            recommendation,
            reason,
            findings,
            rules: RULES,
            changes,
            limitations: LIMITATIONS.to_vec(),
        })
    }

    /// Call sites with this callee name in a test file or a test function.
    fn test_call_sites(&self, name: &str) -> Result<Vec<Edge>> {
        Ok(self
            .edges("c.callee = ?1", Some(name), 1)?
            .into_iter()
            .filter(|e| is_test_path(&e.path) || is_test_caller(&e.from))
            .collect())
    }
}

fn capped<'a>(edges: impl Iterator<Item = &'a Edge>) -> Vec<Edge> {
    edges.take(MAX_SITES).cloned().collect()
}

/// `tests/`, `__tests__/`, `*.test.*`, `*.spec.*`, `*_test.*`, `test_*.py`.
fn is_test_path(path: &str) -> bool {
    let mut parts: Vec<&str> = path.split('/').collect();
    let file = parts.pop().unwrap_or("");
    let stem = file.split('.').next().unwrap_or("");
    parts
        .iter()
        .any(|d| matches!(*d, "test" | "tests" | "__tests__" | "spec"))
        || file.contains(".test.")
        || file.contains(".spec.")
        || stem.ends_with("_test")
        || (stem.starts_with("test_") && file.ends_with(".py"))
}

/// Rust `mod tests`, Python `test_*` functions and `Test*` classes.
fn is_test_caller(qualified: &str) -> bool {
    qualified.split('.').any(|part| {
        matches!(part, "test" | "tests") || part.starts_with("test_") || part.starts_with("Test")
    })
}

/// Text for a pull-request comment. Every name and path from the project is
/// put in a code span, so it cannot add links, mentions or HTML.
pub fn render_markdown(review: &Review) -> String {
    fn code(text: &str) -> String {
        format!("`{}`", text.replace('`', "'").replace(['\n', '\r'], " "))
    }
    let short = review
        .changes
        .base_commit
        .get(..7)
        .unwrap_or(&review.changes.base_commit);
    let mut out = format!(
        "### Tracemark review: {}\n\n{}\n\nCompared with {} ({}). {} file(s) changed.\n\n",
        review.recommendation.label(),
        review.reason,
        code(&review.changes.base),
        code(short),
        review.changes.files.len()
    );
    if review.findings.is_empty() {
        out.push_str("No findings.\n\n");
    } else {
        out.push_str(
            "| Severity | Rule | Where | Finding | Evidence |\n| --- | --- | --- | --- | --- |\n",
        );
        for f in &review.findings {
            let place = if f.line > 0 {
                format!("{}:{}", f.path, f.line)
            } else {
                f.path.clone()
            };
            let what = match &f.symbol {
                Some(symbol) => format!("{} {}", code(symbol), f.message),
                None => f.message.clone(),
            };
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                f.severity.label(),
                code(f.rule),
                code(&place),
                what.replace('|', "\\|"),
                f.evidence.label()
            ));
        }
        out.push('\n');
        let with_sites: Vec<&Finding> = review
            .findings
            .iter()
            .filter(|f| !f.sites.is_empty())
            .collect();
        if !with_sites.is_empty() {
            out.push_str("<details><summary>Call sites behind the findings</summary>\n\n");
            for f in with_sites {
                let symbol = f.symbol.as_deref().unwrap_or("");
                out.push_str(&format!("- {} {}\n", code(f.rule), code(symbol)));
                for e in &f.sites {
                    out.push_str(&format!(
                        "  - {} calls {} at {} ({})\n",
                        code(&e.from),
                        code(&e.to),
                        code(&format!("{}:{}:{}", e.path, e.line, e.column)),
                        e.evidence.label()
                    ));
                }
            }
            out.push_str("\n</details>\n\n");
        }
    }
    for limitation in &review.limitations {
        out.push_str(&format!("> {limitation}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_test_paths_and_callers() {
        for path in [
            "tests/a.rs",
            "src/__tests__/a.ts",
            "a.test.ts",
            "a.spec.js",
            "pkg/a_test.go",
            "test_shop.py",
        ] {
            assert!(is_test_path(path), "{path}");
        }
        for path in ["src/latest.ts", "contest.py", "src/testing_utils.ts"] {
            assert!(!is_test_path(path), "{path}");
        }
        assert!(is_test_caller("tests.it_works"));
        assert!(is_test_caller("TestShop.test_pay"));
        assert!(!is_test_caller("Contest.latest"));
    }

    #[test]
    fn severities_order_from_info_to_blocker() {
        assert!(Severity::Blocker > Severity::High);
        assert!(Severity::High > Severity::Medium);
        assert!(Severity::Medium > Severity::Info);
    }
}
