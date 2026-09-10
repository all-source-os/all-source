import { buttonVariants, cn, Icons } from "@allsource/ui";
import Link from "next/link";
import HeroDemo from "@/components/sections/hero-demo";
import { indiePrice as defaultIndiePrice, siteConfig } from "@/lib/config";

function HeroPill() {
  return (
    <Link
      href="/what-is-allsource"
      className="flex min-h-12 w-fit items-center gap-2 rounded-full border border-border bg-card px-4 py-2 text-xs font-medium text-foreground transition-colors hover:border-primary/50 hover:text-primary sm:text-sm"
    >
      <span className="h-2 w-2 rounded-full bg-primary" aria-hidden="true" />
      Purpose-built event store database
      <span aria-hidden="true">→</span>
    </Link>
  );
}

function HeroTitles() {
  return (
    <div className="flex w-full flex-col gap-5 pt-8">
      <h1 className="text-balance text-4xl font-semibold leading-tight text-foreground sm:text-5xl lg:text-left lg:text-6xl">
        Event store database built for event sourcing.
      </h1>
      <p className="max-w-2xl text-balance text-lg leading-8 text-muted-foreground sm:text-xl lg:text-left">
        AllSource records application state as ordered, immutable event streams. Rebuild
        projections, replay production history, query point-in-time state, and expose same durable
        record to AI agents through Prime.
      </p>
    </div>
  );
}

function HeroCTA({ indiePrice }: { indiePrice: string }) {
  return (
    <>
      <div className="mt-8 flex w-full flex-col items-stretch gap-3 sm:flex-row sm:items-center lg:justify-start">
        <Link
          href="/signup"
          className={cn(
            buttonVariants({ variant: "default" }),
            "min-h-12 w-full gap-2 px-8 text-background sm:w-auto"
          )}
        >
          <Icons.logo className="h-5 w-5" />
          Start event store trial
        </Link>
        <Link
          href={siteConfig.links.github}
          className={cn(
            buttonVariants({ variant: "outline" }),
            "min-h-12 w-full gap-2 px-8 sm:w-auto"
          )}
        >
          <Icons.github className="h-5 w-5" />
          Self-host on GitHub
        </Link>
        <Link
          href="/event-sourcing/patterns"
          className={cn(
            buttonVariants({ variant: "ghost" }),
            "min-h-12 w-full gap-2 px-5 sm:w-auto"
          )}
        >
          Explore patterns →
        </Link>
      </div>
      <p className="mt-4 text-sm text-muted-foreground">
        Hosted plans from {indiePrice}/month after trial · Apache-2.0 Core available to self-host
      </p>
    </>
  );
}

export default function Hero({ indiePrice = defaultIndiePrice }: { indiePrice?: string }) {
  return (
    <section id="hero">
      <div className="mx-auto flex w-full max-w-7xl flex-col items-center gap-12 px-4 pt-16 sm:px-6 sm:pt-20 lg:flex-row lg:items-center lg:justify-between lg:gap-16 lg:px-8 lg:pt-24">
        <div className="flex w-full flex-col items-center text-center lg:w-1/2 lg:items-start lg:text-left">
          <HeroPill />
          <HeroTitles />
          <HeroCTA indiePrice={indiePrice} />
        </div>

        <div className="flex w-full justify-center lg:w-1/2 lg:justify-end">
          <HeroDemo />
        </div>
      </div>
    </section>
  );
}
