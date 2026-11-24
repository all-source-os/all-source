---
title: "Web App"
status: CURRENT
last_updated: 2026-11-24
category: service
port: 3000
technology: TypeScript
---

# AllSource Event Store - Web App

> The web application for AllSource Event Store.

## Quick Start

```bash
# Install dependencies
bun install

# Start development server
bun run dev
```

Visit **http://localhost:3000** to access the app.

## Project Structure

```
apps/web/
├── src/
│   ├── app/                   # Next.js App Router
│   │   ├── (auth)/            # Authentication routes
│   │   ├── (marketing)/       # Marketing pages
│   │   ├── api/               # Route handlers; api/[...path] proxies to the Query Service
│   │   ├── dashboard/         # Dashboard pages
│   │   ├── ui-test/page.tsx   # UI component testing
│   │   └── page.tsx           # Homepage
│   │
│   ├── components/            # React components
│   ├── lib/
│   │   └── api/client.ts      # ApiClient class and the apiClient singleton
│   └── proxy.ts               # Request proxy (redirects)
│
├── next.config.mjs
└── package.json
```

## API Integration

Browser code uses the `apiClient` singleton from `src/lib/api/client.ts`. It sends relative `/api/...` requests, which the Next.js app forwards to the Query Service.

```typescript
import { apiClient } from "@/lib/api/client";

// Ingest a batch of events
const batch = await apiClient.createEventBatch([
  { entity_id: "order-123", event_type: "OrderPlaced", payload: { total: 42 } },
]);

// List events for an entity
const events = await apiClient.listEvents({ entity_id: "order-123", limit: 50 });

// Structured query
const matches = await apiClient.executeQuery({
  entity_id: "order-123",
  event_type: "OrderPlaced",
});

// Metrics
const metrics = await apiClient.getMetrics();
```

Every method returns `ApiResponse<T>`, which is `{ data?: T; error?: ApiError }`. Check `error` before using `data`:

```typescript
const res = await apiClient.listEvents({ entity_id: "order-123" });
if (res.error) {
  console.error(res.error.code, res.error.message);
} else {
  console.log(res.data);
}
```

`apiClient.setAsOf(timestamp)` adds an `as_of` parameter to every later request. Pass `null` to return to the present.

## Environment Variables

Copy `.env.example` to `.env.local`. The variables the app reads:

| Variable | Default in `.env.example` | Purpose |
|----------|---------------------------|---------|
| `NEXT_PUBLIC_API_URL` | `http://localhost:3902` | Query Service base URL for server-side code |
| `NEXT_PUBLIC_CONTROL_PLANE_URL` | `http://localhost:3901` | Control Plane base URL |
| `NEXT_PUBLIC_WS_URL` | empty | WebSocket base URL for the live feed. Empty uses the dev server origin |
| `CONTROL_PLANE_INTERNAL_URL` | `http://localhost:3901` | Server-side target for OAuth rewrites |
| `QUERY_SERVICE_URL` | `http://localhost:3902` in development, `https://allsource-query.fly.dev` in production | Query Service target for the `/api` proxy |

`NEXT_PUBLIC_*` values are inlined at build time, so a change needs a rebuild.

## Tech Stack

- **Framework**: Next.js 16.3.5
- **UI library**: React ^19.1.0
- **Language**: TypeScript ^6.0.2
- **Styling**: Tailwind CSS ^4.1.11
- **UI Components**: `@allsource/ui` (workspace package)
- **Code Quality**: Biome (`@biomejs/biome` ^2.3.13)
- **Runtime**: Bun (`BUN_VERSION` build argument in `Dockerfile`)

## Development Commands

```bash
bun run dev          # Start dev server
bun run build        # Build for production
bun run start        # Start the production build
bun run type-check   # TypeScript check
bun run lint         # Lint code
bun run format       # Format code
bun run test         # Run unit tests (vitest)
```
