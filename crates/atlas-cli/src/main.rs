use std::path::Path;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use atlas_domain::AnalysisReport;

// shortcut: fixed skip list, no .gitignore / .codeatlasignore support.
// Upgrade to the `ignore` crate when atlas-index lands.
const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", "dist", "build"];

fn scan(root: &Path) -> std::io::Result<AnalysisReport> {
    let mut files = Vec::new();
    let mut skipped_files = 0u64;
    let mut pending = vec![root.to_path_buf()];

    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            // DirEntry::file_type does not follow symlinks, so links are
            // neither dirs nor files here and cannot escape the root.
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                if !SKIP_DIRS.contains(&entry.file_name().to_string_lossy().as_ref()) {
                    pending.push(path);
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let Some(adapter) = atlas_parser::adapter_for(extension) else {
                skipped_files += 1;
                continue;
            };
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let analysis = std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|src| adapter.analyze(&relative, &src).map_err(|e| e.to_string()));
            match analysis {
                Ok(a) => files.push(a),
                Err(e) => {
                    eprintln!("warning: skipped {relative}: {e}");
                    skipped_files += 1;
                }
            }
        }
    }

    // read_dir order is platform-dependent; sort for reproducible reports.
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(AnalysisReport {
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        root: root.to_string_lossy().into_owned(),
        generated_at_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        files,
        skipped_files,
    })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [command, path] = args.as_slice() else {
        eprintln!("usage: atlas scan <path>");
        return ExitCode::from(2);
    };
    if command != "scan" {
        eprintln!("unknown command `{command}`\nusage: atlas scan <path>");
        return ExitCode::from(2);
    }
    match scan(Path::new(path)) {
        Ok(report) => {
            // Serializing plain structs of strings and integers cannot fail.
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: cannot scan {path}: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_domain::{ParseStatus, SymbolKind};

    #[test]
    fn scans_sample_repository() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/typescript/sample");
        let report = scan(&root).unwrap();

        let paths: Vec<_> = report.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["src/broken.ts", "src/cart.ts"]);
        // README.md has no adapter; node_modules is never entered.
        assert_eq!(report.skipped_files, 1);

        let broken = &report.files[0];
        assert_eq!(broken.parse_status, ParseStatus::ParsedWithErrors);

        let cart = &report.files[1];
        assert_eq!(cart.parse_status, ParseStatus::Parsed);
        assert_eq!(cart.imports[0].source, "./types");
        let symbols: Vec<_> = cart
            .symbols
            .iter()
            .map(|s| (s.qualified_name.as_str(), s.kind, s.range.start_line))
            .collect();
        assert_eq!(
            symbols,
            [
                ("Cart", SymbolKind::Interface, 3),
                ("Total", SymbolKind::TypeAlias, 5),
                ("Status", SymbolKind::Enum, 7),
                ("handleCheckout", SymbolKind::Function, 9),
                ("applyDiscount", SymbolKind::Function, 13),
                ("CartService", SymbolKind::Class, 15),
                ("CartService.add", SymbolKind::Method, 16),
            ]
        );

        // The report round-trips through JSON unchanged.
        let json = serde_json::to_string(&report).unwrap();
        assert_eq!(
            serde_json::from_str::<AnalysisReport>(&json).unwrap(),
            report
        );
    }
}
