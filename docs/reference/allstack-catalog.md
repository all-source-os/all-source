# Allstack workflow catalog

Run `allstack catalog` to inspect repository workflows without opening run stores. Run `allstack catalog bugfix` for its definition path, methods, supported hosts, lifecycle, and digests. The catalog is versioned at `workflows/catalog.toml`. Existing repositories without that file retain the default delivery workflow.

`delivery` plans and implements an accepted change. `bugfix` diagnoses an evidenced defect and repairs its cause with regression coverage. Both use plan → build → verify → ship, sequential Codex CLI and Claude Code hosts, immutable handoff snapshots, Chronis claims, and source-linked Prime context. Planning is read-only; the coordinator executes compilation and final verification. Supply retained failure output in the indexed request when diagnosis needs execution evidence.

```sh
allstack start --workflow bugfix --request docs/requests/repair.md \
  --policy workflows/verification.toml --host codex --allow-commit --allow-publish
```

Start only from an authorized live feature branch. Authority flags require trusted user instructions. Use the returned run ID and current epoch with the [CLI and recovery guide](allstack.md). Omitting `--workflow` selects `delivery`. Selected workflow and method bytes stay frozen through handoff, recovery, verification, and publication. Catalog edits cannot redirect an accepted run or grant permissions.

Run `allstack init --check --profile rust`, then `allstack init --profile rust` to add bundled catalog resources. Existing delivery resources are preserved byte-for-byte. Conflicting managed resources block initialization before writes; resolve conflicts explicitly. Existing custom policies remain intact. Review and commit initialized files through normal repository policy.

The catalog supports this fixed lifecycle only. Definitions cannot weaken supported host, handoff, storage, process, verification, or publication constraints. AllSource owns run history, Chronis owns task state, and embedded Prime remains local lexical context. Shipping confirms the verified remote commit and one open PR; merge and deployment are separate actions.
