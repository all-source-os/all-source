# Design-partner page simplification verification

## Root cause

`Section` adds vertical spacing but no container or horizontal padding. Previous
page put a two-column grid directly inside it, so content expanded to viewport
edges. Left column also used `position: sticky` with `top-24`; site header is
sticky too, so scrolled content appeared clipped beneath header.

## Reduction

- Two-column layout to one centered 768px column.
- Six form controls to three: name, work email, and one project/memory answer.
- Two long answers to one concrete example.
- Project name and timing moved to follow-up call.
- Large fit, event-trace, and process sections removed.
- No page content uses sticky or fixed positioning.

## Exact-window proof

At 1971x1043, matching reported browser content viewport:

- centered section: x602–1370, width 768px;
- form: x626–1346, width 720px;
- form: y495–1038, entirely inside 1043px viewport;
- horizontal overflow: false;
- sticky elements inside `main`: zero.

At 390x844 and 320x568:

- 16px form gutters;
- horizontal overflow: false;
- all primary controls remain at least 44px tall.

## Data compatibility

One combined answer is forwarded as both legacy `agent_use_case` and
`memory_problem`. Legacy-required project and timeline receive explicit
`Not provided` and `exploring` values. Browser interception confirmed exact
payload and successful state. Admin review collapses matching legacy fields into
one “Project and memory problem” answer.

## Quality gates

- Lighthouse accessibility: 100/100, zero binary failures.
- Keyboard order: name, email, combined answer, consent.
- Web and admin TypeScript checks pass.
- Relevant web route tests: 3/3 pass.
- Web and admin production builds pass.
- ProofShot: zero console errors, zero server errors.
