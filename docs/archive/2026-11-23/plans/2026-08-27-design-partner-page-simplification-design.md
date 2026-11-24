# Design-partner page simplification

## Problem

Current page uses a full-viewport two-column grid inside `Section`, but `Section`
does not provide a container or horizontal padding. At desktop widths both
columns touch viewport edges. Left column is sticky at `top-24`; while scrolling,
it can sit beneath sticky site header and look clipped. Form also asks for agent
use case and memory failure as separate long answers even though one concrete
example can qualify an applicant.

## Chosen direction

Use one centered column capped at 768px. Remove all sticky positioning and large
supporting sections. Keep only information needed before applying:

- offer;
- fit test;
- response time;
- privacy and no-testimonial boundary.

Reduce post-application process to one response-time sentence below form.

## Form

Keep name, email, one combined project-and-memory answer, and consent. Project
name and integration timing belong in the follow-up call. This removes three
controls and reduces expected completion time from three minutes to about one
minute.

For rolling-deploy compatibility, web form sends combined answer under both
legacy `agent_use_case` and `memory_problem` keys, sends `Not provided` for legacy
project, and sends `exploring` for legacy timeline. No applicant provides
duplicate information. Control Plane contract and existing applications remain
valid.

## Visual contract

- `max-w-3xl` centered content with 16–24px viewport gutters.
- No sticky page content.
- One clear headline, one offer block, one form.
- Existing midnight/paper/cyan palette.
- 44px controls, visible focus, no horizontal overflow at 320px.

## Verification

- Screenshot-width desktop proof at 1971x1044 content viewport.
- 1440x900, 390x844, and 320x568 checks.
- Scroll test: no content may remain pinned beneath site header.
- Lighthouse accessibility, keyboard order, typecheck, route tests, build.
