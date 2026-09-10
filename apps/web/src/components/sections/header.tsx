import { buttonVariants, cn, Icons } from "@allsource/ui";
import { ChevronDown, Menu } from "lucide-react";
import Link from "next/link";
import { ThemeToggle } from "@/components/theme-toggle";
import { platformNavigationGroups, primaryNavigation, siteConfig } from "@/lib/config";

export default function Header() {
  return (
    <header className="sticky top-0 z-50 border-b border-border/80 bg-background/95 backdrop-blur">
      <div className="container mx-auto flex h-16 items-center justify-between gap-4">
        <Link
          href="/"
          title="AllSource home"
          className="flex min-h-12 shrink-0 items-center gap-2 rounded-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <Icons.logo className="h-9 w-9" aria-hidden="true" />
          <span className="leading-tight">
            <span className="block text-lg font-semibold tracking-tight">{siteConfig.name}</span>
            <span className="block font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground">
              Event Store
            </span>
          </span>
        </Link>

        <nav aria-label="Primary" className="hidden items-center gap-1 lg:flex">
          <details className="group relative">
            <summary className="inline-flex min-h-12 cursor-pointer list-none items-center gap-1 rounded-md px-3 py-2 text-sm font-medium text-muted-foreground marker:content-none transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
              Platform
              <ChevronDown
                className="h-3.5 w-3.5 transition-transform group-open:rotate-180"
                aria-hidden="true"
              />
            </summary>
            <div className="absolute left-0 top-12 w-[38rem] border border-border bg-background p-5 shadow-xl">
              <div className="grid grid-cols-2 gap-6">
                {platformNavigationGroups.map((group) => (
                  <section key={group.id} aria-labelledby={`platform-${group.id}`}>
                    <h2
                      id={`platform-${group.id}`}
                      className="px-3 font-mono text-[0.68rem] uppercase tracking-[0.18em] text-primary"
                    >
                      {group.label}
                    </h2>
                    <ul className="mt-2 grid gap-1">
                      {group.items.map((item) => (
                        <li key={item.href}>
                          <Link
                            href={item.href}
                            className="block min-h-12 rounded-md px-3 py-2.5 transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                          >
                            <span className="block text-sm font-medium text-foreground">
                              {item.label}
                            </span>
                            <span className="mt-1 block text-xs leading-5 text-muted-foreground">
                              {item.description}
                            </span>
                          </Link>
                        </li>
                      ))}
                    </ul>
                  </section>
                ))}
              </div>
            </div>
          </details>
          {primaryNavigation.map((item) => (
            <Link
              key={item.href}
              href={item.href}
              className="inline-flex min-h-12 items-center rounded-md px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              {item.label}
            </Link>
          ))}
        </nav>

        <div className="hidden items-center gap-2 lg:flex">
          <ThemeToggle />
          <Link href="/login" className={buttonVariants({ variant: "ghost" })}>
            Sign in
          </Link>
          <Link
            href="/signup"
            className={cn(buttonVariants({ variant: "default" }), "text-background")}
          >
            Start 14-day trial
          </Link>
        </div>

        <details className="group relative lg:hidden">
          <summary className="flex h-12 cursor-pointer list-none items-center gap-2 rounded-md border border-border px-3 text-sm font-medium marker:content-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
            <Menu className="h-4 w-4" aria-hidden="true" />
            Menu
          </summary>
          <div className="absolute right-0 top-12 max-h-[calc(100dvh-10rem)] w-[min(20rem,calc(100vw-2rem))] overflow-y-auto rounded-xl border border-border bg-background p-2 shadow-xl">
            <nav aria-label="Mobile primary" className="grid gap-1">
              <details className="group/platform">
                <summary className="flex min-h-12 cursor-pointer list-none items-center justify-between rounded-lg px-3 py-2.5 text-sm font-medium marker:content-none hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
                  Platform
                  <ChevronDown
                    className="h-4 w-4 transition-transform group-open/platform:rotate-180"
                    aria-hidden="true"
                  />
                </summary>
                <div className="border-l border-border pl-2">
                  {platformNavigationGroups.map((group) => (
                    <section key={group.id} className="py-2">
                      <h2 className="px-3 font-mono text-[0.68rem] uppercase tracking-[0.18em] text-primary">
                        {group.label}
                      </h2>
                      <ul className="mt-1 grid gap-1">
                        {group.items.map((item) => (
                          <li key={item.href}>
                            <Link
                              href={item.href}
                              className="flex min-h-12 items-center rounded-lg px-3 py-2.5 text-sm font-medium hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                            >
                              {item.label}
                            </Link>
                          </li>
                        ))}
                      </ul>
                    </section>
                  ))}
                </div>
              </details>
              {primaryNavigation.map((item) => (
                <Link
                  key={item.href}
                  href={item.href}
                  className="flex min-h-12 items-center rounded-lg px-3 py-2.5 text-sm font-medium hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                >
                  {item.label}
                </Link>
              ))}
            </nav>
            <div className="mt-2 grid gap-2 border-t border-border pt-2">
              <div className="flex items-center justify-between gap-3 px-1">
                <span className="text-sm text-muted-foreground">Theme</span>
                <ThemeToggle />
              </div>
              <Link href="/login" className={buttonVariants({ variant: "outline" })}>
                Sign in
              </Link>
              <Link
                href="/signup"
                className={cn(buttonVariants({ variant: "default" }), "text-background")}
              >
                Start 14-day trial
              </Link>
            </div>
          </div>
        </details>
      </div>
    </header>
  );
}
