"use client";

type PostHogClient = typeof import("posthog-js").posthog;

type AnalyticsValue = boolean | number | string | null | undefined;
type AnalyticsProperties = Record<string, AnalyticsValue>;

interface AnalyticsState {
  client: PostHogClient | null;
  initialization: Promise<PostHogClient | null> | null;
  initialized: boolean;
}

declare global {
  interface Window {
    __allsourceAnalytics?: AnalyticsState;
  }
}

const BET = "allsource";
const TRACKING_SCHEMA = 1;
const PRODUCTION_HOSTS = new Set(["all-source.xyz", "www.all-source.xyz"]);

function analyticsState(): AnalyticsState | null {
  if (typeof window === "undefined") return null;
  window.__allsourceAnalytics ??= {
    client: null,
    initialization: null,
    initialized: false,
  };
  return window.__allsourceAnalytics;
}

export function cleanAnalyticsUrl(value: unknown, fallbackOrigin?: string): string | undefined {
  if (typeof value !== "string" || value.length === 0) return undefined;
  try {
    const url = new URL(value, fallbackOrigin);
    return `${url.origin}${url.pathname}`;
  } catch {
    return undefined;
  }
}

function commonProperties(): AnalyticsProperties {
  const production = PRODUCTION_HOSTS.has(window.location.hostname);
  return {
    analytics_test: !production,
    bet: BET,
    page_path: window.location.pathname,
    surface: window.location.pathname.startsWith("/dashboard") ? "dashboard" : "marketing",
    tracking_schema: TRACKING_SCHEMA,
    traffic_role: production ? "production" : "test",
  };
}

export function initProductAnalytics(): Promise<PostHogClient | null> {
  const state = analyticsState();
  const key = process.env.NEXT_PUBLIC_POSTHOG_KEY?.trim();
  if (!state || !key || process.env.NODE_ENV !== "production") return Promise.resolve(null);
  if (state.initialized) return Promise.resolve(state.client);
  if (state.initialization) return state.initialization;

  state.initialization = import("posthog-js").then(({ posthog }) => {
    posthog.init(key, {
      api_host: process.env.NEXT_PUBLIC_POSTHOG_HOST ?? "https://eu.i.posthog.com",
      autocapture: false,
      before_send: (event) => {
        if (!event?.properties) return event;
        Object.assign(event.properties, commonProperties());

        for (const property of ["$current_url", "$referrer"]) {
          const value = cleanAnalyticsUrl(event.properties[property], window.location.origin);
          if (value) event.properties[property] = value;
          else delete event.properties[property];
        }
        return event;
      },
      capture_pageleave: true,
      capture_pageview: false,
      capture_performance: {
        web_vitals: true,
        web_vitals_allowed_metrics: ["LCP", "CLS", "FCP", "INP"],
        web_vitals_attribution: false,
      },
      cookieless_mode: "always",
      defaults: "2026-05-30",
      disable_session_recording: true,
      person_profiles: "never",
    });
    state.client = posthog;
    state.initialized = true;
    return posthog;
  });

  return state.initialization;
}

export function trackProductEvent(event: string, properties: AnalyticsProperties = {}): void {
  if (typeof window === "undefined") return;
  void initProductAnalytics().then((posthog) => {
    posthog?.capture(event, { ...properties, ...commonProperties() });
  });
}
