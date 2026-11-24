# Demo Zone recovery

## Evidence

- Production `/api/v1/config/benchmarks` returns 401 because generic v1 proxy
  targets branded gateway. Query Service returns same public config with 200.
- Existing demo seed endpoint writes Core's global `default` tenant while all
  dashboard reads and WebSocket topics use authenticated tenant ID.
- Page trusts one transient `seeded` boolean. Refresh loses state; successful
  global seed can render three empty panels for authenticated workspace.
- Search fetches tenant events but assigns random similarity values and calls
  results vector similarity. That claim is false.

## Decision

Make Demo Zone tenant-owned and evidence-led:

- Proxy benchmark config directly to Query Service.
- Seed a bounded sample through authenticated tenant batch-ingest endpoint.
- Discover current workspace events on load. Successful seed hydrates recent
  events immediately; WebSocket remains progressive enhancement.
- Rename playground to event search. Use deterministic lexical ranking and show
  why each result matched. Do not claim vector similarity until real semantic
  endpoint exists.
- Replace three equal empty columns with one live workbench: event stream and
  searchable result area carry primary interaction; benchmark proof stays a
  compact supporting rail.
- Failure states keep retry or setup action inside relevant panel.

## Visual system

Keep AllSource shell, Inter body, mono data labels, cyan primary, emerald live,
amber degraded, slate surfaces. Signature: event-path rail from stored event to
search result to reproducible benchmark. No gradients or decorative metrics.
