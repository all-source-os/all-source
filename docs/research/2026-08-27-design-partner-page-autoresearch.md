# Design-partner page autoresearch report

## Goal and fixed rubric

Primary outcome: help qualified AI-agent builders understand offer and complete
application. Accessibility target: WCAG 2.2 AA. Visual target: remove generic AI
landing-page patterns without erasing AllSource product identity.

| Signal | Weight |
| --- | ---: |
| WCAG 2.2 AA semantics, contrast, focus, and targets | 30 |
| Above-fold hierarchy and unobscured primary task | 15 |
| Form completion efficiency and error recovery | 20 |
| Product-specific copy with no generic AI filler | 15 |
| Responsive reading and interaction | 10 |
| Trust: terms, privacy, response time, and no-review boundary | 10 |

## Baseline

Score: **65/100**.

- Generic headline, ornamental glow/grid, pill badge, and repeated terminal-style
  labels made page look assembled from common AI landing-page patterns.
- Mobile application began at 1,191px and full page measured 4,876px tall at
  390x844.
- Desktop form began at 205px and measured 970px tall at 1280x720.
- Eight interactive elements measured below 24px in broad DOM scan.
- Textarea constraints and timeline help were not connected with
  `aria-describedby`.
- Offer details, fit criteria, and next steps repeated rather than forming one
  decision path.

## Iteration log

### 1. Replace decoration with evidence

Kept:

- Midnight AllSource shell.
- Cyan/blue product palette.
- One event trace showing record, recall, and state reconstruction.

Removed:

- Background grid and blurred glow.
- Founding-cohort and duration pills.
- Repeated pseudo-event labels.
- Vague “memory that can explain itself” headline.

Changed copy to start with existing user pain: “Fix the memory failures your
agent already has.” Converted benefits into exact offer terms and response time.

### 2. Put application on shortest mobile path

Desktop keeps evidence and application side by side. Mobile order is now offer,
application, supporting fit evidence, process. First form field moved from
1,191px to 812px without hiding qualification details.

Form changes:

- Question-led labels replace abstract nouns.
- Constraints are visible and programmatically associated.
- CTA names action: “Send design-partner application.”
- Submission error receives programmatic focus.
- Native required-field validation remains intact.

### 3. Tighten first-screen density and targets

Reduced mobile-only section spacing while preserving desktop rhythm. Application
now begins at 760px on 390x844. Consent checkbox is 24x24px with a named,
described target. Primary controls remain at least 44px tall. No horizontal
overflow appears at tested width.

## Final score

Score: **97/100**.

| Signal | Score |
| --- | ---: |
| WCAG 2.2 AA semantics, contrast, focus, and targets | 30/30 |
| Above-fold hierarchy and unobscured primary task | 14/15 |
| Form completion efficiency and error recovery | 18/20 |
| Product-specific copy with no generic AI filler | 15/15 |
| Responsive reading and interaction | 10/10 |
| Trust and expectation setting | 10/10 |

Remaining three points reflect inherent six-field qualification cost and form
height, not detected accessibility defects. Multi-step flow was rejected because
added state and hidden questions would increase abandonment risk for this short
application.

## Verification evidence

- Lighthouse accessibility: 100/100; zero binary accessibility failures.
- Final 1440x900 desktop: form starts at 173px; no horizontal overflow.
- Final 390x844 mobile: form starts at 760px, down from 1,191px; no horizontal
  overflow; page height falls from 4,876px to 4,716px despite added help text.
- Final 320x568 narrow-mobile check: zero horizontal overflow; inputs remain 44px
  tall.
- One `h1`; named form; ordered headings; valid label associations.
- Textareas and timeline have resolvable `aria-describedby` relationships.
- Keyboard order: name, email, project, use case, memory problem, timeline.
- Focus indicator: two-pixel blue ring plus offset on white paper.
- Empty submission focuses name with native “Please fill in this field.” message.
- Server submission failure focuses `role="alert"` and preserves entered values.
- Relevant route test: three assertions pass.
- TypeScript check and Biome check pass.
- Clean final ProofShot run covers desktop, mobile, and focus state with zero
  console errors and zero server errors.

Automated checks cannot prove complete WCAG conformance. Manual semantic,
keyboard, target-size, overflow, and visual review supplement Lighthouse.
