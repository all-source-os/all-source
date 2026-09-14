import Link from "next/link";
import { siteConfig } from "@/lib/config";

/**
 * Server-rendered product proof. Complete event history and recalled answer
 * appear in initial HTML; homepage meaning never waits for timers or hydration.
 */

type DemoEvent = {
  type: string;
  // Pre-serialized payload so the rendered JSON is deterministic (no Date.now
  // in render → no hydration drift, and the final frame is paintable on the
  // server for the reduced-motion path).
  payload: Record<string, string | number>;
  ts: string;
};

const SCRIPTED_EVENTS: DemoEvent[] = [
  { type: "user.signed_up", payload: { user: "u_8f21", plan: "indie" }, ts: "14:58:02" },
  { type: "cart.checkout", payload: { user: "u_8f21", total: 4900 }, ts: "15:00:11" },
  { type: "agent.decided", payload: { agent: "claude", action: "upsell" }, ts: "15:00:12" },
  { type: "user.viewed", payload: { user: "u_8f21", page: "/pricing" }, ts: "15:01:44" },
  { type: "cart.checkout", payload: { user: "u_8f21", total: 1900 }, ts: "15:02:03" },
];

function EventLine({ event }: { event: DemoEvent }) {
  return (
    <div className="font-mono text-[11px] leading-relaxed sm:text-xs">
      <span className="text-muted-foreground">{event.ts}</span>{" "}
      <span className="text-primary">{`{`}</span>
      <span className="text-foreground">&quot;type&quot;</span>
      <span className="text-muted-foreground">: </span>
      <span className="text-emerald-500 dark:text-emerald-400">&quot;{event.type}&quot;</span>
      <span className="text-muted-foreground">, </span>
      <span className="text-foreground">&quot;data&quot;</span>
      <span className="text-muted-foreground">: </span>
      <span className="text-muted-foreground">{JSON.stringify(event.payload)}</span>
      <span className="text-primary">{`}`}</span>
    </div>
  );
}

export default function HeroDemo() {
  return (
    <div className="w-full max-w-xl rounded-xl border border-border bg-card/80 shadow-xl shadow-primary/5 backdrop-blur-sm">
      {/* Window chrome with the headline throughput stat baked into it */}
      <div className="flex items-center justify-between border-b border-border px-4 py-2.5">
        <div className="flex items-center gap-1.5">
          <span className="h-2.5 w-2.5 rounded-full bg-destructive/60" />
          <span className="h-2.5 w-2.5 rounded-full bg-amber-400/60" />
          <span className="h-2.5 w-2.5 rounded-full bg-emerald-400/60" />
        </div>
        <span className="font-mono text-[10px] text-muted-foreground sm:text-xs">
          AllSource Core · {siteConfig.stats[0]?.display} events/sec
        </span>
      </div>

      {/* Top pane: events streaming in */}
      <div className="border-b border-border px-4 py-3">
        <div className="mb-2 flex items-center gap-2">
          <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-emerald-400" />
          <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground sm:text-xs">
            Events streaming → WAL + Parquet
          </span>
        </div>
        <div className="min-h-[7.5rem] space-y-1 overflow-hidden">
          {SCRIPTED_EVENTS.map((event) => (
            <div key={`${event.ts}-${event.type}`}>
              <EventLine event={event} />
            </div>
          ))}
        </div>
      </div>

      {/* Bottom pane: agent recall */}
      <div className="space-y-3 px-4 py-3">
        <div className="flex items-start gap-2">
          <span className="mt-0.5 rounded bg-primary/15 px-1.5 py-0.5 text-[10px] font-semibold text-primary sm:text-xs">
            MCP · reconstruct_state
          </span>
          <p className="text-xs text-foreground sm:text-sm">
            What did the user do yesterday at 3pm?
          </p>
        </div>

        <div className="rounded-lg border border-border bg-background/60 px-3 py-2">
          <div className="space-y-1.5">
            <p className="text-xs text-foreground sm:text-sm">
              At <span className="font-mono">15:00</span> they checked out a{" "}
              <span className="font-semibold">$49</span> cart, then the agent decided to upsell.
            </p>
            <p className="flex items-center gap-1.5 font-mono text-[10px] text-emerald-500 dark:text-emerald-400 sm:text-xs">
              published Core read benchmark: {siteConfig.referenceReadLatency}
            </p>
          </div>
        </div>
        <Link
          href="/examples#capability-workbench"
          className="inline-flex font-mono text-[10px] font-semibold text-primary hover:underline sm:text-xs"
        >
          Open timeline, graph, pipeline, and projection demo →
        </Link>
      </div>
    </div>
  );
}
