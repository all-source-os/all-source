---
design: docs/plans/2026-11-26-event-history-design.md
design-sha256: 77728334d5a15ba64ff47af3e4c4cec0401331ba138b8d90a4ed675a7678700d
verdict: clean
---

## Round 2

1. **Quota admission resolved.** Each host lookup and explicit browser evidence retrieval consumes one query unit. The contract requires atomic admission before reading, duplicate suppression, and tests for final-unit contention and source failure. [Accounting, line 26](2026-11-26-event-history-design.md#semantic-isa)
2. **First-question acceptance resolved.** Both native hosts must answer the exact bare question against fresh synthetic changes. The checks cover the interval, both event IDs and the opened citation. [Host proof, line 30](2026-11-26-event-history-design.md#semantic-isa)
3. **Tool descriptors resolved.** The deployed descriptor checks cover the title, access hints, OAuth scope and host-specific metadata. [Host proof, line 30](2026-11-26-event-history-design.md#semantic-isa)

## Round 1 (superseded)

Reviewed SHA-256 `2b4c9ae04fa6ebdeebeff48e05f30bbe3fa9a047627d82a0c6b45466736e2263`.

1. **Blocking. Query quota admission.** The proposed timeline call checks entitlement and reads Core. Existing customer-facing MCP queries also obtain an atomic quota receipt before reading source data. With one unit remaining, two concurrent calls can both pass an entitlement check. Define whether the history query consumes the existing quota, include the accounting write in the Semantic ISA, and test concurrent admission and backend failure. Specify the accounting policy for opening the browser citation as well. [Design, lines 14–26](2026-11-26-event-history-design.md#semantic-isa) · [Access verification](../../apps/query-service/lib/query_service_ex/application/services/customer_agent_access.ex#L18) · [Quota admission](../../apps/query-service/lib/query_service_ex/application/services/customer_query_admission.ex#L9)
2. **Should. First-question acceptance.** The design permits an explicit seeded interval in both host reviews. That can demonstrate historical lookup while the advertised bare question returns an empty result in the default window. In each native host, run the exact bare prompt with synthetic changes recorded within the preceding 24 hours. Compare both event IDs, the displayed interval and the opened citation with the source. Keep the dated prompt for repeatable directory reviews. [First workspace journey, lines 6–7](2026-11-26-event-history-design.md#first-workspace-journey)
3. **Should. Host tool descriptors.** The handshake check only asks for one read-only tool. Add assertions on the deployed descriptor for the title, read-only and destructive hints, and the ChatGPT tool-level OAuth scheme scoped to `allsource.history.read`. These fields affect listing eligibility and host authentication behavior. [MCP response acceptance, line 38](2026-11-26-event-history-design.md#side-effects-and-verification) · [Anthropic directory policy, section 5.E](https://support.claude.com/en/articles/13145358-anthropic-software-directory-policy) · [ChatGPT tool authentication](https://developers.openai.com/plugins/build/auth)
