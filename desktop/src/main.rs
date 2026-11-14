//! Tracemark desktop: a window over `tracemark_index`. Like the CLI and the
//! MCP server it does no analysis of its own; each command calls the index.

// No console window behind the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::State;
use tracemark_index::{Direction, Edge, Index, Review, ScanSummary, SymbolRow};

/// The open project, if any. One project per window.
struct Project(Mutex<Option<Index>>);

/// Lists and edge sets longer than this are cut; the UI says so.
const MAX_ITEMS: usize = 500;

type Reply<T> = Result<T, String>;

fn with_index<T>(
    state: &State<'_, Project>,
    work: impl FnOnce(&mut Index) -> tracemark_index::Result<T>,
) -> Reply<T> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    let index = guard.as_mut().ok_or("Open a project folder first.")?;
    work(index).map_err(|e| e.to_string())
}

/// Opens (or creates) the index for `path` and scans it.
#[tauri::command]
async fn open_project(path: String, state: State<'_, Project>) -> Reply<ScanSummary> {
    let mut index = Index::open(&PathBuf::from(path)).map_err(|e| e.to_string())?;
    let summary = index.scan().map_err(|e| e.to_string())?;
    *state.0.lock().map_err(|e| e.to_string())? = Some(index);
    Ok(summary)
}

#[derive(Serialize)]
struct Symbols {
    symbols: Vec<SymbolRow>,
    truncated: bool,
}

/// Symbols whose name contains `query`, after rescanning changed files.
#[tauri::command]
async fn symbols(query: String, state: State<'_, Project>) -> Reply<Symbols> {
    with_index(&state, |index| {
        index.scan()?;
        let query = query.trim();
        let mut symbols = index.symbols((!query.is_empty()).then_some(query), None)?;
        let truncated = symbols.len() > MAX_ITEMS;
        symbols.truncate(MAX_ITEMS);
        Ok(Symbols { symbols, truncated })
    })
}

#[derive(Serialize)]
struct Detail {
    symbol: SymbolRow,
    calls: Vec<Edge>,
    called_by: Vec<Edge>,
}

/// One symbol and its direct calls in both directions.
#[tauri::command]
async fn symbol_detail(id: String, state: State<'_, Project>) -> Reply<Detail> {
    with_index(&state, |index| {
        let symbol = index
            .find(&id)?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| {
                tracemark_index::Error("That symbol is no longer in the index.".into())
            })?;
        let mut calls = index.graph(Some(&id), Direction::Dependencies, 1)?.edges;
        let mut called_by = index.graph(Some(&id), Direction::Dependents, 1)?.edges;
        calls.truncate(MAX_ITEMS);
        called_by.truncate(MAX_ITEMS);
        Ok(Detail {
            symbol,
            calls,
            called_by,
        })
    })
}

/// Rule-based review of the working tree against `base`.
#[tauri::command]
async fn review(base: String, state: State<'_, Project>) -> Reply<Review> {
    let base = if base.trim().is_empty() {
        "HEAD"
    } else {
        base.trim()
    };
    with_index(&state, |index| index.review(base, 2))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Project(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            open_project,
            symbols,
            symbol_detail,
            review
        ])
        .run(tauri::generate_context!())
        .expect("Tracemark could not start its window");
}
