"use client";

import { Button, Input, Label } from "@allsource/ui";
import * as Dialog from "@radix-ui/react-dialog";
import { useState } from "react";
import type { Invitation } from "@/lib/api/client";

export function InviteMemberDialog({
  open,
  onClose,
  onInvite,
}: {
  open: boolean;
  onClose: () => void;
  onInvite: (email: string, role: "admin" | "member") => Promise<Invitation>;
}) {
  const [email, setEmail] = useState("");
  const [role, setRole] = useState<"admin" | "member">("member");
  const [invitation, setInvitation] = useState<Invitation | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);
  const close = () => {
    if (!pending) {
      setInvitation(null);
      setEmail("");
      setRole("member");
      setError("");
      setCopied(false);
      onClose();
    }
  };

  return (
    <Dialog.Root
      open={open}
      onOpenChange={(value) => {
        if (!value) close();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-background/80 backdrop-blur-sm" />
        <Dialog.Content className="ph-no-capture ph-no-autocapture fixed left-1/2 top-1/2 z-50 w-[calc(100%-2rem)] max-w-md -translate-x-1/2 -translate-y-1/2 space-y-4 rounded-xl border bg-background p-6 shadow-xl">
          <Dialog.Title className="text-xl font-semibold">
            {invitation ? "Invitation ready" : "Invite a team member"}
          </Dialog.Title>
          <Dialog.Description className="text-base text-muted-foreground">
            {invitation
              ? "Share this code privately with the invited person. They can enter it under Team → Join workspace after signing in with the matching verified email."
              : "Create a code for a specific email. Members share this workspace’s event and query quotas."}
          </Dialog.Description>
          {invitation ? (
            <div className="space-y-4">
              <p className="text-base">
                For {invitation.email} · {invitation.role}
              </p>
              <Label htmlFor="invitation-code">Invitation code</Label>
              <Input
                id="invitation-code"
                readOnly
                value={invitation.token}
                autoComplete="off"
                className="font-mono text-base md:text-base"
              />
              <p className="text-base text-muted-foreground">
                Expires {new Date(invitation.expires_at).toLocaleString()}. No email has been sent.
              </p>
              <Button
                onClick={async () => {
                  try {
                    await navigator.clipboard.writeText(invitation.token);
                    setCopied(true);
                  } catch {
                    setError("Copy the code manually from the field above.");
                  }
                }}
              >
                {copied ? "Copied" : "Copy code"}
              </Button>
            </div>
          ) : (
            <form
              className="space-y-4"
              onSubmit={async (event) => {
                event.preventDefault();
                setPending(true);
                setError("");
                try {
                  setInvitation(await onInvite(email.trim().toLowerCase(), role));
                } catch (cause) {
                  setError(cause instanceof Error ? cause.message : "Could not create invitation.");
                } finally {
                  setPending(false);
                }
              }}
            >
              <div className="space-y-2">
                <Label htmlFor="invite-email">Email address</Label>
                <Input
                  id="invite-email"
                  className="text-base md:text-base"
                  type="email"
                  required
                  maxLength={320}
                  value={email}
                  onChange={(event) => setEmail(event.target.value)}
                  disabled={pending}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="invite-role">Team role</Label>
                <select
                  id="invite-role"
                  className="w-full rounded-md border bg-background p-2 text-base"
                  value={role}
                  disabled={pending}
                  onChange={(event) => setRole(event.target.value as "admin" | "member")}
                >
                  <option value="member">Member</option>
                  <option value="admin">Admin — can manage membership</option>
                </select>
              </div>
              <Button type="submit" disabled={pending}>
                {pending ? "Creating…" : "Create invitation"}
              </Button>
            </form>
          )}
          {error && (
            <p role="alert" className="text-base text-destructive">
              {error}
            </p>
          )}
          <Button variant="outline" onClick={close} disabled={pending}>
            Close
          </Button>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
