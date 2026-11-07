"use client";

import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
  Input,
  Label,
  Skeleton,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  Textarea,
} from "@allsource/ui";
import { CheckCircle2, Fingerprint, KeySquare, Plus, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import {
  CLIENT_KEY_SCOPES,
  type ClientKey,
  type ClientKeyState,
  clientKeyState,
  fetchClientKeys,
  parsePublicJwk,
  type RegisterClientKeyResult,
  registerClientKey,
  revokeClientKey,
} from "@/lib/tenants-api";

/**
 * Registered ES256 client keys for one tenant. A tenant client signs a
 * short-lived JWT with the private half; the gateway verifies it against the
 * public JWK registered here. Role and scopes come from this record, not the
 * token. Procedure: docs/runbooks/CLIENT_KEY_ROTATION.md.
 */

function formatDateTime(iso?: string): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "—";
  return d.toLocaleString("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function stateVariant(state: ClientKeyState): "default" | "secondary" | "destructive" | "outline" {
  switch (state) {
    case "active":
      return "default";
    case "revoked":
      return "secondary";
    case "invalid":
      return "destructive";
    default:
      return "outline";
  }
}

const DEFAULT_SCOPES = ["events:read"];

interface ClientKeysPanelProps {
  tenantId: string;
  tenantName: string;
}

export function ClientKeysPanel({ tenantId, tenantName }: ClientKeysPanelProps) {
  const [keys, setKeys] = useState<ClientKey[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);

  const loadKeys = useCallback(async () => {
    setIsLoading(true);
    try {
      setKeys(await fetchClientKeys(tenantId));
      setLoadError(null);
    } catch (err) {
      setKeys([]);
      setLoadError(err instanceof Error ? err.message : "Failed to load client keys");
    } finally {
      setIsLoading(false);
    }
  }, [tenantId]);

  useEffect(() => {
    loadKeys();
  }, [loadKeys]);

  // ── Register ────────────────────────────────────────────────────────
  const [clientId, setClientId] = useState("");
  const [jwkText, setJwkText] = useState("");
  const [scopes, setScopes] = useState<string[]>(DEFAULT_SCOPES);
  const [registerBusy, setRegisterBusy] = useState(false);
  const [registerError, setRegisterError] = useState<string | null>(null);
  const [registerResult, setRegisterResult] = useState<RegisterClientKeyResult | null>(null);

  const toggleScope = (scope: string) => {
    setScopes((current) =>
      current.includes(scope) ? current.filter((s) => s !== scope) : [...current, scope]
    );
  };

  const submitRegister = useCallback(async () => {
    setRegisterError(null);
    setRegisterResult(null);
    let jwk: ReturnType<typeof parsePublicJwk>;
    try {
      jwk = parsePublicJwk(jwkText.trim());
    } catch (err) {
      setRegisterError(err instanceof Error ? err.message : "Invalid JWK");
      return;
    }
    setRegisterBusy(true);
    try {
      const result = await registerClientKey(tenantId, {
        client_id: clientId.trim(),
        jwk,
        scopes: CLIENT_KEY_SCOPES.filter((s) => scopes.includes(s)),
      });
      setRegisterResult(result);
      setClientId("");
      setJwkText("");
      setScopes(DEFAULT_SCOPES);
      await loadKeys();
    } catch (err) {
      setRegisterError(err instanceof Error ? err.message : "Failed to register client key");
    } finally {
      setRegisterBusy(false);
    }
  }, [tenantId, clientId, jwkText, scopes, loadKeys]);

  const registerReady =
    !registerBusy && clientId.trim() !== "" && jwkText.trim() !== "" && scopes.length > 0;

  // ── Revoke ──────────────────────────────────────────────────────────
  const [revokeTarget, setRevokeTarget] = useState<ClientKey | null>(null);
  const [revokeBusy, setRevokeBusy] = useState(false);
  const [revokeError, setRevokeError] = useState<string | null>(null);

  const confirmRevoke = useCallback(async () => {
    if (!revokeTarget) return;
    setRevokeBusy(true);
    setRevokeError(null);
    try {
      await revokeClientKey(tenantId, revokeTarget.kid);
      setRevokeTarget(null);
      await loadKeys();
    } catch (err) {
      setRevokeError(err instanceof Error ? err.message : "Failed to revoke client key");
    } finally {
      setRevokeBusy(false);
    }
  }, [tenantId, revokeTarget, loadKeys]);

  const closeRevoke = () => {
    if (revokeBusy) return;
    setRevokeTarget(null);
    setRevokeError(null);
  };

  return (
    <Card data-testid="tenant-client-keys-card">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <KeySquare className="h-5 w-5" />
          Client keys
        </CardTitle>
        <CardDescription>
          Registered ES256 public keys. A client signs a 60-second JWT with the private key; the
          gateway checks it against the key registered here. Role and scopes come from this record.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-8">
        <section className="space-y-3" data-testid="client-keys-list">
          {isLoading ? (
            <div className="space-y-2" data-testid="client-keys-loading">
              <Skeleton className="h-8 w-full" />
              <Skeleton className="h-8 w-full" />
            </div>
          ) : loadError ? (
            <div className="space-y-2" data-testid="client-keys-error">
              <p className="text-sm text-red-500">{loadError}</p>
              <Button
                variant="outline"
                size="sm"
                onClick={loadKeys}
                data-testid="client-keys-retry-btn"
              >
                Retry
              </Button>
            </div>
          ) : keys.length === 0 ? (
            <p
              className="text-sm text-muted-foreground py-4 text-center"
              data-testid="client-keys-empty"
            >
              No client keys registered for this tenant.
            </p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Client</TableHead>
                  <TableHead>Key id (kid)</TableHead>
                  <TableHead>Role / scopes</TableHead>
                  <TableHead>State</TableHead>
                  <TableHead>Registered</TableHead>
                  <TableHead className="text-right">Action</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {keys.map((key) => {
                  const state = clientKeyState(key);
                  const keyScopes = Array.isArray(key.scopes) ? key.scopes : [];
                  return (
                    <TableRow key={key.kid} data-testid={`client-key-row-${key.kid}`}>
                      <TableCell className="font-medium">{key.client_id || "—"}</TableCell>
                      <TableCell>
                        <span className="font-mono text-xs break-all" title={key.kid}>
                          {key.kid}
                        </span>
                      </TableCell>
                      <TableCell>
                        <div className="flex flex-wrap gap-1">
                          <Badge variant="outline">{key.role || "unknown"}</Badge>
                          {keyScopes.map((s) => (
                            <Badge key={s} variant="secondary" className="font-mono text-xs">
                              {s}
                            </Badge>
                          ))}
                        </div>
                      </TableCell>
                      <TableCell>
                        <Badge
                          variant={stateVariant(state)}
                          data-testid={`client-key-state-${key.kid}`}
                        >
                          {state}
                        </Badge>
                        {key.revoked && (
                          <p className="mt-1 text-xs text-muted-foreground">
                            {formatDateTime(key.revoked_at)}
                            {key.revoked_by ? ` by ${key.revoked_by}` : ""}
                          </p>
                        )}
                      </TableCell>
                      <TableCell className="text-sm">
                        {formatDateTime(key.created_at)}
                        {key.created_by && (
                          <p className="text-xs text-muted-foreground">by {key.created_by}</p>
                        )}
                      </TableCell>
                      <TableCell className="text-right">
                        {!key.revoked && (
                          <Button
                            variant="outline"
                            size="sm"
                            onClick={() => {
                              setRevokeError(null);
                              setRevokeTarget(key);
                            }}
                            data-testid={`client-key-revoke-btn-${key.kid}`}
                          >
                            <Trash2 className="mr-2 h-4 w-4" />
                            Revoke
                          </Button>
                        )}
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          )}
        </section>

        <section className="space-y-3" data-testid="client-key-register-section">
          <div className="flex items-center gap-2">
            <Plus className="h-4 w-4 text-muted-foreground" />
            <h3 className="text-sm font-medium">Register a client key</h3>
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="client-key-client-id" className="text-xs text-muted-foreground">
              Client id (the token&apos;s iss)
            </Label>
            <Input
              id="client-key-client-id"
              value={clientId}
              onChange={(e) => setClientId(e.target.value)}
              placeholder="billing-worker"
              autoComplete="off"
              data-testid="client-key-client-id"
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="client-key-jwk" className="text-xs text-muted-foreground">
              Public JWK (P-256)
            </Label>
            <Textarea
              id="client-key-jwk"
              value={jwkText}
              onChange={(e) => setJwkText(e.target.value)}
              placeholder='{"kty":"EC","crv":"P-256","x":"…","y":"…"}'
              className="font-mono text-xs"
              spellCheck={false}
              data-testid="client-key-jwk"
            />
            <p className="text-xs text-muted-foreground">
              Paste the public key only. A JWK carrying the private member{" "}
              <span className="font-mono">d</span> is refused here and by the Control Plane.
            </p>
          </div>
          <div className="space-y-1.5">
            <p className="text-xs text-muted-foreground">
              Scopes (role is always <span className="font-mono">serviceaccount</span>)
            </p>
            <div className="flex flex-wrap gap-2" data-testid="client-key-scopes">
              {CLIENT_KEY_SCOPES.map((scope) => {
                const selected = scopes.includes(scope);
                return (
                  <Button
                    key={scope}
                    type="button"
                    size="sm"
                    variant={selected ? "default" : "outline"}
                    aria-pressed={selected}
                    onClick={() => toggleScope(scope)}
                    className="font-mono text-xs"
                    data-testid={`client-key-scope-${scope}`}
                  >
                    {scope}
                  </Button>
                );
              })}
            </div>
          </div>
          <div className="flex justify-end">
            <Button
              onClick={submitRegister}
              disabled={!registerReady}
              data-testid="client-key-register-btn"
            >
              <Fingerprint className="mr-2 h-4 w-4" />
              {registerBusy ? "Registering…" : "Register key"}
            </Button>
          </div>

          {registerError && (
            <p className="text-sm text-red-500" data-testid="client-key-register-error">
              {registerError}
            </p>
          )}
          {registerResult && (
            <div
              className="flex items-start gap-2 rounded-lg border border-green-500/30 bg-green-500/10 p-3 text-sm text-green-600"
              data-testid="client-key-register-success"
            >
              <CheckCircle2 className="h-4 w-4 shrink-0 mt-0.5" />
              <span>
                {registerResult.already_registered ? "Already registered" : "Registered"}{" "}
                <strong>{registerResult.client_id}</strong> on {tenantName}. Give the client this
                kid: <span className="font-mono break-all">{registerResult.kid}</span>
              </span>
            </div>
          )}
        </section>
      </CardContent>

      {revokeTarget && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center"
          data-testid="client-key-revoke-dialog"
        >
          <button
            type="button"
            aria-label="Close"
            className="fixed inset-0 cursor-default bg-background/80 backdrop-blur-sm"
            onClick={closeRevoke}
          />
          <Card className="relative z-50 w-full max-w-md mx-4">
            <CardHeader>
              <CardTitle>Revoke client key</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
              <p className="text-sm text-muted-foreground">
                Revoke <strong>{revokeTarget.client_id || "this key"}</strong> on {tenantName}? Its
                tokens are refused from then on. A revoked key cannot be registered again; the
                client needs a new keypair.
              </p>
              <p className="font-mono text-xs break-all">{revokeTarget.kid}</p>
              {revokeError && (
                <p className="text-sm text-red-500" data-testid="client-key-revoke-error">
                  {revokeError}
                </p>
              )}
            </CardContent>
            <CardFooter className="flex justify-end gap-2">
              <Button
                variant="outline"
                onClick={closeRevoke}
                disabled={revokeBusy}
                data-testid="client-key-revoke-cancel-btn"
              >
                Cancel
              </Button>
              <Button
                variant="destructive"
                onClick={confirmRevoke}
                disabled={revokeBusy}
                data-testid="client-key-revoke-confirm-btn"
              >
                {revokeBusy ? "Revoking…" : "Revoke"}
              </Button>
            </CardFooter>
          </Card>
        </div>
      )}
    </Card>
  );
}
