# Use-cases page redesign

## Goal

Help a technical buyer decide, from one page, whether AllSource fits their workload and which product layer to evaluate next.

## Chosen direction: event-trace atlas

Use event history itself as the page's visual language. A compact trace in the hero introduces the four outcomes; numbered sections then connect each outcome to an example event sequence, a concrete result, and the responsible AllSource products.

This direction wins over a card grid because the interface demonstrates the product model instead of decorating generic marketing copy. It wins over an interactive demo-first page because the core explanation remains server-rendered, indexable, accessible, and fast.

## Information hierarchy

1. Direct answer: four workloads served by durable event history.
2. Four anchor links visible with the hero.
3. Audit, replay, agent memory, and financial history as distinct event traces.
4. Product ownership: Core stores, Prime remembers, hosted services operate, MCP connectors expose tools.
5. Honest fit guide covering workloads that should remain in PostgreSQL, streaming transport, or a vector database.
6. Optional product video, loaded only on user action.
7. Direct-answer FAQ and hosted/self-hosted calls to action.

## Visual system

- Preserve existing dark palette and typography.
- Use thin rules, square panels, mono labels, sequence numbers, and event nodes.
- Signature element: vertical event trace with ordered event names and a derived outcome.
- Avoid gradients, floating cards, decorative blobs, and reveal animation.
- Keep all interactive surfaces keyboard-visible and give links explicit destinations.
- Respect reduced-motion by using no essential animation.

## Content rules

- Never claim mutable databases are incapable of these jobs.
- Explain when event history is primary, and when another store remains the better default.
- Attribute replay and immutable history to Core; agent recall to Prime; operation to hosted services; agent access to MCP connectors.
- Keep benchmarks scoped as published Core reference results, not universal application guarantees.

## Acceptance criteria

- Four use cases and their anchor links appear without waiting for client JavaScript.
- Each use case shows an event sequence, outcome, product ownership, and deeper route.
- Video does not autoplay or preload its payload.
- FAQ and breadcrumb structured data match visible copy.
- Page works at mobile and desktop widths with no horizontal overflow.
- Type check, lint, focused tests, production build, and browser verification pass.
