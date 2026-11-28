# CTO referral homepage implementation

Status: approved design in implementation. [Design](2026-11-26-cto-landing-design.md) · [Research](2026-11-26-cto-landing-brief.md)

## Visitor journey

A CTO arrives at the public root, sees a checkout ownership question answered from recorded company knowledge, and can immediately start a local harness. Keep the existing event-store metadata, discovery paths and technical resources. The first action leads to setup. Optional assistance follows the example and setup.

## Art direction

Use the existing dark navy field and original logo. Set a large left-aligned editorial headline beside a commissioned digital illustration of three machined modules with two signal-blue connections. An adjacent evidence strip names the real relationships represented, and the example expands to its decision source. On narrow screens use one reading column. Respect reduced motion, preserve text selection and keyboard access, and leave every answer available without animation.

## Verification matrix

| Surface | Exercise | Acceptance |
| --- | --- | --- |
| Public homepage | Load root on desktop and at 320 and 390 pixels. Navigate by keyboard. | Headline, graph evidence and setup link render. No horizontal page overflow or sign-up gate. |
| Graph example | Read owner, decision and source against captured local fixture. | Payments, queue decision and ADR-17 match the saved event. An unrelated source fails the fixture check. |
| Setup | Use an isolated profile and synthetic store. Discover Checkout after restart. | Registration and graph reads are independently checked. Distinguish client startup from model-mediated results. Publish instructions according to demonstrated scope. |
| Navigation | Follow setup and example anchors, inspect technical links and optional help. | Destinations resolve and help comes after the independent route. |
| Buyer test | Follow the approved neutral two-cohort procedure. | Independent activation requires retrieving a team's own data without staff assistance. Readers, interviews and observed conversions are recorded only when the pilot runs. |

Implementation stays on PR #443. Frontend tests, a browser walkthrough and an independent code review precede handoff. No publishing or prospect outreach is part of the PR.
