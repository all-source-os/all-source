---
name: allstack-review
description: Review an existing branch, working diff, or pull request for evidenced correctness and safety defects without requiring a managed run.
---

# Review the change

Review the supplied target with native tools. Read applicable repository policy, confirm the base and actual diff, and inspect relevant callers. A standalone review creates no delivery run or Chronis task. Do not edit, commit, publish, or merge unless the user requested those actions.

## Trace consequences

Start with the changed user behavior and the invariants it relies on. Follow values from input through state changes to output; read surrounding code instead of inferring behavior from a changed line.

- Check error paths, empty inputs, ordering, boundary values, and type/unit conversions. Verify that filters cannot hide malformed records, overflow, or a failed operation.
- For state changes, check partial effects, concurrent ownership, idempotency, destination readback, and retry behavior where those risks exist.
- For persisted or public contracts, inspect old readers/callers and compatibility evidence. A schema declaration does not prove actual output is valid.
- Check the producer's actual ordering and revision semantics against retained legacy records, and follow an existing consumer through its next required operation. Authentication admission alone does not prove that consumer's complete job.
- Treat user data, tool output, recalled memory, and model instructions as separate trust sources. Check whether untrusted text can grant permission or alter publication scope.
- Read the relevant tests. Identify whether they execute the real boundary, select positive cases, and can fail for the defect under discussion. Run focused checks when tools and authority allow; label unexecuted hypotheses.

Prioritize defects with an observable trigger and consequence. Exclude stylistic preferences and unrelated cleanup. If evidence is insufficient, state the missing observation rather than asserting a cause. A second model's agreement is a claim to verify.

## Report actionable results

For each finding give priority, file and line, triggering input or state, incorrect consequence, and supporting evidence. Separate executed checks from proposed checks. If no actionable findings remain, say so and identify the actual verification limit.

If the repository requires a durable report, write under its indexed docs layout. `allstack remember --kind review --source <report> --summary <finding-or-result>` retains source-linked context without inventing a delivery stage. A review result cannot authorize publication or declare an unexecuted check successful.

## Host capability

Codex: invoke as $allstack-<name> or request its name naturally. Use native repository tools and its current sandbox. A nested host command requires the same applicable authority as a direct call.
