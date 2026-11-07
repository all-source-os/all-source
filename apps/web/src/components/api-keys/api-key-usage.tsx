"use client";

import { Badge, Button, Input, Label, Tabs, TabsContent, TabsList, TabsTrigger } from "@allsource/ui";
import { Check, Copy } from "lucide-react";
import { useState } from "react";

interface ApiKeyUsageProps {
  /** The raw API key to embed in examples. */
  apiKey: string;
}

/**
 * "Using your key" block shown after an API key is generated. Surfaces the
 * API URL for non-SDK users and an SDK support matrix with install commands
 * for each language. Rust (crates.io) and TypeScript (npm `@allsourcedev/client`)
 * are published to public registries; Go and Python remain experimental and are
 * installed from GitHub.
 */
export function ApiKeyUsage({ apiKey }: ApiKeyUsageProps) {
  // The branded public front door for external callers. Intentionally NOT
  // NEXT_PUBLIC_API_URL — that may point at a raw/internal host (e.g. the
  // allsource-query.fly.dev origin) for the dashboard's own fetches, which is
  // not what users should copy into their SDKs/HTTP clients.
  const apiUrl = "https://api.all-source.xyz";
  const keyPreview = apiKey.length > 28 ? `${apiKey.slice(0, 24)}...` : apiKey;
  const [copiedField, setCopiedField] = useState<string | null>(null);

  const copy = async (value: string, field: string) => {
    await navigator.clipboard.writeText(value);
    setCopiedField(field);
    setTimeout(() => setCopiedField(null), 1500);
  };

  const curlSnippet = `curl -X POST ${apiUrl}/api/v1/events \\
  -H "Authorization: Bearer ${keyPreview}" \\
  -H "Content-Type: application/json" \\
  -d '{"event_type": "user.signup", "entity_id": "user-1", "payload": {}}'`;

  const rustSnippet = `# Cargo.toml
[dependencies]
allsource = "0.29"`;

  const goSnippet = `# Go SDK is experimental — install from the GitHub registry:
go get github.com/all-source-os/all-source/sdks/go`;

  const pythonSnippet = `# Python SDK is experimental — install from GitHub:
pip install git+https://github.com/all-source-os/all-source.git#subdirectory=sdks/python-client`;

  const clientKeySnippet = `# Long-running service? Sign short-lived assertions instead of storing this key.
# 1. Generate a P-256 key; send only the public JWK to a platform operator,
#    who registers it on your tenant and returns its kid.
openssl ecparam -name prime256v1 -genkey -noout \\
  | openssl pkcs8 -topk8 -nocrypt -out client-key.pem

# 2. Rust SDK: allsource = { version = "0.29", features = ["es256-assertion"] }
let provider = allsource::Es256AssertionProvider::from_pkcs8_pem(
    &std::fs::read("client-key.pem")?,
    "my-service", "my-tenant", "<kid>", "${apiUrl}",
)?;
let client = allsource::QueryClient::with_config_and_credential_provider(
    allsource::ClientConfig::new("${apiUrl}", ""),
    provider,
)?;

# Or allsource-mcp --remote ${apiUrl} with:
ALLSOURCE_CLIENT_SIGNING_KEY=/path/to/client-key.pem
ALLSOURCE_CLIENT_ID=my-service
ALLSOURCE_TENANT_ID=my-tenant
ALLSOURCE_CLIENT_KID=<kid>`;

  const tsSnippet = `# TypeScript SDK — install from npm:
bun add @allsourcedev/client
# or: npm install @allsourcedev/client`;

  return (
    <div className="space-y-4">
      <div>
        <Label>API URL</Label>
        <div className="mt-1.5 flex gap-2">
          <Input value={apiUrl} readOnly className="font-mono" />
          <Button variant="outline" onClick={() => copy(apiUrl, "url")}>
            {copiedField === "url" ? (
              <Check className="h-4 w-4 text-green-500" />
            ) : (
              <Copy className="h-4 w-4" />
            )}
          </Button>
        </div>
        <p className="mt-1 text-xs text-muted-foreground">
          The only public front door. Point SDKs or raw HTTP clients here; Core is internal-only.
        </p>
      </div>

      <div>
        <Label>Use your key</Label>
        <Tabs defaultValue="rust" className="mt-1.5">
          <TabsList>
            <TabsTrigger value="rust">
              Rust <Badge className="ml-1.5" variant="default">stable</Badge>
            </TabsTrigger>
            <TabsTrigger value="go">
              Go <Badge className="ml-1.5" variant="secondary">experimental</Badge>
            </TabsTrigger>
            <TabsTrigger value="python">
              Python <Badge className="ml-1.5" variant="secondary">experimental</Badge>
            </TabsTrigger>
            <TabsTrigger value="typescript">
              TypeScript <Badge className="ml-1.5" variant="secondary">experimental</Badge>
            </TabsTrigger>
            <TabsTrigger value="curl">No SDK (curl)</TabsTrigger>
            <TabsTrigger value="client-key">Registered client key</TabsTrigger>
          </TabsList>

          <TabsContent value="rust">
            <SnippetBlock
              code={rustSnippet}
              onCopy={() => copy(rustSnippet, "rust")}
              copied={copiedField === "rust"}
              footer={
                <>
                  Typed client, WebSocket streaming, <code>ProjectionWorker</code>. See{" "}
                  <a
                    className="underline hover:text-foreground"
                    href="https://crates.io/crates/allsource"
                    target="_blank"
                    rel="noreferrer"
                  >
                    crates.io
                  </a>{" "}
                  and the{" "}
                  <a
                    className="underline hover:text-foreground"
                    href="https://github.com/all-source-os/all-source/blob/main/sdks/rust/README.md"
                    target="_blank"
                    rel="noreferrer"
                  >
                    SDK README
                  </a>
                  .
                </>
              }
            />
          </TabsContent>

          <TabsContent value="go">
            <SnippetBlock
              code={goSnippet}
              onCopy={() => copy(goSnippet, "go")}
              copied={copiedField === "go"}
              footer={<>API surface may change before 1.0. Pin to a specific commit for stability.</>}
            />
          </TabsContent>

          <TabsContent value="python">
            <SnippetBlock
              code={pythonSnippet}
              onCopy={() => copy(pythonSnippet, "python")}
              copied={copiedField === "python"}
              footer={<>API surface may change before 1.0. Pin to a specific commit or tag.</>}
            />
          </TabsContent>

          <TabsContent value="typescript">
            <SnippetBlock
              code={tsSnippet}
              onCopy={() => copy(tsSnippet, "typescript")}
              copied={copiedField === "typescript"}
              footer={
                <>
                  Published on{" "}
                  <a
                    className="underline hover:text-foreground"
                    href="https://www.npmjs.com/package/@allsourcedev/client"
                    target="_blank"
                    rel="noreferrer"
                  >
                    npm
                  </a>
                  . API surface may change before 1.0 — pin a version.
                </>
              }
            />
          </TabsContent>

          <TabsContent value="curl">
            <SnippetBlock
              code={curlSnippet}
              onCopy={() => copy(curlSnippet, "curl")}
              copied={copiedField === "curl"}
              footer={
                <>
                  Full wire protocol covered in the{" "}
                  <a
                    className="underline hover:text-foreground"
                    href="https://all-source.xyz/blog/connecting-without-an-sdk"
                    target="_blank"
                    rel="noreferrer"
                  >
                    no-SDK connection guide
                  </a>
                  .
                </>
              }
            />
          </TabsContent>

          <TabsContent value="client-key">
            <SnippetBlock
              code={clientKeySnippet}
              onCopy={() => copy(clientKeySnippet, "client-key")}
              copied={copiedField === "client-key"}
              footer={
                <>
                  Each request carries a 60-second ES256 assertion the service signs itself, so no
                  long-lived bearer is stored. The TypeScript, Python and Go SDKs have the same
                  provider. Registration is done by a platform operator. See the{" "}
                  <a
                    className="underline hover:text-foreground"
                    href="https://github.com/all-source-os/all-source/blob/main/docs/guides/REGISTERED_CLIENT_KEYS.md"
                    target="_blank"
                    rel="noreferrer"
                  >
                    registered client keys guide
                  </a>
                  .
                </>
              }
            />
          </TabsContent>
        </Tabs>
      </div>
    </div>
  );
}

interface SnippetBlockProps {
  code: string;
  onCopy: () => void;
  copied: boolean;
  footer?: React.ReactNode;
}

function SnippetBlock({ code, onCopy, copied, footer }: SnippetBlockProps) {
  return (
    <div className="mt-2 space-y-2">
      <div className="relative">
        <pre className="overflow-x-auto rounded-lg bg-muted p-4 pr-12 text-xs">
          <code>{code}</code>
        </pre>
        <Button
          variant="ghost"
          size="icon"
          className="absolute right-2 top-2 h-7 w-7"
          onClick={onCopy}
          aria-label="Copy snippet"
        >
          {copied ? <Check className="h-3.5 w-3.5 text-green-500" /> : <Copy className="h-3.5 w-3.5" />}
        </Button>
      </div>
      {footer ? <p className="text-xs text-muted-foreground">{footer}</p> : null}
    </div>
  );
}
