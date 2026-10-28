# Native Allstack skills

Invoke skills directly in your coding host. Codex accepts `$allstack-shape` and requests by skill name; Claude Code accepts `/allstack-shape`. The installed methods share original source and carry a small host capability overlay. Reopen an existing host session if its skill discovery requires it.

| Skill | Use |
| --- | --- |
| `allstack-shape` | Challenge user demand, alternatives, and the complete job. |
| `allstack-plan` | Plan architecture, failure behavior, compatibility, and acceptance. |
| `allstack-review` | Inspect an existing branch, diff, or PR for evidenced defects. |
| `allstack-qa` | Exercise the real UI, API, CLI, or job using available tools. |
| `allstack-ship` | Verify and publish an authorized change through managed delivery. |
| `allstack-recall` | Recover relevant decisions and inspect their sources. |
| `allstack-handoff` | Explicitly continue an active delivery in another coding host. |

Install just this interface with `allstack init --skills-only --check`, then `allstack init --skills-only`. No Rust project, Chronis executable, task, delivery run, or Prime store is needed for installation or independent shaping, planning, review, and QA. Existing managed installation also includes these skills. Conflicting files are preserved and reported before writes. Review initialized files before committing them.

`allstack skills` lists bundled methods without opening stores. `allstack skills review` returns the shared instructions. Native skills use the host's existing tools and permissions; unavailable browser or runtime capabilities remain unverified. A skill is guidance, not a sandbox or permission grant.

For CLI/runtime evidence, `allstack check --expect-exit 4 --seconds 120 -- /absolute/program arguments` records expected and observed exit, source checkout, executable identity, and separate stdout/stderr under ignored `.allstack/checks/`. Omit `--expect-exit` for zero. `--input prompt.txt` supplies up to 128 KiB of UTF-8 stdin, including native host prompts. The 2..270 second deadline covers metadata and execution; all children remain supervised. A matching expected negative exit succeeds. Failures retain a receipt and return nonzero. Raw stream paths and digests preserve evidence even when a host combines channels. This command creates no delivery run, Chronis task, or Prime store. Rebuild the application from intended sources before trusting its digest as candidate evidence.

Installation preserves conflicting files. To update a generated skill, compare the installed file with its known previous generated version, review the new shared method and host overlay, and preserve local customization. Resolve reviewed replacements explicitly; initialization never silently overwrites them.

After an update or observed runtime drift, bind executable bytes to their source/artifact and packaged methods, then pin the selected paths across hosts. A version string does not identify the installed methods. Serialization QA should freeze independent inputs/expected output and exercise the changed positions as well as retained-data equality.

Save plans and required reports under indexed repo docs. `allstack remember --kind decision --source docs/plans/change.md --summary 'Accepted decision'` records a bounded source-linked summary and source digest in local Prime. Use `allstack recall 'relevant query'` to retrieve context; inspect the remembered source before relying on it. Memory creates no delivery run or task. Keep private stores ignored; never record secrets or full source contents.

For a complete durable delivery, initialize with a project policy using the CLI/recovery guide. `allstack current` discovers the single active run behind the skill interface. Multiple active runs require explicit selection. `allstack adopt --request docs/requests/change.md --plan docs/plans/change.md --policy workflows/verification.toml --host codex --allow-commit --allow-publish` enrolls a completed native candidate and its existing plan without repeating model work. Stage intended new files explicitly before adoption. Initialize real Chronis first; branch, commit, and publication authority still come from the user.

AllSource owns managed run history and receipts, Chronis owns delivery claims and completion, and Prime supplies source-linked context. The coordinator retains exact-commit verification, immutable handoffs, bounded process ownership, and destination reconciliation. Its plan/build host calls remain more restricted than independent native execution. Shipping ends at a verified remote commit and open PR; merge and deployment remain separate actions.

Keep the coordinator at its original path through verification and publication. Native adoption verifies the candidate in that checkout; an epoch worktree may retain the base. Preserve existing project tracker/memory state and archive path-bound history before removing an isolated coordinator.

Archive required epoch-local receipts and raw streams before `allstack ship`, which can remove accepted epoch worktrees. Verify copied byte lengths and digests. Report host execution from actual calls, preserve failed HTTP observations with safely redacted responses, and distinguish running-image proof from final-commit checks. Existing consumer compatibility and changed-component checks belong in acceptance even when a frozen policy covers another component.

Preserve the application binaries, inputs and path bindings a copied harness needs before cleanup. Repository-bound recall needs Git; recover an archived Prime copy through its own engine and verify counts/dates plus source digests. This recovers context without relocating coordinator history. Inspect actual branch rules before treating visible CI as merge enforcement.
