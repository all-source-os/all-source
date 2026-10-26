# allsource-go

Go client for the [AllSource Event Store](https://all-source.xyz) API.

## Installation

```bash
go get github.com/all-source-os/allsource-go
```

## Authentication

AllSource uses API keys. Get one from your dashboard at [all-source.xyz](https://all-source.xyz) (Settings → API Keys) or mint one via `POST /api/v1/auth/api-keys`. The SDK sends it in the `X-API-Key` header.

```go
client := allsource.New(os.Getenv("ALLSOURCE_API_KEY"), "https://api.all-source.xyz")
```

## Quickstart

```go
package main

import (
	"context"
	"fmt"
	"log"

	allsource "github.com/all-source-os/allsource-go"
)

func main() {
	client := allsource.New("as_your_api_key", "https://api.all-source.xyz")

	ctx := context.Background()

	// Ingest an event
	event, err := client.Ingest(ctx, "user.signup", "user-123", map[string]any{
		"plan": "pro",
		"source": "web",
	})
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Ingested event: %s\n", event.EventID)

	// Query events
	result, err := client.Query(ctx, allsource.QueryOptions{
		EventType: "user.signup",
		Limit:     10,
	})
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Found %d events\n", result.Count)
}
```

## Latest state

Read one entity's folded state without its history. Pass the `ETag` back as
`IfNoneMatch`; an unchanged state returns `ErrNotModified` and no body.

```go
st, err := client.EntityState(ctx, "skill:abc", allsource.EntityStateOptions{})
// later
_, err = client.EntityState(ctx, "skill:abc", allsource.EntityStateOptions{IfNoneMatch: st.ETag})
if errors.Is(err, allsource.ErrNotModified) {
	// st is still current
}
```

`Query` results carry `EntityVersion` when the query filters by entity, and
`QueryOptions.IfNoneMatch` makes a query conditional too.

## Views

A view is a declarative read model. You define it; the Query Service folds it
from the event feed and AllSource stores the rows.

```go
_, err := client.DefineView(ctx, "ops_status", allsource.ViewDefinition{
	Source: allsource.ViewSource{Prefixes: []string{"ops.probe."}},
	Key:    "{payload.probe}:{day:timestamp}",
	Ops: []allsource.ViewOp{
		{On: "ops.probe.observed", When: map[string]any{"payload.ok": true}, Increment: map[string]int64{"ok": 1}},
		{On: "ops.probe.observed", Max: map[string]any{"last_seen_at": "timestamp"}},
	},
})

row, err := client.Latest(ctx, "ops_status", "probe-a:2026-10-27", allsource.LatestOptions{})

for row, err := range client.LatestMany(ctx, "ops_status", allsource.LatestManyOptions{Prefix: "probe-a:"}) {
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println(row.Key, row.State)
}
```

`Sync` keeps a local copy current: a snapshot of every page, then a
long-polled changes loop. Store `Checkpoint` after applying a batch whose
`Snapshotting` is false, and pass it back to resume. A view reset re-runs the
snapshot (that batch has `Reset` set), and a `429` or `503` with `Retry-After`
is waited out. The HTTP client timeout must exceed `Wait`.

```go
err := client.Sync(ctx, "ops_status", allsource.SyncOptions{Checkpoint: stored, Wait: 20 * time.Second},
	func(b allsource.SyncBatch) error {
		if b.Reset {
			clear(rows)
		}
		for _, r := range b.Upserts {
			rows[r.Key] = r.State
		}
		for _, k := range b.Deletes {
			delete(rows, k)
		}
		if !b.Snapshotting {
			saveCheckpoint(b.Checkpoint)
		}
		return nil
	})
```

`GetView`, `ListViews` and `DeleteView` manage definitions.

## Event feed and read models

`Feed` pages through the tenant's event feed; resume from `page.Next()`. An
`*EpochChangedError` means the feed generation changed: rebuild from position
0 of the new epoch and commit with `Reset`. `ReadModel(name).Page` and
`.Commit` read and write a read model; a stale `ExpectedRevision` returns a
`*VersionConflictError`.

## Error Handling

API errors are returned as `*allsource.APIError` with helper methods:

```go
_, err := client.Ingest(ctx, "user.signup", "user-123", data)
if err != nil {
	var apiErr *allsource.APIError
	if errors.As(err, &apiErr) {
		switch {
		case apiErr.IsUnauthorized():
			log.Fatal("Invalid API key")
		case apiErr.IsRateLimited():
			log.Println("Rate limited, retrying...")
		case apiErr.IsNotFound():
			log.Println("Resource not found")
		case apiErr.IsForbidden():
			log.Println("Insufficient permissions")
		default:
			log.Printf("API error %d: %s", apiErr.StatusCode, apiErr.Message)
		}
	}
}
```

## Query Options

```go
result, err := client.Query(ctx, allsource.QueryOptions{
	EventType: "order.placed",     // Filter by event type
	EntityID:  "order-456",        // Filter by entity ID
	Start:     "2026-01-01T00:00:00Z", // Start of time range (ISO-8601)
	End:       "2026-02-01T00:00:00Z", // End of time range (ISO-8601)
	Limit:     50,                 // Max results
	Offset:    10,                 // Skip first N results
})
```

## Custom HTTP Client

```go
httpClient := &http.Client{Timeout: 60 * time.Second}
client := allsource.NewWithHTTPClient("as_key", "https://api.all-source.xyz", httpClient)
```
