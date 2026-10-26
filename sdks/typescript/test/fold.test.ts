import { describe, expect, test } from "bun:test";
import type { EventFolder } from "../src/fold";
import { foldEvents } from "../src/fold";
import type { Event } from "../src/types";

function makeEvent(overrides: Partial<Event> = {}): Event {
  return {
    id: "evt-1",
    event_type: "test.event",
    entity_id: "entity-1",
    payload: {},
    metadata: {},
    timestamp: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

describe("foldEvents", () => {
  test("basic fold: sums amounts", () => {
    const events: Event[] = [
      makeEvent({ id: "1", event_type: "deposit", payload: { amount: 100 } }),
      makeEvent({ id: "2", event_type: "deposit", payload: { amount: 50 } }),
      makeEvent({ id: "3", event_type: "withdrawal", payload: { amount: 30 } }),
    ];

    const folder: EventFolder<number> & { _balance: number; _applied: boolean } = {
      _balance: 0,
      _applied: false,
      apply(event: Event): boolean {
        if (event.event_type === "deposit") {
          this._balance += event.payload.amount as number;
          this._applied = true;
          return true;
        }
        if (event.event_type === "withdrawal") {
          this._balance -= event.payload.amount as number;
          this._applied = true;
          return true;
        }
        return false;
      },
      finalize(): number | undefined {
        return this._applied ? this._balance : undefined;
      },
    };

    const result = foldEvents(folder, events);
    expect(result).toBe(120); // 100 + 50 - 30
  });

  test("empty events returns undefined", () => {
    const folder: EventFolder<string> & { _applied: boolean } = {
      _applied: false,
      apply(): boolean {
        this._applied = true;
        return true;
      },
      finalize(): string | undefined {
        return this._applied ? "done" : undefined;
      },
    };

    const result = foldEvents(folder, []);
    expect(result).toBeUndefined();
  });

  test("no relevant events returns undefined", () => {
    const events: Event[] = [
      makeEvent({ event_type: "irrelevant.event" }),
      makeEvent({ event_type: "another.irrelevant" }),
    ];

    const folder: EventFolder<number> & { _count: number; _applied: boolean } = {
      _count: 0,
      _applied: false,
      apply(event: Event): boolean {
        if (event.event_type === "target.event") {
          this._count++;
          this._applied = true;
          return true;
        }
        return false;
      },
      finalize(): number | undefined {
        return this._applied ? this._count : undefined;
      },
    };

    const result = foldEvents(folder, events);
    expect(result).toBeUndefined();
  });
});
