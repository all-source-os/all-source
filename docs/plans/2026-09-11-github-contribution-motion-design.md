# GitHub contribution motion

Date: 2026-09-11
Status: active

## Decision

Stop unsolicited product comments in other projects' issue and pull-request
queues. Use GitHub to contribute runnable technical evidence before mentioning
AllSource.

## Operating rule

Every interaction must produce or offer one repository-native artifact:

- minimal reproducer;
- failing regression test;
- benchmark with method and limits;
- narrowly scoped patch;
- supported adapter or example; or
- documentation correction verified against current behavior.

No artifact, no comment. Closed issues receive no outreach. Read `CONTRIBUTING`,
issue templates, security policy, and maintainer instructions before work.
Never submit unsolicited AI-generated PR reviews.

## Workflow

1. Research 10 open problems from public project evidence.
2. Deduplicate against outreach ledger.
3. Rank contribution value, evidence quality, maintainer receptiveness, and
   effort.
4. Select two smallest useful artifacts.
5. Build and verify artifact in AllSource-owned source first.
6. Ask founder for approval immediately before public comment, issue, or PR.
7. Lead maintainer note with reproduction and result. Disclose AllSource only
   where relevant.
8. Record response, merge, rejection, support cost, and qualified product use.

## First artifact

`integrations/langgraph-checkpoint-allsource` implements LangGraph's current
checkpoint saver interface over immutable AllSource events. It turns recurring
checkpoint, restart, pending-write, and historical-state discussions into code
maintainers can run and inspect.

Initial acceptance:

- exact and latest checkpoint retrieval;
- ordered history with filters and pagination boundary;
- pending writes with LangGraph deduplication semantics;
- parent checkpoint links;
- logical thread deletion boundary;
- async API wrappers;
- explicit immutable-delete and cross-thread-scan limitations.

Current LangGraph conformance suite passes all 58 tests for five base
capabilities. External publication remains blocked until one live AllSource
restart test passes. Follow-up is tracked as Chronis task `t-e8cb11`.

## Success measure

Primary: verified external contribution accepted or maintainer asks to test
artifact against real workflow.

Diagnostic: reproducer runs, maintainer response, test adoption, integration
installs, restart-proof completions, and qualified hosted trial. Comment count,
reaction count, and link clicks alone do not count.
