---
title: "Design Partner Acquisition Campaign"
status: APPROVED
last_updated: 2026-08-27
owner: "@ddonprogramming"
---

# Design Partner Acquisition Campaign

## Objective

Recruit five AI-agent builders who need durable cross-session memory. AllSource
will provide 60 days of hosted access, founder-led integration support, and two
feedback calls. Participation never requires a review, endorsement, or public
testimonial.

The 30-day funnel target is:

- 25 qualified applications
- 5 accepted design partners
- 3 live integrations
- 2 weekly-active partners

The campaign's primary call to action is:

`https://www.all-source.xyz/design-partners`

## Audience and message

Primary audience: developers and technical founders building production or
serious pre-production AI agents that need memory to survive restarts, preserve
provenance, or reconstruct prior state.

Canonical message:

> Your agent's memory should survive restarts, preserve provenance, and answer
> what it knew at any past moment. AllSource is recruiting five design partners
> building real agent systems.

Applicants must bring a real project, be available for an integration session,
and participate in two feedback calls. AllSource provides hosted Scale access,
direct engineering support, and help reaching a working integration.

## Public application page

Create `/design-partners` with:

- problem statement focused on durable agent memory
- five-slot program limit
- 60-day hosted offer
- eligibility and participation expectations
- technical proof: provenance, time travel, durability, and published benchmarks
- six-field application form
- privacy and retention notice
- FAQ and link to technical documentation

Application fields:

1. Name
2. Work email
3. Project or company
4. Agent use case
5. Current memory problem
6. Integration timeline

## Submission architecture

The browser performs basic required-field validation. Cloudflare Turnstile
limits automated spam. A Next.js route validates field lengths, email format,
consent, and integration timeline before forwarding the request to the control
plane.

The control plane applies IP rate limiting and appends a private
`design_partner.application_submitted` event to a dedicated system stream. The
event contains an application ID, contact fields, qualification answers,
campaign source, consent version, and timestamp. Submissions use an idempotency
key so retries cannot create duplicate applications.

Applicant data must never enter application logs, analytics properties, URLs,
GitHub issues, or public event streams. Rejected applications are deleted after
90 days. Accepted applications are retained through the program plus 90 days.
The privacy page must document purpose, retention, and contact route for removal.

Failure behavior:

- preserve form input after recoverable errors
- return actionable field validation
- return `429` for rate limiting
- return a generic `503` for backend failures
- never expose infrastructure or applicant data in error messages

## Admin inbox

Create `/design-partners` in the separate `admin.all-source.xyz` app behind its
existing admin authorization. This reuses the real admin BFF and avoids adding
operator-only PII to the customer dashboard.
The inbox shows applications grouped by status:

- new
- reviewing
- accepted
- waitlisted
- rejected

Each detail view shows the submitted answers, campaign source, timestamps, and
status history. Status changes append events rather than overwriting history.
The admin dashboard reports applications, acceptance, live integrations, and
weekly-active partners by acquisition source.

## Campaign sequence

Campaign launches only after the page and admin workflow pass production smoke
testing.

### Day 0

- deploy `/design-partners`
- verify application event and admin inbox
- verify privacy copy, status workflow, and UTM capture

### Day 1

- publish a GitHub Discussion
- publish an X thread and pinned post
- publish a LinkedIn founder post
- publish a Build in Public update

### Day 2

- publish a technical article explaining why agent memory needs durable event
  provenance
- submit the article URL to the daily.dev Rustverse Squad
- optionally publish a separately framed version in Dev World if community rules
  permit it

daily.dev content must lead with technical value, disclose the founder
relationship, and place the design-partner call to action near the end. It must
not be a raw recruitment advertisement.

### Days 3-7

- contact 10 qualified builders per day through public professional channels
- tailor every message to a specific public project or stated memory problem
- post distinct technical-community variants for Rust, MCP, agent-builder,
  LangChain, and LlamaIndex audiences
- never advertise inside unrelated GitHub issues

### Follow-up

- one personal follow-up after four business days
- one public reminder after seven days
- stop weak channels after 14 days and focus on sources producing qualified
  applications

## Campaign assets

- canonical design-partner offer
- GitHub Discussion post
- X launch thread and pinned post
- LinkedIn founder post
- Build in Public update
- three technical-community variants
- daily.dev technical article and Squad introduction
- two personalized outreach templates
- provenance screenshot or short demo clip
- UTM map and campaign dashboard

Cross-posted copy must be adapted to each community. No upvote requests, review
requests, automated mass messages, or undisclosed promotion.

## Verification gates

Before launch:

- unit-test validation, idempotency, status transitions, and retention behavior
- API-test Turnstile, rate limiting, malformed input, duplicate submission, and
  admin authorization
- run end-to-end flow: submit, open inbox, accept, and inspect status history
- audit PII boundaries across logs, analytics, URLs, and public streams
- verify mobile, keyboard, screen-reader, success, and error states
- run a production smoke test with a controlled application, then delete it
- verify every campaign URL and UTM source
- confirm one-command installation and linked technical proof remain accurate

Public posts and direct messages require a final review of exact drafts and
recipients immediately before sending.
