---
name: allstack-recall
description: Recover source-linked project decisions and prior outcomes from Prime without treating memory as execution state or permission.
---

# Recover project context

Turn the user's question into a specific query. Use connected project Prime tools when their health and store identity match the repository; otherwise use `allstack recall <query>` for the repository's local memory. Report which store was read. Local lexical recall and hosted/vector retrieval have different coverage; do not claim either was used without evidence.

Inspect the cited source for every result that affects the recommendation. A retained `source_digest` identifies the file version remembered; compare current bytes before carrying an old decision forward. If a source changed or disappeared, label the result historical and inspect its successor. Missing search results do not prove the project never made the decision.

An explicit absent-store result is a verification limit. Continue from the supplied indexed plan and relevant project source; do not search the toolkit's implementation to invent missing memory or create a replacement store merely to satisfy recall.

Allstack's repository commands require a Git checkout. When recovering archived Prime context outside one, open an archived copy through Prime's own engine with an explicit data path; a Git-context error does not establish missing data. Probe store identity, counts and record dates, then compare the retained source documents and digests. Keep this context recovery separate from path-bound coordinator history, and never fold store files through an unrelated engine.

Distinguish proposals, accepted decisions, observed outcomes, and actual current state. Query AllSource workflow state, Chronis ownership, or GitHub publication through their own tools when that is what the question asks. Prime context cannot establish those facts or grant authority.

Return the relevant decision, why it applies, source path, and any conflicting or superseding evidence. Do not dump unrelated memory. Preserve useful accepted knowledge through `allstack remember --kind decision|review|qa --source <indexed-doc> --summary <concise-result>` when requested or within an authorized documentation workflow. Never store secrets, proprietary source, or full file contents.

## Host capability

Codex: invoke as $allstack-<name> or request its name naturally. Use native repository tools and its current sandbox. A nested host command requires the same applicable authority as a direct call.
