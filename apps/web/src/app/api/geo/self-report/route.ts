/**
 * GEO layer 4 — the server side of the "how did you find us?" capture.
 *
 * The onboarding question (`src/components/geo-discovery-question.tsx`) posts
 * an answer here; this route validates it against the shared vocabulary,
 * resolves the tenant from the session cookie, builds the
 * `geo.selfreport.captured` envelope and forwards it to the Control Plane
 * gateway, which authenticates and injects `tenant_id` before it reaches Core.
 *
 * ## Why this route exists at all
 *
 * The AllSource ingest key must never reach the browser. A client-side `fetch`
 * to the gateway would put a writable key in the JS bundle. The key lives only
 * in this server runtime, read from `process.env` at request time.
 *
 * ## What is not trusted
 *
 * Everything in the body is attacker-controlled. In particular the client is
 * NOT asked who it is: `contact_ref` and `tier` are derived server-side from
 * the httpOnly `auth_token` cookie, so this endpoint cannot be used to attach
 * an answer to somebody else's tenant. The discovery source must be one of the
 * twelve ids in the shared vocabulary — free text there would become an
 * unqueryable mess and the AI-sourced share would stop meaning anything.
 *
 * ## Privacy
 *
 * `verbatim` is user-submitted free text. It is stored in Core as event data
 * and is operator-visible — see the layer-4 privacy note in
 * `docs/runbooks/GEO_MEASUREMENT.md` and the "Information you choose to give
 * us" clause in `src/app/(marketing)/privacy/page.tsx`. `contact_ref` is the
 * tenant id, never an email address.
 */

import { type NextRequest, NextResponse } from "next/server";
import {
  buildSelfReportEnvelope,
  isDiscoverySource,
  promptsForVerbatim,
  WEB_CAPTURE_PATH,
} from "@/lib/geo-discovery-sources";

/**
 * Read at request time, not module load, so a Vercel env var change takes
 * effect on redeploy without being baked into the build. Same names as
 * `tooling/geo` — one env-var scheme for the whole GEO programme.
 */
function gatewayUrl(): string {
  return (process.env.ALLSOURCE_API_URL || "https://api.all-source.xyz").replace(/\/+$/, "");
}

/**
 * The Query Service, which serves `/api/tenant`. Same resolution as
 * `src/app/api/auth/session/route.ts` — the branded gateway does not route
 * `/api/tenant`.
 */
function queryServiceUrl(): string {
  return (
    process.env.QUERY_SERVICE_URL ||
    (process.env.NODE_ENV === "production"
      ? "https://allsource-query.fly.dev"
      : "http://localhost:3902")
  );
}

/**
 * Longest free-text answer we will store. A real answer is a sentence; this
 * bounds both abuse and the amount of user-submitted text we accumulate.
 */
const MAX_VERBATIM = 500;
/** Body size ceiling. A legitimate answer is a few hundred bytes. */
const MAX_BODY_BYTES = 4096;
/** Longest source id we will even look at before rejecting it. */
const MAX_SOURCE_ID = 64;

interface Body {
  source?: unknown;
  verbatim?: unknown;
  observedAt?: unknown;
}

/**
 * How long a follow-up may still claim the original capture's timestamp.
 *
 * The question is answered in two beats: the source lands the moment it is
 * clicked (so a user who then walks away still counts), and the free text
 * follows when they finish typing. The second POST replays the first's
 * `observed_at` so it derives the *same* natural key and Core appends version
 * 2 to the same entity — one signup, not two. Exactly the mechanism layer 1
 * uses for a conversion. Beyond this bound the client is confused or lying and
 * the answer is recorded as its own observation instead.
 */
const MAX_REPLAY_AGE_MS = 60 * 60 * 1000;

function resolveObservedAt(raw: unknown): Date {
  const now = new Date();
  if (typeof raw !== "string") return now;
  const parsed = new Date(raw);
  if (Number.isNaN(parsed.getTime())) return now;
  const age = now.getTime() - parsed.getTime();
  if (age < 0 || age > MAX_REPLAY_AGE_MS) return now;
  return parsed;
}

/** Decode a JWT payload without verifying it — the backend already validated the cookie. */
function decodeJwtPayload(token: string): Record<string, unknown> {
  try {
    const parts = token.split(".");
    if (parts.length !== 3) return {};
    return JSON.parse(Buffer.from(parts[1] ?? "", "base64url").toString("utf-8")) as Record<
      string,
      unknown
    >;
  } catch {
    return {};
  }
}

/**
 * The tenant's current tier, best-effort.
 *
 * Best-effort on purpose: a missing tier costs the layer its revenue split for
 * one row, while a *failed capture* costs the whole answer — including the
 * free text, which is the part no probe can reconstruct. `null` is honest and
 * the report excludes untiered rows from both sides of the paid split rather
 * than counting them as unpaid.
 */
async function fetchTier(token: string): Promise<string | null> {
  try {
    const response = await fetch(`${queryServiceUrl()}/api/tenant`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    if (!response.ok) return null;
    const body = (await response.json()) as {
      data?: { subscription?: { tier?: unknown }; subscription_tier?: unknown };
    };
    const tier = body.data?.subscription?.tier ?? body.data?.subscription_tier;
    return typeof tier === "string" && tier.trim() ? tier.trim() : null;
  } catch {
    return null;
  }
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

  const source = typeof body.source === "string" ? body.source.trim().slice(0, MAX_SOURCE_ID) : "";
  if (!isDiscoverySource(source)) {
    return NextResponse.json(
      { error: "source must be one of the known discovery sources" },
      { status: 400 }
    );
  }

  // The free text is only meaningful for the AI options — that is the only
  // context in which "what did you ask it?" is a question. Dropping it
  // elsewhere keeps the field from becoming a general-purpose comment box we
  // never asked for and would then have to hold.
  let verbatim: string | null = null;
  if (promptsForVerbatim(source) && typeof body.verbatim === "string") {
    const trimmed = body.verbatim.trim().slice(0, MAX_VERBATIM);
    verbatim = trimmed || null;
  }

  // Identity is derived, never accepted. A client cannot attach an answer to
  // another tenant.
  const token = request.cookies.get("auth_token")?.value;
  if (!token) {
    return NextResponse.json({ error: "Not authenticated" }, { status: 401 });
  }
  const claims = decodeJwtPayload(token);
  const contactRef = typeof claims.tenant_id === "string" ? claims.tenant_id : null;
  if (!contactRef) {
    return NextResponse.json({ error: "Session carries no tenant" }, { status: 401 });
  }

  const envelope = await buildSelfReportEnvelope({
    observedAt: resolveObservedAt(body.observedAt),
    capturePath: WEB_CAPTURE_PATH,
    surface: source,
    verbatim,
    contactRef,
    tier: await fetchTier(token),
  });

  const apiKey = process.env.ALLSOURCE_API_KEY;
  if (!apiKey) {
    // Loud, not silent. A GEO window with an unnoticed hole in it is worse
    // than a visible failure — and this is the single most likely
    // misconfiguration (the env var is a Fly secret).
    console.error(
      "[geo] ALLSOURCE_API_KEY is not set — geo.selfreport.captured dropped. " +
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
        "[geo] gateway rejected geo.selfreport.captured:",
        response.status,
        await response.text()
      );
      return NextResponse.json({ error: "Failed to record answer" }, { status: 502 });
    }
  } catch (error) {
    console.error("[geo] gateway unreachable for geo.selfreport.captured:", error);
    return NextResponse.json({ error: "Failed to record answer" }, { status: 502 });
  }

  // `observed_at` goes back so the follow-up (the free text) can replay it and
  // land on this same entity rather than minting a second capture.
  return NextResponse.json(
    { recorded: true, source, observed_at: envelope.payload.observed_at },
    { status: 202 }
  );
}
