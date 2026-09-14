# Sea of Demand: AllSource

Status: research complete; no outreach performed
Completed and accessed: 9 September 2026
Mode: focused, with expanded alternatives and channel coverage
Recent-evidence window: 9 March–9 September 2026; older evidence retained only for durable language, direct tool evaluation, or rare buying behaviour

## Research frame

| Field | Working definition |
| --- | --- |
| Product | Hosted AllSource: durable ordered agent events with recall, replay, point-in-time inspection, and source-event provenance after restart |
| Primary ICP | Technical founder or small engineering team operating one live or production-bound stateful AI agent |
| Payer | Technical founder or engineering lead accountable for reliability and operating cost |
| Trigger | Agent loses useful state after crash, restart, deployment, retry, or session boundary; or team cannot explain which event produced recalled state |
| Exclusions | Regulated-enterprise procurement, bespoke connectors, private-cloud deployment, generic database replacement, demo-only workloads, and self-host installs as paid validation |
| Geography | Global, English-language product and research |
| Primary conversion | Qualifying non-founder customer completes a real-workload restart-and-provenance circuit, pays for hosted AllSource, then renews once without refund or chargeback |
| Current first offer | Indie: £18.99/month after 14-day trial; 500K events/month, 14-day retention, three streams, hosted MCP read access; live catalog remains price source of truth |

Coverage accounting: **79 tracked page bodies opened**; **74 distinct sources retained**—72 public pages and two internal operating documents—and five low-trust, duplicated, or marginal pages excluded. Retained pages produced 74 source-level observations; comments within one page did not increase recurrence. Sources span six direct-discussion families—GitHub, Reddit, Hacker News, LangChain Forum, Stack Overflow, and Indie Hackers—plus official product documentation, provider offers, provider-authored case studies, and internal operating evidence. **Five public channels were verified** and one remained a lead. Search snippets, private communities, login-only discussion, and generated market summaries were excluded.

Evidence grades use independently operated platform families. All subreddits form one Reddit family; all GitHub repositories, issues, and discussions form one GitHub family; all sections of LangChain Forum form one forum family. Company pages and official docs prove offer or mechanics, not audience demand. Provider case studies remain supplier evidence unless independently corroborated.

Important limit: this convenience sample proves recurring operational failures, active solution search, and reachable technical surfaces. It does **not** prove prevalence among founders, acceptance of AllSource's combined event-store and memory contract, willingness to pay £18.99, fit of 14-day retention, or renewal. Public identities rarely establish company size. Exact small-team fit therefore remains partly inferred.

## Executive verdict

**A sea exists around durable agent execution, recovery, and inspectability—not yet around a generic hosted memory or event-store purchase.** Problem demand is strong. Buyers repeatedly build around it. Hosted AllSource solution demand remains mixed because native checkpointers, Postgres, Redis, SQLite, Markdown, and custom event logs are credible substitutes, while independent payment and renewal evidence is absent.

| Dimension | Rating | Why |
| --- | --- | --- |
| Problem demand | **Strong** | Restart loss, incomplete checkpoint recovery, duplicate side effects, stale memory, and missing replay/provenance recur across GitHub, Reddit, Hacker News, LangChain Forum, Stack Overflow, and Indie Hackers. |
| Solution demand | **Mixed** | Operators invest days, production infrastructure, migrations, cleanup logic, and custom ledgers. Paid platforms exist and provider case studies show use, but no direct category payment was verified and no independent AllSource payment or renewal was found. |
| Channel access | **Strong for listening; mixed for promotion** | Five exact public channels are active and body-readable. Most prohibit solicitation, low-effort promotion, or generated copy. Human, reproducible technical contributions are required. |

Strongest segment: operator of a multi-step production agent with external side effects, human interrupts, or expensive model/tool calls, after a crash or resume failure makes committed state uncertain.

Strongest trigger: “Can this run resume from the last verified state without repeating work or hiding what already happened?”

Strongest objection: “Why add another store when LangGraph plus Postgres or Redis—and a few explicit files—already persists enough state?”

Next validation move: turn existing restart proof into a **durable execution receipt** test: crash a real workload, restore exact state, identify every committed side effect and recalled fact's source event, then ask five recently triggered operators to complete the circuit and choose paid Indie or decline. Keep current kill rules: five qualified completions with zero payments, or three bespoke onboarding requirements, force reshape.

## 1. ICP and problem-space assumptions

### What evidence supports

- Stateful-agent operators lose or corrupt useful state after machine kills, reloads, resume paths, and failed tasks. Current LangGraph issues include unflushed checkpoints, erased messages, missing runtime inputs, and incomplete Redis/Postgres state ([#8298](https://github.com/langchain-ai/langgraph/issues/8298), [#8653](https://github.com/langchain-ai/langgraph/issues/8653), [#8582](https://github.com/langchain-ai/langgraph/issues/8582), [#8083](https://github.com/langchain-ai/langgraph/issues/8083)).
- Recovery is more than chat history. Operators need to know what ran, which side effects committed, what must not repeat, and where replay diverged. Reddit and forum threads describe duplicate customer messages, custom receipt ledgers, and authoritative transaction-log needs ([double-fire discussion](https://www.reddit.com/r/LangChain/comments/1u4zyd2/has_your_langchain_agent_ever_doublefired_a_side/), [transaction-log discussion](https://forum.langchain.com/t/what-is-the-equivalent-of-a-transaction-log-for-agent-systems/3986)).
- Memory quality creates a second problem: stale, contradictory, low-signal, or untraceable memories can be worse than forgetting. Builders ask for provenance, contradiction handling, temporal validity, pruning, and explanation of why context was selected ([persistent-memory discussion](https://www.reddit.com/r/LocalLLaMA/comments/1rsm45d/how_are_people_handling_persistent_memory_for_ai/), [HN workflow discussion](https://news.ycombinator.com/item?id=48413629), [Mem0 lifecycle discussion](https://github.com/mem0ai/mem0/discussions/5393)).
- Buyers already absorb costly work. Observed behaviours include three-node GKE plus Cloud SQL load testing, EKS plus Postgres plus a custom queue/event log, custom memory cleanup, graph-ingestion throttling, and dev-to-production migration work ([LangGraph #7259](https://github.com/langchain-ai/langgraph/issues/7259), [Graphiti #1262](https://github.com/getzep/graphiti/issues/1262), [Letta #3237](https://github.com/letta-ai/letta/issues/3237)).
- Hosted alternatives publish prices from about $19/month to hundreds per month. These establish market offers, not buyer willingness to pay ([Mem0](https://mem0.ai/pricing), [Letta](https://docs.letta.com/pricing), [Zep](https://www.getzep.com/pricing/), [LangSmith](https://www.langchain.com/pricing)).

### What remains assumption

- Anonymous production operators are technical founders or small teams rather than larger engineering organisations.
- Restart survival and source-event provenance belong in one paid buying decision. Both recur; fewer pages combine both in one request.
- Buyers prefer a separate hosted event substrate over native framework persistence or their existing Postgres/Redis estate.
- Indie limits support real long-term memory. Fourteen-day retention and read-only hosted MCP can look inconsistent with durable-memory expectations.
- A runnable proof creates enough value for payment, rather than free self-host adoption or copying the architecture.
- Reference throughput and recall latency affect this purchase. Public demand concentrates on correctness, recovery, provenance, lifecycle, and operating burden—not microsecond retrieval.

### Trigger-coded exact-core count

Coding unit: one body-readable direct-audience page. Official docs, company pages, provider case studies, and internal documents are excluded. Counts overlap when one page contains multiple triggers.

| Trigger | Distinct direct pages | Retained IDs / direct families | What it proves |
| --- | ---: | --- | --- |
| State missing, erased, or inconsistent after restart, crash, reload, resume, or failed task | **14** | D01–D04, D06–D09, D11, D14, D24–D26, A25; GitHub, Reddit, HN, Stack Overflow | Core recovery problem repeats |
| Replay, transaction boundary, source attribution, or inspectability explicitly requested | **9** | D05, D09–D10, D12–D13, D16, D20, D27–D28; GitHub, Reddit, HN, LangChain Forum, Indie Hackers | Provenance/replay job exists |
| Restart/recovery and authoritative history or side-effect evidence combined in one page | **5** | D05, D07, D09, D20, D27; GitHub, Reddit, LangChain Forum | Compound execution-receipt wedge exists, but is narrower |
| Production infrastructure, custom code, migration, or repeated debugging already invested | **14** | D01, D03–D04, D06–D10, D17–D19, A20–A22; GitHub, Reddit, LangChain Forum, Indie Hackers | Costly behaviour is common enough to test buy-versus-build |
| Founder or small-team payer status directly observable | **4** | D16–D18, D28; HN, Indie Hackers | Current ICP size filter remains weakly evidenced |
| Explicit cash paid for a persistent-agent setup | **0** | None | No direct payment verified |
| Explicit willingness to pay AllSource £18.99/month | **0** | None | Price acceptance unproven |
| Qualifying non-founder AllSource payment plus renewal | **0** | None | Promotion gate remains unmet |

## 2. Problem ecosystem and vocabulary

| Ecosystem role | Situation | Typical vocabulary | Intent level |
| --- | --- | --- | --- |
| Production agent operator | Crash, deployment, or retry leaves state uncertain | checkpoint, durable state, resume, last good state, recovery | Very high problem intent |
| Workflow operator with side effects | Agent may repeat an email, write, charge, or tool action | idempotency, committed effect, receipt, retry, replay boundary | Very high problem intent |
| Long-running/HITL builder | Hours-long run pauses or crosses sessions | interrupt, hydrate, reconnect, thread, runtime input | High problem intent |
| Memory-system builder | Stored context becomes stale, noisy, or contradictory | temporal validity, supersession, confidence, decay, pruning | High technical intent |
| Reliability/platform lead | Needs to explain historical behaviour | provenance, audit trail, source event, point in time, mismatch | High buyer influence |
| Framework user | Wants native persistence to work | checkpointer, PostgresSaver, RedisSaver, store, thread state | High solution intent; incumbent preference |
| Local-first coding-agent user | Uses files and explicit plans | `MEMORY.md`, work log, checkpoint file, project context | Medium problem; low hosted intent |
| Managed-memory evaluator | Compares provider with self-host stack | adds, retrievals, credits, active agents, retention, overage | High evaluation intent |
| Provider or tool builder | Publishes solution or benchmark | memory layer, graph memory, stateful agents, context engine | Inventory evidence; not buyer demand |

Recommended category language: **durable execution receipts for stateful agents**. Lead with failed recovery and proof. Describe memory as a derived, bounded view over durable source events. Avoid “perfect memory,” “memory database,” or generic “AI-native event store” as opening promise.

## 3. Ranked communities and channels

Rank reflects evidence fit and ethical participation, not permission to market.

| Rank | Platform / exact channel | Public URL | Audience fit | Visible scale / recent activity | Status | Value | Landmines |
| ---: | --- | --- | --- | --- | --- | --- | --- |
| 1 | GitHub / LangGraph issues | [Issue tracker](https://github.com/langchain-ai/langgraph/issues) | Engineers debugging production agent state | 41.3K stars, 7K forks, 524 issues when checked; relevant issues through 6 Sep 2026 | Verified | Highest-intent reproductions for resume, pruning, replay, and recovery | Contribution-only. No sales replies, unrelated advertising, or unreviewed generated contributions. |
| 2 | LangChain Forum | [Latest topics](https://forum.langchain.com/latest) | LangGraph adopters and maintainers | Daily Sep 2026 activity; retained threads show 45–146 views | Verified | Production migration, retention, recovery, and transaction-log language | Technical answers only. Promotion or solicitation can trigger immediate ban. |
| 3 | Reddit / r/LocalLLaMA | [Community](https://www.reddit.com/r/LocalLLaMA/) | Local and production-bound agent builders | Moderator reports more than 1M weekly visitors; current relevant threads have substantive replies | Verified | Broad exact-ICP discovery, DIY alternatives, security objections | Search first; disclose affiliation; roughly 1:10 promotion; generated or low-effort copy disallowed. |
| 4 | Hacker News / Show HN | [Hacker News](https://news.ycombinator.com/) | Technical founders, builders, buyers | Exact memory launches ranged from 1 point/2 comments to 202 points/225 comments | Verified | Best public falsification and launch-feedback surface | Founder must write personally. No AI-generated or AI-edited posts/comments, vote solicitation, landing-page-only launch, or repetitive promotion. |
| 5 | Stack Overflow / `langgraph` | [Tagged questions](https://stackoverflow.com/questions/tagged/langgraph) | Developers implementing framework persistence | 143 questions when checked; relevant pages show 53 to 2K views | Verified | Search vocabulary and concrete implementation failure discovery | Support-only. Recommendations off-topic; answers must stand alone; product affiliation must be disclosed. |
| 6 | Indie Hackers | [Community](https://www.indiehackers.com/) | Founder/payer audience | Active hourly/daily; exact memory posts reached 30–31 comments | Lead | Closest observable founder segment and useful build narrative surface | Current official promotion rules were not located. Reverify before any post; avoid repeated promotion and thread hijacking. |

Channel boundary for grades: GitHub is one family even across LangGraph, Mem0, Graphiti, Letta, and AutoGen; all Reddit communities are one family. Hacker News, LangChain Forum, Stack Overflow, and Indie Hackers each form separate families.

## 4. Ranked pains

Score = recurrence across distinct pages (1–4) + operational/emotional intensity (0–3) + proximity to qualifying paid restart-and-provenance conversion (0–3). Score describes this sample, not population prevalence.

| Rank | Pain | Recurrence | Intensity | Proximity | Total | Grade | Evidence |
| ---: | --- | ---: | ---: | ---: | ---: | --- | --- |
| 1 | Crash, restart, reload, or resume loses useful state or restores a different state | 4 | 3 | 3 | **10** | A | [LangGraph #8298](https://github.com/langchain-ai/langgraph/issues/8298); [Reddit restart](https://www.reddit.com/r/LangChain/comments/1rzf1ek/langgraph_memory_doesnt_survive_restarts_heres/); [Stack Overflow resume](https://stackoverflow.com/questions/79471648/when-using-interrupt-followed-by-new-command-resume-get-an-undefin/79898161) |
| 2 | Partial failure or retry repeats world-facing work because committed effects are unknown | 4 | 3 | 3 | **10** | B | [Reddit double-fire](https://www.reddit.com/r/LangChain/comments/1u4zyd2/has_your_langchain_agent_ever_doublefired_a_side/); [Reddit retries](https://www.reddit.com/r/LangChain/comments/1ucfwqz/frustrated_with_retries_in_a_multi_agent_system/); [forum transaction log](https://forum.langchain.com/t/what-is-the-equivalent-of-a-transaction-log-for-agent-systems/3986) |
| 3 | Configured checkpoint storage still erases, omits, or silently misreports state | 4 | 3 | 3 | **10** | B | [LangGraph #8653](https://github.com/langchain-ai/langgraph/issues/8653); [LangGraph #8083](https://github.com/langchain-ai/langgraph/issues/8083); [Stack Overflow external DB](https://stackoverflow.com/questions/79694863/how-to-store-and-load-the-state-from-an-external-database) |
| 4 | Recalled state is stale, contradictory, low-signal, or impossible to trace to source | 4 | 3 | 2 | **9** | A | [LocalLLaMA memory](https://www.reddit.com/r/LocalLLaMA/comments/1rsm45d/how_are_people_handling_persistent_memory_for_ai/); [HN memory quality](https://news.ycombinator.com/item?id=47328951); [Mem0 lifecycle](https://github.com/mem0ai/mem0/discussions/5393) |
| 5 | Operating persistence requires databases, migrations, retention policy, cleanup, monitoring, backup, and recovery semantics | 4 | 2 | 3 | **9** | B | [production PostgresSaver](https://forum.langchain.com/t/production-use-of-postgressaver-service-owned-migrations-and-checkpoint-retention-in-node-js/4481); [LangGraph load test](https://github.com/langchain-ai/langgraph/issues/7259); [LangSmith self-hosting](https://docs.langchain.com/langsmith/self-hosted) |
| 6 | Checkpoint and memory growth creates retention, pruning, storage, and latency risk without safe lifecycle controls | 3 | 2 | 3 | **8** | B | [forum cleanup](https://forum.langchain.com/t/checkpoint-cleanup/3037); [LangGraph #8531](https://github.com/langchain-ai/langgraph/issues/8531); [Mem0 lifecycle](https://github.com/mem0ai/mem0/discussions/5393) |
| 7 | Provider units, model calls, rate limits, and infrastructure make total cost hard to predict | 3 | 2 | 2 | **7** | B | [Graphiti cost evaluation](https://github.com/getzep/graphiti/issues/1193); [Graphiti bulk latency](https://github.com/getzep/graphiti/issues/1262); [LangSmith pricing](https://www.langchain.com/pricing) |
| 8 | Persistent state widens poisoning, secret, PII, isolation, and deletion risk | 2 | 3 | 2 | **7** | B | [Mem0 #6817](https://github.com/mem0ai/mem0/issues/6817); [Letta #3388](https://github.com/letta-ai/letta/issues/3388); [LocalLLaMA coding memory](https://www.reddit.com/r/LocalLLaMA/comments/1r5q7xd/how_are_you_handling_persistent_memory_for_ai/) |
| 9 | Import/export and environment promotion lose history or create switching lock-in | 2 | 2 | 2 | **6** | C | [Letta #3237](https://github.com/letta-ai/letta/issues/3237); [Mem0 platform versus OSS](https://docs.mem0.ai/platform/platform-vs-oss) |

## 5. Audience language

Short phrases preserve audience framing. They are not proposed performance claims.

| Phrase | Form | Meaning | Segment | Source |
| --- | --- | --- | --- | --- |
| Everything disappeared after machine kill | Faithful paraphrase | Durability failed at process boundary | Production operator | [LangGraph #8298](https://github.com/langchain-ai/langgraph/issues/8298) |
| Resume from last good checkpoint | Close paraphrase | Buyer wants bounded recovery, not generic memory | Multi-agent operator | [Reddit retries](https://www.reddit.com/r/LangChain/comments/1ucfwqz/frustrated_with_retries_in_a_multi_agent_system/) |
| Restart and repay earlier model/tool calls | Faithful paraphrase | Failure has direct variable cost | Long-running agent operator | [Reddit halfway failure](https://www.reddit.com/r/LangChain/comments/1wakk4b/how_do_you_all_handle_a_langgraph_agent_failing/) |
| “blank slate” | Exact | Cross-session continuity failed | Agent builder | [AutoGen #6466](https://github.com/microsoft/autogen/issues/6466) |
| Which effects already committed? | Faithful paraphrase | Recovery needs transaction evidence | Side-effecting workflow operator | [Reddit double-fire](https://www.reddit.com/r/LangChain/comments/1u4zyd2/has_your_langchain_agent_ever_doublefired_a_side/) |
| First replay mismatch | Close paraphrase | Determinism needs divergence evidence | Agent framework engineer | [AutoGen discussion](https://github.com/microsoft/autogen/discussions/7695) |
| Why was this memory selected? | Faithful paraphrase | Trust depends on inspectability | Technical operator | [HN workflow discussion](https://news.ycombinator.com/item?id=48413629) |
| Memory should not be audit trail | Close title phrase | Derived memory cannot replace immutable evidence | Founder/builder | [Indie Hackers](https://www.indiehackers.com/post/an-ai-agents-memory-should-never-be-the-audit-trail-62eff5375b) |
| Manual Markdown works better than many tools | Faithful paraphrase | Explicit files are credible substitute | Coding-agent user | [HN memory launch](https://news.ycombinator.com/item?id=46426624) |
| People cannot see what information AI used | Faithful paraphrase | Source visibility affects product trust | AI-product team | [11x case study](https://www.letta.com/case-studies/11x/) |
| We did not want to build management tooling | Faithful paraphrase | Managed value is avoided operational work | AI-product team | [Hunt Club case study](https://www.letta.com/case-studies/hunt-club/) |
| Production retention and migration ownership | Faithful paraphrase | Adoption stalls on lifecycle responsibility | Platform team | [LangChain Forum](https://forum.langchain.com/t/production-use-of-postgressaver-service-owned-migrations-and-checkpoint-retention-in-node-js/4481) |

### Copy implications

Use:

- “Resume from verified state after crash or deploy.”
- “See what already committed before retrying.”
- “Trace every recalled fact to its source event.”
- “Replay until first mismatch.”
- “Keep memory bounded; keep evidence durable.”
- “Run the restart proof on one real workload.”

Avoid:

- “Perfect memory” or “never forget.”
- “Replace every database.”
- Latency as primary benefit without workload method and limits.
- Blanket claims that vector stores or framework checkpointers lose data.
- “Audit-ready,” “compliant,” or “secure” without exact implemented control and scope.
- Treating self-host installs, GitHub interest, trial starts, or founder invoices as customer validation.

## 6. Hair-on-fire segments

### A. Side-effecting multi-step agent after failed recovery — strongest first segment

Trigger stack:

- agent sends messages, writes records, triggers payments, or invokes irreversible tools;
- run lasts long enough to cross a crash, deploy, retry, or human interrupt;
- operator cannot prove which effects committed;
- restart means duplicate action, manual trace surgery, or repeated model/tool spend;
- buyer already maintains Postgres/Redis/checkpointer code or a custom receipt ledger.

Best proof: kill process after one committed side effect, restore verified state, show immutable receipt and source event, then resume without duplicate work.

### B. Long-running HITL or research agent

Trigger stack: 10–60 minute or longer runs, external runtime resources, human interrupts, expensive prior calls, reconnectable streaming, and environment promotion. Strong need; side-effect risk varies.

### C. Multi-user persistent assistant with noisy history

Trigger stack: hundreds or thousands of interactions, stale facts, contradictory updates, retention/privacy obligations, and need to explain source selection. Larger eventual market; requires memory extraction, supersession, deletion, and governance beyond raw event durability.

### D. Local-first coding-agent user — useful counter-segment

Strong forgetting pain but often satisfied by `AGENTS.md`, plans, Markdown work logs, SQLite, or project-local MCP. Useful for architecture feedback and self-host adoption; weaker hosted conversion unless collaboration, uptime, or operational burden appears.

## 7. Ready-to-act signals, timelines, budgets, and workarounds

### Ready-to-act signals

| Signal | Strength | Interpretation |
| --- | --- | --- |
| Recent crash/redeploy lost a thread or restored inconsistent state | Very high | Trigger active; proof can use real failure |
| Duplicate email/write/tool call after retry | Very high | Monetary or trust loss creates urgency |
| Team built custom queue, event log, cleanup bridge, or recovery packet | Very high | Costly workaround and architectural fit |
| Production migration asks about retention, cleanup, sizing, and ownership | High | Buying/implementation window exists |
| Evaluating Graphiti/Mem0/Letta/LangSmith and measuring model or DB cost | High | Active solution search; contract comparison needed |
| Generic complaint that agent forgets context | Medium | Broad need; may accept files or longer context |
| Star, comment, self-host install, or demo tenant | Low | Interest only; no hosted conversion proof |

### Timelines

- Crash/retry urgency is immediate: same incident to days of recovery work.
- Migration and retention decisions appear before production launch or during first load test.
- Trial window is 14 days. One real-workload proof should finish within first session; otherwise value delivery is too slow.
- Renewal proof requires one complete paid billing cycle after first payment. Do not shorten this gate using engagement proxies.
- Fourteen-day data retention is shorter than many observed 30/60/90-day production questions. This is a contract-fit risk, not a conclusion that every buyer needs longer history.

### Pricing, preference, and willingness to pay

| Evidence class | Observed evidence | Interpretation |
| --- | --- | --- |
| Published pricing | AllSource £18.99/£78.99/£298.99; Mem0 $19/$249; Letta $20 base plus usage; LangSmith $39/seat plus usage; Zep $125/$375; Redis Pro from $200; database components from free or low monthly tiers | Offer anchors only; units and included work differ |
| Stated preference | Users prefer local control, explicit files, or existing Postgres/Redis; others ask for managed fleet and less tooling | Directional preference; no cash commitment |
| Costly non-cash signal | Production clusters, migrations, custom durable logs, cleanup logic, performance testing, and days/weeks of debugging | Strong buy-versus-build evidence |
| Direct cash signal | None verified; one low-engagement Reddit poster reported using a product priced at $19 after a weekend of setup | Product-price use is a concrete trade-off, not proof of payment, receipt, or renewal |
| AllSource cash/renewal | Internal billing mirror shows revenue and invoices, but inspected examples include founder-controlled identities; no qualifying non-founder circuit or renewal is evidenced | Does not satisfy `BET.md` gate |

Full customer burden matters more than sticker price:

- AllSource: subscription, integration, retention limits, event modelling, SDK/API write path, and possible tier upgrade for longer retention or read/write MCP.
- Framework-native stack: Postgres/Redis/SQLite, migrations, backups, retention/pruning, monitoring, encryption, replay semantics, and engineering ownership.
- Semantic-memory stack: vector or graph database, LLM and embeddings, extraction, consolidation, stale-memory policy, evaluation, rate-limit tuning, privacy, and operations.
- Local files: low cash cost and high explicit control, but weaker shared service, concurrent access, query semantics, and managed durability.

### Current workarounds

1. LangGraph checkpointer plus Postgres, Redis, MongoDB, or SQLite.
2. Custom Postgres tables, append-only event log, job queue, and idempotency keys.
3. `MEMORY.md`, plans, summaries, work logs, and version-controlled project files.
4. Mem0, Zep/Graphiti, Letta, Redis Agent Memory, or another managed/OSS memory layer.
5. Vector database plus hand-built extraction and retrieval.
6. Longer context windows and manual compaction.
7. Restart whole run and absorb repeated model/tool cost.
8. Keep agent stateless or restrict it to low-risk actions.

## 8. Alternatives and objections

Commercial offer, inventory/capacity, customer use, completed outcomes, provider status, and product status stay separate. A live pricing or signup page proves an offer, not purchase, provisioning, or capacity. Provider case studies prove supplier-reported use, not audited outcomes.

| Alternative | Offer observed | Inventory / capacity observed | Customer-use evidence | Completed-outcome evidence | Published price / total burden | Provider-level regulatory status | Product-level regulatory status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Versioned Markdown/files | Local explicit state and work logs | N/A; user-controlled files | Repeated direct HN, Reddit, and Indie Hackers use | Unverified user reports of adequate continuity; no controlled result retained | Usually no new subscription; manual discipline, stale-file and collaboration burden | N/A; no provider | N/A; implementation pattern, not regulated product |
| LangGraph + Postgres/Redis/SQLite | Checkpoints, thread state, store, replay/time travel | Self-host packages/docs observed; deployed capacity depends on buyer | Many direct issues, forum posts, Stack Overflow questions, and production tests | No independent completed recovery outcome retained | OSS software plus DB, compute, migrations, pruning, backup, monitoring, and support | No sector-authorisation check performed for component providers | No regulated-product authorisation evidenced |
| Redis Agent Memory | Working state, vector recall, summaries, and Streams event log | Offer/docs observed; no account provisioned or capacity tested | No retained direct page isolates use of packaged Agent Memory; broader Redis use excluded | None retained | Free/Essentials from $5; Pro minimum $200; plus models, task workers, APIs, and operations | Security/compliance claims not assessed as authorisation | No regulated-product authorisation evidenced |
| Mem0 | Hosted and OSS memory; hosted-only graph/temporal/lifecycle features | Offer/repository observed; no hosted account provisioned or capacity tested | Direct evaluators and self-host users discuss auth, storage, versions, latency, and cleanup | None independently retained | Free; Starter $19; Pro $249; plus adds/retrieval overage, models, vector DB and OSS operations | No provider-level authorisation evidenced in retained sources | No regulated-product authorisation evidenced |
| Zep / Graphiti | Hosted context graph plus OSS Graphiti | Offer/repository observed; no hosted account provisioned or capacity tested | Direct Graphiti users evaluate scale, ingestion cost, and production workloads | Provider reports scaled deployments; no independent completed result retained | Zep $125/$375; OSS needs graph DB, models, rate-limit tuning, tenant/security tooling | Provider security claims are not regulatory approval; authorisation unknown | No regulated-product authorisation evidenced |
| Letta | Hosted/stateful agent platform and self-host route | Offer/repository observed; no hosted account provisioned or capacity tested | Direct self-host issues plus provider case studies for Hunt Club, 11x, and Bilt | Provider reports 11x growth from 3 to 85 users; no independent completed result retained | $20 base with active-agent, tool-time, model, Postgres/pgvector, key, volume, and HTTPS burden | No provider-level authorisation evidenced in retained sources | No regulated-product authorisation evidenced |
| Pinecone/Qdrant/vector store | Managed vector retrieval substrate | Offers observed; no accounts provisioned or capacity tested | None tied to these providers in retained direct evidence | None retained | Free entry; Pinecone $20/$50 minimums; Qdrant usage pricing; plus extraction, lifecycle, event history, models | Provider status not assessed | No regulated agent-memory product authorisation evidenced |
| Neon/Supabase/Postgres | Managed relational persistence | Offers observed; no accounts provisioned or capacity tested | Direct users adopt Postgres checkpointers, but retained pages do not identify Neon or Supabase | None tied to these providers retained | Free entry; Neon typical Launch $15; Supabase Pro from $25; plus engineering/ops | Provider status not assessed | Database substrate; no regulated agent-memory product authorisation evidenced |
| Long context | More transcript retained in model input | Provider offers not inventoried in this run; capacity unknown | Direct HN/Reddit use and debate | Anecdotal relief from delayed compaction; no controlled outcome retained | Token spend, latency, compaction quality, provider limits | Provider status not assessed | Model capability, not regulated memory product |
| Custom event log/workflow system | Own append-only history, idempotency, queue, replay | N/A; built and operated by buyer | Direct HN, Reddit, GitHub, and forum examples | Unverified operator reports only | Engineering time, infra, migrations, operations, and incident ownership | N/A unless third-party components apply | Internal implementation, not regulated product |
| AllSource | Hosted ordered events, recall, replay, point-in-time inspection, provenance; OSS substrate | Site, repository, internal service, and trial route observed; external capacity not tested | Internal tenants and founder-controlled use exist; qualifying external use not evidenced | Internal local restart proof only; no qualifying non-founder paid renewal | Indie £18.99, Studio £78.99, Scale £298.99; integration and plan-limit burden | No provider-level regulatory authorisation claimed or evidenced | No regulated-product status claimed or evidenced; compliance guarantees excluded from first market |

### Main objections

1. **Existing stack is enough.** Framework checkpointer plus existing Postgres/Redis avoids another vendor and data path.
2. **Files are clearer.** Explicit Markdown and version control are inspectable, portable, cheap, and often adequate for coding agents.
3. **Generic event store is not memory.** Buyer still needs extraction, semantic retrieval, consolidation, supersession, deletion, and evaluation.
4. **Fourteen days is not durable enough.** Indie retention can expire before long-term memory value appears.
5. **Read-only MCP weakens first-value path.** Buyer may expect MCP to write memory; SDK/API route must be obvious or plan contract changed.
6. **Another store creates duplication.** Agent framework already keeps checkpoints while application keeps transactional data and traces.
7. **Replay cannot reproduce changing world.** External APIs, files, prompts, models, and MCP tools drift; claim must be “replay to first mismatch,” not guaranteed deterministic recreation.
8. **Persistent memory increases risk.** Malicious content, secrets, PII, stale beliefs, and cross-user leakage persist longer.
9. **Price comparison is opaque.** Events, memories, retrievals, bytes, credits, seats, tool seconds, and retention are not like-for-like units.

## 9. Counter-evidence and demand gaps

- Native persistence is standard inventory. LangGraph documents production checkpointers and durable stores; Redis combines working state, vector recall, and Streams. AllSource cannot win by claiming no alternative exists.
- Many users deliberately prefer explicit files, short sessions, and human-curated memory. One high-engagement HN thread included practitioners who had tested many tools and returned to Markdown.
- Exact-topic launches vary from 202 points and 225 comments to 1 point and 2 comments. Problem fit does not guarantee distribution.
- Current direct evidence supports agent operators; founder/small-team status is often unknown. Avoid presenting source volume as ICP prevalence.
- Full deterministic replay is not broadly validated. External state changes make divergence detection more credible than perfect reproduction.
- Provider case studies show real-looking deployments but remain supplier-authored. Contracts, invoices, renewal, uptime, and causal attribution are not public.
- Posted prices prove available offers. One weak direct page reports use of a product priced at $19, but no direct payment was verified and none establishes willingness to pay AllSource.
- Internal AllSource admin evidence shows 41 tenants, mirrored billing, and three invoices, but inspected examples include founder-controlled identities. Current [impact plan](IMPACT_PLAN.md) correctly records qualifying hosted customers as “not yet evidenced.”
- Indie plan's 14-day retention and read-only MCP create a contract mismatch with long-term memory language. Studio at £78.99 may be functional comparator; test rather than hide this distinction.
- AllSource's 469K events/sec and 11.9μs p99 reference figures are product evidence, not demand evidence. Buyers in retained pages prioritise recovery correctness, lifecycle, provenance, and total burden.
- No independent evidence shows three qualifying non-founder users complete ingest, recall, provenance/replay, restart verification, first payment, and renewal.

## 10. Engagement plan

No posting, messaging, joining, or outreach was performed.

### Listen

- Monitor LangGraph issues for `checkpoint`, `resume`, `replay`, `crash`, `retention`, `pruning`, and `PostgresSaver` patterns.
- Monitor LangChain Forum and Stack Overflow for production migration and failure language. Treat both as support surfaces, not lead lists.
- Read r/LocalLLaMA and Hacker News objections to local control, stale memory, poisoning, portability, and benchmark design.
- Keep Indie Hackers as founder-language lead until current official promotion rules are verified.

### Contribute

- Submit upstream reproductions, tests, adapters, or documentation only when they solve project issues independently of AllSource adoption.
- Answer forum or Stack Overflow questions completely before any relevant disclosed link.
- Participate in r/LocalLLaMA before sharing work; disclose affiliation and keep self-promotion below community threshold.
- If launching, use a runnable, human-authored Show HN with no required signup for core proof. Founder must write every HN word personally.

### Partner

- Test one framework integration with maintainers or experienced implementers after adapter and failure harness exist.
- Consider technical newsletters or creators only after they can independently run restart/provenance proof and paid retention exists.
- Do not create affiliate programme before retained revenue and contribution margin are known.

### Avoid

- Sales replies on GitHub, LangChain Forum, or Stack Overflow.
- AI-generated or AI-edited Hacker News/Reddit copy.
- Mass-posted launch text, vote solicitation, undisclosed affiliation, scraped personal details, or private-community entry.
- Competitor fear claims, unsourced benchmark tables, and outreach to people solely because they reported a production incident.

## 11. Bounded validation tests

### Test 1 — durable execution receipt

| Field | Definition |
| --- | --- |
| Audience | Five operators who experienced crash, resume, duplicate-effect, or checkpoint inconsistency within prior 60 days |
| Trigger | One real multi-step workload with at least one external side effect or expensive tool/model call |
| Message | “Crash it. Resume from verified state. See what committed and which source event produced recalled state.” |
| Channel | Existing qualified conversations first; then human-authored runnable Show HN or disclosed r/LocalLLaMA contribution after community-readiness rules are met |
| Asset | Open failure harness: write → commit effect → kill process → restore → inspect receipt/source → replay to first mismatch → resume without duplicate |
| CTA | Run on customer-controlled workload, then start hosted trial and choose paid plan or record exact rejection |
| Success | Five qualifying completions; at least two paid starts; no more than one requires bespoke engineering; first renewal remains final gate |
| Stop | Five qualified completions and zero payments, or three onboarding attempts require bespoke engineering |
| Guardrail | No payload, prompt, secret, email, or personal data in analytics; no deterministic-replay guarantee |

### Test 2 — contract and plan-fit falsification

| Field | Definition |
| --- | --- |
| Audience | Builders who complete Test 1 or are actively comparing managed persistence |
| Trigger | Buyer reaches retention, MCP-write, quota, or price question |
| Message | Show Indie and Studio contracts side by side, including retention and access differences |
| Channel | Owned proof route, pricing surface, and consented follow-up with proof participants; no community promotion required |
| Asset | Workload calculator using events/day, event size, required history window, streams, write path, model/embedder costs, support time, and current self-host burden |
| CTA | Choose Indie, Studio, self-host, or “none”; require reason and total-burden comparison |
| Success | At least three qualified buyers can select plan without founder reinterpretation; at least two accept a paid contract whose limits fit measured workload |
| Stop | Majority need more than 14 days but reject Studio price, or cannot understand write path without bespoke help |
| Guardrail | Never compare unmatched units or omit model, storage, retention, and operating costs |

### Test 3 — wedge versus category test

| Field | Definition |
| --- | --- |
| Audience | Ten triggered technical operators, split only after identical proof exposure |
| Trigger | Recent recovery or provenance failure |
| Message A | “Hosted agent memory that survives restart” |
| Message B | “Durable execution receipts: resume verified state and trace every committed effect” |
| Channel | Same owned proof route or consented evaluation session; randomise only after identity deduplication |
| Asset | Same runnable proof and same price; only framing changes |
| CTA | Start real-workload proof |
| Success | Execution-receipt framing produces at least three more qualified proof starts and no lower completion rate |
| Stop | Neither framing produces one qualified paid start after ten completed evaluations |
| Guardrail | Count customer identity once; do not optimise on clicks, stars, or anonymous traffic |

## Recommendation

**Proceed with bounded validation; do not promote or scale hosted-agent-memory offer yet.** Keep open-source substrate. Narrow first promise from broad event store or memory API to recovery-critical execution evidence:

> Resume a stateful agent from verified history. Know what already happened. Trace every recalled fact to its source event.

Product proof must beat real substitutes—Markdown, SQLite, PostgresSaver, Redis, and custom logs—on recovery confidence and operating burden. Before acquisition expansion, resolve or explicitly expose Indie retention and MCP-write fit, publish import/export path, show stale/superseded-state handling, and obtain first qualifying non-founder payment plus renewal.

## Source ledger

| ID | Source | Type | Published | Accessed | Geography | Audience | Visible engagement | Used for | Evidence limits |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| D01 | [LangGraph #8298](https://github.com/langchain-ai/langgraph/issues/8298) | Direct GitHub issue | 2026-07-08 | 2026-09-09 | unknown | Production-like operator | unknown | Crash data loss | Detailed reproduction; one implementation; no purchase signal |
| D02 | [LangGraph #8653](https://github.com/langchain-ai/langgraph/issues/8653) | Direct GitHub issue | 2026-08-19 | 2026-09-09 | unknown | Production operator | unknown | Silent state erasure | Detailed reproduction; one implementation |
| D03 | [LangGraph #8582](https://github.com/langchain-ai/langgraph/issues/8582) | Direct GitHub issue | 2026-08-10 | 2026-09-09 | unknown | Agent engineer | unknown | Resume resource loss | Reproduction supplied; no commercial signal |
| D04 | [LangGraph #8083](https://github.com/langchain-ai/langgraph/issues/8083) | Direct GitHub issue | 2026-06-15 | 2026-09-09 | unknown | Persistence user | unknown | Incomplete checkpoint state | Reproduction supplied; one reporter |
| D05 | [AutoGen deterministic replay](https://github.com/microsoft/autogen/discussions/7695) | GitHub discussion | 2026-05-14 | 2026-09-09 | unknown | Multi-agent builders | Eight comments | Replay vocabulary | Several solution builders; promotion risk |
| D06 | [Retries and recovery](https://www.reddit.com/r/LangChain/comments/1ucfwqz/frustrated_with_retries_in_a_multi_agent_system/) | Direct Reddit discussion | 2026-06-22 | 2026-09-09 | unknown | Production operator | unknown | Partial failure and recovery | Multiple detailed self-reports |
| D07 | [Double-fired side effect](https://www.reddit.com/r/LangChain/comments/1u4zyd2/has_your_langchain_agent_ever_doublefired_a_side/) | Direct Reddit discussion | 2026-06-13 | 2026-09-09 | unknown | Agent operators | unknown | Idempotency/receipts | Multiple convergent self-reports |
| D08 | [Memory fails across restart](https://www.reddit.com/r/LangChain/comments/1rzf1ek/langgraph_memory_doesnt_survive_restarts_heres/) | Direct Reddit discussion | 2026-03-21 | 2026-09-09 | unknown | LangGraph builder | unknown | Restart pain | Promotional/tutorial framing |
| D09 | [Agent fails halfway](https://www.reddit.com/r/LangChain/comments/1wakk4b/how_do_you_all_handle_a_langgraph_agent_failing/) | Direct Reddit discussion | 2026-09-08 | 2026-09-09 | unknown | Agent operators | unknown | Retry cost and receipt ledgers | Detailed replies; very recent; small sample |
| D10 | [Persistent memory approaches](https://www.reddit.com/r/LocalLLaMA/comments/1rsm45d/how_are_people_handling_persistent_memory_for_ai/) | Direct Reddit discussion | 2026-03-13 | 2026-09-09 | unknown | Local/agent builders | +9; multiple implementations | DIY use, provenance, contradictions | Small vote count; self-reports |
| D11 | [Long context after restart](https://www.reddit.com/r/LocalLLaMA/comments/1vt8v4c/how_do_you_deal_with_longcontext_sessions_after/) | Direct Reddit discussion | 2026-08-20 | 2026-09-09 | unknown | Local LLM operators | +9; detailed comments | Restart continuity | Small thread |
| D12 | [Ask HN: AI development workflow](https://news.ycombinator.com/item?id=48413629) | Direct HN discussion | 2026-06-05 | 2026-09-09 | Global | Technical operators | 171 points, 136 comments | Persistent context and inspectability | Broad workflow thread |
| D13 | [HN persistent-memory quality](https://news.ycombinator.com/item?id=47328951) | Direct HN discussion | 2026-03-10 | 2026-09-09 | Global | Agent builder | unknown | Contradiction/staleness | Isolated small discussion |
| D14 | [Stop Claude forgetting](https://news.ycombinator.com/item?id=46426624) | Direct HN discussion | 2025-12-29 | 2026-09-09 | Global | Coding-agent users | 202 points, 225 comments | Adoption and counterevidence | Older; producer-led launch |
| D15 | [Recall project memory](https://news.ycombinator.com/item?id=48622590) | Direct HN discussion | unknown | 2026-09-09 | Global | Technical users | 138 points, 85 comments | Memory demand and file substitute | Displayed 79 days old when accessed; producer-led launch |
| D16 | [Mem0 does not learn patterns](https://news.ycombinator.com/item?id=46891715) | Direct HN discussion | unknown | 2026-09-09 | Global | Technical founder/builder | 9 points, 7 comments | Structured-event workaround | Displayed about seven months old; builder promoting own approach |
| D17 | [IH session forgetting](https://www.indiehackers.com/post/how-do-you-handle-the-fact-that-your-ai-forgets-everything-between-sessions-03d215aa82) | Direct founder discussion | 2026-04-01 | 2026-09-09 | unknown | Founders | 7 likes, 30 comments | Founder problem/search | Seller presale is not buyer WTP |
| D18 | [IH agent crystallize](https://www.indiehackers.com/post/show-ih-i-was-my-ai-coding-agents-memory-so-i-automated-myself-out-of-that-job-31116cfa85) | Direct founder/build discussion | 2026-07-07 | 2026-09-09 | unknown | Agent builder | 5 likes, 31 comments | Markdown workaround | Producer/self-use evidence |
| D19 | [Production PostgresSaver](https://forum.langchain.com/t/production-use-of-postgressaver-service-owned-migrations-and-checkpoint-retention-in-node-js/4481) | Direct forum thread | 2026-09-02 | 2026-09-09 | unknown | Production Node team | 45 views, 2 replies | Migration/retention intent | Discussion active through 2026-09-04; no spend or completed adoption |
| D20 | [Agent transaction log](https://forum.langchain.com/t/what-is-the-equivalent-of-a-transaction-log-for-agent-systems/3986) | Direct forum thread | 2026-06-20 | 2026-09-09 | unknown | Agent engineer | unknown | Authoritative replay/audit | Small practitioner/maintainer exchange |
| D21 | [Checkpoint cleanup](https://forum.langchain.com/t/checkpoint-cleanup/3037) | Direct forum thread | 2026-02-26 | 2026-09-09 | unknown | LangGraph operator | Six posts | Retention/pruning | Active through 2026-03-03; partly outside recent window |
| D22 | [External state database](https://stackoverflow.com/questions/79694863/how-to-store-and-load-the-state-from-an-external-database) | Direct Q&A | 2025-07-08 | 2026-09-09 | unknown | Agent developer | 521 views, one answer | Solution search | Older; bundled alternative solves question |
| D23 | [Injected production checkpointer](https://stackoverflow.com/questions/79924060/testing-langgraph-functional-api-entrypoint-with-injected-checkpointer-raises-ru) | Direct Q&A | 2026-04-11 | 2026-09-09 | unknown | Production developer | 53 views | Implementation effort | No commercial intent |
| D24 | [Interrupt/resume failure](https://stackoverflow.com/questions/79471648/when-using-interrupt-followed-by-new-command-resume-get-an-undefin/79898161) | Direct Q&A | unknown | 2026-09-09 | unknown | LangGraph developer | 2K views, score 3 | Resume failure | Modified about May 2026; framework-specific |
| D25 | [AutoGen session history #6466](https://github.com/microsoft/autogen/issues/6466) | Direct GitHub issue | 2025-05-05 | 2026-09-09 | unknown | Agent developer | unknown | Cross-session blank state | Older; no commercial signal |
| D26 | [LangGraph crash recovery #8234](https://github.com/langchain-ai/langgraph/issues/8234) | Direct GitHub issue | 2026-06-30 | 2026-09-09 | unknown | Production operator | unknown | Inconsistent recovery | One implementation |
| D27 | [LangGraph replay boundary #8358](https://github.com/langchain-ai/langgraph/issues/8358) | Direct GitHub issue | 2026-07-17 | 2026-09-09 | unknown | Agent engineer | unknown | Live-versus-replayed event provenance | Feature request; no purchase signal |
| D28 | [IH memory versus audit trail](https://www.indiehackers.com/post/an-ai-agents-memory-should-never-be-the-audit-trail-62eff5375b) | Direct founder discussion | 2026-09-03 | 2026-09-09 | unknown | AI founders | 2 likes, 2 comments | Immutable-evidence thesis | Low engagement; producer framing |
| D29 | [Coding-agent memory approaches](https://www.reddit.com/r/LocalLLaMA/comments/1r5q7xd/how_are_you_handling_persistent_memory_for_ai/) | Direct Reddit discussion | 2026-02-15 | 2026-09-09 | unknown | Coding-agent users | +8; multiple replies | Tool search and poisoning objection | Older; self-reports |
| D30 | [Priced persistent-agent setup](https://www.reddit.com/r/LocalLLaMA/comments/1r7b20w/spent_a_weekend_configuring_ollama_for_a/) | Direct Reddit account | 2026-02-17 | 2026-09-09 | unknown | Local AI user | +1 | Product-price use claim | Product priced at $19; payment unconfirmed; possible promotion; no receipt or renewal |
| A01 | [AllSource](https://www.all-source.xyz/) | Company offer | unknown | 2026-09-09 | Global | Agent/data engineers | unknown | Offer, prices, product claims | Live when accessed; no independent demand/adoption proof |
| A02 | [AllSource repository](https://github.com/all-source-os/all-source) | Product repository | unknown | 2026-09-09 | Global | Developers | 1,094 commits; 8 issues; 7 PRs; 0 forks visible | OSS inventory and architecture | Active when accessed; activity is not paid validation |
| A03 | [Mem0 pricing](https://mem0.ai/pricing) | Company offer | unknown | 2026-09-09 | Global | Agent builders | unknown | Posted-price anchors | Live when accessed; no WTP/use proof |
| A04 | [Mem0 platform versus OSS](https://docs.mem0.ai/platform/platform-vs-oss) | Official docs | unknown | 2026-09-09 | Global | Evaluators | unknown | Hosted/self-host burden | Current when accessed; vendor-authored comparison |
| A05 | [Zep pricing](https://www.getzep.com/pricing/) | Company offer | unknown | 2026-09-09 | Global | Agent teams | unknown | Posted-price anchors | Live when accessed; credits not comparable to events; no WTP |
| A06 | [Graphiti repository](https://github.com/getzep/graphiti) | OSS inventory | unknown | 2026-09-09 | Global | Memory engineers | unknown | Self-host requirements | Active when accessed; vendor-authored README |
| A07 | [Letta pricing](https://docs.letta.com/pricing) | Company offer/docs | unknown | 2026-09-09 | Global | Agent builders | unknown | Posted price and metering | Current when accessed; no WTP proof |
| A08 | [Deploy Letta with Docker](https://docs.letta.com/v1-sdk/docker) | Official docs | unknown | 2026-09-09 | Global | Self-host operators | unknown | Operating burden | Current when accessed; vendor documentation |
| A09 | [11x with Letta](https://www.letta.com/case-studies/11x/) | Provider case study | unknown | 2026-09-09 | Global | AI product team | unknown | Customer use and source visibility | Page ©2026; supplier-authored; claims 3 to 85 users; no contract or audit |
| A10 | [Hunt Club with Letta](https://www.letta.com/case-studies/hunt-club/) | Provider case study | unknown | 2026-09-09 | United States/global | AI product team | unknown | Managed-tooling value | Page ©2026; supplier-authored; claims 10 initial users, planned 50 |
| A11 | [Redis pricing](https://redis.io/pricing/) | Company offer | unknown | 2026-09-09 | Global | Developers | unknown | DB price anchor | Live when accessed; not full memory-system cost |
| A12 | [Redis as agent memory](https://redis.io/docs/latest/develop/use-cases/agent-memory/) | Official docs | unknown | 2026-09-09 | Global | Agent engineers | unknown | Competitive mechanics | Current when accessed; vendor tutorial, not adoption |
| A13 | [LangGraph persistence](https://docs.langchain.com/oss/python/langgraph/persistence) | Official docs | unknown | 2026-09-09 | Global | Agent engineers | unknown | Native alternative | Current when accessed; vendor docs; no outcome proof |
| A14 | [LangSmith pricing](https://www.langchain.com/pricing) | Company offer | unknown | 2026-09-09 | Global | Agent teams | unknown | Posted-price and compute model | Live when accessed; broader platform; not like-for-like |
| A15 | [Self-hosted LangSmith](https://docs.langchain.com/langsmith/self-hosted) | Official docs | unknown | 2026-09-09 | Global | Enterprise/platform teams | unknown | Total operating burden | Current when accessed; enterprise route outside first ICP |
| A16 | [Qdrant pricing](https://qdrant.tech/pricing/) | Company offer | unknown | 2026-09-09 | Global | Vector-search teams | unknown | Component anchor | Live when accessed; not full agent-memory system |
| A17 | [Pinecone pricing](https://www.pinecone.io/pricing/) | Company offer | unknown | 2026-09-09 | Global | Vector-search teams | unknown | Component anchor | Live when accessed; not full agent-memory system |
| A18 | [Neon pricing](https://neon.com/pricing) | Company offer | unknown | 2026-09-09 | Global | Postgres users | unknown | Managed DB anchor | Live when accessed; app semantics excluded |
| A19 | [Supabase pricing](https://supabase.com/pricing) | Company offer | unknown | 2026-09-09 | Global | App developers | unknown | Managed DB anchor | Live when accessed; app semantics excluded |
| A20 | [Graphiti cost issue #1193](https://github.com/getzep/graphiti/issues/1193) | Direct GitHub issue | 2026-02-02 | 2026-09-09 | unknown | Large-workload evaluator | unknown | Cost and observability | Detailed questions; one evaluator; no purchase |
| A21 | [Graphiti bulk latency #1262](https://github.com/getzep/graphiti/issues/1262) | Direct GitHub issue | 2026-02-23 | 2026-09-09 | unknown | Direct user | unknown | Ingestion burden | Reports 100 records near one hour; self-report; older than window |
| A22 | [LangGraph load test #7259](https://github.com/langchain-ai/langgraph/issues/7259) | Direct GitHub issue | 2026-03-24 | 2026-09-09 | unknown | Production/load-test team | unknown | Costly infrastructure signal | 500-user test; self-reported benchmark |
| A23 | [Mem0 lifecycle #5393](https://github.com/mem0ai/mem0/discussions/5393) | Direct GitHub discussion | 2026-06-05 | 2026-09-09 | unknown | Self-host user | unknown | Cleanup/pruning burden | Detailed 933-memory example; unverified metrics |
| A24 | [Letta import #3237](https://github.com/letta-ai/letta/issues/3237) | Direct GitHub issue | 2026-03-20 | 2026-09-09 | unknown | Deployment user | unknown | Import/export lock-in | Detailed migration request; no purchase evidence |
| A25 | [Letta compaction #3270](https://github.com/letta-ai/letta/issues/3270) | Direct GitHub issue | 2026-04-01 | 2026-09-09 | unknown | ECS self-host operator | unknown | Durability risk | Detailed failure report; one deployment |
| A26 | [Mem0 secret recall #6817](https://github.com/mem0ai/mem0/issues/6817) | Direct GitHub issue | 2026-08-05 | 2026-09-09 | unknown | Memory user | unknown | Secret persistence risk | Reproduction supplied; product-specific |
| A27 | [Letta memory isolation #3388](https://github.com/letta-ai/letta/issues/3388) | Direct GitHub issue | 2026-06-19 | 2026-09-09 | unknown | Agent user | unknown | Cross-session poisoning/isolation | Issue body accessible; AI-assisted issue; confidence reduced |
| A28 | [LangGraph checkpoint pruning #8531](https://github.com/langchain-ai/langgraph/issues/8531) | Direct GitHub issue | 2026-08-05 | 2026-09-09 | unknown | Production operator | unknown | Retention and pruning proposal | Feature request; no outcome |
| C01 | [LangChain Forum latest](https://forum.langchain.com/latest) | Community index | unknown | 2026-09-09 | Global | LangChain engineers | unknown | Channel cadence | Sep 2026 daily activity observed; member total hidden |
| C02 | [LangChain Forum guidelines](https://forum.langchain.com/guidelines) | Official community rules | unknown | 2026-09-09 | Global | Forum participants | unknown | Participation limits | Current when accessed; rules can change |
| C03 | [LangGraph repository](https://github.com/langchain-ai/langgraph) | Community/repository index | unknown | 2026-09-09 | Global | Agent developers | 41.3K stars, 7K forks, 524 issues | Channel scale/activity | Active when accessed; stars are not customers |
| C04 | [r/LocalLLaMA rules](https://www.reddit.com/r/LocalLLaMA/about/rules.json) | Official community rules | unknown | 2026-09-09 | Global | Local-model practitioners | unknown | Promotion limits | Current when accessed; enforcement discretionary |
| C05 | [HN guidelines](https://news.ycombinator.com/newsguidelines.html) | Official community rules | unknown | 2026-09-09 | Global | Technical founders | unknown | Participation limits | Current when accessed; editorial interpretation |
| C06 | [Show HN guidelines](https://news.ycombinator.com/showhn.html) | Official launch rules | unknown | 2026-09-09 | Global | Builders/founders | unknown | Runnable launch requirements | Current when accessed; editorial discretion |
| C07 | [Stack Overflow `langgraph`](https://stackoverflow.com/questions/tagged/langgraph) | Q&A index | unknown | 2026-09-09 | Global | LangGraph developers | 143 questions | Channel size/activity | Current when accessed; tag count is not active-user count |
| C08 | [Stack Overflow promotion policy](https://stackoverflow.com/help/promotion) | Official community rules | unknown | 2026-09-09 | Global | Q&A contributors | unknown | Disclosure/promotion limits | Current when accessed; enforcement contextual |
| C09 | [Stack Overflow on-topic policy](https://stackoverflow.com/help/on-topic) | Official community rules | unknown | 2026-09-09 | Global | Developers | unknown | Support-only fit | Current when accessed; recommendations excluded |
| C10 | [Indie Hackers](https://www.indiehackers.com/) | Community index | unknown | 2026-09-09 | Global | Founders | unknown | Audience and cadence | Hourly/daily posts observed; official promo rules unresolved |
| C11 | [IH promotion norms](https://www.indiehackers.com/post/product-promotion-on-ih-67c946c95e) | Community discussion | 2022-11-12 | 2026-09-09 | Global | Founders | 2 likes, 3 comments | Promotion lead | Old and unofficial |
| C12 | [LangGraph issues](https://github.com/langchain-ai/langgraph/issues) | Community/issue index | unknown | 2026-09-09 | Global | Agent developers | 524 issues displayed | Channel relevance and cadence | Issue count is not buyer count |
| C13 | [r/LocalLLaMA](https://www.reddit.com/r/LocalLLaMA/) | Community index | unknown | 2026-09-09 | Global | Local-model practitioners | 878K members and 21K online displayed | Channel scale and activity | Platform counts fluctuate |
| C14 | [Hacker News](https://news.ycombinator.com/) | Community index | unknown | 2026-09-09 | Global | Technical founders/builders | unknown | Channel access and current cadence | Homepage activity is not topic demand |
| I01 | [AllSource impact plan](IMPACT_PLAN.md) | Internal operating evidence | 2026-09-08 | 2026-09-09 | Global | Product team | unknown | Gate status and current experiment | Baseline recorded; internal, not market evidence |
| I02 | [Admin health runbook](runbooks/ADMIN_HEALTH.md) | Internal operating evidence | 2026-06-26 snapshot | 2026-09-09 | Global | Product operations | unknown | Product/invoice context | 41 tenants and 3 invoices recorded; founder/test identities prevent qualification |
