---
name: semantic-isa
description: >-
  Design a Semantic Instruction Set Architecture (ISA) for an LLM agent — every environment-altering
  step becomes a discrete, typed, verifiable instruction that a deterministic kernel admits or refuses —
  or write an accurate explainer of the idea (Arbiter-K, "agentic computers"). Covers classifying
  operations into cores, governance properties, typed bindings, taint propagation to sinks,
  verify-clears-taint, and violations as recoverable exceptions. Triggers - "semantic ISA", "Arbiter-K",
  "agentic computer", "deterministic kernel for an agent", "stop the LLM owning the control loop",
  "taint tracking for tool calls", "guardrails keep failing", "write up the semantic ISA idea".
depends_on: [brainstorming]
enhances: [arch, security-review, rust-clean-architecture, integration-test]
---

# Semantic ISA for agents

**The thesis:** an LLM must not own the control loop. It becomes a non-privileged proposal generator;
a deterministic kernel decodes each proposal into an instruction from a closed set, checks it against
typed schemas and data provenance, and only then lets it touch the world. A heuristic guardrail filters
text after the fact; an ISA makes an unsafe action **unrepresentable or unadmittable** before it runs.

The reference design is Arbiter-K (arXiv:2604.18652). Its exact vocabulary, numbers and critique are in
[references/arbiter-k.md](references/arbiter-k.md) — read it before quoting anything from the paper.

Two modes. Pick one from the request; ask only if genuinely unclear.

| Mode | Output |
|---|---|
| **DESIGN** | A Semantic ISA spec for a named agent, filled from [references/isa-template.md](references/isa-template.md), plus kernel diagrams and a test matrix |
| **WRITE** | An explainer, article or talk section on the idea, accurate to the source |

## DESIGN

Run in order. Each step names the skill it borrows.

1. **Frame it — `brainstorming`.** Which agent, which environment, which harms matter. Get the
   *inventory of side effects* out of the user: every tool, API, file, network and message the agent can
   reach. An ISA designed without that list is a taxonomy, not a contract.
2. **Enumerate the instructions.** Turn every capability into one instruction with a verb name
   (`TOOL_CALL(db.update)` is too coarse once it is a sink — split it). Put each in a core:

   | Core | Holds | Trust |
   |---|---|---|
   | Cognitive | GENERATE, DECOMPOSE, REFLECT | output is an untrusted proposal |
   | Memory | LOAD, STORE, COMPRESS, FILTER, STRUCTURE, RENDER | I/O deterministic; COMPRESS/FILTER can drop or invent data |
   | Execution | TOOL_CALL, TOOL_BUILD, DELEGATE, RESPOND | touches the world; must be preceded by verification |
   | Normative | VERIFY, CONSTRAIN, FALLBACK, INTERRUPT | privileged; only the kernel issues these |
   | Meta-cognitive | PREDICT_SUCCESS, EVALUATE_PROGRESS, MONITOR_RESOURCES | advisory; routes, never acts |

   Adapt the names to the domain; keep the five-way split unless the user has a reason. **The
   Normative core is privileged**: the model may *request* a VERIFY, never *perform* one.
3. **Attach a governance property to every instruction** — probabilistic output, deterministic I/O,
   high-risk probabilistic, deterministic action/checkpoint, terminal action, deterministic control
   flow. An instruction with no property is undecided policy; the kernel cannot reason about it.
4. **Write the binding for every instruction** — a strictly typed `input_schema` and `output_schema`.
   The binding layer is where probabilistic text becomes structured data, so it is where validation
   lives. Reject on schema failure; never coerce a near-miss into shape — a coerced value is a guess the
   kernel then trusts.
5. **Declare taint and sinks.** Anything from an external source or the Cognitive core is tainted;
   anything computed from tainted input is tainted. Name the **sinks** — the Execution instructions where
   tainted data must never arrive (`SQL_EXECUTE`, payments, outbound messages, deletes, egress). The one
   way to clear taint is a successful VERIFY whose check is stated, deterministic and named in the spec.
   "The model judged it safe" is not a VERIFY.
6. **Decide what a violation does.** Not a session abort: an architectural exception carrying evidence
   (which instruction, which tainted input, which rule). Specify the recovery — FALLBACK path, rollback
   point, or a response state that feeds the refusal back to the model.
7. **Draw it — `arch`.** A C4 container view (model / binding layer / kernel / registry / environment)
   and one sequence diagram tracing a tainted value being refused at a sink, then cleared by VERIFY.
8. **Place it in layers — `rust-clean-architecture`** when implementing. The instruction set, the
   governance properties and the taint rules are **domain**: pure types, no I/O. The kernel's admit
   decision is application. Bindings to real tools are infrastructure. A kernel that imports an HTTP
   client has already lost its determinism argument.
9. **Attack it — `security-review`.** Walk every sink: can tainted data reach it by a path the
   dependency graph does not see (a STORE then LOAD that drops the tag, a DELEGATE to a sub-agent with
   its own memory, a RENDER that re-enters as input)? Each such path is a finding.
10. **Prove it — `integration-test`.** One test per sink asserting a tainted input is refused, one
    asserting VERIFY clears it, one per exception path. Drive the kernel with a scripted proposal stream,
    never a live model — a test that depends on what the model happens to say cannot fail reliably.
11. **Build it — `claude-prd` → `claude-beads`** when the user wants implementation planned.

Done when every capability in step 1 maps to exactly one instruction, every instruction has a core, a
property and a binding, every sink has a refusal test, and the sequence diagram matches the test.

## WRITE

Accuracy rules, each one a mistake the source invites:

- **One proposal, not a field.** Say "Arbiter-K, a 2026 research prototype, proposes…". Not "researchers
  use Arbiter-K architectures" and not "in advanced computer engineering".
- **The analogy is a contract, not hardware.** Like x86 or ARM, the ISA is the boundary software is
  allowed to cross. It does not make the model deterministic — it makes the *admission* of its proposals
  deterministic. Say that explicitly; it is the point most write-ups blur.
- **Name the parts the idea rests on:** PPU (the model as a Probabilistic Processing Unit), the five
  cores, typed bindings, Security Context Registry, Instruction Dependency Graph, taint to sinks,
  VERIFY clears taint, violations as recoverable exceptions.
- **Report the numbers with their frame and their critique.** 76–95% unsafe interception on the
  authors' benchmarks, residual failures in "semantically weak" reads (`web_fetch`, `read_file`), false
  positives on side-effecting boundary cases — and Pith's view that the probabilistic-to-instruction
  step is under-specified. A write-up without the critique is a press release.
- **Cite** arXiv:2604.18652 and its predecessor arXiv:2510.13857 (ArbiterOS). Never cite a figure you
  have not read in the paper.

Shape: problem (model owns the loop; guardrails patch after the fact) → idea (ISA as the contract) →
mechanism (cores, bindings, taint, sinks) → evidence → limits → what it means for the reader's system.
Deliver as Markdown under `docs/`; HTML only if it carries a diagram.

## Rules

- **The model proposes; the kernel disposes.** Any design step that lets model output decide its own
  admission is wrong, however it is phrased.
- **Closed set.** An operation outside the instruction set is refused, not best-effort mapped.
- **Determinism is a claim to test.** Every kernel decision must be reproducible from the instruction,
  its inputs and the registry — no model call, clock or network inside an admit decision.
- **Shadow first.** A new kernel runs advisory and logs what it would refuse before it binds; read the
  log before turning enforcement on.
