import { CircuitBreaker } from "./circuit-breaker";
import { type EventFolder, foldEvents } from "./fold";
import {
  type AllSourceConfig,
  AllSourceError,
  type ConditionalReadOptions,
  type CreatedEvent,
  type EntityState,
  type EntityStateOptions,
  EpochChangedError,
  type FeedOptions,
  type FeedPage,
  type HealthResponse,
  type IngestEventInput,
  InvalidViewError,
  type LatestManyOptions,
  NotModified,
  type PrimeProjection,
  type PrimeProjectionAck,
  type PrimeProvenance,
  type PrimeSnapshot,
  type QueryEventsParams,
  type QueryEventsResponse,
  type ReadModelCommit,
  type ReadModelCommitResult,
  type ReadModelHandle,
  type ReadModelPage,
  type ReadModelPageOptions,
  type RetryConfig,
  VersionConflictError,
  type ViewChangesOptions,
  type ViewChangesPage,
  type ViewDefinition,
  type ViewEntitiesOptions,
  type ViewEntitiesPage,
  type ViewEntity,
  type ViewInfo,
  ViewResetError,
  type ViewRow,
  ViewRowNotFound,
  type ViewSummary,
  type ViewSyncCheckpoint,
  type ViewSyncItem,
  type ViewSyncOptions,
} from "./types";

const DEFAULT_TIMEOUT = 30_000;

const DEFAULT_RETRY: RetryConfig = {
  maxRetries: 3,
  baseDelay: 200,
  backoffFactor: 2.0,
  maxDelay: 10_000,
};

const RETRYABLE_STATUS_CODES = new Set([408, 429, 500, 502, 503, 504]);

/** Extra time a long-poll request gets on top of the seconds Core may hold it. */
const LONG_POLL_GRACE_MS = 10_000;

/** Pause between `sync` polls when Core answered caught-up without holding the request. */
const IDLE_POLL_MS = 1_000;

interface SendOptions {
  body?: unknown;
  headers?: Record<string, string>;
  /** Non-2xx statuses handed back to the caller instead of thrown. */
  accept?: readonly number[];
  timeoutMs?: number;
}

interface RawResponse {
  status: number;
  etag: string | null;
  body: unknown;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function numberField(body: Record<string, unknown>, key: string): number {
  const value = body[key];
  return typeof value === "number" ? value : Number.NaN;
}

function apiError(
  status: number,
  statusText: string,
  body: unknown,
  retryAfterMs: number | undefined
): AllSourceError {
  const message = `AllSource API error: ${status} ${statusText}`;
  if (isRecord(body)) {
    if (status === 409 && body.error === "version_conflict") {
      return new VersionConflictError(
        message,
        status,
        body,
        numberField(body, "expected_version"),
        numberField(body, "current_version")
      );
    }
    if (status === 409 && body.error === "view_reset") {
      return new ViewResetError(message, status, body, numberField(body, "generation"));
    }
    if (status === 400 && body.error === "invalid_view") {
      return new InvalidViewError(message, status, body, String(body.detail ?? ""));
    }
  }
  return new AllSourceError(message, status, body, retryAfterMs);
}

/** `Retry-After` is either delta-seconds or an HTTP date (RFC 9110 section 10.2.3). */
function parseRetryAfter(value: string | null): number | undefined {
  if (value === null || value.trim() === "") return undefined;
  const seconds = Number(value);
  if (Number.isFinite(seconds)) return Math.max(0, seconds * 1000);
  const date = Date.parse(value);
  return Number.isNaN(date) ? undefined : Math.max(0, date - Date.now());
}

function withQuery(path: string, params: object): string {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined && value !== null) {
      query.set(key, String(value));
    }
  }
  const qs = query.toString();
  return qs ? `${path}?${qs}` : path;
}

function conditionalHeaders(ifNoneMatch: string | undefined): Record<string, string> | undefined {
  return ifNoneMatch ? { "If-None-Match": ifNoneMatch } : undefined;
}

function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    if (signal?.aborted) return resolve();
    const timer = setTimeout(done, ms);
    function done() {
      clearTimeout(timer);
      signal?.removeEventListener("abort", done);
      resolve();
    }
    signal?.addEventListener("abort", done, { once: true });
  });
}

const segment = encodeURIComponent;

/** Raw create-ack as returned by the gateway (id keyed as `event_id`). */
interface RawCreatedEvent {
  event_id?: string;
  id?: string;
  timestamp: string;
  version?: number;
}

function normalizeCreated(raw: RawCreatedEvent): CreatedEvent {
  return {
    id: raw.event_id ?? raw.id ?? "",
    timestamp: raw.timestamp,
    version: raw.version,
  };
}

/**
 * Strips trailing slashes in linear time.
 *
 * The previous implementation used `replace(/\/+$/, "")`. That pattern
 * backtracks polynomially: a caller-supplied `baseUrl` ending in a long run of
 * slashes (e.g. "https://x/" + "/".repeat(50_000)) makes the regex engine retry
 * every split point, hanging the thread. `baseUrl` is library input, so a
 * caller passing a value built from user data could stall their own process.
 * A scan from the end has no backtracking to do.
 */
function stripTrailingSlashes(url: string): string {
  let end = url.length;
  while (end > 0 && url.charCodeAt(end - 1) === 47 /* "/" */) end--;
  return url.slice(0, end);
}

export class AllSourceClient {
  private readonly baseUrl: string;
  private readonly apiKey: string;
  private readonly timeout: number;
  private readonly retryConfig: RetryConfig;
  private readonly circuitBreaker: CircuitBreaker;
  private readonly fetch: typeof globalThis.fetch;

  constructor(config: AllSourceConfig) {
    if (!config.baseUrl) throw new Error("baseUrl is required");
    if (!config.apiKey) throw new Error("apiKey is required");

    this.baseUrl = stripTrailingSlashes(config.baseUrl);
    this.apiKey = config.apiKey;
    this.timeout = config.timeout ?? DEFAULT_TIMEOUT;

    this.retryConfig = {
      maxRetries: config.retry?.maxRetries ?? DEFAULT_RETRY.maxRetries,
      baseDelay: config.retry?.baseDelay ?? DEFAULT_RETRY.baseDelay,
      backoffFactor: config.retry?.backoffFactor ?? DEFAULT_RETRY.backoffFactor,
      maxDelay: config.retry?.maxDelay ?? DEFAULT_RETRY.maxDelay,
    };

    this.circuitBreaker = new CircuitBreaker(config.circuitBreaker);
    this.fetch = config.fetch ?? globalThis.fetch;
  }

  /**
   * Ingest a single event into AllSource Core.
   *
   * Returns the created event's `id`, `timestamp` and `version`. The gateway
   * wraps the ack in a `{ data }` envelope and keys the id as `event_id`; this
   * unwraps and normalizes it to `id`.
   */
  async ingestEvent(event: IngestEventInput): Promise<CreatedEvent> {
    const res = await this.request<{ data: RawCreatedEvent }>("POST", "/api/v1/events", event);
    return normalizeCreated(res.data);
  }

  /**
   * Ingest a batch of events into AllSource Core.
   *
   * Returns how many were ingested and the created events
   * (`id` / `timestamp` / `version`).
   */
  async ingestBatch(
    events: IngestEventInput[]
  ): Promise<{ count: number; events: CreatedEvent[] }> {
    const res = await this.request<{ data: RawCreatedEvent[]; count: number }>(
      "POST",
      "/api/v1/events/batch",
      { events }
    );
    return { count: res.count, events: (res.data ?? []).map(normalizeCreated) };
  }

  /**
   * Query events with optional filters.
   *
   * With `ifNoneMatch` set to an earlier response's `etag`, an unchanged
   * result comes back as `NotModified` instead of the events.
   */
  queryEvents(params?: QueryEventsParams): Promise<QueryEventsResponse>;
  queryEvents(
    params: QueryEventsParams,
    options: ConditionalReadOptions
  ): Promise<QueryEventsResponse | NotModified>;
  async queryEvents(
    params: QueryEventsParams = {},
    options: ConditionalReadOptions = {}
  ): Promise<QueryEventsResponse | NotModified> {
    const res = await this.send("GET", withQuery("/api/v1/events/query", params), {
      headers: conditionalHeaders(options.ifNoneMatch),
    });
    if (res.status === 304) return new NotModified(res.etag ?? options.ifNoneMatch ?? null);
    const body = res.body as QueryEventsResponse;
    return res.etag === null ? body : { ...body, etag: res.etag };
  }

  /**
   * Current state of one entity, without its history unless `history: true`.
   *
   * With `ifNoneMatch` set to an earlier result's `etag`, an unchanged state
   * comes back as `NotModified`.
   */
  entityState(
    entityId: string,
    options?: Omit<EntityStateOptions, "ifNoneMatch"> & { ifNoneMatch?: undefined }
  ): Promise<EntityState>;
  entityState(entityId: string, options: EntityStateOptions): Promise<EntityState | NotModified>;
  async entityState(
    entityId: string,
    options: EntityStateOptions = {}
  ): Promise<EntityState | NotModified> {
    const path = withQuery(`/api/v1/entities/${segment(entityId)}/state`, {
      as_of: options.asOf,
      history: options.history === true ? "true" : undefined,
    });
    const res = await this.send("GET", path, {
      headers: conditionalHeaders(options.ifNoneMatch),
    });
    if (res.status === 304) return new NotModified(res.etag ?? options.ifNoneMatch ?? null);
    return { ...(res.body as Omit<EntityState, "etag">), etag: res.etag };
  }

  /**
   * One page of the tenant's event feed. Resume from `page.next`.
   *
   * Throws `EpochChangedError` when the feed generation changed since
   * `cursor` was taken; rebuild from position 0 of `currentEpoch`.
   */
  async feed(options: FeedOptions = {}): Promise<FeedPage> {
    const path = withQuery("/api/v1/event-feed", {
      epoch: options.cursor?.epoch,
      position: options.cursor?.position,
      limit: options.limit,
      prefixes: options.prefixes?.length ? options.prefixes.join(",") : undefined,
      entity_id: options.entityId,
    });
    let res: RawResponse;
    try {
      res = await this.send("GET", path);
    } catch (error) {
      if (error instanceof VersionConflictError) throw new EpochChangedError(error);
      throw error;
    }
    const body = res.body as Omit<FeedPage, "next">;
    const last = body.positions[body.positions.length - 1];
    return { ...body, next: { epoch: body.cursor.epoch, position: last ?? body.cursor.position } };
  }

  /**
   * Page through or commit to one read model.
   *
   * `commit` throws `VersionConflictError` on a stale `expectedRevision` and
   * is never retried.
   */
  readModel<S = unknown>(name: string): ReadModelHandle<S> {
    const path = `/api/v1/read-models/${segment(name)}`;
    return {
      page: async (options: ReadModelPageOptions = {}): Promise<ReadModelPage<S>> => {
        const res = await this.send(
          "GET",
          withQuery(path, {
            prefix: options.prefix,
            after: options.after,
            limit: options.limit,
            revision: options.revision,
          })
        );
        return res.body as ReadModelPage<S>;
      },
      commit: async (commit: ReadModelCommit<S>): Promise<ReadModelCommitResult> => {
        const res = await this.send("PUT", path, {
          body: {
            expected_revision: commit.expectedRevision,
            checkpoint: commit.checkpoint,
            ready: commit.ready,
            reset: commit.reset ?? false,
            changes: commit.changes,
          },
        });
        return res.body as ReadModelCommitResult;
      },
    };
  }

  /** Create or replace a view. Throws `InvalidViewError` when Core rejects the definition. */
  async defineView(name: string, definition: ViewDefinition): Promise<ViewInfo> {
    const res = await this.send("PUT", `/api/v1/views/${segment(name)}`, { body: definition });
    return res.body as ViewInfo;
  }

  /** A view's definition and fold progress. */
  async getView(name: string): Promise<ViewInfo> {
    const res = await this.send("GET", `/api/v1/views/${segment(name)}`);
    return res.body as ViewInfo;
  }

  /** Remove a view's definition and rows. */
  async deleteView(name: string): Promise<void> {
    await this.send("DELETE", `/api/v1/views/${segment(name)}`);
  }

  /** The tenant's views. */
  async listViews(): Promise<ViewSummary[]> {
    const res = await this.send("GET", "/api/v1/views");
    return (res.body as { views: ViewSummary[] }).views;
  }

  /**
   * Latest state of one view row. Returns `ViewRowNotFound` when the row does
   * not exist, and `NotModified` when `ifNoneMatch` still matches.
   */
  async latest<S = Record<string, unknown>>(
    view: string,
    key: string,
    options: ConditionalReadOptions = {}
  ): Promise<ViewRow<S> | NotModified | ViewRowNotFound> {
    const res = await this.send("GET", `/api/v1/views/${segment(view)}/entities/${segment(key)}`, {
      headers: conditionalHeaders(options.ifNoneMatch),
      accept: [404],
    });
    if (res.status === 304) return new NotModified(res.etag ?? options.ifNoneMatch ?? null);
    if (res.status === 404) {
      if (!isRecord(res.body) || res.body.error !== "not_found") {
        throw new AllSourceError("AllSource API error: 404 Not Found", 404, res.body);
      }
      return new ViewRowNotFound(view, key, res.body.ready === true);
    }
    return { ...(res.body as Omit<ViewRow<S>, "etag">), etag: res.etag };
  }

  /** One page of a view's rows, ordered by key. */
  async viewEntities<S = Record<string, unknown>>(
    view: string,
    options: ViewEntitiesOptions = {}
  ): Promise<ViewEntitiesPage<S>> {
    const path = withQuery(`/api/v1/views/${segment(view)}/entities`, {
      prefix: options.prefix,
      after: options.after,
      limit: options.limit,
    });
    const res = await this.send("GET", path);
    return res.body as ViewEntitiesPage<S>;
  }

  /** Every row of a view (optionally under `prefix`), reading page after page. */
  async *latestMany<S = Record<string, unknown>>(
    view: string,
    options: LatestManyOptions = {}
  ): AsyncGenerator<ViewEntity<S>, void, undefined> {
    let after: string | undefined;
    for (;;) {
      const page = await this.viewEntities<S>(view, {
        prefix: options.prefix,
        after,
        limit: options.limit,
      });
      yield* page.rows;
      if (page.next === null || page.next === after) return;
      after = page.next;
    }
  }

  /**
   * Rows changed after a revision. Throws `ViewResetError` when `after`
   * predates the view's last reset.
   */
  async viewChanges<S = Record<string, unknown>>(
    view: string,
    options: ViewChangesOptions
  ): Promise<ViewChangesPage<S>> {
    const path = withQuery(`/api/v1/views/${segment(view)}/changes`, {
      after: options.after,
      limit: options.limit,
      wait: options.wait,
    });
    const timeoutMs = options.wait
      ? Math.max(this.timeout, options.wait * 1000 + LONG_POLL_GRACE_MS)
      : undefined;
    const res = await this.send("GET", path, { timeoutMs });
    return res.body as ViewChangesPage<S>;
  }

  /**
   * Keep a local copy of a view current: every row of the snapshot, then each
   * change after it, then a checkpoint to store. Pass the stored checkpoint
   * back to resume without a snapshot. On a view reset it yields `reset` and
   * re-reads the snapshot. A `429`/`503` with `Retry-After` is waited out and
   * the same request repeated.
   */
  async *sync<S = Record<string, unknown>>(
    view: string,
    options: ViewSyncOptions = {}
  ): AsyncGenerator<ViewSyncItem<S>, void, undefined> {
    const { signal } = options;
    let checkpoint = options.checkpoint;
    let caughtUp = false;
    while (!signal?.aborted) {
      if (!checkpoint) {
        checkpoint = yield* this.snapshot<S>(view, options.limit, signal);
        if (!checkpoint) return;
        yield { type: "checkpoint", checkpoint };
        caughtUp = false;
        continue;
      }
      const current = checkpoint;
      const after = current.revision;
      const wait = caughtUp ? options.wait : undefined;
      const started = Date.now();
      let page: ViewChangesPage<S>;
      try {
        page = await this.honouringRetryAfter(
          () => this.viewChanges<S>(view, { after, wait }),
          signal
        );
      } catch (error) {
        if (error instanceof ViewResetError) {
          yield { type: "reset", generation: error.generation };
          checkpoint = undefined;
          continue;
        }
        throw error;
      }
      for (const change of page.changes) {
        if (change.state === null) {
          yield { type: "delete", key: change.key, version: change.version };
        } else {
          yield { type: "upsert", key: change.key, state: change.state, version: change.version };
        }
      }
      caughtUp = page.caught_up;
      if (page.revision !== current.revision || page.generation !== current.generation) {
        checkpoint = { revision: page.revision, generation: page.generation };
        yield { type: "checkpoint", checkpoint };
      }
      if (signal?.aborted) return;
      const held = Date.now() - started;
      if (caughtUp && page.changes.length === 0 && held < IDLE_POLL_MS) {
        await sleep(IDLE_POLL_MS - held, signal);
      }
    }
  }

  private async *snapshot<S>(
    view: string,
    limit: number | undefined,
    signal: AbortSignal | undefined
  ): AsyncGenerator<ViewSyncItem<S>, ViewSyncCheckpoint | undefined, undefined> {
    let after: string | undefined;
    let watermark: ViewSyncCheckpoint | undefined;
    for (;;) {
      if (signal?.aborted) return undefined;
      const page = await this.honouringRetryAfter(
        () => this.viewEntities<S>(view, { after, limit }),
        signal
      );
      if (watermark && page.generation !== watermark.generation) {
        yield { type: "reset", generation: page.generation };
        after = undefined;
        watermark = undefined;
        continue;
      }
      // The first page's revision is the watermark: a row on an earlier page
      // that changed while later pages were read is re-sent by `changes`.
      watermark ??= { revision: page.revision, generation: page.generation };
      for (const row of page.rows) {
        yield { type: "upsert", key: row.key, state: row.state, version: row.version };
      }
      if (page.next === null || page.next === after) return watermark;
      after = page.next;
    }
  }

  private async honouringRetryAfter<T>(call: () => Promise<T>, signal?: AbortSignal): Promise<T> {
    for (;;) {
      try {
        return await call();
      } catch (error) {
        if (!(error instanceof AllSourceError) || signal?.aborted) throw error;
        const delay = error.status === 429 || error.status === 503 ? error.retryAfterMs : undefined;
        if (delay === undefined) throw error;
        await sleep(delay, signal);
      }
    }
  }
  /** List all Prime projection definitions from the gateway. */
  async listPrimeProjections(): Promise<PrimeProjection[]> {
    const res = await this.request<{ data: PrimeProjection[]; count: number }>(
      "GET",
      "/api/v1/prime/projections"
    );
    return res.data;
  }

  /** Define (or update) a Prime projection with per-field merge policies. */
  async definePrimeProjection(
    entityType: string,
    fieldPolicies: Record<string, string>
  ): Promise<PrimeProjectionAck> {
    const res = await this.request<{ data: PrimeProjectionAck }>(
      "POST",
      "/api/v1/prime/projections",
      { entity_type: entityType, field_policies: fieldPolicies }
    );
    return res.data;
  }

  /** Project a Prime node into a materialized snapshot. */
  async projectNode(nodeId: string): Promise<PrimeSnapshot> {
    const res = await this.request<{ data: PrimeSnapshot }>(
      "POST",
      `/api/v1/prime/nodes/${nodeId}/project`
    );
    return res.data;
  }

  /** Fetch provenance for a single field on a Prime node. Throws AllSourceError (404) when none. */
  async nodeFieldProvenance(nodeId: string, field: string): Promise<PrimeProvenance> {
    const res = await this.request<{ data: PrimeProvenance }>(
      "GET",
      `/api/v1/prime/nodes/${nodeId}/fields/${field}/provenance`
    );
    return res.data;
  }

  /** Query events and fold them into a state using the provided folder. */
  async queryAndFold<S>(params: QueryEventsParams, folder: EventFolder<S>): Promise<S | undefined> {
    const result = await this.queryEvents(params);
    return foldEvents(folder, result.events);
  }

  /** Check the health of the AllSource service. */
  async getHealth(): Promise<HealthResponse> {
    return this.request<HealthResponse>("GET", "/health");
  }

  private async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const res = await this.send(method, path, { body });
    return res.body as T;
  }

  private async send(
    method: string,
    path: string,
    options: SendOptions = {}
  ): Promise<RawResponse> {
    this.circuitBreaker.check();

    const { body } = options;
    const url = `${this.baseUrl}${path}`;
    const headers: Record<string, string> = {
      // The gateway reads only Authorization; the Query Service reads X-API-Key first.
      Authorization: `Bearer ${this.apiKey}`,
      "X-API-Key": this.apiKey,
      Accept: "application/json",
      ...options.headers,
    };
    if (body !== undefined) {
      headers["Content-Type"] = "application/json";
    }
    const accept = options.accept ?? [];
    const timeout = options.timeoutMs ?? this.timeout;

    let lastError: unknown;
    let retryAfterMs: number | undefined;
    const maxAttempts = this.retryConfig.maxRetries + 1;

    for (let attempt = 0; attempt < maxAttempts; attempt++) {
      if (attempt > 0) {
        const delay = retryAfterMs ?? this.computeDelay(attempt);
        retryAfterMs = undefined;
        await new Promise((r) => setTimeout(r, delay));
      }

      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), timeout);

      try {
        const response = await this.fetch(url, {
          method,
          headers,
          body: body !== undefined ? JSON.stringify(body) : undefined,
          signal: controller.signal,
        });
        const etag = response.headers.get("ETag");

        if (response.status === 304) {
          this.circuitBreaker.recordSuccess();
          return { status: 304, etag, body: null };
        }

        if (!response.ok && !accept.includes(response.status)) {
          const text = await response.text();
          let responseBody: unknown;
          try {
            responseBody = JSON.parse(text);
          } catch {
            responseBody = text;
          }
          const error = apiError(
            response.status,
            response.statusText,
            responseBody,
            parseRetryAfter(response.headers.get("Retry-After"))
          );

          if (RETRYABLE_STATUS_CODES.has(response.status) && attempt < maxAttempts - 1) {
            lastError = error;
            retryAfterMs = error.retryAfterMs;
            continue;
          }

          this.circuitBreaker.recordFailure();
          throw error;
        }

        this.circuitBreaker.recordSuccess();
        const text = await response.text();
        return { status: response.status, etag, body: text ? JSON.parse(text) : null };
      } catch (error) {
        if (error instanceof AllSourceError) {
          if (error.isRetryable() && attempt < maxAttempts - 1) {
            lastError = error;
            continue;
          }
          this.circuitBreaker.recordFailure();
          throw error;
        }
        if (error instanceof DOMException && error.name === "AbortError") {
          const timeoutErr = new AllSourceError(`Request timeout after ${timeout}ms`, 0);
          if (attempt < maxAttempts - 1) {
            lastError = timeoutErr;
            continue;
          }
          this.circuitBreaker.recordFailure();
          throw timeoutErr;
        }
        // Network errors are retryable
        if (attempt < maxAttempts - 1) {
          lastError = error;
          continue;
        }
        this.circuitBreaker.recordFailure();
        throw error;
      } finally {
        clearTimeout(timer);
      }
    }

    // Should not reach here, but just in case
    this.circuitBreaker.recordFailure();
    throw lastError;
  }

  private computeDelay(attempt: number): number {
    const { baseDelay, backoffFactor, maxDelay } = this.retryConfig;
    const exponentialDelay = baseDelay * backoffFactor ** (attempt - 1);
    const capped = Math.min(exponentialDelay, maxDelay);
    // Add jitter: random value between 0 and capped delay
    const jitter = Math.random() * capped;
    return Math.floor(jitter);
  }
}
