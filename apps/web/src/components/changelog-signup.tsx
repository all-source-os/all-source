"use client";

import { Button, cn, Input, Label } from "@allsource/ui";
import { Turnstile, type TurnstileInstance } from "@marsidev/react-turnstile";
import { AlertCircle, CheckCircle2, Loader2 } from "lucide-react";
import Link from "next/link";
import { useEffect, useId, useRef, useState } from "react";
import { acquisitionAttributionForUrl } from "@/lib/acquisition-attribution";
import { trackProductEvent } from "@/lib/product-analytics";

const TURNSTILE_SITE_KEY = process.env.NEXT_PUBLIC_TURNSTILE_SITE_KEY || "";

export type ChangelogPlacement = "home" | "prime";

const HEADINGS: Record<ChangelogPlacement, string> = {
  home: "Not ready for a trial?",
  prime: "Not ready to install?",
};

interface ChangelogSignupProps {
  placement: ChangelogPlacement;
  className?: string;
}

interface SubscriptionResponse {
  message?: string;
  retry_after?: number;
}

function campaignSource() {
  const attribution = acquisitionAttributionForUrl(window.location.href);
  return {
    source: attribution.campaign_source,
    medium: attribution.campaign_medium,
    campaign: attribution.campaign_name,
    content: attribution.campaign_content,
  };
}

export function ChangelogSignup({ placement, className }: ChangelogSignupProps) {
  const id = useId();
  const emailId = `${id}-email`;
  const consentId = `${id}-consent`;
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [needsTurnstile, setNeedsTurnstile] = useState(false);
  const [turnstileToken, setTurnstileToken] = useState("");
  const turnstile = useRef<TurnstileInstance>(null);
  const errorAlert = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (error) errorAlert.current?.focus();
  }, [error]);

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");
    setPending(true);
    const email = new FormData(event.currentTarget).get("email");
    trackProductEvent("changelog_signup_submitted", { placement });

    try {
      const response = await fetch("/api/changelog/subscriptions", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          email,
          consent: true,
          placement,
          campaign_source: campaignSource(),
          ...(turnstileToken && { cf_turnstile_response: turnstileToken }),
        }),
      });
      const data = (await response.json().catch(() => ({}))) as SubscriptionResponse;
      if (!response.ok) {
        trackProductEvent("changelog_signup_failed", { placement, status: response.status });
        if (response.status === 429) {
          const minutes = Math.max(1, Math.ceil((data.retry_after || 60) / 60));
          throw new Error(
            `Too many attempts. Try again in about ${minutes} minute${minutes === 1 ? "" : "s"}.`
          );
        }
        throw new Error(data.message || "Signup failed. Please try again.");
      }
      trackProductEvent("changelog_signup_succeeded", { placement });
      setConfirmation(
        data.message ||
          "You're on the list. We'll send the changelog and benchmark notes when they ship."
      );
    } catch (submissionError) {
      setError(
        submissionError instanceof Error ? submissionError.message : "Signup failed. Try again."
      );
      turnstile.current?.reset();
      setTurnstileToken("");
    } finally {
      setPending(false);
    }
  }

  return (
    <div className={cn("border border-border bg-card p-6 sm:p-8", className)}>
      <h2 className="text-xl font-semibold text-foreground">{HEADINGS[placement]}</h2>
      <p className="mt-2 max-w-2xl text-sm leading-6 text-muted-foreground">
        Get the changelog and benchmark notes by email. One message per release, no sales sequence.
      </p>

      {confirmation ? (
        <div
          className="mt-5 flex items-start gap-3 rounded-lg border border-border bg-background p-4 text-sm text-foreground"
          role="status"
        >
          <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-primary" aria-hidden="true" />
          <span>{confirmation}</span>
        </div>
      ) : (
        <form
          onSubmit={handleSubmit}
          className="mt-5"
          aria-busy={pending}
          aria-label="Changelog signup"
        >
          {error && (
            <div
              ref={errorAlert}
              className="mb-4 flex items-start gap-3 rounded-lg border border-destructive/50 bg-destructive/10 p-4 text-sm text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-destructive"
              role="alert"
              tabIndex={-1}
            >
              <AlertCircle
                className="mt-0.5 h-4 w-4 shrink-0 text-destructive"
                aria-hidden="true"
              />
              <span>{error}</span>
            </div>
          )}
          <Label htmlFor={emailId} className="font-medium text-foreground">
            Email
          </Label>
          <div className="mt-2 flex flex-col gap-3 sm:flex-row">
            <Input
              id={emailId}
              name="email"
              type="email"
              autoComplete="email"
              maxLength={254}
              required
              disabled={pending}
              aria-describedby={consentId}
              onFocus={() => setNeedsTurnstile(true)}
              className="min-h-11 sm:max-w-sm"
            />
            <Button
              type="submit"
              disabled={pending || Boolean(TURNSTILE_SITE_KEY && needsTurnstile && !turnstileToken)}
              className="min-h-11 px-6"
            >
              {pending ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" aria-hidden="true" />
                  Subscribing…
                </>
              ) : (
                "Get the changelog + benchmark notes"
              )}
            </Button>
          </div>
          <p id={consentId} className="mt-3 text-xs leading-5 text-muted-foreground">
            By subscribing you agree that AllSource may email you release notes and benchmark
            write-ups. Unsubscribe at any time. See{" "}
            <Link
              href="/privacy#changelog-emails"
              className="rounded-sm font-medium text-foreground underline underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              how we handle your email
            </Link>
            .
          </p>
          {TURNSTILE_SITE_KEY && needsTurnstile && (
            <Turnstile
              ref={turnstile}
              siteKey={TURNSTILE_SITE_KEY}
              onSuccess={setTurnstileToken}
              onExpire={() => setTurnstileToken("")}
              onError={() => setError("Spam check could not load. Refresh and try again.")}
              options={{ size: "invisible" }}
            />
          )}
        </form>
      )}
    </div>
  );
}
