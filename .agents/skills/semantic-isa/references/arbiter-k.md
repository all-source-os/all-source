# Arbiter-K — the source, exactly

*From Craft to Kernel: A Governance-First Execution Architecture and Semantic ISA for Agentic
Computers* — [arXiv:2604.18652](https://arxiv.org/abs/2604.18652). Predecessor: *From Craft to
Constitution* — [arXiv:2510.13857](https://arxiv.org/abs/2510.13857) (ArbiterOS, the Agent Constitution
Framework, a governance-only macro-architecture ISA).

Terms below are the paper's. Quote them as written; paraphrase only the explanations.

## Claim

Agentic AI is stuck in a "crisis of craft" because the system control loop is delegated to the LLM and
patched with heuristic guardrails. Arbiter-K recasts the model as a **Probabilistic Processing Unit
(PPU)**, a non-privileged proposal generator, encapsulated by a **deterministic, neuro-symbolic
kernel**. All environment-altering instructions must be validated by the symbolic kernel.

## The five cores

| Core | Paper's description | Instructions |
|---|---|---|
| Cognitive | probabilistic reasoning; "outputs are treated as untrusted proposals that must be subjected to kernel validation" | GENERATE, DECOMPOSE, REFLECT |
| Memory | "how information is loaded, stored, and compressed" | LOAD, STORE, COMPRESS, FILTER, STRUCTURE, RENDER |
| Execution | "connects the agent to the external environment"; "must be preceded by suitable verification" | TOOL_CALL, TOOL_BUILD, DELEGATE, RESPOND |
| Normative | "privileged safety and alignment operations including verification, constraints, and fallbacks" | VERIFY, CONSTRAIN, FALLBACK, INTERRUPT |
| Meta-cognitive | "probabilistic self-assessment to guide strategic routing decisions" | PREDICT_SUCCESS, EVALUATE_PROGRESS, MONITOR_RESOURCES |

## Governance properties

| Property | Examples |
|---|---|
| Probabilistic output — "fundamentally untrusted" | GENERATE, DECOMPOSE, REFLECT |
| Deterministic I/O | LOAD, STORE |
| High-Risk Probabilistic Operation — "can introduce hallucinations or omit critical data" | COMPRESS, FILTER |
| Deterministic Action / Checkpoint — sandboxable, or "high-confidence PASS/FAIL signals" | TOOL_CALL, VERIFY |
| Terminal Action — "must be verified for quality assurance" | RESPOND |
| Deterministic Control Flow — "predefined, trusted recovery paths" | FALLBACK |

## Kernel mechanisms

- **Instruction binding layer** — "the primary interface connecting the symbolic ISA to the concrete
  agent runtime". Each binding has a strictly typed `input_schema` and `output_schema`, "the primary
  mechanism for data validation at the kernel level".
- **Security Context Registry** — security metadata for all data and tools.
- **Instruction Dependency Graph (IDG)** — built at runtime; drives "active taint propagation" from
  each node's data-flow pedigree.
- **Taint** — data from external sources or the Cognitive core is tainted; outputs of an instruction
  that consumed tainted data are tainted.
- **Sinks** — high-stakes Execution instructions (e.g. `SQL_EXECUTE`). Hard rule: no tainted data may
  reach a sink. Only a successful **VERIFY** clears a taint tag.
- **Correction** — violations are "architectural exceptions with analyzable evidence", not session
  aborts; the policy engine can drive the workflow into a response state carrying the policy feedback.

## Evidence

- Prototype on the OpenClaw and NanoBot frameworks, with minimally invasive changes.
- **76% to 95% unsafe interception**, a **92.79% absolute gain** over native policies (authors'
  benchmarks, including AgentDojo and Agent-SafetyBench).
- Residual failures: "semantically weak operations including web_fetch and read_file".
- False positives: "cross-session delegation, calendar/UI side effects, and external communication
  actions" — boundary cases that inherently carry side effects.

## Critique

[Pith](https://pith.science/paper/2604.18652): mixed verdict — the mapping from probabilistic output to
trackable instructions is described at too high a level to verify the claimed security gains. This is
the load-bearing step of the whole design; a write-up should say so, and a DESIGN should specify it
concretely (the binding schemas) precisely because the paper does not.
