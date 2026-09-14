import { Badge, buttonVariants, Card, CardContent, cn } from "@allsource/ui";
import { ArrowRight } from "lucide-react";
import Link from "next/link";
import { FaGithub } from "react-icons/fa";
import { FadeIn } from "@/components/ui/fade-in";
import { siteConfig } from "@/lib/config";
import { constructMetadata } from "@/lib/utils";

export const metadata = constructMetadata({
  title: "SDKs",
  description: `Official ${siteConfig.name} client SDKs in Rust, TypeScript, Python, and Go.`,
  canonical: "/sdks",
});

const sdks = [
  {
    lang: "Rust",
    crate: "allsource",
    version: "0.20.1",
    install: "cargo add allsource",
    badge: "crates.io",
    badgeHref: "https://crates.io/crates/allsource",
    repoHref: "https://github.com/all-source-os/all-source/tree/main/sdks/rust",
    blurb:
      "First-class client. Async via tokio, both an HTTP `QueryClient` for the gateway and a fast in-process `EventStore` for embedded mode. The SDK ships from the same monorepo as Core, so it's always paired with a known-good release.",
  },
  {
    lang: "TypeScript",
    crate: "@allsourcedev/client",
    version: "0.23.1",
    install: "bun add @allsourcedev/client",
    badge: "npm",
    badgeHref: "https://www.npmjs.com/package/@allsourcedev/client",
    repoHref: "https://github.com/all-source-os/all-source/tree/main/sdks/typescript",
    blurb:
      "Type-safe client for the public gateway. WebSocket streaming, projection subscriptions, and full TypeScript types for every event in your tenant. Published on npm; ships compiled ESM + CJS with .d.ts types.",
  },
  {
    lang: "Python",
    crate: "allsource-client",
    version: "0.20.1",
    install:
      "uv pip install git+https://github.com/all-source-os/all-source#subdirectory=sdks/python-client",
    badge: "GitHub",
    badgeHref: "https://github.com/all-source-os/all-source/tree/main/sdks/python-client",
    repoHref: "https://github.com/all-source-os/all-source/tree/main/sdks/python-client",
    blurb:
      "Synchronous and asynchronous clients (httpx-based). Pydantic models for events and projections. Distributed via the GitHub registry rather than PyPI so it stays version-locked with the gateway you're connecting to.",
  },
  {
    lang: "Go",
    crate: "github.com/all-source-os/all-source/sdks/go",
    version: "0.20.1",
    install: "go get github.com/all-source-os/all-source/sdks/go",
    badge: "GitHub",
    badgeHref: "https://github.com/all-source-os/all-source/tree/main/sdks/go",
    repoHref: "https://github.com/all-source-os/all-source/tree/main/sdks/go",
    blurb:
      "Idiomatic Go client. Standard library `http.Client` under the hood, context-aware everywhere, zero exotic dependencies. Pulled directly from the source repo via Go modules.",
  },
];

export default function SDKsPage() {
  return (
    <div className="mx-auto w-full max-w-screen-md px-4 lg:px-8 py-24">
      <FadeIn delay={0.1} inView>
        <h1 className="text-3xl font-bold text-foreground sm:text-4xl mb-2">AllSource SDKs</h1>
        <p className="text-lg text-muted-foreground">
          Official client SDKs in four languages. All ship from the AllSource monorepo and track the
          same version as the gateway and Core.
        </p>
      </FadeIn>

      <div className="space-y-4 mt-10">
        {sdks.map((sdk, i) => (
          <FadeIn key={sdk.lang} delay={0.15 + i * 0.05} inView>
            <Card>
              <CardContent className="pt-6">
                <div className="flex items-baseline justify-between mb-3">
                  <div className="flex items-center gap-3">
                    <h2 className="text-xl font-semibold text-foreground">{sdk.lang}</h2>
                    <Badge variant="outline" className="text-xs font-mono">
                      v{sdk.version}
                    </Badge>
                  </div>
                  <Link
                    href={sdk.badgeHref}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-sm text-muted-foreground hover:text-foreground transition-colors"
                  >
                    {sdk.badge} ↗
                  </Link>
                </div>
                <p className="text-sm text-muted-foreground mb-4">{sdk.blurb}</p>
                <div className="rounded-md border bg-muted/30 px-3 py-2 mb-3">
                  <code className="text-xs font-mono text-foreground">{sdk.install}</code>
                </div>
                <Link
                  href={sdk.repoHref}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground transition-colors"
                >
                  <FaGithub className="h-3.5 w-3.5" /> View source
                </Link>
              </CardContent>
            </Card>
          </FadeIn>
        ))}
      </div>

      <FadeIn delay={0.5} inView>
        <Card className="mt-8 border-dashed">
          <CardContent className="pt-6">
            <h3 className="font-semibold text-foreground mb-2">Why some SDKs aren't on npm/PyPI</h3>
            <p className="text-sm text-muted-foreground">
              Only the Rust SDK is published to crates.io. The TypeScript, Python, and Go SDKs are
              distributed via GitHub directly — that keeps each install version-locked to a specific
              gateway release, avoids a long tail of package-registry yanks during early
              development, and means every consumer is, by construction, looking at the canonical
              source. We may publish to npm/PyPI/pkg.go.dev once the gateway API surface freezes.
            </p>
          </CardContent>
        </Card>
      </FadeIn>

      <FadeIn delay={0.55} inView>
        <div className="mt-10 flex flex-wrap gap-3">
          <Link href="/docs/api" className={cn(buttonVariants({ variant: "default" }))}>
            API Reference <ArrowRight className="ml-2 h-4 w-4" />
          </Link>
          <Link href="/examples" className={cn(buttonVariants({ variant: "outline" }))}>
            Examples
          </Link>
          <Link href="/changelog" className={cn(buttonVariants({ variant: "ghost" }))}>
            Changelog
          </Link>
        </div>
      </FadeIn>
    </div>
  );
}
