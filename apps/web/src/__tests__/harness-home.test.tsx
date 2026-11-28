import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import Home, { metadata } from "@/app/page";
import graph from "../../../../docs/evidence/2026-11-26-cto-landing/engineering-graph.json";
import approval from "../../../../docs/evidence/2026-11-26-cto-landing/restart.json";
import publicGraph from "../../public/examples/checkout-graph.json";
import publicApproval from "../../public/examples/support-approval.json";

describe("self-service harness homepage", () => {
  it("describes the agent-memory page in search results without dropping the event store", () => {
    expect(metadata.title).toBe("Agent Memory on an Event Store | AllSource Prime");
    expect(metadata.description).toMatch(/^Give your coding agent a memory/);
    expect(metadata.alternates?.canonical).toBe("/");
  });

  it("takes a referral directly to local setup and then a fresh-session check", () => {
    render(<Home />);
    const start = screen.getByRole("link", { name: "Build your first harness" });
    const setup = document.getElementById(start.getAttribute("href")!.slice(1));
    expect(setup).toHaveAccessibleName("Build your first harness.");
    fireEvent.click(within(setup!).getByText("Remove the example registration"));
    for (const command of [
      "codex mcp remove allsource-example",
      "claude mcp remove --scope local allsource-example",
    ]) {
      expect(within(setup!).getByText(command, { exact: true }).tagName).toBe("CODE");
    }
    expect(within(setup!).getByText(/cargo install allsource-prime/)).toHaveTextContent(
      "codex mcp add"
    );
    expect(within(setup!).getByRole("heading", { name: /fresh session/ })).toBeVisible();
    expect(within(setup!).getAllByText(/^Use allsource-example to /)).toHaveLength(2);
    const help = screen.getByRole("link", { name: "Talk through a custom build" });
    expect(start.compareDocumentPosition(help) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("shows both graph answers and the recorded source in the public example", () => {
    render(<Home />);
    const group = screen.getByRole("group", { name: /inspect a graph record/i });
    const figure = group.closest("figure")!;
    const owner = graph.owned_by.nodes[0]!.properties.name;
    expect(publicGraph).toEqual(graph);
    expect(publicApproval).toEqual(approval);
    expect(screen.getByRole("link", { name: /inspect the saved evidence/i })).toHaveAttribute(
      "href",
      "/examples/checkout-graph.json"
    );
    const decision = graph.history.events[0]!.payload.properties;
    fireEvent.click(within(group).getByRole("button", { name: owner }));
    expect(figure.querySelector("figcaption")).toHaveTextContent(owner);
    fireEvent.click(within(group).getByRole("button", { name: decision.source }));
    expect(figure.querySelector("figcaption")).toHaveTextContent(decision.name);
    expect(within(group).getByRole("button", { name: decision.source })).toHaveAttribute(
      "aria-pressed",
      "true"
    );
  });
});
