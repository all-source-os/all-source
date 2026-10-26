# Allstack CLI and recovery

This repository uses the installed Allstack implementation preview. AllSource Core retains local workflow history, Chronis owns tasks and claims, and embedded Prime supplies source-linked lexical recall. Hosts execute sequentially on one Unix machine. No hosted account or Allstack source checkout is needed.

## Repository setup

Run `allstack init --check --profile rust` to inspect proposed changes without writing repository files. Apply with `allstack init --profile rust`. The Rust profile requires `Cargo.toml` and `Cargo.lock`; it installs formatting, positive executed Cargo tests, and whitespace checks. Review the policy against this repository's acceptance requirements before committing it. Initialization creates no execution proof.

For a custom stack, supply an existing policy with `allstack init --policy workflows/custom.toml`. The policy needs at least one Cargo/nextest executed-test gate with a success marker and command deadlines within 300 seconds. Zero selected tests and compilation alone are inconclusive. Mark every compiling gate `compile = true`; Allstack checks competing compilers and serializes managed builds. Unsupported test-runner output remains inconclusive.

Initialization preserves existing policies and repository instructions. Different managed files are conflicts. Ignore/index additions preserve existing bytes; reruns fill only missing resources. Check mode defers existing Chronis engine readback until apply because opening that engine can write storage metadata. Its readiness covers resource and executable preflight. A failure may leave named completed writes; inspect the result and rerun after resolving the cause. No force-overwrite mode is provided.

Both host skills are repository-local under `.agents/skills/allstack-delivery/` and `.claude/skills/allstack-delivery/`. Host discovery and authentication follow each host's ordinary setup. Run `allstack doctor` to inspect setup completeness and executable availability. Version probes do not prove authentication or successful model execution. Doctor reads Chronis through `cn`; it does not create a run or open the mutable AllSource/Prime stores.

Review and commit initialized inputs through the repository's normal process. Start from an existing, explicitly authorized feature branch with a GitHub `origin` and clean working tree. Check its PR is live. Prepare ignored dependencies and build output in new detached worktrees when needed. Initialization does not create branches or authorize commits/publication.

## Complete delivery flow

Create and index a request document with acceptance criteria and required evidence. Use the original repository as the CLI target throughout; substitute the returned run ID and current epoch at every step.

```sh
allstack -C /path/to/repository start --request docs/requests/change.md \
  --policy workflows/verification.toml --host codex --allow-commit --allow-publish
allstack -C /path/to/repository plan <run> --epoch 1
allstack -C /path/to/repository handoff <run> --epoch 1 --to claude-code
allstack -C /path/to/repository resume <run> --epoch 2
allstack -C /path/to/repository build <run> --epoch 2
allstack -C /path/to/repository handoff <run> --epoch 2 --to codex
allstack -C /path/to/repository verify <run> --epoch 3 --message "feat(example): implement accepted change"
allstack -C /path/to/repository ship <run> --epoch 3 --title "feat(example): implement accepted change"
```

Pass authority flags only when trusted user instructions authorize those actions. Reverse the hosts by starting with `--host claude-code` and handing off to `codex`. Allstack creates and reads back the Chronis claim. The incoming host receives the retained plan, request, immutable snapshot, and source-linked context. It does not need a copied conversation.

Edit only the active worktree from `status` or `resume`. Explicitly stage each intended new file there with `git add -- <path>`, then use `checkpoint <run> --epoch <epoch>`. Untracked files block snapshots. Snapshots reject symlinks, recognized sensitive filenames, files above 4 MiB, totals above 64 MiB, and more than 10,000 files. Filename checks do not detect every secret.

## Recovery and evidence

Inspect `status <run>`, `history <run>`, active workspace, and private process receipts before retrying failures. An unresolved operation blocks managed execution. `recover <run> --epoch <epoch>` reconciles task setup, host calls, handoffs, and pending plan indexing. It preserves host edits without replaying uncertain calls; an absent accepted plan still requires deliberate planning. Always use the epoch returned after recovery.

Repeat `verify` for pending commit/verification work. It materializes the accepted checkpoint into the original checkout, creates or amends the authorized owned commit, and runs frozen gates on that exact SHA. Do not edit or switch the original checkout while it runs. Changes to candidate content, workflow, methods, policy, or bound environment invalidate verification.

Repeat `ship` for pending publication/task/outcome work. It reads the destination before retrying, confirms the exact remote commit and open PR head, completes Chronis, indexes Prime, and removes only unchanged run-owned worktrees. Unexpected edits preserve a workspace for inspection. Ship means one feature-branch commit and an open PR; merge and deployment remain separate.

Use `evidence <run>` for digest-validated gate receipts and `recall <query>` for local source-linked Prime context. Recall cannot authorize actions or certify checks. Review excerpts before publishing evidence in indexed repository documents.

`.allstack/`, `.chronis/`, and `.prime/` are private ignored stores. Read them through their engines or CLI tools, never by scraping WAL segments. Allstack holds an exclusive store lock per command, fsyncs workflow events, and owns/kills/reaps child processes within bounded deadlines. Epochs fence managed transitions; native tools remain outside that boundary. Missing objects, unknown host effects, or claim mismatches require inspection and reconciliation.
