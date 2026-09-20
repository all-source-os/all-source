/**
 * GEO layer 1 — the server side of AI-referral attribution.
 *
 * The browser beacon (`src/components/geo-referral-tracker.tsx`) posts a raw
 * referrer here; this route classifies it, builds the `geo.referral.observed`
 * envelope and forwards it to the Control Plane gateway, which authenticates
 * and injects `tenant_id` before it reaches Core.
 *
 * ## Why this route exists at all
 *
 * The AllSource ingest key must never reach the browser. A client-side
 * `fetch` to the gateway would put a writable API key in the JS bundle, where
 * anyone could read it and write arbitrary events into our stream. So the key
 * lives only in this server runtime, read from `process.env` at request time.
 *
 * ## What is not trusted
 *
 * Everything in the body is attacker-controlled. The route re-derives the
 * surface from the referrer itself rather than accepting a `surface` field,
 * bounds every string, and refuses anything that is not a known AI surface —
 * so this endpoint cannot be used to write arbitrary rows.
 */

import { type NextRequest, NextResponse } from "next/server";
import {
  buildReferralEnvelope,
  CONVERSION_KINDS,
  type ConversionKind,
  classifyArrival,
  normaliseLandingPath,
} from "@/lib/geo-referrers";

/**
 * Read at request time, not module load, so a Vercel env var change takes
 * effect on redeploy without being baked into the build.
 *
 * Same names as `tooling/geo` (`geo_core::config`) — one env-var scheme for
 * the whole GEO programme, not two.
 */
function gatewayUrl(): string {
  return (process.env.ALLSOURCE_API_URL || "https://api.all-source.xyz").replace(/\/+$/, "");
}

/** Longest referrer we will store. Real ones are far shorter; this bounds abuse. */
const MAX_REFERRER = 2048;
/** Longest landing path we will store. */
const MAX_PATH = 512;
/** Longest session id we will accept. */
const MAX_SESSION_ID = 64;
/** Longest user agent we will store. */
const MAX_USER_AGENT = 512;
/** Body size ceiling. A legitimate beacon is a few hundred bytes. */
const MAX_BODY_BYTES = 4096;

/** Session ids are opaque; anything outside this alphabet is a probe, not a session. */
const SESSION_ID_PATTERN = /^[A-Za-z0-9_-]{1,64}$/;

function clamp(value: unknown, max: number): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  if (!trimmed) return null;
  return trimmed.slice(0, max);
}

interface Body {
  referrer?: unknown;
  url?: unknown;
  sessionId?: unknown;
  observedAt?: unknown;
  conversion?: unknown;
}

export async function POST(request: NextRequest) {
  const raw = await request.text();
  if (raw.length > MAX_BODY_BYTES) {
    return NextResponse.json({ error: "Body too large" }, { status: 413 });
  }

  let body: Body;
  try {
    body = JSON.parse(raw) as Body;
  } catch {
    return NextResponse.json({ error: "Invalid JSON body" }, { status: 400 });
  }

  const url = clamp(body.url, MAX_PATH + MAX_REFERRER);
  if (!url) {
    return NextResponse.json({ error: "url is required" }, { status: 400 });
  }
  const referrer = clamp(body.referrer, MAX_REFERRER);

  // The client is NOT asked which surface it came from — the server derives it.
  const classified = classifyArrival({ referrer, url });
  if (!classified) {
    // Not an AI arrival. 204 rather than an error: the beacon fires on every
    // first page view and "this was ordinary traffic" is a normal outcome,
    // not a client mistake.
    return new NextResponse(null, { status: 204 });
  }

  const sessionIdRaw = clamp(body.sessionId, MAX_SESSION_ID);
  const sessionId = sessionIdRaw && SESSION_ID_PATTERN.test(sessionIdRaw) ? sessionIdRaw : null;

  let conversionKind: ConversionKind | null = null;
  if (body.conversion !== undefined && body.conversion !== null) {
    const candidate = clamp(body.conversion, 64);
    if (!candidate || !CONVERSION_KINDS.includes(candidate as ConversionKind)) {
      return NextResponse.json(
        { error: `conversion must be one of: ${CONVERSION_KINDS.join(", ")}` },
        { status: 400 }
      );
    }
    conversionKind = candidate as ConversionKind;
  }

  // A conversion re-states the ORIGINAL arrival's timestamp so it lands on the
  // same entity as version 2. Anything else (or nothing) is treated as an
  // arrival happening now — a client cannot backdate an arrival into an
  // earlier reporting week, because we only honour its timestamp when it is
  // inside the session-lifetime bound below.
  const observedAt = resolveObservedAt(body.observedAt, conversionKind !== null);

  const envelope = await buildReferralEnvelope({
    observedAt,
    surface: classified.surface,
    referrerUrl: classified.referrerUrl,
    landingPath: normaliseLandingPath(classified.landingPath).slice(0, MAX_PATH),
    sessionId,
    userAgent: clamp(request.headers.get("user-agent"), MAX_USER_AGENT),
    conversionKind,
  });

  const apiKey = process.env.ALLSOURCE_API_KEY;
  if (!apiKey) {
    // Loud, not silent. A GEO window with an unnoticed hole in it is worse
    // than a visible failure — and this is the single most likely
    // misconfiguration (the env var is a Fly secret).
    console.error(
      "[geo] ALLSOURCE_API_KEY is not set — geo.referral.observed dropped. " +
        "Set it with fly secrets set --app allsource-web ALLSOURCE_API_KEY=..."
    );
    return NextResponse.json({ error: "GEO telemetry is not configured" }, { status: 503 });
  }

  try {
    const response = await fetch(`${gatewayUrl()}/api/v1/events`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${apiKey}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(envelope),
    });
    if (!response.ok) {
      console.error(
        "[geo] gateway rejected geo.referral.observed:",
        response.status,
        await response.text()
      );
      return NextResponse.json({ error: "Failed to record referral" }, { status: 502 });
    }
  } catch (error) {
    console.error("[geo] gateway unreachable for geo.referral.observed:", error);
    return NextResponse.json({ error: "Failed to record referral" }, { status: 502 });
  }

  return NextResponse.json({ recorded: true, surface: classified.surface }, { status: 202 });
}

/**
 * How long after an arrival a conversion may still claim that arrival's
 * timestamp. Beyond this the client is either confused or lying, and we
 * record the conversion as its own observation rather than trusting the
 * backdate.
 */
const MAX_SESSION_AGE_MS = 24 * 60 * 60 * 1000;

function resolveObservedAt(raw: unknown, isConversion: boolean): Date {
  const now = new Date();
  if (!isConversion || typeof raw !== "string") return now;
  const parsed = new Date(raw);
  if (Number.isNaN(parsed.getTime())) return now;
  const age = now.getTime() - parsed.getTime();
  if (age < 0 || age > MAX_SESSION_AGE_MS) return now;
  return parsed;
}
