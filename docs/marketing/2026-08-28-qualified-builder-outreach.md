# Qualified builder outreach — 2026-08-28

Campaign: `design_partners_2026`

Status: founder approved exact recipients and drafts. Eight LinkedIn messages
were sent and verified on 2026-08-28. Kiell is blocked because DEV is signed
out; Sajjad is blocked pending explicit approval to transmit sender contact
details through his form.

Rules:

- use only listed public professional channel;
- do not advertise inside GitHub issues;
- send once, then one follow-up after four business days;
- stop after follow-up unless recipient replies;
- keep technical claim limited to a design-partner test, not a promise that
  AllSource fixes framework-specific bugs.

Campaign URL:

`https://www.all-source.xyz/design-partners?utm_source=direct_outreach&utm_medium=founder&utm_campaign=design_partners_2026&utm_content=personal`

## 1. Sajjad Khan

- Channel: contact form at `https://sajjadanwar.io/contact`
- Public evidence: LangGraph issue
  `https://github.com/langchain-ai/langgraph/issues/8039`
- Fit: crash recovery can replay or re-execute the same node depending on
  checkpoint-write ordering, duplicating external side effects.

Draft:

> Hi Sajjad — I read your LangGraph #8039 repro. The same crash point producing
> replay on one host and re-execution on another is exactly the kind of durable
> history problem I want to test with AllSource. We record decisions and state
> changes as immutable events so a restarted agent can reconstruct what happened
> before the crash. I’m selecting five design partners for 60 hosted days plus a
> founder-led integration. No review or public post required. Would testing your
> crash/replay flow against an event-sourced side log be useful?
>
> Details: https://www.all-source.xyz/design-partners?utm_source=direct_outreach&utm_medium=founder&utm_campaign=design_partners_2026&utm_content=personal

## 2. Doniyor Aliyev

- Channel: LinkedIn `https://uz.linkedin.com/in/doniyor-aliyev`
- Alternate channel: X `https://x.com/doniyor2109`
- Public evidence: LangGraph issue
  `https://github.com/langchain-ai/langgraph/issues/8653`
- Fit: production chat thread loses its complete message history after an
  interrupt is dismissed through `update_state`.

Draft:

> Hi Doniyor — your LangGraph #8653 report stood out: a production thread keeps
> its messages in the parent checkpoint, then `update_state` commits an empty
> head and every later run continues from lost history. I’m testing AllSource
> with five agent teams around failures like this—durable event history,
> provenance, and reconstruction after a bad state transition. Partners get 60
> hosted days and founder-led integration; no review required. Would your
> interrupt-dismissal flow be a useful design-partner case?
>
> https://www.all-source.xyz/design-partners?utm_source=direct_outreach&utm_medium=founder&utm_campaign=design_partners_2026&utm_content=personal

## 3. Kiell Tampubolon

- Channel: DEV Connect `https://dev.to/kielltampubolon`
- Public evidence: LangGraph issue
  `https://github.com/langchain-ai/langgraph/issues/8464`
- Fit: retried tools duplicate side effects after worker restart or timeout;
  proposal adds durable request claims and cached results.

Draft:

> Hi Kiell — I saw your LangGraph proposal for durable tool idempotency. Worker
> restart → tool re-execution → duplicate email/payment/trade is close to the
> recovery boundary I’m testing with AllSource. Its event log can preserve the
> claim, decision, and resulting state transition so replay has inspectable
> history. I’m opening five design-partner spots: 60 hosted days plus direct
> integration help, with no review requirement. Interested in testing one retry
> flow beside your `DurableClaimStore` design?
>
> https://www.all-source.xyz/design-partners?utm_source=direct_outreach&utm_medium=founder&utm_campaign=design_partners_2026&utm_content=personal

## 4. Diogo Santos

- Channel: LinkedIn profile behind
  `https://www.linkedin.com/posts/diogo-santos-genious_github-dgeniocontextweaver-budget-aware-activity-7450090565066649600-fiuN`
- Public evidence: ContextWeaver and LlamaIndex issue
  `https://github.com/run-llama/llama_index/issues/22823`
- Fit: phase-specific agent memory must distinguish relevant current evidence
  from stale accumulated context.

Draft:

> Hi Diogo — ContextWeaver and your LlamaIndex #22823 proposal name the same
> production problem from two sides: investigation, execution, and verification
> should not receive identical accumulated memory, especially when stale
> evidence competes with current state. I’m testing AllSource as the durable
> history beneath that selection layer—source-linked events plus reconstruction
> of what the agent knew at a given phase. Five design partners get 60 hosted
> days and founder-led integration. Would a ContextWeaver + AllSource flow be
> worth a working session?
>
> https://www.all-source.xyz/design-partners?utm_source=direct_outreach&utm_medium=founder&utm_campaign=design_partners_2026&utm_content=personal

## 5. Sanjay Rohith

- Channel: LinkedIn `https://in.linkedin.com/in/sanjayrohith18`
- Public evidence: Dejavu and LlamaIndex issue
  `https://github.com/run-llama/llama_index/issues/22701`
- Fit: shared vector-memory block pins later sessions to the first session ID,
  retrieving another session’s messages; Dejavu already uses superseding,
  auditable notes.

Draft:

> Hi Sanjay — Dejavu’s immutable notes, superseding corrections, and cited
> recall are unusually close to how I think agent memory should behave. Your
> LlamaIndex #22701 repro also shows why: one mutated session filter can make
> session B retrieve session A’s memory. I’m building AllSource around durable,
> source-linked event history and opening five design-partner spots for real
> failure cases. 60 hosted days + founder-led integration, no review required.
> Interested in comparing one Dejavu recall flow with an event-sourced backend?
>
> https://www.all-source.xyz/design-partners?utm_source=direct_outreach&utm_medium=founder&utm_campaign=design_partners_2026&utm_content=personal

## 6. Dineshsuriya D

- Channel: LinkedIn `https://in.linkedin.com/in/dinesh106`
- Alternate channel: X `https://x.com/droideronline`
- Public evidence: AutoGen issue
  `https://github.com/microsoft/autogen/issues/7043`
- Fit: GraphFlow interruption between agent transitions leaves remaining work
  but no ready agent; resumed workflow falsely reports completion.

Draft:

> Hi Dineshsuriya — I read your AutoGen #7043 analysis. A GraphFlow save can
> contain remaining work but an empty ready queue after interruption, so resume
> reports completion even though the next agent never ran. That is a strong test
> for reconstructing coordination state from durable transitions rather than
> trusting one corrupted snapshot. I’m selecting five AllSource design partners
> for 60 hosted days plus founder-led integration. No testimonial required.
> Would replaying that interrupted transition be useful as one test flow?
>
> https://www.all-source.xyz/design-partners?utm_source=direct_outreach&utm_medium=founder&utm_campaign=design_partners_2026&utm_content=personal

## 7. Israel “Izzy” Ekpo

- Channel: LinkedIn `https://www.linkedin.com/in/israelekpo`
- Public evidence: PydanticAI issue
  `https://github.com/pydantic/pydantic-ai/issues/530`
- Fit: remote message persistence must survive process boundaries and resume
  later without relying on application memory.

Draft:

> Hi Izzy — your PydanticAI #530 question gets to the boundary I care about:
> message history must leave process memory, survive a restart, and resume later
> with full type fidelity. I’m testing AllSource as a durable, provenance-aware
> event layer for that flow, alongside framework-native session APIs rather than
> replacing them. Five design partners get 60 hosted days and direct founder
> integration; no review or public mention required. Would a PydanticAI remote
> persistence adapter be useful to explore together?
>
> https://www.all-source.xyz/design-partners?utm_source=direct_outreach&utm_medium=founder&utm_campaign=design_partners_2026&utm_content=personal

## 8. Tathastu Naranje

- Channel: LinkedIn `https://in.linkedin.com/in/tathastu-naranje`
- Public evidence: CrewAI issue
  `https://github.com/crewAIInc/crewAI/issues/6544`
- Fit: procurement negotiation rehearsal needs supplier persona state updated
  by market-analysis agents while coach observes decisions and feedback.

Draft:

> Hi Tathastu — your CrewAI negotiation-rehearsal design is a concrete memory
> challenge: supplier persona, live market analyst, and coach all need the same
> evolving negotiation state without losing which evidence changed leverage.
> I’m looking for five AllSource design partners to test durable, source-linked
> agent history in real workflows. Partners get 60 hosted days plus founder-led
> integration; no review required. Would one rehearsal—market update → persona
> decision → coach feedback—be useful as a working integration case?
>
> https://www.all-source.xyz/design-partners?utm_source=direct_outreach&utm_medium=founder&utm_campaign=design_partners_2026&utm_content=personal

## 9. Nithin R

- Channel: LinkedIn `https://in.linkedin.com/in/nithin-r-385943188`
- Public evidence: PydanticAI issue
  `https://github.com/pydantic/pydantic-ai/issues/4773`
- Fit: production PydanticAI runs start blank; developers manually persist and
  hydrate history, session IDs, and audit traces.

Draft:

> Hi Nithin — your PydanticAI #4773 RFC describes the exact production tax I’m
> trying to remove: every run starts blank, while teams rebuild serialization,
> session isolation, hydration, and audit history around it. Your work on agent
> reliability makes this more than a chat-history use case. I’m opening five
> AllSource design-partner spots to test durable, provenance-aware cross-run
> memory. 60 hosted days + direct integration help, no review required. Would
> one PydanticAI session flow be worth pairing on?
>
> https://www.all-source.xyz/design-partners?utm_source=direct_outreach&utm_medium=founder&utm_campaign=design_partners_2026&utm_content=personal

## 10. Matthias Howell

- Channel: LinkedIn `https://ca.linkedin.com/in/matthias-howell-7b115811`
- Public evidence: CrewAI issue
  `https://github.com/crewAIInc/crewAI/issues/5578`
- Fit: distributed CrewAI workers need shared memory across processes and
  containers instead of embedded/local storage.

Draft:

> Hi Matthias — your CrewAI Valkey proposal identifies a real deployment gap:
> distributed workers need shared memory across processes and containers, not
> an embedded store tied to one instance. I’m testing AllSource with five agent
> teams where durable history, source provenance, and restart reconstruction
> matter alongside framework storage. Partners receive 60 hosted days plus
> founder-led integration; no testimonial required. Would a CrewAI multi-worker
> memory flow be useful as a design-partner test?
>
> https://www.all-source.xyz/design-partners?utm_source=direct_outreach&utm_medium=founder&utm_campaign=design_partners_2026&utm_content=personal

## Send checklist

- [x] Founder confirms exact ten recipients and drafts.
- [x] Verify and send LinkedIn messages to Doniyor, Diogo, Sanjay,
  Dineshsuriya, Izzy, Tathastu, Nithin, and Matthias.
- [ ] Send Kiell through DEV Connect after DEV authentication.
- [ ] Send Sajjad through his contact form after sender-data approval.
- [x] Record send timestamps and canonical profile URLs.
- [ ] Schedule one follow-up for four business days later.
- [ ] Stop after follow-up unless recipient responds.

## Send log

Follow-up due for sent messages: 2026-09-03 (four business days).

| Sent (WEST) | Recipient | Channel | Canonical profile | Verification |
| --- | --- | --- | --- | --- |
| 2026-08-28 02:32 | Doniyor Aliyev | LinkedIn | `https://www.linkedin.com/in/doniyor-aliyev/` | Conversation list showed sent message. |
| 2026-08-28 02:36 | Diogo Santos | LinkedIn | `https://www.linkedin.com/in/diogo-santos-genious/` | Recipient URL and sent message verified. |
| 2026-08-28 02:39 | Sanjay Rohith L | LinkedIn | `https://www.linkedin.com/in/sanjayrohith18/` | Profile URL and sent message verified. |
| 2026-08-28 02:41 | Dineshsuriya D | LinkedIn | `https://www.linkedin.com/in/dinesh106/` | Profile URL and sent message verified. |
| 2026-08-28 02:42 | Israel “Izzy” Ekpo | LinkedIn | `https://www.linkedin.com/in/israelekpo/` | Profile URL and sent message verified. |
| 2026-08-28 02:43 | Tathastu Naranje | LinkedIn | `https://www.linkedin.com/in/tathastu-naranje/` | Profile URL and sent message verified. |
| 2026-08-28 02:44 | Nithin R | LinkedIn | `https://www.linkedin.com/in/nithin-r-385943188/` | Profile URL and sent message verified. |
| 2026-08-28 02:45 | Matthias Howell | LinkedIn | `https://www.linkedin.com/in/matthias-howell-7b115811/` | Profile URL and sent message verified. |

## Reply log

| Received (WEST) | Recipient | Reply | Response sent (WEST) | Outcome |
| --- | --- | --- | --- | --- |
| 2026-08-28 08:04 | Diogo Santos | Confirmed overlap; requested a concrete AllSource–ContextWeaver boundary example before deciding on a working session. | 2026-08-28 11:22 | Sent and verified: durable event timeline versus phase-aware selection, with restart-and-correction test flow. |
| 2026-08-28 04:42 | Sanjay Rohith L | Asked to learn more. | 2026-08-28 11:22 | Sent and verified: event-history model plus a Dejavu correction, isolation, and historical-replay test flow. |
| 2026-08-28 20:59 | Diogo Santos | Requested both compact sequence diagram and integration sketch before deciding on a working session. | 2026-08-28 23:15 | Sent and verified: eight-step correction/restart sequence, ownership boundary, integration contract, and smallest-test assertions. |
| 2026-08-28 15:00 | Tathastu Naranje | Asked how his work was found, what integration requires, and for program, privacy, and security documentation. | 2026-08-28 23:15 | Sent and verified: public CrewAI source, sanitized rehearsal flow, minimum access/setup boundary, and public program/privacy/security links. |
