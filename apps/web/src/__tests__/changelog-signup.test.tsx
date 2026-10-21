import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ChangelogSignup } from "@/components/changelog-signup";

const trackProductEvent = vi.fn();
vi.mock("@/lib/product-analytics", () => ({
  trackProductEvent: (...args: unknown[]) => trackProductEvent(...args),
}));

function submit(email = "ada@example.com") {
  fireEvent.change(screen.getByLabelText("Email"), { target: { value: email } });
  fireEvent.click(screen.getByRole("button", { name: "Get the changelog + benchmark notes" }));
}

afterEach(() => {
  vi.unstubAllGlobals();
  trackProductEvent.mockReset();
});

describe("changelog signup", () => {
  it("labels the field and links consent text to the privacy section", () => {
    render(<ChangelogSignup placement="home" />);

    const input = screen.getByLabelText("Email");
    expect(input).toHaveAttribute("type", "email");
    expect(input).toHaveAccessibleDescription(/may email you release notes/);
    expect(screen.getByRole("link", { name: "how we handle your email" })).toHaveAttribute(
      "href",
      "/privacy#changelog-emails"
    );
  });

  it("shows a pending state, then the confirmation", async () => {
    let resolve: (value: Response) => void = () => {};
    const fetchMock = vi.fn(
      (_url: string, _init: RequestInit) =>
        new Promise<Response>((done) => {
          resolve = done;
        })
    );
    vi.stubGlobal("fetch", fetchMock);
    render(<ChangelogSignup placement="prime" />);

    submit();

    expect(await screen.findByRole("button", { name: /Subscribing/ })).toBeDisabled();
    expect(JSON.parse(fetchMock.mock.calls[0]?.[1].body as string)).toMatchObject({
      email: "ada@example.com",
      consent: true,
      placement: "prime",
    });

    resolve(Response.json({ status: "subscribed", message: "You're on the list." }));

    expect(await screen.findByRole("status")).toHaveTextContent("You're on the list.");
    expect(screen.queryByLabelText("Email")).not.toBeInTheDocument();
    expect(trackProductEvent).toHaveBeenCalledWith("changelog_signup_succeeded", {
      placement: "prime",
    });
    for (const [, properties] of trackProductEvent.mock.calls) {
      expect(JSON.stringify(properties)).not.toContain("ada@example.com");
    }
  });

  it("shows the server error and keeps the form for a retry", async () => {
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValue(
          Response.json(
            { message: "Signups are temporarily unavailable. Please try again later." },
            { status: 503 }
          )
        )
    );
    render(<ChangelogSignup placement="home" />);

    submit();

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Signups are temporarily unavailable.");
    await waitFor(() => expect(alert).toHaveFocus());
    expect(
      screen.getByRole("button", { name: "Get the changelog + benchmark notes" })
    ).toBeEnabled();
  });

  it("explains a rate limit in minutes", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(Response.json({ retry_after: 120 }, { status: 429 }))
    );
    render(<ChangelogSignup placement="home" />);

    submit();

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Too many attempts. Try again in about 2 minutes."
    );
  });
});
