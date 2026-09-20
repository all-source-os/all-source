"use client";

import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
  Textarea,
} from "@allsource/ui";
import { AlertCircle, Check, Loader2, Plus, X } from "lucide-react";
import Link from "next/link";
import { useState } from "react";
import type { CreateEventRequest, Event } from "@/lib/api/client";
import { trackProductEvent } from "@/lib/product-analytics";

interface CreateEventDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreate: (event: CreateEventRequest) => Promise<Event | undefined>;
}

const INITIAL_PAYLOAD = `{
  "source": "dashboard"
}`;

export function parseEventPayload(value: string): Record<string, unknown> {
  const parsed: unknown = JSON.parse(value);
  if (!parsed || Array.isArray(parsed) || typeof parsed !== "object") {
    throw new Error("Payload must be a JSON object.");
  }
  return parsed as Record<string, unknown>;
}

export function CreateEventDialog({ open, onOpenChange, onCreate }: CreateEventDialogProps) {
  const [entityId, setEntityId] = useState("");
  const [eventType, setEventType] = useState("");
  const [payload, setPayload] = useState(INITIAL_PAYLOAD);
  const [isCreating, setIsCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [createdEvent, setCreatedEvent] = useState<{ id?: string; entityId: string } | null>(null);

  const reset = () => {
    setEntityId("");
    setEventType("");
    setPayload(INITIAL_PAYLOAD);
    setError(null);
    setCreatedEvent(null);
  };

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen && !isCreating) {
      reset();
      onOpenChange(false);
    }
  };

  const handleCreate = async () => {
    const nextEntityId = entityId.trim();
    const nextEventType = eventType.trim();
    if (!nextEntityId || !nextEventType) {
      setError("Entity ID and event type are required.");
      return;
    }

    let nextPayload: Record<string, unknown>;
    try {
      nextPayload = parseEventPayload(payload);
    } catch (payloadError) {
      setError(payloadError instanceof Error ? payloadError.message : "Payload is not valid JSON.");
      return;
    }

    setIsCreating(true);
    setError(null);
    try {
      const created = await onCreate({
        entity_id: nextEntityId,
        event_type: nextEventType,
        payload: nextPayload,
      });
      trackProductEvent("dashboard_event_created", { source: "event_dialog" });
      setCreatedEvent({ id: created?.id, entityId: nextEntityId });
    } catch (createError) {
      setError(createError instanceof Error ? createError.message : "Event could not be created.");
    } finally {
      setIsCreating(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent
        onClose={() => handleOpenChange(false)}
        className="max-h-[90vh] overflow-y-auto"
        aria-labelledby="create-event-title"
        aria-describedby="create-event-description"
      >
        <DialogHeader>
          <div className="flex items-start justify-between gap-4">
            <div>
              <DialogTitle id="create-event-title">
                {createdEvent ? "Event stored" : "Create event"}
              </DialogTitle>
              <DialogDescription id="create-event-description" className="mt-1.5">
                {createdEvent
                  ? "AllSource accepted this event and refreshed your tenant data."
                  : "Write one event to your tenant. Payload must be a JSON object."}
              </DialogDescription>
            </div>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label="Close create event dialog"
              onClick={() => handleOpenChange(false)}
              disabled={isCreating}
            >
              <X className="h-4 w-4" />
            </Button>
          </div>
        </DialogHeader>

        {createdEvent ? (
          <div className="space-y-5">
            <div className="flex items-start gap-3 rounded-lg border border-green-500/30 bg-green-500/10 p-4">
              <Check className="mt-0.5 h-5 w-5 shrink-0 text-green-500" />
              <div className="min-w-0">
                <p className="font-medium">Event is available in Event Explorer.</p>
                <p className="mt-1 truncate font-mono text-xs text-muted-foreground">
                  {createdEvent.id ?? createdEvent.entityId}
                </p>
              </div>
            </div>
            <DialogFooter>
              <Button type="button" variant="outline" onClick={reset}>
                <Plus className="mr-2 h-4 w-4" />
                Create another
              </Button>
              <Button asChild>
                <Link
                  href={`/dashboard/events?entity=${encodeURIComponent(createdEvent.entityId)}`}
                  onClick={() => handleOpenChange(false)}
                >
                  View event stream
                </Link>
              </Button>
            </DialogFooter>
          </div>
        ) : (
          <div className="space-y-5">
            <div className="grid gap-4 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label htmlFor="create-event-entity">Entity ID</Label>
                <Input
                  id="create-event-entity"
                  value={entityId}
                  onChange={(event) => setEntityId(event.target.value)}
                  placeholder="customer-123"
                  autoComplete="off"
                  disabled={isCreating}
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="create-event-type">Event type</Label>
                <Input
                  id="create-event-type"
                  value={eventType}
                  onChange={(event) => setEventType(event.target.value)}
                  placeholder="customer.created"
                  autoComplete="off"
                  disabled={isCreating}
                />
              </div>
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="create-event-payload">Payload</Label>
              <Textarea
                id="create-event-payload"
                value={payload}
                onChange={(event) => setPayload(event.target.value)}
                className="min-h-40 font-mono text-sm"
                spellCheck={false}
                disabled={isCreating}
              />
            </div>

            {error && (
              <div
                role="alert"
                className="flex items-start gap-2 rounded-lg border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive"
              >
                <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
                <span>{error}</span>
              </div>
            )}

            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => handleOpenChange(false)}>
                Cancel
              </Button>
              <Button type="button" onClick={handleCreate} disabled={isCreating}>
                {isCreating ? (
                  <>
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    Storing event…
                  </>
                ) : (
                  "Create event"
                )}
              </Button>
            </DialogFooter>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
