import { describe, expect, test } from "bun:test";
import {
  emptyPartnership,
  type PartnershipRevision,
  parsePartnershipImport,
  partnershipPriority,
  partnershipStats,
  safePartnershipURL,
} from "./partnerships-api";

describe("partnership evidence", () => {
  test("unknown scores stay unknown; paid demand does not inflate priority", () => {
    const record = emptyPartnership();
    expect(partnershipPriority(record)).toBeNull();
    record.score = {
      model: "human-rubric-v1",
      run_at: "2026-09-27T00:00:00Z",
      fit: 2,
      leverage: 1.5,
      access: 2,
      paid_demand: 3,
      rationale: "Synthetic fixture",
    };
    expect(partnershipPriority(record)).toBe(5.5);
  });
  test("counts organisations, not messages; uncertain sends are not successes", () => {
    const record = emptyPartnership();
    const message = {
      id: "m1",
      channel: "email" as const,
      direction: "outbound" as const,
      outcome: "sent" as const,
      destination: "team@example.com",
      subject: "",
      body: "Synthetic",
      occurred_at: "2026-09-27T00:00:00Z",
      verification: "fixture",
      approval_note: "fixture",
    };
    const row: PartnershipRevision = {
      record: { ...record, messages: [message, { ...message, id: "m2" }] },
      revision: 1,
      saved_at: "",
      actor: "test",
    };
    const unknown: PartnershipRevision = {
      ...row,
      record: { ...record, messages: [{ ...message, outcome: "unknown" }] },
    };
    expect(partnershipStats([row, unknown])).toEqual({
      researched: 2,
      sent: 1,
      replies: 0,
      unchecked: 1,
    });
    row.record.reply_checked_at = "2026-09-27T00:00:00Z";
    row.record.messages.push({ ...message, id: "m3", outcome: "received", direction: "inbound" });
    expect(partnershipStats([row])).toEqual({ researched: 1, sent: 1, replies: 1, unchecked: 0 });
  });
  test("unsafe links and credentials are not clickable", () => {
    for (const url of [
      "javascript:alert(1)",
      "data:text/html,test",
      "https://secret:password@example.com",
      "not a URL",
    ])
      expect(safePartnershipURL(url)).toBeUndefined();
    expect(safePartnershipURL("https://example.com/research")).toBe("https://example.com/research");
  });
  test("imports reject wrong shapes, oversized data and host aliases", () => {
    const record = {
      ...emptyPartnership(),
      organization: "Example",
      website: "https://example.com",
    };
    expect(parsePartnershipImport(JSON.stringify({ records: [record] }))).toHaveLength(1);
    for (const data of [
      {},
      { records: [] },
      { records: [record, { ...record, website: "https://www.example.com/about" }] },
      { records: [{ organization: "Missing website" }] },
    ])
      expect(() => parsePartnershipImport(JSON.stringify(data))).toThrow();
    expect(() => parsePartnershipImport(" ".repeat(4_000_001))).toThrow("4 MB");
  });
});
