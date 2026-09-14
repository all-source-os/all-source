import { describe, expect, it } from "vitest";
import { acquisitionAttributionForUrl } from "@/lib/acquisition-attribution";

describe("portfolio acquisition attribution", () => {
  it("keeps only approved campaign fields", () => {
    expect(
      acquisitionAttributionForUrl(
        "https://www.all-source.xyz/event-replay-debugging?utm_source=wolventech.com&utm_medium=portfolio&utm_campaign=product_directory&utm_content=allsource_proof&email=private"
      )
    ).toEqual({
      campaign_content: "allsource_proof",
      campaign_medium: "portfolio",
      campaign_name: "product_directory",
      campaign_source: "wolventech.com",
    });
  });

  it("drops arbitrary source and campaign values", () => {
    expect(
      acquisitionAttributionForUrl(
        "https://www.all-source.xyz/?utm_source=person@example.com&utm_medium=portfolio&utm_campaign=product_directory"
      )
    ).toEqual({});
    expect(
      acquisitionAttributionForUrl(
        "https://www.all-source.xyz/?utm_source=reddit.com&utm_medium=community&utm_campaign=private_name"
      )
    ).toEqual({});
  });
});
