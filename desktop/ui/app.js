// Tracemark desktop UI. Every string from the project is inserted with
// textContent, never as HTML.
"use strict";

const { invoke } = window.__TAURI__.core;
const { open } = window.__TAURI__.dialog;
const $ = (id) => document.getElementById(id);

function el(tag, props = {}, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (key === "class") node.className = value;
    else if (key === "text") node.textContent = value;
    else if (key === "onclick") node.addEventListener("click", value);
    else node.setAttribute(key, value);
  }
  node.append(...children);
  return node;
}

function status(message, isError = false) {
  const line = $("status");
  line.textContent = message;
  line.classList.toggle("error", isError);
}

const EVIDENCE = { Syntax: "syntax", StaticAnalysis: "static analysis", Inferred: "inferred" };

function evidenceTag(evidence) {
  return el("span", { class: `tag ev-${evidence}`, title: "Evidence class", text: EVIDENCE[evidence] || evidence });
}

function place(path, line, column) {
  return [path, line, column].filter((part) => part !== undefined && part !== 0).join(":");
}

// ---- Tabs --------------------------------------------------------------

function selectTab(name) {
  for (const tab of ["explore", "review"]) {
    const selected = tab === name;
    const button = $(`tab-${tab}`);
    button.setAttribute("aria-selected", String(selected));
    button.tabIndex = selected ? 0 : -1;
    $(tab).hidden = !selected;
  }
}
$("tab-explore").addEventListener("click", () => selectTab("explore"));
$("tab-review").addEventListener("click", () => selectTab("review"));
document.querySelector("nav").addEventListener("keydown", (event) => {
  if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
  const next = $("tab-explore").getAttribute("aria-selected") === "true" ? "review" : "explore";
  selectTab(next);
  $(`tab-${next}`).focus();
});

// ---- Open a project ----------------------------------------------------

$("open").addEventListener("click", async () => {
  const path = await open({ directory: true, title: "Choose a project folder" });
  if (!path) return;
  status("Scanning…");
  try {
    const summary = await invoke("open_project", { path });
    $("project").textContent = path;
    let message = `${summary.files} file(s), ${summary.symbols} symbol(s), ${summary.calls} call site(s), ${summary.calls_inferred} linked by name.`;
    const problems = summary.warnings.length + summary.skipped.length;
    if (problems) message += ` ${problems} warning(s): some files were not indexed.`;
    status(message);
    for (const id of ["search", "base", "run-review"]) $(id).disabled = false;
    $("detail").replaceChildren(el("p", { class: "dim", text: "Pick a symbol to see what it calls and what calls it." }));
    await loadSymbols("");
  } catch (error) {
    status(String(error), true);
  }
});

// ---- Symbols -----------------------------------------------------------

let searchTimer;
$("search").addEventListener("input", (event) => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => loadSymbols(event.target.value), 200);
});

async function loadSymbols(query) {
  try {
    const result = await invoke("symbols", { query });
    const list = $("symbols");
    list.replaceChildren(
      ...result.symbols.map((s) =>
        el("li", {},
          el("button", { type: "button", onclick: () => showDetail(s.id) },
            el("span", { class: "kind", text: s.kind }),
            el("span", { class: "name", text: s.qualified_name }),
            el("span", { class: "dim loc", text: place(s.path, s.start_line) })))));
    $("list-note").textContent = result.truncated
      ? "Showing the first 500. Type to narrow the list."
      : `${result.symbols.length} symbol(s)`;
  } catch (error) {
    status(String(error), true);
  }
}

// ---- Detail ------------------------------------------------------------

function edgeList(edges, outgoing) {
  if (!edges.length) return el("p", { class: "dim", text: outgoing ? "No calls found in its body." : "No call sites are linked to it." });
  return el("ul", { class: "edges" },
    ...edges.map((e) => {
      const id = outgoing ? e.to_id : e.from_id;
      const name = outgoing ? e.to : e.from;
      const target = id
        ? el("button", { type: "button", class: "link", onclick: () => showDetail(id) }, el("code", { text: name }))
        : el("code", { text: name });
      const parts = [target, " "];
      if (!e.to_id) parts.push(el("span", { class: "tag unresolved", text: e.status }), " ");
      parts.push(evidenceTag(e.evidence), " ",
        el("span", { class: "dim", text: `${place(e.path, e.line, e.column)} · ${e.note}` }));
      return el("li", {}, ...parts);
    }));
}

async function showDetail(id) {
  try {
    const { symbol, calls, called_by } = await invoke("symbol_detail", { id });
    const header = el("div", { class: "detail-head" },
      el("h2", {}, el("code", { text: symbol.qualified_name })),
      el("span", { class: "tag", text: symbol.kind }), " ",
      evidenceTag(symbol.evidence));
    if (symbol.exported === true) header.append(" ", el("span", { class: "tag", text: symbol.language === "rust" ? "pub" : "exported" }));
    const body = [header, el("p", { class: "dim", text: `${symbol.path}, lines ${symbol.start_line}-${symbol.end_line}` })];
    if (symbol.signature) body.push(el("pre", { text: symbol.signature }));
    body.push(el("h3", { text: "Calls" }), edgeList(calls, true),
      el("h3", { text: "Called from" }), edgeList(called_by, false),
      el("p", { class: "dim small", text: "Calls are linked by name. Member calls other than this/self, dynamic calls and Rust calls inside macros are not found." }));
    $("detail").replaceChildren(...body);
  } catch (error) {
    status(String(error), true);
  }
}

// ---- Review ------------------------------------------------------------

const RECOMMENDATION = {
  RequestChanges: "Request changes",
  ReviewCarefully: "Review carefully before approving",
  ApproveAfterChecks: "Approve after checking the listed items",
  NoRuleFlagged: "No rule flagged a risk",
};

$("review-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  $("run-review").disabled = true;
  status("Reviewing…");
  try {
    const review = await invoke("review", { base: $("base").value });
    renderReview(review);
    status(`Review done: ${review.findings.length} finding(s).`);
  } catch (error) {
    status(String(error), true);
  } finally {
    $("run-review").disabled = false;
  }
});

function renderReview(review) {
  const short = review.changes.base_commit.slice(0, 7);
  const parts = [
    el("div", { class: `banner rec-${review.recommendation}` },
      el("strong", { text: RECOMMENDATION[review.recommendation] || review.recommendation }),
      el("p", { text: review.reason })),
    el("p", { class: "dim", text: `Compared with ${review.changes.base} (${short}). ${review.changes.files.length} file(s) changed.` }),
  ];
  if (review.findings.length) {
    parts.push(el("ul", { class: "findings" },
      ...review.findings.map((f) => {
        const item = el("li", {},
          el("span", { class: `tag sev-${f.severity}`, text: f.severity }), " ",
          el("code", { text: f.rule }), " ",
          el("span", { class: "dim", text: place(f.path, f.line) }), " ",
          f.symbol ? el("code", { text: f.symbol }) : "", " ",
          evidenceTag(f.evidence),
          el("p", { text: f.message }));
        if (f.sites.length) {
          item.append(el("details", {},
            el("summary", { text: `${f.sites.length} call site(s)` }),
            edgeList(f.sites, true)));
        }
        return item;
      })));
  } else {
    parts.push(el("p", { text: "No findings." }));
  }
  parts.push(el("ul", { class: "limits" }, ...review.limitations.map((l) => el("li", { class: "dim small", text: l }))));
  $("review-result").replaceChildren(...parts);
}
