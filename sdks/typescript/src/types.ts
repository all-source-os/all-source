/** Retry configuration with exponential backoff. */
export interface RetryConfig {
  /** Maximum number of retries. Defaults to 3. */
  maxRetries: number;
  /** Base delay in milliseconds. Defaults to 200. */
  baseDelay: number;
  /** Backoff multiplier. Defaults to 2.0. */
  backoffFactor: number;
  /** Maximum delay in milliseconds. Defaults to 10000. */
  maxDelay: number;
}

/** Circuit breaker configuration. */
export interface CircuitBreakerConfig {
  /** Number of consecutive failures before opening the circuit. Defaults to 5. */
  threshold: number;
  /** Milliseconds to wait before entering half-open state. Defaults to 30000. */
  recoveryTimeout: number;
}

/** Configuration options for the AllSource client. */
export interface AllSourceConfig {
  /** Base URL of the AllSource Query Service (e.g., "https://allsource-query.fly.dev"). */
  baseUrl: string;
  /** API key, sent as `Authorization: Bearer` and as `X-API-Key`. */
  apiKey: string;
  /** Request timeout in milliseconds. Defaults to 30000. */
  timeout?: number;
  /** Retry configuration. Uses sensible defaults if omitted. */
  retry?: Partial<RetryConfig>;
  /** Circuit breaker configuration. Uses sensible defaults if omitted. */
  circuitBreaker?: Partial<CircuitBreakerConfig>;
  /** Custom fetch function. Defaults to globalThis.fetch. */
  fetch?: typeof globalThis.fetch;
}

/** An event to ingest into AllSource. */
export interface IngestEventInput {
  /** The type of event (e.g., "user.signup", "order.placed"). */
  event_type: string;
  /** The entity this event belongs to (e.g., user ID, order ID). */
  entity_id: string;
  /** Arbitrary JSON payload for the event. */
  payload: Record<string, unknown>;
  /** Optional metadata (e.g., source, version, ip). */
  metadata?: Record<string, unknown>;
}

/** A stored event returned from AllSource. */
export interface Event {
  id: string;
  event_type: string;
  entity_id: string;
  payload: Record<string, unknown>;
  metadata: Record<string, unknown>;
  timestamp: string;
  stream_id?: string;
  version?: number;
  tenant_id?: string;
}

/**
 * Acknowledgement for a created event, returned by `ingestEvent` and (per item)
 * `ingestBatch`. The gateway keys the id as `event_id`; the SDK normalizes it
 * to `id` for parity with queried events.
 */
export interface CreatedEvent {
  /** The stored event's id. */
  id: string;
  /** Server-assigned timestamp (ISO 8601). */
  timestamp: string;
  /** Monotonic version assigned by Core, when provided. */
  version?: number;
}

/** Query parameters for filtering events. */
export interface QueryEventsParams {
  /** Filter by entity ID. */
  entity_id?: string;
  /** Filter by event type. */
  event_type?: string;
  /** Maximum number of events to return. */
  limit?: number;
  /** Number of events to skip. */
  offset?: number;
  /** Start time filter (ISO 8601). */
  since?: string;
  /** End time filter (ISO 8601). */
  until?: string;
}

/** Response from querying events. */
export interface QueryEventsResponse {
  events: Event[];
  count: number;
  total_count?: number;
  has_more?: boolean;
  /** Number of the tenant's events for `entity_id`; Core sends it only when the query filters by entity. */
  entity_version?: number;
  /** `ETag` of the response, to pass as `ifNoneMatch` on the next read. */
  etag?: string;
}

/** Options for a conditional read. */
export interface ConditionalReadOptions {
  /** A tag from an earlier response's `etag`. Core answers `304` when nothing changed. */
  ifNoneMatch?: string;
}

/**
 * Result of a conditional read whose tag still matches: the state the caller
 * already holds is current. Check with `instanceof NotModified`.
 */
export class NotModified {
  readonly notModified = true as const;
  constructor(public readonly etag: string | null) {}
}

/** Options for `entityState`. */
export interface EntityStateOptions extends ConditionalReadOptions {
  /** Fold only events up to this time (ISO 8601). */
  asOf?: string;
  /** Include the entity's `history[]`. Defaults to false. */
  history?: boolean;
}

/** Current state of one entity, folded by Core from the tenant's events. */
export interface EntityState {
  entity_id: string;
  current_state: Record<string, unknown>;
  event_count: number;
  last_updated: string | null;
  as_of: string | null;
  /** Number of this tenant's events folded into the state. */
  version: number;
  /** Present only when requested with `history: true`. */
  history?: Event[];
  etag: string | null;
}

/** Position in the tenant's event feed. */
export interface FeedCursor {
  epoch: number;
  position: number;
}

/** Options for one event-feed page. */
export interface FeedOptions {
  /** Where to resume. Omit to start from the beginning of the current epoch. */
  cursor?: FeedCursor;
  /** Event-type prefixes, at most 8. */
  prefixes?: string[];
  entityId?: string;
  limit?: number;
}

/** One page of the event feed. */
export interface FeedPage {
  tenant_id: string;
  cursor: FeedCursor;
  head: FeedCursor;
  events: Event[];
  positions: number[];
  caught_up: boolean;
  /** Cursor for the next page: the last element of `positions`, or the request cursor when empty. */
  next: FeedCursor;
}

/** Options for one read-model page. */
export interface ReadModelPageOptions {
  prefix?: string;
  after?: string;
  limit?: number;
  /** A `content_revision` from an earlier page; an unchanged model answers `unchanged: true`. */
  revision?: number;
}

/** One page of a read model. */
export interface ReadModelPage<S = unknown> {
  revision: number;
  content_revision: number;
  checkpoint: FeedCursor | null;
  ready: boolean;
  /** True when `revision` matched: `rows` is empty and is not a list of deletions. */
  unchanged: boolean;
  rows: Record<string, S>;
  next: string | null;
}

/** A read-model write, applied atomically at `expectedRevision`. */
export interface ReadModelCommit<S = unknown> {
  expectedRevision: number;
  checkpoint: FeedCursor;
  ready: boolean;
  /** Replace every row instead of merging. Defaults to false. */
  reset?: boolean;
  /** Rows to write; `null` deletes the row. */
  changes: Record<string, S | null>;
}

/** Result of a read-model commit. */
export interface ReadModelCommitResult {
  revision: number;
}

/** Handle on one read model. */
export interface ReadModelHandle<S = unknown> {
  page(options?: ReadModelPageOptions): Promise<ReadModelPage<S>>;
  commit(commit: ReadModelCommit<S>): Promise<ReadModelCommitResult>;
}

/** Value expression in a view operation: a path such as `payload.status`, or a literal. */
export type ViewValueExpr = string | { literal: unknown };

/** One declarative view operation. Exactly one effect key is set alongside `on`. */
export interface ViewOperation {
  /** Exact event type, or a prefix ending in `.` or `*`. */
  on: string;
  when?: Record<string, string | number | boolean | null>;
  set?: Record<string, ViewValueExpr>;
  unset?: string[];
  delete?: true;
  increment?: Record<string, number>;
  max?: Record<string, ViewValueExpr>;
  min?: Record<string, ViewValueExpr>;
  append?: Record<string, ViewValueExpr>;
  remove?: Record<string, ViewValueExpr>;
  status?: {
    field: string;
    from: "suffix" | ViewValueExpr;
    transitions?: Record<string, string[]>;
  };
}

/** A view definition, sent to Core verbatim. */
export interface ViewDefinition {
  source: { prefixes: string[] };
  /** Row-key template, e.g. `{payload.probe}:{day:timestamp}`. */
  key: string;
  ops: ViewOperation[];
}

/** A stored view with its fold progress. */
export interface ViewInfo {
  view: string;
  definition: ViewDefinition;
  generation: number;
  revision: number;
  checkpoint: FeedCursor | null;
  ready: boolean;
}

/** One entry of `listViews`. */
export interface ViewSummary {
  view: string;
  generation: number;
  revision: number;
  ready: boolean;
}

/** Latest state of one view row. */
export interface ViewRow<S = Record<string, unknown>> {
  view: string;
  key: string;
  state: S;
  /** Model revision that last wrote this key. */
  version: number;
  revision: number;
  generation: number;
  ready: boolean;
  etag: string | null;
}

/**
 * The view has no row for the key. `ready: false` means the view is still
 * rebuilding, so the row may yet appear.
 */
export class ViewRowNotFound {
  readonly notFound = true as const;
  constructor(
    public readonly view: string,
    public readonly key: string,
    public readonly ready: boolean
  ) {}
}

/** One row of a view entities page. */
export interface ViewEntity<S = Record<string, unknown>> {
  key: string;
  state: S;
  version: number;
}

/** Options for one view entities page. */
export interface ViewEntitiesOptions {
  prefix?: string;
  after?: string;
  limit?: number;
}

/** One page of a view's rows. */
export interface ViewEntitiesPage<S = Record<string, unknown>> {
  view: string;
  rows: ViewEntity<S>[];
  next: string | null;
  /** Watermark to start `viewChanges` from after the last page. */
  revision: number;
  content_revision: number;
  generation: number;
  ready: boolean;
}

/** Options for `latestMany`. */
export interface LatestManyOptions {
  prefix?: string;
  /** Page size, 1..500. */
  limit?: number;
}

/** Options for one view changes call. */
export interface ViewChangesOptions {
  after: number;
  limit?: number;
  /** Seconds (0..20) Core holds the request when nothing changed after `after`. */
  wait?: number;
}

/** One changed row. `state: null` is a deletion. */
export interface ViewChange<S = Record<string, unknown>> {
  key: string;
  version: number;
  state: S | null;
}

/** Rows changed after a revision. */
export interface ViewChangesPage<S = Record<string, unknown>> {
  view: string;
  changes: ViewChange<S>[];
  /** Cursor for the next call. */
  revision: number;
  caught_up: boolean;
  generation: number;
}

/** Position in a view's change stream, stored by the caller of `sync`. */
export interface ViewSyncCheckpoint {
  revision: number;
  generation: number;
}

/** Options for `sync`. */
export interface ViewSyncOptions {
  /** Resume from a stored checkpoint instead of reading the snapshot. */
  checkpoint?: ViewSyncCheckpoint;
  /** Long-poll seconds (0..20) once caught up. Defaults to 0. */
  wait?: number;
  /** Snapshot page size, 1..500. */
  limit?: number;
  /** Stops the loop at the next request boundary. */
  signal?: AbortSignal;
}

/**
 * One item from `sync`. `reset` means the view was rebuilt: drop every row
 * held locally, because the snapshot that follows re-sends only live rows.
 */
export type ViewSyncItem<S = Record<string, unknown>> =
  | { type: "upsert"; key: string; state: S; version: number }
  | { type: "delete"; key: string; version: number }
  | { type: "reset"; generation: number }
  | { type: "checkpoint"; checkpoint: ViewSyncCheckpoint };

/** A Prime projection definition (entity type plus per-field merge policies). */
export interface PrimeProjection {
  entity_type: string;
  field_policies: Record<string, string>;
}

/** Acknowledgement returned when defining a Prime projection. */
export interface PrimeProjectionAck {
  entity_type: string;
  persisted: boolean;
}

/** A materialized Prime node snapshot. */
export interface PrimeSnapshot {
  entity_type: string;
  fields: Record<string, unknown>;
  observation_count: number;
}

/** Provenance for a single field on a Prime node. */
export interface PrimeProvenance {
  field: string;
  value: unknown;
  source_event_id: string;
  source_event_at: string;
  merge_policy_applied: string;
}

/** Response from the health endpoint. */
export interface HealthResponse {
  status: string;
  [key: string]: unknown;
}

/** Error thrown by the AllSource client. */
export class AllSourceError extends Error {
  constructor(
    message: string,
    public readonly status: number,
    public readonly body?: unknown,
    /** Milliseconds from the response's `Retry-After` header, when it sent one. */
    public readonly retryAfterMs?: number
  ) {
    super(message);
    this.name = "AllSourceError";
  }

  /** Whether the error is a 401 Unauthorized. */
  isUnauthorized(): boolean {
    return this.status === 401;
  }

  /** Whether the error is a 429 Rate Limited. */
  isRateLimited(): boolean {
    return this.status === 429;
  }

  /** Whether the error is a 404 Not Found. */
  isNotFound(): boolean {
    return this.status === 404;
  }

  /** Whether the error is a 403 Forbidden. */
  isForbidden(): boolean {
    return this.status === 403;
  }

  /** Whether the error is a server error (5xx). */
  isServerError(): boolean {
    return this.status >= 500;
  }

  /** Whether the error is retryable (408, 429, 500, 502, 503, 504). */
  isRetryable(): boolean {
    return [408, 429, 500, 502, 503, 504].includes(this.status);
  }
}

/** `409 version_conflict`: a read-model commit's `expected_revision` is stale. Not retried. */
export class VersionConflictError extends AllSourceError {
  constructor(
    message: string,
    status: number,
    body: unknown,
    public readonly expectedVersion: number,
    public readonly currentVersion: number
  ) {
    super(message, status, body);
    this.name = "VersionConflictError";
  }
}

/**
 * `409 version_conflict` from the event feed: the feed epoch changed (Core
 * restart or eviction). Rebuild from position 0 of `currentEpoch` and commit
 * with `reset: true`.
 */
export class EpochChangedError extends VersionConflictError {
  constructor(conflict: VersionConflictError) {
    super(
      conflict.message,
      conflict.status,
      conflict.body,
      conflict.expectedVersion,
      conflict.currentVersion
    );
    this.name = "EpochChangedError";
  }

  get currentEpoch(): number {
    return this.currentVersion;
  }
}

/** `409 view_reset`: the changes cursor predates the view's last reset; re-read the entity pages. */
export class ViewResetError extends AllSourceError {
  constructor(
    message: string,
    status: number,
    body: unknown,
    public readonly generation: number
  ) {
    super(message, status, body);
    this.name = "ViewResetError";
  }
}

/** `400 invalid_view`: Core rejected a view definition. */
export class InvalidViewError extends AllSourceError {
  constructor(
    message: string,
    status: number,
    body: unknown,
    public readonly detail: string
  ) {
    super(message, status, body);
    this.name = "InvalidViewError";
  }
}

/** Error thrown when the circuit breaker is open. */
export class CircuitOpenError extends Error {
  constructor(message = "Circuit breaker is open") {
    super(message);
    this.name = "CircuitOpenError";
  }
}
