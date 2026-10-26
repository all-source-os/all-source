import { siteConfig } from "@/lib/config";
import { constructMetadata } from "@/lib/utils";

export const metadata = constructMetadata({
  title: "API Reference",
  description: `API Reference for ${siteConfig.name} — REST endpoints for events, latest state, read models, views and schemas.`,
  canonical: "/docs/api",
});

const endpoints = [
  {
    method: "POST",
    path: "/api/v1/events",
    description: "Ingest a new event into a stream.",
  },
  {
    method: "GET",
    path: "/api/v1/events/query",
    description:
      "Query events by stream, time range, entity, or event type. Returns {events, count}.",
  },
  {
    method: "POST",
    path: "/api/v1/events/batch",
    description: "Ingest several events in one request.",
  },
  {
    method: "GET",
    path: "/api/v1/entities/{entity_id}/state",
    description:
      "Current state of one entity with its version. No history unless history=true. Supports If-None-Match.",
  },
  {
    method: "GET",
    path: "/api/v1/event-feed",
    description: "Tenant event feed with resumable positions, for folding your own read models.",
  },
  {
    method: "GET",
    path: "/api/v1/read-models/{name}",
    description: "Read the rows of a read model, with its revision and feed checkpoint.",
  },
  {
    method: "PUT",
    path: "/api/v1/read-models/{name}",
    description: "Commit read-model row changes against an expected revision.",
  },
  {
    method: "PUT",
    path: "/api/v1/views/{view}",
    description: "Define a view: source prefixes, a row-key template and declarative operations.",
  },
  {
    method: "GET",
    path: "/api/v1/views",
    description: "List your views. GET or DELETE /api/v1/views/{view} reads or removes one.",
  },
  {
    method: "GET",
    path: "/api/v1/views/{view}/entities/{key}",
    description: "Latest state of one view row. Supports If-None-Match.",
  },
  {
    method: "GET",
    path: "/api/v1/views/{view}/entities",
    description: "Latest state of many view rows by prefix, paginated.",
  },
  {
    method: "GET",
    path: "/api/v1/views/{view}/changes",
    description:
      "Rows changed after a revision, including deletions. wait holds the request up to 20 s.",
  },
  {
    method: "GET",
    path: "/api/v1/schemas",
    description: "List registered event schemas.",
  },
  {
    method: "GET",
    path: "/health",
    description: "Health check endpoint (root path, not under /api/v1).",
  },
  {
    method: "GET",
    path: "/metrics",
    description: "Prometheus-format metrics.",
  },
];

export default function ApiReferencePage() {
  return (
    <div className="mx-auto w-full max-w-screen-md px-4 lg:px-8 py-24">
      <h1 className="text-3xl font-bold text-foreground sm:text-4xl mb-2">AllSource HTTP API</h1>
      <p className="text-lg text-muted-foreground mb-10">
        The hosted gateway at{" "}
        <code className="text-sm bg-muted px-1.5 py-0.5 rounded">https://api.all-source.xyz</code>{" "}
        serves the endpoints below; self-hosted Core serves the same paths on port 3900. All data
        endpoints use the <code className="text-sm bg-muted px-1.5 py-0.5 rounded">/api/v1/</code>{" "}
        prefix.
      </p>

      <div className="prose prose-neutral dark:prose-invert max-w-none space-y-8">
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">Authentication</h2>
          <p className="text-muted-foreground leading-relaxed">
            Requests to the Query Service (port 3902) require a Bearer token in the{" "}
            <code className="text-sm bg-muted px-1.5 py-0.5 rounded">Authorization</code> header.
            Direct Core access (port 3900) does not require authentication — it is intended for
            internal service-to-service communication.
          </p>
          <pre className="mt-3 rounded-lg border border-border bg-muted/50 p-4 text-sm overflow-x-auto">
            <code>{`Authorization: Bearer <your-api-key>`}</code>
          </pre>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">Endpoints</h2>
          <div className="space-y-3">
            {endpoints.map((ep) => (
              <div key={`${ep.method}-${ep.path}`} className="rounded-lg border border-border p-4">
                <div className="flex items-center gap-3 mb-1">
                  <span
                    className={`inline-block rounded px-2 py-0.5 text-xs font-mono font-bold ${
                      ep.method === "POST" || ep.method === "PUT"
                        ? "bg-green-500/10 text-green-600 dark:text-green-400"
                        : "bg-blue-500/10 text-blue-600 dark:text-blue-400"
                    }`}
                  >
                    {ep.method}
                  </span>
                  <code className="text-sm font-mono text-foreground">{ep.path}</code>
                </div>
                <p className="text-sm text-muted-foreground">{ep.description}</p>
              </div>
            ))}
          </div>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">Example: Ingest an Event</h2>
          <pre className="rounded-lg border border-border bg-muted/50 p-4 text-sm overflow-x-auto">
            <code>{`curl -X POST http://localhost:3900/api/v1/events \\
  -H "Content-Type: application/json" \\
  -d '{
    "stream": "orders",
    "event_type": "OrderCreated",
    "data": {
      "order_id": "ord-123",
      "amount": 99.99
    }
  }'`}</code>
          </pre>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">Full Reference</h2>
          <p className="text-muted-foreground leading-relaxed">
            For complete endpoint documentation including request/response schemas, query
            parameters, and advanced features, see the{" "}
            <a
              href="https://github.com/all-source-os/all-source/tree/main/docs"
              target="_blank"
              rel="noopener noreferrer"
              className="text-foreground underline underline-offset-4 hover:opacity-80"
            >
              docs directory on GitHub
            </a>
            .
          </p>
        </section>
      </div>
    </div>
  );
}
