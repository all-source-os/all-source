import { Badge, buttonVariants, cn, Section } from "@allsource/ui";
import { ArrowDown, CheckCircle2, CircleAlert, Clock3, Terminal } from "lucide-react";
import type { Metadata } from "next";
import Link from "next/link";
import { breadcrumbSchema } from "@/lib/structured-data";
import { constructMetadata } from "@/lib/utils";
import { ProofCodeBlock, ProofNextActions, RestartProofView } from "./restart-proof-actions";

const ROUTE = "/agent-memory-restart-proof";

export const metadata: Metadata = constructMetadata({
  title: "AI Agent Memory Restart Proof",
  description:
    "Run a local, repeatable proof that AI-agent memory survives a process restart and still exposes its source event history.",
  canonical: ROUTE,
});

const installCommand = "cargo install allsource-prime";

const serverCommand = `RESTART_PROOF_DIR="\${TMPDIR:-/tmp}/allsource-restart-proof"
mkdir -p "$RESTART_PROOF_DIR"
allsource-prime --mode http --port 3905 --data-dir "$RESTART_PROOF_DIR"`;

const writeCommand = `RESTART_PROOF_NODE="$(
  curl -fsS -X POST http://127.0.0.1:3905/api/v1/prime/nodes \\
    -H 'Content-Type: application/json' \\
    -d '{"type":"decision","properties":{"question":"Which queue handles checkout processing?","answer":"durable-events","source":"runbook-42"}}' \\
    | jq -r .entity_id
)"

curl -fsS -X POST http://127.0.0.1:3905/api/v1/prime/vectors \\
  -H 'Content-Type: application/json' \\
  -d "{"id":"$RESTART_PROOF_NODE","text":"Checkout processing moved to the durable-events queue because runbook-42 requires replay after restart."}"

curl -fsS -X POST http://127.0.0.1:3905/api/v1/prime/recall \\
  -H 'Content-Type: application/json' \\
  -d '{"text":"Which queue must survive a restart?","top_k":3}' | jq

curl -fsS "http://127.0.0.1:3905/api/v1/prime/nodes/$RESTART_PROOF_NODE/history" | jq`;

const verifyCommand = `curl -fsS -X POST http://127.0.0.1:3905/api/v1/prime/recall \\
  -H 'Content-Type: application/json' \\
  -d '{"text":"Which queue must survive a restart?","top_k":3}' | jq

curl -fsS "http://127.0.0.1:3905/api/v1/prime/nodes/$RESTART_PROOF_NODE/history" | jq`;

function JsonLd({ value }: { value: object }) {
  return (
    <script
      type="application/ld+json"
      // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD requires a script tag; '<' is escaped before insertion
      dangerouslySetInnerHTML={{ __html: JSON.stringify(value).replace(/</g, "\\u003c") }}
    />
  );
}

export default function AgentMemoryRestartProofPage() {
  const breadcrumb = breadcrumbSchema([
    { name: "Home", path: "/" },
    { name: "Agent memory", path: "/solutions/agent-memory" },
    { name: "Restart proof", path: ROUTE },
  ]);
  const howTo = {
    "@context": "https://schema.org",
    "@type": "HowTo",
    name: "Prove AI-agent memory survives a process restart",
    description: metadata.description,
    totalTime: "PT10M",
    tool: ["Rust 1.92 or newer", "curl", "jq"],
    step: [
      {
        "@type": "HowToStep",
        name: "Install AllSource Prime",
        text: "Install the Apache-2.0 AllSource Prime binary from crates.io.",
        url: "https://www.all-source.xyz/agent-memory-restart-proof#install",
      },
      {
        "@type": "HowToStep",
        name: "Write one decision and inspect its history",
        text: "Write one source-bearing decision, embed it, recall it, and inspect its event history.",
        url: "https://www.all-source.xyz/agent-memory-restart-proof#write",
      },
      {
        "@type": "HowToStep",
        name: "Restart and verify",
        text: "Stop the process, reopen the same data directory, then repeat recall and history queries.",
        url: "https://www.all-source.xyz/agent-memory-restart-proof#verify",
      },
    ],
  };

  return (
    <div className="overflow-hidden">
      <RestartProofView />
      <JsonLd value={breadcrumb} />
      <JsonLd value={howTo} />

      <Section className="border-b border-border py-20 sm:py-28">
        <div className="mx-auto max-w-4xl text-center">
          <Badge variant="outline" className="font-mono text-xs uppercase tracking-[0.18em]">
            <Clock3 className="mr-2 h-3.5 w-3.5" />
            10-minute local proof
          </Badge>
          <h1 className="mt-6 text-balance text-4xl font-semibold leading-[1.04] tracking-tight sm:text-6xl">
            Prove your agent memory survives a restart.
          </h1>
          <p className="mx-auto mt-6 max-w-2xl text-pretty text-lg leading-8 text-muted-foreground">
            Write one decision with a source, stop AllSource Prime, reopen the same data directory,
            then recall the decision and inspect the event that produced it. Local only. No account,
            API key, or telemetry from the Prime binary.
          </p>
          <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
            <Link href="#run-proof" className={cn(buttonVariants({ size: "lg" }), "gap-2")}>
              Run restart proof
              <ArrowDown className="h-4 w-4" />
            </Link>
            <Link
              href="/event-sourcing-for-ai-agents"
              className={cn(buttonVariants({ size: "lg", variant: "outline" }))}
            >
              Read storage model
            </Link>
          </div>
        </div>
      </Section>

      <Section id="run-proof" className="py-16 sm:py-24">
        <div className="mx-auto max-w-4xl space-y-12">
          <div className="grid gap-4 rounded-xl border bg-muted/20 p-6 sm:grid-cols-3">
            <div>
              <div className="font-mono text-xs uppercase tracking-[0.16em] text-primary">
                Pass 1
              </div>
              <p className="mt-2 text-sm text-muted-foreground">Recall returns the decision.</p>
            </div>
            <div>
              <div className="font-mono text-xs uppercase tracking-[0.16em] text-primary">
                Pass 2
              </div>
              <p className="mt-2 text-sm text-muted-foreground">
                History returns its source event.
              </p>
            </div>
            <div>
              <div className="font-mono text-xs uppercase tracking-[0.16em] text-primary">
                Pass 3
              </div>
              <p className="mt-2 text-sm text-muted-foreground">Both still pass after restart.</p>
            </div>
          </div>

          <section id="install" aria-labelledby="install-title">
            <h2 id="install-title" className="flex items-center gap-3 text-2xl font-semibold">
              <span className="font-mono text-sm text-primary">01</span>
              Install Prime
            </h2>
            <p className="mt-3 text-muted-foreground">
              Requires Rust 1.92 or newer. Prime runs as one local binary.
            </p>
            <div className="mt-4">
              <ProofCodeBlock action="install_copy" code={installCommand} label="install command" />
            </div>
          </section>

          <section aria-labelledby="server-title">
            <h2 id="server-title" className="flex items-center gap-3 text-2xl font-semibold">
              <span className="font-mono text-sm text-primary">02</span>
              Start local HTTP mode
            </h2>
            <p className="mt-3 text-muted-foreground">
              Run this in terminal A. Keep it open. Data stays under one explicit temporary
              directory so restart uses the same store.
            </p>
            <div className="mt-4">
              <ProofCodeBlock action="server_copy" code={serverCommand} label="server command" />
            </div>
          </section>

          <section id="write" aria-labelledby="write-title">
            <h2 id="write-title" className="flex items-center gap-3 text-2xl font-semibold">
              <span className="font-mono text-sm text-primary">03</span>
              Write, recall, inspect source history
            </h2>
            <p className="mt-3 text-muted-foreground">
              Run this in terminal B. It creates one decision, adds searchable text, recalls it, and
              fetches its immutable event history.
            </p>
            <div className="mt-4">
              <ProofCodeBlock action="write_copy" code={writeCommand} label="write command" />
            </div>
            <div className="mt-4 flex gap-3 rounded-lg border p-4 text-sm text-muted-foreground">
              <CheckCircle2 className="mt-0.5 h-5 w-5 shrink-0 text-primary" />
              <p>
                Before restart, recall should return a <code>decision</code> node. History should
                contain <code>prime.node.created</code> and the visible <code>runbook-42</code>
                source property.
              </p>
            </div>
          </section>

          <section id="verify" aria-labelledby="verify-title">
            <h2 id="verify-title" className="flex items-center gap-3 text-2xl font-semibold">
              <span className="font-mono text-sm text-primary">04</span>
              Restart, then run the same checks
            </h2>
            <p className="mt-3 text-muted-foreground">
              Press <kbd>Ctrl+C</kbd> in terminal A. Run step 02 again without changing
              <code> RESTART_PROOF_DIR</code>. Then run these checks in terminal B.
            </p>
            <div className="mt-4">
              <ProofCodeBlock
                action="verify_copy"
                code={verifyCommand}
                label="verification command"
              />
            </div>
            <div className="mt-4 flex gap-3 rounded-lg border border-primary/40 bg-primary/5 p-4 text-sm">
              <CheckCircle2 className="mt-0.5 h-5 w-5 shrink-0 text-primary" />
              <p>
                Proof passes when recall returns the same decision and history returns the same
                source-bearing creation event after restart. Score can vary by build and hardware;
                node identity and event history are the checks.
              </p>
            </div>
          </section>

          <section className="rounded-xl border bg-muted/20 p-6" aria-labelledby="scope-title">
            <h2 id="scope-title" className="flex items-center gap-3 text-xl font-semibold">
              <CircleAlert className="h-5 w-5 text-primary" />
              What this proves—and what it does not
            </h2>
            <div className="mt-4 grid gap-6 text-sm text-muted-foreground sm:grid-cols-2">
              <div>
                <h3 className="font-semibold text-foreground">Proves</h3>
                <ul className="mt-2 space-y-2">
                  <li>Accepted local memory survives one clean process restart.</li>
                  <li>Semantic recall can find the stored decision after recovery.</li>
                  <li>History exposes the event payload carrying its source field.</li>
                </ul>
              </div>
              <div>
                <h3 className="font-semibold text-foreground">Does not prove</h3>
                <ul className="mt-2 space-y-2">
                  <li>Your production failure mode is fixed.</li>
                  <li>Crash durability beyond your chosen fsync policy.</li>
                  <li>Hosted availability, compliance, or workload-specific latency.</li>
                </ul>
              </div>
            </div>
          </section>
        </div>
      </Section>

      <Section className="border-t border-border py-16 text-center sm:py-24">
        <div className="mx-auto max-w-3xl">
          <Terminal className="mx-auto h-8 w-8 text-primary" />
          <h2 className="mt-4 text-3xl font-semibold">Now run your real failing memory flow.</h2>
          <p className="mx-auto mt-4 max-w-2xl text-muted-foreground">
            Local proof establishes the storage behavior. Hosted AllSource lets you connect a real
            agent, keep tenant-scoped history, and test the same restart and provenance circuit
            during a 14-day trial.
          </p>
          <div className="mt-8">
            <ProofNextActions />
          </div>
          <p className="mt-5 text-xs text-muted-foreground">
            No review, testimonial, endorsement, or public mention required.
          </p>
        </div>
      </Section>
    </div>
  );
}
