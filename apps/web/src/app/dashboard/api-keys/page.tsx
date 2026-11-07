"use client";

import { Button, Card, CardContent, Input, Label } from "@allsource/ui";
import {
  AlertTriangle,
  Check,
  Copy,
  Eye,
  EyeOff,
  Key,
  Plus,
  Shield,
  Terminal,
  X,
} from "lucide-react";
import { useSearchParams } from "next/navigation";
import { useEffect, useState } from "react";
import { CreateKeyDialog } from "@/components/api-keys/create-key-dialog";
import { KeyTable } from "@/components/api-keys/key-table";
import { LoadError } from "@/components/dashboard/load-error";
import { FadeIn } from "@/components/ui/fade-in";
import { useApiKeys } from "@/hooks/use-api-keys";
import type { ApiKeyWithSecret } from "@/lib/api/client";

const CHRONIS_SCOPES = ["events:read", "events:write"];

export default function ApiKeysPage() {
  const searchParams = useSearchParams();
  const { keys, isLoading, error, createKey, rotateKey, revokeKey, refresh } = useApiKeys();
  const [createIntent, setCreateIntent] = useState<"generic" | "chronis" | null>(null);
  const [confirmRevoke, setConfirmRevoke] = useState<string | null>(null);
  const [rotatedKey, setRotatedKey] = useState<ApiKeyWithSecret | null>(null);

  // Check URL for action param
  useEffect(() => {
    if (searchParams.get("action") === "create") {
      setCreateIntent("generic");
    }
  }, [searchParams]);

  const handleCreateKey = async (data: {
    name: string;
    description?: string;
    scopes: string[];
    expires_at?: string;
  }): Promise<ApiKeyWithSecret | undefined> => {
    try {
      return await createKey(data);
    } catch (error) {
      console.error("Failed to create API key:", error);
      return undefined;
    }
  };

  const handleRotate = async (id: string) => {
    try {
      const result = await rotateKey(id);
      if (result) setRotatedKey(result);
    } catch (error) {
      console.error("Failed to rotate API key:", error);
    }
  };

  const handleRevoke = async (id: string) => {
    if (confirmRevoke !== id) {
      setConfirmRevoke(id);
      return;
    }

    try {
      await revokeKey(id);
    } catch (error) {
      console.error("Failed to revoke API key:", error);
    }
    setConfirmRevoke(null);
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <FadeIn delay={0.1} inView>
        <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h1 className="text-2xl font-bold tracking-tight md:text-3xl">API Keys</h1>
            <p className="mt-1 text-muted-foreground">
              Manage API keys for authenticating your applications
            </p>
          </div>
          <Button onClick={() => setCreateIntent("generic")}>
            <Plus className="mr-1.5 h-4 w-4" />
            Create Key
          </Button>
        </div>
      </FadeIn>

      {/* Security notice */}
      <FadeIn delay={0.2} inView>
        <Card className="border-primary/20 bg-primary/5">
          <CardContent className="flex items-start gap-4 p-4">
            <Shield className="h-5 w-5 shrink-0 text-primary" />
            <div>
              <h3 className="font-medium">Keep your keys secure</h3>
              <p className="text-sm text-muted-foreground">
                API keys grant access to your AllSource account. Never share them publicly or commit
                them to version control. Use environment variables instead.
              </p>
            </div>
          </CardContent>
        </Card>
      </FadeIn>

      {/* Connection setup */}
      <FadeIn delay={0.25} inView>
        <Card>
          <CardContent className="grid gap-5 p-5 lg:grid-cols-[minmax(0,1fr)_minmax(22rem,0.85fr)] lg:items-center">
            <div className="flex items-start gap-3">
              <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-primary/10">
                <Terminal className="h-5 w-5 text-primary" />
              </div>
              <div>
                <h2 className="font-semibold">Connect Chronis</h2>
                <p className="mt-1 text-sm text-muted-foreground">
                  Create a dedicated key with Read Events and Write Events. Its secret appears once;
                  paste it into your local config, then keep it out of source control.
                </p>
                <Button
                  className="mt-4"
                  variant="outline"
                  onClick={() => setCreateIntent("chronis")}
                >
                  <Plus className="mr-1.5 h-4 w-4" />
                  Create sync key
                </Button>
              </div>
            </div>
            <pre className="overflow-x-auto rounded-lg border border-border bg-muted/40 p-4 font-mono text-xs text-muted-foreground">
              {`mode = "remote"\n\n[sync]\nremote_url = "https://api.all-source.xyz"\napi_key = "<YOUR_API_KEY>"`}
            </pre>
          </CardContent>
        </Card>
      </FadeIn>

      {/* Registered client keys */}
      <FadeIn delay={0.28} inView>
        <Card>
          <CardContent className="flex items-start gap-4 p-4">
            <Key className="h-5 w-5 shrink-0 text-muted-foreground" />
            <div>
              <h3 className="font-medium">Running a service? Use a registered client key</h3>
              <p className="text-sm text-muted-foreground">
                A service can hold a P-256 private key instead of a stored API key and sign a
                60-second assertion for each request. A platform operator registers the public key
                on your tenant; it is not created on this page. The Rust, TypeScript, Python and Go
                SDKs sign the assertions for you. Keys from the old onboarding flow that were never
                registered stop working at the legacy-key sunset date; keys created here are not
                affected.{" "}
                <a
                  className="underline hover:text-foreground"
                  href="https://github.com/all-source-os/all-source/blob/main/docs/guides/REGISTERED_CLIENT_KEYS.md"
                  target="_blank"
                  rel="noreferrer"
                >
                  Registered client keys guide
                </a>
              </p>
            </div>
          </CardContent>
        </Card>
      </FadeIn>

      {/* Keys table */}
      <FadeIn delay={0.3} inView>
        {error ? (
          <LoadError title="API keys could not be loaded" message={error} onRetry={refresh} />
        ) : (
          <KeyTable
            keys={keys}
            isLoading={isLoading}
            onRotate={handleRotate}
            onRevoke={handleRevoke}
          />
        )}
      </FadeIn>

      {/* Quick tips */}
      <FadeIn delay={0.4} inView>
        <div className="grid gap-4 sm:grid-cols-3">
          <Card>
            <CardContent className="p-4">
              <Key className="mb-2 h-5 w-5 text-muted-foreground" />
              <h3 className="font-medium">Scope Permissions</h3>
              <p className="text-sm text-muted-foreground">
                Only grant the minimum permissions needed for each key.
              </p>
            </CardContent>
          </Card>
          <Card>
            <CardContent className="p-4">
              <Shield className="mb-2 h-5 w-5 text-muted-foreground" />
              <h3 className="font-medium">Rotate Regularly</h3>
              <p className="text-sm text-muted-foreground">
                Rotate keys periodically to reduce risk of compromised credentials.
              </p>
            </CardContent>
          </Card>
          <Card>
            <CardContent className="p-4">
              <AlertTriangle className="mb-2 h-5 w-5 text-muted-foreground" />
              <h3 className="font-medium">Monitor Usage</h3>
              <p className="text-sm text-muted-foreground">
                Check "Last Used" to identify unused keys that can be revoked.
              </p>
            </CardContent>
          </Card>
        </div>
      </FadeIn>

      {/* Create dialog */}
      <CreateKeyDialog
        key={createIntent ?? "closed"}
        open={createIntent !== null}
        onClose={() => setCreateIntent(null)}
        onCreateKey={handleCreateKey}
        initialName={createIntent === "chronis" ? "Chronis sync" : undefined}
        initialDescription={
          createIntent === "chronis" ? "Dedicated key for cn sync from this workspace." : undefined
        }
        initialScopes={createIntent === "chronis" ? CHRONIS_SCOPES : undefined}
      />

      {rotatedKey ? (
        <RotatedKeyDialog apiKey={rotatedKey} onClose={() => setRotatedKey(null)} />
      ) : null}

      {/* Revoke confirmation */}
      {confirmRevoke && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <button
            type="button"
            className="absolute inset-0 bg-background/80 backdrop-blur-sm"
            onClick={() => setConfirmRevoke(null)}
            aria-label="Cancel API key revocation"
          />
          <Card className="relative z-10 w-full max-w-sm mx-4">
            <CardContent className="p-6">
              <AlertTriangle className="mx-auto mb-4 h-12 w-12 text-destructive" />
              <h3 className="mb-2 text-center text-lg font-semibold">Revoke API Key?</h3>
              <p className="mb-6 text-center text-sm text-muted-foreground">
                This action cannot be undone. Any applications using this key will stop working
                immediately.
              </p>
              <div className="flex gap-2">
                <Button variant="outline" className="flex-1" onClick={() => setConfirmRevoke(null)}>
                  Cancel
                </Button>
                <Button
                  variant="destructive"
                  className="flex-1"
                  onClick={() => handleRevoke(confirmRevoke)}
                >
                  Revoke Key
                </Button>
              </div>
            </CardContent>
          </Card>
        </div>
      )}
    </div>
  );
}

function RotatedKeyDialog({ apiKey, onClose }: { apiKey: ApiKeyWithSecret; onClose: () => void }) {
  const [revealed, setRevealed] = useState(false);
  const [copied, setCopied] = useState(false);

  const copyKey = async () => {
    await navigator.clipboard.writeText(apiKey.key);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-background/80 backdrop-blur-sm" aria-hidden="true" />
      <Card
        role="dialog"
        aria-modal="true"
        aria-labelledby="rotated-key-title"
        className="relative z-10 mx-4 w-full max-w-lg"
      >
        <CardContent className="space-y-5 p-6">
          <Button
            variant="ghost"
            size="icon"
            onClick={onClose}
            aria-label="Close"
            className="absolute right-3 top-3"
          >
            <X className="h-4 w-4" />
          </Button>
          <div className="pr-8">
            <h2 id="rotated-key-title" className="text-lg font-semibold">
              API key rotated
            </h2>
            <p className="mt-1 text-sm text-muted-foreground">
              Replace the old key now. This secret will not be shown again.
            </p>
          </div>
          <div className="rounded-lg border border-yellow-500/40 bg-yellow-500/10 p-3 text-sm text-yellow-700 dark:text-yellow-400">
            Rotation invalidates clients using the old key. Store this replacement in your secret
            manager, not in source control.
          </div>
          <div>
            <Label htmlFor="rotated-api-key">New API key</Label>
            <div className="mt-1.5 flex gap-2">
              <div className="relative min-w-0 flex-1">
                <Input
                  id="rotated-api-key"
                  value={revealed ? apiKey.key : "•".repeat(Math.min(apiKey.key.length, 40))}
                  readOnly
                  className="font-mono pr-10"
                />
                <button
                  type="button"
                  onClick={() => setRevealed((value) => !value)}
                  className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                  aria-label={revealed ? "Hide new API key" : "Reveal new API key"}
                >
                  {revealed ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                </button>
              </div>
              <Button variant="outline" onClick={copyKey} aria-label="Copy new API key">
                {copied ? (
                  <Check className="h-4 w-4 text-green-500" />
                ) : (
                  <Copy className="h-4 w-4" />
                )}
              </Button>
            </div>
          </div>
          <div className="flex justify-end">
            <Button onClick={onClose}>Done</Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
