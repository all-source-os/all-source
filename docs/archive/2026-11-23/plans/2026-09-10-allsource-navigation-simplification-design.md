# AllSource navigation simplification

## Decision

Reduce public header from nine equal informational links to four choices:

1. Platform
2. Use cases
3. Docs
4. Pricing

Platform uses progressive disclosure. It groups product detail into **Core** and **Build with it**,
with concise descriptions on desktop and compact labels on mobile.

## Route map

- Core: Platform overview, Core event store, Query Service
- Build with it: Prime agent memory, production patterns, live demo
- Direct: Use cases, Docs, Pricing
- Utility: theme, sign in, Start 14-day trial

Design partners moves from global primary navigation to contextual placement on Use Cases and in
footer. Primary trial action remains sole dominant header action.

## Interaction and accessibility

- Native `details`/`summary` disclosure supports keyboard and touch without client JavaScript.
- Minimum 48px header and menu targets.
- Visible focus rings and semantic navigation labels.
- Mobile contains same routes and groups as desktop.
- AllSource logo asset, minimum size, clear space, and deep-blue brand tokens remain unchanged.

## Quality gates

- Contract test locks top-level count and destination grouping.
- Web type-check, tests, scoped Biome, production build.
- Production deployment followed by route and rendered-header checks.
