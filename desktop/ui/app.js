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
  node.append(...children.filter((c) => c !== null && c !== undefined && c !== ""));
  return node;
}

let busyCount = 0;
function status(message, kind = "info") {
  const line = $("status");
  line.className = `status ${kind}`;
  line.replaceChildren(kind === "busy" ? el("span", { class: "spinner", "aria-hidden": "true" }) : "", message);
}

async function busy(message, work) {
  busyCount += 1;
  document.body.classList.add("busy");
  status(message, "busy");
  try {
    return await work();
  } catch (error) {
    status(String(error), "error");
    return undefined;
  } finally {
    busyCount -= 1;
    if (!busyCount) document.body.classList.remove("busy");
  }
}

const EVIDENCE = { Syntax: "syntax", StaticAnalysis: "static analysis", Inferred: "inferred" };
const evidenceTag = (e) => el("span", { class: `ev ev-${e}`, title: "Evidence class", text: EVIDENCE[e] || e });

const KIND = {
  Function: ["fn", "k-fn"], Method: ["method", "k-fn"], Class: ["class", "k-type"],
  Struct: ["struct", "k-type"], Interface: ["interface", "k-iface"], Trait: ["trait", "k-iface"],
  Module: ["module", "k-mod"], Enum: ["enum", "k-type"], TypeAlias: ["type", "k-iface"],
};
const kindTag = (kind) => {
  const [label, cls] = KIND[kind] || [kind, "k-mod"];
  return el("span", { class: `kind ${cls}`, text: label });
};

const place = (path, line, column) =>
  [path, line, column].filter((part) => part !== undefined && part !== 0).join(":");

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
$("tabs").addEventListener("keydown", (event) => {
  if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
  const next = $("tab-explore").getAttribute("aria-selected") === "true" ? "review" : "explore";
  selectTab(next);
  $(`tab-${next}`).focus();
});

// ---- Open a project ----------------------------------------------------

function stat(value, label) {
  return el("span", { class: "stat" }, el("strong", { text: String(value) }), ` ${label}`);
}

async function openProject() {
  const path = await open({ directory: true, title: "Choose a project folder" });
  if (!path) return;
  const summary = await busy("Indexing the project…", () => invoke("open_project", { path }));
  if (!summary) return;
  $("project").textContent = path;
  $("project").title = path;
  $("project-chip").hidden = false;
  $("welcome").hidden = true;
  $("tabs").hidden = false;
  selectTab("explore");
  $("stats").replaceChildren(
    stat(summary.files, "files"),
    stat(summary.symbols, "symbols"),
    stat(summary.calls, "calls"),
    stat(summary.calls_inferred, "linked"));
  const problems = summary.warnings.length + summary.skipped.length;
  status(problems
    ? `Indexed, with ${problems} warning(s): some files could not be read and are not in the index.`
    : `Indexed ${summary.files} file(s). Pick a symbol to explore.`, problems ? "warn" : "ok");
  showDetailEmpty();
  await loadSymbols("");
  $("search").focus();
}
$("open").addEventListener("click", openProject);
$("open-welcome").addEventListener("click", openProject);

document.addEventListener("keydown", (event) => {
  const mod = event.ctrlKey || event.metaKey;
  if (mod && event.key.toLowerCase() === "o") { event.preventDefault(); openProject(); }
  if (mod && event.key.toLowerCase() === "k" && !$("tabs").hidden) {
    event.preventDefault();
    selectTab("explore");
    $("search").focus();
    $("search").select();
  }
});

// ---- Symbols -----------------------------------------------------------

let searchTimer;
let selectedId = null;
$("search").addEventListener("input", (event) => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => loadSymbols(event.target.value), 180);
});

async function loadSymbols(query) {
  const result = await busy("Searching…", () => invoke("symbols", { query }));
  if (!result) return;
  const list = $("symbols");
  list.replaceChildren(...result.symbols.map((s) =>
    el("li", {},
      el("button", {
        type: "button",
        class: s.id === selectedId ? "symbol selected" : "symbol",
        "data-id": s.id,
        onclick: () => showDetail(s.id),
      },
        kindTag(s.kind),
        el("span", { class: "symbol-text" },
          el("span", { class: "symbol-name", text: s.qualified_name }),
          el("span", { class: "symbol-loc", text: place(s.path, s.start_line) }))))));
  $("list-note").textContent = result.truncated
    ? "First 500 shown. Type to narrow."
    : `${result.symbols.length} symbol${result.symbols.length === 1 ? "" : "s"}`;
  if (!result.symbols.length) {
    list.replaceChildren(el("li", { class: "list-empty", text: query ? "No symbol matches." : "No symbols found in this project." }));
  }
  if (status && $("status").classList.contains("busy")) status("");
}

// ---- Detail ------------------------------------------------------------

function showDetailEmpty() {
  $("detail").replaceChildren(el("div", { class: "empty" },
    el("p", { text: "Pick a symbol on the left." }),
    el("p", { class: "muted", text: "You will see where it is, its signature, what it calls and what calls it, each with how it is known." })));
}

function edgeList(edges, outgoing) {
  if (!edges.length) {
    return el("p", { class: "muted", text: outgoing ? "No calls found in its body." : "No call sites are linked to it." });
  }
  return el("ul", { class: "edges" }, ...edges.map((e) => {
    const id = outgoing ? e.to_id : e.from_id;
    const name = outgoing ? e.to : e.from;
    const target = id
      ? el("button", { type: "button", class: "link", onclick: () => showDetail(id) }, el("code", { text: name }))
      : el("code", { class: "plain", text: name });
    return el("li", { class: "edge" },
      el("div", { class: "edge-main" },
        target,
        !e.to_id ? el("span", { class: "pill muted-pill", text: "unresolved" }) : null,
        evidenceTag(e.evidence)),
      el("div", { class: "edge-meta" },
        el("code", { class: "loc", text: place(e.path, e.line, e.column) }),
        el("span", { text: e.note })));
  }));
}

function section(title, count, body) {
  return el("section", { class: "card" },
    el("h3", {}, title, el("span", { class: "count", text: String(count) })),
    body);
}

async function showDetail(id) {
  const detail = await busy("Loading…", () => invoke("symbol_detail", { id }));
  if (!detail) return;
  status("");
  selectedId = id;
  for (const button of document.querySelectorAll(".symbol")) {
    button.classList.toggle("selected", button.dataset.id === id);
  }
  const { symbol, calls, called_by } = detail;
  const exported = symbol.exported === true
    ? el("span", { class: "pill", text: symbol.language === "rust" ? "pub" : "exported" })
    : null;
  const head = el("div", { class: "detail-head" },
    el("div", { class: "detail-title" }, kindTag(symbol.kind), el("h2", {}, el("code", { text: symbol.qualified_name }))),
    el("div", { class: "detail-sub" },
      el("code", { class: "loc", text: `${symbol.path}  ·  lines ${symbol.start_line}–${symbol.end_line}` }),
      exported,
      evidenceTag(symbol.evidence)));
  const parts = [head];
  if (symbol.signature) parts.push(el("pre", { class: "signature", text: symbol.signature }));
  parts.push(
    section("Calls", calls.length, edgeList(calls, true)),
    section("Called from", called_by.length, edgeList(called_by, false)),
    el("p", { class: "footnote", text: "Calls are linked by name. Member calls other than this/self, dynamic calls, JSX such as <Card />, and Rust calls inside macros are not found, so real usage can be wider. Rust Type::f() is matched on f alone." }));
  $("detail").replaceChildren(...parts);
  $("detail").scrollTop = 0;
}

// ---- Review ------------------------------------------------------------

const RECOMMENDATION = {
  RequestChanges: ["Request changes", "✕"],
  ReviewCarefully: ["Review carefully before approving", "!"],
  ApproveAfterChecks: ["Approve after checking the listed items", "?"],
  NoRuleFlagged: ["No rule flagged a risk", "✓"],
};

$("review-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  $("run-review").disabled = true;
  const review = await busy("Reviewing the change…", () => invoke("review", { base: $("base").value }));
  $("run-review").disabled = false;
  if (!review) return;
  renderReview(review);
  status(`Review done: ${review.findings.length} finding(s).`, "ok");
});

function renderReview(review) {
  const [label, icon] = RECOMMENDATION[review.recommendation] || [review.recommendation, "•"];
  const counts = {};
  for (const f of review.findings) counts[f.severity] = (counts[f.severity] || 0) + 1;
  const short = review.changes.base_commit.slice(0, 7);
  const parts = [
    el("div", { class: `banner rec-${review.recommendation}` },
      el("span", { class: "banner-icon", "aria-hidden": "true", text: icon }),
      el("div", {},
        el("strong", { text: label }),
        el("p", { text: review.reason }),
        el("div", { class: "sev-counts" },
          ...["Blocker", "High", "Medium", "Info"].filter((s) => counts[s]).map((s) =>
            el("span", { class: `pill sev-${s.toLowerCase()}`, text: `${counts[s]} ${s.toLowerCase()}` }))))),
    el("p", { class: "muted", text: `Compared with ${review.changes.base} (${short}) · ${review.changes.files.length} file(s) changed` }),
  ];
  if (review.findings.length) {
    parts.push(el("ul", { class: "findings" }, ...review.findings.map((f) => {
      const card = el("li", { class: `finding sev-border-${f.severity.toLowerCase()}` },
        el("div", { class: "finding-head" },
          el("span", { class: `pill sev-${f.severity.toLowerCase()}`, text: f.severity.toLowerCase() }),
          el("code", { class: "rule", text: f.rule }),
          f.symbol ? el("code", { class: "plain", text: f.symbol }) : null,
          evidenceTag(f.evidence)),
        el("p", { text: f.message }),
        el("code", { class: "loc", text: place(f.path, f.line) }));
      if (f.sites.length) {
        card.append(el("details", {},
          el("summary", { text: `${f.sites.length} call site${f.sites.length === 1 ? "" : "s"}` }),
          edgeList(f.sites, true)));
      }
      return card;
    })));
  } else {
    parts.push(el("div", { class: "empty" }, el("p", { text: "No findings." })));
  }
  parts.push(el("ul", { class: "limits" }, ...review.limitations.map((l) => el("li", { text: l }))));
  $("review-result").replaceChildren(...parts);
}
