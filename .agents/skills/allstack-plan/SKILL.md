---
name: allstack-plan
description: Turn an accepted change into a source-grounded engineering plan covering architecture, failure behavior, compatibility, and executable acceptance; usable without starting delivery.
---

# Plan the change

Use native repository tools. Independent planning creates no delivery run or Chronis task and grants no commit or publication permission. In a managed invocation, keep the coordinator's read-only scope; return the plan for indexing instead of writing files yourself.

## Establish the current path

Read the request, repository instructions, relevant code, and actual gates. Trace the entry point to the state owner and output. Recall related Prime decisions when available, then inspect their source. Treat missing evidence as an unresolved assumption. Confirm whether the behavior belongs in workflow/configuration data or generic engine code before choosing a repair site.

## Review the design

- **Architecture:** choose the owner of each invariant; list the smallest coherent files and boundaries that must change. Compare an existing seam with a new abstraction and explain the choice.
- **Failure behavior:** follow invalid input, empty results, partial writes, retries, interruption, and duplicate execution where relevant. Name which component detects each failure and what the user sees.
- **Compatibility:** inspect current callers, serialized data, command output, and persisted state that the change touches. Specify what remains stable and how compatibility will be exercised.
- For persisted contracts, inspect the producer's actual revision/count semantics and representative retained history, including legacy records. Include an existing consumer's complete job; a fresh synthetic fixture can miss a supported caller's next operation.
- **Data correctness:** identify units, rounding, ordering, identifiers, and trust boundaries. For money, use exact minor units and checked arithmetic; never hide malformed input behind a favorable result.
- **Verification:** map each user-visible requirement to a real command or runtime action. Include a negative case that would catch the likely defect, with its expected exit and output channels. State when a build only compiles and when a test actually executes. Name how the executable will be rebuilt from the intended source; a digest alone cannot establish source provenance, and cloned warm artifacts can retain stale timestamps.

Compare the frozen delivery policy with the changed components. Name supplemental checks and CI where the policy covers another component; its green result proves only its executed commands. Bind each acceptance claim to its source revision, target, host, and observable result.

Do not expand the assignment into speculative platforms, services, or catalog entries. Escalate a concrete decision only when choosing without it risks the wrong outcome. Do not translate unknowns into time estimates.

## Hand off an executable plan

Return the complete human flow, current behavior, proposed behavior, exact files, implementation sequence, acceptance commands, prerequisites, and unresolved permissions. Bind checks to the actual repository tools. Preserve the request's scope when another host implements it.

Choose the durable coordinator before starting delivery. Prefer an exclusively owned clean primary checkout; isolate unrelated dirty work when needed. Preserve the project's existing Chronis history and Prime context. Moving a coordinator does not make its path-bound run history valid at the new path; archive that history before removing an owned checkout.

For standalone work, save and index the plan under docs when requested or required by repository policy. Record accepted source-linked decisions with `allstack remember` when available. For an explicitly requested durable delivery, the host can use the coordinator guide to start and manage it behind this interface. Do not require durable delivery merely to produce a plan.

## Host capability

Codex: invoke as $allstack-<name> or request its name naturally. Use native repository tools and its current sandbox. A nested host command requires the same applicable authority as a direct call.
