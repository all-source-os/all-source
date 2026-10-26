---
name: allstack-delivery
description: Continue an Allstack plan, build, verify, and ship run across Codex CLI and Claude Code with retained evidence and explicit handoff.
---

# Allstack delivery

Read repository instructions and the [installed CLI guide](../../../docs/reference/allstack.md). Use the installed `allstack` executable and the original repository as `-C`; edit only the active detached worktree returned by the run.

Run `status <run>` and `resume <run> --epoch <current-epoch>` before continuing. Read the request, indexed plan, source-linked Prime context, current stage, workspace, and pending operation. Inspect recalled sources before treating them as requirements. The coordinator owns the Chronis claim; do not change it independently.

Use `plan` and `build` with the current epoch to invoke the selected host. Follow its ordinary authentication, repository rules, and permission controls. Never add bypass flags. Follow the frozen request and plan; explicitly stage intended new files before checkpointing. Do not commit, push, create branches, alter run state, or change verification policy from a managed build.

Use `handoff <run> --epoch <epoch> --to codex|claude-code` for a sequential transfer. Read the returned epoch and workspace, then stop editing the outgoing workspace. Native tools remain outside epoch enforcement.

After failure, inspect `status`, `history`, active edits, and private process evidence. Use `recover` for uncertain host/handoff work only after inspection. Recovery preserves edits without replaying the uncertain call. Use `verify` or `ship` for their destination-specific pending operations.

Run `verify <run> --epoch <epoch> --message <message>` only under trusted user commit authority. Require positive executed Cargo/nextest test counts on the exact candidate commit. Run `ship <run> --epoch <epoch> --title <title>` only under trusted publication authority. Confirm remote/PR readbacks, Chronis completion, and Prime outcome before reporting completion. Merge and deployment are separate actions. Keep private stores, transcripts, credentials, and snapshots out of public documents.
