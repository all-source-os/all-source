# Design-partner page accessibility autoresearch

## Page job

Help developers building serious AI agents decide whether AllSource's five-seat
design-partner program fits their current memory failure, then complete an
application without guessing what happens next.

## Chosen direction

Use a conversion-first application brief. Keep AllSource's midnight product
identity, but remove generic landing-page decoration: blurred glows, ornamental
grid, excessive pills, repeated terminal labels, oversized headline, and heavy
card rounding. Retain one product-specific signature: a compact event trace that
connects a source event to durable recall and historical reconstruction.

Rejected alternatives:

- Proof-first technical essay: stronger education, slower application path.
- Multi-step wizard: smaller first step, more state, navigation, and abandonment
  risk for a six-field form.

## Visual system

- Night: `#0B1522`
- Deep panel: `#111E2E`
- Paper: `#F7F9FC`
- Ink: `#122033`
- Signal cyan: `#33C6D0`
- Action blue: `#0C69C7`
- Warm marker: `#C9861A`

Use existing system sans for reading and controls. Use monospace only for real
event names and reference IDs. Prefer square rules and 12–16px radii over large
pill and 24px card defaults.

Desktop layout:

```text
+---------------------------+----------------------------------+
| offer + fit                | application                      |
| concrete memory failures  | identity                         |
| source -> recall -> state  | project + two qualification asks|
| terms + next steps         | consent + apply                  |
+---------------------------+----------------------------------+
```

Mobile layout:

```text
offer -> application -> fit signals -> next steps -> technical/privacy links
```

Putting application before secondary fit evidence on small screens keeps offer
terms visible while moving first field 431px closer to initial viewport.

## Accessibility and usability contract

- WCAG 2.2 AA contrast for text, controls, focus indicators, and states.
- One `h1`, ordered headings, named form regions, associated labels, and
  described constraints.
- Visible 2px focus indicators with at least 3:1 contrast.
- Click targets at least 24x24px; primary controls at least 44px tall.
- No horizontal overflow at 320px; readable line lengths; no sticky overlap.
- Native validation remains available; submission errors use a focused alert.
- Reduced-motion users receive no non-essential animation.
- Offer terms, applicant expectations, privacy, and response time appear before
  submission.

## Fixed autoresearch rubric

| Signal | Weight |
| --- | ---: |
| WCAG 2.2 AA semantics, contrast, focus, and targets | 30 |
| Above-fold hierarchy and unobscured primary task | 15 |
| Form completion efficiency and error recovery | 20 |
| Product-specific copy with no generic AI filler | 15 |
| Responsive reading and interaction | 10 |
| Trust: terms, privacy, response time, and no-review boundary | 10 |

Stop after six scored iterations or three consecutive discarded experiments.
Keep correctness and accessibility wins even when visual score stays flat.

## Verification

- Type-check and relevant Vitest coverage.
- Production or dev browser proof at desktop and 320–390px mobile widths.
- Keyboard traversal through every form control.
- Automated accessibility scan when locally available, plus manual contrast,
  landmark, heading, target-size, and overflow checks.
- ProofShot session with screenshots, console errors, server errors, and video.
