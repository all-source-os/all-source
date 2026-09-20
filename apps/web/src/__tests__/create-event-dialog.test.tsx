import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CreateEventDialog, parseEventPayload } from "@/components/events/create-event-dialog";

const { trackProductEvent } = vi.hoisted(() => ({ trackProductEvent: vi.fn() }));

vi.mock("@/lib/product-analytics", () => ({ trackProductEvent }));

describe("parseEventPayload", () => {
  it("accepts JSON objects", () => {
    expect(parseEventPayload('{"plan":"studio"}')).toEqual({ plan: "studio" });
  });

  it.each(["[]", '"text"', "null"])("rejects non-object payload %s", (payload) => {
    expect(() => parseEventPayload(payload)).toThrow("Payload must be a JSON object.");
  });
});

describe("CreateEventDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("creates a real event from validated form values", async () => {
    const onCreate = vi.fn().mockResolvedValue({ id: "evt-123" });
    const onOpenChange = vi.fn();

    render(<CreateEventDialog open onOpenChange={onOpenChange} onCreate={onCreate} />);

    fireEvent.change(screen.getByLabelText("Entity ID"), {
      target: { value: "customer-123" },
    });
    fireEvent.change(screen.getByLabelText("Event type"), {
      target: { value: "customer.created" },
    });
    fireEvent.change(screen.getByLabelText("Payload"), {
      target: { value: '{"plan":"studio"}' },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create event" }));

    await waitFor(() => {
      expect(onCreate).toHaveBeenCalledWith({
        entity_id: "customer-123",
        event_type: "customer.created",
        payload: { plan: "studio" },
      });
    });
    expect(screen.getByText("Event stored")).toBeInTheDocument();
    expect(trackProductEvent).toHaveBeenCalledWith("dashboard_event_created", {
      source: "event_dialog",
    });
    expect(onOpenChange).not.toHaveBeenCalled();
  });

  it("keeps form data and reports invalid JSON", async () => {
    const onCreate = vi.fn();

    render(<CreateEventDialog open onOpenChange={vi.fn()} onCreate={onCreate} />);

    fireEvent.change(screen.getByLabelText("Entity ID"), {
      target: { value: "customer-123" },
    });
    fireEvent.change(screen.getByLabelText("Event type"), {
      target: { value: "customer.created" },
    });
    fireEvent.change(screen.getByLabelText("Payload"), {
      target: { value: "{" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create event" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(/JSON/i);
    expect(screen.getByLabelText("Payload")).toHaveValue("{");
    expect(onCreate).not.toHaveBeenCalled();
  });
});
