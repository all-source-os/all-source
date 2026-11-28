# Self-service AI harnesses for engineering teams

Status: research complete, 26 November 2026. [Homepage copy and buyer test](2026-11-26-cto-landing-design.md)

## Buyer and offer

Start with CTOs and heads of engineering at software companies whose engineers already use coding agents and repeatedly ask colleagues for company-specific context. The champion is the platform, developer-experience or staff engineer who can connect a client and own the first workflow. A CTO can recognise the problem when a new coding session proposes changing checkout and someone has to find the person who remembers why it was built that way.

Sell **self-service infrastructure for building an internal agent harness**. A harness here means the instructions, tools and company knowledge a team puts around its chosen agent. The team defines the job, chooses tools and permissions, and maintains its source material. AllSource Prime supplies persistent memory, a knowledge graph and tools for inspecting how a recorded fact entered the system. Engineering is the entry point. Other departments can build different workflows with the same pattern.

Offer implementation help below the working setup path. A team may ask AllSource to help build a harness for its particular tools or maintain a content-sync design. Qualification for that service is separate from self-service activation.

## What the research changes

| Source | Observation | Implication for the offer |
| --- | --- | --- |
| [AllSource homepage](https://www.all-source.xyz/) and [use cases](https://www.all-source.xyz/use-cases) | The category headline explains event sourcing. Four use cases follow. One warm referral could not identify a job quickly. | Lead with something an engineer can build and one question it can answer. Keep technical category pages accessible. A single referral is a clarity signal. |
| [Stripe engineering](https://stripe.dev/blog/minions-stripes-one-shot-end-to-end-coding-agents) | Stripe built internal coding agents and gives them curated tools and context from its own systems. | Company-specific engineering harnesses exist. Stripe demonstrates the pattern and has no known buying relationship with AllSource. |
| [OpenAI engineering](https://openai.com/index/harness-engineering/) | One team organised repository knowledge so Codex could navigate decisions and constraints through a short index. | A repository and good documentation are a credible starting point. Test whether durable relationships and source history add value when a question spans records or sessions. |
| [Daytona](https://www.daytona.io/) | A verb-led headline leads into a small runnable example. | Show the result of a completed workflow beside the setup action. |
| [Longhand](https://www.getlonghand.com/) | Longhand publicly demonstrates reusable workflows, shared skills, human review and publishing tested skill updates. The founder identifies executive and GTM harnesses, content management and synchronisation strategy as work supported by AllSource. Longhand's project architecture also documents local and hosted AllSource persistence. | Use as an example of a company building its own departmental experience. Attribute the roles precisely. Any public partner quote, logo or quantified outcome needs its own evidence and permission. |

**Evidence boundary.** Stripe and OpenAI provide first-party accounts of internal engineering work. They establish neither prevalence among smaller companies nor willingness to pay. Longhand illustrates a deployment relationship reported by the founder. Its public page describes Longhand's own offer. It does not establish that AllSource supplies every feature or publishes a customer endorsement.

## Treg as an integration business

Treg packages fragmented providers into a self-service product. Agents can discover tools, connect team accounts and use a single interface to reach several services. Its pricing describes metered provider calls, free use of a team's own registered tools and accounts, and workflow receipts. The buyer can already assemble a useful toolset and inspect what a run costs. [Treg](https://treg.to/) · [Pricing and receipts](https://treg.to/pricing)

| Buying decision | Where the two offers meet | AllSource position to test |
| --- | --- | --- |
| How does our coding agent reach the tools it needs? | Both can compete for the first integration an engineering team adopts. Treg already packages discovery and access to many providers. | Make connecting one memory tool and completing one engineering job just as easy. Show the input and answer together. |
| Where do our service owners, decisions and sources live between sessions? | A customer can assemble a similar workflow from several tools or build it internally. | Demonstrate persistent records and named relationships that the next session can inspect. Compete on time to a correct, source-backed answer. |
| Where do we get external data for an internal agent? | The same harness can use Treg for an external lookup and Prime for company knowledge. | Present this as a possible customer-built composition. No bundled Treg connector, provider-marketplace parity or special pricing arrangement is assumed. |

The original seven successful page reads cost $0.028. Five additional live Treg reads covering Stripe, OpenAI, Longhand, Treg and Treg pricing cost $0.020. Receipts and extracted local observations are indexed in the [evidence bundle](../evidence/2026-11-26-cto-landing/README.md).

## What works today

A local MCP client wrote three synthetic nodes into Prime 0.30.1 and linked Checkout to its owner and design decision. The writers exited cleanly. Fresh processes on the same directory discovered Checkout by listing services, followed both links and read the decision's creation history. It returned Payments, the decision to queue checkout requests during payment provider outages, and the supplied source reference ADR-17. The reader discovered the starting record without a saved ID. The source reference is data supplied by the writer. The test did not read or validate an external ADR. [Captured graph traversal](../evidence/2026-11-26-cto-landing/engineering-graph.json)

The repository documents project-scoped Claude Code setup. Installed Codex CLI accepts a local stdio MCP server command. This research tested the Prime protocol directly. Named-client setup and end-to-end interactions in each client remain launch checks. Team-wide access control, live source ingestion and multi-user sync require independent validation before appearing as self-service promises. The separate support fixture shows that a saved approval can survive a clean restart. [Prime setup](../../apps/prime-mcp/README.md) · [Support fixture](../evidence/2026-11-26-cto-landing/restart.json)

**Confidence.** The local graph answer is rock for this fixture. Buyer demand is sand until qualified CTOs name an owned workflow and an engineer can finish setup unaided. Savings remain unmeasured. Test the build-it-yourself offer with teams before promising an organisation-wide rollout.
