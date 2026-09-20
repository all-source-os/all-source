"use client";

import Link from "next/link";
import type { ComponentProps } from "react";
import { trackProductEvent } from "@/lib/product-analytics";

type MarketingDestination = "api_docs" | "live_demo" | "signup";
type MarketingPlacement = "event_store_definition_footer";

interface TrackedMarketingLinkProps extends ComponentProps<typeof Link> {
  destination: MarketingDestination;
  placement: MarketingPlacement;
}

export function TrackedMarketingLink({
  destination,
  onClick,
  placement,
  ...props
}: TrackedMarketingLinkProps) {
  return (
    <Link
      {...props}
      onClick={(event) => {
        trackProductEvent("marketing_cta_clicked", { destination, placement });
        onClick?.(event);
      }}
    />
  );
}
