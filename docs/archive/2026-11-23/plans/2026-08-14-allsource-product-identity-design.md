# AllSource product identity design

## Decision

Use **AllSource Event Store** as the canonical branded modifier and **AllSource**
as the short UI name. Publish one disambiguation hub at `/what-is-allsource`.
Define four product layers from one typed source:

1. AllSource Core — store;
2. AllSource Prime — remember;
3. Hosted AllSource — operate;
4. AllSource MCP connectors — connect.

Chronis remains a reference application. Industry and solution pages remain
workloads, not extra products.

## Alternatives considered

### Rename the whole product

Strongest long-term escape from ArcGIS and company-name collision, but creates
domain, package, repository, backlink, and customer migration cost. Rejected for
this iteration.

### Lead only with agent memory

More familiar buyer problem, but collapses Core and Prime and makes the event
store architecture harder to explain accurately. Rejected as canonical entity
definition; retained as the active commercial bet.

### Canonical modifier plus product map

Chosen. Preserves existing brand equity and URLs while making entity and layer
boundaries explicit to people, crawlers, and answer engines.

## Page design

Subject: developer infrastructure for technical founders and engineering
teams. Page job: answer “what is AllSource?” without requiring another click.

- Palette: existing AllSource background, foreground, border, and primary
  tokens. No new decorative palette.
- Type: existing UI font for prose; monospace labels for system roles and
  verification state.
- Layout: answer first, real dependency map, four boundary panels,
  disambiguation table, visible FAQ.
- Signature: product map uses verbs—Store, Remember, Operate, Connect—because
  those verbs encode actual ownership.
- Motion: none. Entity definition should remain stable, printable, and fast.

## Content architecture

- Homepage and header link to canonical hub.
- JSON-LD adds alternate names and a disambiguating description.
- `llms.txt` starts with canonical entity and layer map.
- Two substantive articles answer identity and component-boundary intents.
- Sitemap uses canonical host and actual `updatedAt` values rather than request
  time.
- Tests reject missing disambiguation and merged MCP registries.

## Measurement

Use `docs/marketing/GEO_AEO_ENGINE_QUESTION_CHECKLIST.md`. Test fresh engine
sessions, capture citations, score identity and product boundaries separately,
and refuse to average away a wrong-company answer.
