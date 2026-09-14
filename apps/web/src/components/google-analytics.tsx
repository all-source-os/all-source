"use client";

import { usePathname } from "next/navigation";
import Script from "next/script";
import { useEffect, useState } from "react";
import { acquisitionAttributionForUrl } from "@/lib/acquisition-attribution";

const PRODUCTION_MEASUREMENT_ID = "G-3347JLG51K";
const PRODUCTION_HOSTS = new Set(["all-source.xyz", "www.all-source.xyz"]);

type Gtag = (...args: unknown[]) => void;

declare global {
  interface Window {
    __gaConfigured?: string;
    dataLayer?: unknown[];
    gtag?: Gtag;
  }
}

function cleanReferrer() {
  if (!document.referrer) return undefined;

  try {
    const referrer = new URL(document.referrer);
    return `${referrer.origin}${referrer.pathname}`;
  } catch {
    return undefined;
  }
}

export function GoogleAnalytics() {
  const configuredMeasurementId = process.env.NEXT_PUBLIC_GA_MEASUREMENT_ID;
  const [measurementId, setMeasurementId] = useState<string>();
  const pathname = usePathname();

  useEffect(() => {
    if (configuredMeasurementId) {
      setMeasurementId(configuredMeasurementId);
      return;
    }

    if (PRODUCTION_HOSTS.has(window.location.hostname)) {
      setMeasurementId(PRODUCTION_MEASUREMENT_ID);
    }
  }, [configuredMeasurementId]);

  useEffect(() => {
    if (!measurementId) return;

    const page = {
      ...acquisitionAttributionForUrl(window.location.href),
      page_location: `${window.location.origin}${pathname}`,
      page_path: pathname,
      page_referrer: cleanReferrer(),
      page_title: document.title,
    };

    window.dataLayer ||= [];
    window.gtag ||= function (this: unknown) {
      // gtag's official wrapper must enqueue its arguments object.
      void this;
      // biome-ignore lint/complexity/noArguments: gtag requires its arguments object.
      window.dataLayer?.push(arguments);
    };

    if (window.__gaConfigured !== measurementId) {
      window.gtag("consent", "default", {
        ad_personalization: "denied",
        ad_storage: "denied",
        ad_user_data: "denied",
        analytics_storage: "denied",
      });
      window.gtag("set", "url_passthrough", false);
      window.gtag("set", "ads_data_redaction", true);
      window.gtag("js", new Date());
      window.gtag("config", measurementId, {
        allow_ad_personalization_signals: false,
        allow_google_signals: false,
        ...page,
      });
      window.__gaConfigured = measurementId;
      return;
    }

    window.gtag("event", "page_view", {
      ...page,
      send_to: measurementId,
    });
  }, [measurementId, pathname]);

  if (!measurementId) return null;

  return (
    <Script
      id="google-analytics"
      src={`https://www.googletagmanager.com/gtag/js?id=${measurementId}`}
      strategy="lazyOnload"
    />
  );
}
