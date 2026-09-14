import { describe, expect, it } from "bun:test";
import { render } from "@testing-library/react";
import { Icons } from "./icons";

describe("Icons.logo", () => {
  it("renders the full supplied navbar mark", () => {
    const { container } = render(<Icons.logo className="h-9 w-9" />);
    const logo = container.querySelector("img");

    expect(logo).toBeTruthy();
    expect(logo).toHaveAttribute("src", "/logo.png");
    expect(logo).toHaveAttribute("alt", "");
    expect(logo).toHaveAttribute("aria-hidden", "true");
    expect(logo).toHaveClass("h-9", "w-9", "object-contain");
  });
});
