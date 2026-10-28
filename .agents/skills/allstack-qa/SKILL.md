---
name: allstack-qa
description: Exercise a real application flow through its UI, API, CLI, or job entry point and report reproducible observed failures and verification limits.
---

# Exercise the real job

Identify the target and the complete user job. Use an explicit supplied target directly; probe only missing browser or runtime capabilities. This skill runs independently and creates no delivery run or Chronis task. Read repository policy and preserve the user's existing authorization boundaries.

## Choose observable checks

Trace the success path from user input to persisted result or delivered output. Include representative empty input, invalid input, and a boundary that could produce a plausible but incorrect success. For a filter, compare matching, nonmatching, and unfiltered results and inspect the original data for silently skipped failures.

For serialization changes, freeze representative inputs and independent expected output before implementation. Exercise the changed positions, such as keys and values separately, and compare retained data plus the complete decoded result. Distinguish an encoder's required representation from forms its decoder merely accepts; preserve unrelated compatibility choices.

For a UI, use an available browser to perform the interaction and inspect the visible result plus relevant runtime failures. For a CLI, execute the built program with concrete input, capture exit status, stdout, and stderr. For an API or job, exercise its actual ingress and read back its result through the owning system. Do not substitute a source inspection or unit test for a claimed browser check.

## Execute safely and retain evidence

Use an isolated local target when mutations would otherwise affect production. Verify the server or executable belongs to this work, not another session. Respect explicit authorization for external writes. Give every subprocess a deadline no greater than 300 seconds, kill then reap on timeout, and run only one local build at once.

Establish that the application was rebuilt from the intended source before recording its executable digest. Warm build directories can reuse stale binaries after timestamp-preserving copies; invalidate the affected application artifact or otherwise prove the rebuild. Run the actual built program, including negative cases with explicit expected exits.

Qualify immutable copies of the final runtime artifacts when later builds or cleanup can replace their paths. Rehash after execution and bind observations to their actual build inputs. A copied Rust integration executable may still embed a `CARGO_BIN_EXE_*` path; invoke the retained application directly or preserve that dependency before claiming the copied harness remains runnable.

When installed, `allstack check --expect-exit 4 --seconds 120 -- /absolute/program arguments` retains a private receipt without creating a delivery run or task. Omit `--expect-exit` for zero; `--input prompt.txt` supplies bounded UTF-8 stdin. The deadline includes metadata. The repository must ignore `.allstack/`. A matching expected negative exit succeeds; timeout, signal, spawn failure, changed executable, and status mismatch fail and retain their receipt. Inspect `outcome`, separated raw stream paths and digests, and the target's result. Decoded text can be lossy; raw streams preserve bytes. Host tool summaries can combine channels and do not replace those files. An executable digest binds observed bytes, not the source that produced them.

Record the target version, command or interaction, input, observed result, expected result, and executed test count where available. A zero-case run, timeout, skipped browser, or unavailable credential remains unverified. Inspect the underlying result rather than trusting a model summary or green wrapper.

For HTTP failures, retain the status, a safely redacted response body, timestamp, and request identifier when supplied. Preserve the failed observation alongside later successes. Read the relevant runtime conditions before assigning a cause; missing response evidence leaves the cause unresolved. Keep credentials and identity payloads out of shared reports.

For independent host proof, inspect actual tool calls and retained outputs: which skill file was read, which program ran, and which assertions executed. Separate source review, proposed commands, denied calls, and completed runtime checks. Another host's summary cannot fill a missing execution.

When qualifying a deployment, bind observations to the running image and its source/configuration. Keep earlier deployment evidence separate. If the final publication adds only tests or docs, compare the actual runtime build inputs before claiming the deployed application still matches; final-commit CI and runtime proof establish different facts.

## Deliver the outcome

Report reproducible defects with steps and consequences, then map the completed user flow to actual evidence. Fix only within the user's requested scope; repeat affected checks after a repair. Save required reports under indexed repo docs, and use `allstack remember --kind qa --source <report> --summary <observed-result>` for durable context when available. Stop when the requested flow is proved or a concrete external prerequisite prevents completion.

## Host capability

Codex: invoke as $allstack-<name> or request its name naturally. Use native repository tools and its current sandbox. A nested host command requires the same applicable authority as a direct call.
