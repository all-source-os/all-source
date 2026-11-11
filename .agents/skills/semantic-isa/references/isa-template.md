# Semantic ISA spec — template

Copy into the target repo's `docs/` and fill every section. A section left blank is a decision not
made; write **Not applicable.** plus a reason instead.

## 1. Scope

- **Agent:** what it is for, one sentence.
- **Environment:** every system it can reach.
- **Harms:** the outcomes the kernel exists to prevent, ranked.

## 2. Side-effect inventory

Every capability the agent has today, before any ISA exists. One row each.

| Capability | System | Reversible? | Who is affected if it misfires |
|---|---|---|---|

## 3. Instruction set

| Instruction | Core | Governance property | Sink? | Replaces capability |
|---|---|---|---|---|
| `GENERATE` | Cognitive | Probabilistic output | no | — |
| `LOAD(customer_record)` | Memory | Deterministic I/O | no | crm.read |
| `TOOL_CALL(refund.issue)` | Execution | Deterministic action | **yes** | payments.refund |
| `VERIFY(refund_within_policy)` | Normative | Deterministic checkpoint | no | — |

Every row in §2 maps to exactly one row here. An unmapped capability is removed from the agent, not
left reachable around the kernel.

## 4. Bindings

One per instruction. Schemas are closed (`additionalProperties: false`); enums enumerate.

```json
{
  "instruction": "TOOL_CALL(refund.issue)",
  "input_schema": {
    "type": "object",
    "additionalProperties": false,
    "required": ["order_id", "amount_minor", "currency"],
    "properties": {
      "order_id": { "type": "string", "pattern": "^ord_[a-z0-9]{12}$" },
      "amount_minor": { "type": "integer", "minimum": 1 },
      "currency": { "enum": ["EUR", "GBP", "USD"] }
    }
  },
  "output_schema": { "...": "..." }
}
```

## 5. Taint and sinks

- **Taint sources:** list them (user input, fetched pages, file reads, every Cognitive output).
- **Propagation:** output tainted if any input tainted. Note every instruction that could *drop* a tag
  (STORE → LOAD round-trips, DELEGATE, RENDER that re-enters) and how the tag survives it.
- **Sinks:** list them.
- **Clearing:** for each sink, the VERIFY that clears its input, and the deterministic check it runs.

| Sink | Accepted input must be cleared by | Check |
|---|---|---|
| `TOOL_CALL(refund.issue)` | `VERIFY(refund_within_policy)` | amount ≤ order total, order ≤ 30 days old, no prior refund |

## 6. Violations

| Violation | Evidence recorded | Recovery |
|---|---|---|
| tainted input at sink | instruction, IDG path to the taint source, rule | FALLBACK to human review; feed refusal to model |
| schema failure | instruction, field, value | re-prompt with the schema error; INTERRUPT after N |
| instruction outside the set | raw proposal | refuse; log for ISA review |

## 7. Kernel sketch (Rust)

Domain types only — no I/O, so the admit decision is reproducible.

```rust
pub enum Core { Cognitive, Memory, Execution, Normative, MetaCognitive }

pub struct Value { pub data: serde_json::Value, pub tainted: bool }

pub enum Admit {
    Run,
    Refuse { rule: &'static str, evidence: Vec<NodeId> },
}

pub fn admit(ins: &Instruction, inputs: &[Value], registry: &Registry) -> Admit {
    if !registry.binding(ins).input_schema.accepts(inputs) {
        return Admit::Refuse { rule: "schema", evidence: vec![] };
    }
    if registry.is_sink(ins) && inputs.iter().any(|v| v.tainted) {
        return Admit::Refuse { rule: "tainted-input-at-sink", evidence: registry.taint_path(inputs) };
    }
    Admit::Run
}
```

## 8. Test matrix

| Case | Expectation |
|---|---|
| each sink, tainted input | refused, evidence names the taint source |
| each sink, input cleared by its VERIFY | admitted |
| VERIFY check fails | taint stays; sink still refuses |
| each tag-dropping path in §5 | tag survives |
| proposal outside the set | refused |

Drive with a scripted proposal stream; no live model in the suite.

## 9. Rollout

Shadow mode first: log admit decisions without enforcing, review the refusals, then bind.
