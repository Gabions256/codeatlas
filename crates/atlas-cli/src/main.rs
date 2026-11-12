//! Terminal interface. Parsing and presentation only: all analysis is in
//! `atlas-index`.

use std::path::PathBuf;
use std::process::ExitCode;

use atlas_index::{Direction, Edge, Index};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "atlas",
    version,
    about = "Local, evidence-first code intelligence"
)]
struct Cli {
    /// Project root. Query commands read the index a scan left there.
    #[arg(long, global = true, default_value = ".")]
    project: PathBuf,
    /// Print JSON instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Index a project into PATH/.codeatlas. Reparses only changed files.
    Scan {
        /// Project root. Defaults to --project.
        path: Option<PathBuf>,
    },
    /// Query indexed symbols.
    Symbols {
        #[command(subcommand)]
        command: SymbolsCommand,
    },
    /// Query call relationships. All links are inferred from names.
    Graph {
        #[command(subcommand)]
        command: GraphCommand,
    },
    /// Describe a symbol from indexed evidence. No AI involved.
    Explain {
        #[command(subcommand)]
        command: ExplainCommand,
    },
    /// Symbols changed since a git commit, and callers that may be affected.
    /// Rescans the project first. Runs git read-only.
    Changes {
        /// Commit to compare the working tree with.
        #[arg(long, default_value = "HEAD")]
        base: String,
        /// Caller hops to follow from each changed symbol.
        #[arg(long, default_value_t = 1)]
        depth: u32,
    },
    /// Print a project report to stdout.
    Report {
        #[arg(long, value_enum, default_value = "html")]
        format: Format,
    },
}

#[derive(Subcommand)]
enum SymbolsCommand {
    /// List symbols, optionally filtered by a name fragment.
    List {
        query: Option<String>,
        /// Function, Method, Class, Struct, Interface, Trait, Module, Enum or TypeAlias.
        #[arg(long)]
        kind: Option<String>,
    },
}

#[derive(Subcommand)]
enum GraphCommand {
    /// What a symbol calls. Without a symbol: every call site.
    Dependencies {
        /// Name, qualified name or id.
        symbol: Option<String>,
        /// Hops to follow.
        #[arg(long, default_value_t = 1)]
        depth: u32,
    },
    /// What calls a symbol. Without a symbol: every call site.
    Dependents {
        /// Name, qualified name or id.
        symbol: Option<String>,
        /// Hops to follow.
        #[arg(long, default_value_t = 1)]
        depth: u32,
    },
}

#[derive(Subcommand)]
enum ExplainCommand {
    /// Explain a function or method by name, qualified name or id.
    Function { name: String },
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Html,
    Json,
}

fn print_json<T: serde::Serialize>(value: &T) -> atlas_index::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn print_edges(edges: &[Edge]) {
    for e in edges {
        println!(
            "{} -> {}  [{}]  {}:{}:{}  ({})",
            e.from, e.to, e.status, e.path, e.line, e.column, e.note
        );
    }
}

fn run(cli: Cli) -> atlas_index::Result<()> {
    let root = match &cli.command {
        Command::Scan { path: Some(path) } => path.clone(),
        _ => cli.project.clone(),
    };
    let scans = matches!(cli.command, Command::Scan { .. } | Command::Changes { .. });
    if !scans && !Index::exists(&root) {
        return Err(atlas_index::Error(format!(
            "no index in {}: run `atlas scan` first",
            root.display()
        )));
    }
    let mut index = Index::open(&root)?;

    match cli.command {
        Command::Scan { .. } => {
            let summary = index.scan()?;
            for warning in &summary.warnings {
                eprintln!("warning: skipped {warning}");
            }
            for path in &summary.skipped {
                eprintln!("warning: could not read {path}");
            }
            if cli.json {
                return print_json(&summary);
            }
            println!(
                "{} file(s) indexed: {} reparsed, {} unchanged, {} removed, {} unsupported",
                summary.files,
                summary.reparsed,
                summary.unchanged,
                summary.removed,
                summary.unsupported
            );
            println!(
                "{} symbol(s); {} call site(s), {} linked by name; {} import(s), {} resolved",
                summary.symbols,
                summary.calls,
                summary.calls_inferred,
                summary.imports,
                summary.imports_resolved
            );
            if !summary.skipped.is_empty() {
                println!(
                    "Incomplete: {} path(s) could not be read and are not in the index.",
                    summary.skipped.len()
                );
            }
        }
        Command::Symbols {
            command: SymbolsCommand::List { query, kind },
        } => {
            let symbols = index.symbols(query.as_deref(), kind.as_deref())?;
            if cli.json {
                return print_json(&symbols);
            }
            for s in &symbols {
                println!(
                    "{:<10} {}  {}:{}  {}",
                    s.kind,
                    s.qualified_name,
                    s.path,
                    s.start_line,
                    s.signature.as_deref().unwrap_or("")
                );
            }
        }
        Command::Graph { command } => {
            let (symbol, depth, direction) = match command {
                GraphCommand::Dependencies { symbol, depth } => {
                    (symbol, depth, Direction::Dependencies)
                }
                GraphCommand::Dependents { symbol, depth } => {
                    (symbol, depth, Direction::Dependents)
                }
            };
            let graph = index.graph(symbol.as_deref(), direction, depth)?;
            if cli.json {
                return print_json(&graph);
            }
            print_edges(&graph.edges);
            if graph.truncated {
                println!(
                    "(cut at {} edges: more exist. Lower --depth or query a narrower symbol.)",
                    graph.edges.len()
                );
            }
        }
        Command::Explain {
            command: ExplainCommand::Function { name },
        } => {
            let explanations = index.explain_function(&name)?;
            if explanations.is_empty() {
                return Err(atlas_index::Error(format!(
                    "no function or method named `{name}` in the index"
                )));
            }
            if cli.json {
                return print_json(&explanations);
            }
            for e in &explanations {
                println!("{}\n", e.summary);
                if let Some(signature) = &e.symbol.signature {
                    println!("Signature: {signature}");
                }
                println!(
                    "Source:    {}:{}-{}",
                    e.symbol.path, e.symbol.start_line, e.symbol.end_line
                );
                println!("\nCalls:");
                print_edges(&e.calls);
                println!("\nCalled from:");
                print_edges(&e.called_by);
                println!("\nLimitations:");
                for limitation in &e.limitations {
                    println!("- {limitation}");
                }
                println!();
            }
        }
        Command::Changes { base, depth } => {
            let report = index.changes(&base, depth)?;
            if cli.json {
                return print_json(&report);
            }
            let short = report.base_commit.get(..7).unwrap_or(&report.base_commit);
            println!(
                "Compared with {} ({short}). Index rescanned first.",
                report.base
            );
            if report.files.is_empty() {
                println!("No changed files.");
            }
            for file in &report.files {
                match &file.old_path {
                    Some(old) => println!("\n{} {old} -> {}", file.status, file.path),
                    None => println!("\n{} {}", file.status, file.path),
                }
                if let Some(note) = &file.note {
                    println!("  changed, not analysed ({note})");
                }
                for s in &file.symbols {
                    println!(
                        "  {:<18} {} {}  line {}  [{}]",
                        s.change.label(),
                        s.kind.to_lowercase(),
                        s.qualified_name,
                        s.line,
                        s.evidence.label()
                    );
                    for e in &s.may_affect {
                        println!(
                            "    may affect: {}  {}:{}:{}  [{}]",
                            e.from,
                            e.path,
                            e.line,
                            e.column,
                            e.evidence.label()
                        );
                    }
                    if s.truncated {
                        println!("    (cut at {} call sites: more exist)", s.may_affect.len());
                    }
                }
            }
            println!("\nLimitations:");
            for limitation in &report.limitations {
                println!("- {limitation}");
            }
        }
        Command::Report { format } => {
            let report = index.report()?;
            match format {
                Format::Json => return print_json(&report),
                Format::Html => print!("{}", atlas_index::render_html(&report)),
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    // clap exits with code 2 on usage errors.
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_definition_is_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
