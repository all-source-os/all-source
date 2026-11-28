# CTO landing page evidence

Status: verified 26 November 2026.

The signed-in Codex [writer](codex-seed.jsonl) and [fresh reader](codex-recall.jsonl) receipts contain the actual tool arguments and results for the two homepage prompts, run against released Prime 0.30.1. The reader recovered Payments, the queue decision and ADR-17.

The [browser walkthrough](../../reports/2026-11-26-cto-homepage.html) shows the locally built homepage at [1440 pixels](home-desktop.webp) and [390 pixels](home-mobile.webp). The interaction check also passed at 320 pixels. The [client verification](client-verification.md) identifies which graph operations each client completed.

The full web suite passed 278 tests across 55 files (Allstack receipt `f49d4f99-a6e4-4c4c-b7fc-18f45f269d6c`). The final Next.js webpack production build targeted the root route and completed type checking (receipt `f6d07cac-44bf-4b6f-a393-a1add29cd60f`). Chrome repeated the three viewport checks on that build after the setup instructions added a graph-write permission cue. The initial Turbopack run encountered a worker socket permission error.

[Engineering graph observations](engineering-graph.json) show fresh local Prime 0.30.1 processes discovering Checkout and reading its owner, decision and source. [Harness research receipts](harness-research.json) cover Stripe, OpenAI, Longhand, Treg and Treg pricing. Five successful reads cost $0.020 total.

[Restart observations](restart.json) contain the support decision write and its history lookup from a separate local Prime 0.30.1 process. [Treg receipts](treg-receipts.json) list the public pages fetched, response statuses, call references and charges. See the [research brief](../../plans/2026-11-26-cto-landing-brief.md) for interpretation.
