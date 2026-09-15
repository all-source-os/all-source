"use client";

import { BlurFade, Button, buttonVariants, Card, CardContent, cn } from "@allsource/ui";
import { ArrowRight, Check, Copy, Download, ExternalLink, Sparkles, Terminal } from "lucide-react";
import Link from "next/link";
import { useState } from "react";
import {
  type ConfigBlock,
  INSTALL_BINARY_CMD,
  type Integration,
  withApiKey,
} from "@/lib/integrations";

// Shared template for every per-tool install page. The CONTENT comes from the
// integration data module (lib/integrations.ts); this file owns the LAYOUT
// only. Adding a tool = one object in the data module, no edit here.
//
// Order: the hosted MCP URL when the client supports one, then (1) install the
// binary, (2) hosted sync via --sync-to, (3) LOCAL fallback — no account.

// CopyBlock is intentionally a near-copy of the one in connect-client.tsx so
// the install pages render identically to /connect. Kept local rather than
// extracted because the two flows evolve independently.
function CopyBlock({ content, kind }: { content: string; kind: ConfigBlock["kind"] }) {
  const [copied, setCopied] = useState(false);
  const onCopy = async () => {
    await navigator.clipboard.writeText(content);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };
  return (
    <div className="relative">
      <pre className="overflow-x-auto rounded-md border bg-muted/30 p-4 text-xs">
        <code className={`language-${kind} font-mono`}>{content}</code>
      </pre>
      <Button
        type="button"
        size="sm"
        variant="outline"
        className="absolute right-2 top-2 h-7 gap-1 px-2 text-xs"
        onClick={onCopy}
      >
        {copied ? (
          <>
            <Check className="h-3 w-3" />
            Copied
          </>
        ) : (
          <>
            <Copy className="h-3 w-3" />
            Copy
          </>
        )}
      </Button>
    </div>
  );
}

function ConfigSection({ block }: { block: ConfigBlock }) {
  return (
    <div>
      <div className="mb-2 rounded-md border bg-muted/30 px-3 py-1.5 text-xs text-muted-foreground">
        <span className="font-medium text-foreground">Paste into:</span>{" "}
        <span className="font-mono">{block.label}</span>
      </div>
      <CopyBlock content={block.content} kind={block.kind} />
    </div>
  );
}

export function InstallPage({ integration }: { integration: Integration }) {
  // No real key on a static page — render a readable placeholder. The hosted
  // path sends the reader to /connect, which mints the key and renders the
  // ready-to-paste config there.
  const hostedContent = withApiKey(integration.hosted.content);
  const remote = integration.remote;

  // Tag the /connect deep-link so minted keys are attributable in
  // /dashboard/api-keys (see the deep-link contract in connect-client.tsx).
  const connectHref = `/connect?source=install-${integration.slug}&key_name=${encodeURIComponent(
    `${integration.name} (Prime)`
  )}`;

  return (
    <div className="mx-auto w-full max-w-screen-md px-4 py-24 lg:px-8">
      <BlurFade delay={0.1} inView>
        <Link
          href="/install"
          className="mb-6 inline-flex items-center gap-1.5 text-sm text-muted-foreground transition-colors hover:text-foreground"
        >
          ← All integrations
        </Link>
        <h1 className="mb-3 text-3xl font-bold tracking-tight text-foreground sm:text-4xl">
          AllSource Prime with {integration.name}
        </h1>
        <p className="text-lg text-muted-foreground">{integration.blurb}</p>
        <p className="mt-3 text-sm text-muted-foreground">
          {remote ? (
            <>
              Connect {integration.name} to hosted Prime over HTTPS with nothing to install, or run
              the
            </>
          ) : (
            <>Prime runs as a</>
          )}{" "}
          local <code className="rounded bg-muted px-1.5 py-0.5 font-mono">allsource-prime</code>{" "}
          binary over stdio. The same store serves every MCP client you wire it into — one source of
          truth, everywhere your agents work.
        </p>
      </BlurFade>

      {remote && (
        <BlurFade delay={0.12} inView>
          <section className="mt-12">
            <h2 className="mb-2 flex items-center gap-2 text-xl font-semibold text-foreground">
              <Sparkles className="h-5 w-5" />
              Hosted MCP URL — nothing to install (recommended)
            </h2>
            <p className="mb-4 text-sm text-muted-foreground">
              Mint an API key, then point {integration.name} at the hosted endpoint. Memory lives in
              your AllSource tenant and shows up live in the dashboard.
            </p>
            <Card className="mb-4 border-primary/30 bg-primary/5">
              <CardContent className="flex flex-col gap-3 py-5 sm:flex-row sm:items-center sm:justify-between">
                <div>
                  <div className="font-medium text-foreground">Mint your API key</div>
                  <p className="mt-0.5 text-sm text-muted-foreground">
                    We don&apos;t show secrets twice — paste it into the config below.
                  </p>
                </div>
                <Link href={connectHref} className={cn(buttonVariants(), "shrink-0 gap-1.5")}>
                  Get API key
                  <ArrowRight className="h-4 w-4" />
                </Link>
              </CardContent>
            </Card>
            <p className="mb-2 text-sm text-muted-foreground">
              Then paste this, swapping{" "}
              <code className="rounded bg-muted px-1.5 py-0.5 font-mono">&lt;YOUR_API_KEY&gt;</code>{" "}
              for your key:
            </p>
            <ConfigSection block={{ ...remote, content: withApiKey(remote.content) }} />
            <p className="mt-6 text-sm text-muted-foreground">
              Prefer a local binary? The steps below run Prime on your machine instead.
            </p>
          </section>
        </BlurFade>
      )}

      {/* Step 1 — install the binary (identical for every client) */}
      <BlurFade delay={0.15} inView>
        <section className="mt-12">
          <h2 className="mb-2 flex items-center gap-2 text-xl font-semibold text-foreground">
            <Terminal className="h-5 w-5" />
            1. Install the binary
          </h2>
          <p className="mb-3 text-sm text-muted-foreground">
            From crates.io. Builds standalone — no AllSource server required, just a Rust toolchain.
          </p>
          <CopyBlock content={INSTALL_BINARY_CMD} kind="bash" />
        </section>
      </BlurFade>

      {/* Step 2 — HOSTED first */}
      <BlurFade delay={0.2} inView>
        <section className="mt-12">
          <h2 className="mb-2 flex items-center gap-2 text-xl font-semibold text-foreground">
            <Sparkles className="h-5 w-5" />
            2. Hosted memory{remote ? " via the local binary" : " (recommended)"}
          </h2>
          <p className="mb-4 text-sm text-muted-foreground">
            Mint an API key, then run Prime with{" "}
            <code className="rounded bg-muted px-1.5 py-0.5 font-mono">--sync-to</code>. Your memory
            persists to your AllSource tenant and shows up live in the dashboard — and the same key
            works from any other client you connect.
          </p>

          <Card className="mb-4 border-primary/30 bg-primary/5">
            <CardContent className="flex flex-col gap-3 py-5 sm:flex-row sm:items-center sm:justify-between">
              <div>
                <div className="font-medium text-foreground">Mint your API key</div>
                <p className="mt-0.5 text-sm text-muted-foreground">
                  One click mints a Prime-scoped key. We don&apos;t show secrets twice — paste it
                  into the config below.
                </p>
              </div>
              <Link href={connectHref} className={cn(buttonVariants(), "shrink-0 gap-1.5")}>
                Get API key
                <ArrowRight className="h-4 w-4" />
              </Link>
            </CardContent>
          </Card>

          <p className="mb-2 text-sm text-muted-foreground">
            Then paste this, swapping{" "}
            <code className="rounded bg-muted px-1.5 py-0.5 font-mono">&lt;YOUR_API_KEY&gt;</code>{" "}
            for the key you just minted:
          </p>
          <ConfigSection block={{ ...integration.hosted, content: hostedContent }} />
        </section>
      </BlurFade>

      {/* Step 3 — LOCAL fallback */}
      <BlurFade delay={0.25} inView>
        <section className="mt-12">
          <h2 className="mb-2 flex items-center gap-2 text-xl font-semibold text-foreground">
            <Download className="h-5 w-5" />
            3. Local-only alternative (no account)
          </h2>
          <p className="mb-3 text-sm text-muted-foreground">
            Skip the account entirely. Drop the{" "}
            <code className="rounded bg-muted px-1.5 py-0.5 font-mono">--sync-to</code> /{" "}
            <code className="rounded bg-muted px-1.5 py-0.5 font-mono">--api-key</code> flags and
            memory stays on disk at{" "}
            <code className="rounded bg-muted px-1.5 py-0.5 font-mono">~/.prime/memory</code>.
            Nothing leaves your machine.
          </p>
          <ConfigSection block={integration.local} />
        </section>
      </BlurFade>

      {/* Optional — agent-assisted setup */}
      {integration.agentPrompt && (
        <BlurFade delay={0.3} inView>
          <section className="mt-12">
            <h2 className="mb-2 text-xl font-semibold text-foreground">Or let the agent do it</h2>
            <p className="mb-3 text-sm text-muted-foreground">
              {integration.name} can edit its own config. Paste this prompt and let it wire Prime
              up:
            </p>
            <CopyBlock content={integration.agentPrompt} kind="bash" />
          </section>
        </BlurFade>
      )}

      {/* Notes / caveats */}
      {integration.notes.length > 0 && (
        <BlurFade delay={0.35} inView>
          <section className="mt-12">
            <h2 className="mb-3 text-xl font-semibold text-foreground">Notes</h2>
            {!integration.verified && (
              <div className="mb-3 rounded-md border border-yellow-500/40 bg-yellow-500/5 px-3 py-2 text-sm text-yellow-700 dark:text-yellow-400">
                Heads up: we couldn&apos;t fully verify {integration.name}&apos;s MCP config path
                against current vendor docs — it varies across versions. Double-check it in the
                client&apos;s settings, and read the notes below.
              </div>
            )}
            <ul className="ml-5 list-disc space-y-1.5 text-sm text-muted-foreground">
              {integration.notes.map((note) => (
                <li key={note}>{note}</li>
              ))}
            </ul>
          </section>
        </BlurFade>
      )}

      {/* After install */}
      <BlurFade delay={0.4} inView>
        <section className="mt-12">
          <h2 className="mb-2 text-xl font-semibold text-foreground">After install</h2>
          <ul className="ml-5 list-disc space-y-1.5 text-sm text-muted-foreground">
            <li>Restart {integration.name} so it picks up the new MCP server.</li>
            <li>
              Verify by asking it: <em>&quot;List the MCP tools you have available.&quot;</em> You
              should see <code className="font-mono text-xs">prime_add_node</code>,{" "}
              <code className="font-mono text-xs">prime_recall</code>, and friends.
            </li>
            <li>
              On the hosted path, watch nodes appear live at{" "}
              <Link className="underline" href="/dashboard/memory">
                /dashboard/memory
              </Link>
              .
            </li>
          </ul>
        </section>
      </BlurFade>

      <BlurFade delay={0.45} inView>
        <div className="mt-12 flex flex-wrap gap-2">
          <Link href="/install" className={cn(buttonVariants({ variant: "outline" }), "gap-1.5")}>
            ← Other integrations
          </Link>
          <Link
            href="/docs/prime/mcp"
            className={cn(buttonVariants({ variant: "ghost" }), "gap-1.5")}
          >
            MCP setup docs
            <ExternalLink className="h-3.5 w-3.5" />
          </Link>
        </div>
      </BlurFade>
    </div>
  );
}
