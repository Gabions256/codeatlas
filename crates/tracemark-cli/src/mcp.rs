//! MCP server for AI coding agents: JSON-RPC 2.0 over stdio, one message
//! per line. Only JSON-RPC goes to stdout. Like the other interfaces, it
//! calls `tracemark_index` and does no analysis of its own.
//!
//! The project root is fixed when the server starts. Tools cannot reach
//! other folders. Every query tool rescans changed files first, so answers
//! reflect edits the agent just made.

use std::io::{BufRead, Write};

use serde_json::{json, Value};
use tracemark_index::{Direction, Index};

/// Newest first. A client asking for an unknown version gets the newest.
const PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// Results larger than this are cut, and the result says so.
const MAX_ITEMS: usize = 500;

const INSTRUCTIONS: &str = "Tracemark answers questions about this project's code structure from a local index. \
Every symbol, import and call carries an evidence class: Syntax (written in the source), StaticAnalysis \
(matched by a rule such as a file path) or Inferred (matched by name, not type-checked). Treat Inferred \
links as leads to verify, not facts. Tools rescan changed files before answering. Nothing is run, built \
or tested, and no code leaves this machine.";

pub fn serve(index: &mut Index) -> tracemark_index::Result<()> {
    let mut stdout = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = handle(index, &line) {
            writeln!(stdout, "{reply}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

/// One incoming message to at most one reply. Notifications and responses
/// from the client get none.
fn handle(index: &mut Index, line: &str) -> Option<Value> {
    let msg: Value = match serde_json::from_str(line) {
        Ok(msg) => msg,
        Err(e) => return Some(error(Value::Null, -32700, &format!("parse error: {e}"))),
    };
    if !msg.is_object() {
        // Batches were removed from MCP in 2025-06-18.
        return Some(error(Value::Null, -32600, "expected one JSON-RPC object"));
    }
    let id = msg.get("id")?.clone();
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        return Some(error(id, -32600, "invalid request: no method"));
    };
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
    Some(match method {
        "initialize" => ok(id, initialize(&params)),
        "ping" => ok(id, json!({})),
        "tools/list" => ok(id, json!({ "tools": tools() })),
        "tools/call" => match call(index, &params) {
            Ok(result) => ok(id, result),
            Err(message) => error(id, -32602, &message),
        },
        other => error(id, -32601, &format!("method not found: {other}")),
    })
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = PROTOCOLS
        .iter()
        .find(|v| Some(**v) == asked)
        .unwrap_or(&PROTOCOLS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "tracemark", "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS,
    })
}

fn tools() -> Value {
    let base = json!({ "type": "string", "description": "Git commit to compare the working tree with. Default HEAD." });
    let depth = json!({ "type": "integer", "minimum": 1, "maximum": 10, "description": "Caller hops to follow. Default 1." });
    let read_only = json!({ "readOnlyHint": true, "openWorldHint": false });
    json!([
        {
            "name": "tracemark_scan",
            "description": "Index the project (only changed files are reparsed) and return counts and warnings.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
        },
        {
            "name": "tracemark_symbols",
            "description": "List functions, methods, classes and other symbols whose name contains `query`, with file, lines and signature.",
            "inputSchema": { "type": "object", "properties": {
                "query": { "type": "string", "description": "Name fragment. Omit for all symbols." },
                "kind": { "type": "string", "description": "Function, Method, Class, Struct, Interface, Trait, Module, Enum or TypeAlias." },
            } },
            "annotations": read_only,
        },
        {
            "name": "tracemark_graph",
            "description": "Call relationships of a symbol: what it calls (dependencies) or what calls it (dependents). Every edge has file:line:column and an evidence class.",
            "inputSchema": { "type": "object", "properties": {
                "symbol": { "type": "string", "description": "Name, qualified name or id." },
                "direction": { "type": "string", "enum": ["dependencies", "dependents"], "description": "Default dependents." },
                "depth": depth,
            }, "required": ["symbol"] },
            "annotations": read_only,
        },
        {
            "name": "tracemark_explain",
            "description": "Structure of a function or method from indexed evidence: location, signature, calls and callers, and the limits of that evidence. Not a description of behaviour.",
            "inputSchema": { "type": "object", "properties": {
                "name": { "type": "string", "description": "Name, qualified name or id." },
            }, "required": ["name"] },
            "annotations": read_only,
        },
        {
            "name": "tracemark_changes",
            "description": "Symbols added, removed or changed since a git commit, and the callers they may affect. Use after editing code.",
            "inputSchema": { "type": "object", "properties": { "base": base, "depth": depth } },
            "annotations": read_only,
        },
        {
            "name": "tracemark_review",
            "description": "Review the change since a git commit with fixed rules: findings with evidence and a recommendation. Use before declaring a change done. Structure only: it does not run or test code.",
            "inputSchema": { "type": "object", "properties": { "base": base, "depth": depth } },
            "annotations": read_only,
        },
    ])
}

/// Runs one tool. `Err` is a protocol error (unknown tool, bad arguments);
/// a failure inside the tool is a result with `isError`, so the agent sees it.
fn call(index: &mut Index, params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or("missing tool name")?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let text = |key: &str| args.get(key).and_then(Value::as_str).map(str::to_string);
    let depth = args
        .get("depth")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .clamp(1, 10) as u32;
    let base = text("base").unwrap_or_else(|| "HEAD".to_string());
    let required = |key: &str| text(key).ok_or(format!("`{key}` is required"));

    let outcome = match name {
        "tracemark_scan" => index.scan().and_then(to_value),
        "tracemark_symbols" => {
            let (query, kind) = (text("query"), text("kind"));
            index
                .scan()
                .and_then(|_| index.symbols(query.as_deref(), kind.as_deref()))
                .and_then(|symbols| capped("symbols", symbols))
        }
        "tracemark_graph" => {
            let symbol = required("symbol")?;
            let direction = match text("direction").as_deref() {
                None | Some("dependents") => Direction::Dependents,
                Some("dependencies") => Direction::Dependencies,
                Some(other) => return Err(format!("unknown direction `{other}`")),
            };
            index
                .scan()
                .and_then(|_| index.graph(Some(&symbol), direction, depth))
                .and_then(|graph| {
                    let cut = graph.truncated;
                    let mut value = capped("edges", graph.edges)?;
                    if cut {
                        value["truncated"] = json!(true);
                    }
                    Ok(value)
                })
        }
        "tracemark_explain" => {
            let target = required("name")?;
            index
                .scan()
                .and_then(|_| index.explain_function(&target))
                .and_then(|found| {
                    if found.is_empty() {
                        Err(tracemark_index::Error(format!(
                            "no function or method named `{target}` in the index"
                        )))
                    } else {
                        to_value(found)
                    }
                })
        }
        "tracemark_changes" => index.changes(&base, depth).and_then(to_value),
        "tracemark_review" => index.review(&base, depth.max(2)).and_then(to_value),
        other => return Err(format!("unknown tool `{other}`")),
    };
    Ok(match outcome {
        Ok(value) => {
            json!({ "content": [{ "type": "text", "text": value.to_string() }], "isError": false })
        }
        Err(e) => {
            json!({ "content": [{ "type": "text", "text": e.to_string() }], "isError": true })
        }
    })
}

fn to_value<T: serde::Serialize>(value: T) -> tracemark_index::Result<Value> {
    Ok(serde_json::to_value(value)?)
}

/// `{ key: [first MAX_ITEMS], "truncated": bool }`.
fn capped<T: serde::Serialize>(key: &str, mut items: Vec<T>) -> tracemark_index::Result<Value> {
    let truncated = items.len() > MAX_ITEMS;
    items.truncate(MAX_ITEMS);
    Ok(json!({ key: to_value(items)?, "truncated": truncated }))
}
