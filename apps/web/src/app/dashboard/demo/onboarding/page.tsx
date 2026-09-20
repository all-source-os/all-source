"use client";

import { Button, Card, CardContent } from "@allsource/ui";
import { cn } from "@allsource/ui/utils";
import { ArrowLeft, ArrowRight, Check, Code2, Download, Play, Search } from "lucide-react";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { useCallback, useState } from "react";
import { trackProductEvent } from "@/lib/product-analytics";

const STEPS = [
  { id: "choose-sdk", label: "Choose SDK", icon: Code2 },
  { id: "install", label: "Install", icon: Download },
  { id: "send-event", label: "Send First Event", icon: Play },
  { id: "query-back", label: "Query It Back", icon: Search },
] as const;

type SDK = "rust" | "go" | "typescript" | "python";

const SDK_OPTIONS: { id: SDK; label: string; mark: string }[] = [
  { id: "rust", label: "Rust", mark: "Rs" },
  { id: "go", label: "Go", mark: "Go" },
  { id: "typescript", label: "TypeScript", mark: "TS" },
  { id: "python", label: "Python", mark: "Py" },
];

const INSTALL_COMMANDS: Record<SDK, { cmd: string; registry?: string }> = {
  rust: {
    cmd: 'cargo add allsource\n\n# Or add to Cargo.toml:\n[dependencies]\nallsource = "0.14"',
  },
  go: {
    cmd: "go get github.com/all-source-os/allsource-go@latest",
  },
  typescript: {
    cmd: "bun add @allsourcedev/client\n# or\nnpm install @allsourcedev/client",
  },
  python: {
    cmd: "pip install allsource-client",
  },
};

const SEND_EVENT_SNIPPETS: Record<SDK, string> = {
  rust: `use allsource::{CoreClient, IngestEventInput};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), allsource::Error> {
    let client = CoreClient::new(
        "https://api.all-source.xyz",
        "your-api-key",
    )?;

    let resp = client.ingest_event(IngestEventInput {
        event_type: "user.signup".into(),
        entity_id: "user-001".into(),
        payload: json!({
            "email": "dev@example.com",
            "plan": "pro"
        }),
    }).await?;

    println!("Created event: {}", resp.event_id);
    Ok(())
}`,
  go: `package main

import (
    "context"
    "fmt"
    allsource "github.com/all-source-os/allsource-go"
)

func main() {
    client := allsource.New("your-api-key", "https://api.all-source.xyz")

    resp, err := client.Ingest(context.Background(),
        "user.signup", "user-001",
        map[string]any{
            "email": "dev@example.com",
            "plan":  "pro",
        },
    )
    if err != nil {
        panic(err)
    }
    fmt.Printf("Created event: %s\\n", resp.EventID)
}`,
  typescript: `import { AllSourceClient } from "@allsourcedev/client";

const client = new AllSourceClient({
  baseUrl: "https://api.all-source.xyz",
  apiKey: "your-api-key",
});

const resp = await client.ingestEvent({
  event_type: "user.signup",
  entity_id: "user-001",
  payload: {
    email: "dev@example.com",
    plan: "pro",
  },
});

console.log("Created event:", resp.event_id);`,
  python: `from allsource_client import AllSourceClient

client = AllSourceClient(
    api_key="your-api-key",
    base_url="https://api.all-source.xyz",
)

event = client.ingest(
    event_type="user.signup",
    entity_id="user-001",
    data={
        "email": "dev@example.com",
        "plan": "pro",
    },
)

print(f"Created event: {event.id}")`,
};

const QUERY_SNIPPETS: Record<SDK, string> = {
  rust: `use allsource::{QueryClient, QueryEventsParams};

let qs = QueryClient::new(
    "https://api.all-source.xyz",
    "your-api-key",
)?;

let result = qs.query_events(
    QueryEventsParams::new()
        .event_type("user.signup")
        .entity_id("user-001")
        .limit(10),
).await?;

for event in &result.events {
    println!("{}: {}", event.event_type, event.id);
}`,
  go: `result, err := client.Query(context.Background(),
    allsource.QueryOptions{
        EventType: "user.signup",
        EntityID:  "user-001",
        Limit:     10,
    },
)
if err != nil {
    panic(err)
}
for _, e := range result.Events {
    fmt.Printf("%s: %s\\n", e.EventType, e.ID)
}`,
  typescript: `const result = await client.queryEvents({
  event_type: "user.signup",
  entity_id: "user-001",
  limit: 10,
});

for (const event of result.events) {
  console.log(\`\${event.event_type}: \${event.id}\`);
}`,
  python: `result = client.query(
    event_type="user.signup",
    entity_id="user-001",
    limit=10,
)

for event in result.events:
    print(f"{event.event_type}: {event.id}")`,
};

function getLanguageLabel(sdk: SDK): string {
  return SDK_OPTIONS.find((s) => s.id === sdk)?.label ?? sdk;
}

export function OnboardingWizard({
  basePath = "/dashboard/demo/onboarding",
}: {
  basePath?: string;
}) {
  const router = useRouter();
  const searchParams = useSearchParams();

  const stepParam = parseInt(searchParams.get("step") ?? "1", 10);
  const currentStep = Math.max(1, Math.min(4, Number.isNaN(stepParam) ? 1 : stepParam));
  const stepIndex = currentStep - 1;

  const sdkParam = searchParams.get("sdk") as SDK | null;
  const selectedSdk: SDK | null =
    sdkParam && SDK_OPTIONS.some((s) => s.id === sdkParam) ? sdkParam : null;

  const [createdEventId, setCreatedEventId] = useState<string | null>(null);
  const [queryVerified, setQueryVerified] = useState(false);

  const setParams = useCallback(
    (updates: Record<string, string>) => {
      const params = new URLSearchParams(searchParams.toString());
      for (const [key, value] of Object.entries(updates)) {
        params.set(key, value);
      }
      router.push(`${basePath}?${params.toString()}`);
    },
    [basePath, router, searchParams]
  );

  const goToStep = useCallback((step: number) => setParams({ step: String(step) }), [setParams]);

  const handleSelectSdk = useCallback(
    (sdk: SDK) => {
      setCreatedEventId(null);
      setQueryVerified(false);
      trackProductEvent("onboarding_sdk_selected", { sdk, path: basePath });
      setParams({ sdk, step: "2" });
    },
    [basePath, setParams]
  );

  const handleBack = useCallback(() => {
    if (currentStep > 1) goToStep(currentStep - 1);
  }, [currentStep, goToStep]);

  const handleNext = useCallback(() => {
    if (currentStep < 4) goToStep(currentStep + 1);
  }, [currentStep, goToStep]);

  return (
    <section className="mx-auto max-w-3xl space-y-8" aria-labelledby="onboarding-title">
      <div className="text-center">
        <p className="text-xs font-semibold uppercase tracking-[0.18em] text-primary">
          First event
        </p>
        <h1 id="onboarding-title" className="mt-2 text-3xl font-bold tracking-tight">
          Connect AllSource to your stack
        </h1>
        <p className="mx-auto mt-2 max-w-xl text-muted-foreground">
          Install one SDK, write a real event to this tenant, then query it back.
        </p>
      </div>
      {/* Step indicator */}
      <div data-testid="step-indicator" className="flex items-center justify-center gap-2">
        {STEPS.map((step, index) => {
          const StepIcon = step.icon;
          const isActive = index === stepIndex;
          const isCompleted =
            (index === 0 && !!selectedSdk) ||
            (index === 1 && currentStep > 2) ||
            (index === 2 && !!createdEventId) ||
            (index === 3 && queryVerified);
          const isAvailable =
            index === 0 ||
            (index <= 2 && !!selectedSdk) ||
            (index === 3 && !!createdEventId) ||
            isActive;
          return (
            <div key={step.id} className="flex items-center gap-2">
              <button
                type="button"
                onClick={() => goToStep(index + 1)}
                disabled={!isAvailable}
                className={cn(
                  "flex items-center gap-2 rounded-full px-3 py-1.5 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-60",
                  isActive
                    ? "bg-primary text-primary-foreground"
                    : isCompleted
                      ? "bg-green-500/20 text-green-500"
                      : "bg-muted text-muted-foreground"
                )}
                aria-label={`Step ${index + 1}: ${step.label}`}
                aria-current={isActive ? "step" : undefined}
              >
                {isCompleted ? <Check className="h-4 w-4" /> : <StepIcon className="h-4 w-4" />}
                <span className="hidden sm:inline">{step.label}</span>
                <span className="sm:hidden">{index + 1}</span>
              </button>
              {index < STEPS.length - 1 && (
                <div className={cn("h-0.5 w-6", isCompleted ? "bg-green-500" : "bg-muted")} />
              )}
            </div>
          );
        })}
      </div>

      {/* Mobile step label */}
      <p className="text-center text-sm text-muted-foreground sm:hidden">
        Step {currentStep} of {STEPS.length} — {STEPS[stepIndex]?.label}
      </p>

      {/* Step content */}
      <div
        key={currentStep}
        className="animate-in fade-in-50 slide-in-from-right-4 duration-300"
        data-testid="step-content"
      >
        {currentStep === 1 && <StepChooseSdk selected={selectedSdk} onSelect={handleSelectSdk} />}
        {currentStep === 2 && selectedSdk && <StepInstall sdk={selectedSdk} />}
        {currentStep === 2 && !selectedSdk && (
          <Card>
            <CardContent className="py-12 text-center">
              <p className="text-muted-foreground">
                Please{" "}
                <button
                  type="button"
                  onClick={() => goToStep(1)}
                  className="text-primary underline"
                >
                  choose an SDK
                </button>{" "}
                first.
              </p>
            </CardContent>
          </Card>
        )}
        {currentStep === 3 && selectedSdk && (
          <StepSendEvent
            sdk={selectedSdk}
            createdEventId={createdEventId}
            onEventCreated={setCreatedEventId}
          />
        )}
        {currentStep === 3 && !selectedSdk && (
          <Card>
            <CardContent className="py-12 text-center">
              <p className="text-muted-foreground">
                Please{" "}
                <button
                  type="button"
                  onClick={() => goToStep(1)}
                  className="text-primary underline"
                >
                  choose an SDK
                </button>{" "}
                first.
              </p>
            </CardContent>
          </Card>
        )}
        {currentStep === 4 && selectedSdk && (
          <StepQueryBack sdk={selectedSdk} onVerificationChange={setQueryVerified} />
        )}
        {currentStep === 4 && !selectedSdk && (
          <Card>
            <CardContent className="py-12 text-center">
              <p className="text-muted-foreground">
                Please{" "}
                <button
                  type="button"
                  onClick={() => goToStep(1)}
                  className="text-primary underline"
                >
                  choose an SDK
                </button>{" "}
                first.
              </p>
            </CardContent>
          </Card>
        )}
      </div>

      {/* Navigation buttons */}
      <div className="flex items-center justify-between" data-testid="step-navigation">
        <Button
          variant="outline"
          onClick={handleBack}
          disabled={currentStep === 1}
          data-testid="back-button"
        >
          <ArrowLeft className="mr-2 h-4 w-4" />
          Back
        </Button>

        {currentStep < 4 ? (
          <Button
            onClick={handleNext}
            disabled={(currentStep === 1 && !selectedSdk) || (currentStep === 3 && !createdEventId)}
            data-testid="next-button"
          >
            Next
            <ArrowRight className="ml-2 h-4 w-4" />
          </Button>
        ) : (
          <Button
            type="button"
            disabled={!queryVerified}
            data-testid="go-to-dashboard-button"
            onClick={() => {
              trackProductEvent("onboarding_completed", {
                sdk: selectedSdk ?? "unknown",
                path: basePath,
              });
              router.push("/dashboard");
            }}
          >
            Go to Dashboard
          </Button>
        )}
      </div>
    </section>
  );
}

export default function OnboardingWizardPage() {
  return <OnboardingWizard />;
}

function StepChooseSdk({
  selected,
  onSelect,
}: {
  selected: SDK | null;
  onSelect: (sdk: SDK) => void;
}) {
  return (
    <div className="space-y-6 text-center">
      <div>
        <h2 className="text-2xl font-bold">Choose Your SDK</h2>
        <p className="mt-2 text-muted-foreground">Pick the client used by your application.</p>
      </div>
      <div className="grid grid-cols-2 gap-4 sm:grid-cols-4" data-testid="sdk-selector">
        {SDK_OPTIONS.map((sdk) => (
          <button
            type="button"
            key={sdk.id}
            onClick={() => onSelect(sdk.id)}
            className={cn(
              "flex flex-col items-center gap-3 rounded-xl border-2 p-6 transition-all hover:border-primary/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
              selected === sdk.id ? "border-primary bg-primary/5" : "border-border"
            )}
            data-testid={`sdk-option-${sdk.id}`}
            aria-label={`Select ${sdk.label} SDK`}
          >
            <span className="flex h-12 w-12 items-center justify-center rounded-lg border border-border bg-muted font-mono text-sm font-semibold text-foreground">
              {sdk.mark}
            </span>
            <span className="font-medium">{sdk.label}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

function CodeBlock({ code, language }: { code: string; language: string }) {
  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(code);
  }, [code]);

  return (
    <div className="relative">
      <div className="flex items-center justify-between rounded-t-lg border border-b-0 border-border bg-muted/50 px-4 py-2 text-xs text-muted-foreground">
        <span>{language}</span>
        <button
          type="button"
          onClick={handleCopy}
          className="rounded px-2 py-1 transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          data-testid="copy-button"
          aria-label={`Copy ${language} code to clipboard`}
        >
          Copy
        </button>
      </div>
      <pre className="overflow-x-auto rounded-b-lg border border-border bg-muted/30 p-4 text-sm">
        <code>{code}</code>
      </pre>
    </div>
  );
}

function StepInstall({ sdk }: { sdk: SDK }) {
  const config = INSTALL_COMMANDS[sdk];
  return (
    <div className="space-y-6">
      <div className="text-center">
        <h2 className="text-2xl font-bold">Install the {getLanguageLabel(sdk)} SDK</h2>
        <p className="mt-2 text-muted-foreground">
          Add the client to your project. Create an API key before using the snippet.
        </p>
      </div>
      <div className="flex items-center justify-between gap-4 rounded-lg border border-primary/20 bg-primary/5 p-4 text-sm">
        <span className="text-muted-foreground">Hosted SDK calls require an API key.</span>
        <Button asChild size="sm" variant="outline">
          <Link href="/dashboard/api-keys?action=create" target="_blank" rel="noopener noreferrer">
            Create API key
          </Link>
        </Button>
      </div>
      <div className="space-y-4" data-testid="install-commands">
        <CodeBlock code={config.cmd} language={getLanguageLabel(sdk)} />
        {config.registry && (
          <div>
            <p className="mb-2 text-sm text-muted-foreground">Registry configuration:</p>
            <CodeBlock code={config.registry} language="config" />
          </div>
        )}
      </div>
    </div>
  );
}

function StepSendEvent({
  sdk,
  createdEventId,
  onEventCreated,
}: {
  sdk: SDK;
  createdEventId: string | null;
  onEventCreated: (id: string) => void;
}) {
  const [isRunning, setIsRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleRunIt = useCallback(async () => {
    setIsRunning(true);
    setError(null);
    try {
      const res = await fetch("/api/v1/events", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify({
          event_type: "user.signup",
          entity_id: "user-001",
          payload: {
            email: "dev@example.com",
            plan: "pro",
            source: "onboarding-wizard",
          },
        }),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = await res.json();
      const id = data.id ?? data.event_id ?? "evt-demo";
      trackProductEvent("onboarding_event_created", { sdk });
      onEventCreated(id);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to send event");
    } finally {
      setIsRunning(false);
    }
  }, [onEventCreated, sdk]);

  return (
    <div className="space-y-6">
      <div className="text-center">
        <h2 className="text-2xl font-bold">Send Your First Event</h2>
        <p className="mt-2 text-muted-foreground">
          Review the {getLanguageLabel(sdk)} request, then use your signed-in session to write the
          same event to this tenant.
        </p>
      </div>
      <div data-testid="send-event-snippet">
        <CodeBlock code={SEND_EVENT_SNIPPETS[sdk]} language={getLanguageLabel(sdk)} />
      </div>
      <div className="flex flex-col items-center gap-3">
        <Button
          onClick={handleRunIt}
          disabled={isRunning || !!createdEventId}
          data-testid="run-it-button"
          className="gap-2"
        >
          <Play className="h-4 w-4" />
          {isRunning ? "Sending..." : "Run It"}
        </Button>
        {createdEventId && (
          <div
            className="flex items-center gap-2 rounded-lg border border-green-500/30 bg-green-500/10 px-4 py-3 text-sm"
            data-testid="event-created-feedback"
          >
            <Check className="h-5 w-5 text-green-500" />
            <span>
              Event stored. ID:{" "}
              <code className="font-mono text-green-600 dark:text-green-400">{createdEventId}</code>
            </span>
          </div>
        )}
        {error && (
          <p className="text-sm text-red-500" data-testid="send-event-error">
            {error}
          </p>
        )}
      </div>
    </div>
  );
}

function StepQueryBack({
  sdk,
  onVerificationChange,
}: {
  sdk: SDK;
  onVerificationChange: (verified: boolean) => void;
}) {
  const [isQuerying, setIsQuerying] = useState(false);
  const [queryResult, setQueryResult] = useState<Record<string, unknown>[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleTryIt = useCallback(async () => {
    setIsQuerying(true);
    setError(null);
    onVerificationChange(false);
    try {
      const params = new URLSearchParams({
        event_type: "user.signup",
        entity_id: "user-001",
        limit: "10",
      });
      const res = await fetch(`/api/v1/events/query?${params.toString()}`, {
        credentials: "include",
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = await res.json();
      const events: Record<string, unknown>[] = Array.isArray(data.events) ? data.events : [];
      setQueryResult(events);
      if (events.length === 0) {
        setError("No matching event returned yet. Retry the query after the event is available.");
        return;
      }
      onVerificationChange(true);
      trackProductEvent("onboarding_query_completed", { sdk, result_count: events.length });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to query events");
    } finally {
      setIsQuerying(false);
    }
  }, [onVerificationChange, sdk]);

  return (
    <div className="space-y-6">
      <div className="text-center">
        <h2 className="text-2xl font-bold">Query It Back</h2>
        <p className="mt-2 text-muted-foreground">
          Query this tenant for the event you just stored.
        </p>
      </div>
      <div data-testid="query-snippet">
        <CodeBlock code={QUERY_SNIPPETS[sdk]} language={getLanguageLabel(sdk)} />
      </div>
      <div className="flex flex-col items-center gap-3">
        <Button
          onClick={handleTryIt}
          disabled={isQuerying}
          data-testid="try-it-button"
          variant="outline"
          className="gap-2"
        >
          <Search className="h-4 w-4" />
          {isQuerying ? "Querying..." : "Try It"}
        </Button>
        {queryResult && (
          <div
            className="w-full rounded-lg border border-border bg-muted/30 p-4"
            data-testid="query-result"
          >
            <p className="mb-2 text-sm font-medium text-muted-foreground">
              Found {queryResult.length} event{queryResult.length !== 1 ? "s" : ""}:
            </p>
            <pre className="max-h-48 overflow-auto text-xs">
              <code>{JSON.stringify(queryResult, null, 2)}</code>
            </pre>
          </div>
        )}
        {error && (
          <p className="text-sm text-red-500" data-testid="query-error">
            {error}
          </p>
        )}
      </div>
      {queryResult && queryResult.length > 0 && (
        <Card>
          <CardContent className="py-6 text-center">
            <p className="text-lg font-semibold">Ingest and query verified</p>
            <p className="mt-1 text-sm text-muted-foreground">
              Open Event Explorer to inspect payloads, streams, and event history.
            </p>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
