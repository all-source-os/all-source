"use client";

import { Button, Card, CardContent, Input, Label } from "@allsource/ui";
import { useState } from "react";

export function JoinWorkspace() {
  const [code, setCode] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  return (
    <Card className="ph-no-capture ph-no-autocapture">
      <CardContent className="space-y-4 p-6">
        <h2 className="text-xl font-semibold">Join workspace</h2>
        <p className="text-base text-muted-foreground">
          Enter a code shared by a team admin. Use the verified email they invited. Joining switches
          your current workspace.
        </p>
        <form
          className="flex flex-col gap-3 sm:flex-row sm:items-end"
          onSubmit={async (event) => {
            event.preventDefault();
            setPending(true);
            setError("");
            try {
              const response = await fetch("/api/team/join", {
                method: "POST",
                credentials: "same-origin",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ token: code.trim() }),
              });
              const data = await response.json();
              if (!response.ok || data.joined !== true)
                throw new Error(
                  data.error?.message ||
                    "Unable to join workspace. Check the code and your signed-in email."
                );
              // Drop every old-workspace client cache after replacing the HttpOnly session.
              window.location.assign("/dashboard/team");
            } catch (cause) {
              setError(cause instanceof Error ? cause.message : "Unable to join workspace.");
              setPending(false);
            }
          }}
        >
          <div className="flex-1 space-y-2">
            <Label htmlFor="join-code">Invitation code</Label>
            <Input
              id="join-code"
              autoComplete="off"
              spellCheck={false}
              required
              pattern="[A-Za-z0-9_-]{32}"
              maxLength={32}
              value={code}
              onChange={(event) => setCode(event.target.value)}
              disabled={pending}
              className="font-mono text-base md:text-base"
            />
          </div>
          <Button type="submit" disabled={pending}>
            {pending ? "Joining…" : "Join workspace"}
          </Button>
        </form>
        {error && (
          <p role="alert" className="text-base text-destructive">
            {error}
          </p>
        )}
      </CardContent>
    </Card>
  );
}
