"use client";

import { Button, buttonVariants, cn } from "@allsource/ui";
import { ArrowRight, Check, Copy, ExternalLink } from "lucide-react";
import Link from "next/link";
import { useEffect, useState } from "react";

type Gtag = (...args: unknown[]) => void;

declare global {
  interface Window {
    gtag?: Gtag;
  }
}

type ProofAction =
  | "install_copy"
  | "server_copy"
  | "write_copy"
  | "verify_copy"
  | "hosted_click"
  | "github_click";

function trackProofAction(action: ProofAction): void {
  window.gtag?.("event", "restart_proof_action", {
    proof_action: action,
    proof_name: "agent_memory_restart_provenance",
  });
}

export function RestartProofView() {
  useEffect(() => {
    window.gtag?.("event", "restart_proof_view", {
      proof_name: "agent_memory_restart_provenance",
    });
  }, []);

  return null;
}

export function ProofCodeBlock({
  action,
  code,
  label,
}: {
  action: Extract<ProofAction, `${string}_copy`>;
  code: string;
  label: string;
}) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    await navigator.clipboard.writeText(code);
    trackProofAction(action);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div className="relative">
      <pre className="overflow-x-auto rounded-xl border bg-muted/30 p-5 pr-24 text-xs leading-6 sm:text-sm">
        <code className="font-mono">{code}</code>
      </pre>
      <Button
        type="button"
        size="sm"
        variant="outline"
        className="absolute right-3 top-3 gap-1.5"
        onClick={copy}
        aria-label={`Copy ${label}`}
      >
        {copied ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
        {copied ? "Copied" : "Copy"}
      </Button>
    </div>
  );
}

export function ProofNextActions() {
  return (
    <div className="flex flex-col items-stretch justify-center gap-3 sm:flex-row sm:items-center">
      <Link
        href="/connect?source=restart-proof&key_name=Restart%20proof%20(Prime)"
        className={cn(buttonVariants({ size: "lg" }), "gap-2")}
        onClick={() => trackProofAction("hosted_click")}
      >
        Connect hosted memory
        <ArrowRight className="h-4 w-4" />
      </Link>
      <Link
        href="https://github.com/all-source-os/all-source"
        className={cn(buttonVariants({ size: "lg", variant: "outline" }), "gap-2")}
        onClick={() => trackProofAction("github_click")}
      >
        <ExternalLink className="h-4 w-4" />
        Inspect source
      </Link>
    </div>
  );
}
