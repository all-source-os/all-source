"use client";

import { X } from "lucide-react";
import Link from "next/link";
import { useEffect, useState } from "react";

const STORAGE_KEY = "allsource-product-hunt-launch-dismissed";

export function EarlyAccessBanner() {
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    setDismissed(globalThis.localStorage?.getItem(STORAGE_KEY) === "1");
  }, []);

  if (dismissed) return null;

  const handleDismiss = () => {
    globalThis.localStorage?.setItem(STORAGE_KEY, "1");
    setDismissed(true);
  };

  return (
    <aside
      aria-label="Product Hunt launch announcement"
      className="relative isolate flex min-h-12 items-center gap-x-3 overflow-hidden bg-primary px-3 sm:px-6 sm:before:flex-1"
    >
      <p className="py-2 text-sm/6 text-primary-foreground">
        <strong className="font-semibold">AllSource is live on Product Hunt.</strong>
        <svg
          viewBox="0 0 2 2"
          aria-hidden="true"
          className="mx-2 hidden size-0.5 fill-current sm:inline"
        >
          <circle r={1} cx={1} cy={1} />
        </svg>
        <span className="hidden sm:inline">
          Durable, time-travel memory for production AI agents.
        </span>
        <Link
          href="https://www.producthunt.com/products/allsource?launch=allsource&utm_source=website&utm_medium=banner&utm_campaign=product_hunt_launch"
          className="ml-2 inline-flex min-h-12 items-center font-semibold text-primary-foreground underline underline-offset-2 hover:opacity-80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-foreground"
        >
          See launch &rarr;
        </Link>
      </p>
      <div className="flex flex-1 justify-end">
        <button
          type="button"
          onClick={handleDismiss}
          className="grid size-12 shrink-0 place-items-center rounded-md text-primary-foreground/80 hover:bg-primary-foreground/10 hover:text-primary-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-foreground"
        >
          <span className="sr-only">Dismiss</span>
          <X className="size-4" aria-hidden="true" />
        </button>
      </div>
    </aside>
  );
}
