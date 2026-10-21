"use client";

import { Button } from "@allsource/ui";
import { AlertCircle, CheckCircle2, Loader2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";

interface ChangelogUnsubscribeProps {
  subscriberId: string;
  token: string;
}

export function ChangelogUnsubscribe({ subscriberId, token }: ChangelogUnsubscribeProps) {
  const [pending, setPending] = useState(false);
  const [done, setDone] = useState("");
  const [error, setError] = useState("");
  const errorAlert = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (error) errorAlert.current?.focus();
  }, [error]);

  async function unsubscribe() {
    setError("");
    setPending(true);
    try {
      const response = await fetch("/api/changelog/unsubscribe", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ id: subscriberId, token }),
      });
      const body = (await response.json().catch(() => ({}))) as { message?: string };
      if (response.ok) {
        setDone(body.message || "You're unsubscribed. We won't email you again.");
      } else {
        setError(body.message || "We couldn't record that right now. Please try again shortly.");
      }
    } catch {
      setError("We couldn't reach the server. Check your connection and try again.");
    } finally {
      setPending(false);
    }
  }

  if (done) {
    return (
      <p role="status" className="flex items-center gap-2 text-foreground">
        <CheckCircle2 className="h-4 w-4 text-green-600" aria-hidden="true" />
        {done}
      </p>
    );
  }

  return (
    <div className="space-y-4">
      <Button onClick={unsubscribe} disabled={pending} aria-busy={pending}>
        {pending && <Loader2 className="mr-2 h-4 w-4 animate-spin" aria-hidden="true" />}
        {pending ? "Unsubscribing…" : "Unsubscribe"}
      </Button>
      {error && (
        <div
          ref={errorAlert}
          role="alert"
          tabIndex={-1}
          className="flex items-center gap-2 text-sm text-destructive"
        >
          <AlertCircle className="h-4 w-4" aria-hidden="true" />
          {error}
        </div>
      )}
    </div>
  );
}
