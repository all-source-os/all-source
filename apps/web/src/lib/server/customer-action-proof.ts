import { createHash, createHmac } from "node:crypto";

function canonical(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonical);
  if (value !== null && typeof value === "object")
    return Object.entries(value)
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
      .map(([key, item]) => [key, canonical(item)]);
  return value;
}

const hash = (value: string) => createHash("sha256").update(value).digest("hex");

// Server-only relay proof. Never returned to the browser or a connected agent.
export function customerActionProof(operation: string, input: unknown, session: string) {
  const secret = process.env.CUSTOMER_HUMAN_ACTION_SECRET;
  if (!secret || Buffer.byteLength(secret) < 32) throw new Error("Action relay unavailable");
  const now = Math.floor(Date.now() / 1000);
  const payload = Buffer.from(
    JSON.stringify({
      v: 1,
      aud: "allsource-product-action",
      op: operation,
      iat: now,
      exp: now + 30,
      session_sha256: hash(session),
      body_sha256: hash(JSON.stringify(canonical(input))),
    })
  ).toString("base64url");
  return `${payload}.${createHmac("sha256", secret).update(payload).digest("base64url")}`;
}
