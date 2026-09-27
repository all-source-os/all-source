# Commercial-route evidence rubric v1

Each dimension is 0–3. The first three sum to a priority out of 9; paid demand stays separate. This is not a conversion probability or a ranking by fund size.

| Dimension | 0 | 1 | 2 | 3 |
| --- | --- | --- | --- | --- |
| Product fit | No substantiated technical audience | Broad software/deep-tech audience | Named adjacent B2B software, AI workflow or infrastructure audience | Explicit sustained developer-infrastructure, open-source or foundational-software focus |
| Channel leverage | Investment activity only | General operator network or support | Structured technical community or enterprise advisory network | Explicit customer discovery, design partnerships, pilots or business-development matchmaking |
| Commercial access | No suitable verified route | Relevant team but route gated, investment-oriented or unclear | General business/other enquiry route allows a routing question | Explicit external commercial intake, matching eligibility substantiated |
| Paid demand | No buyer-specific requirement | Named buyer describes current relevant problem | Relevant current procurement/pilot request with owner; budget uncertain | Funded relevant requirement with owner and timing |

Score the dated evidence packet, not reputation. Unknowns do not count as positive evidence. Keep subsequent contact-route verification separate from the original model run rather than silently rewriting it. Model decimals may represent expected ordinal scores, not probabilities of commercial success. A historical zero means no paid-demand evidence in that packet, not proof that no demand exists today.

Store `model` (actual version or `human-rubric-v1`), `run_at`, `fit`, `leverage`, `access`, `paid_demand`, and `rationale`. Rationale identifies this rubric, evidence date, scoring method and known gaps. Leave `score: null` when unevaluated. For Jev, retain raw request/reply privately and label any operator correction separately.
