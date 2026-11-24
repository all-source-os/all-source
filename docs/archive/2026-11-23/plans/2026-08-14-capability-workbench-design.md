# Capability workbench design

## Product job

Give engineers evaluating AllSource one public, hands-on explanation of six related capabilities: MCP data access, point-in-time reconstruction, event timelines, graph views, pipelines, and projections. The page must show that each view comes from the same ordered event history. It must not imply that a scripted public fixture is a live tenant or that Core pipelines and Query Service projections are the same subsystem.

## Chosen approach

Build one deterministic order-history workbench on `/examples`. A shared event cursor controls every panel. Moving backward through six order events changes reconstructed state, visible graph nodes, pipeline input, projection output, and MCP response. This makes the relationship between capabilities observable instead of describing six isolated features.

Alternatives rejected:

- Separate feature cards and animations: easy to scan, but they hide the shared-history model.
- A live seeded tenant: higher fidelity, but public auth, network, and seed failures make the core product explanation unreliable.

The fixture is labelled clearly. Existing code samples and repository examples remain below the workbench for evaluators who want executable integrations.

## Visual system

- **Subject:** event-sourced developer infrastructure.
- **Audience:** engineers and technical buyers evaluating AllSource.
- **Single job:** make one event history inspectable from every product surface.
- **Palette:** existing AllSource background, foreground, border, muted, and primary tokens. Emerald marks accepted data; amber marks historical position; violet distinguishes Prime graph context.
- **Type:** UI copy uses the existing sans stack; timestamps, event types, payloads, tool calls, and stage names use monospace.
- **Layout:** a wide instrument panel with a persistent event rail, then paired evidence panels. Hard dividers encode subsystem boundaries. Mobile stacks panels without hiding any capability.
- **Signature:** the `as_of` rail is the shared control for every panel. It is functional structure, not decoration.

## Data and boundaries

Six immutable events describe one order. A pure reducer reconstructs order state through the selected event. A graph builder reveals related entities only when their source event exists. A projection builder returns a Query Service-style current-state read model. Pipeline stages show Core inline filter/map/branch processing for the selected event, then explicitly hand off source history to a separate Query Service fold.

MCP examples use real event-store connector tool names and parameters: `event_timeline`, `reconstruct_state`, and `query_events`. Responses are computed locally from the fixture and labelled as an interactive walkthrough, not a live connector call.

## Failure, accessibility, and verification

The deterministic fixture has no network failure state. Controls use native buttons and a labelled range input. Keyboard focus remains visible, selected states are expressed with `aria-pressed` or `aria-current`, and changing output is announced through a polite live region. Motion is limited to short color transitions and respects reduced-motion defaults.

Pure reducer, graph, projection, and MCP-model logic receive unit coverage. Component coverage proves cursor and MCP controls update derived output. Type-check, lint, web tests, production build, desktop/mobile screenshots, keyboard interaction, and browser console checks form the release gate.
