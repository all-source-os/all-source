"use client";

/**
 * GEO layer 1 — the browser half of AI-referral attribution.
 *
 * Renders nothing. On the first page view of a session it reads
 * `document.referrer` and the landing URL, and if they look like an AI surface
 * it posts them to `/api/geo/referral`. The server re-classifies and is the
 * only side holding a credential — see that route for the trust boundary.
 *
 * ## Why a beacon at all, when PostHog is installed
 *
 * PostHog measures acquisition and product UX. It is not the durable source of
 * truth for AllSource product outcomes and does not receive raw event payloads.
 * This beacon keeps AI-referral arrival and conversion evidence in AllSource,
 * where temporal joins and provenance can be demonstrated directly.
 *
 * ## Privacy
 *
 * No cookies. `sessionStorage` only, which dies with the tab and is never sent
 * to another origin. The session id is a random UUID with no link to a person.
 * Nothing here needs a consent banner that the site does not already need.
 */

import { useEffect } from "react";
import { type ConversionKind, classifyArrival } from "@/lib/geo-referrers";

/** Marks the arrival as already reported, so a client-side navigation does not re-send it. */
const ARRIVAL_KEY = "geo.referral.arrival";
/** The session id, reused by the conversion beacon. */
const SESSION_KEY = "geo.referral.session";

interface StoredArrival {
  /** The arrival's `observed_at`, replayed by a conversion so it lands on the same Core entity. */
  observedAt: string;
  /** The landing URL as classified, replayed for the same reason. */
  url: string;
  /** The referrer as sent. */
  referrer: string;
  sessionId: string;
}

function readSessionStorage(key: string): string | null {
  try {
    return window.sessionStorage.getItem(key);
  } catch {
    // Private mode, or storage disabled. Losing an arrival is acceptable;
    // throwing inside a layout effect is not.
    return null;
  }
}

function writeSessionStorage(key: string, value: string): void {
  try {
    window.sessionStorage.setItem(key, value);
  } catch {
    /* see above */
  }
}

function sessionId(): string {
  const existing = readSessionStorage(SESSION_KEY);
  if (existing) return existing;
  // Base64url-ish alphabet only — the route rejects anything else.
  const fresh = crypto.randomUUID().replace(/-/g, "");
  writeSessionStorage(SESSION_KEY, fresh);
  return fresh;
}

function post(body: Record<string, unknown>): void {
  void fetch("/api/geo/referral", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
    keepalive: true,
  }).catch(() => {
    // Attribution is telemetry. It must never surface as a broken page.
  });
}

/**
 * Report that this session converted.
 *
 * Re-sends the stored arrival, so the server derives the *same* idempotency
 * key and Core appends version 2 to the arrival's entity with
 * `converted: true`. A session that never arrived from an AI surface is a
 * no-op, which is why this is safe to call unconditionally from a signup
 * handler.
 */
export function reportGeoConversion(kind: ConversionKind): void {
  if (typeof window === "undefined") return;
  const stored = readSessionStorage(ARRIVAL_KEY);
  if (!stored) return;
  try {
    const arrival = JSON.parse(stored) as StoredArrival;
    post({
      referrer: arrival.referrer,
      url: arrival.url,
      sessionId: arrival.sessionId,
      observedAt: arrival.observedAt,
      conversion: kind,
    });
  } catch {
    /* corrupt storage — drop it rather than break the signup flow */
  }
}

/** Mounted once in the root layout. Renders nothing. */
export function GeoReferralTracker() {
  useEffect(() => {
    if (readSessionStorage(ARRIVAL_KEY)) return;

    const referrer = document.referrer || "";
    const url = window.location.href;

    // Client-side classification decides only whether to spend a request. The
    // server re-classifies from scratch and is the authority.
    if (!classifyArrival({ referrer, url })) return;

    const arrival: StoredArrival = {
      observedAt: new Date().toISOString(),
      url,
      referrer,
      sessionId: sessionId(),
    };
    writeSessionStorage(ARRIVAL_KEY, JSON.stringify(arrival));
    post({ referrer, url, sessionId: arrival.sessionId });
  }, []);

  return null;
}
