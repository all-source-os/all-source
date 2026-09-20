"use client";

import { usePathname } from "next/navigation";
import { useEffect } from "react";
import { cleanAnalyticsUrl, initProductAnalytics } from "@/lib/product-analytics";

export function PostHogAnalytics(): null {
  const pathname = usePathname();

  useEffect(() => {
    void initProductAnalytics().then((posthog) => {
      if (!posthog) return;
      posthog.capture("$pageview", {
        $current_url: `${window.location.origin}${pathname}`,
        $referrer: cleanAnalyticsUrl(document.referrer, window.location.origin),
        page_path: pathname,
        page_title: document.title,
      });
    });
  }, [pathname]);

  return null;
}
