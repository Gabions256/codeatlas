//! Self-contained HTML report: no scripts, no external resources.
//! Everything from the scanned project is untrusted and goes through `esc`.

use std::collections::HashMap;

use tracemark_domain::Evidence;

use crate::{Edge, Report};

fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

const STYLE: &str = "body{font:15px/1.5 system-ui,sans-serif;max-width:60rem;margin:2rem auto;\
padding:0 1rem;color:#1a1a1a;background:#fff}code,pre{font:13px ui-monospace,monospace}\
pre{background:#f4f4f4;padding:.5rem;overflow-x:auto}table{border-collapse:collapse}\
td,th{border:1px solid #ccc;padding:.25rem .5rem;text-align:left;vertical-align:top}\
section{border-top:1px solid #ccc;margin-top:2rem}.sym{margin:1rem 0 1rem 1rem}\
.tag{font-size:12px;border:1px solid #888;border-radius:3px;padding:0 .3rem}\
.note{background:#fff8e1;padding:.5rem 1rem}.dim{color:#555}\
h2 code,h3 code{font-size:inherit}h3 .dim,h3 .tag{font-size:13px;font-weight:normal}";

fn tag(evidence: Evidence) -> String {
    format!(
        "<span class=\"tag\" title=\"evidence class\">{}</span>",
        evidence.label()
    )
}

fn edge_item(edge: &Edge, outgoing: bool) -> String {
    let (id, name) = if outgoing {
        (&edge.to_id, &edge.to)
    } else {
        (&edge.from_id, &edge.from)
    };
    let name = match id {
        Some(id) => format!("<a href=\"#s-{}\"><code>{}</code></a>", esc(id), esc(name)),
        None => format!("<code>{}</code>", esc(name)),
    };
    // A linked edge's status and evidence class are both "inferred".
    let status = if edge.to_id.is_some() {
        String::new()
    } else {
        format!("<span class=\"tag\">{}</span> ", esc(&edge.status))
    };
    format!(
        "<li>{name} {status}{} <span class=\"dim\">{}:{}:{} · {}</span></li>",
        tag(edge.evidence),
        esc(&edge.path),
        edge.line,
        edge.column,
        esc(&edge.note)
    )
}

fn edge_list(title: &str, edges: Option<&Vec<&Edge>>, outgoing: bool) -> String {
    let Some(edges) = edges else {
        return String::new();
    };
    let items: String = edges.iter().map(|e| edge_item(e, outgoing)).collect();
    format!("<p>{title}</p><ul>{items}</ul>")
}

pub fn render(report: &Report) -> String {
    let anchors: HashMap<&str, usize> = report
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| (file.path.as_str(), index))
        .collect();
    let mut outgoing: HashMap<&str, Vec<&Edge>> = HashMap::new();
    let mut incoming: HashMap<&str, Vec<&Edge>> = HashMap::new();
    let mut file_scope: HashMap<&str, Vec<&Edge>> = HashMap::new();
    for edge in &report.calls {
        match &edge.from_id {
            Some(id) => outgoing.entry(id).or_default().push(edge),
            None => file_scope.entry(&edge.path).or_default().push(edge),
        }
        if let Some(id) = &edge.to_id {
            incoming.entry(id).or_default().push(edge);
        }
    }

    let mut h = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>Tracemark report</title><style>{STYLE}</style></head><body>\
         <h1>Tracemark report</h1>\
         <p class=\"dim\">Project <code>{}</code> · engine {} · generated at unix time {}</p>",
        esc(&report.root),
        report.engine_version,
        report.generated_at_unix
    );
    h.push_str(
        "<div class=\"note\"><strong>What this report claims.</strong> Symbols, imports and \
         call sites are read from syntax: they exist in the source at the stated position. \
         A call marked <em>Inferred</em> is linked to a symbol because exactly one name \
         matched; it was not type-checked or observed running. <em>Unresolved</em> calls \
         may target libraries, built-ins or dynamic code. Nothing here says the code works.\
         <br><strong>Evidence classes.</strong> <em>syntax</em>: written in the source at that \
         position. <em>static analysis</em>: an import matched to a file by path convention. \
         <em>inferred</em>: a call matched to a symbol by name. This report contains no \
         runtime, test or AI evidence.\
         <br><strong>Not seen.</strong> <code>x.f()</code> is linked only when the receiver is \
         <code>this</code> or <code>self</code>. Rust calls inside macros are not seen. \n         JSX such as <code>&lt;Card /&gt;</code> is not read as a call. Rust <code>Type::f()</code> \n         is matched on <code>f</code> alone.</div>",
    );

    h.push_str(
        "<h2>Languages</h2><table><tr><th>Language</th><th>Extensions</th>\
         <th>Export detection</th><th>Import resolution</th><th>Call resolution</th></tr>",
    );
    for language in &report.languages {
        h.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            language.id,
            language.extensions.join(", "),
            if language.capabilities.export_detection {
                "yes"
            } else {
                "no"
            },
            language.capabilities.import_resolution,
            language.capabilities.call_resolution
        ));
    }
    h.push_str("</table><h2>Files</h2><ul>");
    for (index, file) in report.files.iter().enumerate() {
        h.push_str(&format!(
            "<li><a href=\"#f{index}\"><code>{}</code></a> <span class=\"dim\">{} · {} symbol(s)</span></li>",
            esc(&file.path),
            esc(&file.language),
            file.symbols.len()
        ));
    }
    h.push_str("</ul>");

    for (index, file) in report.files.iter().enumerate() {
        h.push_str(&format!(
            "<section id=\"f{index}\"><h2><code>{}</code></h2><p class=\"dim\">{} · {}</p>",
            esc(&file.path),
            esc(&file.language),
            esc(&file.parse_status)
        ));
        if !file.diagnostics.is_empty() {
            h.push_str("<p>Parser diagnostics</p><ul>");
            for d in &file.diagnostics {
                h.push_str(&format!(
                    "<li>{} <span class=\"dim\">line {}, column {}</span></li>",
                    esc(&d.message),
                    d.line,
                    d.column
                ));
            }
            h.push_str("</ul>");
        }
        if !file.imports.is_empty() {
            h.push_str("<p>Imports</p><ul>");
            for import in &file.imports {
                let target = match import
                    .resolved_path
                    .as_deref()
                    .and_then(|p| anchors.get(p).map(|i| (p, i)))
                {
                    Some((path, i)) => {
                        format!(" → <a href=\"#f{i}\"><code>{}</code></a>", esc(path))
                    }
                    None => " <span class=\"dim\">not resolved to a project file</span>".into(),
                };
                h.push_str(&format!(
                    "<li><code>{}</code>{target} {} <span class=\"dim\">line {}</span></li>",
                    esc(&import.source),
                    tag(import.evidence),
                    import.line
                ));
            }
            h.push_str("</ul>");
        }
        h.push_str(&edge_list(
            "Calls at file scope",
            file_scope.get(file.path.as_str()),
            true,
        ));
        for symbol in &file.symbols {
            let exported = match crate::visibility(&symbol.language, symbol.exported) {
                Some(label) if symbol.exported == Some(true) => {
                    format!(" <span class=\"tag\">{label}</span>")
                }
                _ => String::new(),
            };
            h.push_str(&format!(
                "<div class=\"sym\" id=\"s-{}\"><h3><code>{}</code> <span class=\"tag\">{}</span>{exported} {} \
                 <span class=\"dim\">lines {}-{}</span></h3>",
                esc(&symbol.id),
                esc(&symbol.qualified_name),
                esc(&symbol.kind),
                tag(symbol.evidence),
                symbol.start_line,
                symbol.end_line
            ));
            if let Some(signature) = &symbol.signature {
                h.push_str(&format!("<pre>{}</pre>", esc(signature)));
            }
            h.push_str(&edge_list("Calls", outgoing.get(symbol.id.as_str()), true));
            h.push_str(&edge_list(
                "Called from",
                incoming.get(symbol.id.as_str()),
                false,
            ));
            h.push_str("</div>");
        }
        h.push_str("</section>");
    }
    h.push_str("</body></html>\n");
    h
}
