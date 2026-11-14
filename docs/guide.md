# Tracemark quick guide

Tracemark maps a codebase and tells you how sure it is about each fact. Use it to understand code you didn't write, especially code from AI agents, and to check a change before you approve it.

Everything runs on your computer. Nothing is uploaded, and no AI is involved.

## 1. Install

- **Desktop app:** download the installer for your system from the [latest release](https://github.com/Gabions256/tracemark/releases/latest). The README's Install section explains the "unknown publisher" warnings you will see.
- **Command line:**
  - macOS and Linux: `brew install gabions256/tap/tracemark`
  - Any system: download the `tracemark-v<version>-<system>` archive from the release.

## 2. Understand a project (desktop app)

1. Click **Open project folder…** and choose your project. Tracemark indexes it in a few seconds and shows how many files, symbols and calls it found.
2. In **Find a symbol**, type part of a name: a function, class, method and so on.
3. Click a symbol. You see:
   - where it is (file and lines) and its signature;
   - **Calls:** what it calls;
   - **Called from:** what calls it.
4. Click any blue name to jump to that symbol. Follow the chain to see how the pieces connect.

## 3. Check a change before you approve it

1. Make or receive the change in your project (for example, let an AI agent edit it).
2. Open the **Review a change** tab. Leave `HEAD` to compare with your last commit, or type a branch such as `main`.
3. Click **Review**. You get one of four recommendations:

| Recommendation | Meaning |
| --- | --- |
| **Request changes** | Something is broken for sure. For example, a deleted function is still called somewhere. |
| **Review carefully before approving** | A likely break. For example, a function's parameters changed but some callers were not updated. |
| **Approve after checking the listed items** | A wide reach. Many places call what changed. |
| **No rule flagged a risk** | The rules found nothing. That is not proof the change is correct. |

Every finding shows the exact file:line behind it. Open those places and check them.

## 4. Read the evidence labels

| Label | How it is known | How much to trust it |
| --- | --- | --- |
| **syntax** | It is written in the source at that position. | A fact about the text. |
| **static analysis** | Worked out by a rule, for example matching an import to a file. | Usually right. The rule is stated. |
| **inferred** | Matched by name only, not type-checked. | A lead. Check it. |

Tracemark never says code works. It shows what is written, what is connected, and what changed.

## 5. The same from the command line

    tracemark scan .                              # index the current folder
    tracemark symbols list checkout               # find symbols by name
    tracemark graph dependents handleCheckout     # what calls it
    tracemark explain function handleCheckout     # its structure and limits
    tracemark review --base main                  # check a change
    tracemark report --format html > report.html  # a page to share

Add `--json` to any command for machine-readable output.

## 6. On every pull request

Add the GitHub Action from the README's "On every pull request" section. Each pull request then gets one comment with the review, updated on every push.

## 7. With AI coding agents

Connect Tracemark so the agent can look things up instead of guessing:

    claude mcp add tracemark -- tracemark mcp --project /path/to/project

The agent can then find symbols, follow calls and review its own edits before calling them done. The README section "Use from AI coding agents" covers other agents.

## Limits to keep in mind

- Calls are linked by name. A call written as `x.f()` is followed only when `x` is `this` or `self`.
- Rust calls inside macros (`println!`, `assert!`) are not seen.
- JSX such as `<Card />` is not a call. A React component used only in JSX shows no callers; that does not mean it is unused.
- Rust `Type::f()` is matched on `f` alone, so `Vec::new()` can link to an unrelated project `new`.
- `review` does not check imports: a renamed or deleted file whose importers still use the old path is not flagged.
- Rust and Python imports are not resolved to files.
- Renaming a symbol shows as one removed and one added.
- Supported languages: TypeScript, JavaScript (with JSX/TSX), Rust and Python.
