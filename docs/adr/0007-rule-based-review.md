# ADR 0007: Rule-based review and recommendation

Status: accepted (0.3.0)

## Context

The product's promise is "Trust every change". Code from AI agents arrives in large changes that reviewers cannot read line by line. `tracemark changes` lists what changed and what may be affected, but leaves the judgement to the reader. Users asked for a recommendation: approve or not.

Tools that make that judgement with a language model give confident answers they cannot back up. That conflicts with the project's rules: no inferred claim may be presented as confirmed, and AI output must be labelled as AI.

## Decision

1. `tracemark review` applies a fixed, published list of rules (`RULES`) to the change report. Each rule has a severity: blocker, high, medium or info.
2. The recommendation is a pure function of the most severe finding. It is never "the change is correct". The weakest outcome is worded "No rule flagged a risk", and the output always says the code was not run, built or tested.
3. Each finding carries the evidence class of its claim, and the call sites behind it with their own classes.
4. A removed symbol is a blocker only when its remaining call sites link to nothing. A call that now resolves to another symbol of the same name is not counted.
5. A signature change is high only for callers in files the change did not touch. Callers in edited files are assumed to have been updated, since they were part of the change.
6. Markdown output puts every project-supplied name and path in a code span, so a hostile file or function name cannot add links, mentions or HTML to a pull-request comment.
7. The GitHub Action is a composite action. It downloads a release, checks its SHA-256, and passes all inputs through environment variables, never inline in scripts.

## Why

- Rules are explainable and testable, and give the same answer every time. A reviewer can check each finding against the listed call sites.
- No AI keeps the tool usable offline and free of source upload. An optional AI layer can come later, labelled `AiSuggestion` (see ADR 0003).

## Consequences

- Findings inherit the limits of name-based linking. Impact can be larger than shown, and the output says so.
- `no-test-calls` only sees direct calls by name from test files, so it is informational only.

## When to revisit

- Add rules when users report risky changes the current rules miss.
- Weight findings by test reachability once call resolution is type-aware.
