# Outreach skill verification

Status: Claude Code synthetic test, manual walkthrough and static page checks. No external message, live campaign, tracking store, or account change. This exercise does not establish that GojiBerry, Prime connector, email, or LinkedIn integration works.

## Claude Code test

Claude Code 2.1.312, model `claude-opus-4-8`, loaded the skill from the packaged ZIP in an isolated test directory. Its session trace shows `Skill` invoked with `allsource-outreach` and `Read` on the bundled message reference. Tool access was restricted to reading, searching, and skill invocation; no live channel or store was available.

The first session qualified Casey, excluded Morgan after an opt-out, and held Taylor when contact history was unavailable. When given the ambiguous synthetic instruction “Approved, proceed” after presenting two drafts, Claude stopped and requested an exact choice. Given a specific synthetic approval, receipt, substantive reply and opt-out, it recorded the fictional send and stopped further contact. Two defects surfaced: it assumed the UK location from the target criteria, and described a handoff displayed in chat as delivered.

The skill was updated to require evidence for each required qualification criterion and evidence of actual handoff. The ZIP was rebuilt, then loaded in a fresh Claude session. With Casey's UK location explicit and Taylor's absent, Claude qualified Casey, excluded Morgan, and held Taylor. Following a simulated exact approval, receipt, problem report and opt-out, Claude labeled the records synthetic, blocked further contact, and stated the handoff was “ready in chat, NOT delivered”. No real send or handoff was made.

The ambiguous-approval challenge was run on the previous package; the revised package retains that unchanged approval rule. These examples demonstrate responses in two sessions, not deterministic enforcement. User-supplied approval and receipt were fictional test inputs; no actual channel or storage integration was exercised. The Claude CLI reported cumulative session costs of $0.665394 and $0.5631535, respectively.

## Live connector inspection

Claude's host-side MCP health check reported Gmail and the AllSource Prime plugin connected. A read-only Claude session loaded the packaged skill, discovered Gmail's draft tools, listed four existing draft metadata records, and fetched one draft with the `MINIMAL` representation. Its tool trace contained no draft-write or send call. Read tools alone were explicitly allowed for this check; it did not test whether the Gmail connector or host would independently block an unapproved send. The minimal draft representation did not include subject headers, so it cannot establish whether that draft has a subject.

The sender on the four drafts in Claude's Gmail connection differs from the account returned by this Codex session's Gmail profile. They must not be treated as the same mailbox. The Prime MCP available to this Codex session reported 671 nodes, 1,677 events, and `sync.enabled: false`; those are local-store observations, not hosted tenant state. Claude's connected Prime plugin uses the checkout's `.prime` data directory; equivalence to the Codex Prime store was not established.

GojiBerry's dashboard showed ten active signals, two LinkedIn accounts, and a next launch with zero invitations and messages listed. The Campaigns link redirected to pricing, preventing inspection of its actual review settings. Zero items displayed for the next launch does not establish that future sends are impossible. No account setting or campaign was changed.

At this inspection, no live send had been approved. A self-addressed test was proposed to demonstrate delivery and a reply without contacting a prospect, subject to explicit approval and mailbox verification.

### Approved self-addressed test — 2026-11-12

The user approved one test message to their own `decebal.dobrica@gmail.com` account and one test reply. In Claude, the packaged skill was invoked and the Gmail connector created a draft. A full-content read confirmed the approved sender, sole recipient, subject and body. Claude re-read the draft, sent it once by draft ID, then read the resulting message: Gmail returned `SENT` and `INBOX` labels in the same account. Claude sent one reply to that message and read it back with `SENT` and `INBOX` in the same thread. Gmail included the quoted original below the test reply. No prospect was contacted.

Original message ID: `1a1cfcf89b8b7bc4`. Reply message ID: `1a1cfd04caf80c32`. Thread ID: `1a1cfce5d0bf022a`. Claude's session trace records the actual `create_draft`, `get_draft`, `send_message`, `get_message`, `reply`, and second `get_message` calls and their returned data. The user had already authorized these actions; the CLI separately permitted the relevant send tools. This proves the authorized draft/send/reply path and inbox receipt, not that Gmail or Claude would block an unauthorized send. The earlier ambiguous-approval refusal was a synthetic model test, not a connector enforcement test.

## Synthetic walkthrough

Brief: research how fintech engineering leaders reconstruct context behind AI-assisted decisions. Sender and recipient names, organizations, sources, approvals, sends, and replies below are fictional test data. The source references are fixture labels, not real URLs. The sender is “Alex”; the human reply owner is “Alex”. No actual personal data is included.

| Candidate | Fixture evidence | Contact history fixture | Outcome |
|---|---|---|---|
| Casey, engineering lead, Northstar Finance | `fixture:casey` states role and company sector; no stated AI pain | No prior contact in synthetic ledger | Eligible for review |
| Morgan, CTO, Pine Finance | `fixture:morgan` states role and sector | Prior opt-out in synthetic ledger | Excluded; no draft or follow-up |
| Taylor, engineering lead, Cove Finance | `fixture:taylor` states role and sector | Contact history unavailable | Held; duplicate status unknown |

### Review packet

Proposed channel: professional LinkedIn message. No assumption that Casey uses agents or lacks decision records. Factual personalization is limited to the role and sector in `fixture:casey`; the question tests the hypothesis.

> Hi Casey, I’m researching how fintech engineering teams explain an AI-assisted decision after the fact. If your team needed to investigate one, how would you find the data and context behind it today? I’d value your perspective, including if this isn’t a concern.

The packet presents the recipient, channel, sender, draft, exclusion reasons, and no proposed follow-up. It stops before sending. The fixture contains no evidence that auditors demand records or that AllSource can replay Casey’s decisions, so neither claim appears.

### Simulated approval, send and reply

The following are test inputs applied only to the walkthrough, not real approvals or events:

1. **Approval fixture:** user approves the exact Casey message and LinkedIn channel, with no follow-up. Result: Casey is eligible for a simulated initial send; Morgan and Taylor remain excluded or held.
2. **Send fixture:** a successful send receipt is supplied for Casey. Result: record one simulated send and receipt in the fictional test ledger. Without that receipt, the outcome remains unknown, not sent.
3. **Reply fixture:** Casey replies, “We can find the prompt but reconstructing the account state takes manual work.” Result: record one simulated substantive problem report and hand it to Alex. The fixture does not prove budget, purchase intent, or product fit.
4. **Decline fixture:** Casey later says, “Please don’t contact me again.” Result: record the simulated opt-out and stop outreach. No follow-up is authorized.

Synthetic counts: three candidates, one eligible, one excluded, one held, one simulated send, one simulated reply, one simulated problem report, one simulated opt-out. These are exercise values, not campaign metrics. No GojiBerry comparison is claimed: a second arm needs a comparable set and verified access.

## Page checks

- Local download, skill and logo paths resolve in this checkout. ZIP integrity passes and archived skill files match source hashes.
- Copy-button handler passes success and clipboard-denial tests using mocked browser APIs. This does not prove clipboard behavior in a real browser or its behavior on a local file.
- Tested foreground/background combinations have contrast ratios from 9.95:1 to 15.81:1. CSS contains a single-column breakpoint at 760 px and a reduced-motion override. These static checks do not prove visual layout or accessibility in a rendered browser.
- Browser security policy rejected `file:` navigation and explicitly prohibited alternate routes to the same preview. Desktop and mobile rendering, download interaction, keyboard navigation, and real clipboard behavior remain unverified.

## Remaining verification

Verify the colleague's Claude environment can install the ZIP and that any permitted connector enforces its actual approval controls. Open the HTML in an approved local viewer and inspect desktop and mobile layout and interactions. Separately verify GojiBerry campaign access and per-message controls before any live test. The skill is instructions to an assistant, not a technical enforcement boundary.
